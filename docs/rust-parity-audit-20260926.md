# Rust parity audit — 26 September 2026

This audit compares the Rust rewrite with the preserved Mac source in the same repository. It distinguishes implementation from executed Linux tests and from macOS/hardware comparisons that have not been performed. The [current feature matrix](rust-rewrite-status.md) is the implementation index.

## Closed implementation gaps

The current branch adds editable Swift-compatible text/shape records, six live layer effects, twelve adjustment kinds, independent/linked masks, chained live sources and alpha-preserving contiguous clipping stacks. Canvas resize/rotation handles and multi-layer transactions are connected. Multi-layer grouping, duplication, deletion, nudging and drag/copy use one undo transaction, normalize selected parent/child trees and validate locks, cycles and live-mask dependencies. Group targets are explicit; groups collapse; mask/effect rows copy their source semantics. Floating selections can be lifted, transformed, committed once or cancelled; normal clipboard copy preserves fractional coverage and same-session paste origin.

Camera Raw now models the source settings and ordered stages for geometry, calibration, light/color, curves, mixer/point color, grading, texture/clarity/dehaze, glow/vignette/grain, detail and optics. Its UI exposes section forms, point colors, guides and asynchronous preview/cancel. This is substantially more than the earlier Basic-only implementation, with draggable channel curves, point-color sampling and geometry-guide gestures. Core Image output comparison remains separate.

LibRaw develops local camera files; LittleCMS handles ICC-to-sRGB conversion; exports embed sRGB profiles. TIFF profile extraction has a bounded direct IFD path because the image decoder wrapper did not expose its embedded profile. PSD imports preserve supported layers, groups, masks, blends, clipping and selected adjustments, including ZIP/prediction channels. Simple vector promotion is conservative; other supported cached conversions retain notes.

Local U2NETP inference, source-guided matte refinement, contrast and edge shift feed a cancellable preview, canvas selection or nondestructive layer mask. Blur/smudge/liquify stroke kernels and deterministic content-aware fill are connected. Brush/pencil/eraser have an explicit mask target. A prepared mask sampler removes per-pixel metadata parsing and repeated edge scans.

## Remaining distinctions and audit corrections

1. **Mask targets:** fill/gradient/retouch/clone/heal/copy/cut and adjustments use a shared mask target. Floating selection transforms and Spot Healing are excluded from masks in the preserved Mac source too, so their absence is not a Mac parity gap.
2. **Selection and gesture UX:** actual boundary contours, draggable guides/rulers, snapping, Ctrl-drag distortion and interactive Camera Raw channel curves are present. Visual point-color picking and geometry-guide drawing are also connected. Animated marching ants and selection undo/redo are connected. Exact geometry/reference comparisons remain distinctions. Shared text/shape/effect color pickers now isolate drafts from the foreground color; non-JPEG exports no longer validate hidden JPEG settings.
3. **Brush/tool setting depth:** Configurable asynchronous wand sampling, selection combinations, draft gradients and aligned/all-layer clone sampling are connected. Pulled-string smoothing and three Spot Healing modes now follow the preserved implementation. The earlier claim about missing Mac tip/spacing/dynamics controls was too broad: the inspected Mac BrushSettings contains diameter, hardness, color, opacity, smoothing, erasing/healing and healingMode. Tablet input is a hardware qualification item, not an established missing Mac settings surface.
4. **Color/file options:** resolution/DPI controls and PNG/JPEG/TIFF metadata now exist; PSD conversions have a readable report. Display-profile management, broader RAW camera profiles and remaining PSD descriptors need review. LibRaw and Apple RAW are different processing pipelines.
5. **Performance architecture:** this is a CPU, full-surface renderer. Cached mask samplers, allocation-free Lanczos taps, bounded row parallelism, explicit interpolation work limits and stroke limits prevent some extreme workloads, but tiled painting/compositing, incremental GPU filters and large-document memory behavior do not match the Mac architecture.

## Remaining UI distinctions

Type now opens a canvas-anchored multiline text editor: click existing text, click for point text, drag a paragraph box, or Alt-click for a new layer. Drafts leave document pixels/history unchanged until Apply/Ctrl+Enter; Escape discards them, and Ctrl+S applies before saving. Detailed type styling remains in Edit Object. Point-text edits preserve scale, rotation, flips and the transformed upper-left anchor as the source raster changes size.

The editing surface uses Omarchy's native textarea and its selection/clipboard/input behavior. It is an upright, viewport-bounded editing box, using the theme’s readable input font and size, rather than a pixel-aligned rotated/distorted text overlay. Tracking/leading and layer effects remain visible in the committed artwork, not faithfully previewed in the input widget. Rotated inline layout and text-box resize handles remain Mac interaction differences. Canvas Size includes original-aspect locking alongside anchors, units, relative dimensions and extension fills; JPEG has an explicit encoded preview.

## Required comparison and release evidence

- Compile preserved C kernels to generate deterministic golden fixtures. Existing exact fixtures cover Grain, Black & White, Color Balance, and a combined Camera Raw curve/mixer/grading case; Basic/calibration fixtures now cover extrema and alpha too; advanced fixtures exercise detail, optics and effects ordering; geometry still requires Core Image comparison. Regenerate portable fixtures with `scripts/generate-rust-kernel-fixtures.py`.
- Run actual macOS `.comp` open/edit/save comparisons. Swift-compatible JSON and Linux self-round-trips do not prove Mac acceptance, text metrics, antialiasing or Core Image interpolation.
- Validate segmentation on diverse held-out people/products/hair/fur and ambiguous/multiple-subject images. U2NETP is a different model from Apple Vision; do not promise identical masks.
- Exercise native Linux clipboard/portals, physical theme changes, tablet pressure/tilt, mixed-DPI displays, graphics fallback, long sessions and interrupted saves. No physical tablet was enumerated during this host audit.
- Use the exact source revision and executable hash for release evidence. The source status, automated test count, native journey outcome and actual installed launcher must all agree.

## Contribution recommendation

Keep the preserved candidate available. Offer a focused Rust-port proposal with this compatibility matrix, reproducible scripts, dependency/license provenance and golden fixtures. Do not advertise the rewrite as completed Mac parity or submit it as a small compatibility patch. No upstream publication is implied by local commits or local installation.

## Portable reference coverage

The generated references retain aggressive ordered effects, optics, luminance noise reduction, color noise reduction and sharpening, with intermediate prefixes to locate a regression. Advanced inputs include alpha 43, 128, 219 and 255; Basic/calibration also cover zero alpha and alpha 37. Assertions require exact alpha and at most two premultiplied RGB levels of error. The final ordered case is enforced, not ignored. Source SHA-256 values are embedded in the fixtures.

The advanced color-noise reference excludes input alpha zero because the preserved C implementation allocates its chroma plane with `malloc`, skips those entries, then blurs the plane (`Compositor/Rendering/AdjustPixels.c`, color-noise block around lines 844–859). The Rust implementation initializes that scratch plane deterministically. A standalone [reproduction and review-only patch](upstream-color-noise-reproduction.md) now cover this candidate; the preserved C source is unchanged here.

The source audit also confirmed that the Mac PSD reader rejects PSB and non-8-bit files. Those shared exclusions are format boundaries, not additional Rust parity gaps.

## Deterministic UI test setup

A release-suite run exposed a real test-process abort: an inotify theme event woke GPUI's deterministic scheduler from an OS thread after a test finished. The core dump and test trace identify the `notify-rs inotify loop` and scheduler thread assertion; this was not an out-of-memory failure or a user's document window. Headless UI tests now initialize the actual Omarchy controls and immediately apply a fixed theme before yielding, which stops the filesystem watcher. Native acceptance retains the normal upstream system-theme watcher and tests theme transitions on the real event loop. No upstream dependency was modified.

## Layer-drop compatibility choice

Drop operations adopt layers inserted into contiguous clipping stacks and detach formerly contiguous links that move away. Unlike the Mac cleanup pass, they preserve pre-existing arbitrary live-mask links elsewhere in the document. This avoids changing unrelated artwork when a layer moves. Locked affected dependents reject the entire transaction. Tests cover both clipping-stack behavior and preservation of unrelated links.

## Bounded save and recovery qualification

The [release qualification harness](rust-release-qualification.md) repeatedly publishes and reopens generated project/recovery packages, checks every synthetic pixel against the stored revision, and interrupts a separate writer during project publication. This complements actual recovery-worker/ownership tests. It proves bounded process-interruption behavior for the executed run; it does not certify a long editing session or power-loss durability.
