# Vector workflow qualification — 2 October 2026

**Local development qualification passed. This batch is unreleased.** The
installed and published 0.7.0 source release is unchanged. This record covers
the selection, construction and SVG work following the Vectorpea review.
It does not replace the [0.7.0 qualification](release-070-qualification.md).

The working branch is `feature/photo-vector-studio`, based on
`c902b7245332775a5fd815cef3ecc17df010f1d8`. The new implementation is a working-tree
change, not a numbered release or an exact-source CI result.

- Tested executable SHA-256:
  `04afaaa4e93e09a68ecee28634a7ea82776622db6404a4ae0b91c169327a13b8`.
- Final 477-file Rust source manifest SHA-256:
  `5952caa638ff5eed604d70327cbaaddba8551fb39d193ab59628272c0dd1fd03`.
- Optimized, locked build with `ui-test` enabled. A normal-feature production
  build, exact-source CI, installation and release publication remain separate
  release gates. The package version remains the 0.7.0 development baseline.
- [Compact validation receipt](vector-workflow-receipts.json).

## Implemented scope

- Multi-object Shift-click and enclosed-marquee selection; individual selection
  within a group; move, nudge, duplicate, hide and delete selected objects.
- Nested organizational groups, group-aware stacking, relative transforms,
  align/distribute and matching fill/stroke/opacity selection.
- Background Unite, Subtract, Intersect, Exclude and Divide with draft Undo,
  cancellation and stale-document checks.
- Multiple-object editable SVG import/export for the supported solid-paint
  subset, including names, visibility, object opacity and organizational groups.
- Outline view and bounded integer-zoom editing previews, up to 4× and
  16 million pixels in admitted simple photo/vector documents.
- Commands and keyboard reference, inspector controls, draft history and
  versioned `.omuse` persistence integrated with the shared main canvas.

The [workflow instructions](user-guide/photo-vector.md#new-vector-workflow--unreleased-development-build)
describe the gestures, controls and compatibility boundaries.

## Qualification and evidence

The final full run passed **1,254 application tests: 495 library, 364 UI and
395 integration**, with zero failures and four explicitly ignored manual timing
benchmarks. The editing journey, six-page Create journey, PNG/PDF/MP4/GIF exports
and all **80 editable template variants** passed. Formatting and the generated
shortcut reference passed too.

The new focused integration targets passed: **8 boolean, 14 group/arrangement,
8 SVG-scene and 4 zoom-preview cases**. References include independently
rendered SVG pixels, expected boolean coverage, holes, winding rules, repeated
cuts, transformed geometry, hidden paint and opacity, cancellation and bounded
fallbacks. UI checks cover multi-selection, movement, Undo/Redo, grouping,
persistence, async cancellation, SVG append, style isolation, outline mode,
zoom coalescing, compact scrolling and keyboard focus.

The initial full run caught a test-fixture error in the new Raise/Undo check:
its expected scene generated new UUIDs instead of retaining the original
fixture. The assertion was corrected to compare the exact pre-edit snapshot;
the subsequent complete optimized run passed. Earlier failed attempts are
retained alongside the final log.

The full acceptance entry point is:

```sh
env PATH=/usr/bin:/bin:$HOME/.local/bin \
  CARGO_NET_OFFLINE=true OMUSE_TEST_KEEP=1 bash scripts/test-rust.sh
```

It checks the generated shortcut reference and formatting, runs the locked
optimized Rust/UI suite, then the editing, Create/media and 80-template
acceptance journeys. Its temporary XDG directories isolate preferences,
history and recovery. The explicit PATH avoids the host's Python mise shim
requiring trust inside that disposable configuration.

Local detailed evidence is retained outside the source repository under
`Omuse-release-evidence/vector-workflow-2026-10-02`. Interim Cua screenshots in
`cua-layout` predate the final compact-layout and shortcut-focus fixes; they
are not final interface qualification.

### Native desktop and physical input

The same tested executable passed **27 native checks on Wayland/dark and 27 on
XWayland/light**, both at **800×600 logical viewport size**. Both final captures
were inspected. These synthetic journeys exercise raster edits, retained
16-bit export, save/reopen, palette keyboard control and the shared vector
canvas/inspector. They do not simulate every physical device.

Cua Driver 0.29.1 then drove an isolated XWayland window through **10 verified
checks**: open artwork, create a shape and cancel via Escape, select all,
group/ungroup, reveal transforms with Ctrl+T, subtract filled shapes, undo,
toggle outlines and discard the entire draft. The original project package
remained byte-for-byte unchanged; the owned window and Cua session were closed.
Input used the previously verified foreground route and each result was checked
against a fresh screenshot. The driver could not discover the native Wayland
window, so this physical-input evidence is specifically XWayland.

The [illustrated workflow](user-guide/photo-vector.md) includes final captures.
Raw logs, source manifest, licence inventory, acceptance artifacts, native
receipts and `cua-final/verified-cua.json` remain in the local evidence directory.

## Defects corrected during qualification

- Closed flattened contours repeated their first point. Removing that duplicate
  before the overlay engine fixed triangle-shaped rectangle results and holes.
- Divide now processes each existing partition independently, retaining earlier
  cuts when another cutter is applied.
- SVG opacity wrappers are distinguished from real group opacity; imports retain
  anonymous groups, hidden geometry/paint and a visible child's visibility
  override. Unsupported appearance is refused explicitly.
- Whole-group ordering preserves nested groups; aligning individually selected
  members does not move their unselected siblings.
- Late async results release busy state and cannot replace newer document work.
- Style controls precede the Select-only arrangement controls; Ctrl+T reveals
  its focused transform field. Shape, matching, outline and boolean actions
  restore canvas shortcut focus. Invalid matching-style input repaints its error.
- Raise/Lower availability considers the full selection, including a movable
  lower member when the active object is already at the top.

## Limits and release boundaries

Groups are organizational ancestry on an ordered object scene. Group opacity,
isolated blending, clipping, reusable symbols and appearance effects remain
open. Alignment uses conservative geometry/control-point bounds, not analytic
curve extrema or stroke-expanded visual bounds.

Booleans accept closed filled paths. Curves are flattened at 0.05 source pixels;
results retain editable polygonal points rather than original Bézier handles.
Divide partitions the bottom selected object and discards cutter-only regions.
Work is bounded by input/output/object budgets. Cancellation is checked around
overlay work; an individual overlay call is not preempted midway. Interactive
Shape Builder and retained live boolean recipes remain planned.

Editable SVG exchange excludes text, gradients, clipping, masks, filters, group
opacity, external resources and unsupported strokes. Round solid uniform
strokes are admitted; skewed/nonuniform stroke transforms are refused. General
raster SVG import is a separate path. No Illustrator or vector PDF parity is
claimed.

Zoom rerenders are bounded editing previews. Large or unsupported mixed
documents use their ordinary settled preview. Persistent viewport tile caches,
procedural 16-bit rendering, comparative speed and broad hardware acceptance
remain open.

Scene version 2, including grouped artwork, requires canvas format 12. The new
reader accepts formats 1–12; legacy flat version-1 scenes write format 11 and
ordinary canvases write format 10. Ungrouping does not downgrade a version-2
scene. Omuse 0.7.0 cannot read format 12; use Save As to retain an older copy.

The new overlay dependency and its support crates have retained MIT licence
texts in the local inventory. The locked inventory contains 613 dependencies;
the existing `hexf-parse` and `mac` notice findings remain open for binary
redistribution. No provider requests, subscription changes, tablet work,
Windows qualification, public binary, installation or release publication are
part of this batch's validation.
