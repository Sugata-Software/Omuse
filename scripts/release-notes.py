#!/usr/bin/env python3
"""Validate reviewed source-release notes and publish them without overwriting a version.

--check is offline. --verify reads public GitHub evidence. --publish is reserved
for this repository's main-push workflow and performs at most one release POST.
No local Git history, binary assets, credentials or generated notes are uploaded.
"""

import argparse
import base64
import datetime
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib


REPOSITORY = "Sugata-Software/Omuse"
API_ROOT = f"repos/{REPOSITORY}"
MANIFEST = "docs/releases/latest.json"
WORKFLOW = ".github/workflows/rust-validation.yml"
REQUIRED_JOBS = {
    "Linux installer and reference fixtures",
    "Omuse format, check, UI tests, and editing journey",
}
FIELDS = {
    "schemaVersion", "version", "tag", "title", "date", "prerelease",
    "sourceRevision", "notes", "validationRun",
}
SHA = re.compile(r"[0-9a-f]{40}\Z")
VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\Z")


class ReleaseError(Exception):
    pass


def require(condition, message):
    if not condition:
        raise ReleaseError(message)


def parse_json(text):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, f"Duplicate JSON field: {key}")
            result[key] = value
        return result
    try:
        return json.loads(text, object_pairs_hook=unique)
    except (ValueError, TypeError) as error:
        raise ReleaseError("Invalid JSON") from error


def read_file(root, relative, limit):
    path = root
    for part in Path(relative).parts:
        require(part not in ("..", "/"), "Release paths must stay inside the checkout")
        path /= part
        require(not path.is_symlink(), f"Symlinks are not allowed: {relative}")
    require(path.is_file(), f"Missing release input: {relative}")
    require(path.stat().st_size <= limit, f"Release input is too large: {relative}")
    return path.read_text(encoding="utf-8")


def installer_pin(text):
    matches = re.findall(r"^readonly omuse_release_revision=([0-9a-f]{40})$", text, re.M)
    require(len(matches) == 1, "Installer must declare exactly one immutable source pin")
    return matches[0]


def cargo_version(text):
    try:
        package = tomllib.loads(text).get("package", {})
    except tomllib.TOMLDecodeError as error:
        raise ReleaseError("Invalid Cargo package metadata") from error
    require(package.get("name") == "omuse", "Source package must be Omuse")
    return package.get("version")


def check_local(root):
    manifest = parse_json(read_file(root, MANIFEST, 16_384))
    require(isinstance(manifest, dict) and set(manifest) == FIELDS,
            "Release manifest must contain exactly the schemaVersion 1 fields")
    require(type(manifest["schemaVersion"]) is int and manifest["schemaVersion"] == 1,
            "Unsupported release manifest schema")
    for name in ("version", "tag", "title", "date", "sourceRevision", "notes"):
        require(isinstance(manifest[name], str), f"{name} must be a string")
    version = manifest["version"]
    require(VERSION.fullmatch(version), "Use a numbered major.minor.patch version")
    require(manifest["tag"] == f"v{version}", "Tag must match the declared version")
    require(manifest["notes"] == f"docs/releases/v{version}.md",
            "Notes must be the matching versioned Markdown file")
    title = manifest["title"]
    require(title.startswith(f"Omuse {version}") and len(title) <= 160
            and all(ord(char) >= 32 and ord(char) != 127 for char in title),
            "Title must identify the Omuse version on one line")
    require(re.fullmatch(r"\d{4}-\d{2}-\d{2}", manifest["date"]), "Date must be YYYY-MM-DD")
    try:
        datetime.date.fromisoformat(manifest["date"])
    except ValueError as error:
        raise ReleaseError("Release date is invalid") from error
    require(SHA.fullmatch(manifest["sourceRevision"]), "Source must be a full lowercase commit SHA")
    require(type(manifest["prerelease"]) is bool, "prerelease must be boolean")
    require(not version.startswith("0.") or manifest["prerelease"],
            "Early 0.x source releases must remain marked as prereleases")
    require(type(manifest["validationRun"]) is int and manifest["validationRun"] > 0,
            "validationRun must be a positive workflow run ID")
    notes = read_file(root, manifest["notes"], 60_000)
    require(notes.strip() and version in notes, "Notes must describe the declared version")
    require(all(char in "\n\t" or (ord(char) >= 32 and ord(char) != 127) for char in notes),
            "Notes must be UTF-8 text without control characters")
    require(cargo_version(read_file(root, "rust/Cargo.toml", 1_000_000)) == version,
            "Release version differs from Cargo package version")
    require(installer_pin(read_file(root, "install.sh", 1_000_000)) == manifest["sourceRevision"],
            "Release source differs from the reviewed installer pin")
    return manifest, notes


class GitHub:
    """Use the runner's gh CLI; never print raw diagnostics, headers or tokens."""

    def request(self, endpoint, *, method="GET", payload=None, missing_ok=False):
        require(endpoint.startswith(API_ROOT + "/") or endpoint == API_ROOT,
                "GitHub requests must stay in the public Omuse repository")
        command = [
            "gh", "api", "--hostname", "github.com", "--include", "--method", method,
            "--header", "Accept: application/vnd.github+json",
            "--header", "X-GitHub-Api-Version: 2026-03-10", endpoint,
        ]
        if payload is not None:
            command.extend(["--input", "-"])
        environment = dict(os.environ)
        environment.pop("GH_DEBUG", None)
        environment["GH_PROMPT_DISABLED"] = "1"
        environment["GH_PAGER"] = "cat"
        try:
            result = subprocess.run(
                command, input=None if payload is None else json.dumps(payload),
                capture_output=True, text=True, encoding="utf-8", timeout=60,
                env=environment, check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise ReleaseError(f"GitHub {method} did not complete; rerun to inspect existing state") from error
        headers, separator, body = result.stdout.replace("\r\n", "\n").partition("\n\n")
        match = re.match(r"HTTP/\S+ ([0-9]{3})(?:\s|$)", headers)
        require(separator and match, f"GitHub {method} did not return an HTTP response")
        status = int(match[1])
        if status == 404 and missing_ok and method == "GET":
            return None
        require(result.returncode == 0 and status in (200, 201),
                f"GitHub {method} failed with HTTP {status}; no existing release was overwritten")
        return parse_json(body)


def remote_file(api, path, revision, limit=1_000_000):
    response = api.request(f"{API_ROOT}/contents/{path}?ref={revision}")
    require(isinstance(response, dict) and response.get("type") == "file"
            and response.get("encoding") == "base64", f"Unexpected remote file: {path}")
    try:
        content = base64.b64decode("".join(response["content"].split()), validate=True)
        require(len(content) <= limit, f"Remote file is too large: {path}")
        return content.decode("utf-8")
    except (ValueError, KeyError, UnicodeError) as error:
        raise ReleaseError(f"Invalid remote file: {path}") from error


def tag_target(api, tag):
    response = api.request(f"{API_ROOT}/git/ref/tags/{tag}", missing_ok=True)
    if response is None:
        return None
    require(response.get("ref") == f"refs/tags/{tag}", "GitHub returned a different tag")
    obj = response.get("object", {})
    seen = set()
    for _ in range(8):
        sha = obj.get("sha", "")
        require(isinstance(sha, str) and SHA.fullmatch(sha), "Invalid tag object SHA")
        if obj.get("type") == "commit":
            return sha
        require(obj.get("type") == "tag" and sha not in seen, "Invalid or cyclic tag object")
        seen.add(sha)
        obj = api.request(f"{API_ROOT}/git/tags/{sha}").get("object", {})
    raise ReleaseError("Tag indirection exceeds the supported limit")


def existing_release(api, tag):
    # Listing with the publisher's token also sees drafts. Never replace a draft
    # or silently publish one through a second release using the same version.
    for page in range(1, 11):
        releases = api.request(f"{API_ROOT}/releases?per_page=100&page={page}")
        require(isinstance(releases, list), "Invalid release listing")
        matching = [release for release in releases if release.get("tag_name") == tag]
        require(len(matching) <= 1, "Multiple releases already use this tag")
        if matching:
            return matching[0]
        if len(releases) < 100:
            return None
    raise ReleaseError("Release listing exceeded its limit; inspect existing versions before publishing")


def check_existing(release, manifest, notes, target):
    require(target == manifest["sourceRevision"], "Existing tag points to a different source commit")
    expected = {
        "tag_name": manifest["tag"], "target_commitish": manifest["sourceRevision"],
        "name": manifest["title"], "body": notes, "draft": False,
        "prerelease": manifest["prerelease"], "assets": [],
    }
    for field, value in expected.items():
        require(release.get(field) == value, f"Existing release differs in {field}; published versions are never overwritten")


def verify_remote(api, manifest, notes):
    repository = api.request(API_ROOT)
    require(repository.get("full_name") == REPOSITORY and repository.get("private") is False
            and repository.get("default_branch") == "main", "Expected the public Omuse main repository")
    main = api.request(f"{API_ROOT}/git/ref/heads/main").get("object", {})
    head = main.get("sha", "")
    require(main.get("type") == "commit" and isinstance(head, str) and SHA.fullmatch(head),
            "Invalid public main revision")
    source = manifest["sourceRevision"]
    comparison = api.request(f"{API_ROOT}/compare/{source}...{head}")
    require(comparison.get("status") in ("ahead", "identical")
            and comparison.get("base_commit", {}).get("sha") == source
            and comparison.get("merge_base_commit", {}).get("sha") == source,
            "Release source is not an ancestor of public main")
    require(parse_json(remote_file(api, MANIFEST, head, 16_384)) == manifest,
            "Public main declares different release metadata; this publishing run is stale")
    require(remote_file(api, manifest["notes"], head, 60_000) == notes,
            "Public main contains different release notes; this publishing run is stale")
    require(installer_pin(remote_file(api, "install.sh", head)) == source,
            "Public installer pin differs from the release source")
    require(cargo_version(remote_file(api, "rust/Cargo.toml", source)) == manifest["version"],
            "Validated source has a different Cargo version")

    run_id = manifest["validationRun"]
    run = api.request(f"{API_ROOT}/actions/runs/{run_id}")
    require(run.get("id") == run_id and run.get("head_sha") == source
            and run.get("name") == "Omuse Rust validation" and run.get("path") == WORKFLOW
            and run.get("event") in ("push", "workflow_dispatch")
            and run.get("repository", {}).get("full_name") == REPOSITORY
            and run.get("head_repository", {}).get("full_name") == REPOSITORY,
            "Validation run must belong to the exact public source and Rust workflow")
    require(run.get("status") == "completed" and run.get("conclusion") == "success",
            "Exact-source Rust validation has not completed successfully")
    attempt = run.get("run_attempt")
    require(type(attempt) is int and attempt > 0, "Validation attempt is missing")
    result = api.request(f"{API_ROOT}/actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100")
    jobs = result.get("jobs", [])
    for name in REQUIRED_JOBS:
        matches = [job for job in jobs if job.get("name") == name]
        require(len(matches) == 1 and matches[0].get("head_sha") == source
                and matches[0].get("status") == "completed" and matches[0].get("conclusion") == "success",
                f"Validation job did not pass for this source: {name}")

    target = tag_target(api, manifest["tag"])
    require(target is None or target == source, "Existing tag points to a different source commit")
    release = existing_release(api, manifest["tag"])
    if release is not None:
        check_existing(release, manifest, notes, target)
    return release


def publish(api, manifest, notes):
    if verify_remote(api, manifest, notes) is not None:
        return "unchanged"
    payload = {
        "tag_name": manifest["tag"], "target_commitish": manifest["sourceRevision"],
        "name": manifest["title"], "body": notes, "draft": False,
        "prerelease": manifest["prerelease"], "make_latest": "false",
        "generate_release_notes": False,
    }
    # Creating the release also creates a missing tag at the explicit source.
    # GitHub ignores target_commitish for an existing tag, hence both the
    # preflight and readback resolve the actual ref. Never PATCH or move refs.
    created = api.request(f"{API_ROOT}/releases", method="POST", payload=payload)
    release_id = created.get("id")
    require(type(release_id) is int and release_id > 0, "Release creation returned no release ID; rerun to inspect")
    release = api.request(f"{API_ROOT}/releases/{release_id}")
    check_existing(release, manifest, notes, tag_target(api, manifest["tag"]))
    return "published"


def require_publisher_context(environment):
    require(environment.get("GITHUB_ACTIONS") == "true"
            and environment.get("GITHUB_REPOSITORY") == REPOSITORY
            and environment.get("GITHUB_EVENT_NAME") == "push"
            and environment.get("GITHUB_REF") == "refs/heads/main",
            "Publication is restricted to the reviewed public main-push workflow")
    require(environment.get("GH_TOKEN"), "The release job requires its GitHub Actions token")


def publication_requested(root, environment):
    """Only a reviewed declaration change or first publisher addition requests a release.

    Inspect the complete push range, not just head_commit: a push can contain
    several reviewed commits. This mode reads Git history and needs no token.
    """
    if (environment.get("GITHUB_EVENT_NAME") != "push"
            or environment.get("GITHUB_REF") != "refs/heads/main"
            or environment.get("GITHUB_REPOSITORY") != REPOSITORY):
        return False
    event_path = environment.get("GITHUB_EVENT_PATH")
    require(event_path, "Push event payload is unavailable")
    event = parse_json(Path(event_path).read_text(encoding="utf-8"))
    before, after = event.get("before", ""), event.get("after", "")
    require(isinstance(before, str) and SHA.fullmatch(before)
            and isinstance(after, str) and SHA.fullmatch(after)
            and after == environment.get("GITHUB_SHA"), "Push event revisions are invalid")
    if before == "0" * 40:
        return True  # The first main push; the offline check requires its declaration.
    result = subprocess.run(
        ["git", "diff", "--name-status", "--no-renames", before, after, "--",
         MANIFEST, ".github/workflows/release-notes.yml"],
        cwd=root, capture_output=True, text=True, encoding="utf-8", timeout=30, check=False,
    )
    require(result.returncode == 0, "Cannot inspect the complete reviewed push range")
    changed = [line.split("\t", 1) for line in result.stdout.splitlines()]
    return any(
        len(entry) == 2 and (entry[1] == MANIFEST
                            or entry == ["A", ".github/workflows/release-notes.yml"])
        for entry in changed
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--check", action="store_true", help="Validate local release inputs without network or credentials")
    modes.add_argument("--verify", action="store_true", help="Read GitHub validation and immutable-release evidence without publishing")
    modes.add_argument("--publish", action="store_true", help="Publish once from the reviewed main-push workflow")
    modes.add_argument("--publication-requested", action="store_true", help="Read the workflow push range and emit a publication-intent output")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        if args.publication_requested:
            requested = publication_requested(args.root.resolve(), os.environ)
            print(f"publish={'true' if requested else 'false'}")
            return 0
        manifest, notes = check_local(args.root.resolve())
        outcome = "checked offline"
        if args.publish:
            require_publisher_context(os.environ)
            outcome = publish(GitHub(), manifest, notes)
        elif args.verify:
            existing = verify_remote(GitHub(), manifest, notes)
            outcome = "verified unchanged" if existing else "verified ready to publish"
        print(f"Omuse {manifest['tag']}: {outcome}; source {manifest['sourceRevision']}")
        return 0
    except (ReleaseError, OSError, UnicodeError, subprocess.TimeoutExpired) as error:
        print(f"Release notes: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
