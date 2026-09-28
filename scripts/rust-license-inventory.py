#!/usr/bin/env python3
"""Create an offline inventory of legal metadata for the locked Rust graph.

This is evidence for release review and bundle notices, not legal advice or a
license-compliance certification. It never downloads crates or builds code.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib


LEGAL_NAMES = ("license", "licence", "copying", "notice", "copyright")
MAX_LEGAL_FILE_BYTES = 4 * 1024 * 1024
SHA256_RE = re.compile(r"[0-9a-f]{64}")
REVISION_RE = re.compile(r"[0-9a-f]{40}")


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new directory for inventory and notices")
    parser.add_argument(
        "--target",
        default="x86_64-unknown-linux-gnu",
        help="Cargo filter platform (default: %(default)s)",
    )
    return parser.parse_args()


def safe_component(value: str) -> str:
    clean = re.sub(r"[^A-Za-z0-9._+-]+", "-", value).strip("-.")
    return clean or "package"


def git_checkout_root(path: Path) -> Path | None:
    parts = path.resolve().parts
    try:
        index = parts.index("checkouts")
    except ValueError:
        return None
    # Cargo git checkouts are .../git/checkouts/repository-hash/revision/...
    if len(parts) <= index + 2:
        return None
    return Path(*parts[: index + 3])


def within(path: Path, roots: list[Path]) -> bool:
    resolved = path.resolve()
    return any(resolved == root or root in resolved.parents for root in roots)


def file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def override_error(detail: str) -> ValueError:
    return ValueError(f"Invalid dependency license override: {detail}")


def canonical_repository_url(value: object) -> str | None:
    if not isinstance(value, str) or not value.startswith("https://"):
        return None
    normalized = value.rstrip("/")
    for marker in ("/tree/", "/blob/", "/-/tree/", "/-/blob/"):
        normalized = normalized.split(marker, 1)[0]
    return normalized.removesuffix(".git")


def validate_override_origin(package_dir: Path, override: dict) -> None:
    """Confirm the local published source identifies the override's origin."""
    try:
        cargo_manifest = tomllib.loads((package_dir / "Cargo.toml").read_text(encoding="utf-8"))
        metadata = cargo_manifest["package"]
        vcs = json.loads((package_dir / ".cargo_vcs_info.json").read_text(encoding="utf-8"))
    except (OSError, KeyError, TypeError, tomllib.TOMLDecodeError, json.JSONDecodeError) as error:
        raise override_error(
            f"{override['id']} cannot verify the published crate source: {error}"
        ) from error
    if not isinstance(metadata, dict) or not isinstance(vcs, dict):
        raise override_error(f"{override['id']} has malformed published crate metadata")
    git_metadata = vcs.get("git")
    if not isinstance(git_metadata, dict):
        raise override_error(f"{override['id']} has no published VCS revision")
    published_origin = canonical_repository_url(
        metadata.get("repository") or metadata.get("homepage")
    )
    expected_origin = canonical_repository_url(override["sourceUrl"])
    published_revision = git_metadata.get("sha1")
    if published_origin != expected_origin or published_revision != override["sourceRevision"]:
        raise override_error(
            f"{override['id']} does not match the published crate repository and VCS revision"
        )
    for legal_file in override["legalFiles"]:
        published_path = legal_file["publishedCratePath"]
        if published_path is None:
            continue
        source = (package_dir / published_path).resolve()
        if (
            not source.is_file()
            or not within(source, [package_dir.resolve()])
            or file_sha256(source) != legal_file["publishedCrateSha256"]
            or legal_file["path"].read_bytes() not in source.read_bytes()
        ):
            raise override_error(
                f"{override['id']} source-header notice does not match the published crate"
            )


def load_overrides(override_dir: Path) -> dict[tuple[str, str], dict]:
    """Load hash-verified, version-scoped legal-text fallbacks.

    Crate archives occasionally omit the repository-root notice. Each fallback
    is tied to the exact published revision, or to a separately identified
    legal-file-only direct successor, plus an explicit crate name/version/
    license-expression allowlist; it is never a blanket license.
    """
    manifest_path = override_dir / "overrides.json"
    if not manifest_path.is_file():
        raise override_error(f"manifest is missing: {manifest_path}")
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise override_error(f"manifest cannot be read: {error}") from error
    if not isinstance(manifest, dict) or manifest.get("schemaVersion") != 1:
        raise override_error("manifest schemaVersion must be 1")
    entries = manifest.get("overrides")
    if not isinstance(entries, list) or not entries:
        raise override_error("manifest overrides must be a non-empty array")

    by_crate: dict[tuple[str, str], dict] = {}
    override_ids: set[str] = set()
    for entry in entries:
        if not isinstance(entry, dict):
            raise override_error("each override must be an object")
        identifier = entry.get("id")
        source_url = entry.get("sourceUrl")
        revision = entry.get("sourceRevision")
        legal_text_provenance = entry.get("legalTextProvenance")
        allowed = entry.get("crateVersionAllowlist")
        legal_files = entry.get("legalFiles")
        if (
            not isinstance(identifier, str)
            or not identifier
            or identifier in override_ids
            or not isinstance(source_url, str)
            or not source_url.startswith("https://")
            or not isinstance(revision, str)
            or not REVISION_RE.fullmatch(revision)
            or not isinstance(allowed, list)
            or not allowed
            or not isinstance(legal_files, list)
            or not legal_files
        ):
            raise override_error(f"malformed override {identifier!r}")
        override_ids.add(identifier)

        legal_text_revision = revision
        checked_provenance = None
        if legal_text_provenance is not None:
            if not isinstance(legal_text_provenance, dict):
                raise override_error(f"{identifier} has malformed legal-text provenance")
            legal_text_revision = legal_text_provenance.get("revision")
            direct_parent = legal_text_provenance.get("directParentRevision")
            commit_url = legal_text_provenance.get("commitUrl")
            if (
                set(legal_text_provenance)
                != {"revision", "directParentRevision", "commitUrl"}
                or not isinstance(legal_text_revision, str)
                or not REVISION_RE.fullmatch(legal_text_revision)
                or legal_text_revision == revision
                or direct_parent != revision
                or not isinstance(commit_url, str)
                or not commit_url.startswith("https://")
                or legal_text_revision not in commit_url
            ):
                raise override_error(
                    f"{identifier} retrospective legal text is not tied to a direct successor commit"
                )
            checked_provenance = {
                "revision": legal_text_revision,
                "directParentRevision": direct_parent,
                "commitUrl": commit_url,
            }

        checked_files: list[dict] = []
        seen_files: set[Path] = set()
        for legal_file in legal_files:
            if not isinstance(legal_file, dict):
                raise override_error(f"{identifier} has a malformed legal file")
            relative = legal_file.get("path")
            digest = legal_file.get("sha256")
            file_source_url = legal_file.get("sourceUrl")
            published_path = legal_file.get("publishedCratePath")
            published_digest = legal_file.get("publishedCrateSha256")
            if (
                not isinstance(relative, str)
                or not relative
                or Path(relative).is_absolute()
                or not isinstance(digest, str)
                or not SHA256_RE.fullmatch(digest)
                or not isinstance(file_source_url, str)
                or not file_source_url.startswith("https://")
                or legal_text_revision not in file_source_url
                or (published_path is None) != (published_digest is None)
                or (published_path is not None and (not isinstance(published_path, str) or not published_path))
                or (published_digest is not None and (not isinstance(published_digest, str) or not SHA256_RE.fullmatch(published_digest)))
            ):
                raise override_error(f"{identifier} has unverifiable legal-file metadata")
            candidate = (override_dir / relative).resolve()
            if (
                candidate in seen_files
                or not candidate.is_file()
                or not within(candidate, [override_dir.resolve()])
                or candidate.stat().st_size > MAX_LEGAL_FILE_BYTES
                or file_sha256(candidate) != digest
            ):
                raise override_error(f"{identifier} legal file failed integrity verification")
            seen_files.add(candidate)
            checked_files.append(
                {
                    "path": candidate,
                    "sha256": digest,
                    "sourceUrl": file_source_url,
                    "publishedCratePath": published_path,
                    "publishedCrateSha256": published_digest,
                }
            )

        for item in allowed:
            if not isinstance(item, dict):
                raise override_error(f"{identifier} has a malformed crate allowlist")
            name = item.get("name")
            version = item.get("version")
            expression = item.get("licenseExpression")
            key = (name, version)
            if (
                not isinstance(name, str)
                or not name
                or not isinstance(version, str)
                or not version
                or not isinstance(expression, str)
                or not expression
                or key in by_crate
            ):
                raise override_error(f"{identifier} has a duplicate or malformed crate allowlist")
            by_crate[key] = {
                "id": identifier,
                "sourceUrl": source_url,
                "sourceRevision": revision,
                "licenseExpression": expression,
                "crateVersionAllowlist": allowed,
                "legalFiles": checked_files,
            }
            if checked_provenance is not None:
                by_crate[key]["legalTextProvenance"] = checked_provenance
    return by_crate


def legal_files(package: dict, package_dir: Path) -> tuple[list[Path], list[str]]:
    roots = [package_dir.resolve()]
    git_root = git_checkout_root(package_dir)
    if git_root is not None:
        roots.append(git_root.resolve())

    candidates: list[Path] = []
    declared = package.get("license_file")
    if declared:
        candidates.append(package_dir / declared)
    try:
        candidates.extend(
            entry
            for entry in package_dir.iterdir()
            if entry.name.lower().startswith(LEGAL_NAMES)
        )
    except OSError:
        pass

    found: list[Path] = []
    findings: list[str] = []
    seen: set[Path] = set()
    for candidate in candidates:
        try:
            resolved = candidate.resolve(strict=True)
            size = resolved.stat().st_size
        except OSError:
            if declared and candidate == package_dir / declared:
                findings.append("declared_license_file_missing")
            continue
        if resolved in seen or not resolved.is_file() or not within(resolved, roots):
            continue
        if size > MAX_LEGAL_FILE_BYTES:
            findings.append("legal_file_exceeds_4_mib")
            continue
        seen.add(resolved)
        found.append(resolved)
    # Cargo git dependencies are often workspace members whose legal text lives
    # only at the checkout root. Use that fallback only when the package itself
    # did not supply a usable file, avoiding repeated unrelated repository files.
    if not found and git_root is not None and git_root.resolve() != package_dir.resolve():
        try:
            fallback = [
                entry
                for entry in git_root.iterdir()
                if entry.name.lower().startswith(LEGAL_NAMES)
            ]
        except OSError:
            fallback = []
        for candidate in fallback:
            try:
                resolved = candidate.resolve(strict=True)
                size = resolved.stat().st_size
            except OSError:
                continue
            if (
                resolved in seen
                or not resolved.is_file()
                or not within(resolved, roots)
                or size > MAX_LEGAL_FILE_BYTES
            ):
                continue
            seen.add(resolved)
            found.append(resolved)
    found.sort(key=lambda path: (path.name.lower(), str(path)))
    return found, sorted(set(findings))


def dependency_scopes(metadata: dict) -> dict[str, set[str]]:
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    root = metadata["resolve"]["root"]
    scopes: dict[str, set[str]] = {}

    def walk(label: str, accepted_kinds: set[str | None]) -> None:
        pending = [root]
        visited = {root}
        while pending:
            current = pending.pop()
            for dependency in nodes[current].get("deps", []):
                kinds = dependency.get("dep_kinds", [])
                if not any(kind.get("kind") in accepted_kinds for kind in kinds):
                    continue
                package_id = dependency["pkg"]
                scopes.setdefault(package_id, set()).add(label)
                if package_id not in visited:
                    visited.add(package_id)
                    pending.append(package_id)

    walk("application", {None, "build"})
    walk("validation", {None, "build", "dev"})
    return scopes


def source_label(package: dict, package_dir: Path, repo_root: Path) -> str:
    if package.get("source"):
        return package["source"]
    try:
        relative = package_dir.resolve().relative_to(repo_root)
    except ValueError:
        return "local-path-redacted"
    return f"workspace-path:{relative.as_posix()}"


def validate_locked_override_applicability(
    packages: dict[str, dict], dependency_ids: list[str], overrides: dict[tuple[str, str], dict]
) -> None:
    """Reject stale or misidentified fallbacks before creating an inventory."""
    applied: set[tuple[str, str]] = set()
    for package_id in dependency_ids:
        package = packages[package_id]
        package_dir = Path(package["manifest_path"]).resolve().parent
        files, _ = legal_files(package, package_dir)
        key = (package["name"], package["version"])
        override = overrides.get(key)
        if files or override is None:
            continue
        if package.get("license") != override["licenseExpression"]:
            raise override_error(
                f"{override['id']} does not match {package['name']} {package['version']} license expression"
            )
        validate_override_origin(package_dir, override)
        applied.add(key)
    unused = sorted(set(overrides) - applied)
    if unused:
        raise override_error(
            "configured crate allowlist did not match a legal-text-missing dependency: "
            + ", ".join(f"{name} {version}" for name, version in unused)
        )


def main() -> int:
    args = arguments()
    script = Path(__file__).resolve()
    repo_root = script.parent.parent
    manifest = repo_root / "rust" / "Cargo.toml"
    output = args.output.resolve()
    if output.exists():
        print(f"Output directory already exists: {output}", file=sys.stderr)
        return 2
    try:
        overrides = load_overrides(repo_root / "rust" / "licenses" / "dependency-overrides")
    except ValueError as error:
        print(error, file=sys.stderr)
        return 1

    command = [
        "cargo",
        "metadata",
        "--manifest-path",
        str(manifest),
        "--locked",
        "--offline",
        "--all-features",
        "--filter-platform",
        args.target,
        "--format-version",
        "1",
    ]
    try:
        result = subprocess.run(command, check=True, capture_output=True, text=True)
        metadata = json.loads(result.stdout)
    except (OSError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        detail = getattr(error, "stderr", None)
        if detail:
            print(detail.rstrip(), file=sys.stderr)
        print(f"Could not resolve the locked offline Cargo graph: {error}", file=sys.stderr)
        return 1

    scopes = dependency_scopes(metadata)
    packages = {package["id"]: package for package in metadata["packages"]}
    dependency_ids = sorted(
        scopes,
        key=lambda package_id: (
            packages[package_id]["name"].lower(),
            packages[package_id]["version"],
            package_id,
        ),
    )
    try:
        validate_locked_override_applicability(packages, dependency_ids, overrides)
    except ValueError as error:
        print(error, file=sys.stderr)
        return 1

    license_root = output / "licenses"
    output.mkdir(parents=True)
    license_root.mkdir()
    records: list[dict] = []
    review_findings: list[dict] = []
    notice_sections: list[str] = []
    applied_overrides: set[tuple[str, str]] = set()

    for package_id in dependency_ids:
        package = packages[package_id]
        package_dir = Path(package["manifest_path"]).resolve().parent
        files, findings = legal_files(package, package_dir)
        expression = package.get("license")
        override = None
        override_key = (package["name"], package["version"])
        if not files and override_key in overrides:
            override = overrides[override_key]
            applied_overrides.add(override_key)
            files = [legal_file["path"] for legal_file in override["legalFiles"]]
        if not expression:
            findings.append("missing_license_declaration")
        if not files:
            findings.append("no_legal_text_found")
        findings = sorted(set(findings))

        identity = hashlib.sha256(package_id.encode()).hexdigest()[:12]
        directory_name = safe_component(
            f"{package['name']}-{package['version']}-{identity}"
        )
        copied: list[str] = []
        used_names: set[str] = set()
        for source in files:
            filename = safe_component(source.name)
            if filename in used_names:
                digest = hashlib.sha256(source.read_bytes()).hexdigest()[:8]
                filename = f"{filename}-{digest}"
            used_names.add(filename)
            destination = license_root / directory_name / filename
            destination.parent.mkdir(exist_ok=True)
            shutil.copyfile(source, destination)
            relative = destination.relative_to(output).as_posix()
            copied.append(relative)
            text = source.read_text(encoding="utf-8", errors="replace")
            notice_sections.append(
                f"===== {package['name']} {package['version']} :: {filename} =====\n{text.rstrip()}\n"
            )

        record = {
            "name": package["name"],
            "version": package["version"],
            "source": source_label(package, package_dir, repo_root),
            "scopes": sorted(scopes[package_id]),
            "licenseExpression": expression,
            "licenseFiles": copied,
            "reviewFindings": findings,
        }
        if override is not None:
            record["licenseOverride"] = {
                "id": override["id"],
                "sourceUrl": override["sourceUrl"],
                "sourceRevision": override["sourceRevision"],
                "crateVersionAllowlist": override["crateVersionAllowlist"],
                "legalFiles": [
                    {
                        key: value
                        for key, value in {
                            "sha256": legal_file["sha256"],
                            "sourceUrl": legal_file["sourceUrl"],
                            "publishedCratePath": legal_file["publishedCratePath"],
                            "publishedCrateSha256": legal_file["publishedCrateSha256"],
                        }.items()
                        if value is not None
                    }
                    for legal_file in override["legalFiles"]
                ],
            }
            if "legalTextProvenance" in override:
                record["licenseOverride"]["legalTextProvenance"] = override[
                    "legalTextProvenance"
                ]
        records.append(record)
        if findings:
            review_findings.append(
                {
                    "name": package["name"],
                    "version": package["version"],
                    "findings": findings,
                }
            )

    if set(overrides) != applied_overrides:
        print("Dependency license override application changed during inventory generation.", file=sys.stderr)
        return 1

    inventory = {
        "schemaVersion": 1,
        "purpose": "offline locked Rust dependency license inventory; not legal certification",
        "target": args.target,
        "cargoCommand": "cargo metadata --locked --offline --all-features --filter-platform <target> --format-version 1",
        "packageCount": len(records),
        "packages": records,
        "reviewFindings": review_findings,
    }
    (output / "inventory.json").write_text(
        json.dumps(inventory, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    header = (
        "THIRD-PARTY RUST DEPENDENCY NOTICES\n\n"
        "Generated from the locked offline Cargo graph. This collection is an inventory, "
        "not legal advice or certification. See the accompanying inventory JSON for missing texts and declarations.\n\n"
    )
    (output / "THIRD_PARTY_NOTICES.txt").write_text(
        header + "\n".join(notice_sections), encoding="utf-8"
    )
    print(
        f"Recorded {len(records)} dependencies and {len(review_findings)} review findings in {output}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
