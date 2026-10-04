#!/usr/bin/env python3
"""Verify a release archive and optionally smoke-test it in disposable directories."""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import zipfile


SHA256 = re.compile(r"[0-9a-f]{64}\Z")
SOURCE = re.compile(r"[0-9a-f]{40}\Z")
VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z")
TARGETS = {"linux-x86_64", "windows-x86_64"}
MAX_ARCHIVE = 1024 ** 3
MAX_EXPANDED = 2 * 1024 ** 3
MAX_MEMBERS = 1000
ROOT = Path(__file__).resolve().parent.parent


class BundleError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise BundleError(message)


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def parse_json(text):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, f"Duplicate JSON key: {key}")
            result[key] = value
        return result
    try:
        return json.loads(text, object_pairs_hook=unique)
    except (ValueError, UnicodeError) as error:
        raise BundleError("Malformed JSON in archive or receipt") from error


def member_parts(name):
    require(isinstance(name, str) and name and len(name) < 1024, "Invalid archive member name")
    require(not any(ord(c) < 32 or ord(c) == 127 or c in "\\:" for c in name), "Unsafe archive member name")
    require(not name.startswith("/"), "Absolute archive path")
    parts = name.rstrip("/").split("/")
    require(all(part and part not in (".", "..") and not part.endswith((".", " ")) for part in parts), "Unsafe archive path segment")
    require(all(part.split(".")[0].upper() not in {
        "CON", "PRN", "AUX", "NUL", *(f"COM{i}" for i in range(1, 10)), *(f"LPT{i}" for i in range(1, 10)),
    } for part in parts), "Windows device name in archive")
    return tuple(parts)


@contextlib.contextmanager
def archive_members(path, target, source):
    require(Path(path).is_file() and not Path(path).is_symlink(), "Archive must be a regular file")
    require(0 < Path(path).stat().st_size <= MAX_ARCHIVE, "Archive exceeds the compressed-size limit")
    expected_root = "omuse-bundle" if target == "linux-x86_64" else f"omuse-{source[:12]}-windows-x86_64"
    archive = tarfile.open(path, "r:gz") if target == "linux-x86_64" else zipfile.ZipFile(path)
    try:
        entries = archive if target == "linux-x86_64" else archive.infolist()
        files = {}
        names = set()
        expanded = 0
        for count, entry in enumerate(entries, 1):
            require(count <= MAX_MEMBERS, "Archive has too many members")
            if target == "linux-x86_64":
                name, size, directory = entry.name, entry.size, entry.isdir()
                require(entry.type in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE), "Archive contains a link or special file")
            else:
                name, size, directory = entry.filename, entry.file_size, entry.is_dir()
                mode = stat.S_IFMT(entry.external_attr >> 16)
                require(mode in (0, stat.S_IFDIR if directory else stat.S_IFREG), "Zip contains a link or special file")
                require(not entry.flag_bits & 1, "Encrypted zip members are unsupported")
            parts = member_parts(name)
            require(parts[0] == expected_root, "Archive has an unexpected root folder")
            folded = "/".join(parts).casefold()
            require(folded not in names, "Duplicate or case-aliased archive member")
            names.add(folded)
            if directory:
                continue
            require(len(parts) > 1 and 0 <= size <= MAX_EXPANDED, "Invalid payload member")
            expanded += size
            require(expanded <= MAX_EXPANDED, "Archive exceeds the expanded-size limit")
            files["/".join(parts[1:])] = entry
        require(files, "Archive has no files")
        folded_files = {name.casefold() for name in files}
        for name in files:
            for parent in PurePosixPath(name).parents:
                if str(parent) != ".":
                    require(str(parent).casefold() not in folded_files, "Archive file shadows a parent directory")

        def opened(name):
            require(name in files, f"Missing bundle member: {name}")
            stream = archive.extractfile(files[name]) if target == "linux-x86_64" else archive.open(files[name])
            require(stream is not None, f"Cannot read bundle member: {name}")
            return stream

        yield files, opened
    finally:
        archive.close()


def bounded_read(opened, name, limit):
    with opened(name) as stream:
        content = stream.read(limit + 1)
    require(len(content) <= limit, f"Bundle metadata is too large: {name}")
    return content


def inspect_archive(path, target, source, version):
    require(target in TARGETS and SOURCE.fullmatch(source) and VERSION.fullmatch(version), "Invalid source, target or version")
    binary = "bin/omuse" if target == "linux-x86_64" else "omuse.exe"
    extension = "so" if target == "linux-x86_64" else "dll"
    with archive_members(path, target, source) as (files, opened):
        required = {
            binary, "SOURCE-REVISION", "SHA256SUMS", "models/u2netp.onnx",
            f"lib/libraw.{extension}", f"lib/{'lib' if extension == 'so' else ''}onnxruntime.{extension}",
            "licenses/Omuse-MIT.txt", "licenses/rust-dependency-inventory.json", "licenses/Rust-THIRD-PARTY-NOTICES.txt",
        }
        if target == "linux-x86_64":
            required.update({"install.sh", "install-app.py", "share/icons/omuse.png", "share/icons/omuse.svg"})
        else:
            required.add("README.txt")
        require(required <= files.keys(), f"Required bundle members are missing: {sorted(required - files.keys())}")
        checksums = {}
        for line in bounded_read(opened, "SHA256SUMS", 1024 * 1024).decode("utf-8").splitlines():
            match = re.fullmatch(r"([0-9a-f]{64})  \./(.+)", line)
            require(match is not None, "Malformed bundle checksum line")
            name = match[2]
            member_parts(name)
            require(name != "SHA256SUMS" and name not in checksums, "Duplicate or self-referential bundle checksum")
            checksums[name] = match[1]
        require(set(checksums) == set(files) - {"SHA256SUMS"}, "Checksums do not cover exactly the archive payload")
        for name, expected in checksums.items():
            with opened(name) as stream:
                actual = hashlib.file_digest(stream, "sha256").hexdigest()
            require(actual == expected, f"Bundle checksum mismatch: {name}")
        revision = {}
        for line in bounded_read(opened, "SOURCE-REVISION", 4096).decode("utf-8").splitlines():
            key, separator, value = line.partition("=")
            require(separator and key not in revision, "Malformed source revision record")
            revision[key] = value
        require(revision == {"source_revision": source, "source_tree_dirty": "false", "target": target, "bundle_kind": "full-feature"}, "Bundle source identity or clean-tree evidence differs")
        inventory = parse_json(bounded_read(opened, "licenses/rust-dependency-inventory.json", 16 * 1024 * 1024))
        expected_target = "x86_64-unknown-linux-gnu" if target == "linux-x86_64" else "x86_64-pc-windows-msvc"
        packages = inventory.get("packages")
        require(inventory.get("schemaVersion") == 1 and inventory.get("target") == expected_target, "Notice inventory has the wrong schema or target")
        require(isinstance(packages, list) and packages and type(inventory.get("packageCount")) is int
                and inventory["packageCount"] == len(packages), "Notice inventory is empty or incomplete")
        require(inventory.get("reviewFindings") == [], "Notice inventory has unresolved findings")
        for package in packages:
            require(isinstance(package, dict) and all(isinstance(package.get(key), str) and package[key].strip()
                    for key in ("name", "version", "licenseExpression"))
                    and isinstance(package.get("licenseFiles"), list) and package["licenseFiles"]
                    and all(isinstance(path, str) and path for path in package["licenseFiles"])
                    and package.get("reviewFindings") == [], "A dependency has incomplete legal metadata")
        require(len(bounded_read(opened, "licenses/Rust-THIRD-PARTY-NOTICES.txt", 16 * 1024 * 1024)) > 100, "Collected dependency notices are empty")
    return {
        "schemaVersion": 1, "target": target, "version": version, "sourceRevision": source,
        "fileName": Path(path).name, "sha256": digest(path), "size": Path(path).stat().st_size,
        "binarySha256": checksums[binary], "inventorySha256": checksums["licenses/rust-dependency-inventory.json"],
        "dependencyCount": len(packages), "noticeFindings": 0,
    }


def extract_verified(path, target, source, destination):
    require(not destination.exists(), "Smoke extraction destination must be new")
    destination.mkdir(parents=True)
    with archive_members(path, target, source) as (files, opened):
        for name in sorted(files):
            output = destination.joinpath(*PurePosixPath(name).parts)
            output.parent.mkdir(parents=True, exist_ok=True)
            with opened(name) as payload, output.open("xb") as stream:
                shutil.copyfileobj(payload, stream, 1024 * 1024)
            if target == "linux-x86_64":
                output.chmod(0o755 if files[name].mode & 0o111 else 0o644)


def run(command, directory, env, log):
    result = subprocess.run([str(value) for value in command], cwd=directory, env=env,
                            capture_output=True, encoding="utf-8", errors="replace", timeout=240)
    log.write_text(result.stdout + result.stderr, encoding="utf-8")
    require(result.returncode == 0, f"Smoke command failed; see {log}")
    return result.stdout


def smoke_archive(archive, record, evidence):
    target, source, version = (record[key] for key in ("target", "sourceRevision", "version"))
    require((sys.platform == "win32") == (target == "windows-x86_64"), "Smoke tests must run on the native target host")
    require(sys.platform == "win32" or sys.platform.startswith("linux"), "Unsupported smoke host")
    require(not evidence.exists(), "Use a fresh smoke evidence directory")
    evidence.mkdir(parents=True)
    env = os.environ.copy()
    for key in list(env):
        if key.startswith(("OMUSE_", "COMPOSITOR_")):
            del env[key]
    state = evidence / "disposable-profile"
    sentinels = {}
    for variable in ("XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME", "APPDATA", "LOCALAPPDATA"):
        directory = state / variable.lower()
        directory.mkdir(parents=True)
        env[variable] = str(directory)
        sentinel = directory / "omuse" / "existing-profile-sentinel.txt"
        sentinel.parent.mkdir()
        sentinel.write_text("Retain unrelated existing profile data.\n")
        sentinels[sentinel] = digest(sentinel)
    extracted = evidence / "unpacked"
    extract_verified(archive, target, source, extracted)
    binary = extracted / ("bin/omuse" if target == "linux-x86_64" else "omuse.exe")
    require(run([binary, "--version"], evidence, env, evidence / "version.log").strip() == f"Omuse {version}", "Executable version differs from archive declaration")
    run([binary, "--self-test", evidence / "editing-journey"], evidence, env, evidence / "editing.log")
    journey = parse_json((evidence / "editing-journey" / "results.json").read_text())
    require(journey.get("status") == "passed", "Packaged editing journey did not pass")

    # Load the bundled libraries in a child process, so Windows never leaves a
    # DLL handle open while the portable upgrade/rollback directories are moved.
    libraries = [extracted / "lib" / name for name in
                 (("libraw.so", "libonnxruntime.so") if target == "linux-x86_64" else ("libraw.dll", "onnxruntime.dll"))]
    run([sys.executable, "-c", "import ctypes,sys; [ctypes.CDLL(p) for p in sys.argv[1:]]", *libraries], evidence, env, evidence / "runtime-load.log")
    if target == "linux-x86_64":
        prefix = evidence / "installed prefix"
        install = ["sh", extracted / "install.sh", archive, "--prefix", prefix]
        run(install, evidence, env, evidence / "install.log")
        launcher = prefix / "bin" / "omuse"
        first = (prefix / "opt" / "omuse" / "current").resolve()
        require(digest(first / "omuse") == record["binarySha256"], "Installed binary differs")
        run(install, evidence, env, evidence / "upgrade.log")
        current = (prefix / "opt" / "omuse" / "current").resolve()
        previous = (prefix / "opt" / "omuse" / "previous").resolve()
        require(current != first and previous == first, "Upgrade did not preserve a complete prior generation")
        require(run([launcher, "--version"], evidence, env, evidence / "installed-version.log").strip()
                == f"Omuse {version}", "Installed launcher reports a different version")
        run([prefix / "bin" / "omuse-manage", "rollback"], evidence, env, evidence / "rollback.log")
        require((prefix / "opt" / "omuse" / "current").resolve() == first, "Rollback did not restore the prior generation")
        require(digest(first / "omuse") == record["binarySha256"], "Rollback altered the binary")
        install_mode = "per-user generation installer"
    else:
        portable = evidence / "portable"
        current, previous, staged = portable / "current", portable / "previous", portable / "upgrade"
        extract_verified(archive, target, source, current)
        extract_verified(archive, target, source, staged)
        current.rename(previous)
        staged.rename(current)
        require(digest(previous / "omuse.exe") == digest(current / "omuse.exe") == record["binarySha256"], "Portable upgrade did not preserve both complete versions")
        run([current / "omuse.exe", "--self-test", evidence / "upgraded-journey"], evidence, env, evidence / "upgrade.log")
        upgraded = parse_json((evidence / "upgraded-journey" / "results.json").read_text())
        require(upgraded.get("status") == "passed", "Upgraded portable editing journey did not pass")
        current.rename(staged)
        previous.rename(current)
        require(run([current / "omuse.exe", "--version"], evidence, env, evidence / "rollback.log").strip()
                == f"Omuse {version}", "Portable rollback reports a different version")
        require(digest(current / "omuse.exe") == record["binarySha256"], "Portable rollback altered the executable")
        install_mode = "portable side-by-side folders"
    require(all(path.is_file() and digest(path) == expected for path, expected in sentinels.items()), "Smoke test altered unrelated profile data")
    return dict(record, status="passed", checks=["archive-integrity", "zero-notice-findings", "native-version", "packaged-editing-journey", "runtime-library-load", "install", "upgrade", "rollback", "profile-preservation"],
                installMode=install_mode, upgradeBaseline="same-candidate-reinstall",
                limitations=["Automated headless package checks; native interactive desktop and live AI operations are not qualified.",
                             "Upgrade checks exercise complete generation replacement using the same candidate, not migration from an older published version."])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--target", choices=sorted(TARGETS), required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--smoke", type=Path, help="run native smoke checks in this new disposable directory")
    args = parser.parse_args()
    try:
        record = inspect_archive(args.archive.resolve(), args.target, args.source, args.version)
        if args.smoke:
            record = smoke_archive(args.archive.resolve(), record, args.smoke.resolve())
        require(not args.output.exists(), "Receipt output already exists")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(f"Verified {record['fileName']}: {record['sha256']}")
        return 0
    except (BundleError, OSError, ValueError, KeyError, tarfile.TarError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
        print(f"Release bundle: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
