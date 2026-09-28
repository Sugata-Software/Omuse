#!/usr/bin/env python3
"""Exercise installer transactions and the curl entry point without network/sudo."""

import argparse
import contextlib
import ctypes
import errno
import fcntl
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parent.parent
MANAGER = ROOT / "scripts/install-app.py"
INTEGRATION_FILES = (
    "opt/omuse/manage.py",
    "bin/omuse",
    "bin/omuse-manage",
    "share/applications/omuse.desktop",
    "share/icons/hicolor/256x256/apps/omuse.png",
    "share/icons/hicolor/scalable/apps/omuse.svg",
    "opt/omuse/installed-files.json",
)


def load_manager():
    spec = importlib.util.spec_from_file_location("omuse_installer_under_test", MANAGER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="omuse-install-test-")
        self.base = Path(self.temporary.name)
        self.prefix = self.base / "prefix space $dollar `tick` %percent"

    def tearDown(self):
        self.temporary.cleanup()

    def payload(self, version, failing=False):
        payload = self.base / version
        payload.mkdir()
        binary = payload / "omuse"
        binary.write_text("#!/bin/sh\n" + ("exit 5\n" if failing else
            f"if [ \"${{1:-}}\" = --self-test ]; then exit 0; fi\nprintf '%s\\n' '{version}' \"$@\"\n"))
        binary.chmod(0o755)
        for path in ("icons/omuse.svg", "icons/omuse.png", "lib/libraw.so", "models/u2netp.onnx"):
            target = payload / path
            target.parent.mkdir(exist_ok=True)
            target.write_text(version)
        return payload

    def run_manager(self, action, *args, check=True):
        result = subprocess.run(["python3", str(MANAGER), action, "--prefix", str(self.prefix), *map(str, args)],
                                text=True, capture_output=True)
        if check:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def version(self):
        return subprocess.check_output([str(self.prefix / "bin/omuse")], text=True).strip()

    def integration_state(self):
        state = {}
        paths = (*INTEGRATION_FILES, *("opt/omuse/" + name for name in
            ("current", "previous", "omuse", "omuse.previous")))
        for relative in paths:
            path = self.prefix / relative
            if path.is_symlink():
                state[relative] = ("link", os.readlink(path))
            elif path.is_file():
                state[relative] = ("file", hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mode & 0o777)
            else:
                state[relative] = ("absent",)
        return state

    def fail_integration_write(self, payload, relative, *, after_write=False, persistent=False):
        manager = load_manager()
        original = manager.atomic_file
        target = self.prefix / relative
        failures = 0

        def injected(path, content, mode=0o644):
            nonlocal failures
            if path == target and (persistent or failures == 0):
                failures += 1
                if after_write:
                    original(path, content, mode)
                raise OSError(errno.ENOSPC, "injected integration write failure")
            return original(path, content, mode)

        app = self.prefix / "opt/omuse"
        app.mkdir(parents=True, exist_ok=True)
        with mock.patch.object(manager, "atomic_file", side_effect=injected), contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaisesRegex(OSError, "injected integration write failure"):
                manager.install(argparse.Namespace(payload=payload, revision="fault-test"), self.prefix, app)
        self.assertGreater(failures, 0, "fault injection did not reach the requested integration write")

    def test_new_install_update_and_complete_rollback(self):
        self.run_manager("install", "--payload", self.payload("one"))
        self.assertEqual(self.version(), "one")
        self.assertFalse((self.prefix / "bin/compositor-rust").exists())
        self.run_manager("install", "--payload", self.payload("two"))
        self.assertEqual(self.version(), "two")
        self.run_manager("rollback")
        self.assertEqual(self.version(), "one")
        app = self.prefix / "opt/omuse"
        self.assertEqual((app / "current/lib/libraw.so").read_text(), "one")
        self.assertEqual((app / "current/models/u2netp.onnx").read_text(), "one")
        self.run_manager("rollback")
        self.assertEqual(self.version(), "two")

    def test_self_test_failure_preserves_current_and_previous(self):
        self.run_manager("install", "--payload", self.payload("one"))
        self.run_manager("install", "--payload", self.payload("two"))
        app = self.prefix / "opt/omuse"
        current, previous = os.readlink(app / "current"), os.readlink(app / "previous")
        self.run_manager("install", "--payload", self.payload("broken", failing=True), check=False)
        self.assertEqual(os.readlink(app / "current"), current)
        self.assertEqual(os.readlink(app / "previous"), previous)
        self.assertEqual(self.version(), "two")
        self.assertEqual(len(list((app / "releases").iterdir())), 2)

    def test_fresh_integration_write_failures_leave_no_advertised_installation(self):
        payload = self.payload("candidate")
        before = self.integration_state()
        for relative in INTEGRATION_FILES:
            for after_write in (False, True):
                with self.subTest(relative=relative, after_write=after_write):
                    self.fail_integration_write(payload, relative, after_write=after_write)
                    self.assertEqual(self.integration_state(), before)
                    self.assertEqual(list((self.prefix / "opt/omuse/releases").iterdir()), [])
                    self.assertEqual(list(self.prefix.rglob(".omuse-backup-*")), [])

    def test_update_integration_write_failures_preserve_both_generations_and_launchers(self):
        self.run_manager("install", "--payload", self.payload("one"))
        self.run_manager("install", "--payload", self.payload("two"))
        payload = self.payload("candidate")
        before = self.integration_state()
        generations = set((self.prefix / "opt/omuse/releases").iterdir())
        for relative in INTEGRATION_FILES:
            for after_write in (False, True):
                with self.subTest(relative=relative, after_write=after_write):
                    self.fail_integration_write(payload, relative, after_write=after_write)
                    self.assertEqual(self.integration_state(), before)
                    self.assertEqual(set((self.prefix / "opt/omuse/releases").iterdir()), generations)
                    self.assertEqual(self.version(), "two")
                    self.assertEqual(list(self.prefix.rglob(".omuse-backup-*")), [])

    def test_persistent_integration_write_failure_preserves_current_and_previous(self):
        self.run_manager("install", "--payload", self.payload("one"))
        self.run_manager("install", "--payload", self.payload("two"))
        before = self.integration_state()
        generations = set((self.prefix / "opt/omuse/releases").iterdir())
        self.fail_integration_write(self.payload("candidate"), "bin/omuse-manage", persistent=True)
        self.assertEqual(self.integration_state(), before)
        self.assertEqual(set((self.prefix / "opt/omuse/releases").iterdir()), generations)
        self.assertEqual(self.version(), "two")
        self.assertEqual(list(self.prefix.rglob(".omuse-backup-*")), [])

    def test_rollback_atomic_exchange_failure_preserves_both_links(self):
        self.run_manager("install", "--payload", self.payload("one"))
        self.run_manager("install", "--payload", self.payload("two"))
        manager = load_manager()
        app = self.prefix / "opt/omuse"
        before = self.integration_state()
        libc = mock.Mock()

        def rejected_exchange(*args):
            ctypes.set_errno(errno.EOPNOTSUPP)
            return -1

        libc.renameat2.side_effect = rejected_exchange
        with mock.patch.object(manager.ctypes, "CDLL", return_value=libc):
            with self.assertRaises(OSError) as raised:
                manager.rollback(self.prefix, app)
        self.assertEqual(raised.exception.errno, errno.EOPNOTSUPP)
        libc.renameat2.assert_called_once()
        self.assertEqual(self.integration_state(), before)
        self.assertEqual(self.version(), "two")
        self.run_manager("rollback")
        self.assertEqual(self.version(), "one")

    def test_sigterm_during_cli_integration_restores_fresh_install_and_update(self):
        # Exercise the real __main__ signal handlers while interrupting an actual
        # integration rename; the once-only hook lets restoration use os.replace.
        wrapper = '''import os
from pathlib import Path
import runpy
import signal
import sys
manager, prefix, payload = sys.argv[1:]
target = Path(prefix) / "bin/omuse-manage"
original_replace = os.replace
interrupted = False
def replace(source, destination, *args, **kwargs):
    global interrupted
    if Path(destination) == target and not interrupted:
        interrupted = True
        os.kill(os.getpid(), signal.SIGTERM)
    return original_replace(source, destination, *args, **kwargs)
os.replace = replace
sys.argv = [manager, "install", "--prefix", prefix, "--payload", payload]
runpy.run_path(manager, run_name="__main__")
'''
        for updating in (False, True):
            with self.subTest(updating=updating):
                if updating:
                    self.run_manager("install", "--payload", self.payload("one"))
                    self.run_manager("install", "--payload", self.payload("two"))
                before = self.integration_state()
                releases = self.prefix / "opt/omuse/releases"
                generations = set(releases.iterdir()) if releases.exists() else set()
                candidate = self.payload("update-candidate" if updating else "fresh-candidate")
                result = subprocess.run(
                    ["python3", "-c", wrapper, str(MANAGER), str(self.prefix), str(candidate)],
                    capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 143, result.stdout + result.stderr)
                self.assertEqual(self.integration_state(), before)
                self.assertEqual(set(releases.iterdir()), generations)
                self.assertEqual(list(self.prefix.rglob(".omuse-backup-*")), [])
                if updating:
                    self.assertEqual(self.version(), "two")

    def test_uninstall_keeps_artwork_settings_and_modified_launcher(self):
        self.run_manager("install", "--payload", self.payload("one"))
        artwork = self.prefix / "share/omuse/project.omuse"
        artwork.parent.mkdir()
        artwork.write_text("precious artwork")
        launcher = self.prefix / "bin/omuse"
        launcher.write_text("custom launcher")
        result = self.run_manager("uninstall")
        self.assertIn("Kept modified file", result.stdout)
        self.assertEqual(launcher.read_text(), "custom launcher")
        self.assertEqual(artwork.read_text(), "precious artwork")
        self.assertFalse((self.prefix / "share/applications/omuse.desktop").exists())
        self.assertFalse((self.prefix / "opt/omuse").exists())

    def test_unmodified_uninstall_removes_only_installed_files(self):
        self.run_manager("install", "--payload", self.payload("one"))
        sentinel = self.prefix / "bin/other-app"
        sentinel.write_text("other application")
        subprocess.run([str(self.prefix / "bin/omuse-manage"), "uninstall"], check=True, capture_output=True)
        self.assertTrue(sentinel.is_file())
        self.assertFalse((self.prefix / "bin/omuse").exists())

    def test_flat_install_upgrade_restores_original_binary_and_runtime(self):
        app = self.prefix / "opt/omuse"
        old = self.payload("old")
        app.mkdir(parents=True)
        shutil.copy2(old / "omuse", app / "omuse")
        shutil.copytree(old / "lib", app / "lib")
        shutil.copytree(old / "models", app / "models")
        self.run_manager("install", "--payload", self.payload("new"))
        self.run_manager("rollback")
        self.assertEqual(self.version(), "old")
        self.assertEqual((app / "current/lib/libraw.so").read_text(), "old")

    def test_no_previous_and_concurrent_install_do_not_replace_app(self):
        self.run_manager("install", "--payload", self.payload("one"))
        self.run_manager("rollback", check=False)
        with (self.prefix / "opt/.omuse-install.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            self.run_manager("install", "--payload", self.payload("two"), check=False)
        self.assertEqual(self.version(), "one")

    def test_arguments_and_desktop_entry_escape_metacharacters(self):
        self.run_manager("install", "--payload", self.payload("one"))
        result = subprocess.check_output([str(self.prefix / "bin/omuse"), "space argument", "$literal", "`literal`"], text=True)
        self.assertEqual(result.splitlines(), ["one", "space argument", "$literal", "`literal`"])
        desktop = (self.prefix / "share/applications/omuse.desktop").read_text()
        self.assertIn("%%percent", desktop)
        self.assertIn("\\$dollar", desktop)
        self.assertIn("\\`tick\\`", desktop)
        if shutil.which("desktop-file-validate"):
            subprocess.run(["desktop-file-validate", str(self.prefix / "share/applications/omuse.desktop")], check=True)

    def test_invalid_prefix_is_rejected_before_writes(self):
        self.prefix = self.base / "bad\nname"
        self.run_manager("install", "--payload", self.payload("one"), check=False)
        self.assertFalse(self.prefix.exists())


@unittest.skipIf(os.geteuid() == 0, "curl installer intentionally refuses root")
class BootstrapTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="omuse-bootstrap-test-")
        self.base = Path(self.temporary.name)
        self.source = self.base / "source"
        self.source.mkdir()
        self.published_revision = re.search(
            r"^readonly omuse_release_revision=([0-9a-f]{40})$",
            (ROOT / "install.sh").read_text(), re.MULTILINE).group(1)
        self.release_revision = self.published_revision
        self.prefix = self.base / "installed"
        for relative in ("scripts/install-rust.sh", "scripts/install-app.py", "rust/assets/omuse.png", "rust/assets/omuse.svg", "LICENSE", "rust-toolchain.toml", "rust/Cargo.toml"):
            dest = self.source / relative
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, dest)
        (self.source / "rust/licenses").mkdir()
        (self.source / "rust/licenses/test.txt").write_text("synthetic license")
        (self.source / "scripts/build-rust.sh").write_text(
            '#!/bin/bash\nset -eu\nmkdir -p "$CARGO_TARGET_DIR/release"\n'
            'printf \'#!/bin/sh\\nprintf "synthetic Omuse\\\\n"\\n\' > "$CARGO_TARGET_DIR/release/omuse"\n'
            'chmod +x "$CARGO_TARGET_DIR/release/omuse"\n')
        self.mock = self.base / "mock-bin"
        self.mock.mkdir()
        for name, body in {"rustup": "exit 0", "git": "printf '0123456789abcdef\\n'", "cc": "exit 0", "make": "exit 0", "pkg-config": "exit 0", "ffmpeg": "exit 0"}.items():
            path = self.mock / name
            path.write_text("#!/bin/sh\n" + body + "\n")
            path.chmod(0o755)
        self.env = os.environ.copy()
        for variable in ("CARGO_TARGET_DIR", "OMUSE_RUNTIME_DIR", "COMPOSITOR_RUNTIME_DIR"):
            self.env.pop(variable, None)
        self.env.update(PATH=str(self.mock) + ":" + self.env["PATH"], XDG_CACHE_HOME=str(self.base / "cache"))

    def tearDown(self):
        self.temporary.cleanup()

    def bootstrap(self, *args, use_cache=False):
        source_args = [] if use_cache else ["--source", str(self.source)]
        return subprocess.run(["bash", "-s", "--", "--no-deps", "--no-runtime-assets", *source_args, "--prefix", str(self.prefix), *args],
            input=(ROOT / "install.sh").read_text().replace(self.published_revision, self.release_revision),
            text=True, capture_output=True, env=self.env)

    def prepare_git_fixture(self):
        real_git = shutil.which("git")
        subprocess.run([real_git, "init", "--quiet", "-b", "main", str(self.source)], check=True)
        subprocess.run([real_git, "-C", str(self.source), "add", "."], check=True)
        subprocess.run([real_git, "-C", str(self.source), "-c", "user.name=Installer Test", "-c", "user.email=test@example.invalid", "commit", "--quiet", "-m", "Synthetic source"], check=True)
        self.release_revision = subprocess.check_output(
            [real_git, "-C", str(self.source), "rev-parse", "HEAD"], text=True).strip()
        # Exercise real shallow fetch/checkout locally, without network.
        (self.mock / "git").write_text(
            "#!/usr/bin/env python3\nimport subprocess,sys\n"
            f"real_git={real_git!r}\nlocal_url={self.source.as_uri()!r}\n"
            "public_url='https://github.com/Sugata-Software/Omuse.git'\n"
            "args=[local_url if a==public_url else a for a in sys.argv[1:]]\n"
            "if args[-3:]==['remote','get-url','origin']:\n"
            "    r=subprocess.run([real_git,*args],capture_output=True,text=True)\n"
            "    print(public_url if r.stdout.strip()==local_url else r.stdout.strip())\n"
            "else:\n"
            "    r=subprocess.run([real_git,*args])\n"
            "sys.exit(r.returncode)\n")

    def test_downloaded_source_uses_stable_checkout_for_cached_updates(self):
        self.prepare_git_fixture()
        first = self.bootstrap(use_cache=True)
        self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
        checkout = self.base / "cache/omuse/installer/source"
        manifest = checkout / "rust/Cargo.toml"
        before = manifest.stat().st_mtime_ns
        second = self.bootstrap(use_cache=True)
        self.assertEqual(second.returncode, 0, second.stdout + second.stderr)
        self.assertEqual(manifest.stat().st_mtime_ns, before)
        self.assertTrue((checkout / ".git/omuse-installer").exists())

    def test_modified_cached_source_is_not_overwritten_on_update(self):
        self.prepare_git_fixture()
        first = self.bootstrap(use_cache=True)
        self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
        checkout = self.base / "cache/omuse/installer/source"
        manifest = checkout / "rust/Cargo.toml"
        manifest.write_text(manifest.read_text() + "\n# user's local edit\n")
        current = os.readlink(self.prefix / "opt/omuse/current")
        result = self.bootstrap(use_cache=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("source cache has local edits", result.stderr + result.stdout)
        self.assertIn("user's local edit", manifest.read_text())
        self.assertEqual(os.readlink(self.prefix / "opt/omuse/current"), current)

    def test_newer_main_does_not_replace_the_tested_source_revision(self):
        self.prepare_git_fixture()
        real_git = shutil.which("git")
        manifest = self.source / "rust/Cargo.toml"
        manifest.write_text(manifest.read_text() + "\n# unqualified main change\n")
        subprocess.run([real_git, "-C", str(self.source), "add", "."], check=True)
        subprocess.run([real_git, "-C", str(self.source), "-c", "user.name=Installer Test",
                        "-c", "user.email=test@example.invalid", "commit", "--quiet", "-m", "Unqualified change"], check=True)
        for _ in range(2):
            result = self.bootstrap(use_cache=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            checkout = self.base / "cache/omuse/installer/source"
            self.assertEqual(subprocess.check_output(
                [real_git, "-C", str(checkout), "rev-parse", "HEAD"], text=True).strip(), self.release_revision)
            self.assertNotIn("unqualified main change", (checkout / "rust/Cargo.toml").read_text())
            receipt = (self.prefix / "opt/omuse/current/SOURCE-REVISION").read_text()
            self.assertIn(f"source_revision={self.release_revision}\n", receipt)

    def test_unavailable_source_revision_preserves_the_installed_app(self):
        self.prepare_git_fixture()
        first = self.bootstrap(use_cache=True)
        self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
        current = os.readlink(self.prefix / "opt/omuse/current")
        self.release_revision = "0" * 40
        result = self.bootstrap(use_cache=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(os.readlink(self.prefix / "opt/omuse/current"), current)
        self.assertEqual(list((self.base / "cache/omuse/installer").glob("run.*")), [])

    def commit_fixture_update(self, body):
        real_git = shutil.which("git")
        (self.source / "scripts/build-rust.sh").write_text(body)
        subprocess.run([real_git, "-C", str(self.source), "add", "."], check=True)
        subprocess.run([real_git, "-C", str(self.source), "-c", "user.name=Installer Test",
                        "-c", "user.email=test@example.invalid", "commit", "--quiet", "-m", "Next tested version"], check=True)
        self.release_revision = subprocess.check_output(
            [real_git, "-C", str(self.source), "rev-parse", "HEAD"], text=True).strip()

    def installed_state(self):
        state = {}
        for path in self.prefix.rglob("*"):
            relative = str(path.relative_to(self.prefix))
            if path.is_symlink():
                state[relative] = ("link", os.readlink(path))
            elif path.is_file():
                state[relative] = ("file", hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mode & 0o777)
        return state

    def test_cached_upgrade_advances_to_new_tested_revision_and_retains_rollback(self):
        self.prepare_git_fixture()
        first_revision = self.release_revision
        first = self.bootstrap(use_cache=True)
        self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
        first_generation = (self.prefix / "opt/omuse/current").resolve()
        self.commit_fixture_update((self.source / "scripts/build-rust.sh").read_text().replace(
            "synthetic Omuse", "updated Omuse"))
        result = self.bootstrap(use_cache=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        checkout = self.base / "cache/omuse/installer/source"
        self.assertEqual(subprocess.check_output(
            [shutil.which("git"), "-C", str(checkout), "rev-parse", "HEAD"], text=True).strip(), self.release_revision)
        app = self.prefix / "opt/omuse"
        self.assertIn(f"source_revision={self.release_revision}\n", (app / "current/SOURCE-REVISION").read_text())
        self.assertEqual((app / "previous").resolve(), first_generation)
        self.assertIn(f"source_revision={first_revision}\n", (app / "previous/SOURCE-REVISION").read_text())
        self.assertEqual(subprocess.check_output([str(self.prefix / "bin/omuse")], text=True).strip(), "updated Omuse")
        rollback = self.bootstrap("--rollback")
        self.assertEqual(rollback.returncode, 0, rollback.stdout + rollback.stderr)
        self.assertEqual(subprocess.check_output([str(self.prefix / "bin/omuse")], text=True).strip(), "synthetic Omuse")

    def assert_failed_cached_upgrade_preserves_install(self, *, self_test):
        self.prepare_git_fixture()
        first = self.bootstrap(use_cache=True)
        self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
        before = self.installed_state()
        body = '#!/bin/bash\nset -eu\nexit 42\n'
        if self_test:
            body = ('#!/bin/bash\nset -eu\nmkdir -p "$CARGO_TARGET_DIR/release"\n'
                    'printf \'#!/bin/sh\\nexit 42\\n\' > "$CARGO_TARGET_DIR/release/omuse"\n'
                    'chmod +x "$CARGO_TARGET_DIR/release/omuse"\n')
        self.commit_fixture_update(body)
        result = self.bootstrap(use_cache=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.installed_state(), before)
        self.assertEqual(subprocess.check_output([str(self.prefix / "bin/omuse")], text=True).strip(), "synthetic Omuse")
        self.assertEqual(list((self.base / "cache/omuse/installer").glob("run.*")), [])

    def test_cached_upgrade_build_failure_preserves_complete_install(self):
        self.assert_failed_cached_upgrade_preserves_install(self_test=False)

    def test_cached_upgrade_self_test_failure_preserves_complete_install(self):
        self.assert_failed_cached_upgrade_preserves_install(self_test=True)

    def test_piped_install_and_uninstall_from_unrelated_directory(self):
        result = self.bootstrap()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((self.prefix / "bin/omuse").exists())
        self.assertIn("Open Omuse", result.stdout)
        self.assertEqual(list((self.base / "cache/omuse/installer").glob("run.*")), [])
        result = self.bootstrap("--uninstall")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse((self.prefix / "bin/omuse").exists())

    def test_failed_build_does_not_activate_and_cleans_download_directory(self):
        (self.source / "scripts/build-rust.sh").write_text("#!/bin/sh\nexit 42\n")
        result = self.bootstrap()
        self.assertEqual(result.returncode, 42, result.stdout + result.stderr)
        self.assertFalse((self.prefix / "bin/omuse").exists())
        self.assertIn("installation stopped", result.stdout + result.stderr)
        self.assertEqual(list((self.base / "cache/omuse/installer").glob("run.*")), [])

    def test_matching_system_rust_is_used_without_rustup_or_downloads(self):
        toolchain = tomllib.loads((self.source / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
        for name in ("cargo", "rustc"):
            command = self.mock / name
            command.write_text(f"#!/bin/sh\nprintf '%s\\n' '{name} {toolchain} (synthetic system toolchain)'\n")
            command.chmod(0o755)
        for name in ("rustup", "curl"):
            command = self.mock / name
            command.write_text("#!/bin/sh\nexit 77\n")
            command.chmod(0o755)
        result = self.bootstrap()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Using the existing Rust", result.stdout)
        self.assertTrue((self.prefix / "bin/omuse").is_file())

    def test_selected_rustup_toolchain_precedes_distribution_cargo(self):
        selected = self.base / "selected toolchain/bin"
        selected.mkdir(parents=True)
        for name in ("cargo", "rustc"):
            for directory in (self.mock, selected):
                command = directory / name
                command.write_text(f"#!/bin/sh\nprintf '%s\\n' '{name} 0.0.0 (synthetic toolchain)'\n")
                command.chmod(0o755)
        (self.mock / "rustup").write_text(
            "#!/bin/sh\nif [ \"$1\" = which ]; then printf '%s\\n' "
            + shlex.quote(str(selected / "cargo")) + "; fi\n")
        build = self.source / "scripts/build-rust.sh"
        build.write_text(
            '#!/bin/bash\nset -eu\n'
            + '\n'.join(f'[ "$(command -v {name})" = {shlex.quote(str(selected / name))} ]'
                        for name in ("cargo", "rustc"))
            + '\n' + build.read_text())
        result = self.bootstrap()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((self.prefix / "bin/omuse").is_file())

    def test_bad_jobs_rejected_without_cache_or_installation(self):
        result = self.bootstrap("--jobs", "0")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.prefix.exists())
        self.assertFalse((self.base / "cache").exists())


if __name__ == "__main__":
    unittest.main()
