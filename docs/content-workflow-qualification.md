# Editable carousel and targeted page revision — development qualification

**Historical development evidence.** The implemented changes below are included
in Omuse 0.10.0. The [0.10 release qualification](release-0100-qualification.md)
records the final source, CI, packages and publication status. Measurements,
source identities and open-gate statements below describe their original
checkpoint; they are not silently rerun or promoted by that release.

This change is based on `1c987c132053a240e4887a225b3b8ed4f9bab1c4` in
`Omuse-next-studio`, after the published 0.9.0 runtime. It does not replace the
installed app, change the project format, or qualify a new provider operation.

## Problem and change

The assistant's project brief previously listed inactive page IDs and names,
but only the active document exposed editable text-layer IDs. A request such
as “revise page three” therefore could not reliably address that page's text
while the cover remained selected.

Standalone Design & layout preparation now includes `otherPageText`. Each
record pairs its page ID with native text-layer IDs, names, text excerpts,
and inherited lock/visibility state. The assistant is instructed to select
the supplied page before revising its text. Omitted targets require selecting
the page; the prompt explicitly forbids invented IDs or recreating a page as
an alternative to revising it. Existing typed operations, atomic preparation,
source identity checks and review/Keep enforce the same editing boundaries.

The context contains at most 24 inactive pages, 64 native text layers per page,
2,048 Unicode characters per text excerpt and 64 KiB of serialized page
records. Truncation and unavailable pages are explicit. A read-only per-page
visitor avoids populating the shared lazy-page cache. Image bytes, file paths
and unrelated layer/document metadata are not added. Photo, caption, image
requests and image-workflow finishing do not receive this additional context.

## Reproducible content journey

`rust/examples/content_workflow.rs` creates original companion content for
*Find your files*: a cover, four practical steps and a ten-minute action. It
uses the native templates, brand kit and typed assistant-plan parser. It is
an authored deterministic plan, not a live provider response or a book excerpt.

The journey starts with the cover selected, obtains page-three layer IDs
solely from the new context, and revises that page's heading/body and export
copy. It checks the other five page documents exactly, saves both the original
and revised editable projects, reopens the final project, and exports all six
pages to PNG/JPEG/WebP and a multipage PDF with captions, alt text and manifest.
It verifies 18 native text fields, text overflow, editable text/layout and rendered pixels through save/reopen, page
order, metadata and exact decoded PNG/WebP pixels against the saved artwork.
JPEG is checked for dimensions; lossy JPEG is not claimed pixel-identical.

Run from this checkout, choosing a destination that does not exist:

```sh
cargo run --manifest-path rust/Cargo.toml --release --locked --features ui-test \
  --example content_workflow -- /absolute/path/to/new-output-directory
```

A new GPUI regression drives the actual kept-project path and ordinary Undo
and Redo commands, checks all six pages, and saves/reopens the redone result.
Context regressions cover lazy pages, targeting, inherited lock/visibility,
Unicode excerpts, page/layer/byte bounds and metadata exclusion.

## Current validation

Local Linux checks completed on **5 October 2026** against the working-tree
change above:

| Check | Result |
| --- | --- |
| Creative plans and bounded page context | 19 passed |
| Direct shared lazy-cache inspection | 1 passed |
| Export transactions and PDF order/dimensions | 3 passed |
| Native collection UI and standard Undo/Redo | 17 passed |
| Assistant UI, review and request handling | 40 passed |

**80 focused tests passed, no failures.** This is a focused regression run,
not a new full-suite or release qualification. Compilation used the release
dependency cache with `--config 'profile.release.package.omuse.opt-level=0'`
and `--features ui-test`. Omuse itself was unoptimized for this correctness
run; no speed, memory-peak or release-package claim follows from it.

The final sample passed all 18 native field checks and exact five-page
preservation, save/reopen native text/layout and rendered-pixel comparisons,
and decoded PNG/WebP comparisons. The ordered pack contains 18 raster images,
a six-page PDF, captions, alt text and manifest. All six PDF pages were rendered
and visually inspected. The initial visual review led to larger body copy
(at least 38 px), stronger headings, dark cover/closing cards and page counters.
The original draft and typed creation, layout and revision plans are retained.

Cua Driver opened the final saved project in the separate verification binary
under an isolated XDG profile on XWayland. Fresh exact-window screenshots
confirmed the cover, Ctrl+0 fitting the artwork, and a page-strip action showing
page three's revised heading, complete filename and native text layers. A
first click lacked Cua screenshot context and made no change; the explicit
session retry was observed successfully. Native Undo/Redo was not claimed
from this pointer journey; the GPUI regressions cover those commands.

Local artifacts live in `~/Pictures/Omuse/Find Your Files 2026-10-05/`; source
hashes, logs, captures and the verification executable are retained in the
maintainer's `Omuse-release-evidence/content-workflow-2026-10-05` directory.
The installed 0.9.0 executable was checked before/after and remains unchanged.

## Limits

No new live Codex or Claude request is claimed. These checks establish local
request context and editing/export integrity, not model compliance with every
brief, independent design quality, arbitrary font coverage or accessibility
certification. Physical/native UI observations are recorded separately from
headless GPUI tests. The existing release gates remain unchanged.
