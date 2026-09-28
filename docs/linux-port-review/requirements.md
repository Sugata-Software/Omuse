# Requirements and evidence ledger

The verbatim brief in original-plan.md remains authoritative. “Specified” below means a requirement is preserved in this review, not that the application implements it. No unrun test, source comment, dependency presence, historical pass count or candidate-generated image is accepted as implementation proof.

## Original numbered sections

| Original section | Required disposition / proof | Review location |
|---|---|---|
| 0 Baseline | Keep a19db90 as macOS 1.0.4 oracle; separately identify current branch | plan.md, ceo.md |
| 1 Apple inventory | Recheck literal imports and real call sites; Metal brush is distinct from canvas renderer | ceo.md, engineering.md |
| 2 Swift reuse | Preserve domain ownership; classify portable, platform, UI and rendering code | ceo.md leverage table |
| 3 CoreGraphicsCompat | Actual API inventory, canonical CPU pixels, Skia backend, no general clone | engineering.md compatibility contract |
| 4 Existing C | All eight modules from shared original sources; behavioral tests and portability fixes only | test-plan.md C tier |
| 5 Metal/Vulkan | CPU brush first; independent optional acceleration; no direct Vulkan LayerRenderer | engineering.md backend contract |
| 6 CPU failsafe | Missing loader/device/queue/swapchain/ICD and runtime loss exercised | test-plan.md GPU tier |
| 7 Qt Widgets | Native menus/docks/dialogs/tree, Swift authoritative state | design.md |
| 8 Flatpak foundation | Exact pinned SDK/toolchain/Qt/Skia/OpenCV; actual calls and runtime-only launch | plan.md delivery gates, dx.md |
| 9 Swift/C++ ABI | C/POD/opaque handles, ownership/version/thread/error contracts | engineering.md ABI |
| 10 Stages 0–12 | Every stage retained below; order corrected for dependencies | Stage table below |
| 11 Codecs | PNG/JPEG orientation+sRGB normalization; later HEIC/TIFF adapters | engineering.md codec contract |
| 12 Format/filesystem | Keep manifest and directory compatibility; safe package transactions | engineering.md persistence |
| 13 Platform boundaries | Small role-specific adapters and RendererDevice | engineering.md architecture |
| 14 Layout | linux tree is illustrative; retain current paths where useful, no C++ Document clone | plan.md convergence |
| 15 Migration matrix | KEEP/SHIM/ADAPTER/UI ownership retained; separate implementation verification | ceo.md, test-plan.md |
| 16 Test pyramid | Original assertions → core → Raster oracle → Vulkan → Qt → Flatpak portals | test-plan.md |
| 17 Minimal changes | Original algorithm/source correspondence measured; duplicate implementations retired | engineering.md source-sharing |
| 18 v1 | Full listed editing/document/workflow set below; GPU and CPU both required | v1 table below |
| 19 Costs | Percentages explicitly unproven; measured spike before estimating | ceo.md |
| 20 Risks | UI/input, CG semantics, GPU sync, package portals, segmentation | ceo.md registries |
| 21 Shim leverage | Geometry/image/context/path/blend/resampling reused by real call sites | engineering.md |
| 22 Non-shims | No SwiftUI/AppKit/CoreImage object-graph/Vision/Sparkle emulation | ceo.md NOT in scope |
| 23 Commit sequence | Dependency-correct slice; Raster correctness before Vulkan presentation | plan.md delivery gates |
| 24 End architecture | Shared Swift+C; Qt presentation; CoreGraphicsCompat→Skia GPU/CPU; selected OpenCV | engineering.md diagram |

## Every original stage and its completion evidence

| Stage | Retained scope | Evidence necessary to close implementation |
|---|---|---|
| 0 | Freedesktop 26.08 build/runtime, pinned Swift 6.4 and Qt 6.11.2, Skia, Vulkan loader, OpenCV smoke, desktop assets | Clean fetch; network-disabled build; installed app executes Swift, original C, Qt, Skia Raster operation and OpenCV operation; Vulkan enumeration failure still succeeds; runtime-only install; no compiler shipped |
| 1 | Bidirectional C ABI | Real Swift-host integration, ownership and cancellation stress; sanitizers with justified runtime suppressions only |
| 2 | Eight original C modules | Calls and expected outputs for every module; valid padded-stride cases and invalid boundary cases; source manifest |
| 3 | Portable Swift model/history/workspace/tools | Retained/shared source compiles against foundational shim subset; original assertions ported and independently reviewed |
| 4 | Geometry/images/contexts/paths/color compatibility | API inventory covers every retained call; conformance fixtures; direct-data lifetime and immutable snapshot checks |
| 5 | Skia Raster, original renderer orchestration, caches | Mac-generated references for transforms/masks/clipping/blends/tiles/downsampling/export; sparse memory and no seam tests |
| 6 | Window/canvas/input/basic menus/zoom/pan | Real Qt event routing, native Wayland and XWayland; focus/coordinates tested at fractional DPI |
| 7 | Tabs/layers/import/save/load/masks/blends | Editable multi-layer cross-platform round trip; no flattened-export substitution; close/reopen and revision correctness |
| 8 | Skia Vulkan presentation and CPU fallback | Same render fixtures, real GPU execution evidence, startup failure matrix, loss during edit, resize/minimize, no higher-level renderer fork |
| 9 | Move/zoom/pan/transform/groups/masks/CPU brush/selections/wand/adjustments/filters/healing/content-aware/clone/smudge/liquify | Per-command mapping and complete test inventory. Smudge/liquify names remain listed; determine original equivalents or explicit new-feature scope before claiming completion |
| 10 | Operation-specific OpenCV; segmentation generator adapter | Each use justified by implementation/parity measurement; original C retained; selected model and preprocessing validated for later segmentation stage |
| 11 | Portals/access/Save As/clipboard/DnD/activation/HiDPI/metadata/MIME/icons | GNOME Wayland, KDE Wayland, X11/XWayland and CPU/Intel/AMD/NVIDIA matrix; artifact permissions and portal grants inspected |
| 12 | HEIC/TIFF/JPEG metadata/segmentation/remaining filters/panels/cursors/cross-project DnD/performance | Full original feature matrix; per-feature cross-platform fixtures and workflow proof. Tablet and Vulkan brush compute retain original optional qualification |

## Linux v1 release ledger

| Feature set | Required result | Test family |
|---|---|---|
| Platform | Flatpak, requested Freedesktop baseline, Qt Widgets, Wayland/XWayland | Runtime artifact and desktop matrix |
| Rendering | Skia Raster always; Skia Vulkan preferred when operational; automatic safe fallback | Raster oracle/GPU parity/fault injection |
| Projects | Create/open/save editable `.comp`, multi-project state/unsaved protection | Package+Qt journey |
| Codecs | PNG/JPEG import/export, orientation and color normalization, export options | Codec fixtures |
| Layer model | Multiple layers, groups, visibility, opacity, blend subset with truthful unsupported-feature handling, reorder | Original model tests + QTreeView routing |
| Masks | Raster masks, basic edit, correct alpha/placement/link semantics for supported documents | Mask+render+history tests |
| Editing | Pan/zoom/move/scale/rotate/flip, CPU brush | Input/transform/brush/undo tests |
| Selection | Rectangle/ellipse/lasso/magic wand | Geometry, coverage, selected-pixel action tests |
| History | Undo/redo each supported committed action; one item per stroke/preview accept | Original history assertions + real UI routing |
| Processing | Levels/curves/exposure-adjustments/noise/lens/healing/content-aware scope as listed | Original C + operation adapters + selected-area preview/cancel |
| Trust | Save error cannot destroy old project; unsupported read/edit/render/export behavior visible | Failure registry and capability matrix |

## Current evidence status

Verified in this review: source brief copied byte-for-byte; historical commit exists; branch identity and dirty user edit recorded; original selected renderer/store files match historical commit; current manifest uses KDE SDK, disables Skia GPU and grants home access; CMake says dependencies are discovered but not linked into a renderer; portable LayerRenderer contains independent sampling/blending loops; current save implementation uses sibling temporary/backup rename stages without a demonstrated crash-durable transaction. These facts contradict a claim that the target is already complete.

Not yet proven: requested full Flatpak build, all retained source on Linux, CoreGraphicsCompat coverage, macOS/Skia reference parity, actual Skia Vulkan canvas, runtime fallback, narrow-permission portal journeys, full feature parity, performance acceptance and macOS regressions. Review documents specify how to prove these; they must remain unchecked implementation tasks.
