# OmaPhoto comparison and improvement plan

Reviewed 1 October 2026 against [OmaPhoto v1.3.3](https://github.com/ZacharyZhang-NY/OmaPhoto/releases/tag/v1.3.3),
published 30 September at exact tag commit
[`190414451148d9627e902ded53082dc3b3019913`](https://github.com/ZacharyZhang-NY/OmaPhoto/tree/190414451148d9627e902ded53082dc3b3019913).
The local source comparison uses Omuse 0.5.0, canonical `479ae31` and public
runtime `73dd0d4`; the installed launcher reports 0.5.0 with that public source
receipt. Source and regression tests establish intended behavior. OmaPhoto was
not installed or executed for this review, and neither application's tests were
rerun. This is not evidence that either editor is faster or more reliable.

The **whole-repository** watch includes the three post-tag commits through
[`e94fd73`](https://github.com/ZacharyZhang-NY/OmaPhoto/tree/e94fd73573aee5eefd8e955e72cda5b379d4120f).
Since the previous `838d5b1` review, 12 commits change 150 distinct paths: nine
commits enter 1.3.3 and three follow it. Four releases/tags, one branch, eight
issues, two pull requests and nine workflow results were checked. The three
older releases retain their notes and asset metadata. The
[watch baseline](omaphoto-watch.json) separates the release, updated packaging
and later application changes.

Omuse's strengths are its native Omarchy integration, searchable and executable
command reference, editable content collections, templates and reversible
in-app AI workflows. OmaPhoto has a more complete Camera Raw interaction surface
and a substantially broader downloadable Linux release. Both matter to a
credible public editor.

## Omuse 0.6 implementation following this review

The tables below retain the **0.5.0 review baseline**. The subsequent
`feature/omaphoto-133-editing` work implements the recommended editing backlog:
format-10 colour-run import, direct live text/colour previews, Open Recent,
numeric scrubbing, ten Dither styles and richer finishing effects, bounded
PSB/SVG imports and supported Photoshop text, growing masks, 3:4 crop,
capability-aware layer actions, background selection outlines, queued saves
and external-change decisions. Camera Raw gains stage-aware sampling and
targeted drags with coalesced, cancellable work.

See the [implementation and qualification record](omaphoto-133-implementation.md)
for current evidence. These implementations do not establish interchangeable
project semantics, cross-app performance superiority, a larger safe document
budget or a portable binary release. Historical gaps in the tables should be
read with this newer implementation record.

## OmaPhoto 1.3.3 against Omuse 0.5.0

This release includes the previously reviewed finishing filters, crop and
clipboard work, plus PSB/SVG imports, editable Photoshop text, live text/effects,
growing masks, numeric scrubbing, selection-outline work and external reload.
They are now shipped features, rather than just development to watch. The
historical tables below retain the original review scope and limitations.

| Area | Source evidence and Omuse comparison | Priority |
| --- | --- | --- |
| Project format 10 | OmaPhoto now writes 10 and reads 1–10. Its optional `text.colorRuns` use UTF-16 offsets; Omuse's reader rejects versions above 9 and its rich runs use UTF-8 byte offsets. Renaming a project to `.omuse` cannot bridge this difference. | Highest interoperability priority: a bounded importer with Unicode range conversion, cached-pixel preservation, malformed-input tests and actual upstream fixtures. Do not simply increase the accepted version. Omuse collection schema v2 is a separate format. |
| Dither | Ten styles span Atkinson/Floyd–Steinberg, three Bayer sizes, halftone dots/lines/diamonds, patterns and ASCII, with pixel/cell sizes and colour choices. Source tests cover alpha, previews, committed output, picker cancel and worker rendering. Omuse has no equivalent Dither command or filter. | Strong creative addition for retro posters and social artwork. Start with deterministic diffusion/Bayer/halftone, reversible preview, selection/alpha correctness, bounded workers and Undo before expanding to ASCII. |
| Live text and selected-letter colour | Upstream renders draft artwork through the committed path and colours an actual text selection, with caret-linked swatches and picker cancel/focus restoration. Omuse supports mixed font/colour runs through its typography editor, but its inline textarea is an overlay and lacks this direct selection/picker integration. | High everyday usability value: the same render path for preview and Apply, plus native selection-aware colour editing. Preserve IME, Unicode, masks/effects and Undo. |
| Open Recent | A persisted ten-item menu deduplicates canonical paths, drops missing projects, supports Clear and only promotes successful opens/saves. Tests cover cancelled/failed actions and files disappearing while listed. Omuse has no shared recent-project list or command. | Add a keyboard-searchable recent list covering canvases and collections, with clear history, safe dirty-document navigation and missing-file handling. |
| Numeric and slider polish | Numeric scrubbing is now released. Black & White, Color Balance and Hue/Saturation also gain coloured slider tracks and double-click reset. Omuse's refined panels retain typed/button controls without shared label scrubbing. | High daily editing value. Combine visible drag affordances with exact typing, keyboard access, reset, Escape rollback and one Undo entry per drag. |
| Save, close and focus safety | Upstream adds snapshot saves while editing and queues save/reload/open/close operations behind one writer. Its new tests cover repeated saves, quit, failed writes, disappearing controllers and exporting beside a save. Omuse already has background snapshots, destination locks, conflict detection and publication checks; this is not a wholly missing feature. | Extend lifecycle stress coverage and external-change choices while preserving Omuse's existing save guard. Do not substitute the upstream manifest/image-size fingerprint for Omuse's metadata/inode checks. |
| Imports and masks | PSB, bounded SVG/SVGZ raster import, simple editable PSD text and paint-driven mask growth from the previous review are included in 1.3.3. Those remain concrete Omuse gaps, with upstream's own conversion limitations. | Keep the planned bounded import and mask work. Existing PSD support, rich typography and fixed-size mask painting are not equivalent. |
| Smaller parity details | Omuse already has anchored keyboard zoom, session layer Copy/Paste, context menus and selection-started crop with portrait 9:16 via orientation swap. It lacks a 3:4 preset, empty-layer Vignette and the richer spatial finishing-filter semantics described below. | Avoid rebuilding existing workflows; add the missing behaviours with explicit modes and visual references. |

Primary source references: [format specification](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/docs/project-format.md),
[Dither implementation](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/src/Document/Dither.cpp),
[Dither UI tests](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/tests/DitherSheetTests.cpp),
[selected text tests](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/tests/TextColorRunsTests.cpp),
[recent-project tests](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/tests/OpenRecentTests.cpp)
and [save queue tests](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/190414451148d9627e902ded53082dc3b3019913/tests/ProjectSaveQueueTests.cpp).
Corresponding Omuse paths include `rust/src/document.rs`, `objects.rs`,
`rich_text_ui.rs`, `ui.rs`, `crop.rs`, `filters.rs` and `save_guard.rs`.

### Release evidence and changes after the tag

The [1.3.3 release workflow](https://github.com/ZacharyZhang-NY/OmaPhoto/actions/runs/36737522836)
passed all six jobs at `1904144`. The inspected workflow and Arch recipe build
and package the application; they do not run the full test suite. Upstream's
`TASKS.md` reports 291 passing test programs across distro checks and fresh
container install/start checks. These are upstream-reported results, not our
independent rerun or directly demonstrated by that release workflow.

The release currently has six application packages plus `SHA256SUMS`. Fedora
43 and 44 packages were added separately to handle their different LibRaw ABI;
upstream reports building the new Fedora 44 package from the unchanged tag.
The updated asset metadata is captured in the watch baseline. No AppImage or
Flatpak is published, and PR #8 remains unmerged.

The final post-tag [acceptance commit](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/e94fd73573aee5eefd8e955e72cda5b379d4120f)
adds three application-driven test programs, sixteen screenshots and a
[feature acceptance table](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/e94fd73573aee5eefd8e955e72cda5b379d4120f/docs/acceptance.md).
Those screenshots use Qt's offscreen platform. The same commit fixes window
teardown reaching already-destroyed menus/toolbars when panels retain focus
or pending field edits. That application fix is **after the 1.3.3 tag**, even
though the acceptance document is titled 1.3.3; it must not be attributed to
the tagged release binaries. Upstream reports 295 test programs at this later
head. Raw counts are not comparable with Omuse's individually counted cases.

Recommended order: address format-10 import and save/focus lifecycle coverage;
then recent projects, numeric controls and accurate live text; then Dither,
mask growth and richer imports. Omuse's current
[0.5.0 qualification](release-050-qualification.md) records 970 passing cases,
all 80 template variants, exact-source CI and installed update/rollback checks.
Those receipts remain separate from this source-only competitor review. A
same-machine photo corpus and timing run is still required for any competitive
speed, quality or stability claim.

## Historical v1.2.3 editing and interaction review

| Area in OmaPhoto v1.2.3 | Omuse implementation and evidence | Remaining work |
| --- | --- | --- |
| LibRaw import and development | Retained original RAW bytes and 16-bit source; two-camera, same-LibRaw reference comparisons in [photo qualification](photo-release-qualification.md) | Broader camera/profile corpus; this is not independent colour-science certification |
| Camera Raw's nine sections | Light/Color, Effects, Curve, Mixer, Grading, Detail, Optics, Geometry and Calibration; preserved-C pixel references in `rust/tests/camera_reference.rs` | More graphical controls and targeted canvas gestures; the compatibility operation is still 8-bit and capped at 16 MP |
| Histogram, vectorscope and clipping | This pass adds RGB histogram and hue/saturation vectorscope to the existing clipping preview. Analysis excludes fully transparent pixels, weights alpha and samples at most 512×512 cells | The full suite and light/dark native inspection passed; scopes describe the selected layer's sampled display RGB, not sensor RAW/HDR |
| White-balance/defringe eyedroppers and targeted adjustments | Existing point-colour sampling, geometry-guide dragging and channel curve graph | White-balance/defringe sampling, targeted curve and HSL/mixer drags are still missing |
| Interactive Camera Raw preview | Temporary editor, no artwork/history commit until Apply; this pass moves the temporary selection-aware composite off the UI thread and rejects stale analysis/preview results | Still computes the grade at full source resolution. Add scale-aware bounded previews and compare spatial detail/glow against a downsampled full-resolution oracle |
| Layered PSD | Groups, opacity/fill, masks, clipping and supported adjustment/blend mapping; this pass adds always-on synthetic nested/masked/clipped fixtures with exact pixels | Real Adobe/Affinity/OmaPhoto corpus, layered compressed-channel combinations, unsupported constructs and conversion-report review |
| Trim | Transparent, top-left and bottom-right colour trim, per-edge options; `rust/tests/layer_canvas_parity.rs` | Real-photo/manual acceptance of edge tolerance |
| Object/Subject selection and morphology | Local U²-Net path, refine, feather, expand/contract; selection-aware transactions | Independent people/product/hair/fur quality corpus; release feature names do not establish segmentation quality |
| New adjustment layers | Black & White, Color Balance, Invert, Gaussian Blur, Motion Blur and Add Noise are represented in the engine/UI | Broader layered interchange and visual references |
| Inner/Outer Glow and added blend modes | Existing glow kernels and blend implementations; this pass adds exact PSD import/render checks for Vivid Light, Linear Light, Pin Light and Hard Mix | Multiscale preview and translucent/grouped cross-app references |
| Duplicate folders and group opacity | Descendants preserved by grouped duplication; group rendering/opacity tests. This pass also fixes selected-group pointer moves with linked-mask and one-step Undo preservation | Group resize/rotation, large deeply nested editing and interchange corpus |
| Brush smoothing | Screen-space pulled-string Brush smoothing; this pass extends it to Eraser, including masks and release catch-up | Pointer/tablet quality remains separate; Pencil stays unsmoothed for precise work |
| Middle-button pan | Added in this pass with independent primary/middle release state and lost-release cleanup | Native foreground acceptance across mice/trackpads and window edges |
| Auto Select | Frontmost visible nested hit; this pass fixes an already-selected background move box intercepting a higher layer | Bounding-box hit testing, as in the reviewed OmaPhoto source; not per-pixel alpha picking |
| Remember guides/grid/snap/Auto Select | Atomic view preferences already exist; this pass fixes adding a guide while hidden failing to persist auto-show | Real-file preference serialization is tested; actual UI restart and broader profile migration acceptance |
| Shortcut reference | Ctrl+K searches and executes commands; Ctrl+Alt+K records/remaps shortcuts, with conflict handling and generated gesture documentation | Non-US layouts, IME and physical acceptance; no claim of universal shortcut delivery |
| Lazy, fixed-width font picker | This pass loads and caches installed font names on first text-dialog use; dialog width and visible results were already bounded | Large/missing/unusual font collections and long-name visual acceptance |
| Version-9 project format | `.comp` versions 1–9 are accepted and saved as 9, with bounded validation and roundtrip tests | Bidirectional files saved by the actual OmaPhoto v1.2.3 application; a shared version number is not interchange certification |
| Low-memory export failure | Validation, bounded document admission, staged output and cancellation protect existing destinations | OmaPhoto has allocation-starvation tests. Omuse has not qualified allocator exhaustion; Rust OOM can terminate the process. Add a controlled low-memory harness and recoverable admission/isolation before claiming parity |

The relevant upstream tests include
[`CameraRawCanvasTests.cpp`](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/96ce546508a920e10f2747ad3907da6eb87f4700/tests/CameraRawCanvasTests.cpp),
[`ProjectExportFailureTests.cpp`](https://github.com/ZacharyZhang-NY/OmaPhoto/blob/96ce546508a920e10f2747ad3907da6eb87f4700/tests/ProjectExportFailureTests.cpp)
and the tagged source's brush, canvas-navigation and project-manifest tests.
Omuse's corresponding code lives in `rust/src/camera_raw.rs`,
`camera_canvas.rs`, `photo_scopes.rs`, `camera_scopes.rs`, `editor.rs`,
`psd.rs`, `raster.rs`, `document.rs`, `preferences.rs` and `ui.rs`.

## Development after v1.2.3

The first development review inspected source through
[`ffba175`](https://github.com/ZacharyZhang-NY/OmaPhoto/tree/ffba1753c62f5ad82a48248a598366c951faac8d),
five commits after the release. These changes were merged on upstream `main`
but **unreleased at that review**; they are included in 1.3.3. Source and tests were inspected;
neither editor was executed for this additional comparison.

| Upstream development | Omuse comparison and useful next work |
| --- | --- |
| [Finishing filters](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/fdb62978c54c1fdac95b1065919acf80a637982c): transparent-margin Bloom, richer Vignette and radius-based local Tonal Contrast | Omuse has all three names and tests in `rust/tests/raster_filters.rs`, but different semantics: Bloom/Vignette preserve transparent destination pixels and Tonal Contrast is per-pixel. Add explicit richer modes with pixel references, selection/cancellation checks and one-step Undo; do not silently break existing alpha guarantees. |
| [Stepped keyboard zoom](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/9ac3b1b81c9ff64bed214e270b7bd6531e296e65) preserves the document point at viewport center | Omuse 0.3.0 adds fixed 2%–1600% stops with centre anchoring, pointer-centred wheel zoom and round-trip tests; its automated/native qualification passed. The upstream document-tab sizing change has no equivalent in Omuse's current single-document window. |
| [Layer context menus and shared format version](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/fc076de71d1c873f94b69beb7db20b309e6826cc) | Omuse already targets the clicked row and retains multiselection, but its action labels/enablement are generic. Add capability-based menus and direct right-click tests, and centralize the format-version constant. The guide's stale writer version was corrected to 9 in this documentation pass. |
| [Whole-layer clipboard and multiple duplicates](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/7a873eb9c70b754060663bf83862eafcf2b5516e) | Omuse already duplicates multiple selected roots transactionally and remaps mask links. 0.3.0 retains whole selected trees in the running session with internal mask-link remapping, bounded admission and PNG interoperability. Automated/Cua checks passed; independent processes and clipboard managers receive PNG, not editable trees. Cross-document tabs are a separate design choice. |
| [Crop ratios, empty-layer Vignette and live text-color preview](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/ffba1753c62f5ad82a48248a598366c951faac8d) | Omuse 0.3.0 adds a movable ratio frame, corner resizing, orientation swap and reversible Apply/Cancel; automated/native/Cua checks passed. Vignette cannot paint an empty layer; text-color changes remain drafts until Apply. Explicit empty-layer Vignette and cancel-safe live color preview remain future work. |

Omuse 0.3.0 implements **anchored keyboard zoom**, **session layer Copy/Paste**
and **interactive crop ratios**. Its [qualification](editing-workflows-qualification.md)
records 901 passing cases, exact-source CI, production desktop checks,
clipboard exchange and installed rollback. This updates Omuse's status against
the first upstream snapshot above; those qualification results belong to that
Omuse runtime, not the newer upstream source reviewed below.
Remaining work includes bounded Camera Raw preview, missing Camera Raw gestures,
empty-layer Vignette, live colour preview and low-memory qualification. Relevant Omuse paths are
`rust/src/ui.rs`, `filters.rs`, `document.rs`, `shortcuts.rs`,
`rust/tests/raster_filters.rs` and `rust/tests/layer_canvas_parity.rs`.

### 30 September development review

The exact [nine-commit range](https://github.com/ZacharyZhang-NY/OmaPhoto/compare/ffba1753c62f5ad82a48248a598366c951faac8d...838d5b1459965e83f1693ff480fe60d14cff3377)
changes 201 paths. Relevant implementation, regression fixtures, test registration,
dependency and packaging changes were inspected. These changes were **merged but
unreleased on 30 September at that snapshot**; they are included in 1.3.3.
Upstream's references to Compositor versions 1.2.8–1.3 were not then new
OmaPhoto releases. Neither application's tests or runtime were executed for
this scheduled source review. The Omuse source comparison uses `488cf34` on
`feature/inspector-refinement`; earlier runtime evidence remains separately scoped.

| New upstream work | What the code/tests establish | Omuse gap and next qualification |
| --- | --- | --- |
| [PSB and oversized Photoshop imports](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/a2241601ec3e5ac10378d075689253b36dc257d8) | Version-2 lengths and RLE row counts, layerless merged images, and over-budget layer/mask cropping with a conversion notice. Tests compare PSD/PSB pixels and reject oversized/truncated data before large allocations. The reviewed channel decoder supports raw/RLE, with 8-bit RGB constraints. | `rust/src/psd.rs` explicitly rejects PSB. It already handles layerless PSD raw/RLE/ZIP/prediction with pixel tests, so that is not a new gap. Add bounded PSB decoding and optional, explicitly reviewed cropping; preserve off-canvas pixels by default. Test malformed lengths, masks, negative origins, all supported compression modes and actual Photoshop/Affinity exports. |
| [Editable Photoshop text](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/008493b0a3de461775014a4e8341e01fcd6d542b) | TySh descriptors become native point/paragraph text. Mixed styles use the first style with a notice; warps and faux styles are reported, missing fonts substitute, and unsupported placement/vertical/broken text falls back to cached pixels. This is not full text fidelity. | Omuse preserves cached text pixels and reports the loss of editability. Add supported descriptors with a comparison preview, explicit conversion notes and fallback; qualify fonts, rich runs, transforms, baseline placement and reopen. Omuse currently rejects embedded ICC profiles in PSD, whereas upstream's new merged-image path converts its profile; qualify that separately from layered colour fidelity. |
| [SVG/SVGZ import](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/8fbb104c485611e8eaa3c7805c5635d878b5d80a) | Qt SVG rasterizes to intrinsic size for a new document or to fit an existing canvas. Fixtures cover transparency, orientation, compressed files and size budgets. Shapes are not retained as editable vectors by this import. | Omuse uses SVG for packaged UI assets but has no document SVG import path. Add bounded raster import with selectable dimensions first, then consider editable conversion separately. Test external-resource rejection, malformed/compressed files, transparency and budget admission before advertising support. |
| [Live typed text and effect continuity](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/136ee65a48f60e2cf20f48f886b426a340fe86da) | Draft text uses its committed raster path, preserving layer order, opacity/blend and masks/effects; recent effect results support undo. Tests compare editing/committed output and cancellation, including moved text boxes and placed masks. The preceding commit adds baseline placement and Move-tool double-click editing. | Omuse's `inline_text_view` is a textarea overlay; commit/cancel/Undo and text metadata have tests, but this is not a matching live artwork preview. Add a reversible canvas draft rendered by the same text/effects pipeline, then verify exact preview/commit pixels across fonts, zoom, masks, blend modes and undo. |
| [Painting masks beyond their original bounds](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/f335818e533118c94ee1a8abde4671e1ca6c4b46) | Brush/fill/gradient growth preserves mask placement and its white/black outside coverage; placed-mask effect previews and thumbnails follow the same coverage rule. Other tools retain their existing mask bounds. | Omuse paints existing masks through their transform and clamps dabs to their raster bounds in `Editor::stamp_dab`. Add bounded mask growth without moving source artwork; test reveal/hide masks, linked/detached placement, rotated layers, effect previews, untouched corners and single-step Undo/save/reopen. |
| [Draggable numeric labels](https://github.com/ZacharyZhang-NY/OmaPhoto/compare/f335818e533118c94ee1a8abde4671e1ca6c4b46...aa253d97159ee6736307bab59bcd1ee9e0000116) | Shared scrubbing spans tools, opacity, transforms, sheets and selected Camera Raw rows. Tests cover clamping/rounding, disabled controls, lost releases, focused fields, layer switches and one-step opacity Undo. | Omuse's refined inspector still uses buttons/typed fields for these values and has no shared numeric-label drag control. Add a native control with visible affordance, precise typing/keyboard access, one transaction per drag, Escape restoration, loss-of-focus cleanup and target-change protection. The layout work alone does not qualify this interaction. |
| [Document budgets and selection outlines](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/8a9c56ee1fd677adca6c2c154fed0ee44d7911b5) | Adds 200 MP generated-surface limits and RAM-derived document budgets capped at 800 MP, with separate image/mask accounting in project storage. Complex zoomed-out ants are retraced asynchronously at reduced resolution; tests check stale outlines, fill rules and repaint pacing. | Omuse retains its fixed 100 MP limit and caches up to 200,000 contour points, scanning the mask on a selection revision. Add zoom-aware background outline generation with stale-result checks and measure large-selection UI latency. Higher pixel limits need peak-memory/low-memory qualification, including history, masks and concurrent previews; copying a larger constant is not a speed or stability improvement. |
| [External project changes](https://github.com/ZacharyZhang-NY/OmaPhoto/commit/838d5b1459965e83f1693ff480fe60d14cff3377) | Watches/coalesces changes, reloads clean projects, asks Revert/Keep Mine for unsaved work and defers while busy. Tests include package replacement, partial writes, background tabs and concurrent notifications. The digest hashes manifest bytes plus sorted image names/sizes; same-size image-byte rewrites deliberately do not change it. | Omuse already has locked atomic saves, consistent-read checks and metadata/inode conflict stamps in `save_guard.rs`, including rechecks before publication. It offers Save as on conflict, not automatic reload. Add an in-app external-change/reload decision with save-copy preservation and generation checks; test same-size image replacement, partial writes, changes during the dialog and active edits/AI requests. Do not replace the existing save guard with the weaker image digest. |

The upstream evidence includes `PSBImportTests`, `CropToCanvasImportTests`,
`PSDTextTests`, `PSDTextReaderTests`, `SVGImportTests`, `TypedTextCanvasTests`,
`MaskPaintAnywhereTests`, `MaskEffectsCanvasTests`, `NumericScrubTests`,
`Scrubbable*Tests`, `MarchingAntsTests`, `ExternalChange*Tests`,
`ProjectWatcherTests` and `ProjectDigestRaceTests`, registered in `tests/Tests.cmake`.
These are inspected regression intentions, not a report that we ran them.

Prioritize numeric controls, live text and mask growth for everyday editing;
then bounded SVG/PSB/editable-text import and external-change recovery. Profile
selection-outline latency before choosing a performance target. These are
planned improvements in the guide, not implemented Unreleased features.

## Distribution

OmaPhoto 1.3.3 publishes Ubuntu 24.04/26.04 DEBs, Fedora 43/44 RPMs, an Arch
package and checksums. Its source also provides a Nix flake. The Arch artifact
is about 174 MB; that download size alone says nothing about startup speed,
installed footprint or runtime memory.

Omuse has a one-command Arch/Omarchy source installer, complete-generation
update/rollback, and checksum bundle tooling. It does **not** yet have a
qualified downloadable native package. The two unresolved locked dependency
legal texts, clean-target installation and physical desktop acceptance remain
explicit [release gates](public-release-readiness.md).

Start with a qualified Arch package and AppStream metadata after the legal gate
is resolved. Build DEB/RPM/Nix support only with tests on each declared target;
an Arch-built dynamic executable is not automatically portable.

The broader watch also found these public proposals and reports:

- [AppImage PR #8](https://github.com/ZacharyZhang-NY/OmaPhoto/pull/8) is open and
  unmerged at `e2ad754`. Its current workflow makes Debian a release gate,
  permits Alpine failure and adds third-party glibc to Alpine. Its observed
  [PR run](https://github.com/ZacharyZhang-NY/OmaPhoto/actions/runs/36351361361)
  awaits approval (`action_required`); this is not evidence of a shipped or
  verified portable AppImage. For Omuse, require startup and real editing
  checks on each declared clean target before advertising support.
- [AppImage PR #10](https://github.com/ZacharyZhang-NY/OmaPhoto/pull/10) was
  closed without merging at `4af63d3`. Its only changed file is the AppImage
  workflow. The [run](https://github.com/ZacharyZhang-NY/OmaPhoto/actions/runs/36642096544)
  reports failure with no jobs returned; this is not evidence that a produced
  application failed a runtime test. No AppImage was added to the releases.
- The 1.3.3 source includes Qt SVG in CMake, Arch/Nix and distro build inputs,
  plus PSB/SVG desktop MIME entries. Post-tag packaging updates split Fedora
  releases by ABI; published package metadata and upstream-reported install
  checks are distinguished from independent runtime qualification above.
- [AUR concern #7](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/7) and
  [Flatpak request #4](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/4)
  reinforce demand for a simple installation path. Omuse's source installer
  uses `pacman` and verified runtime downloads without invoking an AUR helper;
  a portable package remains separate release work.
- [Interaction-preference request #3](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/3)
  asks for wheel/zoom, temporary tools and Photoshop-like preferences. Treat
  these as requests, not upstream implementation. Omuse's command search and
  remapping already exist; broader input preferences need design and tests.
- Closed [container import #1](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/1),
  [render-device #2](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/2) and
  [RPM dependency #5](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/5)
  are useful clean-target test cases, not reproduced Omuse bugs.
- New [cursor report #9](https://github.com/ZacharyZhang-NY/OmaPhoto/issues/9)
  describes corrupt custom Qt cursors on NVIDIA/Hyprland at fractional scale.
  It remains an unverified report for this comparison, with no cursor fix in
  the reviewed main-branch range. Add visible cursor/brush/transform checks on
  NVIDIA at fractional scale to Omuse's desktop acceptance matrix. Omuse uses
  GPUI rather than Qt; do not assume the reported failure or change users'
  compositor settings without reproducing a relevant problem.

## Where Omuse offers a broader workflow

These are implemented Omuse capabilities, not claims that OmaPhoto can never
support them:

- Editable multipage content collections, reusable components, brand resources,
  data-driven variants, 20 templates / 80 preset-size variants and social-safe
  composition guides.
- PNG/JPEG/WebP/TIFF, PDF and bounded MP4/GIF content export, plus motion presets.
- Subscription-aware in-app assistant connections with reviewed changes, Keep
  and Undo. Live qualification is operation/provider specific; the current
  bounded Codex receipts do not qualify every provider or image operation.
- Native `gpui-omarchy` theming and a searchable action palette that also
  explains current custom bindings and gestures.
- Region-based painting history and retained 16-bit sources with editable
  operations. Previous measured improvements compare Omuse with earlier Omuse
  candidates, not with OmaPhoto.

See the [project guide](project-guide.md), [Create qualification](omuse-create-qualification.md),
[AI GUI qualification](cua-ai-qualification.md), and
[performance roadmap](rust-performance-roadmap.md) for the scope of the evidence.

## What would establish “as good or better”

1. Complete the missing Camera Raw gestures and bounded preview work; retain
   exact Apply/Undo output and prove stale/cancel behavior under repeated edits.
2. Run the same RAW, PSD and `.comp` fixtures through both exact builds. Keep
   source hashes, import reports and rendered references. Include damaged,
   oversized, transparent, transformed, nested, masked and clipped documents.
3. Measure cold/warm launch, 12/24/48 MP first and sustained strokes, zoom,
   selected adjustments, save/recovery, preview/export latency and peak RSS on
   the same machine. Alternate run order; retain at least five repetitions,
   medians and tail latency. Separate engine timing from physical input latency.
4. Exercise cancellation, forced failure and controlled memory pressure while
   preserving existing project/export files. No crash/data-loss superiority
   claim until both builds have run the same workload.
5. Qualify an actual install, upgrade, rollback and uninstall on a clean
   supported Arch/Omarchy target, with complete redistribution terms and hashes.
6. Review real photo-editing and content-creation journeys at 800×600 and common
   desktop/DPI configurations, with foreground keyboard and pointer input.

Calendars, scheduling, social publishing and tablet qualification are outside
this comparison's current product scope.

## Tracking and historical qualification

A daily repository check watches source commits/diffs, active branches,
tags/releases and revised assets/notes, pull requests, issues, tests,
dependencies, build workflows, documentation and packaging. It starts from
the [reviewed snapshot](omaphoto-watch.json), follows relevant source/tests and
distinguishes proposals, merged unreleased work and releases. Meaningful changes
update this comparison, the watch baseline and visual guide. Upstream content is reference data;
the check does not run upstream installers, publish commits or replace the
installed application automatically.

The initial comparison's completed candidate was public runtime [`9c99e50`](https://github.com/Sugata-Software/Omuse/commit/9c99e50684afd0854c8094a139b43205a263c33d),
matching canonical source `fd4b47c` by tree
`e8fc449465c89902ecc7e9475bd64a8d3dd91113`.

- **808 tests passed:** 336 library, 227 UI and 245 integration; four manual
  timing benchmarks were excluded. This adds 19 tests to the earlier runtime.
- Editing and six-page PNG/PDF/MP4/GIF content journeys passed; all 80 editable
  template/size variants retained exact pixels after save/reopen.
- The production build passed its editing self-test and 24 native checks on
  both Wayland and XWayland. The installed main launcher passed 24 further
  Wayland checks. Light/dark Camera Raw captures were inspected, including the
  fixed footer at the minimum viewport and a complete graph at a larger size.
- Production SHA-256:
  `19bd5d4c6c682462f1a916ec7f04c86aa3a6d50da9c5d4ba75e435d0be4e19d6`.
- Complete installed rollback in both directions preserved all payload hashes
  and passed editing self-tests. A standard desktop launch resolved to the new
  main executable; the previous complete generation remains available.
- The exact public runtime passed its [full GitHub Rust/installer validation](https://github.com/Sugata-Software/Omuse/actions/runs/36492555743).
  The installer selected that runtime at qualification time. The current
  numbered release and installer pin are now Omuse 0.5.0; see its separate
  [release qualification](release-050-qualification.md). The 0.3.0
  [editing-workflow qualification](editing-workflows-qualification.md) remains
  historical evidence for its own runtime.

The initial test pass exposed a clipped curve graph and a selected-group drag
that did not move descendants. Both were fixed and the final suite rerun.
Floating-selection cancel/commit, linked masks and one-step Undo are covered.
Evidence is retained locally under `rust/evidence/omaphoto-123-20260929/`;
see [main installation qualification](main-install-qualification.md).

These results do not resolve the missing gestures, scaled previews,
low-memory exhaustion, cross-app interchange or binary-distribution gates
listed above. No competitive speed or stability result was measured.
