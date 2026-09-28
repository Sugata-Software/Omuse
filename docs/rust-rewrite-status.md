# Omuse implementation status

The `rewrite/rust-gpui` branch contains the active Omuse Linux application in
`rust/`. The original installation and Swift/Qt candidate are preserved for
compatibility and reference. Use [the Rust instructions](../rust/README.md) for
the `omuse` executable and the [rename contract](omuse-rename.md) for legacy
identifiers.

## Implementation

The executable uses Rust, GPUI via `gpui-kit`, and direct upstream `gpui-omarchy` components and theme watching. It does not embed the Swift engine or Qt host. Raster operations are CPU-based, with bounded parallel row processing for expensive transforms; GPUI provides graphics-backed presentation. Text uses cosmic-text; ICC conversion uses system LittleCMS; optional native LibRaw and ONNX Runtime assets provide camera decoding and local subject segmentation.

| Area | Implemented behavior | Qualification still needed |
| --- | --- | --- |
| Documents | `.comp` open/save, atomic package replacement, nested layers, original metadata preservation, unsaved guards and isolated recovery | Real Mac open/edit/save round trips |
| Live objects | Canvas-anchored multiline editing, click-to-place/drag paragraph text, installed-font search, editable content/tracking/leading/alignment/box, and live rectangle/ellipse/line shapes with pre-draw radius/line-width settings; source metadata and cached image round-trip | Rotated editing overlays, text-box handles, font substitution, layout and antialiasing comparisons |
| Layers | Reordering/nesting, multi-layer drag and Alt-copy, explicit group targets, collapsible groups, mask/effect copying, multi-selection group/duplicate/delete/nudge, opacity/blends, locks and merge | Broader real pointer and large-project coverage |
| Effects/adjustments | Six layer effects and twelve source-compatible adjustment kinds, derived without destroying source pixels | Full Mac reference-image matrix; kernel approximations remain |
| Masks | Enabled/link state, independent placement, source dependencies, contiguous clipping stacks, nested/chained sources, brush/pencil/eraser, fill/gradient, retouch, clone/heal and copy/cut mask targets; canonical interpolation | Wider Mac comparisons; floating selection transforms are excluded from mask targets in both versions |
| Transforms | Canvas move/resize/rotate handles, modifier constraints, multi-layer transactions, numeric and Ctrl-drag corner distortion, snapping, floating selection lift/commit/cancel | Broader Mac gesture equivalence |
| Selections | Rectangle/ellipse/lasso/wand, soft masks, feather/expand/contract, copy/cut/paste, internal paste origin, floating transforms, animated boundary contours, selection undo/redo, Replace/Add/Subtract, configurable asynchronous wand, and luminosity/colour range previews with selection or layer-mask output | Broader Mac selection-edge comparisons |
| Retouch | Aligned/all-layer Clone/Heal, portable three-mode Spot Healing, exact numeric brush controls and smoothing, blur brush, smudge/liquify, deterministic content-aware fill | Blur is a Linux approximation; broader transformed-edge comparisons |
| Camera Raw | Typed full settings surface: calibration, light/color, curves, mixer/point color, grading, effects, detail, optics and geometry; section controls, draggable channel curve graphs, visual point-color sampling/guide drawing, clipping overlays and cancellable preview | Core Image sampling/blur and geometry reference comparisons |
| Subject tools | Local U2NETP inference, clicked connected-subject selection, guided matte/refine/contrast/shift, preview/cancel, canvas selection or nondestructive layer mask | Diverse-image quality evaluation; Apple Vision segmentation differs |
| Raster/RAW import | Ordinary image orientation and ICC-to-sRGB conversion; bounded LibRaw development with exposure/WB/boost controls | Broader cameras/profiles, as-shot temperature metadata and preview UI |
| PSD import | Layered 8-bit RGB PSD, groups/masks/blends/clipping, raw/RLE/ZIP/prediction channels, selected adjustments, conservative simple-shape promotion; cached raster conversions shown in an import report | Remaining descriptor coverage; PSB/16-bit/non-RGB rejection matches the Mac reader |
| Image size | Off-canvas transformed extents, independently placed mask assets, scaled guides, DPI-only edits and Nearest/Smooth/High quality resampling; one undo | CoreGraphics resampling comparisons |
| Canvas size/Trim | Nine anchors, pixel/percent/inch/centimeter units, relative dimensions and original-aspect lock, transparent/foreground/background/black/white/custom extension fill; transparent or corner-color trim with per-edge choices | Broader Mac boundary comparisons |
| Gradients | Linear/radial, foreground-background/transparent, reverse/opacity, draft preview Apply/Cancel and mask target | Wider Mac edge comparisons |
| Export | Atomic PNG/JPEG/WebP/TIFF with sRGB ICC; format-specific JPEG quality/matte, encoded preview and resolution controls; PNG/JPEG/TIFF DPI metadata | External color-managed viewer checks; WebP has no standard DPI field here |
| Shortcuts | Recording, search and gesture reference, collision/reserved-key validation, persisted remaps, unbound advanced commands | Full Mac command/context coverage and keyboard-layout testing |
| Desktop | Direct Omarchy theme integration, Linux file dialogs with path fallback, separate launcher, persisted grid/guide/ruler/snapping preferences | Physical theme changes, tablet pressure/tilt, mixed-DPI monitors and long-session stability |

## Advanced editing workspaces

The [advanced workflow guide](rust-advanced-workflows.md) documents the new editable-source architecture and thirteen workstreams: stacks, Blend If, high precision and colour, smart/RAW sources, paths/masks, refinement, retouch, removal, brushes/tablet input, warp, automation, multi-image processing and the existing range tools. It is the scope/limit ledger for these additions. The pre-rename checkpoint `5784e83`, installed on 27 September, passed 470 automated tests, the headless editing/export journey, an additional tablet-backend test, seven CLI batch checks and nineteen native acceptance checks through the installed launcher. Thirteen panel layouts were inspected at 800×600 across dark and light themes. Twelve repeated saves and a killed-writer recovery test verified three retained 16-bit layers, including source/result samples and recipes. Exact evidence and qualification limits are in that guide and the machine-local release receipt. Subsequent identity changes are documented in the [Omuse rename checkpoint](omuse-rename.md).

These additions introduce tiled 16-bit source/result assets, while the ordinary paint pipeline and many compatibility paths remain 8-bit. Painting directly on retained editable sources is guarded; explicit rasterization converts that layer to the ordinary paint representation. The Wayland tablet-v2 extension carries pressure and tilt into brush/mask strokes, with physical-device validation still required.

## Compatibility and resource boundaries

Known unsupported or malformed document semantics fail explicitly rather than silently dropping metadata. Live text/shapes, adjustment/effect records and mask placement/source records are supported; older claims that all these require the preserved editor are superseded.

The format bounds are 30,000 pixels per axis, 100 million canvas pixels, cumulative decoded pixel budgets, 10,000 layers and 64 hierarchy levels. Camera Raw, subject detection/refinement, retouch and floating selections have tighter 16-million-pixel processing limits. Retouch strokes also bound interpolated work. Full-canvas CPU buffers remain an architectural limitation. Layer and mask pixels now use whole-image copy-on-write; retained history is bounded by 256 MiB and 100 entries. See [the performance roadmap](rust-performance-roadmap.md). The advanced source pipeline adds tiled 16-bit storage with a tighter 16 MP/768 MiB retained-asset budget; full-image CPU intermediates remain, and this is not a GPU-compute replacement for the Mac renderer.

Platform substitutes are not described as pixel-identical implementations. LibRaw development differs from Apple's RAW processor; U2NETP differs from Vision; cosmic-text uses available Linux fonts; Gaussian/perspective sampling paths require reference comparisons; high-quality transforms use bounded premultiplied Lanczos3. Source-compatible settings alone do not establish visual parity.

## Verification

Run `scripts/test-rust.sh` for formatting, core tests, headless GPUI interactions and a synthetic save/reopen/export journey under isolated temporary XDG directories. `scripts/native-rust-check.py` launches a separate synthetic native window and validates interaction, persistence and programmatic theme transitions. Optional runtime fixture environment variables exercise real RAW, PSD and ONNX paths. See [runtime dependencies](rust-runtime-assets.md).

Machine-local evidence under ignored `rust/evidence/` records the exact build and executed checks. The current source audit is [rust-parity-audit-20260926.md](rust-parity-audit-20260926.md). Automated Linux results do not prove physical hardware support or a completed macOS comparison run.

Full parity is not yet certified. Contribution should be offered as a substantial experimental Rust port with explicit remaining work and reproducible evidence, not a completed drop-in Mac replacement. No upstream pull request has been published from this work.

## Verified checkpoint 94818ef — 26 September 2026

That checkpoint passed 298 automated tests: 92 library/runtime tests, 55 binary/UI/recovery/control tests, two independently generated Camera Raw reference tests, 77 editor tests, 36 layer/canvas transaction tests, two floating-selection tests, two live-object round trips, nine filter tests, eighteen raster tests and five integrated tool/selection tests. The single manual timing benchmark is separate. The synthetic paint/undo/redo/save/reopen/PNG/JPEG/WebP/TIFF/gradient/adjustment journey passed. Real U2NETP, LibRaw DNG, layered PSD/reference-render and unsupported PSD-depth fixtures were enabled.

The native Wayland journey passed twelve acceptance checks with the release executable and installed launcher. Three additional 55-test headless UI runs passed with deliberate filesystem-event noise, verifying deterministic theme initialization. Startup was verified before the harness resized the window: the installed Omarchy configuration now opens Compositor Rust maximized at 1776×1075 logical pixels. Omarchy's default suppresses application maximize requests, so a scoped `maximize = true` rule was added to the existing Compositor Rust rule, with a backup and clean `hyprctl configerrors`. Existing artwork and editor processes were preserved.

The final curves, point-color and geometry panels were captured and inspected at 1000×840 logical pixels. Their action footer remains visible while content scrolls; GPUI tests also cover an 800×600 layout. The curve, point-color and guide widgets receive actual event-dispatch tests. Selection animation reuses contour data instead of recompositing on each animation tick.

Evidence is machine-local under ignored `rust/evidence/`: `parity-accepted-native-20260926`, `parity-installed-final-20260926`, `parity-installed-{curves,mixer,geometry}-final-20260926`, and the release-test, repeated UI-test and benchmark logs. The installed launcher also exported the real DNG to a 3024×4032 PNG using adjacent runtime assets without environment overrides; PNG CRCs, complete pixel-stream decoding and the embedded 588-byte ICC profile were checked. The installation receipt records the final source revision, executable hash, test log, runtime fixtures and installed-launcher checks. The earlier installed checkpoint was `ed3848b`; it passed 186 tests and remains recoverable.

These Linux tests do not certify macOS round trips, physical tablets, external color-managed displays, mixed-DPI devices, long-session reliability, or identical Apple RAW/Vision output. No upstream publication has been made.

## Measured transform performance

The local release benchmark composites three 1024×768 layers, then rotates one layer 17° at 0.85 scale using High quality sampling. Twenty samples improved from a 1208.512 ms median to 189.080 ms after cached Lanczos weights and bounded parallel rows (about 6.4×). The identity-layer median was 9.772 ms; a separate single Gaussian sigma-8 sample took 172.925 ms. This is a CPU microbenchmark on this host, not a large-document responsiveness guarantee. Cached sampling matches the original scalar formula exactly, and serial/parallel whole-image tests cover transforms, opacity, blends and layered masks. Logs are retained in `rust/evidence/`.

## Inline text and release hardening follow-up

The follow-up source passes 306 automated tests: the earlier 298 plus five additional GPUI interaction tests and three text-transaction tests. This includes actual multiline/Unicode input with editor shortcuts enabled, canvas click/drag editing, draft cancel/apply, one-step document undo, Save versus Save As, locked-layer rejection without losing typed content, preserved text transforms/mask placement, and save/reopen. All optional RAW/PSD/ONNX fixtures and the synthetic export journey ran again.

The [release qualification harness](rust-release-qualification.md) passed eleven actual recovery-worker tests and twelve repeated project/recovery publications. A second writer was killed after its revision-4 staging directory appeared: revision 3 remained readable as the project and revision 4 as recovery. Every reopened synthetic pixel was checked against the expected revision. This is bounded process-interruption evidence, not long-session or power-loss certification.

The [portable C reproduction](upstream-color-noise-reproduction.md) confirmed the transparent-neighbor color-noise issue with MemorySanitizer and controlled scratch contents. The standalone candidate patch stabilizes that output while preserving opaque results and alpha. It remains unapplied to protected Mac source and unpublished.

The final executable and installed launcher both passed all twelve native Wayland acceptance checks, now including inline text insertion/editing. The actual editing field was visually inspected at 1000×840 and 1776×1075 logical window sizes after correcting a clipped-font layout. Installed DNG export again produced a valid 3024×4032 PNG with complete pixel decoding, chunk CRCs and a 588-byte ICC profile. Evidence is under `rust/evidence/parity-inline-native-accepted-20260926`, `parity-inline-installed-20260926`, and the release receipt; the preceding executable is preserved as `compositor-rust.94818ef`. Existing user document windows were not restarted.

## Shared raster ownership and release tooling

Layer and mask images now share immutable pixel allocations across document copies, history, recovery and preview sources. A mutation detaches the edited image; untouched layers remain shared. Retained-history accounting charges unique allocations once, excludes pixels still held by the live document, and enforces both the existing byte budget and a 100-entry undo/redo bound. This is whole-image copy-on-write, not tiled storage.

The candidate passes 316 automated tests with RAW/PSD/ONNX fixtures, the synthetic editing/export journey, and twelve native Wayland checks. New tests cover frozen snapshot/save isolation, nested mask edits, retained allocation capacity, history eviction, raster fast-path equivalence, and stale background completions. The companion installer has seven isolated regression tests. The attempted extra Cua inspection could not run because its service was unavailable; native results come from the existing application harness.

See [measured performance and next architecture work](rust-performance-roadmap.md), [offline bundle installation](rust-bundle.md), and [release gates](rust-release-gates.md). The new Rust CI workflow has been syntax-checked locally but has not run on GitHub. The dependency notice inventory includes all local features and records missing legal texts for review; no public distribution or licensing certification is claimed. Machine-local qualification, bundle and installation receipts are retained under `rust/evidence/`.
