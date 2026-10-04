#!/usr/bin/env python3
"""Regression checks for dependency-test isolation, lock guards and diagnostics."""

from __future__ import annotations

import contextlib
import copy
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("dependency_replacements", ROOT / "scripts" / "test-dependency-replacements.py")
assert SPEC is not None and SPEC.loader is not None
HELPER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HELPER)


class DependencyHarnessTests(unittest.TestCase):
    def test_failed_child_preserves_captured_diagnostics_and_exit_code(self) -> None:
        diagnostics = io.StringIO()
        with contextlib.redirect_stderr(diagnostics), self.assertRaises(subprocess.CalledProcessError) as caught:
            HELPER.command([
                sys.executable, "-c",
                "import sys; print('resolver context'); print('missing cached crate', file=sys.stderr); sys.exit(101)",
            ], capture_output=True, encoding="utf-8")
        self.assertEqual(caught.exception.returncode, 101)
        self.assertIn("Command failed (101)", diagnostics.getvalue())
        self.assertIn("resolver context", diagnostics.getvalue())
        self.assertIn("missing cached crate", diagnostics.getvalue())

    def test_successful_metadata_output_remains_parseable(self) -> None:
        result = HELPER.command([sys.executable, "-c", 'print(\'{"packages": []}\')'], capture_output=True, encoding="utf-8")
        self.assertEqual(json.loads(result.stdout), {"packages": []})

    def test_workspace_copies_all_contracts_and_provenance_without_the_app(self) -> None:
        original_lock = (ROOT / "rust" / "Cargo.lock").read_bytes()
        with tempfile.TemporaryDirectory(prefix="omuse-harness-regression-") as temporary:
            scratch = Path(temporary)
            HELPER.prepare_workspace(scratch)
            manifest = tomllib.loads((scratch / "Cargo.toml").read_text(encoding="utf-8"))
            self.assertEqual(set(manifest["workspace"]["members"]), {"vendor/hexf-parse", "vendor/mac-compat"})
            self.assertEqual(set(manifest["dependencies"]), {*HELPER.CONSUMERS, "hexf-parse"})
            for directory in HELPER.REPLACEMENTS.values():
                original = HELPER.VENDOR / directory
                files = {path.relative_to(original): path.read_bytes() for path in original.rglob("*") if path.is_file()}
                copied = scratch / "vendor" / directory
                self.assertEqual(files, {path.relative_to(copied): path.read_bytes() for path in copied.rglob("*") if path.is_file()})
            self.assertTrue((scratch / "vendor" / "mac-compat" / "tests" / "contract.rs").is_file())
            self.assertEqual((scratch / "src" / "lib.rs").read_bytes(), (ROOT / "scripts" / "fixtures" / "dependency-replacements.rs").read_bytes())
            self.assertEqual((scratch / "Cargo.lock").read_bytes(), original_lock)
        self.assertEqual((ROOT / "rust" / "Cargo.lock").read_bytes(), original_lock)

    def test_lock_guards_reject_version_source_checksum_and_consumer_drift(self) -> None:
        lock = HELPER.check_sources()
        names = {*HELPER.CONSUMERS, *HELPER.REPLACEMENTS, "libm"}
        packages = [copy.deepcopy(p) for p in lock["package"] if p["name"] in names]
        with tempfile.TemporaryDirectory(prefix="omuse-resolution-regression-") as temporary:
            scratch = Path(temporary)
            metadata = {"packages": [dict(p, id=f"{p['name']}@{p['version']}", manifest_path=str(scratch / "vendor" / HELPER.REPLACEMENTS.get(p["name"], p["name"]) / "Cargo.toml")) for p in packages]}

            def write_lock(entries: list[dict]) -> None:
                lines = ["version = 4"]
                for entry in entries:
                    lines.append("[[package]]")
                    lines.extend(f"{key} = {json.dumps(entry[key])}" for key in ("name", "version", "source", "checksum") if key in entry)
                (scratch / "Cargo.lock").write_text("\n".join(lines) + "\n", encoding="utf-8")

            write_lock(packages)
            HELPER.check_resolution(metadata, scratch, lock)
            for field, value in (("version", "99.0.0"), ("checksum", "0" * 64), ("source", "registry+https://unreviewed.invalid/index")):
                with self.subTest(drift=field):
                    changed = copy.deepcopy(packages)
                    next(p for p in changed if p["name"] == "libm")[field] = value
                    write_lock(changed)
                    with self.assertRaises(AssertionError):
                        HELPER.check_resolution(metadata, scratch, lock)
            write_lock(packages)
            with self.subTest(drift="original registry mac"):
                changed = copy.deepcopy(metadata)
                package = next(p for p in changed["packages"] if p["name"] == "mac")
                package.update(version="0.1.1", source="registry+https://github.com/rust-lang/crates.io-index")
                with self.assertRaisesRegex(AssertionError, "left the application lock"):
                    HELPER.check_resolution(changed, scratch, lock)
            with self.subTest(drift="wrong replacement path"):
                changed = copy.deepcopy(metadata)
                next(p for p in changed["packages"] if p["name"] == "hexf-parse")["manifest_path"] = str(scratch / "unreviewed" / "Cargo.toml")
                with self.assertRaises(AssertionError):
                    HELPER.check_resolution(changed, scratch, lock)
            with self.subTest(drift="missing consumer"):
                changed = {"packages": [p for p in metadata["packages"] if p["name"] != "html5ever"]}
                with self.assertRaisesRegex(AssertionError, "Required replacement/consumer absent"):
                    HELPER.check_resolution(changed, scratch, lock)


if __name__ == "__main__":
    unittest.main()
