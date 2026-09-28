#!/usr/bin/env python3
"""Compare Create raster ownership in fresh, bounded headless GPUI processes.

Both modes use the same release test executable. Legacy retains the synced
Project cache; current checks those page documents out to their Editors.
Recovery must finish before each measured stroke. This is not input latency.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import signal
import statistics
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parent.parent
RSS_LIMIT = 3 * 1024**3
ADDRESS_LIMIT = 4 * 1024**3
LOG_LIMIT = 16 * 1024**2
TEST = "ui::desktop_history_tests::benchmark_desktop_create_history"


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (ADDRESS_LIMIT, ADDRESS_LIMIT))


def run(binary, evidence, repetition, megapixels, mode):
    name = f"run-{repetition}-{megapixels}mp-{mode}"
    log = evidence / f"{name}.log"
    started = time.monotonic()
    peak = 0
    with tempfile.TemporaryDirectory(prefix="omuse-desktop-history-") as scratch:
        env = dict(os.environ, OMUSE_DESKTOP_STRESS_MP=str(megapixels),
                   OMUSE_DESKTOP_STRESS_MODE=mode)
        for kind in ("DATA", "CONFIG", "CACHE", "STATE"):
            env[f"XDG_{kind}_HOME"] = str(Path(scratch) / kind.lower())
        with log.open("w") as output:
            process = subprocess.Popen(
                [str(binary), TEST, "--ignored", "--exact", "--nocapture", "--test-threads=1"],
                cwd=ROOT, env=env, stdin=subprocess.DEVNULL, stdout=output,
                stderr=subprocess.STDOUT, start_new_session=True, preexec_fn=limits,
            )
            try:
                while process.poll() is None:
                    if log.stat().st_size > LOG_LIMIT:
                        raise RuntimeError(f"{name}: exceeded the 16 MiB diagnostic log ceiling")
                    try:
                        status = Path(f"/proc/{process.pid}/status").read_text()
                        rss = next((int(line.split()[1]) * 1024 for line in status.splitlines()
                                    if line.startswith("VmRSS:")), 0)
                        peak = max(peak, rss)
                        if rss > RSS_LIMIT:
                            raise RuntimeError(f"{name}: exceeded the 3 GiB RSS ceiling")
                    except (FileNotFoundError, ProcessLookupError):
                        pass
                    if time.monotonic() - started > 900:
                        raise RuntimeError(f"{name}: exceeded 900 seconds")
                    time.sleep(0.2)
                if process.returncode:
                    raise RuntimeError(f"{name}: exited {process.returncode}; see {log}")
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
    records = []
    with log.open() as output:
        while line := output.readline(1024**2 + 1):
            if len(line) > 1024**2:
                raise RuntimeError(f"{name}: oversized diagnostic line")
            try:
                record = json.loads(line[line.index("{"):])
                if record.get("event") == "desktop-history-result":
                    records.append(record)
            except (ValueError, json.JSONDecodeError):
                pass
    if len(records) != 1:
        raise RuntimeError(f"{name}: expected exactly one completed result")
    result = records[0]
    expected_detach = 0 if mode == "current" else megapixels * 1_000_000 * 4 * 12
    checks = {
        "requested mode and size": result["mode"] == mode and result["megapixels"] == megapixels,
        "complete stroke history": result["strokes"] == 12 and result["history"]["undoDepth"] == 13,
        "expected full-raster detach count": result["history"]["detachedRasterBytes"] == expected_detach,
        "exact final redo and recovery": result["hashes"]["final"] == result["hashes"]["redo"] == result["hashes"]["recovery"],
        "exact full undo": result["hashes"]["undo"] == result["hashes"]["warmup"],
        "peak RSS within ceiling": isinstance(result["peakRssBytes"], int) and result["peakRssBytes"] <= RSS_LIMIT,
    }
    failures = [label for label, passed in checks.items() if not passed]
    if failures:
        raise RuntimeError(f"{name}: failed acceptance: {', '.join(failures)}")
    result.update(repetition=repetition, log=log.name, observedPeakRssBytes=peak,
                  processWallSeconds=time.monotonic() - started)
    (evidence / f"{name}.json").write_text(json.dumps(result, indent=2) + "\n")
    print(f"Passed {name}: paint {result['paintCoreMs']['median']:.2f} ms; "
          f"frame/refresh {result['desktopFrameAndChangedWallMs']['median']:.2f} ms", flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path, help="Release omuse test executable")
    parser.add_argument("--evidence", required=True, type=Path, help="New evidence directory")
    parser.add_argument("--repetitions", type=int, choices=range(1, 6), default=3)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    report = {
        "passed": False,
        "binarySha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "comparison": "Legacy cache strategy versus checked-out pages in the same test executable",
        "scope": "Recovery idle before each stroke; headless GPUI frame plus changed(), not input latency",
        "limits": {"rssBytes": RSS_LIMIT, "addressBytes": ADDRESS_LIMIT, "logBytes": LOG_LIMIT, "timeoutSeconds": 900},
        "expectedRuns": args.repetitions * 6,
        "runs": [],
    }
    destination = evidence / "report.json"
    try:
        for repetition in range(1, args.repetitions + 1):
            for megapixels in (12, 24, 48):
                modes = ("legacy", "current") if repetition % 2 else ("current", "legacy")
                pair = [run(binary, evidence, repetition, megapixels, mode) for mode in modes]
                if pair[0]["hashes"] != pair[1]["hashes"]:
                    raise RuntimeError("Paired pixels differ")
                report["runs"].extend(pair)
                destination.write_text(json.dumps(report, indent=2) + "\n")
        report["summary"] = []
        for megapixels in (12, 24, 48):
            row = {"megapixels": megapixels}
            for mode in ("legacy", "current"):
                runs = [r for r in report["runs"] if r["megapixels"] == megapixels and r["mode"] == mode]
                row[mode] = {field: {quantile: statistics.median(r[field][quantile] for r in runs)
                                     for quantile in ("median", "p95", "max")}
                             for field in ("paintCoreMs", "desktopFrameAndChangedWallMs")}
                row[mode]["peakRssBytes"] = statistics.median(r["peakRssBytes"] for r in runs)
            report["summary"].append(row)
        report["passed"] = len(report["runs"]) == report["expectedRuns"]
    except BaseException as error:
        report["error"] = str(error)
        raise
    finally:
        destination.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
