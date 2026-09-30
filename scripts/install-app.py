#!/usr/bin/env python3
"""Install complete per-user Omuse generations; switch only after validation."""

from __future__ import annotations

import argparse
import contextlib
import ctypes
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import uuid


MARKER = ".omuse-generation.json"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def atomic_file(path: Path, content: bytes, mode: int = 0o644) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".omuse-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as output:
            output.write(content)
            output.flush()
            os.fsync(output.fileno())
        os.chmod(temporary, mode)
        os.replace(temporary, path)
    finally:
        with contextlib.suppress(FileNotFoundError):
            os.unlink(temporary)


def atomic_link(path: Path, target: str) -> None:
    temporary = path.with_name(".omuse-link-" + uuid.uuid4().hex)
    try:
        temporary.symlink_to(target)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def exchange_links(first: Path, second: Path) -> None:
    """Linux swaps both symlinks atomically, including across process interruption."""
    libc = ctypes.CDLL(None, use_errno=True)
    exchange = libc.renameat2
    exchange.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    exchange.restype = ctypes.c_int
    if exchange(-100, os.fsencode(first), -100, os.fsencode(second), 2) != 0:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code))


def generation(app: Path, name: str) -> Path | None:
    link = app / name
    if not link.is_symlink():
        return None
    target = os.readlink(link)
    parts = Path(target).parts
    if len(parts) != 2 or parts[0] != "releases" or not parts[1].startswith("install-"):
        raise ValueError(f"Unrecognized Omuse {name} link; leaving it untouched")
    result = app / target
    if result.is_symlink() or not (result / MARKER).is_file():
        raise ValueError(f"Incomplete Omuse {name} generation; leaving it untouched")
    return result


def verify(candidate: Path) -> None:
    # Never open the user's artwork, recovery directory or provider connections.
    with tempfile.TemporaryDirectory(prefix="omuse-install-check-") as scratch:
        env = os.environ.copy()
        for key in list(env):
            if key.startswith(("OMUSE_", "COMPOSITOR_")):
                del env[key]
        for variable, directory in (("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "state")):
            env[variable] = str(Path(scratch) / directory)
        subprocess.run([str(candidate / "omuse"), "--self-test", str(Path(scratch) / "evidence")],
                       cwd=scratch, env=env, check=True, timeout=180,
                       stdout=subprocess.DEVNULL)


def desktop_entry(launcher: Path) -> bytes:
    escaped = str(launcher).replace("\\", "\\\\")
    for character in ('"', '`', '$'):
        escaped = escaped.replace(character, "\\" + character)
    escaped = escaped.replace("%", "%%")
    return ("[Desktop Entry]\nType=Application\nName=Omuse\n"
            "Comment=Native image editor and content studio\n"
            f'Exec="{escaped}" %f\nIcon=omuse\nTerminal=false\n'
            "Categories=Graphics;2DGraphics;RasterGraphics;\n"
            "MimeType=image/png;image/jpeg;image/webp;image/tiff;image/bmp;image/gif;"
            "image/vnd.adobe.photoshop;image/x-photoshop;image/svg+xml;image/svg+xml-compressed;\n"
            "StartupNotify=true\nStartupWMClass=omuse\n").encode()


def refresh_desktop(prefix: Path) -> None:
    if shutil.which("update-desktop-database"):
        subprocess.run(["update-desktop-database", str(prefix / "share/applications")],
                       check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def install(args: argparse.Namespace, prefix: Path, app: Path) -> None:
    payload = args.payload.resolve()
    if not (payload / "omuse").is_file():
        raise ValueError("Payload does not contain an Omuse executable")
    for icon in ("omuse.svg", "omuse.png"):
        if not (payload / "icons" / icon).is_file():
            raise ValueError(f"Payload is missing {icon}")
    old = generation(app, "current")
    legacy = app.parent / "compositor-rust"
    old_flat_binary = app / "omuse"
    if not old_flat_binary.is_file() and (legacy / "compositor-rust").is_file():
        old_flat_binary = legacy / "compositor-rust"
    releases = app / "releases"
    releases.mkdir(exist_ok=True)
    candidate = Path(tempfile.mkdtemp(prefix="install-", dir=releases))
    activated = False
    backups = {}

    def remember(path):
        if path in backups:
            return
        if path.exists() and not (path.is_file() or path.is_symlink()):
            raise ValueError(f"Refusing to replace a directory: {path}")
        if path.is_symlink() or path.is_file():
            # Retain the original inode so restoring after a full-disk failure
            # needs no new file contents, permissions changes or Python writer.
            backup = path.with_name(".omuse-backup-" + uuid.uuid4().hex)
            os.link(path, backup, follow_symlinks=False)
            backups[path] = backup
        else:
            backups[path] = None

    def link(path, target):
        remember(path)
        atomic_link(path, target)

    def write(path, data, mode=0o644):
        remember(path)
        atomic_file(path, data, mode)
    try:
        shutil.copytree(payload, candidate, dirs_exist_ok=True)
        os.chmod(candidate / "omuse", 0o755)
        (candidate / MARKER).write_text(json.dumps({"schema": 1, "source": args.revision}) + "\n")
        verify(candidate)
        if old is None and old_flat_binary.is_file() and not old_flat_binary.is_symlink():
            # Preserve an earlier flat installation with its own runtime assets.
            old = Path(tempfile.mkdtemp(prefix="install-", dir=releases))
            shutil.copy2(old_flat_binary, old / "omuse")
            for item in ("lib", "models", "licenses", "SOURCE-REVISION"):
                source = old_flat_binary.parent / item
                if source.is_dir():
                    shutil.copytree(source, old / item)
                elif source.is_file():
                    shutil.copy2(source, old / item)
            (old / MARKER).write_text('{"schema":1,"source":"previous-flat-install"}\n')
            # Existing launchers remain usable while desktop integration is prepared.
            link(app / "current", str(old.relative_to(app)))
        entries = {
            "bin/omuse": (f'#!/bin/sh\nset -eu\nexec {shlex.quote(str(app / "current/omuse"))} "$@"\n'.encode(), 0o755),
            "bin/omuse-manage": (f'#!/bin/sh\nset -eu\nexec python3 {shlex.quote(str(app / "manage.py"))} "$@" --prefix {shlex.quote(str(prefix))}\n'.encode(), 0o755),
            "share/applications/omuse.desktop": (desktop_entry(prefix / "bin/omuse"), 0o644),
            "share/icons/hicolor/256x256/apps/omuse.png": ((payload / "icons/omuse.png").read_bytes(), 0o644),
            "share/icons/hicolor/scalable/apps/omuse.svg": ((payload / "icons/omuse.svg").read_bytes(), 0o644),
        }
        # Only an actual previous Rust installation gets its old command bridged.
        compatibility = prefix / "bin/compositor-rust"
        if old_flat_binary == legacy / "compositor-rust" and not compatibility.exists() and not compatibility.is_symlink():
            entries["bin/compositor-rust"] = (f'#!/bin/sh\nexec {shlex.quote(str(prefix / "bin/omuse"))} "$@"\n'.encode(), 0o755)
        old_desktop = prefix / "share/applications/compositor-rust.desktop"
        if old_desktop.is_file() and not old_desktop.is_symlink():
            text = old_desktop.read_text()
            if all(value in text.splitlines() for value in ("Name=Compositor Rust", "Icon=compositor-rust", "StartupWMClass=compositor-rust")):
                retired = old_desktop.with_name(old_desktop.name + ".retired-by-omuse")
                if not retired.exists():
                    write(retired, old_desktop.read_bytes())
                remember(old_desktop)
                old_desktop.unlink()
        if old:
            link(app / "previous", str(old.relative_to(app)))
            link(app / "omuse.previous", "previous/omuse")
        link(app / "omuse", "current/omuse")
        write(app / "manage.py", Path(__file__).read_bytes(), 0o755)
        owned = {}
        existing_manifest = app / "installed-files.json"
        if existing_manifest.is_file():
            previous_owned = json.loads(existing_manifest.read_text())
            compatibility_name = "bin/compositor-rust"
            if isinstance(previous_owned, dict) and compatibility_name in previous_owned \
                and compatibility.is_file() and not compatibility.is_symlink() \
                and digest(compatibility) == previous_owned[compatibility_name]:
                owned[compatibility_name] = previous_owned[compatibility_name]
        for relative, (data, mode) in entries.items():
            write(prefix / relative, data, mode)
            owned[relative] = hashlib.sha256(data).hexdigest()
        write(app / "installed-files.json", (json.dumps(owned, indent=2) + "\n").encode())
        # All validation and integration writes precede the single activation step.
        old_mask = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM, signal.SIGHUP, signal.SIGINT})
        try:
            atomic_link(app / "current", str(candidate.relative_to(app)))
            activated = True
        finally:
            signal.pthread_sigmask(signal.SIG_SETMASK, old_mask)
        # Old user-created launchers are not overwritten; fresh installs have only Omuse.
        refresh_desktop(prefix)
        print(f"Installed Omuse: {prefix / 'bin/omuse'}")
        if old:
            print(f"Previous complete installation retained. Rollback: {prefix / 'bin/omuse-manage'} rollback")
    finally:
        if not activated:
            for path, backup in reversed(backups.items()):
                try:
                    if backup is None:
                        path.unlink(missing_ok=True)
                    else:
                        os.replace(backup, path)
                        # Renaming two hard links to the same inode is a no-op.
                        backup.unlink(missing_ok=True)
                except OSError as error:
                    # Restore independent paths even if one directory changed
                    # permissions; keep any surviving backup for recovery.
                    print(f"Could not restore {path}: {error}; backup: {backup}", file=sys.stderr)
            shutil.rmtree(candidate)
        else:
            for backup in backups.values():
                if backup is not None:
                    backup.unlink(missing_ok=True)


def rollback(prefix: Path, app: Path) -> None:
    current, previous = generation(app, "current"), generation(app, "previous")
    if not current or not previous:
        raise ValueError("No previous Omuse installation is available")
    verify(previous)
    exchange_links(app / "current", app / "previous")
    print("Restored the previous complete Omuse installation. Reopen Omuse to use it.")


def uninstall(prefix: Path, app: Path) -> None:
    manifest = app / "installed-files.json"
    if not manifest.is_file() or not generation(app, "current"):
        raise ValueError("No managed Omuse installation found")
    owned = json.loads(manifest.read_text())
    allowed = {"bin/omuse", "bin/omuse-manage", "bin/compositor-rust", "share/applications/omuse.desktop",
               "share/icons/hicolor/256x256/apps/omuse.png", "share/icons/hicolor/scalable/apps/omuse.svg"}
    if not isinstance(owned, dict) or not set(owned).issubset(allowed):
        raise ValueError("Unrecognized installation manifest; leaving files untouched")
    for relative, expected in owned.items():
        path = prefix / relative
        if path.is_file() and not path.is_symlink() and digest(path) == expected:
            path.unlink()
        elif path.exists() or path.is_symlink():
            print(f"Kept modified file: {path}")
    for entry in (app / "releases").iterdir():
        if entry.name.startswith("install-") and not entry.is_symlink() and (entry / MARKER).is_file():
            shutil.rmtree(entry)
    for name in ("current", "previous", "omuse", "omuse.previous"):
        path = app / name
        if path.is_symlink():
            path.unlink()
    manifest.unlink()
    (app / "manage.py").unlink(missing_ok=True)
    with contextlib.suppress(OSError):
        (app / "releases").rmdir()
        app.rmdir()
    refresh_desktop(prefix)
    print("Omuse uninstalled. Projects, settings, recovery and build cache are kept.")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("install", "rollback", "uninstall"))
    parser.add_argument("--prefix", type=Path, default=Path.home() / ".local")
    parser.add_argument("--payload", type=Path)
    parser.add_argument("--revision", default="local-build")
    args = parser.parse_args()
    prefix = args.prefix
    if not prefix.is_absolute() or prefix == Path("/") or any(ord(c) < 32 or ord(c) == 127 or c in "'\\" for c in str(prefix)):
        parser.error("prefix must be an absolute directory without control characters, single quotes or backslashes")
    if args.action == "install" and args.payload is None:
        parser.error("install requires --payload")
    app = prefix / "opt/omuse"
    if args.action != "install" and not app.is_dir():
        parser.error("No managed Omuse installation found")
    app.parent.mkdir(parents=True, exist_ok=True)
    with (app.parent / ".omuse-install.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        app.mkdir(exist_ok=True)
        if args.action == "install":
            install(args, prefix, app)
        elif args.action == "rollback":
            rollback(prefix, app)
        else:
            uninstall(prefix, app)


if __name__ == "__main__":
    def interrupted(number, _frame):
        raise SystemExit(128 + number)

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGHUP, interrupted)
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        sys.exit(f"Omuse: {error}")
