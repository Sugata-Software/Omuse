#!/usr/bin/env python3
"""Verify dependency provenance and exercise the patched parser dependencies.

Uses the existing locked Cargo cache, a temporary acceptance crate and two build
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


def command(args: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(args, check=True, **kwargs)


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
    command([
        "cargo", "test", "--manifest-path", str(ROOT / "rust" / "Cargo.toml"),
        "--offline", "--locked", "--package", "hexf-parse", "--package", "mac",
    ], env=env)
    with tempfile.TemporaryDirectory(prefix="omuse-dependency-test-") as temporary:
        scratch = Path(temporary)
        lines = [
            "[package]", 'name = "omuse-dependency-acceptance"', 'version = "0.0.0"',
            'edition = "2021"', 'publish = false', "[dependencies]",
            f'hexf-parse = {{ path = {json.dumps((VENDOR / "hexf-parse").as_posix())} }}',
            *[f'{name} = "={version}"' for name, version in CONSUMERS.items()],
            "[patch.crates-io]",
            f'mac = {{ path = {json.dumps((VENDOR / "mac-compat").as_posix())} }}',
        ]
        (scratch / "Cargo.toml").write_text("\n".join(lines) + "\n")
        (scratch / "src").mkdir()
        shutil.copyfile(ROOT / "scripts" / "fixtures" / "dependency-replacements.rs", scratch / "src" / "lib.rs")
        # Retain the application lock's selections, then let Cargo prune only the
        # temporary harness lock. Fail if any resolved dependency leaves that set.
        shutil.copyfile(ROOT / "rust" / "Cargo.lock", scratch / "Cargo.lock")
        result = command([
            "cargo", "metadata", "--manifest-path", str(scratch / "Cargo.toml"),
            "--offline", "--format-version", "1",
        ], env=env, capture_output=True, encoding="utf-8")
        metadata = json.loads(result.stdout)
        locked = {(p["name"], p["version"], p.get("source")) for p in lock["package"]}
        for package in metadata["packages"]:
            if package["name"] == "omuse-dependency-acceptance":
                continue
            assert (package["name"], package["version"], package.get("source")) in locked, package["id"]
        command([
            "cargo", "test", "--manifest-path", str(scratch / "Cargo.toml"),
            "--offline", "--locked",
        ], env=env)


if __name__ == "__main__":
    main()
