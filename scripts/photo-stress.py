#!/usr/bin/env python3
"""Run bounded synthetic large-photo qualification, one size per process."""

import argparse
import datetime
import hashlib
import json
import os
import pathlib
import resource
import signal
import subprocess
import sys
import time


ROOT = pathlib.Path(__file__).resolve().parent.parent
SIZES = (("12mp", 4000, 3000), ("24mp", 6000, 4000), ("48mp", 8000, 6000))
RSS_LIMIT = 3 * 1024**3
ADDRESS_LIMIT = 4 * 1024**3
MAX_TIMEOUT_SECONDS = 900
MIN_REGION_HISTORY_UNDO = 80


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def artifact_inventory(path: pathlib.Path) -> list[dict]:
    result = []
    for item in sorted(candidate for candidate in path.rglob("*") if candidate.is_file()):
        result.append(
            {
                "path": item.relative_to(path).as_posix(),
                "bytes": item.stat().st_size,
                "sha256": sha256_file(item),
            }
        )
    return result


def current_rss(pid: int) -> int | None:
    try:
        status = pathlib.Path(f"/proc/{pid}/status").read_text()
    except (FileNotFoundError, ProcessLookupError, PermissionError):
        return None
    for line in status.splitlines():
        if line.startswith("VmRSS:"):
            return int(line.split()[1]) * 1024
    return None


def child_limits() -> None:
    resource.setrlimit(resource.RLIMIT_AS, (ADDRESS_LIMIT, ADDRESS_LIMIT))
    # Linux treats RLIMIT_RSS as advisory. The parent also polls VmRSS and
    # terminates the process if it crosses this limit.
    resource.setrlimit(resource.RLIMIT_RSS, (RSS_LIMIT, RSS_LIMIT))


def terminate_group(process: subprocess.Popen, sig: signal.Signals) -> None:
    try:
        os.killpg(process.pid, sig)
    except ProcessLookupError:
        pass


def parse_result(stdout_path: pathlib.Path) -> dict | None:
    result = None
    for line in stdout_path.read_text(errors="replace").splitlines():
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if record.get("event") == "result":
            result = record
    return result


def region_history_acceptance(result: dict | None, name: str) -> dict:
    history = result.get("history", {}) if result is not None else {}
    checks = result.get("checks", {}) if result is not None else {}
    undo_count = checks.get("undoCount")
    patch_entries = history.get("rasterPatchEntries")
    failures = []
    if name == "48mp" and (not isinstance(undo_count, int) or undo_count < MIN_REGION_HISTORY_UNDO):
        failures.append(f"48mp undoCount must be at least {MIN_REGION_HISTORY_UNDO}")
    if not isinstance(patch_entries, int) or patch_entries <= 0:
        failures.append("rasterPatchEntries must be greater than zero")
    return {
        "required": True,
        "minimum48mpUndoCount": MIN_REGION_HISTORY_UNDO,
        "passed": not failures,
        "failures": failures,
    }


def run_case(
    binary: pathlib.Path,
    input_path: pathlib.Path,
    evidence: pathlib.Path,
    name: str,
    width: int,
    height: int,
    edits: int,
    timeout_seconds: int,
) -> dict:
    case = evidence / name
    stdout_path = evidence / f"{name}.stdout.jsonl"
    stderr_path = evidence / f"{name}.stderr.txt"
    command = [
        str(binary),
        "--input",
        str(input_path),
        "--output",
        str(case),
        "--width",
        str(width),
        "--height",
        str(height),
        "--edits",
        str(edits),
    ]
    started = time.monotonic()
    maximum_observed_rss = 0
    violation = None
    process = None
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        try:
            process = subprocess.Popen(
                command,
                cwd=ROOT,
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                start_new_session=True,
                preexec_fn=child_limits,
            )
            while process.poll() is None:
                rss = current_rss(process.pid)
                if rss is not None:
                    maximum_observed_rss = max(maximum_observed_rss, rss)
                    if rss > RSS_LIMIT:
                        violation = f"RSS exceeded {RSS_LIMIT} bytes"
                        terminate_group(process, signal.SIGKILL)
                        break
                if time.monotonic() - started > timeout_seconds:
                    violation = f"case exceeded {timeout_seconds} seconds"
                    terminate_group(process, signal.SIGKILL)
                    break
                time.sleep(0.2)
            return_code = process.wait()
        finally:
            if process is not None and process.poll() is None:
                terminate_group(process, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    terminate_group(process, signal.SIGKILL)
                    process.wait()

    fixture_result = parse_result(stdout_path)
    status = "passed" if return_code == 0 and fixture_result is not None else "failed"
    if violation is not None:
        status = "resource-limit-terminated"
    fixture_peak_rss = (
        fixture_result.get("memory", {}).get("peakRssBytes") if fixture_result is not None else None
    )
    if fixture_peak_rss is not None:
        maximum_observed_rss = max(maximum_observed_rss, fixture_peak_rss)
        if fixture_peak_rss > RSS_LIMIT:
            violation = f"reported peak RSS exceeded {RSS_LIMIT} bytes"
            status = "resource-limit-exceeded"
    return {
        "name": name,
        "width": width,
        "height": height,
        "megapixels": width * height / 1_000_000,
        "status": status,
        "returnCode": return_code,
        "elapsedSeconds": time.monotonic() - started,
        "maximumObservedRssBytes": maximum_observed_rss,
        "resourceViolation": violation,
        "stdout": stdout_path.name,
        "stderr": stderr_path.name,
        "result": fixture_result,
        "artifacts": artifact_inventory(case) if case.exists() else [],
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Run 12/24/48 MP synthetic photo stress cases sequentially. "
            "Build rust/examples/photo_stress.rs separately before invoking this runner."
        )
    )
    parser.add_argument("--input", required=True, type=pathlib.Path, help="source photograph")
    parser.add_argument("--evidence", required=True, type=pathlib.Path, help="fresh output directory")
    parser.add_argument(
        "--binary",
        type=pathlib.Path,
        default=ROOT / "rust/target/release/examples/photo_stress",
        help="prebuilt photo_stress example",
    )
    parser.add_argument("--edits", type=int, default=100, help="brush edits per size (80-120)")
    parser.add_argument(
        "--fixture-source",
        type=pathlib.Path,
        default=ROOT / "rust/examples/photo_stress.rs",
        help="source snapshot used to build --binary (supply the frozen source for baseline runs)",
    )
    parser.add_argument(
        "--timeout-seconds",
        type=int,
        default=MAX_TIMEOUT_SECONDS,
        help=f"per-size timeout, at most {MAX_TIMEOUT_SECONDS} seconds",
    )
    parser.add_argument(
        "--require-region-history",
        action="store_true",
        help=(
            "require raster patch entries in every case and at least 80 retained undo steps "
            "at 48 MP; this is runner-side validation and is not passed to the binary"
        ),
    )
    args = parser.parse_args()

    input_path = args.input.resolve()
    evidence = args.evidence.resolve()
    binary = args.binary.resolve()
    fixture_source = args.fixture_source.resolve()
    if not input_path.is_file():
        parser.error("--input must be a regular file")
    if not fixture_source.is_file():
        parser.error("--fixture-source must be a regular file")
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error(
            "--binary must be an executable prebuilt photo_stress example; "
            "the runner intentionally does not invoke Cargo"
        )
    if not 80 <= args.edits <= 120:
        parser.error("--edits must be between 80 and 120")
    if not 1 <= args.timeout_seconds <= MAX_TIMEOUT_SECONDS:
        parser.error(f"--timeout-seconds must be between 1 and {MAX_TIMEOUT_SECONDS}")
    if evidence.exists():
        parser.error("--evidence must not exist; use a fresh directory")
    evidence.mkdir(parents=True)

    report = {
        "schemaVersion": 1,
        "qualification": "Omuse synthetic large-photo stress",
        "startedUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "input": {
            "path": str(input_path),
            "bytes": input_path.stat().st_size,
            "sha256": sha256_file(input_path),
        },
        "harness": {
            "runner": {
                "path": str(pathlib.Path(__file__).resolve()),
                "sha256": sha256_file(pathlib.Path(__file__).resolve()),
            },
            "fixtureSource": {
                "path": str(fixture_source),
                "sha256": sha256_file(fixture_source),
            },
            "binary": {"path": str(binary), "bytes": binary.stat().st_size, "sha256": sha256_file(binary)},
        },
        "configuration": {
            "sizes": [{"name": name, "width": width, "height": height} for name, width, height in SIZES],
            "brushEdits": args.edits,
            "historyLimitBytes": 256 * 1024**2,
            "compositeTimingSamples": 5,
            "processIsolation": "one size per process, sequential",
            "timeoutSecondsPerSize": args.timeout_seconds,
            "rssLimitBytes": RSS_LIMIT,
            "addressSpaceLimitBytes": ADDRESS_LIMIT,
            "rssEnforcement": "RLIMIT_RSS plus parent polling of Linux /proc VmRSS",
            "requireRegionHistory": args.require_region_history,
        },
        "limitations": [
            "The input photograph is resized and tiled into a synthetic size stress fixture",
            "This does not measure physical-input latency or camera decoding",
            "GUI recovery, Create pages and background saves can retain images and force additional copies",
            "This is a bounded workflow qualification, not a multi-hour soak test",
            "Wall-clock timings depend on host load",
        ],
        "cases": [],
        "expectedCases": [name for name, _, _ in SIZES],
        "completedCases": [],
    }

    exit_code = 0
    try:
        for name, width, height in SIZES:
            case = run_case(
                binary,
                input_path,
                evidence,
                name,
                width,
                height,
                args.edits,
                args.timeout_seconds,
            )
            report["cases"].append(case)
            report["completedCases"].append(name)
            if args.require_region_history:
                acceptance = region_history_acceptance(case["result"], name)
                case["regionHistoryAcceptance"] = acceptance
                if not acceptance["passed"] and case["status"] == "passed":
                    case["status"] = "acceptance-failed"
            if case["status"] != "passed":
                exit_code = 1
                break
    finally:
        report["finishedUtc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        all_cases_completed = report["completedCases"] == report["expectedCases"]
        report["allCasesCompleted"] = all_cases_completed
        report["status"] = "passed" if all_cases_completed and exit_code == 0 else "failed"
        (evidence / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    if exit_code:
        last = report["cases"][-1]
        print(
            f"{last['name']} failed ({last['status']}); see {evidence / last['stderr']}",
            file=sys.stderr,
        )
    else:
        print(evidence / "report.json")
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
