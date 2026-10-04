#!/usr/bin/env python3
"""Verify dependency provenance and exercise the patched parser dependencies.

Uses the existing locked Cargo cache, an isolated test workspace and two build
jobs by default. Does not download crates, change the app lockfile or build GPUI.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib


ROOT = Path(__file__).resolve().parent.parent
VENDOR = ROOT / "rust" / "vendor"
CONSUMERS = {
    "html5ever": "0.27.0",
    "markup5ever_rcdom": "0.3.0",
    "xml5ever": "0.18.1",
    "tendril": "0.4.3",
    "futf": "0.1.5",
}
REPLACEMENTS = {"hexf-parse": "hexf-parse", "mac": "mac-compat"}
ACCEPTANCE = "omuse-dependency-acceptance"


def command(args: list[str], **kwargs) -> subprocess.CompletedProcess:
    try:
        return subprocess.run(args, check=True, **kwargs)
    except subprocess.CalledProcessError as error:
        # Metadata must be captured as JSON on success. On failure, keeping its
        # stderr inside CalledProcessError hides the actual Cargo/cache error.
        print(f"Command failed ({error.returncode}): {subprocess.list2cmdline(args)}", file=sys.stderr)
        for output in (error.stdout, error.stderr):
            if output:
                if isinstance(output, bytes):
                    output = output.decode("utf-8", errors="replace")
                print(output, end="" if output.endswith("\n") else "\n", file=sys.stderr)
        raise


def host_target(env: dict[str, str]) -> str:
    result = command(["rustc", "--version", "--verbose"], env=env, capture_output=True, encoding="utf-8")
    for line in result.stdout.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ").strip()
    raise RuntimeError("rustc did not report its host target")


def prepare_workspace(scratch: Path) -> None:
    # The replacements must be workspace members, not packages selected through
    # Omuse's target-dependent GPUI graph. hexf-parse is not active on Windows in
    # that graph, and asking Cargo to test it there can panic during resolution.
    for directory in REPLACEMENTS.values():
        shutil.copytree(VENDOR / directory, scratch / "vendor" / directory)
    lines = [
        "[workspace]", 'members = ["vendor/hexf-parse", "vendor/mac-compat"]', 'resolver = "3"',
        "[package]", f'name = "{ACCEPTANCE}"', 'version = "0.0.0"',
        'edition = "2021"', 'publish = false', "[dependencies]",
        'hexf-parse = { path = "vendor/hexf-parse" }',
        *[f'{name} = "={version}"' for name, version in CONSUMERS.items()],
        "[patch.crates-io]", 'mac = { path = "vendor/mac-compat" }',
    ]
    (scratch / "Cargo.toml").write_text("\n".join(lines) + "\n", encoding="utf-8")
    (scratch / "src").mkdir()
    shutil.copyfile(ROOT / "scripts" / "fixtures" / "dependency-replacements.rs", scratch / "src" / "lib.rs")
    shutil.copyfile(ROOT / "rust" / "Cargo.lock", scratch / "Cargo.lock")


def check_resolution(metadata: dict, scratch: Path, lock: dict) -> None:
    def identity(package: dict) -> tuple:
        return package["name"], package["version"], package.get("source")

    locked = {identity(p): p.get("checksum") for p in lock["package"]}
    acceptance = (ACCEPTANCE, "0.0.0", None)
    # Metadata is filtered to the native target, but the temporary lock can
    # still contain other-target dependencies. Check all of them, including
    # registry checksums, before any crate is built.
    scratch_lock = tomllib.loads((scratch / "Cargo.lock").read_text(encoding="utf-8"))
    for package in scratch_lock["package"]:
        if identity(package) == acceptance:
            continue
        key = identity(package)
        assert key in locked, f"Dependency left the application lock: {key}"
        assert package.get("checksum") == locked[key], f"Dependency checksum changed: {key}"
    resolved = set()
    for package in metadata["packages"]:
        if identity(package) == acceptance:
            continue
        assert identity(package) in locked, f"Dependency left the application lock: {package['id']}"
        resolved.add((package["name"], package["version"]))
        if package["name"] in REPLACEMENTS:
            expected = scratch / "vendor" / REPLACEMENTS[package["name"]] / "Cargo.toml"
            assert Path(package["manifest_path"]).resolve() == expected.resolve(), package["id"]
    for name, version in {**CONSUMERS, "mac": "0.1.1+omuse.1", "hexf-parse": "0.2.1"}.items():
        assert (name, version) in resolved, f"Required replacement/consumer absent: {name} {version}"


def check_sources() -> dict:
    manifest = json.loads((VENDOR / "hexf-parse" / "OMUSE-PROVENANCE.json").read_text())
    assert manifest["sourceRepository"] == "https://github.com/lifthrasiir/hexf"
    assert manifest["sourceRevision"] == "41f0018229c1ee3d6fd813b6808d1ad1f506554c"
    assert manifest["licenseExpression"] == "0BSD"
    assert {(f["path"], f["sourcePath"]) for f in manifest["files"]} == {
        ("Cargo.toml", "parse/Cargo.toml"), ("src/lib.rs", "parse/src/lib.rs"), ("LICENSE", "LICENSE")
    }
    for entry in manifest["files"]:
        content = (VENDOR / "hexf-parse" / entry["path"]).read_bytes()
        assert hashlib.sha256(content).hexdigest() == entry["sha256"], entry["path"]
    patches = tomllib.loads((ROOT / "rust" / "Cargo.toml").read_text())["patch"]["crates-io"]
    assert patches["mac"] == {"path": "vendor/mac-compat"}, patches["mac"]
    assert patches["hexf-parse"] == {"path": "vendor/hexf-parse"}, patches["hexf-parse"]
    lock = tomllib.loads((ROOT / "rust" / "Cargo.lock").read_text())
    for name, version in {"mac": "0.1.1+omuse.1", "hexf-parse": "0.2.1"}.items():
        matches = [p for p in lock["package"] if p["name"] == name]
        assert len(matches) == 1 and matches[0]["version"] == version, matches
        assert "source" not in matches[0] and "checksum" not in matches[0], matches
    for name, version in CONSUMERS.items():
        assert any(p["name"] == name and p["version"] == version for p in lock["package"]), name
    return lock


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--provenance-only", action="store_true")
    args = parser.parse_args()
    lock = check_sources()
    print("Verified pinned upstream source hashes and removal of original registry packages.", flush=True)
    if args.provenance_only:
        return
    env = os.environ.copy()
    env.setdefault("CARGO_BUILD_JOBS", "2")
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "rust" / "target" / "dependency-replacements"))
    target = host_target(env)
    with tempfile.TemporaryDirectory(prefix="omuse-dependency-test-") as temporary:
        scratch = Path(temporary)
        prepare_workspace(scratch)
        # Retain the application lock's selections, then let Cargo prune only the
        # temporary harness lock. Unfiltered metadata downloads sources for other
        # platforms even when they are not needed to run these tests; CI only
        # promises a cache populated by the native application build.
        result = command([
            "cargo", "metadata", "--manifest-path", str(scratch / "Cargo.toml"),
            "--offline", "--format-version", "1", "--filter-platform", target,
        ], env=env, cwd=scratch, capture_output=True, encoding="utf-8")
        metadata = json.loads(result.stdout)
        check_resolution(metadata, scratch, lock)
        print(f"Verified isolated {target} test graph against the application lock.", flush=True)
        command([
            "cargo", "test", "--manifest-path", str(scratch / "Cargo.toml"),
            "--offline", "--locked", "--workspace", "--target", target,
        ], env=env, cwd=scratch)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(error.returncode)
