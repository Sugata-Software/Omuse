#!/usr/bin/env python3
"""Offline archive/publication regressions. No network calls or releases occur."""

from __future__ import annotations

import base64
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock
import zipfile


SPEC = importlib.util.spec_from_file_location("release_downloads", Path(__file__).with_name("release-downloads.py"))
downloads = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(downloads)
bundle = downloads.bundle
SOURCE, MAIN, VERSION = "a" * 40, "b" * 40, "0.8.0"


def encoded(value):
    return {"type": "file", "encoding": "base64", "content": base64.b64encode(value.encode()).decode()}


def payload(target):
    inventory = {
        "schemaVersion": 1, "target": "x86_64-unknown-linux-gnu" if target == "linux-x86_64" else "x86_64-pc-windows-msvc",
        "packageCount": 1, "packages": [{"name": "fixture", "version": "1.0.0", "licenseExpression": "MIT", "licenseFiles": ["LICENSE"], "reviewFindings": []}], "reviewFindings": [],
    }
    files = {
        "SOURCE-REVISION": f"source_revision={SOURCE}\nsource_tree_dirty=false\ntarget={target}\nbundle_kind=full-feature\n".encode(),
        "models/u2netp.onnx": b"fixture model",
        "licenses/Omuse-MIT.txt": b"fixture root license",
        "licenses/rust-dependency-inventory.json": downloads.canonical(inventory),
        "licenses/Rust-THIRD-PARTY-NOTICES.txt": b"Collected exact fixture terms.\n" * 10,
    }
    if target == "linux-x86_64":
        files.update({name: b"fixture payload" for name in ("bin/omuse", "lib/libraw.so", "lib/libonnxruntime.so", "install.sh", "install-app.py", "share/icons/omuse.png", "share/icons/omuse.svg")})
    else:
        files.update({name: b"fixture payload" for name in ("omuse.exe", "lib/libraw.dll", "lib/onnxruntime.dll", "README.txt")})
    return files


def archive(directory, target, files=None, *, additions=(), sums=None):
    directory.mkdir(parents=True, exist_ok=True)
    files = copy.deepcopy(payload(target) if files is None else files)
    files["SHA256SUMS"] = sums if sums is not None else "".join(f"{hashlib.sha256(content).hexdigest()}  ./{name}\n" for name, content in sorted(files.items())).encode()
    destination = directory / downloads.filename(VERSION, target)
    top = "omuse-bundle" if target == "linux-x86_64" else f"omuse-{SOURCE[:12]}-windows-x86_64"
    if target == "linux-x86_64":
        with tarfile.open(destination, "w:gz") as stream:
            for name, content in files.items():
                member = tarfile.TarInfo(f"{top}/{name}")
                member.size = len(content)
                member.mode = 0o755 if name in ("bin/omuse", "install.sh") else 0o644
                stream.addfile(member, io.BytesIO(content))
            for member, content in additions:
                stream.addfile(member, io.BytesIO(content) if content is not None else None)
    else:
        with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED) as stream:
            for name, content in files.items():
                stream.writestr(f"{top}/{name}", content)
            for member, content in additions:
                stream.writestr(member, content)
    return destination


def candidate(directory):
    for target in downloads.TARGETS:
        path = archive(directory / target, target)
        record = bundle.inspect_archive(path, target, SOURCE, VERSION)
        receipt = dict(record, status="passed", checks=sorted(downloads.CHECKS), upgradeBaseline="same-candidate-reinstall")
        (path.parent / "smoke.json").write_bytes(downloads.canonical(receipt))
    return downloads.assemble(directory, SOURCE, VERSION, 123, 321, 2, MAIN)


class WorkflowTests(unittest.TestCase):
    def test_publication_request_uses_explicit_branch_and_exact_checkout(self):
        workflow = (Path(__file__).resolve().parent.parent / downloads.PUBLISH_WORKFLOW).read_text()
        self.assertIn('branches: ["publish-downloads/v*"]', workflow)
        self.assertNotIn("pull_request", workflow)
        self.assertEqual(workflow.count("ref: ${{ github.sha }}"), 2)
        self.assertEqual(workflow.count("contents: write"), 1)
        self.assertIn("--plan-publication", workflow)

    def test_release_push_build_filters_match_exact_source_validation_filters(self):
        workflows = Path(__file__).resolve().parent.parent / ".github/workflows"
        def paths(name):
            # The two reviewed workflows use simple, quoted path allowlists.
            # This checks their coordination contract without a YAML dependency.
            text = (workflows / name).read_text()
            push = re.search(r"(?ms)^  push:\n(.*?)(?=^  [a-z_]+:|\Z)", text)
            self.assertIsNotNone(push)
            values = re.findall(r"^      - ['\"]([^'\"]+)['\"]\s*$", push[1], re.M)
            self.assertTrue(values)
            return set(values)
        self.assertEqual(paths("build-downloads.yml"), paths("rust-validation.yml"))


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def inspect(self, path, target="linux-x86_64"):
        return bundle.inspect_archive(path, target, SOURCE, VERSION)

    def test_both_archive_types_verify_hashes_source_and_notice_inventory(self):
        for target in downloads.TARGETS:
            path = archive(self.root / target, target)
            result = self.inspect(path, target)
            self.assertEqual(result["sha256"], bundle.digest(path))
            self.assertEqual(result["noticeFindings"], 0)
            self.assertEqual(result["sourceRevision"], SOURCE)

    def test_tampered_payload_and_incomplete_checksum_coverage_fail(self):
        files = payload("linux-x86_64")
        sums = "".join(f"{hashlib.sha256(content).hexdigest()}  ./{name}\n" for name, content in sorted(files.items())).encode()
        files["bin/omuse"] = b"changed executable"
        with self.assertRaises(bundle.BundleError):
            self.inspect(archive(self.root, "linux-x86_64", files, sums=sums))
        files = payload("linux-x86_64")
        files["unchecked-file"] = b"unchecked"
        with self.assertRaises(bundle.BundleError):
            self.inspect(archive(self.root, "linux-x86_64", files, sums=sums))

    def test_dirty_wrong_source_and_wrong_target_are_rejected(self):
        original = payload("linux-x86_64")
        for old, new in ((b"false", b"true"), (SOURCE.encode(), MAIN.encode()), (b"linux-x86_64", b"windows-x86_64")):
            files = copy.deepcopy(original)
            files["SOURCE-REVISION"] = files["SOURCE-REVISION"].replace(old, new)
            with self.subTest(change=(old, new)), self.assertRaises(bundle.BundleError):
                self.inspect(archive(self.root, "linux-x86_64", files))

    def test_notice_gate_rejects_top_level_and_per_package_findings_and_empty_inventory(self):
        cases = [{"reviewFindings": [{"name": "fixture", "findings": ["no_legal_text_found"]}]}, {"packages": []}]
        for patch in cases:
            files = payload("linux-x86_64")
            inventory = json.loads(files["licenses/rust-dependency-inventory.json"])
            inventory.update(patch)
            files["licenses/rust-dependency-inventory.json"] = downloads.canonical(inventory)
            with self.assertRaises(bundle.BundleError):
                self.inspect(archive(self.root, "linux-x86_64", files))
        for field, value in (("licenseFiles", []), ("reviewFindings", ["missing_license_declaration"]), ("licenseExpression", None)):
            files = payload("linux-x86_64")
            inventory = json.loads(files["licenses/rust-dependency-inventory.json"])
            inventory["packages"][0][field] = value
            files["licenses/rust-dependency-inventory.json"] = downloads.canonical(inventory)
            with self.assertRaises(bundle.BundleError):
                self.inspect(archive(self.root, "linux-x86_64", files))

    def test_archive_paths_links_case_aliases_and_file_parent_collisions_fail(self):
        for name in ("../escape", "/absolute", "wrong-root/file", "omuse-bundle/../escape", "omuse-bundle/bin/OMUSE", "omuse-bundle/x:stream", "omuse-bundle/CON.txt", "omuse-bundle/bin/omuse/child", "omuse-bundle/Bin/OMUSE/child"):
            member = tarfile.TarInfo(name)
            member.size = 1
            with self.subTest(name=name), self.assertRaises(bundle.BundleError):
                self.inspect(archive(self.root, "linux-x86_64", additions=[(member, b"x")]))
        for member_type in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.CHRTYPE, tarfile.FIFOTYPE):
            member = tarfile.TarInfo("omuse-bundle/redirect")
            member.type = member_type
            member.linkname = "/outside"
            with self.assertRaises(bundle.BundleError):
                self.inspect(archive(self.root, "linux-x86_64", additions=[(member, None)]))

    def test_zip_rejects_backslash_paths_and_symlinks(self):
        top = f"omuse-{SOURCE[:12]}-windows-x86_64"
        for name in (f"{top}/../escape", f"{top}\\backslash", f"{top}/OMUSE.exe"):
            with self.assertRaises(bundle.BundleError):
                self.inspect(archive(self.root, "windows-x86_64", additions=[(name, b"x")]), "windows-x86_64")
        member = zipfile.ZipInfo(f"{top}/link")
        member.create_system = 3
        member.external_attr = (stat.S_IFLNK | 0o777) << 16
        with self.assertRaises(bundle.BundleError):
            self.inspect(archive(self.root, "windows-x86_64", additions=[(member, b"outside")]), "windows-x86_64")

    def test_extraction_never_reuses_an_existing_directory(self):
        path = archive(self.root, "linux-x86_64")
        destination = self.root / "extracted"
        destination.mkdir()
        (destination / "sentinel").write_text("untouched")
        with self.assertRaises(bundle.BundleError):
            bundle.extract_verified(path, "linux-x86_64", SOURCE, destination)
        self.assertEqual((destination / "sentinel").read_text(), "untouched")

    def test_manifest_and_smoke_identity_are_bound_to_both_archives(self):
        manifest = candidate(self.root)
        downloads.check_artifacts(self.root, manifest)
        receipt = self.root / "windows-x86_64" / "smoke.json"
        value = json.loads(receipt.read_text())
        value["status"] = "failed"
        receipt.write_bytes(downloads.canonical(value))
        with self.assertRaises(downloads.DownloadError):
            downloads.check_artifacts(self.root, manifest)
        manifest["assets"][1]["smokeSha256"] = bundle.digest(receipt)
        with self.assertRaises(downloads.DownloadError):
            downloads.check_artifacts(self.root, manifest)

    @unittest.skipUnless(sys.platform.startswith("linux") and shutil.which("cc"), "native Linux compiler required for tiny runtime fixtures")
    def test_native_linux_smoke_runs_the_real_installer_upgrade_and_rollback(self):
        # This is a synthetic executable/library contract test, not application
        # or image-quality evidence. No subprocess or installation step is mocked.
        files = payload("linux-x86_64")
        files["bin/omuse"] = (
            "#!/usr/bin/env python3\nimport json,sys\nfrom pathlib import Path\n"
            f"if sys.argv[1:] == ['--version']: print('Omuse {VERSION}')\n"
            "elif len(sys.argv)==3 and sys.argv[1]=='--self-test':\n"
            " p=Path(sys.argv[2]); p.mkdir(parents=True,exist_ok=True)\n"
            " (p/'results.json').write_text(json.dumps({'status':'passed'}))\n"
            "else: raise SystemExit(2)\n"
        ).encode()
        scripts = Path(__file__).resolve().parent
        files["install.sh"] = (scripts / "install-rust-bundle.sh").read_bytes()
        files["install-app.py"] = (scripts / "install-app.py").read_bytes()
        source = self.root / "runtime-fixture.c"
        source.write_text("int omuse_runtime_fixture(void) { return 1; }\n")
        library = self.root / "runtime-fixture.so"
        subprocess.run(["cc", "-shared", "-fPIC", str(source), "-o", str(library)], check=True, capture_output=True)
        files["lib/libraw.so"] = files["lib/libonnxruntime.so"] = library.read_bytes()
        path = archive(self.root, "linux-x86_64", files)
        record = self.inspect(path)
        evidence = self.root / "native-smoke"
        result = bundle.smoke_archive(path, record, evidence)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(set(result["checks"]), downloads.CHECKS)
        self.assertEqual(result["upgradeBaseline"], "same-candidate-reinstall")
        self.assertTrue((evidence / "rollback.log").is_file())
        self.assertTrue((evidence / "installed prefix" / "bin" / "omuse").is_file())


class FakeGitHub:
    def __init__(self, manifest):
        self.manifest = manifest
        root = downloads.API_ROOT
        body = f"# Omuse {VERSION}\n\nUnsigned experimental previews with bounded automated package checks.\n"
        self.release = {"id": 456, "tag_name": f"v{VERSION}", "target_commitish": SOURCE, "draft": False, "prerelease": True, "body": body, "assets": []}
        self.responses = {
            root: {"full_name": downloads.REPOSITORY, "private": False, "default_branch": "main"},
            f"{root}/git/ref/heads/main": {"object": {"type": "commit", "sha": MAIN}},
            f"{root}/compare/{SOURCE}...{MAIN}": {"status": "ahead", "base_commit": {"sha": SOURCE}, "merge_base_commit": {"sha": SOURCE}},
            f"{root}/compare/{MAIN}...{MAIN}": {"status": "identical", "base_commit": {"sha": MAIN}, "merge_base_commit": {"sha": MAIN}},
            f"{root}/contents/rust/Cargo.toml?ref={SOURCE}": encoded(f'[package]\nname="omuse"\nversion="{VERSION}"\n'),
            f"{root}/contents/{downloads.manifest_path(VERSION)}?ref={MAIN}": encoded(downloads.canonical(manifest).decode()),
            f"{root}/contents/docs/releases/v{VERSION}.md?ref={MAIN}": encoded(body),
            f"{root}/git/ref/tags/v{VERSION}": {"ref": f"refs/tags/v{VERSION}", "object": {"type": "commit", "sha": SOURCE}},
        }
        self.add_run(123, SOURCE, downloads.notes.WORKFLOW, "Omuse Rust validation", downloads.notes.REQUIRED_JOBS | {"Windows build, tests and journey"})
        self.add_run(321, MAIN, downloads.BUILD_WORKFLOW, downloads.BUILD_NAME, {"Build linux-x86_64", "Build windows-x86_64", "Validate exact source", "Assemble immutable download manifest"})

    def add_run(self, identifier, sha, path, name, jobs):
        root = downloads.API_ROOT
        self.responses[f"{root}/actions/runs/{identifier}"] = {
            "id": identifier, "head_sha": sha, "path": path, "name": name, "event": "workflow_dispatch", "head_branch": "main",
            "repository": {"full_name": downloads.REPOSITORY}, "head_repository": {"full_name": downloads.REPOSITORY},
            "status": "completed", "conclusion": "success", "run_attempt": 2,
        }
        self.responses[f"{root}/actions/runs/{identifier}/attempts/2/jobs?per_page=100"] = {"jobs": [
            {"name": job, "head_sha": sha, "status": "completed", "conclusion": "success"} for job in jobs
        ]}

    def request(self, endpoint, *, method="GET", payload=None, missing_ok=False):
        assert method == "GET", "Publisher must not mutate repository, releases or tags through the API"
        root = downloads.API_ROOT
        if endpoint == f"{root}/releases?per_page=100&page=1":
            return [copy.deepcopy(self.release)]
        if endpoint == f"{root}/releases/456":
            return copy.deepcopy(self.release)
        return copy.deepcopy(self.responses[endpoint])


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.manifest = candidate(self.root)
        self.api = FakeGitHub(self.manifest)
        self.uploaded, self.downloaded = [], []
        self.environment = {"GITHUB_ACTIONS": "true", "GITHUB_REPOSITORY": downloads.REPOSITORY,
                            "GITHUB_EVENT_NAME": "workflow_dispatch", "GITHUB_REF": "refs/heads/main",
                            "GITHUB_SHA": MAIN, "GITHUB_WORKFLOW_SHA": MAIN,
                            "GITHUB_WORKFLOW_REF": f"{downloads.REPOSITORY}/{downloads.PUBLISH_WORKFLOW}@refs/heads/main", "GH_TOKEN": "fixture-only"}

    def publication_request(self):
        ref = f"{downloads.PUBLISH_BRANCH}{VERSION}"
        self.environment.update(GITHUB_EVENT_NAME="push", GITHUB_REF=ref,
                                GITHUB_WORKFLOW_REF=f"{downloads.REPOSITORY}/{downloads.PUBLISH_WORKFLOW}@{ref}")
        self.api.responses[f"{downloads.API_ROOT}/git/ref/heads/publish-downloads/v{VERSION}"] = {
            "object": {"type": "commit", "sha": MAIN},
        }

    def download(self, command, environment):
        self.downloaded.append(command)
        target = "linux-x86_64" if "linux-x86_64" in command[command.index("--name") + 1] else "windows-x86_64"
        shutil.copytree(self.root / target, Path(command[command.index("--dir") + 1]), dirs_exist_ok=True)

    def upload(self, command, environment):
        self.assertNotIn("--clobber", command)
        path = Path(command[3])
        self.uploaded.append(path.name)
        self.api.release["assets"].append({"name": path.name, "state": "uploaded", "size": path.stat().st_size, "digest": "sha256:" + bundle.digest(path)})

    def publish(self):
        return downloads.publish(self.api, self.manifest, self.environment, self.download, self.upload)

    def test_publication_uploads_only_exact_declared_assets_and_rerun_is_read_only(self):
        self.assertEqual(self.publish(), "published")
        self.assertEqual(len(self.uploaded), 4)
        self.assertEqual(self.uploaded[-1], f"omuse-{VERSION}-downloads.json")
        self.assertEqual(self.publish(), "unchanged")
        self.assertEqual(len(self.uploaded), 4)
        self.assertEqual(len(self.downloaded), 2)

    def test_explicit_request_branch_publishes_only_reviewed_main_and_reruns_read_only(self):
        self.publication_request()
        self.assertEqual(downloads.plan_publication(self.api, self.environment), downloads.manifest_path(VERSION))
        self.assertEqual(self.uploaded, [])
        self.assertEqual(self.downloaded, [])
        self.assertEqual(self.publish(), "published")
        self.assertEqual(self.publish(), "unchanged")
        self.assertEqual(len(self.uploaded), 4)
        self.assertEqual(self.uploaded[-1], f"omuse-{VERSION}-downloads.json")

    def test_dispatch_plan_needs_explicit_numbered_version(self):
        self.assertEqual(downloads.plan_publication(self.api, self.environment, VERSION), downloads.manifest_path(VERSION))
        for version in (None, "", "main", "v0.8.0", "0.8.0/extra", "00.8.0", "0.8.0\n", True):
            with self.subTest(version=version), self.assertRaises(downloads.DownloadError):
                downloads.plan_publication(self.api, self.environment, version)

    def test_request_version_cannot_differ_from_selected_manifest(self):
        self.publication_request()
        with self.assertRaisesRegex(downloads.DownloadError, "differs from the publication branch"):
            downloads.plan_publication(self.api, self.environment, "0.9.0")
        self.assertEqual(self.uploaded, [])

    def test_foreign_event_ref_workflow_and_unreviewed_commits_cannot_publish(self):
        self.publication_request()
        allowed = self.environment.copy()
        for field, value in (
            ("GITHUB_ACTIONS", "false"), ("GITHUB_REPOSITORY", "other/Omuse"),
            ("GITHUB_EVENT_NAME", "pull_request"), ("GITHUB_REF", "refs/heads/main"),
            ("GITHUB_REF", "refs/heads/release/0.8.0"),
            ("GITHUB_REF", "refs/heads/publish-downloads/v0.8.0/extra"),
            ("GITHUB_WORKFLOW_REF", f"{downloads.REPOSITORY}/other.yml@{allowed['GITHUB_REF']}"),
            ("GITHUB_WORKFLOW_SHA", SOURCE), ("GITHUB_SHA", SOURCE), ("GH_TOKEN", ""),
        ):
            self.environment = dict(allowed, **{field: value})
            with self.subTest(field=field, value=value), self.assertRaises(downloads.DownloadError):
                self.publish()
            self.assertEqual(self.downloaded, [])
            self.assertEqual(self.uploaded, [])

    def test_request_with_modified_workflow_commit_is_not_public_main(self):
        self.publication_request()
        self.environment.update(GITHUB_SHA=SOURCE, GITHUB_WORKFLOW_SHA=SOURCE)
        with self.assertRaisesRegex(downloads.DownloadError, "exact current reviewed public main"):
            self.publish()
        self.assertEqual(self.downloaded, [])
        self.assertEqual(self.uploaded, [])

    def test_request_branch_must_still_identify_its_original_commit(self):
        self.publication_request()
        endpoint = f"{downloads.API_ROOT}/git/ref/heads/publish-downloads/v{VERSION}"
        for response in ({}, {"type": "tag", "sha": MAIN}, {"type": "commit", "sha": SOURCE}):
            self.api.responses[endpoint] = {"object": response}
            with self.subTest(response=response), self.assertRaisesRegex(downloads.DownloadError, "moved or disappeared"):
                self.publish()
            self.assertEqual(self.downloaded, [])
            self.assertEqual(self.uploaded, [])

    def test_main_or_request_moving_during_preparation_blocks_all_uploads(self):
        for branch in ("main", f"publish-downloads/v{VERSION}"):
            self.api = FakeGitHub(self.manifest)
            self.publication_request()
            def moved_download(command, environment):
                self.download(command, environment)
                self.api.responses[f"{downloads.API_ROOT}/git/ref/heads/{branch}"]["object"]["sha"] = SOURCE
            with self.subTest(branch=branch), self.assertRaises(downloads.DownloadError):
                downloads.publish(self.api, self.manifest, self.environment, moved_download, self.upload)
            self.assertEqual(self.uploaded, [])

    def test_partial_upload_resumes_without_replacing_an_asset(self):
        asset = self.manifest["assets"][0]
        self.api.release["assets"] = [{"name": asset["fileName"], "state": "uploaded", "size": asset["size"], "digest": "sha256:" + asset["sha256"]}]
        self.assertEqual(self.publish(), "published")
        self.assertEqual(len(self.uploaded), 3)
        self.assertNotIn(asset["fileName"], self.uploaded)

    def test_changed_or_unlisted_existing_asset_fails_before_download_or_write(self):
        expected = self.manifest["assets"][0]
        for name, digest in (("unreviewed.exe", expected["sha256"]), (expected["fileName"], "f" * 64)):
            self.api.release["assets"] = [{"name": name, "state": "uploaded", "size": expected["size"], "digest": "sha256:" + digest}]
            with self.assertRaises(downloads.DownloadError):
                self.publish()
            self.assertEqual(self.uploaded, [])
            self.assertEqual(self.downloaded, [])

    def test_wrong_tag_source_failed_ci_and_restarted_attempt_are_rejected(self):
        root = downloads.API_ROOT
        changes = [
            (f"{root}/git/ref/tags/v{VERSION}", "object", {"type": "commit", "sha": MAIN}),
            (f"{root}/actions/runs/123", "conclusion", "failure"),
            (f"{root}/actions/runs/321", "run_attempt", 3),
            (f"{root}/actions/runs/321", "head_branch", "unreviewed-branch"),
        ]
        for endpoint, key, value in changes:
            original = copy.deepcopy(self.api.responses[endpoint])
            self.api.responses[endpoint][key] = value
            with self.subTest(key=key), self.assertRaises(downloads.DownloadError):
                self.publish()
            self.api.responses[endpoint] = original
            self.assertEqual(self.uploaded, [])

    def test_missing_windows_validation_and_stale_manifest_block_publication(self):
        root = downloads.API_ROOT
        endpoint = f"{root}/actions/runs/123/attempts/2/jobs?per_page=100"
        self.api.responses[endpoint]["jobs"] = [job for job in self.api.responses[endpoint]["jobs"] if not job["name"].startswith("Windows")]
        with self.assertRaises(downloads.DownloadError):
            self.publish()
        self.api = FakeGitHub(self.manifest)
        endpoint = f"{root}/contents/{downloads.manifest_path(VERSION)}?ref={MAIN}"
        self.api.responses[endpoint] = encoded(downloads.canonical(self.manifest).decode() + " ")
        with self.assertRaises(downloads.DownloadError):
            self.publish()
        self.assertEqual(self.uploaded, [])

    def test_exact_release_branch_push_can_build_before_merging_and_publish_afterward(self):
        root = downloads.API_ROOT
        environment = {"GITHUB_ACTIONS": "true", "GITHUB_REPOSITORY": downloads.REPOSITORY,
                       "GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/release/0.8.0", "GITHUB_SHA": SOURCE,
                       "GITHUB_WORKFLOW_REF": f"{downloads.REPOSITORY}/{downloads.BUILD_WORKFLOW}@refs/heads/release/0.8.0"}
        self.api.responses[f"{root}/git/ref/heads/release/0.8.0"] = {"object": {"type": "commit", "sha": SOURCE}}
        self.api.responses[f"{root}/compare/{SOURCE}...{SOURCE}"] = {"status": "identical", "base_commit": {"sha": SOURCE}, "merge_base_commit": {"sha": SOURCE}}
        # The candidate may not be in main yet. Planning must not require it.
        main_comparison = self.api.responses.pop(f"{root}/compare/{SOURCE}...{MAIN}")
        self.assertEqual(downloads.plan_release_push(self.api, SOURCE, environment), VERSION)
        for key, value in (("GITHUB_EVENT_NAME", "pull_request"), ("GITHUB_REPOSITORY", "other/Omuse"),
                           ("GITHUB_SHA", MAIN), ("GITHUB_REF", "refs/heads/feature/unreviewed")):
            with self.subTest(key=key), self.assertRaises(downloads.DownloadError):
                downloads.plan_release_push(self.api, SOURCE, dict(environment, **{key: value}))
        self.api.responses[f"{root}/compare/{SOURCE}...{MAIN}"] = main_comparison
        self.manifest["buildWorkflowRevision"] = SOURCE
        self.api.responses[f"{root}/contents/{downloads.manifest_path(VERSION)}?ref={MAIN}"] = encoded(downloads.canonical(self.manifest).decode())
        self.api.add_run(321, SOURCE, downloads.BUILD_WORKFLOW, downloads.BUILD_NAME,
                         {"Build linux-x86_64", "Build windows-x86_64", "Validate exact source", "Assemble immutable download manifest"})
        run = self.api.responses[f"{root}/actions/runs/321"]
        run.update(event="push", head_branch="release/0.8.0")
        self.assertEqual(self.publish(), "published")

    def test_validation_wait_binds_successful_exact_source_and_has_a_finite_deadline(self):
        root = downloads.API_ROOT
        endpoint = f"{root}/actions/workflows/rust-validation.yml/runs?head_sha={SOURCE}&per_page=100"
        run = self.api.responses[f"{root}/actions/runs/123"]
        run.update(status="in_progress", conclusion=None)
        self.api.responses[endpoint] = {"workflow_runs": [run]}
        seconds = [0]
        def finish(delay):
            seconds[0] += delay
            run.update(status="completed", conclusion="success")
        self.assertEqual(downloads.wait_validation(self.api, SOURCE, timeout=30, sleep=finish, clock=lambda: seconds[0]), 123)
        self.assertEqual(seconds[0], 20)
        run["conclusion"] = "failure"
        with self.assertRaises(downloads.DownloadError):
            downloads.wait_validation(self.api, SOURCE, timeout=30, sleep=finish, clock=lambda: seconds[0])
        self.api.responses[endpoint] = {"workflow_runs": [dict(run, head_sha=MAIN)]}
        with self.assertRaisesRegex(downloads.DownloadError, "did not complete"):
            downloads.wait_validation(self.api, SOURCE, timeout=1, sleep=finish, clock=lambda: seconds[0])

    def test_release_push_cannot_package_a_different_source_or_skip_validation_job(self):
        endpoint = f"{downloads.API_ROOT}/actions/runs/321"
        self.api.responses[endpoint].update(event="push", head_branch="release/0.8.0")
        with self.assertRaisesRegex(downloads.DownloadError, "exact release-branch push"):
            self.publish()
        self.api = FakeGitHub(self.manifest)
        endpoint += "/attempts/2/jobs?per_page=100"
        self.api.responses[endpoint]["jobs"] = [job for job in self.api.responses[endpoint]["jobs"] if job["name"] != "Validate exact source"]
        with self.assertRaisesRegex(downloads.DownloadError, "Required job did not pass"):
            self.publish()
        self.assertEqual(self.uploaded, [])

    def test_tampered_downloads_fail_before_any_upload(self):
        path = self.root / "linux-x86_64" / self.manifest["assets"][0]["fileName"]
        path.write_bytes(path.read_bytes() + b"tampered archive")
        with self.assertRaises(downloads.DownloadError):
            self.publish()
        self.assertEqual(self.uploaded, [])

    def test_release_or_tag_changes_during_download_block_the_first_write(self):
        for field in ("body", "target_commitish", "tag"):
            self.api = FakeGitHub(self.manifest)
            def changed_download(command, environment):
                self.download(command, environment)
                if field == "tag":
                    self.api.responses[f"{downloads.API_ROOT}/git/ref/tags/v{VERSION}"]["object"]["sha"] = MAIN
                else:
                    self.api.release[field] = "changed during preparation"
            with self.subTest(field=field), self.assertRaises(downloads.DownloadError):
                downloads.publish(self.api, self.manifest, self.environment, changed_download, self.upload)
            self.assertEqual(self.uploaded, [])

    def test_interrupted_upload_resumes_by_readback_without_clobbering(self):
        def uncertain_upload(command, environment):
            self.upload(command, environment)
            raise downloads.DownloadError("simulated lost response")
        with self.assertRaises(downloads.DownloadError):
            downloads.publish(self.api, self.manifest, self.environment, self.download, uncertain_upload)
        first = self.uploaded[0]
        self.assertEqual(self.publish(), "published")
        self.assertEqual(self.uploaded.count(first), 1)
        self.assertEqual(len(self.uploaded), 4)

    def test_unqualified_metadata_and_local_publisher_context_are_rejected(self):
        for key, value in (("prerelease", False), ("signed", True), ("qualification", "stable"), ("sourceRevision", "main"), ("buildRun", True), ("limitations", [])):
            changed = copy.deepcopy(self.manifest)
            changed[key] = value
            with self.subTest(key=key), self.assertRaises(downloads.DownloadError):
                downloads.validate_manifest(changed)
        self.environment["GITHUB_EVENT_NAME"] = "pull_request"
        with self.assertRaises(downloads.DownloadError):
            self.publish()
        self.assertEqual(self.uploaded, [])

    def test_source_publisher_recognizes_only_manifest_bound_downloads(self):
        self.publish()
        manifest = {"sourceRevision": SOURCE, "version": VERSION, "tag": f"v{VERSION}", "title": "Omuse " + VERSION, "prerelease": True}
        release = copy.deepcopy(self.api.release)
        release["name"] = manifest["title"]
        downloads.notes.check_existing(release, manifest, release["body"], SOURCE, self.manifest)
        release["assets"][0]["digest"] = "sha256:" + "f" * 64
        with self.assertRaises(downloads.notes.ReleaseError):
            downloads.notes.check_existing(release, manifest, release["body"], SOURCE, self.manifest)


if __name__ == "__main__":
    unittest.main()
