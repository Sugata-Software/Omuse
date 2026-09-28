#!/usr/bin/env python3
"""Regression tests for curated Rust dependency license overrides."""

from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "rust-license-inventory.py"
SPEC = importlib.util.spec_from_file_location("rust_license_inventory", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
INVENTORY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(INVENTORY)
OVERRIDE_DIR = ROOT / "rust" / "licenses" / "dependency-overrides"


class DependencyOverrideTests(unittest.TestCase):
    def test_current_override_manifest_is_hash_verified_and_narrow(self) -> None:
        overrides = INVENTORY.load_overrides(OVERRIDE_DIR)
        self.assertEqual(len(overrides), 28)
        self.assertEqual(
            set(overrides),
            {
                ("accesskit", "0.24.1"),
                ("accesskit_atspi_common", "0.19.1"),
                ("accesskit_consumer", "0.38.0"),
                ("accesskit_unix", "0.22.1"),
                ("gpu-descriptor", "0.3.2"),
                ("gpu-descriptor-types", "0.2.0"),
                ("gpui-kit", "0.6.6"),
                ("harfrust", "0.5.2"),
                ("lyon", "1.0.19"),
                ("lyon_algorithms", "1.0.21"),
                ("lyon_geom", "1.0.19"),
                ("lyon_path", "1.0.19"),
                ("lyon_tessellation", "1.0.22"),
                ("pathfinder_geometry", "0.5.1"),
                ("pathfinder_simd", "0.5.6"),
                ("profiling", "1.0.18"),
                ("profiling-procmacros", "1.0.18"),
                ("pulp-wasm-simd-flag", "0.1.1"),
                ("rust-i18n-macro", "4.2.2"),
                ("rust-i18n-support", "4.2.2"),
                ("seahash", "4.1.0"),
                ("simd_helpers", "0.1.0"),
                ("spirv", "0.4.0+sdk-1.4.341.0"),
                ("svg_fmt", "0.4.5"),
                ("taffy", "0.13.0"),
                ("xim-ctext", "0.3.0"),
                ("xim-parser", "0.2.2"),
                ("zune-inflate", "0.2.54"),
            },
        )
        for override in overrides.values():
            self.assertTrue(override["sourceUrl"].startswith("https://"))
            self.assertTrue(INVENTORY.REVISION_RE.fullmatch(override["sourceRevision"]))
            self.assertTrue(override["legalFiles"])
        for key in (("seahash", "4.1.0"), ("simd_helpers", "0.1.0")):
            provenance = overrides[key]["legalTextProvenance"]
            self.assertEqual(
                provenance["directParentRevision"], overrides[key]["sourceRevision"]
            )
            self.assertIn(provenance["revision"], provenance["commitUrl"])
        manifest = json.loads((OVERRIDE_DIR / "overrides.json").read_text(encoding="utf-8"))
        listed = {
            legal_file["path"]
            for entry in manifest["overrides"]
            for legal_file in entry["legalFiles"]
        }
        actual = {
            path.name
            for path in OVERRIDE_DIR.iterdir()
            if path.is_file() and path.name != "overrides.json"
        }
        self.assertEqual(actual, listed)

    def test_hash_and_revision_patterns_reject_trailing_input(self) -> None:
        self.assertIsNotNone(INVENTORY.SHA256_RE.fullmatch("a" * 64))
        self.assertIsNone(INVENTORY.SHA256_RE.fullmatch("a" * 64 + "x"))
        self.assertIsNotNone(INVENTORY.REVISION_RE.fullmatch("b" * 40))
        self.assertIsNone(INVENTORY.REVISION_RE.fullmatch("b" * 40 + "x"))

    def test_tampered_notice_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            notice = directory / "NOTICE.txt"
            notice.write_text("exact upstream notice\n", encoding="utf-8")
            revision = "a" * 40
            manifest = {
                "schemaVersion": 1,
                "overrides": [
                    {
                        "id": "test",
                        "sourceUrl": "https://example.invalid/source",
                        "sourceRevision": revision,
                        "crateVersionAllowlist": [
                            {
                                "name": "test-crate",
                                "version": "1.0.0",
                                "licenseExpression": "MIT",
                            }
                        ],
                        "legalFiles": [
                            {
                                "path": "NOTICE.txt",
                                "sha256": hashlib.sha256(b"different").hexdigest(),
                                "sourceUrl": f"https://example.invalid/{revision}/NOTICE.txt",
                            }
                        ],
                    }
                ],
            }
            (directory / "overrides.json").write_text(
                json.dumps(manifest), encoding="utf-8"
            )
            with self.assertRaises(ValueError):
                INVENTORY.load_overrides(directory)

    def test_retrospective_notice_requires_the_published_revision_as_direct_parent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            notice = directory / "LICENSE"
            notice.write_text("exact upstream notice\n", encoding="utf-8")
            source_revision = "a" * 40
            legal_text_revision = "b" * 40
            manifest = {
                "schemaVersion": 1,
                "overrides": [
                    {
                        "id": "test",
                        "sourceUrl": "https://example.invalid/source",
                        "sourceRevision": source_revision,
                        "legalTextProvenance": {
                            "revision": legal_text_revision,
                            "directParentRevision": "c" * 40,
                            "commitUrl": f"https://example.invalid/commit/{legal_text_revision}",
                        },
                        "crateVersionAllowlist": [
                            {
                                "name": "test-crate",
                                "version": "1.0.0",
                                "licenseExpression": "MIT",
                            }
                        ],
                        "legalFiles": [
                            {
                                "path": "LICENSE",
                                "sha256": hashlib.sha256(
                                    notice.read_bytes()
                                ).hexdigest(),
                                "sourceUrl": f"https://example.invalid/{legal_text_revision}/LICENSE",
                            }
                        ],
                    }
                ],
            }
            (directory / "overrides.json").write_text(
                json.dumps(manifest), encoding="utf-8"
            )
            with self.assertRaises(ValueError):
                INVENTORY.load_overrides(directory)

    def test_override_origin_requires_repository_and_published_vcs_revision(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "Cargo.toml").write_text(
                "[package]\nname = 'test-crate'\nversion = '1.0.0'\n"
                "repository = 'https://example.invalid/wrong-origin'\n",
                encoding="utf-8",
            )
            revision = "c" * 40
            (directory / ".cargo_vcs_info.json").write_text(
                json.dumps({"git": {"sha1": revision}}), encoding="utf-8"
            )
            with self.assertRaises(ValueError):
                INVENTORY.validate_override_origin(
                    directory,
                    {
                        "id": "test",
                        "sourceUrl": "https://example.invalid/right-origin",
                        "sourceRevision": revision,
                    },
                )

    def test_source_header_override_must_match_the_published_source_file(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            package = directory / "package"
            source = package / "src" / "lib.rs"
            source.parent.mkdir(parents=True)
            header = b"// Copyright Example\n// Licensed under MIT\n"
            source.write_bytes(header + b"pub fn example() {}\n")
            (package / "Cargo.toml").write_text(
                "[package]\nname = 'test-crate'\nversion = '1.0.0'\n"
                "repository = 'https://example.invalid/source'\n",
                encoding="utf-8",
            )
            revision = "d" * 40
            (package / ".cargo_vcs_info.json").write_text(
                json.dumps({"git": {"sha1": revision}}), encoding="utf-8"
            )
            notice = directory / "NOTICE.txt"
            notice.write_bytes(header)
            override = {
                "id": "test",
                "sourceUrl": "https://example.invalid/source",
                "sourceRevision": revision,
                "legalFiles": [
                    {
                        "path": notice,
                        "publishedCratePath": "src/lib.rs",
                        "publishedCrateSha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                    }
                ],
            }
            INVENTORY.validate_override_origin(package, override)
            source.write_bytes(b"changed\n")
            with self.assertRaises(ValueError):
                INVENTORY.validate_override_origin(package, override)


if __name__ == "__main__":
    unittest.main()
