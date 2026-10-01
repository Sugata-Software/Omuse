#!/usr/bin/env python3
"""Offline checks for the bounded, documented v0.6.0 evidence-link correction."""

import base64
import copy
import importlib.util
import json
from pathlib import Path
import unittest


SPEC = importlib.util.spec_from_file_location("correction", Path(__file__).with_name("correct-release-060-links.py"))
correction = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(correction)
ROOT = correction.release_notes.API_ROOT
MAIN = "b" * 40
MANIFEST = {
    "schemaVersion": 1, "version": "0.6.0", "tag": "v0.6.0",
    "title": "Omuse 0.6.0 — More control over every edit", "date": "2026-10-01",
    "prerelease": True, "sourceRevision": correction.SOURCE,
    "notes": "docs/releases/v0.6.0.md", "validationRun": 36784517002,
}
NOTES = (Path(__file__).resolve().parents[1] / MANIFEST["notes"]).read_text()
ORIGINAL = NOTES.removesuffix(correction.NOTICE)
for filename in ("release-060-qualification.md", "release-060-receipts.json"):
    ORIGINAL = ORIGINAL.replace(
        f"](https://github.com/Sugata-Software/Omuse/blob/{correction.RECORDS}/docs/{filename})",
        f"](../{filename})",
    )


def encoded(text):
    return {"type": "file", "encoding": "base64", "content": base64.b64encode(text.encode()).decode()}


class FakeAPI:
    def __init__(self):
        self.writes = []
        self.discard_patch = False
        self.release = {
            "id": correction.RELEASE_ID, "tag_name": MANIFEST["tag"],
            "target_commitish": correction.SOURCE, "name": MANIFEST["title"],
            "body": ORIGINAL, "draft": False, "prerelease": True, "assets": [],
        }
        self.responses = {
            f"{ROOT}/git/ref/tags/v0.6.0": {
                "ref": "refs/tags/v0.6.0", "object": {"type": "commit", "sha": correction.SOURCE},
            },
            f"{ROOT}/git/ref/heads/main": {"object": {"type": "commit", "sha": MAIN}},
            f"{ROOT}/contents/docs/releases/latest.json?ref={MAIN}": encoded(json.dumps(MANIFEST)),
            f"{ROOT}/contents/{MANIFEST['notes']}?ref={MAIN}": encoded(NOTES),
        }

    def request(self, endpoint, *, method="GET", payload=None, missing_ok=False):
        if endpoint == f"{ROOT}/releases/{correction.RELEASE_ID}":
            if method == "PATCH":
                assert set(payload) == {"body"}
                self.writes.append(copy.deepcopy(payload))
                if not self.discard_patch:
                    self.release.update(payload)
            else:
                assert method == "GET"
            return copy.deepcopy(self.release)
        assert method == "GET"
        return copy.deepcopy(self.responses[endpoint])


class CorrectionTests(unittest.TestCase):
    def assert_refused(self, api, notes=NOTES):
        with self.assertRaises(correction.release_notes.ReleaseError):
            correction.repair(api, MANIFEST, notes, MAIN)
        self.assertEqual(api.writes, [])

    def test_only_reviewed_links_and_visible_notice_change(self):
        self.assertEqual(correction.digest(ORIGINAL), correction.ORIGINAL_SHA256)
        self.assertEqual(correction.corrected_body(ORIGINAL), NOTES)

    def test_one_body_only_patch_and_retry_is_noop(self):
        api = FakeAPI()
        self.assertEqual(correction.repair(api, MANIFEST, NOTES, MAIN), "documentation links corrected")
        self.assertEqual(api.writes, [{"body": NOTES}])
        self.assertEqual(correction.repair(api, MANIFEST, NOTES, MAIN), "already corrected")
        self.assertEqual(len(api.writes), 1)

    def test_unexpected_existing_body_is_never_overwritten(self):
        api = FakeAPI()
        api.release["body"] += "\nAn external amendment.\n"
        self.assert_refused(api)

    def test_unreviewed_checkout_is_refused_before_write(self):
        self.assert_refused(FakeAPI(), NOTES + "\nUnreviewed text\n")

    def test_changed_identity_flags_or_assets_are_refused(self):
        for key, value in [("id", 123), ("tag_name", "v0.5.0"), ("target_commitish", "c" * 40),
                           ("name", "Changed title"), ("draft", True), ("prerelease", False),
                           ("assets", [{"name": "binary"}])]:
            with self.subTest(key=key):
                api = FakeAPI()
                api.release[key] = value
                self.assert_refused(api)

    def test_changed_actual_tag_is_refused(self):
        api = FakeAPI()
        api.responses[f"{ROOT}/git/ref/tags/v0.6.0"]["object"]["sha"] = "c" * 40
        self.assert_refused(api)

    def test_stale_main_or_notes_are_refused(self):
        for endpoint, value in [
            (f"{ROOT}/git/ref/heads/main", {"object": {"type": "commit", "sha": "c" * 40}}),
            (f"{ROOT}/contents/{MANIFEST['notes']}?ref={MAIN}", encoded(ORIGINAL)),
            (f"{ROOT}/contents/docs/releases/latest.json?ref={MAIN}", encoded(json.dumps({}))),
        ]:
            with self.subTest(endpoint=endpoint):
                api = FakeAPI()
                api.responses[endpoint] = value
                self.assert_refused(api)

    def test_unsuccessful_write_is_detected_in_readback(self):
        api = FakeAPI()
        api.discard_patch = True
        with self.assertRaises(correction.release_notes.ReleaseError):
            correction.repair(api, MANIFEST, NOTES, MAIN)
        self.assertEqual(len(api.writes), 1)


if __name__ == "__main__":
    unittest.main()
