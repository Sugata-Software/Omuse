# Image trace and curve editing: development qualification

Development work, 1 October 2026, on `feature/photo-vector-studio`.
This extends the [unified photo/vector canvas](unified-vector-canvas-qualification.md).
Installed/public Omuse 0.6.0 is unchanged. This record describes a development
candidate, not an Illustrator parity or public-release qualification claim.

## Behaviour

- Pen click creates corners; dragging creates opposed smooth Bézier handles.
  Nodes mode moves anchors and handles; Alt breaks handle alignment and Shift
  constrains movement/handle angles. Pen click on the first anchor closes an
  open contour. Pen segment-click or Nodes double-click inserts an exact
  De Casteljau split. Existing point delete, smooth/clear handles, reverse,
  compound fill and local Undo remain integrated.
- Image trace runs locally on the main canvas with an Omarchy-themed inspector.
  Logo/Illustration/Photo art presets, Colour/Gray/B&W modes and independent
  palette, detail, smoothing, corner, noise, threshold, processing edge and
  point-limit controls feed a deterministic bounded contour engine.
- Source/Trace comparison and background previews do not change the document.
  One worker and a newest-request queue coalesce settings changes; cancellation,
  generation, document revision and document identity checks fence late results.
  Invalid or incomplete settings keep the previous display and disable Keep.
  Navigation remains available; other edits require Keep or Cancel first.
- Keep inserts editable geometry next to the source in the same group, hides
  and retains the bitmap, preserves source dimensions/placement/masks/supported
  effects and makes one document Undo transaction. It immediately opens Nodes.
  Save/reopen retains source, scene/cache and trace settings. Retrace replaces
  only that trace layer's geometry/cache and retains its later placement/style.
- Locked targets/parents are refused before tracing. Cancelling an unsaved Quit
  retains the draft. Trace publication rechecks identity/revision and project
  budgets. No new tracing dependency or provider connection is required.

## Qualification results

Source **`3fbca66`**, extending implementation **`15cacd2`**, passed the complete
local validation script: **1,206 tests** (494 library, 351 UI and 361 integration),
**zero failures**, and four ignored benchmarks. This includes all six trace UI
regressions and 11 trace integration tests. Formatting, the generated shortcut
reference, the editing/save/reopen/export journey, Create PNG/PDF and MP4/GIF
output, and all **80 editable template variants** passed. The script exited 0.

Two initial gesture fixtures were corrected: their tiny artwork at 65% zoom
placed segment clicks inside the intentional nine-screen-pixel handle targets.
The final tests zoom first, retain their geometry assertions and verify Undo/Redo.
An inspector debug selector supports the anchor-status reachability check; this
does not change drawing or layout behaviour.

The final native Image Trace journey passed **27 checks each** in Wayland/dark
and XWayland/light at an 800×600 logical viewport, and in a wider 1896×1150
Wayland window. A further **27-check** XWayland/light journey captured the curve
editor at 800×600 logical size. Screenshots were inspected: tracing stays on the
main canvas, buttons/fields align, and the compact inspector scrolls to the lower
controls. The bundled Muse icon produced 347 editable points and 50 subpaths
with the default trace settings. These runs used isolated test profiles.

All four final native runs used executable SHA-256
`29ab09aad116d43cbcefffc1a48ff2a2491309f97b23ac709f282cf694ba506b`.
The installed 0.6.0 binary remains
`bf3b8f5f510865fd832d3305b1b2ea40505c5eab6c93bf9fa2d49dc629b41a46`.

The focused engine suite passed **9 tests**. The independently sampled analytic
fixtures use 256×192 RGBA8 pixels and exercise actual vector rendering:

| Fixture | Silhouette intersection/union | White-matte RGB mean absolute error (0–255) |
| --- | ---: | ---: |
| Curved logo with a hole and speckles | 0.9501 | 3.5175 |
| Overlapping flat colours | 0.9846 | 1.1243 |
| Semi-transparent curves with hidden RGB | 0.9767 | 0.5789 |
| Antialiased grayscale drawing | 0.9794 | 0.9562 |

The logo used **20 editable anchors / 20 nonlinear cubic segments**, versus
**476 anchors** for the unsimplified linear baseline: **95.8% fewer**. Raw pixel
edges counted before contour cleanup were 816. The transparent fixture's mean
absolute alpha error was 1.1225/255. These measurements establish only the named
fixtures and settings, not quality or latency on arbitrary photographs.

The acceptance program checks deterministic repeat geometry, hidden-RGB
invariance, interior/hole probes, point-budget refusal, cancellation, bounded
processing dimensions, unchanged source bytes, Undo/Redo and exact saved/reopened
scene/cache/composite equality. Source, trace, white-matte and silhouette-difference
images were inspected; aggressive reduction visibly approximates the original
contours, and thin antialiased grayscale edges remain an area for improvement.

## Limits and remaining work

Source layers are limited to 16,777,216 pixels, with a four-megapixel working
image, 32 colours, two million raw contour edges, 100,000 editable anchors and
4,096 subpaths, plus conservative tracing/simplification/render/document work
budgets. A point limit refuses excess geometry; it does not silently discard
objects or guarantee an automatic best fit to a target count. Higher colours
can reach the work budget before the processing-edge limit.

Tracing produces RGBA8 solid-colour compound shapes. The current canvas displays
a cache at the source image's resolution, so magnifying small artwork can show
pixels even though its retained points remain editable. It does not infer text,
gradients, semantic objects, lossless photo detail or original illustration
structure. The original image stays in the project. Masks/effects stay as layer
treatment, rather than becoming vector contours. Retained curves use format 11,
which public 0.6.0 cannot open. Complete multi-object SVG/PDF exchange, path
split/join, standalone simplification of existing hand-drawn curves, broad
photographic quality, topology under aggressive fitting, long-session peak
memory and large-document interactive latency remain separate work.

The review removed an unsafe background shortcut that filled transparent holes
and compounded semi-transparent regions. Regression fixtures cover those cases.
It also repaired settings persistence for null layer metadata and preserving a
ready trace when an unsaved Quit is cancelled. These are tested behaviours, not
a promise that every pathological contour is topology-preserving.

## Reproduce and inspect

- `PATH=/usr/local/bin:/usr/bin:/bin OMUSE_TEST_KEEP=1 ./scripts/test-rust.sh`
- `cargo run --manifest-path rust/Cargo.toml --release --locked --offline --example image_trace_acceptance -- /tmp/omuse-trace-new-evidence`
- `python3 scripts/native-rust-check.py rust/target/release/omuse rust/evidence/trace-new-native --capture --minimum-window --panel image-trace --theme dark`
- Repeat the native command with a fresh directory, `--theme light --x11`.

The native trace demonstration uses Omuse's actual bundled Muse icon. The
analytic acceptance sources do not use the production path renderer to generate
their input pixels. See the [user guide](user-guide/photo-vector.md) and searchable
[keyboard reference](keyboard-shortcuts.md) for the everyday workflow.

Local evidence:

- `rust/evidence/image-trace-final-source.txt`
- `rust/evidence/image-trace-final-full.log` (complete passing validation at `3fbca66`)
- `rust/evidence/image-trace-development-full-v2.log` (initial run, including the two ambiguous gesture fixtures)
- `rust/evidence/image-trace-fixtures/image-trace-measurements.json` and adjacent rendered fixtures
- `rust/evidence/image-trace-final-native-dark/verified-native.json` (wide window)
- `rust/evidence/image-trace-final-native-compact-dark/verified-native.json`
- `rust/evidence/image-trace-final-native-light/verified-native.json`
- `rust/evidence/curve-edit-final-native-light/verified-native.json`
- `/tmp/omuse-tests.z6GaDVtw` (temporary validation output, retained locally)

Screenshots are also retained in [dark tracing](user-guide/images/image-trace-dark.png),
[light tracing](user-guide/images/image-trace-light.png) and
[curve editing](user-guide/images/vector-curves-light.png).
