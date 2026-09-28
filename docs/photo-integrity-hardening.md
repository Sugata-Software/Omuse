# Photo integrity and recovery hardening

The 29 September 2026 pass tightens ordinary editing, 16-bit output and
collection recovery in the native Linux editor. Each original failure was
reproduced before its fix.

## Corrected behavior

- Moving a rotated or reflected layer preserves its dimensions, rotation and
  reflections. A drag commits one undo step; Undo and Redo restore placement.
- Proportional corner resizing can cross its opposite anchor, including
  rotation and Alt resizing from the centre, without collapsing the layer.
- Corner distortion uses geometric corner order. Reflected source pixels are
  reflected exactly once, and the visible handle controls the intended corner.
  Clicking without dragging leaves the layer and undo history unchanged.
- A 16-bit clipping stack changes colour while retaining the base's coverage.
  It no longer increases transparency coverage or leaks the base colour into
  an opaque clipped overlay. Base masks apply once; Blend If sees the actual
  backdrop and the base blend mode remains active.
- Live mask chains resolve in dependency order and retain source parent
  opacity. Clipped groups retain their descendants' independent live masks in
  both ordinary and 16-bit rendering, without copying the full surface map.
- Converting an edited photo to a multi-page collection, or back to a photo,
  can replace its recovery package. The worker stages the complete new format
  before an atomic exchange. Failed conversion retains the prior recovery;
  clearing recovery still waits for the worker's acknowledgment.
- Adding and duplicating pages enforce the same dimensions, name and aggregate
  canvas budget before modifying the collection. Blank pages are checked before
  allocating pixels; invalid duplicates do not load lazy source pages.

The 16-bit renderer also checks total live-mask surface size and chain depth
before allocating buffers. Invalid mask graphs and oversized requests preserve
an existing export file.

## Regression evidence

The final Rust suite passed **757 tests**: 326 library, 190 UI and 241
integration tests, with no failures and four manual timing benchmarks excluded.
This includes 23 added regression cases. The headless editing self-test,
six-page PNG/PDF/MP4/GIF exports and all 80 editable template variants passed.
The catalog retains its advisory content-guide warnings; this is not a new
visual review of every template.

The production executable passed 22 native Wayland checks and the same 22
checks through XWayland at the 800×600 logical viewport in the light theme.
The installed preview launcher passed all 22 checks with reduced motion.
Captures of these synthetic-artwork windows were visually inspected. These
journeys dispatch GPUI events in process, rather than proving physical input.

The new cases are in `rust/tests/raster16_export.rs`,
`rust/tests/raster_clipping_descendants.rs`,
`rust/tests/create_page_admission.rs`, `rust/src/recovery.rs`,
`rust/src/transform_interaction.rs` and `rust/src/ui.rs`.

Tests compare exact retained 16-bit samples and decoded PNG/TIFF output,
partially transparent clipping, child-mask coverage, source-state preservation,
page collection state, recovered artwork and the prior recovery package after
a failed conversion. Editor interaction tests dispatch drag and Undo/Redo
events through GPUI; they do not establish physical pointer delivery.

## Published source and installed preview

The runtime changes are published in
[`b2f6e85`](https://github.com/Sugata-Software/Omuse/commit/b2f6e854a96b72e1715d9048f0d7f4885e569efa).
The production and installed-preview executable SHA-256 is
`52a9e0656e2a463c7c2b7d25c4212aa190913b8b8ed341b34f86c055e77055d9`.
The preview retains the matching runtime assets and the previous complete
installation. Rollback to the prior executable (`5e7d8b9c…`) and forward to
this candidate both passed their self-tests and exact executable/asset checks.
The older separate application remains available.

The GitHub project-guide and installer/reference jobs passed for this source,
as did the complete [Rust run](https://github.com/Sugata-Software/Omuse/actions/runs/36454128430). No public binary release is implied by this source checkpoint.

## Remaining boundaries

The 16-bit **High quality** transform path still uses bilinear sampling while
the canvas uses scale-aware Lanczos. A 64-to-8-pixel reduction of a quarter-duty
stripe produces roughly 64 in the interior canvas pixels but 128 in 16-bit
export when expressed on an 8-bit scale. This known aliasing difference needs
a separately tested resampling change; export sampling parity is not claimed.

Detached live-mask sources include parent opacity, but ancestor-folder mask
coverage is not yet included in that detached lookup. Traditional adjustment
and effect records remain unsupported by explicit 16-bit export. Existing
16 MP precision-processing limits remain in force.

Recovery checks cover application errors, cancellation and atomic replacement
on this Linux host. They do not establish abrupt power-loss durability or
compatibility with every filesystem. No new physical hardware, tablet,
multi-machine, external RAW-corpus or live AI acceptance is claimed by this pass.
