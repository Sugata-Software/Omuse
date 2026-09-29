#!/usr/bin/env python3
"""Offline regression tests: this suite never connects to or publishes on GitHub."""

import base64
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock


SPEC = importlib.util.spec_from_file_location("release_notes", Path(__file__).with_name("release-notes.py"))
release_notes = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release_notes)
SOURCE = "a" * 40
MAIN = "b" * 40
ROOT = release_notes.API_ROOT
NOTES = "# Omuse 0.1.0\n\nFirst public source release. No binary downloads are attached.\n"
MANIFEST = {
    "schemaVersion": 1,
    "version": "0.1.0",
    "tag": "v0.1.0",
    "title": "Omuse 0.1.0 — First public source release",
    "date": "2026-09-29",
    "prerelease": True,
    "sourceRevision": SOURCE,
    "notes": "docs/releases/v0.1.0.md",
    "validationRun": 123,
}
CARGO = '[package]\nname = "omuse"\nversion = "0.1.0"\n'
INSTALL = f"#!/bin/bash\nreadonly omuse_release_revision={SOURCE}\n"
RUN_PATH = f"{ROOT}/actions/runs/123"
JOBS_PATH = f"{RUN_PATH}/attempts/2/jobs?per_page=100"
TAG_PATH = f"{ROOT}/git/ref/tags/v0.1.0"
LIST_PATH = f"{ROOT}/releases?per_page=100&page=1"


def encoded_file(text):
    return {"type": "file", "encoding": "base64", "content": base64.b64encode(text.encode()).decode()}


def matching_release():
    return {
        "id": 456,
        "tag_name": MANIFEST["tag"],
        "target_commitish": SOURCE,
        "name": MANIFEST["title"],
        "body": NOTES,
        "draft": False,
        "prerelease": True,
        "assets": [],
    }


def tag_ref(sha=SOURCE, kind="commit"):
    return {"ref": "refs/tags/v0.1.0", "object": {"type": kind, "sha": sha}}


class FakeGitHub:
    def __init__(self):
        self.writes = []
        self.reads = []
        self.responses = {
            ROOT: {"full_name": release_notes.REPOSITORY, "private": False, "default_branch": "main"},
            f"{ROOT}/git/ref/heads/main": {"object": {"type": "commit", "sha": MAIN}},
            f"{ROOT}/compare/{SOURCE}...{MAIN}": {
                "status": "ahead", "base_commit": {"sha": SOURCE}, "merge_base_commit": {"sha": SOURCE},
            },
            f"{ROOT}/contents/{release_notes.MANIFEST}?ref={MAIN}": encoded_file(json.dumps(MANIFEST)),
            f"{ROOT}/contents/{MANIFEST['notes']}?ref={MAIN}": encoded_file(NOTES),
            f"{ROOT}/contents/install.sh?ref={MAIN}": encoded_file(INSTALL),
            f"{ROOT}/contents/rust/Cargo.toml?ref={SOURCE}": encoded_file(CARGO),
            RUN_PATH: {
                "id": 123, "head_sha": SOURCE, "name": "Omuse Rust validation",
                "path": release_notes.WORKFLOW, "event": "push",
                "repository": {"full_name": release_notes.REPOSITORY},
                "head_repository": {"full_name": release_notes.REPOSITORY},
                "status": "completed", "conclusion": "success", "run_attempt": 2,
            },
            JOBS_PATH: {"jobs": [
                {"name": name, "head_sha": SOURCE, "status": "completed", "conclusion": "success"}
                for name in sorted(release_notes.REQUIRED_JOBS)
            ]},
            TAG_PATH: None,
            LIST_PATH: [],
        }

    def request(self, endpoint, *, method="GET", payload=None, missing_ok=False):
        if method == "POST":
            if endpoint != f"{ROOT}/releases":
                raise AssertionError(f"Unexpected mutation: {method} {endpoint}")
            self.writes.append(copy.deepcopy(payload))
            release = matching_release()
            release.update(payload)
            self.responses[LIST_PATH] = [release]
            self.responses[f"{ROOT}/releases/456"] = release
            self.responses[TAG_PATH] = tag_ref()
            return copy.deepcopy(release)
        if method != "GET":
            raise AssertionError(f"Unexpected mutation: {method} {endpoint}")
        self.reads.append(endpoint)
        result = self.responses[endpoint]
        if result is None and not missing_ok:
            raise AssertionError(f"Missing unexpected response: {endpoint}")
        return copy.deepcopy(result)


class LocalDeclarationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.manifest = copy.deepcopy(MANIFEST)
        self.write(release_notes.MANIFEST, json.dumps(self.manifest))
        self.write(MANIFEST["notes"], NOTES)
        self.write("rust/Cargo.toml", CARGO)
        self.write("install.sh", INSTALL)

    def write(self, path, text):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text, encoding="utf-8")

    def test_valid_inputs_pass_without_gh_or_network(self):
        with mock.patch.object(release_notes.subprocess, "run", side_effect=AssertionError("No subprocess allowed")):
            self.assertEqual(release_notes.check_local(self.root), (MANIFEST, NOTES))

    def test_schema_path_types_and_version_mismatches_are_rejected(self):
        cases = [
            ("schemaVersion", True), ("schemaVersion", 2), ("version", "01.1.0"),
            ("tag", "v0.2.0"), ("notes", "../../secret"), ("notes", "docs/releases/v0.2.0.md"),
            ("sourceRevision", "main"), ("date", "2026-02-30"), ("date", "2026-9-29"),
            ("prerelease", False), ("prerelease", 1), ("validationRun", True),
            ("validationRun", -1), ("title", "Another project"), ("title", "Omuse 0.1.0\nInjected"),
        ]
        for field, value in cases:
            with self.subTest(field=field, value=value):
                manifest = dict(MANIFEST, **{field: value})
                self.write(release_notes.MANIFEST, json.dumps(manifest))
                with self.assertRaises(release_notes.ReleaseError):
                    release_notes.check_local(self.root)

    def test_duplicate_and_unknown_fields_are_rejected(self):
        for contents in [json.dumps(MANIFEST)[:-1] + ', "schemaVersion": 1}', json.dumps(dict(MANIFEST, assets=[]))]:
            self.write(release_notes.MANIFEST, contents)
            with self.assertRaises(release_notes.ReleaseError):
                release_notes.check_local(self.root)

    def test_cargo_version_and_installer_source_must_match(self):
        for path, text in [
            ("rust/Cargo.toml", CARGO.replace("0.1.0", "0.2.0")),
            ("rust/Cargo.toml", CARGO.replace('"omuse"', '"another-app"')),
            ("install.sh", INSTALL.replace(SOURCE, MAIN)),
            ("install.sh", INSTALL + INSTALL),
        ]:
            with self.subTest(path=path, text=text):
                self.write("rust/Cargo.toml", CARGO)
                self.write("install.sh", INSTALL)
                self.write(path, text)
                with self.assertRaises(release_notes.ReleaseError):
                    release_notes.check_local(self.root)

    def test_notes_must_be_bounded_versioned_text_not_a_symlink(self):
        for notes in ["", "# Another release\n", NOTES + "\x00", NOTES + "a" * 60_000]:
            self.write(MANIFEST["notes"], notes)
            with self.assertRaises(release_notes.ReleaseError):
                release_notes.check_local(self.root)
        destination = self.root / MANIFEST["notes"]
        destination.unlink()
        destination.symlink_to(self.root / "install.sh")
        with self.assertRaises(release_notes.ReleaseError):
            release_notes.check_local(self.root)


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.api = FakeGitHub()

    def assert_refused_without_writes(self):
        with self.assertRaises(release_notes.ReleaseError):
            release_notes.publish(self.api, MANIFEST, NOTES)
        self.assertEqual(self.api.writes, [])

    def test_publish_uses_one_create_request_with_exact_source_no_assets_or_latest(self):
        self.assertEqual(release_notes.publish(self.api, MANIFEST, NOTES), "published")
        self.assertEqual(len(self.api.writes), 1)
        payload = self.api.writes[0]
        self.assertEqual(payload["target_commitish"], SOURCE)
        self.assertEqual(payload["body"], NOTES)
        self.assertEqual(payload["make_latest"], "false")
        self.assertTrue(payload["prerelease"])
        self.assertFalse(payload["generate_release_notes"])
        self.assertNotIn("assets", payload)
        self.assertIn(f"{ROOT}/releases/456", self.api.reads)

    def test_second_identical_publication_is_a_read_only_noop(self):
        release_notes.publish(self.api, MANIFEST, NOTES)
        self.assertEqual(release_notes.publish(self.api, MANIFEST, NOTES), "unchanged")
        self.assertEqual(len(self.api.writes), 1)

    def test_read_only_verification_never_publishes(self):
        self.assertIsNone(release_notes.verify_remote(self.api, MANIFEST, NOTES))
        self.assertEqual(self.api.writes, [])

    def test_existing_matching_tag_can_be_used_without_moving_it(self):
        self.api.responses[TAG_PATH] = tag_ref()
        self.assertEqual(release_notes.publish(self.api, MANIFEST, NOTES), "published")
        self.assertEqual(len(self.api.writes), 1)

    def test_existing_tag_at_another_commit_stops_before_any_write(self):
        self.api.responses[TAG_PATH] = tag_ref(MAIN)
        self.assert_refused_without_writes()

    def test_annotated_tag_is_resolved_and_cannot_hide_a_different_commit(self):
        annotated = "c" * 40
        self.api.responses[TAG_PATH] = tag_ref(annotated, "tag")
        self.api.responses[f"{ROOT}/git/tags/{annotated}"] = {"object": {"type": "commit", "sha": SOURCE}}
        self.assertEqual(release_notes.tag_target(self.api, MANIFEST["tag"]), SOURCE)
        self.api.responses[f"{ROOT}/git/tags/{annotated}"]["object"]["sha"] = MAIN
        self.assert_refused_without_writes()

    def test_tag_cycle_is_rejected(self):
        annotated = "c" * 40
        self.api.responses[TAG_PATH] = tag_ref(annotated, "tag")
        self.api.responses[f"{ROOT}/git/tags/{annotated}"] = {"object": {"type": "tag", "sha": annotated}}
        self.assert_refused_without_writes()

    def test_changed_or_draft_release_is_never_overwritten(self):
        for field, value in [
            ("body", NOTES + "Changed"), ("name", "Changed title"),
            ("target_commitish", MAIN), ("prerelease", False), ("draft", True),
            ("assets", [{"name": "unqualified-binary.tar.gz"}]),
        ]:
            with self.subTest(field=field):
                self.api = FakeGitHub()
                self.api.responses[TAG_PATH] = tag_ref()
                existing = matching_release()
                existing[field] = value
                self.api.responses[LIST_PATH] = [existing]
                self.assert_refused_without_writes()

    def test_existing_release_without_its_tag_fails(self):
        self.api.responses[LIST_PATH] = [matching_release()]
        self.assert_refused_without_writes()

    def test_failed_running_skipped_or_wrong_source_validation_never_publishes(self):
        for field, value in [
            ("conclusion", "failure"), ("conclusion", "skipped"), ("status", "in_progress"),
            ("head_sha", MAIN), ("event", "pull_request"), ("name", "Unrelated check"),
            ("path", ".github/workflows/project-guide.yml"),
            ("head_repository", {"full_name": "untrusted/fork"}),
        ]:
            with self.subTest(field=field, value=value):
                self.api = FakeGitHub()
                self.api.responses[RUN_PATH][field] = value
                self.assert_refused_without_writes()

    def test_successful_run_with_skipped_missing_or_other_source_job_fails(self):
        for mutation in ("skip", "missing", "different-source"):
            with self.subTest(mutation=mutation):
                self.api = FakeGitHub()
                jobs = self.api.responses[JOBS_PATH]["jobs"]
                if mutation == "skip":
                    jobs[0]["conclusion"] = "skipped"
                elif mutation == "missing":
                    jobs.pop()
                else:
                    jobs[0]["head_sha"] = MAIN
                self.assert_refused_without_writes()

    def test_diverged_public_history_is_rejected(self):
        self.api.responses[f"{ROOT}/compare/{SOURCE}...{MAIN}"]["status"] = "diverged"
        self.assert_refused_without_writes()

    def test_stale_notes_manifest_pin_or_source_version_is_rejected(self):
        replacements = [
            (release_notes.MANIFEST, MAIN, json.dumps(dict(MANIFEST, validationRun=124))),
            (MANIFEST["notes"], MAIN, NOTES + "Changed on main\n"),
            ("install.sh", MAIN, INSTALL.replace(SOURCE, MAIN)),
            ("rust/Cargo.toml", SOURCE, CARGO.replace("0.1.0", "0.2.0")),
        ]
        for path, revision, text in replacements:
            with self.subTest(path=path):
                self.api = FakeGitHub()
                self.api.responses[f"{ROOT}/contents/{path}?ref={revision}"] = encoded_file(text)
                self.assert_refused_without_writes()

    def test_drafts_and_older_releases_are_checked_across_pages(self):
        self.api.responses[TAG_PATH] = tag_ref()
        self.api.responses[LIST_PATH] = [{"tag_name": f"v9.0.{number}"} for number in range(100)]
        draft = dict(matching_release(), draft=True)
        self.api.responses[f"{ROOT}/releases?per_page=100&page=2"] = [draft]
        self.assert_refused_without_writes()

    def test_changed_tag_during_creation_is_detected_by_readback(self):
        original_request = self.api.request
        def request(endpoint, **options):
            result = original_request(endpoint, **options)
            if options.get("method") == "POST":
                self.api.responses[TAG_PATH] = tag_ref(MAIN)
            return result
        self.api.request = request
        with self.assertRaisesRegex(release_notes.ReleaseError, "different source"):
            release_notes.publish(self.api, MANIFEST, NOTES)
        self.assertEqual(len(self.api.writes), 1)


class TransportAndContextTests(unittest.TestCase):
    def test_http_404_is_optional_only_for_get_and_403_is_not_absence(self):
        for status in (404, 403):
            response = subprocess.CompletedProcess([], 1, f"HTTP/2.0 {status} Error\r\n\r\n{{}}\n", "private diagnostic")
            with mock.patch.object(release_notes.subprocess, "run", return_value=response):
                if status == 404:
                    self.assertIsNone(release_notes.GitHub().request(TAG_PATH, missing_ok=True))
                else:
                    with self.assertRaisesRegex(release_notes.ReleaseError, "HTTP 403") as error:
                        release_notes.GitHub().request(TAG_PATH, missing_ok=True)
                    self.assertNotIn("private diagnostic", str(error.exception))

    def test_json_body_goes_to_stdin_and_token_stays_in_environment(self):
        response = subprocess.CompletedProcess([], 0, 'HTTP/2.0 201 Created\nContent-Type: application/json\n\n{"id":456}', "")
        with mock.patch.dict(os.environ, {"GH_TOKEN": "test-token", "GH_DEBUG": "api"}):
            with mock.patch.object(release_notes.subprocess, "run", return_value=response) as run:
                payload = {"body": "Markdown `literal`\n$(literal)\n"}
                self.assertEqual(release_notes.GitHub().request(f"{ROOT}/releases", method="POST", payload=payload), {"id": 456})
                arguments, options = run.call_args
                self.assertNotIn("test-token", " ".join(arguments[0]))
                self.assertEqual(json.loads(options["input"]), payload)
                self.assertEqual(options["env"]["GH_TOKEN"], "test-token")
                self.assertNotIn("GH_DEBUG", options["env"])
                self.assertNotIn("shell", options)

    def test_timeout_does_not_retry_an_uncertain_create(self):
        with mock.patch.object(release_notes.subprocess, "run", side_effect=subprocess.TimeoutExpired("gh", 60)) as run:
            with self.assertRaisesRegex(release_notes.ReleaseError, "rerun to inspect"):
                release_notes.GitHub().request(f"{ROOT}/releases", method="POST", payload={})
            self.assertEqual(run.call_count, 1)

    def test_only_expected_main_push_environment_can_publish(self):
        allowed = {
            "GITHUB_ACTIONS": "true", "GITHUB_REPOSITORY": release_notes.REPOSITORY,
            "GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/main", "GH_TOKEN": "test-token",
        }
        release_notes.require_publisher_context(allowed)
        for field, value in [
            ("GITHUB_ACTIONS", "false"), ("GITHUB_EVENT_NAME", "pull_request"),
            ("GITHUB_REPOSITORY", "another/fork"), ("GITHUB_REF", "refs/heads/release/test"), ("GH_TOKEN", ""),
        ]:
            with self.subTest(field=field):
                with self.assertRaises(release_notes.ReleaseError):
                    release_notes.require_publisher_context(dict(allowed, **{field: value}))


class PublicationIntentTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.event_path = self.root / "push.json"
        self.event_path.write_text(json.dumps({"before": SOURCE, "after": MAIN}))
        self.environment = {
            "GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/main",
            "GITHUB_REPOSITORY": release_notes.REPOSITORY, "GITHUB_SHA": MAIN,
            "GITHUB_EVENT_PATH": str(self.event_path),
        }

    def test_complete_push_range_requests_only_declared_or_first_workflow_release(self):
        for changed, expected in [
            ("M\tdocs/releases/latest.json\n", True),
            ("A\t.github/workflows/release-notes.yml\n", True),
            ("M\t.github/workflows/release-notes.yml\n", False),
            ("M\tscripts/release-notes.py\n", False),
            ("M\tdocs/releases/v0.1.0.md\n", False),
            ("", False),
        ]:
            with self.subTest(changed=changed):
                response = subprocess.CompletedProcess([], 0, changed, "")
                with mock.patch.object(release_notes.subprocess, "run", return_value=response) as run:
                    self.assertEqual(release_notes.publication_requested(self.root, self.environment), expected)
                    self.assertIn(SOURCE, run.call_args.args[0])
                    self.assertIn(MAIN, run.call_args.args[0])
                    self.assertIn("--no-renames", run.call_args.args[0])

    def test_pull_requests_and_other_branches_never_request_publication(self):
        for field, value in [("GITHUB_EVENT_NAME", "pull_request"), ("GITHUB_REF", "refs/heads/topic"), ("GITHUB_REPOSITORY", "another/fork")]:
            with mock.patch.object(release_notes.subprocess, "run", side_effect=AssertionError("No Git command required")):
                self.assertFalse(release_notes.publication_requested(self.root, dict(self.environment, **{field: value})))

    def test_missing_commit_range_fails_instead_of_guessing(self):
        response = subprocess.CompletedProcess([], 128, "", "missing object")
        with mock.patch.object(release_notes.subprocess, "run", return_value=response):
            with self.assertRaisesRegex(release_notes.ReleaseError, "complete reviewed push range"):
                release_notes.publication_requested(self.root, self.environment)

    def test_first_main_push_is_explicitly_handled(self):
        self.event_path.write_text(json.dumps({"before": "0" * 40, "after": MAIN}))
        with mock.patch.object(release_notes.subprocess, "run", side_effect=AssertionError("No prior revision exists")):
            self.assertTrue(release_notes.publication_requested(self.root, self.environment))


if __name__ == "__main__":
    unittest.main()
