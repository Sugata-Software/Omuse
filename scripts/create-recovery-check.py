#!/usr/bin/env python3
"""Qualify atomic multi-page `.omuse` saves with a prebuilt fixture executable.

The checker never builds or modifies Omuse. Pass the exact release fixture
binary so the caller can reuse the compiler artifact selected by its normal
release gate.
"""

import argparse
import datetime
import hashlib
import json
import os
import pathlib
import signal
import subprocess
import sys
import time


ROOT = pathlib.Path(__file__).resolve().parent.parent
PAGE_COUNT = 6
MAX_REVISIONS = 32


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def package_sha256(path: pathlib.Path) -> str:
    if not path.is_dir():
        raise RuntimeError(f"package is missing: {path}")
    digest = hashlib.sha256()
    for item in sorted(candidate for candidate in path.rglob("*") if candidate.is_file()):
        digest.update(item.relative_to(path).as_posix().encode("utf-8"))
        digest.update(b"\0")
        with item.open("rb") as source:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
    return digest.hexdigest()


def git_value(arguments: list[str]) -> str | None:
    result = subprocess.run(
        ["git", *arguments], cwd=ROOT, text=True, capture_output=True, timeout=10
    )
    return result.stdout.strip() if result.returncode == 0 else None


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def read_json_line(output: str, label: str) -> dict:
    lines = [line for line in output.splitlines() if line.strip()]
    if not lines:
        raise RuntimeError(f"{label} produced no JSON evidence")
    try:
        value = json.loads(lines[-1])
    except json.JSONDecodeError as error:
        raise RuntimeError(f"{label} did not end with JSON evidence: {error}") from error
    if not isinstance(value, dict):
        raise RuntimeError(f"{label} JSON evidence is not an object")
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("evidence", type=pathlib.Path, help="new evidence directory")
    parser.add_argument(
        "--fixture",
        type=pathlib.Path,
        required=True,
        help="explicit compiled create_recovery_fixture executable",
    )
    parser.add_argument("--revisions", type=int, default=6)
    parser.add_argument("--kill-revision", type=int, default=3)
    args = parser.parse_args()
    if not 2 <= args.revisions <= MAX_REVISIONS:
        parser.error(f"--revisions must be 2–{MAX_REVISIONS}")
    if not 1 <= args.kill_revision < args.revisions:
        parser.error("--kill-revision must be after the first completed save")

    evidence = args.evidence.resolve()
    fixture = args.fixture.resolve()
    if evidence.exists():
        parser.error("use a fresh evidence directory so stale artifacts cannot pass")
    if not fixture.is_file() or not os.access(fixture, os.X_OK):
        parser.error("--fixture must be an executable file")
    fixture_source = ROOT / "rust/examples/create_recovery_fixture.rs"
    if not fixture_source.is_file():
        parser.error("fixture source is missing from this checkout")

    evidence.mkdir(parents=True)
    environment = os.environ.copy()
    for key, directory in (
        ("XDG_DATA_HOME", "xdg-data"),
        ("XDG_CONFIG_HOME", "xdg-config"),
        ("XDG_CACHE_HOME", "xdg-cache"),
        ("XDG_STATE_HOME", "xdg-state"),
    ):
        location = evidence / directory
        location.mkdir()
        environment[key] = str(location)

    configuration = {
        "evidence": str(evidence),
        "fixture": str(fixture),
        "revisions": args.revisions,
        "killRevision": args.kill_revision,
        "pageCount": PAGE_COUNT,
    }
    report: dict = {
        "startedUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "configuration": configuration,
        "commit": git_value(["rev-parse", "HEAD"]),
        "workingTreeStatus": git_value(["status", "--porcelain"]),
        "binarySha256": sha256_file(fixture),
        "sourceSha256": {
            "fixture": sha256_file(fixture_source),
            "checker": sha256_file(pathlib.Path(__file__).resolve()),
        },
        "checks": [],
        "limitations": [
            "SIGKILL timing is coordinated at the staged-before-publish callback, not a power-loss simulation.",
            "The check proves complete old and new package snapshots visible after an interrupted writer; it does not remove the preserved staging evidence.",
        ],
        "passed": False,
    }

    def write_report() -> None:
        (evidence / "results.json").write_text(json.dumps(report, indent=2) + "\n")

    def record(name: str, started: float, **details: object) -> None:
        report["checks"].append(
            {"name": name, "seconds": round(time.monotonic() - started, 3), **details}
        )
        write_report()

    def invoke(label: str, command: list[str], timeout: float, env: dict[str, str] | None = None) -> dict:
        result = subprocess.run(
            command,
            cwd=ROOT,
            env=environment if env is None else env,
            text=True,
            capture_output=True,
            timeout=timeout,
        )
        (evidence / f"{label}.stdout.log").write_text(result.stdout)
        (evidence / f"{label}.stderr.log").write_text(result.stderr)
        if result.returncode:
            detail = result.stderr.strip() or result.stdout.strip() or f"exit {result.returncode}"
            raise RuntimeError(f"{label} failed: {detail}")
        return read_json_line(result.stdout, label)

    verify_counter = 0

    def verify(label: str, package: pathlib.Path) -> dict:
        nonlocal verify_counter
        verify_counter += 1
        evidence_value = invoke(
            f"{label}-verify-{verify_counter}",
            [str(fixture), "verify", str(package)],
            90,
        )
        require(evidence_value.get("pageCount") == PAGE_COUNT, f"{label} lost a page")
        require(evidence_value.get("brandCount") == 1, f"{label} lost its brand")
        require(evidence_value.get("componentCount") == 2, f"{label} lost a component")
        require(evidence_value.get("hasSharedBackground") is True, f"{label} lost its shared background")
        resources = evidence_value.get("resources")
        pages = evidence_value.get("pages")
        require(isinstance(resources, list) and len(resources) == 1, f"{label} lost its packaged resource")
        require(isinstance(pages, list) and len(pages) == PAGE_COUNT, f"{label} has incomplete page evidence")
        for page in pages:
            require(page.get("sharedBackgrounds") == 1, f"{label} has an invalid shared background")
            require(page.get("sourceCount", 0) >= 1, f"{label} lost original source pixels")
            fields = page.get("nativeFields")
            require(
                isinstance(fields, dict) and set(fields) == {"page_label", "headline", "body"},
                f"{label} lost native text fields",
            )
        return evidence_value

    try:
        sustained = evidence / "sustained"
        started = time.monotonic()
        invoke(
            "sustained-run",
            [str(fixture), "run", str(sustained), str(args.revisions)],
            240,
        )
        sustained_destination = verify("sustained-destination", sustained / "collection.omuse")
        sustained_recovery = verify("sustained-recovery", sustained / "recovery.omuse")
        journal = [
            json.loads(line)
            for line in (sustained / "journal.jsonl").read_text().splitlines()
            if line.strip()
        ]
        require(len(journal) == args.revisions, "sustained journal is incomplete")
        require(
            sustained_destination.get("revision") == sustained_recovery.get("revision") == args.revisions - 1,
            "sustained save/reopen revision mismatch",
        )
        require(
            sustained_destination.get("resources") == sustained_recovery.get("resources"),
            "sustained package resource evidence differs",
        )
        record(
            "sustained-save-reopen",
            started,
            status="passed",
            journalRevisions=len(journal),
            destination=sustained_destination,
            recovery=sustained_recovery,
            destinationSha256=package_sha256(sustained / "collection.omuse"),
            recoverySha256=package_sha256(sustained / "recovery.omuse"),
        )

        interrupted = evidence / "interrupted"
        interrupted_env = environment.copy()
        interrupted_env["OMUSE_CREATE_RECOVERY_PAUSE_REVISION"] = str(args.kill_revision)
        started = time.monotonic()
        stdout_path = evidence / "interrupted-run.stdout.log"
        stderr_path = evidence / "interrupted-run.stderr.log"
        with stdout_path.open("w") as stdout, stderr_path.open("w") as stderr:
            process = subprocess.Popen(
                [str(fixture), "run", str(interrupted), str(args.revisions)],
                cwd=ROOT,
                env=interrupted_env,
                stdout=stdout,
                stderr=stderr,
                text=True,
            )
            deadline = time.monotonic() + 90
            observed = None
            while time.monotonic() < deadline:
                phase_path = interrupted / "phase.json"
                if phase_path.is_file():
                    try:
                        phase = json.loads(phase_path.read_text())
                    except (OSError, json.JSONDecodeError):
                        phase = {}
                    stages = sorted(
                        item.name
                        for item in interrupted.glob(".omuse-create-stage-*")
                        if item.is_dir()
                    )
                    if (
                        phase.get("phase") == "destination-staged"
                        and phase.get("revision") == args.kill_revision
                        and stages
                    ):
                        observed = {"phase": phase, "stages": stages}
                        process.send_signal(signal.SIGKILL)
                        break
                if process.poll() is not None:
                    break
                time.sleep(0.005)
            if observed is None:
                if process.poll() is None:
                    process.kill()
                process.wait(timeout=10)
                raise RuntimeError("did not observe a staged destination save for SIGKILL")
            process.wait(timeout=10)

        require(process.returncode == -signal.SIGKILL, "writer did not terminate from SIGKILL")
        orphan_stages = sorted(
            item.name
            for item in interrupted.glob(".omuse-create-stage-*")
            if item.is_dir()
        )
        require(
            set(observed["stages"]).issubset(orphan_stages),
            "the observed staged package disappeared before interrupted-save inspection",
        )
        interrupted_destination = verify("interrupted-destination", interrupted / "collection.omuse")
        interrupted_recovery = verify("interrupted-recovery", interrupted / "recovery.omuse")
        require(
            interrupted_destination.get("revision") == args.kill_revision - 1,
            "destination is not the complete snapshot before the interrupted publish",
        )
        require(
            interrupted_recovery.get("revision") == args.kill_revision,
            "recovery is not the complete snapshot before the interrupted publish",
        )
        require(
            interrupted_destination.get("resources") == interrupted_recovery.get("resources"),
            "old and new snapshots disagree about packaged resources",
        )
        record(
            "sigkill-staged-save",
            started,
            status="passed",
            signal="SIGKILL",
            observed=observed,
            preservedStages=orphan_stages,
            destination=interrupted_destination,
            recovery=interrupted_recovery,
            destinationSha256=package_sha256(interrupted / "collection.omuse"),
            recoverySha256=package_sha256(interrupted / "recovery.omuse"),
        )
        report["completedUtc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        report["passed"] = True
        write_report()
        print(json.dumps(report, indent=2))
        return 0
    except Exception as error:
        report["completedUtc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        report["error"] = str(error)
        write_report()
        raise


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"create recovery qualification failed: {error}", file=sys.stderr)
        raise SystemExit(1)
