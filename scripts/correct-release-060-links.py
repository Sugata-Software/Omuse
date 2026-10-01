#!/usr/bin/env python3
"""Apply the explicitly recorded v0.6.0 documentation-link correction once.

This is not a general release editor. Both complete note bodies are pinned by
SHA-256. Only the two links and the visible correction notice may change.
Tags, source, title, dates, release flags and assets are never written.
"""

import argparse
import hashlib
import importlib.util
import os
from pathlib import Path
import sys


SPEC = importlib.util.spec_from_file_location("release_notes", Path(__file__).with_name("release-notes.py"))
release_notes = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release_notes)
RELEASE_ID = 400490197
SOURCE = "a8b7ac70e7513d305a671673a347eecaf2d6cc4c"
RECORDS = "5729919b3a23a006701acc86d2ad78b92d5e0351"
ORIGINAL_SHA256 = "93bd681c272eb2df8816559f95dd5fe5186fe60fc2e37c3e12dd872f9bbc72f6"
CORRECTED_SHA256 = "5414e24a2b88ed70871ef0a29e692422c102e8a9751f45b4f4848d693dda7014"
NOTICE = (
    "\n**Documentation correction, 1 October 2026:** the qualification and receipt links\n"
    "now point directly to the published records. Release source, results and downloads\n"
    "are unchanged.\n"
)
require = release_notes.require


def digest(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def corrected_body(original):
    require(digest(original) == ORIGINAL_SHA256, "Published notes changed; correction refused")
    result = original
    for name in ("release-060-qualification.md", "release-060-receipts.json"):
        old = f"](../{name})"
        require(result.count(old) == 1, "Expected evidence link is missing or duplicated")
        result = result.replace(old, f"](https://github.com/Sugata-Software/Omuse/blob/{RECORDS}/docs/{name})")
    result += NOTICE
    require(digest(result) == CORRECTED_SHA256, "Correction differs from the reviewed content")
    return result


def validate_declaration(manifest, notes):
    require(manifest["version"] == "0.6.0" and manifest["tag"] == "v0.6.0"
            and manifest["sourceRevision"] == SOURCE and manifest["validationRun"] == 36784517002,
            "This correction applies only to the qualified Omuse 0.6.0 release")
    require(digest(notes) == CORRECTED_SHA256, "Checkout notes differ from the reviewed correction")


def repair(api, manifest, notes, expected_main):
    validate_declaration(manifest, notes)
    root = release_notes.API_ROOT
    endpoint = f"{root}/releases/{RELEASE_ID}"
    current = api.request(endpoint)
    require(current.get("id") == RELEASE_ID, "Unexpected release ID")
    target = release_notes.tag_target(api, manifest["tag"])
    release_notes.check_existing(current, manifest, current.get("body"), target)
    if current.get("body") == notes:
        return "already corrected"
    require(corrected_body(current.get("body", "")) == notes, "Unreviewed note replacement refused")
    main = api.request(f"{root}/git/ref/heads/main").get("object", {})
    require(main.get("type") == "commit" and main.get("sha") == expected_main,
            "Public main changed; inspect before retrying this correction")
    require(release_notes.parse_json(release_notes.remote_file(api, release_notes.MANIFEST, expected_main)) == manifest,
            "Public declaration differs from the correction checkout")
    require(release_notes.remote_file(api, manifest["notes"], expected_main) == notes,
            "Public notes differ from the reviewed correction")
    # Body-only PATCH; all release metadata and the actual tag are checked on
    # both sides. An uncertain write can be retried and becomes a read-only no-op.
    api.request(endpoint, method="PATCH", payload={"body": notes})
    release_notes.check_existing(api.request(endpoint), manifest, notes,
                                 release_notes.tag_target(api, manifest["tag"]))
    return "documentation links corrected"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true", help="Apply only in the public main-push workflow")
    args = parser.parse_args()
    try:
        manifest, notes = release_notes.check_local(Path(__file__).resolve().parents[1])
        validate_declaration(manifest, notes)
        outcome = "reviewed correction checked offline"
        if args.apply:
            release_notes.require_publisher_context(os.environ)
            outcome = repair(release_notes.GitHub(), manifest, notes, os.environ.get("GITHUB_SHA"))
        print(f"Omuse v0.6.0: {outcome}")
        return 0
    except (release_notes.ReleaseError, OSError, ValueError) as error:
        print(f"Release link correction stopped: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
