# Omuse 0.7.0 qualification — 2 October 2026

**Local and exact-source GitHub qualification passed.** Omuse 0.7.0 is installed
on the development host. The numbered source release and installer select the
same tested runtime.

The scope is the main-canvas vector workflow, Image Trace, target-colour
uniformity, transformed 16-bit sampling and interface polish described in the
[release notes](releases/v0.7.0.md). This release adds no new live AI-provider,
comparative performance or broad hardware qualification.

## Exact runtime

- Public runtime: `5b3daefbb5258afff4a74a2ff3db5247b074972d`.
- Git tree: `f9b4326d6ef37ea3edc56002566e1df399e47e69`.
- Local runtime checkpoint: `12d034b8ca88a55bf258d7ba7491ba0209f72a54`, with
  the identical committed tree. Later working changes were release documents.
- [Exact-source GitHub validation](https://github.com/Sugata-Software/Omuse/actions/runs/36910650216): **completed successfully**.
- Normal production executable SHA-256: `fdb1e47366fec20668d19e5969c34182c9c934dc90946993715c6820b8db05ea`.
- Production build uses locked dependencies and normal features, without
  `ui-test`. The clean public checkout supplies the installation source receipt.
- [Compact release receipt](release-070-receipts.json).

## Automated and synthetic acceptance

The full local release suite passed **1,206 application tests: 494 library,
351 UI and 361 integration**, with zero failures and four ignored timing
benchmarks. The editing journey, six-page Create journey, PNG/PDF/MP4/GIF
exports and all **80 editable template variants** passed. The generated
keyboard reference and Rust formatting checks also passed.

Four independently sampled synthetic tracing fixtures passed the existing
geometry, image and save/reopen assertions. Separate examples verified
target-colour retained samples and Undo, editable SVG exchange, multi-object
scene persistence and allocation accounting. These are bounded fixtures, not
proof of tracing quality on arbitrary photographs or comparative speed.

The media acceptance example passed. The collection-save interruption check
and twelve-revision retained-16-bit save/recovery journey passed, including
independent reopening after the writer was killed during staging. These check
process/filesystem recovery, not power-loss durability.

The existing installer (25), bundle (11), release-publisher (28) and
dependency-notice (6) test suites also passed. The locked inventory records
608 dependencies; the existing `hexf-parse` and `mac` notice findings remain
open for binary redistribution.

## Desktop, installation and screenshots

The production executable passed **27 native checks on Wayland and 27 on
XWayland at 800×600 logical size**, with dark and light captures inspected.
The **installed normal launcher passed 24 checks** at the same minimum size.
Ten further production journeys captured the gallery: three vector/trace views
passed 27 checks each; seven photo, Create, AI, export, motion and command views
passed 24 each. All ten captures were inspected.

The normal launcher selects **0.7.0**, generation `install-gowujrk8`, with a
clean public-source receipt and a complete **20-file payload**. Its executable
matches the production hash. The complete 19-file 0.6.0 generation
`install-brzel9z6` remains available for rollback. Both rollback directions
passed isolated editing self-tests with every payload hash unchanged; the
desktop file validates. Existing windows keep their executable until reopened.

Cua Driver 0.29.1 separately opened the installed app and a saved synthetic
project on XWayland. An exact-window screenshot verified foreground Ctrl+K
opening command search. Background delivery had no visible effect. This is a
bounded shortcut/capture check, not complete accessibility qualification.

The [ten-image gallery](releases/v0.7.0-gallery.md) contains actual production
interface captures with isolated synthetic artwork. The AI panel is an offline
presentation check; no subscription or API request was sent. Native journeys
drive owned GPUI windows and in-process events. Their result does not qualify
every physical input device or accessibility route.

## Previous-release compatibility

The 0.6.0 and 0.7.0 production readers exported identical decoded RGBA pixels
from the synthetic **128×96** previous-release project. Both original package
files remained unchanged. The old reader refused the new format-11 vector
fixture, created no export, and left that package unchanged. This verifies
these fixtures, not independent Photoshop or Illustrator interchange.

The reader accepts canvas formats 1–11. Ordinary canvases remain format 10;
vector scenes require format 11. Omuse 0.6.0 cannot open format-11 artwork or
projects with the new Target Colour Uniformity node. Opening an older project
does not rewrite it. **Use Save As to retain an older compatible copy.**
Application rollback does not downgrade artwork. Collection schema version 2
is a separate compatibility boundary.

## Release boundaries

- Arch/Omarchy Linux x86_64 **source pre-release**; no application binaries
  are attached. Clean-machine and broader hardware acceptance remain open.
- Image Trace produces bounded RGBA8 solid-colour approximations. It does not
  reconstruct text, gradients, original illustration structure or lossless
  photographic detail. Inputs are limited to 16 MP, working images to 4 MP,
  with explicit colour, geometry and work budgets.
- Vector scenes retain editable geometry plus one source-resolution RGBA8
  display cache. Exporting that cache through a 16-bit path adds no precision.
  Gradients, booleans, broad multi-object SVG, vector PDF and Illustrator
  interchange remain outside the implemented scope.
- High-quality retained-source export uses bounded Lanczos sampling. Extreme
  reductions remain approximate, and masked 8-bit preview/16-bit export
  equivalence is not established. Target colour uses encoded-sRGB HSL rather
  than perceptual or complete wide-gamut colour matching.
- Earlier AI receipts retain their original source and operation boundaries.
  This release makes no new live-provider or full Photoshop/Illustrator claim.

See the [public-release gates](public-release-readiness.md) for remaining work.
