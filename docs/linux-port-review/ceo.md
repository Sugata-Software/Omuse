# CEO review: compatibility port and convergence

Mode: SELECTIVE EXPANSION. This review preserves every mandatory destination in the original brief. A usable v1 is a release milestone, not a redefinition of the complete port. Implementation has not begun in this review.

## 0A. Premises and outcome

P1: The outcome is a trustworthy Linux compositing editor with familiar macOS behavior, not successful compilation. README's layer, mask and non-destructive transform workflows support this premise. A multi-layer document must remain editable after save/reopen; a flattened PNG round trip cannot satisfy it.

P2: Keep Swift/C and use Qt only for presentation. The eight C modules are already shared by CMake and SwiftPM, and the original renderer/store remain available as reference. The current portable Swift renderer is useful migration evidence, but preserving a second production compositing implementation would contradict the newly supplied destination.

P3: CoreGraphicsCompat is a bounded compatibility surface, not a CoreGraphics clone. The scope list is incomplete: LayerRenderer uses device transforms, clip bounds and antialias state; TiledLayerRenderer uses path subtraction and transparency layers; BrushStroke uses gradients; RasterSnapshot uses lazy CGDataProvider callbacks. Keep the approach, but price and schedule it from a call-site inventory and a representative compile/render spike.

P4: CPU-addressable document state makes fallback possible, but does not itself make GPU failure safe. Pending operations, GPU resource lifetime, callback invalidation and a replacement raster presentation surface need explicit state transitions. Treat their tests as part of v1.

P5: The new Freedesktop 26.08 + bundled Qt/Swift target supersedes the older KDE SDK packaging recommendation in this review. Release announcements prove version availability, not toolchain/runtime compatibility. Stage 0 remains a real feasibility gate, with exact archives and transitive dependency pins to be selected by successful build evidence.

P6: Full `.comp` semantic compatibility is mandatory. Original ProjectStore checks format versions 1–7, sRGB, asset names and graph validity, and uses coordinated atomic replacement. A Linux-only ZIP with the same suffix would not open in that reader; retain directories, and escalate any container change as a format decision rather than a hidden adapter detail.

P7: Preservation percentages are hypotheses, not measured savings. Count shared source, conditional source, adapters and replacement source separately after the first representative compile. “No rewrite” is not equivalent to “unchanged bytes” and neither establishes behavioral parity.

Premise gate context: the previous turn asked whether to review convergence or only history. The continuation instructed work from current authoritative state toward the attached destination. This review follows that instruction; it does not record an explicit A/B response or authorize implementation. Toolkit, CPU fallback and source-retention premises are explicitly present in the user brief.

## 0B. What already exists

| Subproblem | Existing code | Disposition |
|---|---|---|
| Pixel algorithms | Compositor/Rendering/{Adjust,Brush,Heal,Levels,Wand,Noise,Lens}Pixels.c and ContentFill.c | Share the same sources; preserve padded-stride call sites and validate portability guards |
| Document/history | Compositor/Document; Sources/CompositorCore/Document | Compare behavior and reconcile source sharing; do not create a third model |
| Geometry | Original LayerTransform and portable CompositorGeometry | Reuse formulae, explicitly test Foundation type collisions and transform multiplication |
| Render orchestration | Original LayerRenderer, TiledLayerRenderer, LiveMaskRenderer | Compile against narrow compatibility layer; preserve tiling, clipping and caches |
| Current Linux rendering | Sources/CompositorCore/Rendering/{LayerRenderer,DocumentRenderer}.swift | Test evidence and transition scaffold; not final CPU fallback |
| Format | Original IO/ProjectStore.swift and docs/project-format.md | Keep schema and validation; replace only filesystem/codec boundaries |
| App startup | Sources/CompositorHostBootstrap/hostMain.swift, host/host_run.cpp | Retain proven Swift-entry shape as starting point; revalidate with selected toolchain |
| Presentation | host/SessionWindow.cpp, dialogs and CanvasWidget | Reuse widgets/routing where semantics match; upgrade layers to tree adapter |
| ABI | include/CompositorCore.h, EditorBridge.swift | Reuse bounded commands and ownership documentation; add snapshot/revision consistency |
| Tests | CompositorTests, Tests/CompositorCoreTests, tests | Classify reference vs actual Swift vs Qt integration; no inherited pass claims |
| Packaging | Two current Flatpak manifests and provenance files | Converge to requested baseline; remove stale/development permissions from release |

## 0C. Dream state

```text
CURRENT                          THIS PLAN                         12-MONTH IDEAL
macOS oracle + partial Linux -> retained core + one Skia contract -> same document semantics
duplicate raster logic         trusted package transactions        shared regression fixtures
KDE SDK prototype              requested sandbox/toolchain         reproducible maintenance
partial Qt workflows           complete v1 and full-parity backlog full original feature matrix
```

The delta after v1 remains HEIC/TIFF where deferred, foreground segmentation quality, final panels/cursors, cross-project DnD edge cases and performance acceptance. These remain tracked Stage 12 work; they do not vanish because v1 is usable. New unrelated editor capabilities are outside this port.

## 0C-bis. Alternatives

| Approach | Effort (human / agent-assisted) | Risk | Pros | Cons | Reuse / completeness |
|---|---|---|---|---|---|
| A. Converge existing work through a representative compatibility slice, then all surfaces | XL / L, re-estimate after spike | High, localized by gates | Preserves target and useful work; detects semantic risk early | Temporary scaffolds need explicit removal; mixed APIs during transition | Swift/C/Qt/tests; 10/10 destination coverage |
| B. Complete current custom Swift raster architecture | L / M | Medium initially, high maintenance | Smallest immediate diff; existing tests apply | Violates one Skia contract; duplicated semantics survive | Existing prototype; 6/10 target coverage, not selected |
| C. Start a parallel clean port from a19db90 | XL / XL | High | Simple initial source provenance; pristine reference | Repeats host/build/ABI effort; discards useful tests and fixes | Original core; 10/10 eventual coverage |

Recommend A under explicitness and reuse principles. B is a genuine alternative but not an authorized replacement for the user's destination. C offers no demonstrated advantage sufficient to justify another application implementation.

## 0D. Scope calibration and expansions

The >8-file smell is inherent to a platform port, not a reason to shrink it. Reduce moving parts by one session owner, one renderer contract, one package transaction boundary and one authoritative release recipe. A manifest dependency check that never draws with Skia is insufficient.

The ambitious version is the entire original editor on Linux with transferable documents; it is already the destination. Adjacent improvements considered: visible renderer status (accept, aids fallback diagnosis), dirty-title indicator (accept, needed for safe save), actionable package errors (accept), keyboard focus restoration after dialogs (accept), export alpha/background explanation (accept), automatic cloud sync (exclude, unrelated), a plugin API (defer, unrelated infrastructure). The five accepted items are requirements inside existing flows, each small enough to fit those components without new services.

## 0E–0F. Temporal interrogation and mode

| Implementation period | Decision fixed now | Evidence required |
|---|---|---|
| Hour 1 foundations | Exact SDK/compiler source, app ID, build/runtime separation | Stage-0 reproducible dependency smoke and runtime-only launch |
| Hours 2–3 core | Pixel order/premultiplication/row orientation, ownership, source-sharing map | ABI and first CG-shaped fixture |
| Hours 4–5 integration | Main-thread actor/Qt queue rule, package capabilities, revision tokens | Native event loop and transactional save tests |
| Hour 6+ testing | Oracle availability, seam/alpha tolerances, device-loss and portal coverage | Artifact-level tests and platform matrix |

These are decision-order labels, not a six-hour promise for the port. No fixed AI speedup or preservation percentage is treated as an estimate. SELECTIVE EXPANSION retains full scope and adds only behavior necessary for the specified workflows.

## 1. Architecture review

The principal correction is to make the compatibility surface and dependency order executable. Stage 3 cannot promise most document/history tests before the image/geometry types from Stage 4 exist. Implement geometry and CPU-backed image ownership first, then the representative retained renderer; expand the same source target afterward.

```text
Swift @main -> Qt application/event loop -> C command API -> Swift EditorSession
                                                      -> immutable document/history
Qt view adapter <- revision/state snapshot <-----------+
retained CanvasScene/Layer/Tiled/LiveMask renderers
    -> CoreGraphicsCompat -> narrow C -> Skia Raster | Skia Vulkan
CPU assets/tiles <--------------------------------------+
ProjectStore -> ProjectPackageIO -> portal capability + filesystem transaction
             -> PlatformImageCodec -> Skia codec / specific extra codec
existing C kernels <-> bounded CPU buffers; OpenCV only behind named operations
```

10x layers first stresses whole-canvas intermediates and UI models; 100x pixels exceeds memory before arithmetic throughput. A failed GPU must leave authoritative Swift state usable; a failed save must leave the previous valid package readable. Rollback is a previous Flatpak commit plus compatible files, never a destructive schema downgrade.

## 2. Error & Rescue Registry

Names below are proposed typed categories, not claims that current code implements them. Every row needs a test and must carry operation/revision context without logging image content or personal paths by default.

| Codepath | Error category / trigger | Required rescue | User sees | Current proof |
|---|---|---|---|---|
| Bootstrap/toolchain | RuntimeDependencyMissing / missing Swift or Qt plugin | Fail launch preflight with dependency name and repair command | Clear startup diagnosis | Requested baseline unbuilt |
| C ABI request | InvalidArgument, UnsupportedVersion, InvalidHandle | Reject before mutation; no exception crosses C | Action rejected, document intact | Existing status codes; broader coverage to inspect |
| Raster allocation | AllocationBudgetExceeded / checked image-size budget | Preflight before allocation; retain previous state | Document too large, safe cancel | Limits exist; total-live budget missing |
| Import codec | UnsupportedFormat, DecodeFailed, InvalidProfile | Reject import atomically | Filename-safe explanation and supported formats | Qt codec tests cover a subset |
| Project read | InvalidPackage, MissingAsset, UnsupportedProjectVersion | Preserve current document; never partial success | Named missing/invalid resource | Original validator reference |
| Portal request | Cancelled, PermissionDenied, PortalUnavailable | Cancel is no-op; denial can reopen chooser | Retry/location choice | Native GNOME/KDE proof missing |
| Package stage/write | NoSpace, ReadOnly, WriteFailed | Preserve old package, dirty flag and safe staging state | Retry/Save As with reason | Transaction proof missing |
| Package commit | Conflict, RenameFailed, SyncFailed | No partial replacement; recover previous generation | Save not confirmed; recovery guidance | Portal filesystem proof missing |
| Preview/edit worker | Cancelled, StaleRevision, TargetClosed | Drop result, release handles, no undo item | Preview cancelled or no visible stale update | Some existing stale-target tests |
| GPU initialization | LoaderMissing, NoDevice, UnsupportedQueue, SurfaceFailed | Create Raster presentation | Usable canvas, optional status explanation | Manifest currently disables Skia GPU |
| GPU runtime | DeviceLost, SwapchainOutOfDate | Stop submissions, invalidate GPU epoch, release safely, redraw Raster | Editing continues; no lost committed stroke | Not demonstrated |
| Segmentation | ModelUnavailable, InferenceFailed | Keep image and matte unmodified; explain unavailable feature | Actionable model error | Stage 12 work |

## 3. Security and trust boundaries

Local files and clipboard/DnD are untrusted inputs even though no server exists. High-impact risks are decompression bombs, integer overflow, package path escapes and symlink races; likelihood is medium for malformed inputs and lower for targeted attacks. Preserve original validation and add descriptor-relative traversal checks, decoder dimension preflight, bounded commands and aggregate memory limits.

Portal capability is an authority boundary, not just a path string. A selected package may grant no sibling write authority, so an atomic sibling-replacement algorithm needs demonstrated parent capability or a compatible in-directory transaction. Runtime permissions must exclude home/host/network; dependency download belongs to the builder fetch stage. No new authentication system, credentials, SQL or cloud audit service is needed.

## 4. Data flow and interaction edge cases

```text
file/clipboard -> bounded decode -> normalized CPU asset -> session transaction -> redraw
 missing/cancel -> no-op; empty/invalid -> typed rejection; decode error -> no state change

save request -> immutable revision snapshot -> staged assets -> manifest -> commit -> saved revision
 missing grant -> chooser; empty layers -> valid empty doc; disk error -> old package + dirty state

input -> view-to-document coordinate map -> edit transaction -> tiles/history -> render
 no target -> disabled; zero movement -> no undo; focus loss -> defined cancel/commit; stale result -> drop

CPU document -> Skia backend -> presentation
 no GPU -> Raster; zero surface on minimize -> suspend; device loss -> new backend epoch -> redraw
```

Double-save must coalesce or serialize, close during save must await/cancel without falsely clearing dirty state, and editing during save must compare saved revision with current revision. Layer DnD must validate cycles and target identity at commit, not row index at drag start. Failed tab open must leave current tab intact; repeated input and a released pointer outside canvas must terminate an edit exactly once.

## 5. Code quality review

Current CMake explicitly states no target links Skia/OpenCV yet, while the brief requires real raster operations in Stage 0. The portable LayerRenderer implements blend formulae and sampling loops, creating a second semantic owner; reconcile it through retained source plus compatibility, preserving its tests as regression cases. Avoid renaming every file or moving all existing code under linux simply to match an illustrative directory tree.

Use the original immutable history/tile contracts as style references. Keep the bounded C ABI and platform-specific dialog interfaces; avoid the current pair of different host executables becoming independent supported products. Remove obsolete composition-root comments when the new build proves runtime behavior; current comments about an impossible C++ entry are observed toolchain failures, not a universal Swift restriction.

## 6. Test review

```text
UX: create/open/edit/layers/masks/undo/save/reopen/export -> packaged Qt journey
data: bytes/orientation/profile/schema/assets/history -> unit + codec/package integration
render: geometry/path/clip/blend/tiles/downsample -> macOS reference vs Skia Raster -> Vulkan
async: preview/save/close/cancel/stale generation -> deterministic scheduler tests
external: loader/portal/driver/window/plugin -> real Flatpak desktop matrix
rescue: every registry row -> fault injection + old-state/old-package assertions
```

Existing Swift tests and C reference tests are different tiers; neither demonstrates the shipped Swift↔Qt ABI by itself. Shipping confidence requires a real `.comp` with groups, masks and adjustment state, edited then saved, reopened on both platforms and rendered against independent references. Chaos tests kill the saving process at every transaction boundary and inject device loss during a transient edit. Random image tests use recorded seeds and oracle thresholds fixed before evaluating candidate output.

## 7. Performance review

The original sparse tiles/lazy materialization/downsample cache must survive porting. A 100-million-pixel RGBA buffer alone is 400 MB; several per-layer intermediates can exceed ordinary desktop memory even below existing dimension limits. Budget live scratch, decoded assets, history and GPU caches together; no full-canvas copy on every pointer event.

The top paths to measure are brush-to-present, tiled compositing at reduced zoom, and save/export materialization. The plan has no measured p95/p99, so numbers must be established using fixed hardware and reference documents, with peak RSS and allocations as well as frame time. Database/N+1/connection-pool checks were evaluated and are inapplicable to this local editor; the analogous risk is repeated full-layer scans and full-size image generation.

## 8. Observability review

Record selected renderer, fallback reason, device epoch, build dependency IDs, operation duration and bounded error category locally. Provide a diagnostics view/export with paths and image content omitted by default; no network telemetry is added. A report of a blank canvas should distinguish failed import, failed raster allocation, GPU initialization and presentation loss.

Tests must assert fallback reason as well as continued rendering, preventing an accidentally CPU-only build from passing Vulkan tests. Package save logs need stage and snapshot revision to explain recovery, but not entire manifests. The runbook covers forced Raster, revoked portal grant, missing plugin, failed package commit and incompatible runtime.

## 9. Deployment and rollback

```text
pin/fetch -> offline build -> unit/ABI -> package -> runtime-only smoke
 -> native desktop/driver matrix -> preview artifact -> approved stable release
bad release -> select previous Flatpak commit -> open unchanged format -> verify editable project
```

Use one app ID consistently in command, desktop/MIME/AppStream and portal grants. The supplied lowercase ID differs from the current capitalized ID; choose once before distribution, and treat migration of existing local permissions/data as an explicit task. Publication and signing are not performed by this planning run.

The first five minutes after installation cover launch, dependency diagnostics, import, edit and a save/reopen; the first-hour matrix includes CPU mode, device-loss injection, GNOME/KDE portals and display scaling. Release rollback must not require reopening a new Linux-only archive format. No database migrations exist; package compatibility is the equivalent irreversible risk.

## 10. Long-term trajectory

The maintainable endpoint has one source of document semantics and one renderer contract with two Skia devices. A compatibility wrapper is useful only while bounded by actual Compositor call sites; reject unrelated API emulation. Store the source-retention map and reference fixtures with the project so a new contributor can update upstream behavior without guessing which Linux copy is authoritative.

Architecture reversibility is 4/5 while adapters remain private; storage-format or distributed app-ID changes are 2/5. Technical debt to retire includes custom production raster paths, duplicate bootstrap/build routes, committed generated Qt moc output and outdated SDK documentation. Full feature parity and operation-specific OpenCV remain future stages of this same objective, not separate abandoned scope.

## 11. Design and UX

```text
Empty window -> New/Open/Import -> Canvas + layers + tool state
 -> transient edit/preview -> Commit or Cancel -> Dirty document
 -> Save pending -> Saved revision | Error, still dirty
 -> Close -> Save / Discard / Cancel -> Next tab or empty window
```

Map existing macOS commands and editor hierarchy into native Qt Widgets without changing workflow meaning. Specify loading, empty, error, success and partial-progress states for open/save/import/filters rather than leaving them to widget defaults. Keyboard focus, shortcut routing, high-DPI coordinate conversion and accessible numeric alternatives for transform handles are acceptance criteria; responsive behavior here means desktop resizing and fractional scaling, not inventing mobile UI.

## NOT in scope

1. Swift-to-C++ rewrite, Qt-owned duplicate document model, or production custom CPU compositor: contradict target.
2. General CoreGraphics/AppKit/SwiftUI/CoreImage framework clones: unnecessary API surface.
3. Cloud sync, accounts, hosted telemetry or plugin marketplace: unrelated product work.
4. New smudge/liquify/tablet features beyond the audited macOS behavior: require a baseline feature inventory before claiming parity.
5. Publication/merge/deployment: requires implementation and release evidence; this run creates plans only.

Deferred beyond v1 but retained in Stage 12: HEIC/TIFF as allowed, segmentation, remaining original filters/panels/cursors, cross-project DnD details, performance closure and optional brush GPU acceleration. Existing Vulkan brush code can be retained isolated without making it a v1 dependency.

## Failure Modes Registry

| Codepath | Failure | Rescue in new plan | Test now proven? | User result required | Logged? |
|---|---|---|---|---|---|
| Packaged launch | Swift/plugin dependency absent | Preflight + repair | No | Actionable launch error | Required |
| Raster scene | Incorrect alpha/path/tile semantics | Reject parity gate | No independent golden proof | Correct pixels before release | Fixture diff |
| Import | Huge/malformed image | Preflight/atomic rejection | Partial legacy coverage | Existing doc intact | Required |
| Package open | Missing asset/symlink escape | Whole-open rejection | Partial core validation | No partial document | Required |
| Save | Crash/no-space midway | Recover old/new whole generation | No | Work not silently lost | Required |
| Portal | Grant revoked/cancel | Reauthorize/no-op | No desktop proof | No false save success | Required |
| ABI | Stale/closed handle | Reject | Existing API declares it | No crash/use-after-free | Bounded |
| Preview | Result arrives after close | Revision/generation rejection | Partial | No resurrection/undo corruption | Required |
| GPU startup | No loader/ICD/device | Raster | No Skia proof | Fully usable editor | Required |
| GPU runtime | Device lost | New raster epoch/surface | No | Latest committed pixels recover | Required |
| Build | Offline dependency fetch attempt | Fail/re-pin sources | No target build | Reproducible artifact | Builder log |
| Regression | macOS source divergence | Reference CI gate | Not run here | macOS behavior preserved | CI artifact |

All rows are obligations. Four release-critical areas lack decisive implementation evidence: reference render parity, transactional package persistence, packaged dependency closure and Skia GPU/fallback. Specifying rescue is not proof it exists.

## Completion Summary

| Item | Result |
|---|---|
| Mode/System audit/Step 0 | SELECTIVE; current branch vs historical oracle separated; 7 premises evaluated |
| Architecture | Compatibility inventory, ordering, single ownership and memory boundaries specified |
| Errors | 12 error categories mapped; required rescues stated, not yet implementation-proven |
| Security | File/path/decoder/budget/capability boundaries covered; high-impact data-integrity risks |
| Data/UX | Four flow families and double-save/close/stale-input shadow paths mapped |
| Code quality | Three parallel semantic/build ownership risks identified |
| Tests | Tier diagram; independent pixel, package crash and artifact gaps identified |
| Performance | Three measured paths required; 400 MB base-buffer example |
| Observability | Local diagnostics, backend reason and save-stage context required |
| Deployment | Offline build/runtime-only proof, app ID and rollback gates |
| Future | Reversibility 4/5 for adapters, 2/5 format/ID; debt retirement explicit |
| Design | State map, keyboard/HiDPI/accessibility passed to design review |
| Required outputs | Existing leverage, dream delta, NOT in scope, 12-row failure registry and six diagram types written |
| Scope candidates | Seven considered; five inside-flow requirements accepted, cloud excluded, plugin infrastructure deferred |
| Independent voices | Pending below; no consensus claimed yet |
| Review status | Primary CEO analysis complete; phase waits for independent voices |

## Independent GPT subagent findings

The independent reviewer read the original brief and macOS source, without prior review context. Six findings: HIGH workflow-level release acceptance missing; HIGH save crash/cancel guarantees under-specified; HIGH Linux-only archive breaks macOS reader; HIGH retention percentages hide maintenance cost; MEDIUM early OpenCV smoke versus per-operation adoption ambiguity; MEDIUM platform capability disclosure incomplete. All are folded into proposed requirements except removing the requested OpenCV Stage-0 smoke, which remains as a dependency proof and does not mandate using OpenCV in any operation. No change to the selected stack is recommended.

The reviewer judged the user problem and chosen architecture sound, with conditional premises/trajectory, insufficient workflow calibration and insufficient user-trust criteria. This is a GPT-family review, not a Claude review or cross-model consensus.
