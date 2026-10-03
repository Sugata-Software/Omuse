#!/usr/bin/env python3
"""Build evidence for, verify, and attach immutable experimental release downloads.

Publication never creates releases, moves tags, replaces assets or edits notes.
The candidate build and the reviewed manifest publication are separate workflows.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def sibling(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


notes = sibling("release-notes")
bundle = sibling("verify-release-bundle")
REPOSITORY, API_ROOT = notes.REPOSITORY, notes.API_ROOT
BUILD_WORKFLOW = ".github/workflows/build-downloads.yml"
PUBLISH_WORKFLOW = ".github/workflows/publish-downloads.yml"
BUILD_NAME = "Omuse downloadable preview builds"
TARGETS = ("linux-x86_64", "windows-x86_64")
FIELDS = {"schemaVersion", "version", "tag", "sourceRevision", "validationRun", "buildRun", "buildAttempt", "buildWorkflowRevision", "prerelease", "signed", "qualification", "limitations", "assets"}
ASSET_FIELDS = {"target", "fileName", "sha256", "size", "binarySha256", "inventorySha256", "dependencyCount", "smokeSha256"}
LIMITATIONS = [
    "Unsigned experimental previews; native interactive desktop, mixed-DPI and live AI operation qualification remain incomplete.",
    "Linux binaries are built and smoke-tested on Ubuntu 24.04 x86_64; other Linux distributions are not established by that test.",
    "Windows binaries are built and smoke-tested on Windows Server 2025 x86_64; desktop Windows 10/11 qualification remains separate.",
    "Upgrade smoke checks replace and roll back the same candidate; migration from an older published binary is not claimed.",
]
CHECKS = {"archive-integrity", "zero-notice-findings", "native-version", "packaged-editing-journey", "runtime-library-load", "install", "upgrade", "rollback", "profile-preservation"}


class DownloadError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise DownloadError(message)


def canonical(value):
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("utf-8")


def positive(value):
    return type(value) is int and value > 0


def filename(version, target):
    return f"omuse-{version}-{target}.{'tar.gz' if target == 'linux-x86_64' else 'zip'}"


def manifest_path(version):
    return f"docs/releases/downloads/v{version}.json"


def validate_manifest(value):
    require(isinstance(value, dict) and set(value) == FIELDS, "Download manifest has unknown or missing fields")
    require(type(value["schemaVersion"]) is int and value["schemaVersion"] == 1, "Unsupported download manifest")
    require(isinstance(value["version"], str) and notes.VERSION.fullmatch(value["version"]), "Invalid download version")
    require(value["tag"] == "v" + value["version"], "Download tag differs from version")
    for key in ("sourceRevision", "buildWorkflowRevision"):
        require(isinstance(value[key], str) and notes.SHA.fullmatch(value[key]), f"{key} must identify an immutable commit")
    for key in ("validationRun", "buildRun", "buildAttempt"):
        require(positive(value[key]), f"{key} must be a positive integer")
    require(value["prerelease"] is True and value["signed"] is False and value["qualification"] == "automated-preview", "This pipeline only publishes unsigned experimental prereleases")
    require(value["limitations"] == LIMITATIONS, "Preview qualification limits must remain explicit")
    require(isinstance(value["assets"], list) and len(value["assets"]) == 2, "A download manifest needs both supported target archives")
    seen = set()
    for asset in value["assets"]:
        require(isinstance(asset, dict) and set(asset) == ASSET_FIELDS, "Malformed archive declaration")
        target = asset["target"]
        require(target in TARGETS and target not in seen, "Duplicate or unsupported download target")
        seen.add(target)
        require(asset["fileName"] == filename(value["version"], target), "Archive name must contain the exact version and target")
        for field in ("sha256", "binarySha256", "inventorySha256", "smokeSha256"):
            require(isinstance(asset[field], str) and bundle.SHA256.fullmatch(asset[field]), f"Invalid {field}")
        require(positive(asset["size"]) and asset["size"] <= bundle.MAX_ARCHIVE, "Invalid archive size")
        require(positive(asset["dependencyCount"]), "Dependency inventory cannot be empty")
    return value


def read_manifest(path):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size < 32_768, "Manifest must be a bounded regular file")
    content = path.read_bytes()
    value = validate_manifest(bundle.parse_json(content))
    require(content == canonical(value), "Manifest must retain its canonical generated bytes")
    return value


def expected_assets(manifest):
    """All permissible GitHub assets, including the manifest completion marker."""
    manifest = validate_manifest(manifest)
    result = {asset["fileName"]: {"sha256": asset["sha256"], "size": asset["size"]} for asset in manifest["assets"]}
    name = f"omuse-{manifest['version']}-downloads.json"
    content = canonical(manifest)
    result[name] = {"sha256": hashlib.sha256(content).hexdigest(), "size": len(content), "content": content}
    sums = "".join(f"{record['sha256']}  {name}\n" for name, record in sorted(result.items())).encode()
    result[f"omuse-{manifest['version']}-SHA256SUMS"] = {"sha256": hashlib.sha256(sums).hexdigest(), "size": len(sums), "content": sums}
    return result


def check_remote_assets(assets, manifest, complete=False):
    expected = expected_assets(manifest)
    require(isinstance(assets, list), "Invalid GitHub asset listing")
    seen = set()
    for asset in assets:
        name = asset.get("name")
        require(name in expected and name not in seen, "Release contains an unlisted or duplicate asset")
        seen.add(name)
        require(asset.get("state") == "uploaded" and asset.get("size") == expected[name]["size"]
                and asset.get("digest") == "sha256:" + expected[name]["sha256"],
                f"Existing release asset differs: {name}; it will never be replaced")
    if complete:
        require(seen == set(expected), "Release download publication is incomplete")
    return seen


def check_artifacts(directory, manifest):
    for asset in manifest["assets"]:
        folder = directory / asset["target"]
        archive, receipt = folder / asset["fileName"], folder / "smoke.json"
        require(archive.is_file() and not archive.is_symlink() and receipt.is_file() and not receipt.is_symlink(), "Downloaded archive or smoke receipt is missing")
        require(bundle.digest(receipt) == asset["smokeSha256"], "Smoke receipt differs from the reviewed manifest")
        require(receipt.stat().st_size < 64 * 1024, "Smoke receipt is too large")
        smoke = bundle.parse_json(receipt.read_bytes())
        inspected = bundle.inspect_archive(archive, asset["target"], manifest["sourceRevision"], manifest["version"])
        for key in ("target", "fileName", "sha256", "size", "binarySha256", "inventorySha256", "dependencyCount"):
            require(asset[key] == inspected[key] == smoke.get(key), f"Archive/receipt identity differs: {key}")
        require(smoke.get("sourceRevision") == manifest["sourceRevision"] and smoke.get("version") == manifest["version"]
                and smoke.get("status") == "passed" and set(smoke.get("checks", [])) == CHECKS
                and smoke.get("noticeFindings") == 0 and smoke.get("upgradeBaseline") == "same-candidate-reinstall",
                "Native package smoke evidence is incomplete")


def assemble(directory, source, version, validation_run, build_run, attempt, workflow_revision):
    assets = []
    for target in TARGETS:
        archive = directory / target / filename(version, target)
        receipt = directory / target / "smoke.json"
        inspected = bundle.inspect_archive(archive, target, source, version)
        assets.append({key: inspected[key] for key in ASSET_FIELDS - {"smokeSha256"}} | {"smokeSha256": bundle.digest(receipt)})
    manifest = validate_manifest({
        "schemaVersion": 1, "version": version, "tag": "v" + version, "sourceRevision": source,
        "validationRun": validation_run, "buildRun": build_run, "buildAttempt": attempt,
        "buildWorkflowRevision": workflow_revision, "prerelease": True, "signed": False,
        "qualification": "automated-preview", "limitations": LIMITATIONS, "assets": assets,
    })
    check_artifacts(directory, manifest)
    return manifest


def check_run(api, run_id, source, path, name, required_jobs, *, attempt=None):
    run = api.request(f"{API_ROOT}/actions/runs/{run_id}")
    require(run.get("id") == run_id and run.get("head_sha") == source and run.get("path") == path
            and run.get("name") == name and run.get("event") in ("push", "workflow_dispatch")
            and run.get("repository", {}).get("full_name") == REPOSITORY
            and run.get("head_repository", {}).get("full_name") == REPOSITORY,
            "Build/validation run does not belong to the declared public source and workflow")
    require(run.get("status") == "completed" and run.get("conclusion") == "success", "Required workflow has not passed")
    current_attempt = run.get("run_attempt")
    require(positive(current_attempt) and (attempt is None or attempt == current_attempt), "Workflow attempt differs from the reviewed artifacts")
    jobs = api.request(f"{API_ROOT}/actions/runs/{run_id}/attempts/{current_attempt}/jobs?per_page=100").get("jobs", [])
    for name in required_jobs:
        matches = [job for job in jobs if job.get("name") == name]
        require(len(matches) == 1 and matches[0].get("head_sha") == source and matches[0].get("status") == "completed"
                and matches[0].get("conclusion") == "success", f"Required job did not pass: {name}")
    return run


def public_main(api):
    repository = api.request(API_ROOT)
    require(repository.get("full_name") == REPOSITORY and repository.get("private") is False
            and repository.get("default_branch") == "main", "Expected public Omuse main repository")
    ref = api.request(f"{API_ROOT}/git/ref/heads/main").get("object", {})
    require(ref.get("type") == "commit" and isinstance(ref.get("sha"), str) and notes.SHA.fullmatch(ref["sha"]), "Invalid public main revision")
    return ref["sha"]


def ancestor(api, source, head):
    comparison = api.request(f"{API_ROOT}/compare/{source}...{head}")
    require(comparison.get("status") in ("ahead", "identical") and comparison.get("base_commit", {}).get("sha") == source
            and comparison.get("merge_base_commit", {}).get("sha") == source, "Candidate is not an ancestor of the required public revision")


def verify_source(api, source, validation_run):
    require(isinstance(source, str) and notes.SHA.fullmatch(source) and positive(validation_run), "Use an immutable source and validation run")
    ancestor(api, source, public_main(api))
    version = notes.cargo_version(notes.remote_file(api, "rust/Cargo.toml", source))
    require(isinstance(version, str) and notes.VERSION.fullmatch(version), "Source has an invalid version")
    check_run(api, validation_run, source, notes.WORKFLOW, "Omuse Rust validation",
              notes.REQUIRED_JOBS | {"Windows build, tests and journey"})
    return version


def plan_release_push(api, source, environment):
    """Allow read-only candidate builds before their workflow reaches main."""
    ref = environment.get("GITHUB_REF", "")
    require(environment.get("GITHUB_ACTIONS") == "true" and environment.get("GITHUB_REPOSITORY") == REPOSITORY
            and environment.get("GITHUB_EVENT_NAME") == "push" and ref.startswith("refs/heads/release/")
            and environment.get("GITHUB_WORKFLOW_REF") == f"{REPOSITORY}/{BUILD_WORKFLOW}@{ref}"
            and source == environment.get("GITHUB_SHA") and isinstance(source, str) and notes.SHA.fullmatch(source),
            "Branch preview builds require the exact public release-branch push")
    public_main(api)  # Validate the repository without requiring a prior main merge.
    from urllib.parse import quote
    branch = quote(ref.removeprefix("refs/heads/"), safe="/")
    current = api.request(f"{API_ROOT}/git/ref/heads/{branch}").get("object", {})
    require(current.get("type") == "commit" and isinstance(current.get("sha"), str)
            and notes.SHA.fullmatch(current["sha"]), "Release branch is not available")
    ancestor(api, source, current["sha"])
    version = notes.cargo_version(notes.remote_file(api, "rust/Cargo.toml", source))
    require(isinstance(version, str) and notes.VERSION.fullmatch(version), "Source has an invalid version")
    return version


def wait_validation(api, source, *, timeout=7200, sleep=time.sleep, clock=time.monotonic):
    """Wait only for CI on this immutable source; no repository writes occur."""
    require(isinstance(source, str) and notes.SHA.fullmatch(source), "Validation needs an immutable source")
    deadline = clock() + timeout
    endpoint = f"{API_ROOT}/actions/workflows/rust-validation.yml/runs?head_sha={source}&per_page=100"
    while True:
        runs = api.request(endpoint).get("workflow_runs", [])
        require(isinstance(runs, list), "Invalid source validation listing")
        matching = [run for run in runs if run.get("head_sha") == source and positive(run.get("id"))
                    and run.get("event") in ("push", "workflow_dispatch")]
        if matching:
            latest = max(matching, key=lambda run: run["id"])
            if latest.get("status") == "completed":
                check_run(api, latest["id"], source, notes.WORKFLOW, "Omuse Rust validation",
                          notes.REQUIRED_JOBS | {"Windows build, tests and journey"})
                return latest["id"]
        remaining = deadline - clock()
        require(remaining > 0, "Exact-source Rust validation did not complete within the build window")
        sleep(min(20, remaining))


def verify_publication(api, manifest, raw):
    version = verify_source(api, manifest["sourceRevision"], manifest["validationRun"])
    require(version == manifest["version"], "Source package differs from the reviewed download version")
    head = public_main(api)
    remote = notes.remote_file(api, manifest_path(version), head, 32_768).encode("utf-8")
    require(remote == raw, "Public main contains a different download manifest; this run is stale")
    ancestor(api, manifest["sourceRevision"], manifest["buildWorkflowRevision"])
    ancestor(api, manifest["buildWorkflowRevision"], head)
    run = check_run(api, manifest["buildRun"], manifest["buildWorkflowRevision"], BUILD_WORKFLOW, BUILD_NAME,
                    {"Build linux-x86_64", "Build windows-x86_64", "Validate exact source", "Assemble immutable download manifest"}, attempt=manifest["buildAttempt"])
    main_dispatch = run.get("event") == "workflow_dispatch" and run.get("head_branch") == "main"
    release_push = (run.get("event") == "push" and isinstance(run.get("head_branch"), str)
                    and run["head_branch"].startswith("release/")
                    and manifest["sourceRevision"] == manifest["buildWorkflowRevision"])
    require(main_dispatch or release_push, "Download build must originate from main dispatch or its exact release-branch push")
    require(notes.tag_target(api, manifest["tag"]) == manifest["sourceRevision"], "Published tag differs from the candidate source")
    release = notes.existing_release(api, manifest["tag"])
    require(isinstance(release, dict) and release.get("draft") is False and release.get("prerelease") is True
            and release.get("target_commitish") == manifest["sourceRevision"], "Publish the matching source prerelease before attaching downloads")
    body = notes.remote_file(api, f"docs/releases/{manifest['tag']}.md", head, 60_000)
    require(release.get("body") == body and "unsigned" in body.lower() and "experimental" in body.lower(), "Published notes must match reviewed notes and disclose unsigned experimental downloads")
    require(positive(release.get("id")), "Release has no stable ID")
    check_remote_assets(release.get("assets"), manifest)
    return release


def cli_environment(environment):
    env = dict(environment)
    env.pop("GH_DEBUG", None)
    env["GH_PROMPT_DISABLED"] = "1"
    env["GH_PAGER"] = "cat"
    return env


def gh(command, environment):
    result = subprocess.run(["gh", *command], env=cli_environment(environment), capture_output=True, timeout=900)
    require(result.returncode == 0, "GitHub artifact operation did not complete; rerun to inspect existing state")


def publisher_context(environment):
    require(environment.get("GITHUB_ACTIONS") == "true" and environment.get("GITHUB_REPOSITORY") == REPOSITORY
            and environment.get("GITHUB_EVENT_NAME") == "workflow_dispatch" and environment.get("GITHUB_REF") == "refs/heads/main"
            and environment.get("GITHUB_WORKFLOW_REF") == f"{REPOSITORY}/{PUBLISH_WORKFLOW}@refs/heads/main"
            and environment.get("GH_TOKEN"), "Publication is restricted to the explicit public main download workflow")


def publish(api, manifest, environment, download=gh, upload=gh):
    publisher_context(environment)
    raw = canonical(manifest)
    release = verify_publication(api, manifest, raw)
    existing = check_remote_assets(release["assets"], manifest)
    expected = expected_assets(manifest)
    if existing == set(expected):
        return "unchanged"
    with tempfile.TemporaryDirectory(prefix="omuse-release-assets-") as temporary:
        directory = Path(temporary)
        for target in TARGETS:
            folder = directory / target
            folder.mkdir()
            download(["run", "download", str(manifest["buildRun"]), "--repo", REPOSITORY, "--name",
                      f"omuse-download-{target}-attempt-{manifest['buildAttempt']}", "--dir", str(folder)], environment)
        check_artifacts(directory, manifest)
        paths = {asset["fileName"]: directory / asset["target"] / asset["fileName"] for asset in manifest["assets"]}
        for name, value in expected.items():
            if "content" in value:
                paths[name] = directory / name
                paths[name].write_bytes(value["content"])
        # Inspect again immediately before the first write. Never overwrite an
        # asset that appeared while downloading, even if a concurrent run added it.
        current = api.request(f"{API_ROOT}/releases/{release['id']}")
        require(current.get("tag_name") == manifest["tag"] and current.get("draft") is False and current.get("prerelease") is True,
                "Release identity or qualification changed during preparation")
        require(current.get("target_commitish") == manifest["sourceRevision"] and current.get("body") == release.get("body")
                and notes.tag_target(api, manifest["tag"]) == manifest["sourceRevision"],
                "Release source or notes changed during preparation")
        existing = check_remote_assets(current.get("assets"), manifest)
        completion = f"omuse-{manifest['version']}-downloads.json"
        for name in sorted(expected, key=lambda name: (name == completion, name)):
            if name in existing:
                continue
            # No --clobber and no DELETE/PATCH operation exists in this publisher.
            upload(["release", "upload", manifest["tag"], str(paths[name]), "--repo", REPOSITORY], environment)
            current = api.request(f"{API_ROOT}/releases/{release['id']}")
            existing = check_remote_assets(current.get("assets"), manifest)
            require(name in existing, "Uploaded asset was not confirmed by GitHub")
        require(notes.tag_target(api, manifest["tag"]) == manifest["sourceRevision"], "Tag identity changed during publication")
        check_remote_assets(existing_release_assets(api, release["id"]), manifest, complete=True)
    return "published"


def existing_release_assets(api, release_id):
    return api.request(f"{API_ROOT}/releases/{release_id}").get("assets")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--plan", action="store_true")
    mode.add_argument("--plan-release-push", action="store_true")
    mode.add_argument("--wait-validation", action="store_true")
    mode.add_argument("--assemble", type=Path)
    mode.add_argument("--check", type=Path)
    mode.add_argument("--verify", type=Path)
    mode.add_argument("--publish", type=Path)
    parser.add_argument("--source")
    parser.add_argument("--version")
    parser.add_argument("--validation-run", type=int)
    parser.add_argument("--build-run", type=int)
    parser.add_argument("--build-attempt", type=int)
    parser.add_argument("--workflow-revision")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--artifacts", type=Path)
    args = parser.parse_args()
    try:
        if args.plan_release_push:
            version = plan_release_push(notes.GitHub(), args.source, os.environ)
            print(f"source={args.source}\nversion={version}")
            return 0
        if args.wait_validation:
            validation_run = wait_validation(notes.GitHub(), args.source)
            print(f"validation_run={validation_run}")
            return 0
        if args.plan:
            version = verify_source(notes.GitHub(), args.source, args.validation_run)
            print(f"source={args.source}\nversion={version}\nvalidation_run={args.validation_run}")
            return 0
        if args.assemble:
            value = assemble(args.assemble, args.source, args.version, args.validation_run, args.build_run, args.build_attempt, args.workflow_revision)
            require(args.output is not None and not args.output.exists(), "Use a new candidate manifest output")
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_bytes(canonical(value))
            print(f"Prepared candidate manifest: {args.output}")
            return 0
        path = args.check or args.verify or args.publish
        value = read_manifest(path)
        if args.artifacts:
            check_artifacts(args.artifacts, value)
        outcome = "checked offline"
        if args.verify:
            verify_publication(notes.GitHub(), value, path.read_bytes())
            outcome = "verified read-only"
        if args.publish:
            outcome = publish(notes.GitHub(), value, os.environ)
        print(f"Omuse {value['tag']} downloads: {outcome}")
        return 0
    except (DownloadError, notes.ReleaseError, bundle.BundleError, OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print(f"Release downloads: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
