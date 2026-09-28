#!/usr/bin/env python3
"""Regression tests for the offline Rust bundle installer."""

from __future__ import annotations

import hashlib
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
INSTALLER = ROOT / "scripts" / "install-rust-bundle.sh"


class BundleTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="omuse-bundle-test-")
        self.base = Path(self.temporary.name)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def make_tree(self, version: str = "one", target: str = "linux-x86_64") -> Path:
        root = self.base / f"tree-{version}" / "omuse-bundle"
        files = {
            "bin/omuse": (
                "#!/bin/sh\nprintf '%s\\n' 'version=" + version + "' \"$@\"\n"
            ),
            "lib/libonnxruntime.so": "dummy onnx runtime\n",
            "lib/libraw.so": "dummy libraw\n",
            "models/u2netp.onnx": "dummy model\n",
            "share/icons/omuse.png": "dummy png icon\n",
            "share/icons/omuse.svg": "<svg/>\n",
            "licenses/rust-dependency-inventory.json": "{}\n",
            "licenses/Rust-THIRD-PARTY-NOTICES.txt": "synthetic notices\n",
            "SOURCE-REVISION": (
                "source_revision=synthetic\nsource_tree_dirty=false\n"
                f"target={target}\nbundle_kind=full-feature\n"
            ),
        }
        for relative, content in files.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        (root / "bin/omuse").chmod(0o755)
        checksums = []
        for path in sorted(root.rglob("*")):
            if path.is_file():
                relative = "./" + path.relative_to(root).as_posix()
                checksums.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {relative}\n")
        (root / "SHA256SUMS").write_text("".join(checksums), encoding="utf-8")
        return root

    def archive(
        self,
        root: Path,
        name: str,
        additions: list[tuple[tarfile.TarInfo, bytes | None]] | None = None,
    ) -> Path:
        archive = self.base / name
        with tarfile.open(archive, "w:gz") as output:
            output.add(root, arcname="omuse-bundle")
            for member, payload in additions or []:
                output.addfile(member, io.BytesIO(payload) if payload is not None else None)
        return archive

    def install(self, archive: Path, prefix: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(INSTALLER), str(archive), "--prefix", str(prefix)],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def assert_rejected_without_writes(
        self, archive: Path, outside: Path | None = None, prefix_name: str | None = None
    ) -> None:
        prefix = self.base / (prefix_name or f"rejected-{archive.stem}")
        prefix.mkdir()
        sentinel = prefix / "sentinel"
        sentinel.write_text("unchanged", encoding="utf-8")
        result = self.install(archive, prefix)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "unchanged")
        self.assertEqual([path.name for path in prefix.iterdir()], ["sentinel"])
        if outside is not None:
            self.assertFalse(outside.exists())

    def test_valid_metacharacter_prefix_launch_and_reinstall_preserves_binary(self) -> None:
        prefix = self.base / "prefix space $dollar `tick` %percent"
        first = self.archive(self.make_tree("one"), "one.tar.gz")
        result = self.install(first, prefix)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        launcher = prefix / "bin/omuse"
        launched = subprocess.run(
            [str(launcher), "argument with spaces", "$literal", "`literal`"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
        )
        self.assertEqual(
            launched.stdout.splitlines(),
            ["version=one", "argument with spaces", "$literal", "`literal`"],
        )
        desktop = (prefix / "share/applications/omuse.desktop").read_text(
            encoding="utf-8"
        )
        self.assertIn("Name=Omuse\n", desktop)
        self.assertIn("Icon=omuse\n", desktop)
        self.assertIn("StartupWMClass=omuse\n", desktop)
        self.assertIn("%%percent", desktop)
        self.assertIn("\\$dollar", desktop)
        self.assertIn("\\`tick\\`", desktop)

        second = self.archive(self.make_tree("two"), "two.tar.gz")
        result = self.install(second, prefix)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        previous = prefix / "opt/omuse/omuse.previous"
        old = subprocess.run([str(previous)], check=True, text=True, stdout=subprocess.PIPE)
        new = subprocess.run([str(launcher)], check=True, text=True, stdout=subprocess.PIPE)
        compatible = subprocess.run(
            [str(prefix / "bin/compositor-rust")],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
        )
        self.assertEqual(old.stdout, "version=one\n")
        self.assertEqual(new.stdout, "version=two\n")
        self.assertEqual(compatible.stdout, "version=two\n")
        self.assertTrue(
            (prefix / "share/icons/hicolor/256x256/apps/omuse.png").is_file()
        )
        self.assertTrue(
            (prefix / "share/icons/hicolor/scalable/apps/omuse.svg").is_file()
        )

    def test_first_rename_install_preserves_old_rust_binary_and_retires_its_desktop(self) -> None:
        prefix = self.base / "rename-prefix"
        old_binary = prefix / "opt/compositor-rust/compositor-rust"
        old_binary.parent.mkdir(parents=True)
        old_binary.write_text("#!/bin/sh\nprintf '%s\\n' legacy-rust\n", encoding="utf-8")
        old_binary.chmod(0o755)
        applications = prefix / "share/applications"
        applications.mkdir(parents=True)
        old_desktop = applications / "compositor-rust.desktop"
        old_desktop_text = (
            "[Desktop Entry]\nType=Application\nName=Compositor Rust\n"
            f'Exec="{prefix}/bin/compositor-rust" %f\n'
            "Icon=compositor-rust\nStartupWMClass=compositor-rust\n"
        )
        old_desktop.write_text(old_desktop_text, encoding="utf-8")

        protected = {
            prefix / "bin/compositor": "old Swift launcher\n",
            applications / "com.wonderassembly.Compositor.desktop": "old Swift desktop\n",
            applications / "compositor-candidate.desktop": "candidate desktop\n",
        }
        for path, content in protected.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")

        result = self.install(self.archive(self.make_tree("one"), "rename.tar.gz"), prefix)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        previous = prefix / "opt/omuse/omuse.previous"
        preserved = subprocess.run(
            [str(previous)], check=True, text=True, stdout=subprocess.PIPE
        )
        self.assertEqual(preserved.stdout, "legacy-rust\n")
        self.assertTrue(old_binary.is_file())
        self.assertFalse(old_desktop.exists())
        self.assertEqual(
            (applications / "compositor-rust.desktop.retired-by-omuse").read_text(
                encoding="utf-8"
            ),
            old_desktop_text,
        )
        for path, content in protected.items():
            self.assertEqual(path.read_text(encoding="utf-8"), content)

    def test_unrecognized_legacy_desktop_is_left_untouched(self) -> None:
        prefix = self.base / "custom-desktop-prefix"
        applications = prefix / "share/applications"
        applications.mkdir(parents=True)
        desktop = applications / "compositor-rust.desktop"
        custom = "[Desktop Entry]\nName=My custom launcher\nIcon=compositor-rust\n"
        desktop.write_text(custom, encoding="utf-8")
        result = self.install(self.archive(self.make_tree(), "custom.tar.gz"), prefix)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(desktop.read_text(encoding="utf-8"), custom)
        self.assertFalse(
            (applications / "compositor-rust.desktop.retired-by-omuse").exists()
        )

    def test_unrepresentable_desktop_prefix_is_rejected(self) -> None:
        archive = self.archive(self.make_tree(), "single-quote.tar.gz")
        self.assert_rejected_without_writes(archive, None, "rejected-'quote")
        self.assert_rejected_without_writes(archive, None, "rejected-\\slash")

    def test_omuse_install_prefix_takes_precedence_over_compatibility_alias(self) -> None:
        archive = self.archive(self.make_tree(), "environment-prefix.tar.gz")
        canonical = self.base / "canonical-prefix"
        compatibility = self.base / "compatibility-prefix"
        environment = os.environ.copy()
        environment["OMUSE_INSTALL_PREFIX"] = str(canonical)
        environment["COMPOSITOR_INSTALL_PREFIX"] = str(compatibility)
        result = subprocess.run(
            [str(INSTALLER), str(archive)],
            env=environment,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((canonical / "bin/omuse").is_file())
        self.assertFalse(compatibility.exists())

    def test_compatibility_install_prefix_alias_remains_supported(self) -> None:
        archive = self.archive(self.make_tree(), "compatibility-prefix.tar.gz")
        compatibility = self.base / "compatibility-only-prefix"
        environment = os.environ.copy()
        environment.pop("OMUSE_INSTALL_PREFIX", None)
        environment["COMPOSITOR_INSTALL_PREFIX"] = str(compatibility)
        result = subprocess.run(
            [str(INSTALLER), str(archive)],
            env=environment,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((compatibility / "bin/omuse").is_file())

    def test_checksum_tamper_is_rejected_before_install(self) -> None:
        root = self.make_tree()
        with (root / "bin/omuse").open("ab") as executable:
            executable.write(b"tamper")
        self.assert_rejected_without_writes(self.archive(root, "tampered.tar.gz"))

    def test_unmanifested_payload_is_rejected_before_install(self) -> None:
        root = self.make_tree()
        (root / "licenses/unlisted.txt").write_text("unchecked", encoding="utf-8")
        self.assert_rejected_without_writes(self.archive(root, "unlisted.tar.gz"))

    def test_symlink_is_rejected_before_outside_write(self) -> None:
        root = self.make_tree()
        outside = self.base / "outside"
        link = tarfile.TarInfo("omuse-bundle/escape")
        link.type = tarfile.SYMTYPE
        link.linkname = str(outside)
        payload = tarfile.TarInfo("omuse-bundle/escape/marker")
        data = b"escaped"
        payload.size = len(data)
        self.assert_rejected_without_writes(
            self.archive(root, "symlink.tar.gz", [(link, None), (payload, data)]), outside
        )

    def test_path_traversal_is_rejected_before_outside_write(self) -> None:
        root = self.make_tree()
        outside = self.base / "outside-marker"
        member = tarfile.TarInfo("omuse-bundle/../../outside-marker")
        data = b"escaped"
        member.size = len(data)
        self.assert_rejected_without_writes(
            self.archive(root, "traversal.tar.gz", [(member, data)]), outside
        )

    def test_wrong_platform_manifest_is_rejected_before_install(self) -> None:
        root = self.make_tree(target="linux-aarch64")
        self.assert_rejected_without_writes(self.archive(root, "wrong-platform.tar.gz"))


if __name__ == "__main__":
    unittest.main()
