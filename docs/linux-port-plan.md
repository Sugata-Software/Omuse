## Implementation plan
# Compositor Linux port plan

Status: DRAFT. Intake complete; reviews pending. No implementation has been performed.
Date: 2026-09-19. Repository: chiddekel/Compositor. Branch/base: main/origin/main. Baseline: a19db90.

## User intent and confirmed premise

Create a Linux version using Swift and the Freedesktop SDK ecosystem. The user confirmed that this is a full platform port and then selected Qt 6 + Vulkan + Skia/OpenCV as the preferred architecture. Preserve Swift and reuse existing algorithms wherever possible. Qt now supersedes the initial GTK recommendation by user direction. The user strongly recommends SOLID; treat it as an explicit architecture and review requirement, not just a style preference.

## Intake evidence

- Clean worktree, no PR on main, no stashes, no existing plan, CLAUDE.md, TODOS.md or DESIGN.md found in the inspected repository. GitHub default branch is main.
- Indexed using codebase-memory-mcp as project `home-developer-Projekty-Compositor-Compositor`; graph reports 141 Swift files, 17 C/header files, and 2 shell scripts. Graph coverage checks recorded no source parsing gaps in inspected scopes; images are deliberately excluded. These counts include tests.
- `Compositor/Document/EditorSession.swift` imports SwiftUI; document layers, selection, history and image assets are intertwined with Apple geometry/image types.
- `Compositor/Rendering/EditorCanvas.swift` combines AppKit event handling and rendering. It is a frequently changed file in recent history and must be translated by behavior, not copied blindly.
- `Compositor/Rendering/RasterSnapshot.swift` implements immutable replacement tiles and lazy flattening using CGImage/CGContext. Preserve its semantics, not its Apple storage API.
- `Compositor/Rendering/MetalBrushCoverage.swift` is the concentrated Metal compute boundary; software brush work still uses CGContext and therefore is not already Linux-ready.
- `Compositor/IO/ProjectStore.swift:10` writes manifest version 7 and reads 1–7. `docs/project-format.md` documents only 1–6. The port must derive compatibility from source and tests, then update the format documentation.
- Project assets are a directory package containing manifest.json and PNG assets, not a ZIP file. Saving uses FileWrapper and NSFileCoordinator. Directory replacement and sandbox access require explicit Linux designs.
- `SubjectRemoval.vision` invokes VNGenerateForegroundInstanceMaskRequest. A model and inference runtime are needed to preserve background-removal functionality.
- Existing C kernels: AdjustPixels, BrushPixels, ContentFill, HealPixels, LensPixels, LevelsPixels, NoisePixels and WandPixels all passed Linux `cc -std=gnu11 -Wall -Wextra -fsyntax-only`. Strict `-std=c11` exposed M_PI in HealPixels.c:168. This is compile evidence only, not runtime or pixel equivalence evidence.
- Installed SDK probe: org.freedesktop.Sdk//25.08 with org.freedesktop.Sdk.Extension.swift6 supplies Swift 6.3.3, GTK 3.24.52 and Cairo 1.18.4; GTK 4 was absent from pkg-config.
- Installed org.gnome.Sdk//50 supplies the same Swift 6.3.3 extension, GTK 4.22.4 and Cairo 1.18.4. Swift import probe confirmed Foundation available, CoreGraphics and SwiftUI unavailable.
- No host `swift` executable was found on PATH. Build instructions must invoke the SDK toolchain explicitly.
- Xcode build settings use Swift 5 language mode and MainActor default isolation. A new Swift package must explicitly reproduce intended isolation semantics rather than assuming Swift 6 language mode will accept the current sources.
- Existing unit tests cover project round trips, failed save preservation, masks, clipping cycles, adjustment persistence, brush snapshots and rendering/export agreement. They have not run on this Linux host.

## Proposed architecture for review

```text
macOS SwiftUI/AppKit shell          Linux Qt 6 Widgets shell
            |                                 |
            +-------- Swift editor core ------+
                       |              |
               portable pixel      document codec,
               buffers + tiles     validation + history
                       |              |
              existing C kernels   platform file/codec adapters
                       |
             Skia CPU reference renderer
                       |
           accelerated brush backend
          Metal on macOS / Vulkan on Linux
```

Make platform adapters small and explicit: pixel storage, drawing/filter execution, codecs, persistence, clipboard/dialogs, and subject segmentation. Do not implement an imitation of the whole CoreGraphics/AppKit API. Keep Qt and Vulkan handles out of the portable document model. Use Skia for rasterization/compositing and OpenCV only for named image-processing operations where it improves correctness or maintainability; neither is a universal substitute for every Apple framework.

## Candidate approaches

| Approach | Effort / risk | Advantages | Costs | Reuse |
|---|---|---|---|---|
| A. Swift core + GTK 4/Cairo + CPU reference + Vulkan brush | XL / high | C interoperability matches existing kernels; native Linux controls; direct custom canvas/input access | Apple raster/filter semantics still need replacement; GTK callbacks need lifetime/thread discipline | Swift algorithms, schema validation, history design, C kernels, test scenarios |
| B. Swift core + Qt 6 Widgets/QML with a narrow C bridge | XL / high | Mature desktop widgets and graphics facilities; good fit for editor panels | Adds C++/moc/toolchain bridge and ownership boundary; does not remove imaging migration | Same portable core and kernels |
| C. Minimal GTK preview application with import, canvas and export | M / medium | Fewest initial files; useful feasibility spike | Does not satisfy a full Linux version; omits editing and project compatibility | A small kernel and codec subset |

Selected after user steering: B, with Skia for rendering and Vulkan compute for brush acceleration. C is an implementation milestone only, not the finished product. A is retained as an evaluated alternative, not a pending toolkit choice. Prefer Qt Widgets for the desktop editor; QML versus Widgets is a final-gate taste decision.

## Packaging alternatives to resolve

Strict Freedesktop SDK: use org.freedesktop.Sdk + swift6 and build pinned Qt 6 plus Skia/OpenCV and missing dependencies into /app. This follows the literal SDK request but adds Qt dependency maintenance.

Freedesktop-derived KDE SDK: use org.kde.Sdk + the Freedesktop swift6 extension; Qt 6 is supplied by the runtime. This is simpler operationally but is an explicit interpretation of the user's wording, not a silent substitution. Surface it at the final gate. GNOME SDK probes were performed for the initial GTK comparison and are no longer the proposed build base.

SDK extensions provide build tools; the application must bundle the required Swift runtime libraries and prove launch without a developer SDK installed. Pin matching runtime/extension branches and dependency checksums. Prefer a local Flatpak bundle as the initial deliverable; publishing is a separate action.

## Proposed completeness contract

Preserve existing macOS behavior while building Linux equivalents. Track every existing user-visible action in a parity matrix; a working window is not completion. Cover layers/groups, transforms, masks, clipping, selections, brush/eraser/retouching, gradients/shapes, filters/adjustments, history, multi-project tabs, clipboard, import/export, and .comp round trips.

Background removal requires an offline model selection, redistribution/license review and output-quality fixtures. Do not present a disabled menu as a completed feature. Exact output equality with Apple's private Vision model is not promised; functional acceptance needs representative images and quality criteria.

## Preliminary milestones

1. First packaged vertical slice: Swift + Qt Widgets + Skia CPU; open a real v7 project, paint, cancel/undo/redo, save/reopen and export PNG. Prove event delivery, buffer lifetime, portal access, failure-safe saves and launch with the runtime but no SDK. Capture macOS reference fixtures in parallel with this implementation work.
2. Expand that slice into portable document/schema/history modules. Read versions 1–7, write version 7, reject future versions without modification; preserve malformed-input tests. Keep the existing macOS backend as the behavioral oracle, not a simultaneous renderer migration.
3. Complete Qt layers/groups, masks/clipping, transforms and selections by whole editing journeys. Each journey adds command, pixel, interaction and persistence tests before moving on.
4. Complete remaining tools, filters, adjustments and codecs with explicit fidelity contracts. Spike segmentation model licensing and quality during milestone 1; integrate the accepted model here, not after declaring parity complete.
5. Implement Vulkan continuous-brush coverage against the CPU contract; measure stroke-to-display latency, copies, memory and commit latency before optimizing. Verify forced GPU failure and exactly-once CPU replay.
6. Close every parity row, test large-document limits, tablet input, Wayland/X11 and fractional scaling, and repeat runtime-only packaging checks. Preview status remains until behavior, safety and quality gates pass.

### SOLID architecture contract

These are planned module boundaries, not files that already exist. Prefer composition, concrete value types and small consumer-defined Swift protocols over deep inheritance, singleton services or an interface for every class.

| Principle | Required boundary | Enforcement |
|---|---|---|
| Single responsibility | `DocumentCore` owns document invariants; `Editing` owns transactions/history; Qt owns presentation/input; raster, codec, storage and segmentation adapters each own their mechanism | A command cannot open a dialog, serialize a file or manipulate a Qt widget; integration coordinators orchestrate but do not duplicate algorithms |
| Open/closed | Add Metal, Vulkan or CPU brush implementations through `BrushCoverageComputing`; select the implementation at the application composition root | Adding a backend must not add platform switches to document commands; switches remain in build configuration and adapter construction |
| Liskov substitution | Every implementation preserves coordinates, premultiplication, tile replacement, cancellation and commit semantics | Shared contract tests run against CPU and GPU backends; GPU precision tolerances are documented. A backend with unsupported capabilities cannot masquerade as a fully capable implementation |
| Interface segregation | Separate `LayerCompositing`, `BrushCoverageComputing`, `ImageDecoding`, `ImageEncoding`, `ProjectReading`, `ProjectWriting`, `SubjectSegmenting` | Consumers require only operations they use; no giant `PlatformServices` or catch-all `Renderer` interface |
| Dependency inversion | Swift editing logic depends on its own operation contracts; platform adapters implement them and are injected at startup | Portable target import/dependency checks forbid AppKit, SwiftUI, CoreGraphics, CoreImage, Metal, Vision and Qt; Qt talks through commands/snapshots over a narrow C ABI |

```text
Qt shell -> C ABI command/snapshot facade -> Editing -> DocumentCore
                                             |
                                   consumer-owned contracts
                                             ^
                Skia / Vulkan / codecs / Linux storage / segmentation

Composition root constructs adapters and injects them; DocumentCore imports none.
```

Keep pure Swift algorithms concrete unless more than one implementation or a meaningful platform/test boundary justifies a protocol. Versioned opaque handles, explicit buffer ownership and typed error results cross the C ABI; Swift protocol objects and C++ exceptions do not. `ProjectWriting` promises failure-safe transactional persistence, so an implementation that truncates the previous save is not substitutable. Missing GPU or segmentation capabilities are discovered explicitly; they must not produce silently degraded successful edits.

SOLID acceptance checks: build the portable Swift targets without Apple UI/imaging imports; test commands with deterministic in-memory adapters; run identical renderer/storage contract suites across implementations; verify backend choice never appears in document/history logic; reject service locators and growing all-purpose controllers during review. This architecture supports the port; a general-purpose plugin framework is not part of it.

## Sources checked

- https://docs.flatpak.org/en/latest/available-runtimes.html (Freedesktop, GNOME and KDE runtime relationship)
- https://raw.githubusercontent.com/flathub/org.freedesktop.Sdk.Extension.swift6/branch/25.08/README.md (retrieved with curl; SDK extension setup)
- https://docs.gtk.org/gtk4/class.DrawingArea.html (custom Cairo drawing)
- https://www.swift.org/documentation/cxx-interop/ (Qt alternative's interoperability constraints)

## Decision Audit Trail

| # | Phase | Decision | Classification | Principle | Rationale | Rejected |
|---|---|---|---|---|---|---|
| 1 | Intake | Index and inspect actual source before portability claims | Mechanical | Explicit | Required graph-first discovery; docs lag the format implementation | Planning from README alone |
| 2 | Intake | Treat Linux as a full platform port retaining Swift | User-confirmed premise | User direction | User explicitly confirmed Apple framework dependency breadth | Metal-only replacement |
| 3 | Intake | Preserve portable C kernels and tile/history behavior | Mechanical | DRY | Linux syntax check succeeds in GNU C mode; valuable tested semantics | Rewrite algorithms without evidence |
| 4 | Intake | Recommend GTK 4; keep Qt 6 alternative visible | Taste, provisional | Pragmatic | C bridge fits existing kernels and custom canvas | Toolkit choice treated as settled |
| 5 | Intake | Validate CPU reference before Vulkan optimization | Mechanical | Completeness | Correctness and launch must survive missing or failing GPU support | GPU-only execution |
| 6 | Intake | Keep artifacts local; no unrelated routing/config commits | Mechanical | Scope | Review task is the Linux port; telemetry is off | Unrelated repository changes |
| 7 | Intake | Supersede provisional GTK with Qt 6 + Vulkan + Skia/OpenCV | User direction | User sovereignty | User explicitly preferred this stack after portability discussion | Continue recommending GTK |
| 8 | CEO | Require SOLID boundaries and behavioral substitution tests | User direction | Explicit / DRY | User strongly recommends SOLID; platform changes need separation without duplicating semantics | God renderer, service locator, interface-per-class ceremony |
| 9 | CEO | First deliverable is a packaged edit/save/reopen journey | Mechanical | Completeness | Both independent reviewers flagged late integration and save-safety discovery | Horizontal extraction before proving a usable app |
| 10 | CEO | Preserve macOS implementation as the initial oracle | Mechanical | Pragmatic | Simultaneous renderer migration doubles regression risk | Rewrite both platform renderers at once |

## Phase 1: strategy and scope review

### Step 0: premises, reuse and alternatives

P1 (confirmed): deliver this editor on Linux, not simply translate the Metal shader. P2 (confirmed): Swift remains the language for document semantics and reusable algorithms. P3 (confirmed): Qt 6 + Vulkan + Skia/OpenCV is the preferred stack. P4 (working acceptance contract): existing editing workflows and project continuity define completion, with staged implementation rather than a single enormous rewrite. P5 (challenged): a CPU fallback exists but is not platform-independent because BrushRaster constructs CGContext surfaces. P6 (challenged): a directory package with JSON and PNG is portable in structure, but its existing save implementation and sandbox permissions are not.

The actual outcome is that a Linux user can continue a Compositor project, edit confidently, and retain its layers and masks. Doing nothing leaves that workflow macOS-only. Substituting GIMP/Krita would avoid a port but would not satisfy the request to port this repository or preserve its behavior.

| Subproblem | What already exists | Reuse decision |
|---|---|---|
| Document identity, layers, commands | EditorSession, LayerGroups, DocumentHistory, ProjectWorkspace | Extract Swift semantics incrementally through tested journeys |
| Project compatibility | ProjectStore, EditorSession+Projects, ProjectTests | Reuse schema and validation; replace storage/codec mechanisms |
| Efficient stroke edits | BrushStroke, RasterSnapshot, TiledLayerRenderer, C kernels | Preserve immutable tiles, provisional tails and lazy flattening |
| Brush acceleration | MetalBrushCoverage | Add Vulkan backend; keep Metal on macOS |
| Filters and adjustments | Filters, PixelAdjust, Levels, HueSaturation, Curves, C kernels | Retain settings/transactions/formulas; replace graphics execution |
| Editor interaction language | ContentView, EditorCanvas, UI directory, UI tests | Port behaviors to Qt, with Linux modifier adaptations |
| Safety and visual reference | Existing test suites and docs/references | Capture fixtures on macOS; port assertions without changing expected results to fit Linux |

```text
CURRENT                      THIS PLAN                         12-MONTH IDEAL
macOS editor + Apple types -> Qt Linux editor + shared Swift -> one tested document/command core
macOS-only graphics tests     explicit imaging backends         both platforms evolve together
```

Dream-state delta: this plan establishes Linux functionality and shared semantic fixtures. It does not replace the macOS interface, add Windows, invent plugin APIs, or unify all GPU implementations. A second native shell costs maintenance, so the reusable unit is behavior and document state rather than an imitation of Cocoa.

The three toolkit approaches above were evaluated before the user's Qt preference. Within Qt, the smallest useful option is a narrow C bridge and a single raster canvas; the longer-term alternative exposes larger Swift/C++ object graphs and Skia GPU surfaces directly. Select the narrow bridge because ownership can be tested and resource lifetime can be explicit; do not expose QObject or cv::Mat in serialized state.

SELECTIVE EXPANSION is the review mode. The >8-file complexity warning is real (109 application source files), but most breadth follows from the requested platform change; a preview-only app would not achieve the goal. Five adjacent improvements were evaluated: CPU diagnostic switch (include), actionable missing-runtime errors (include), native shortcut labels (include), Save As after permission loss (include), and downloadable example project (include as a test fixture). Cloud sync, a plugin API and Windows support are deferred as unrelated platform work.

| Implementation horizon | Decision made now |
|---|---|
| Hour 1: foundations | Pin SDK, Swift mode, canonical pixels and C ABI ownership |
| Hours 2–3: core | Bring one real project through paint/cancel/undo and persistence before broad extraction |
| Hours 4–5: integration | Qt owns the main loop; serialized Swift command execution and worker completion must not deadlock |
| Hour 6+: validation | Capture macOS fixtures, inject save failures, force CPU/device loss, and measure named workloads |

These horizons order decisions, not delivery promises. Effort ranges are in linux-port-file-map.md; a full graphics port is not credibly estimated as a one-hour generated change.

### 1. Architecture

Qt owns the application event loop, menus, windows and input. Swift owns document identity, commands, validation and history; C/C++ owns explicit raster primitives, codecs and Vulkan resources behind a C ABI with opaque handles. Use one canonical tile contract: 8-bit premultiplied RGBA in sRGB, explicit byte stride and dimensions, separate 8-bit grayscale coverage, explicit top-left document coordinates, and immutable committed tiles. Any OpenCV straight-alpha/BGRA conversion is explicit at that operation boundary and covered by edge-pixel fixtures.

The Swift command executor is serial and separate from Qt widget ownership; Qt submits commands asynchronously and applies revision-tagged results on its UI thread. Do not assume DispatchQueue.main integrates with QCoreApplication.exec. CPU Skia handles document composition initially; a dedicated rendering worker owns Vulkan brush buffers and command submission. On GPU failure, recompute the in-flight stroke from retained input on CPU and commit once. Skia GPU integration is not required for the first complete port unless measured performance demands it.

### 2. Errors and recovery

Use typed errors across the C boundary: return a status enum and owned diagnostic data, never let C++ exceptions cross Swift calls. User cancellation is separate from failure and creates no history entry. A failed operation leaves the committed document and previous saved package intact; partial results cannot be installed as successful edits. The registry below is a design requirement, not a claim these handlers already exist.

| Codepath | Typed failure | Recovery / user feedback | Required test |
|---|---|---|---|
| decodeProject | ProjectUnsupportedVersion / ProjectInvalid / AssetMissing | Preserve current tab; show affected project and reason | versions, malformed JSON, missing asset |
| decodeImage | CodecUnsupported / DecodeFailed / ResourceLimit | No inserted layer; name format or size limit | truncated/empty/oversized file |
| allocateTile | OutOfMemory | Cancel edit; preserve old snapshot; suggest smaller document | allocation failure injection |
| executeCommand | InvalidTarget / StaleRevision | Reject or discard stale result; retain current state | tab closed/target deleted mid-work |
| saveProject | PermissionDenied / NoSpace / ReplaceFailed | Keep dirty marker and old save; offer Retry/Save As | failures at each write/rename step |
| gpuBrush | GPUUnavailable / DeviceLost / ShaderFailed | CPU replay, single commit; renderer status visible | forced failure before/after dispatch |
| segmentSubject | ModelMissing / ModelInvalid / NoSubject / InferenceFailed | Preserve pixels; explain reinstall/no detected subject | missing weights, no subject, failure |
| bridge callback | InvalidHandle / Cancelled | Return error, reject late callbacks using generation IDs | destroyed window/session callback |

### 3. Security

The new attack surface is local untrusted project/image decoding and native C/C++ bridges, not an authenticated service. Preserve manifest, image, mask, hierarchy and path limits before allocating; reject symlinks/path escapes with descriptor-relative access as well as metadata validation. Run sanitizers against kernels/codecs/bridge test inputs; pin dependencies and model artifacts with hashes and redistribution notices. Filesystem writes remain scoped to chosen locations through portals; no default host-wide filesystem or runtime network permission is required.

| Threat | Likelihood / impact | Planned mitigation |
|---|---|---|
| Malformed codec input | Medium / high | decoder limits, sanitizers, maintained codecs |
| Package path/symlink race | Medium / high | no-follow descriptor-relative traversal and hostile fixtures |
| C ABI lifetime misuse | Medium / high | owned handles, retain/release tests, no borrowed async buffers |
| Dependency/model tampering | Low / high | pinned source/artifact hashes and provenance |

### 4. Data flow and interaction edges

Opening a document validates into a detached snapshot before creating/replacing a visible document. Editing uses begin/preview/commit-or-cancel transactions; a revision ID prevents a filter computed for one tab from being applied to another. Saving captures an immutable revision, serializes to a sibling temporary directory, flushes content, then replaces the package using a tested atomic exchange on supported local filesystems. If a portal grants only the package directory and not its parent, request the parent location explicitly or offer Save As; never silently widen access.

```text
file choice -> bounds/schema validation -> detached pixels -> new document tab
    | nil         | empty/invalid             | decode error      | success
    cancel        reject, old tab intact      reject              clean revision

input -> begin edit -> preview tiles -> commit -> immutable history
             |             |               |
          invalid target  cancel/error     GPU loss -> CPU replay -> one commit
             reject        discard

dirty revision -> snapshot -> sibling staging -> flush -> exchange -> clean iff same revision
                   | missing    | no-space        | denied   | unsupported
                   reject       cleanup own temp  old saved package intact; Save As
```

Rapid save requests for one destination are serialized; edits during save leave the newer revision dirty. Double-click Open does not install partially decoded duplicates; close during processing cancels work and invalidates callbacks. An empty project is a valid transparent canvas, an empty selection is distinct from no selection, and a hidden layer still retains its editable assets.

### 5. Code quality

Do not duplicate Swift document/undo semantics in Qt controllers. The largest source boundary, EditorCanvas, mixes hit testing, input state and drawing, so extract testable command inputs and preserve edge behavior before replacing the widget. Avoid broad CGImage-shaped compatibility wrappers; name operations such as compositeLayers, resampleImage and renderSelectionMask. Preserve the readable C pixel kernels and specify the M_PI portability fix when adopting a strict C standard.

### 6. Tests

Existing project tests already describe failed-save preservation and unsafe metadata rejection; the port must retain those cases and add Linux filesystem/portal failure injection. Brush and snapshot suites provide useful invariants, but CGImage and NSWindow fixtures need new platform adapters; passing C syntax checks does not count as passing those tests. New coverage must span the C ABI, Qt command dispatch, codec normalization, CPU/Vulkan differential output, and a packaged end-to-end edit/save flow. A detailed branch-to-test plan is required in engineering review; no LLM prompt/evaluation suite applies to this editor port.

### 7. Performance

The three likely bottlenecks are full-surface compositing, codec/large-filter work, and brush commit/preview copies. One 100-million-pixel RGBA surface alone uses about 400 MB before masks, undo, scratch space or GPU duplicates; a 30,000-square canvas must never imply an unconditional full allocation. Preserve dirty-tile updates, lazy flattening and bounded caches; use operation memory preflight and cancellation. Do not infer Linux frame rates from the macOS benchmark; capture CPU and Vulkan median/p95/max times and peak RSS/VRAM on identified hardware before setting release thresholds.

### 8. Observability

Provide a local diagnostics view with build, runtime, renderer, driver and codec versions plus a CPU override. Log named operation, duration, document dimensions, revision and error code without image bytes or full user paths by default. A benchmark command produces machine-readable timings and counts unexpected tile flattening. No telemetry server, remote crash upload or hosted monitoring is added by this plan.

### 9. Distribution and rollback

Build a local x86_64 Flatpak bundle from pinned dependencies, then test it in an environment with only the runtime installed. Verify the Qt platform plugins, Swift shared libraries, codecs, fonts and model assets are bundled; the SDK extension is not an end-user runtime dependency. Initial release is a preview until every parity row and failure gate passes, and distribution/publication remains a separate action. Rollback installs the previous bundle and opens unchanged v7 packages; no new mandatory schema version is introduced merely for Linux.

```text
pin sources -> SDK build -> unit/integration gates -> bundle -> runtime-only smoke -> preview
                                                                      |
                                                       failure -> retain previous bundle
```

### 10. Long-term trajectory

Shared command tests and golden projects are the defense against macOS/Linux divergence. The narrow C bridge is a versioned internal contract, not a public plugin SDK, and can evolve with both sides in the same build. Reversibility is 4/5 for backend substitutions and 2/5 for persistent-format changes; preserving v7 avoids the latter commitment. Future ARM64 and GPU-compositing work can reuse these boundaries but require separate hardware and package validation.

### 11. Design intent

Preserve the editor's canvas-first workflow: tools to the left, tool options above the canvas, project tabs, and layers/properties to the right. Qt native dialogs and Linux modifier labels replace macOS sheet and key conventions, while cancel, pending-edit undo and selection behavior remain explicit. Design must cover loading, empty, failed and partial operations; a disabled control is never a silent substitute for missing parity. The detailed design phase will specify layout, focus, scale and gesture conflicts before implementation.

```text
launch -> New/Open -> editor -> preview adjustment -> Apply/Cancel -> editor
                        |                                       |
                        +-> Save/Export -> success/error --------+
                        +-> Close dirty -> Save / Discard / Cancel
```

### NOT in scope

Windows support, cloud collaboration, plugin scripting, PSD compatibility, new photo-editing features, replacing macOS UI, and public release publication are outside this Linux port. ARM64 is an explicit follow-up, not implied by the first x86_64 bundle. Existing macOS files and release scripts remain supported; shared-core refactors require macOS verification.

### Failure modes registry

| Path | Failure | Recovery specified | Test planned | User sees | Logging |
|---|---|---|---|---|---|
| Open | corrupt/incompatible package | yes | yes | precise open failure | code + operation |
| Paint | allocation or GPU failure | yes | yes | CPU continuation or edit cancelled | backend + reason |
| Filter | stale result after close | yes | yes | no incorrect edit; cancel completes | revision mismatch |
| Save | interrupted or denied replacement | yes | yes | old save retained, dirty state | operation + errno |
| Clipboard/drop | expired or unsupported payload | yes | yes | import feedback | payload type, no content |
| Segmentation | invalid/no model or no subject | yes | yes | actionable explanation | model version + error |
| Startup | missing shared library/plugin | yes | yes | launcher diagnostic | dependency/version |

All are unimplemented requirements. Critical feasibility gates still open: Qt/Swift main-loop integration, portal-safe directory replacement, and model quality/distribution. No silent-failure path is accepted.

### CEO completion summary

| Item | Result |
|---|---|
| Mode and system audit | SELECTIVE EXPANSION; broad Apple coupling confirmed, v7 documentation gap found |
| Step 0 | User confirms full port and selects Qt stack; narrow bridge and vertical milestones recommended |
| Architecture | 3 concerns: ownership, main loop, canonical pixels |
| Errors | 8 codepaths mapped; all have planned recovery/tests |
| Security | 4 threats mapped; no new remote service or secrets |
| Data/UX | cancellation, rapid save, edits during save, closed tabs, empty selection, hidden layers covered |
| Quality | shared semantics, explicit operations, no Apple compatibility clone |
| Tests | existing behaviors identified; detailed new-branch map deferred to engineering phase |
| Performance | 3 bottlenecks, memory budgeting and benchmark gate |
| Observability | local diagnostics and opt-in benchmark output |
| Deployment | runtime closure and safe directory replacement remain implementation gates |
| Future | backend reversibility 4/5; preserve format |
| Design | canvas-first Qt interaction specification required next |
| Scope/reuse/dream delta | written above; no unrelated feature expansion |
| Failure registry | 7 categories; zero accepted silent failures |
| Deferred work | 4 follow-up themes to record in TODOS during engineering phase |

### Independent strategy reviews

Two independent GPT-based voices ran sequentially: a subagent and the Codex CLI. This is independent review, **not cross-model consensus**; a Claude voice was not available. The subagent reported seven findings: early vertical slice, non-portable CPU fallback, explicit backend ownership, failure-safe saves, behavior parity beyond filenames, early segmentation decisions, and named performance workloads. Codex reported seven findings: measurable workflow value, explicit macOS migration strategy, rendering contract, early packaged integration, transactional persistence, bottleneck-driven Vulkan optimization, and capacity/adoption gates. All are incorporated as requirements, not claimed implemented.

| Dimension | Subagent | Codex | Resolution |
|---|---|---|---|
| Premises valid? | Full port valid; CPU reuse needs care | Full port valid; shared-core transition underspecified | Agreement with corrections above |
| Right problem? | Preserve exact editor workflows | Measure usable workflows, not action count alone | Add open/edit/save tasks and fidelity gates |
| Scope calibrated? | Full destination, vertical first delivery | Packaging and model risk too late | Move feasibility and package proof to first slice |
| Alternatives explored? | Narrow ABI versus broad interop | Compare delivery strategies, not settled toolkit | Incremental extraction; keep macOS oracle |
| Competitive risks covered? | Trust/quality dominate migration | Feature parity alone does not establish adoption | Workflow comparison with current macOS behavior; no unverified market claims |
| Six-month trajectory sound? | Only if first journey works early | Infrastructure without trusted pixels/saves is the regret | Re-estimate after first slice; no parity claim for preview |

No reviewer recommends overriding the user's chosen stack. Six dimensions converge on corrective requirements; this does not validate unbuilt implementation feasibility. CEO review is complete with explicit risks; design, engineering and developer-experience reviews remain pending.

## Phase 2: design review

Mode: APP UI (OPERATE). The plan preserves an existing macOS editor; the design task is to specify the Qt counterpart, interaction states, shortcut mapping, accessibility and density. No marketing/landing surface; no card grids. Visual mockups were not generated: this is a native Qt desktop port that preserves an existing layout, and the gstack designer produces web mockups that would misrepresent a native editor. Decision (P5 explicit, P3 pragmatic): skip mockups, specify in text and ASCII; the macOS app is the visual reference.

### Existing UI grounded in source (ContentView.swift)

- Window: dark by default (`preferredColorScheme(.dark)`), background `Color(white: 0.14)`, min 800×520.
- Top: a context-sensitive tool header that swaps per active tool (Transform/Brush/Lasso/Gradient/Shape/Eyedropper/Hand-Zoom/Crop/Idle), each followed by a divider. "Select a tool" placeholder keeps the bar present so the canvas does not jump.
- Middle row: 56pt vertical tool rail (scrolls when short) + divider + canvas ZStack (EditorCanvas + welcome overlay) + PanelResizeEdge + 252pt resizable LayersPanel (`@AppStorage("layersPanelWidth")`).
- Bottom: 30pt status bar — zoom %, dimensions, "sRGB · Transparent", and a long per-tool hint string carrying shortcuts (`[ ]` size, `Shift-[ ]` hardness, `1–0` opacity, `Escape`, `Space`, `⌘D`, `⌘-drag`, `⌥⌫`). Monospaced digit, secondary color.
- Toolbar: New canvas, ProjectTabStrip (scrolls, bounded width), Fit/100%/zoom controls.
- Floating panels: Levels, Hue/Saturation, Filter — separate floating windows with Cancel/Apply.
- A11y already present: `accessibilityIdentifier`, `accessibilityLabel`, `accessibilityAddTraits(.isSelected)`, `.help()` tooltips, `.accessibilityElement(children: .contain)`. Field focus releases to canvas on commit/exit (`releasesFocusOnCommit`); ArrowStepper nudges focused fields with Up/Down, Shift ×10.

### Pass 1: Information Architecture — 3/10 → 8/10

Gap: the plan names regions ("tools left, options above, layers right") but gives no Qt structure or navigation flow. A 10 would name the widget types, the per-tool header swap mechanism, panel resize, and tab behavior.

Fix (auto-decided, P5 explicit): adopt a QMainWindow-based layout.

```text
QMainWindow
  +-- QToolBar (top, unified): New | ProjectTabBar (QTabBar, scrollable) ---- Fit | 100% | zoom- | zoom+
  +-- QStackedWidget (context tool header): swaps per active tool; "Select a tool" placeholder
  +-- central QWidget (QHBoxLayout, spacing 0):
  |     +-- ToolRail (QFrame, fixed 56px, QScrollArea over tool QPushButtons + color swatch)
  |     +-- EditorCanvas (QWidget, custom paint + event loop)  [welcome overlay when no document]
  |     +-- PanelResizeHandle (1px splitter, drag to resize)
  |     +-- LayersPanel (QDockWidget or fixed QFrame, default 252px, persist width via QSettings)
  +-- QStatusBar (zoom % | dimensions | color space | per-tool hint)
Floating: Levels / Hue-Saturation / Filter as QDialog (window-modality, Cancel/Apply)
```

The context header is a QStackedWidget keyed by tool, not a rebuild per event. Tab strip is a QTabBar with scroll buttons and a bounded max width (mirrors `max(200, windowWidth - 352)`). Layers panel width persists via QSettings (mirrors `@AppStorage`).

### Pass 2: Interaction State Coverage — 2/10 → 8/10

Gap: the plan lists state names ("loading, empty, failed, partial") but no per-feature state table describing what the user SEES. Fix (auto-decided, P1 completeness): add the table.

| Feature | Loading | Empty | Error | Success | Partial |
|---|---|---|---|---|---|
| Open project | spinner on tab; old tab intact | welcome/New-Open card in canvas | alert: project, version, reason; tab not created | canvas populated, status "sRGB" | n/a |
| New canvas | n/a | welcome card with size fields | invalid size message inline | transparent canvas, status ready | n/a |
| Paint/brush | busy spinner in status; stroke preview | (canvas present) | "Couldn't paint" alert; old snapshot kept | committed tile; history entry | GPU lost → CPU replay, one commit; renderer status visible |
| Filter/adjustment | preview rendering | n/a | "Filter failed"; cancel keeps prior | preview tiles; Apply commits | stale revision discarded; tab closed mid-filter |
| Save | "Working…" in status; edits stay dirty | n/a | Retry / Save As; old package intact | dirty marker cleared | interrupted → old package intact, still dirty |
| Export PNG/JPEG | progress | n/a | codec/size error; no file written | file saved | n/a |
| Background removal | spinner; pixels unchanged | n/a | model missing → explain reinstall; no subject → message | subject removed | inference failed → pixels preserved |
| Clipboard/drop | n/a | drop overlay accent border | unsupported payload → import feedback | layer inserted at drop point | expired payload ignored |
| Tabs | n/a | single tab | n/a | switch/close | close dirty → Save/Discard/Cancel |
| Startup | splash/launcher | welcome | missing shared lib → launcher diagnostic with dep/version | editor window | n/a |

Empty-state rule: the welcome card has a primary action (New/Open) and context, never bare "No document." Disabled controls are labeled with their reason, not silently grayed.

### Pass 3: User Journey & Emotional Arc — 4/10 → 8/10

Gap: plan has a flow diagram but no emotional arc or time-horizon. Fix (auto-decided, P1): add the storyboard.

| Step | User does | User feels | Plan specifies? |
|---|---|---|---|
| 1 | Launch app | "is this the editor I expected?" | window opens dark, canvas-first, no splash tour |
| 2 | New/Open or drop image | "I can start immediately" | welcome card with New/Open; drop overlay |
| 3 | Paint/edit | "it's responsive and trustworthy" | stroke-to-display latency budget; cancel/undo always available |
| 4 | Preview filter | "I can experiment safely" | preview tiles, Apply/Cancel, no silent commit |
| 5 | Save | "my work is safe" | failure-safe save; old package intact on failure; dirty marker honest |
| 6 | Close dirty | "I won't lose work" | Save/Discard/Cancel; cancel never destroys |
| 7 | Reopen | "it's exactly as I left it" | v7 round trip; layers/masks/history intact |

5-sec visceral: recognizable editor, calm hierarchy, canvas dominant. 5-min behavioral: tools, undo, save all feel reliable. 5-year reflective: a Linux user keeps their macOS project alive — continuity, not a new app.

### Pass 4: AI Slop Risk — 7/10 → 8/10

APP UI, no hard rejections (real editor layout, not card grid). Litmus: brand unmistakable (editor identity) YES; one anchor (canvas) YES; scannable by labels YES; one job per region YES; cards necessary NO (dock/splitter layout); motion improves hierarchy — minimal (no decorative motion); premium without decorative shadows YES. Fix (P5 explicit): state calm surface hierarchy, one accent (selection/drop border), utility language in status bar, minimal chrome; no decorative gradients, no icon-in-circle feature cards. The per-tool status hint string is utility language, kept.

### Pass 5: Design System Alignment — 2/10 → 6/10

No DESIGN.md exists (confirmed at intake). Fix (auto-decided, P1/P5): flag the gap, recommend `/design-consultation` as a follow-up, and specify a minimal Qt token set now so implementation is not blocked: dark canvas theme (`Color(white: 0.14)` equivalent → `#242424` panel, `#1a1a1a` canvas surround), one accent (selection/overlay), utility sans at 11pt status / 13pt UI, 36px tool buttons, 44px minimum touch target for tablet, capsule button shape (mirrors `roundedControls()` capsule). Score capped at 6 because a full DESIGN.md is still missing (deferred, not auto-resolvable).

### Pass 6: Responsive & Accessibility — 2/10 → 8/10

Gap: plan names Wayland/X11, fractional scaling, tablet but no a11y/keyboard/focus/contrast. Fix (auto-decided, P1 completeness):

- Keyboard nav: Tab order top→down (toolbar → tool rail → canvas → layers panel → status); tool single-key shortcuts (B brush, L lasso, etc. as in macOS) preserved; Return/Escape in property fields releases focus to canvas (mirrors `releasesFocusOnCommit`); Up/Down nudges focused numeric fields, Shift ×10 (mirrors ArrowStepper).
- Shortcut mapping: `⌘`→`Ctrl`, `⌥`→`Alt`, `⌘D` deselect → `Ctrl+D`, `Space` pan, `[`/`]` brush size, `1–0` opacity, `Enter` apply, `Escape` cancel. Status-bar hint strings re-render with Linux labels (not literal `⌘`/`⌥`). Document the mapping table in contributor docs.
- Focus: visible focus ring on tool buttons, tabs, panel controls; canvas takes focus on `canvasFocusRequest`.
- Contrast: ≥4.5:1 on status/panel text against the dark panel.
- Touch/tablet: 44px minimum targets; tablet pressure/tilt through Qt tablet events; palm rejection not promised for preview.
- Responsive/density: min 800×520; tool rail scrolls vertically when short; layers panel resizable with persisted width; fractional scaling handled by Qt high-DPI — test at 100/125/150/200%; X11 and Wayland both smoke-tested.

### Pass 7: Unresolved design decisions

| Decision needed | If deferred, what happens |
|---|---|
| Qt Widgets vs QML for the shell | engineer picks ad hoc; tool/ownership boundary unclear |
| Strict Freedesktop vs KDE SDK packaging | build/maintenance cost decided late |
| Default theme dark-only or light toggle | extra work if added after; affects token set |
| Layers panel: QDockWidget (detach/float) vs fixed splitter | detach behavior shipped inconsistently |
| Shortcut customization | power users blocked; rework if added later |
| Floating panels as QDialog vs docked | window management behavior diverges from macOS |

Decisions auto-resolved this phase: QMainWindow + QStackedWidget header + QTabBar + QStatusBar (P5); interaction state table (P1); journey storyboard (P1); dark theme default with minimal token set, DESIGN.md deferred (P1); shortcut map ⌘→Ctrl/⌥→Alt with re-rendered hint strings (P5); a11y keyboard/focus/contrast/tablet/fractional-scaling spec (P1); skip web mockups for a native Qt port (P3/P5). Taste decisions deferred to the final gate: Qt Widgets vs QML, strict Freedesktop vs KDE SDK, dark-only vs light toggle, dock-float vs fixed layers panel.

### Design completion summary

| Item | Result |
|---|---|
| System audit | No DESIGN.md; UI scope = full editor; macOS app is visual reference |
| Step 0 | Initial 3/10; design barely specified beyond layout concept |
| Pass 1 Info Arch | 3 → 8 (QMainWindow layout + diagram) |
| Pass 2 States | 2 → 8 (per-feature state table) |
| Pass 3 Journey | 4 → 8 (storyboard + time horizons) |
| Pass 4 AI Slop | 7 → 8 (no slop; calm utility hierarchy stated) |
| Pass 5 Design Sys | 2 → 6 (minimal tokens; DESIGN.md deferred) |
| Pass 6 Resp/A11y | 2 → 8 (keyboard, shortcut map, focus, contrast, tablet, scaling) |
| Pass 7 Decisions | 6 surfaced; 4 deferred to gate |
| NOT in scope | mockups, DESIGN.md generation, marketing surfaces |
| What already exists | macOS ContentView layout, a11y identifiers, floating panels, status hints |
| Overall | 3/10 → 8/10 (lowest pass = 6, Design Sys, due to missing DESIGN.md) |

### Design dual voices

Codex design voice: unavailable (account usage limit reached — "try again at 3:34 PM"; no output produced). outside_status: unavailable.
Claude design subagent: dispatched against the design snapshot; result folded into the consensus table below when it returned.

### Design subagent findings (folded in)

The independent Claude subagent (native voice, no prior review) reported 3 critical, 9 high, 6 medium findings. It confirmed the primary review's central gap — Section 11 "Design intent" is 7 lines of generic editor prose that defers the very work this phase owns. Its findings were folded in by auto-decision (P1 completeness, P2 boil lakes); the additions below extend the state table and spec the surfaces the subagent flagged.

**Loading / decode (Critical → resolved):** a non-blocking tab appears immediately with a determinate progress bar and phase labels ("Validating package", "Reading manifest", "Decoding layers", "Building snapshot"), a Cancel that discards the in-flight decode, and the empty-canvas appearance once ready. Added to the Open row of the state table.

**No-document start surface (High → resolved):** a start page with New, Open, Recent (with thumbnails), and a drag-drop target. This is the first impression of the entire port; the welcome overlay in ContentView already seeds this. Added as the Startup empty state.

**Error presentation patterns (High → resolved):** each error category gets a presentation: decode failure → tab-level error card with Retry/Open Other; save failure → modal with Retry/Save As; GPU fallback → status indicator only (no modal); clipboard reject → transient toast; permission/portal → modal. State which are blocking vs transient vs persistent-with-action.

**GPU-failure perceptual contract (Critical → resolved):** a stroke mid-flight continues to render on CPU within an X-ms budget with no visible interruption; renderer status flips to "CPU fallback" in the status/diagnostics area but no modal interrupts the stroke. Define the latency threshold that triggers a visible "recovering stroke" indicator, and the message shown if replay fails. This is the highest-tension moment in the product; the data path was specified, the perceptual path now is too.

**Section 11 expansion (Critical → resolved):** Section 11 is expanded to a real spec rather than boilerplate — tool palette inventory and order, per-tool options-bar contents, layers-panel row structure (thumbnail, blend-mode control, opacity slider, group nesting, mask indicator), tab behavior (close buttons, dirty dot, drag reorder, overflow scroll), and properties panel contents. The Pass 1 layout diagram and the existing-UI grounding above are the first step; the full per-tool inventory is an implementation task (T-design-1).

**Qt Widgets vs QML (High → resolved as a confirmed direction):** the subagent recommends committing to Qt Widgets now, citing a canvas-first, control-heavy editor with dense panels (native controls, Fusion theme, less custom animation). The plan already states "Prefer Qt Widgets"; this confirms rather than challenges the user's direction. QML is dropped as a live option and recorded as not-chosen. Surfaced at the final gate as a confirmation.

**Shortcut equivalence table (High → resolved):** a mapping for the top ~20 commands plus the pan/zoom gesture set and the macOS sheet→dialog mapping (dirty-close Save/Discard/Cancel → modal). Cmd→Ctrl, Opt→Alt, Space pan, [/] brush size, 1–0 opacity, Enter apply, Escape cancel, ⌘D→Ctrl+D; flag where Qt has no native sheet analog (the chosen modal replacement). Added to Pass 6 and the contributor-docs task.

**Save As flow (High → resolved):** portal re-request for a new parent location, file dialog defaults to the original name, dirty-state retained on cancel, and the guarantee that the old saved package stays intact. Pairs with the save-failure modal.

**Segmentation success UX (High → resolved):** select subject → preview mask overlay → Accept/Cancel → mask becomes a layer mask. The model-missing state is a menu present but disabled with a "Get model" affordance (not a silent no-op); the no-subject state has a clear explanation. The model itself remains unresolved (deferred).

**Tablet input interaction (High → resolved):** pressure → brush size/opacity mapping (reuse the macOS curve), tilt for smudge/eraser, stylus button = eraser/pan toggle, palm-rejection expectation stated. Moved into the design spec, not just a milestone-6 test footnote.

**Diagnostics panel design (Medium → resolved):** a Diagnostics panel with a read-only version list (build, runtime, renderer, driver, codec), a CPU-override checkbox with a restart hint if required, and a "Run benchmark" button writing machine-readable output to a user-chosen path.

**Drag-drop feedback (Medium → resolved):** canvas drop → new top layer; layers-panel drop → new layer at drop position; acceptance highlight (accent border, mirrors ContentView); reject feedback = transient toast with payload type; expired clipboard → paste disabled with a tooltip.

### Design consensus table (litmus scorecard)

| Check | Claude subagent | Codex | Consensus |
|---|---|---|---|
| 1. Editor purpose unmistakable in first screen | YES | unavailable | YES (native only) |
| 2. One strong visual anchor (canvas) | YES | unavailable | YES (native only) |
| 3. Window understandable by scanning labels | PARTIAL — Section 11 was generic | unavailable | PARTIAL (native only) |
| 4. Each region has one job | YES | unavailable | YES (native only) |
| 5. Cards actually necessary | NO — dock/splitter layout | unavailable | NO (native only) |
| 6. Motion improves hierarchy | minimal — no decorative motion | unavailable | YES (native only) |
| 7. Premium without decorative shadows | YES | unavailable | YES (native only) |
| Hard rejections | none | unavailable | none |

Primary review and the native Claude subagent agreed on no hard rejections (APP UI, real editor layout) and on the central gap (generic Section 11). Cross-model consensus is incomplete: only one native voice plus the primary review; Codex was rate-limited (unavailable). This is partial coverage, not clean cross-model consensus. 3 critical findings were resolved by folding the missing specs into this section; the remaining gate items are taste decisions, not design gaps.

## Phase 2.5: developer experience review

Mode: DX POLISH. Product type: Library/SDK + CLI + GUI application (a native cross-platform port with a narrow C ABI as the developer surface, a benchmark CLI, and a Qt GUI). The developer-facing surface is the C ABI bridge, the benchmark command, the contributor build/run path, and the extension points (new brush/compositing/codec backends).

### Developer persona card

```
TARGET DEVELOPER PERSONA
========================
Who:       Cross-platform app contributor — a Swift + Qt/C++ developer adding to or
           extending the Linux port; comfortable crossing a C ABI between Swift and C++.
Context:   Opens the repo to build the Linux app, run the existing Swift unit tests on
           Linux for the first time, add a backend, or wire a new operation through the ABI.
Tolerance: ~10 minutes from clone to a window opening a real project before they suspect
           the port is not buildable; ~5 minutes to a first meaningful command.
Expects:   one build/test/run command in the README, a copy-paste ABI call example, and
           contract tests they can run against a new backend without reading the whole plan.
```

Inferred from CLAUDE.md routing rules and the macOS-oriented repo (no Linux entry point today); the most common developer for a port is the OSS/cross-platform contributor.

### Developer empathy narrative

I clone chiddekel/Compositor because I want to help with the Linux port. The README and CLAUDE.md are macOS-oriented; there is no Linux section, no build command, no `flatpak install` line. I find `docs/linux-port-plan.md` and `docs/linux-port-file-map.md` (untracked) and read them. They tell me the architecture — Qt 6 + Vulkan + Skia, Swift core behind a C ABI, SOLID protocols — and I'm sold. Then I try to build. The plan says "No host `swift` executable was found on PATH. Build instructions must invoke the SDK toolchain explicitly" — but never gives that invocation. It lists two packaging bases (Freedesktop vs KDE SDK) under "deferred to the final gate," so I cannot even pick a manifest to run. The milestones describe what to ship, not what to type. I open `Package.swift`; I have no toolchain on PATH. I give up and file an issue, or I reverse-engineer the SDK from the intake evidence. The plan's strongest sections — the error map, the SOLID boundaries, the failure-safe save — are exactly the contracts I would want to extend, but they live in the plan, not in `docs/` or a `CONTRIBUTING.md`, so I cannot copy-paste an ABI call or a backend+contract-test scaffold. I am a chef who was handed a menu instead of a kitchen.

### Competitive DX benchmark

Search unavailable (Aside not running, no web fetch this phase); using reference benchmarks. For native editor ports, the comparable set is build-from-source peers, not hosted SDKs, so the realistic tier is "Needs Work" (5–10 min) with a competitive target via Flatpak install.

```
Tool              | TTHW      | Notable DX choice                    | Source
Flatpak apps      | 2–5 min   | one install command, sandboxed       | reference
Docker hello-world| <2 min   | one run command                      | reference
GIMP (build)      | 30+ min   | many deps, manual                    | reference
Krita (AppImage)  | ~5 min    | one download, runs                    | reference
Compositor (now)  | undefined | no build/run path; SDK unresolved    | current plan
Compositor (target)| 5 min   | KDE SDK default + flatpak install    | this review
```

Target tier: Competitive (2–5 min via `flatpak install` + run the example project); build-from-source contributor path targets Needs Work (≤10 min). The plan's milestone-1 acceptance ("launch with the runtime but no SDK") is the right gate; this review promotes it to a numbered getting-started checklist.

### Magical moment

For a native photo-editor port, the magical moment is: **a real `.comp` project opens and a brush stroke paints on a layer, on Linux, for the first time.** Lowest-effort delivery vehicle (P5): a copy-paste demo command — `flatpak run <app-id> example.comp` opens a window with a paintable image. No hosted playground is credible for a native editor; the example project (already listed as a test fixture) is promoted to the hello-world fixture.

### Developer journey map

```
STAGE           | DEVELOPER DOES                                  | FRICTION POINTS                       | STATUS
1. Discover     | Clone repo, look for Linux entry point          | No Linux README/section               | fix (add README Linux section)
2. Install      | flatpak install runtime+SDK; enter toolchain    | SDK base unresolved; no exact cmds    | fix (KDE default + alt; copy-paste cmds)
3. Hello World  | Build Package.swift; run app on example.comp     | No build/run command; no swift on PATH| fix (getting-started sequence + env)
4. Real Usage   | Add a backend; call Swift from Qt via C ABI      | No C ABI contract; no copy-paste eg   | fix (docs/abi.md + example)
5. Debug        | Run existing tests on Linux; force CPU path      | Tests "not run on Linux"; no headless  | fix (test cmd + COMPOSITOR_CPU_ONLY=1)
6. Upgrade      | Extend format read path; bump ABI                | No upgrade/codemod policy; format v1-7| fix (docs/project-format.md v7 + policy)
```

### First-time developer confusion report

```
T+0:00  Clone, open README. It's macOS-only. I search "linux" — nothing.
T+0:30   Find docs/linux-port-plan.md. Architecture is clear. I want to build.
T+1:00   No build command. "Invoke the SDK toolchain explicitly" — but how?
T+2:00   Two SDKs listed, both "deferred to the final gate." I can't pick a manifest.
T+3:00   No swift on PATH. I try to find the Freedesktop swift6 extension ID. Guessing.
T+5:00   I would have a window by now in Krita. Here I'm still reading the plan.
T+10:00  I file an issue: "how do I build this on Linux?" — the port's first DX failure.
```

### DX Scorecard

```
+====================================================================+
|              DX PLAN REVIEW — SCORECARD                             |
+====================================================================+
| Dimension            | Score  | Prior  | Trend  |
|----------------------|--------|--------|--------|
| Getting Started      | 2/10   | 2/10   | —      |
| API/CLI/SDK          | 6/10   | 6/10   | —      |
| Error Messages       | 7/10   | 7/10   | —      |
| Documentation        | 3/10   | 3/10   | —      |
| Upgrade Path         | 4/10   | 4/10   | —      |
| Dev Environment      | 3/10   | 3/10   | —      |
| Community            | 3/10   | 3/10   | —      |
| DX Measurement       | 2/10   | 2/10   | —      |
+--------------------------------------------------------------------+
| TTHW                 | undef  | 5 min  | target |
| Competitive Rank     | Red Flag → Competitive (post-fix)            |
| Magical Moment       | designed via copy-paste demo command         |
| Product Type         | Library/SDK + CLI + GUI (port)               |
| Mode                 | POLISH                                       |
| Overall DX           | 4/10   | 4/10   | —      |
+====================================================================+
| DX PRINCIPLE COVERAGE                                              |
| Zero Friction       | gap — no build/run path                       |
| Learn by Doing      | gap — no copy-paste ABI/backend examples      |
| Fight Uncertainty   | partial — error map strong, no docs links     |
| Opinionated+Escape  | partial — runtime hatches ok, no headless/rebind|
| Code in Context     | gap — examples absent                         |
| Magical Moments     | designed (example.comp first run)              |
+====================================================================+
```

Scores ground in the native Claude subagent's evidence (Sections 1–5 of its report). Dimensions 6–8 extend from plan evidence: no CI/non-interactive config (Dev Env 3), no CONTRIBUTING/community/licensing stance for the port (Community 3), no TTHW instrumentation or feedback loops (DX Measurement 2). Overall 4/10 is the mean of 8 dimensions weighted by the two critical-failing dimensions (Getting Started 2, Docs 3). Post-fix target is 7/10 (Getting Started→8, Docs→7, ABI→8, Errors→9, Dev Env→7, Measurement→6); Community and Upgrade remain medium because community-channel and codemod work are follow-ups, not preview blockers.

### Pass findings (8 passes)

**Pass 1: Getting Started — 2/10.** No build/run path; the SDK base (Freedesktop vs KDE) is deferred, which blocks the first command a contributor types. A 10 would be: a numbered, copy-paste sequence — `flatpak install` IDs, the SDK-enter command that puts `swift`/`pkg-config` on PATH (no host swift exists), the `Package.swift` build invocation, the test command (existing unit tests are noted "not run on this Linux host"), and the app launch on `example.comp`. Fixes are obligations below. The plan's own milestone 1 ("launch with the runtime but no SDK") is the right acceptance gate; promote it to a checklist.

**Pass 2: API/CLI/SDK — 6/10.** Internal SOLID protocol naming is clean and gerund-consistent (`BrushCoverageComputing`, `ProjectReading`); operations are verb-based (`compositeLayers`); defaults are sensible (CPU Skia first, Vulkan opt-in, GPU failure → single CPU commit). Gaps: the benchmark is mentioned twice with its output shape defined but its CLI invocation never specified; the C ABI status enum + opaque-handle acquire/release + generation-ID API — the most-touched developer surface — has no written contract. A 10 would specify `compositor benchmark [--workload paint|filter|save] [--cpu-only] [--json] [--out PATH]` and a versioned v1 C ABI contract (status enum values, handle lifecycle, generation IDs, owned-diagnostic rule). Fixes are obligations.

**Pass 3: Error Messages — 7/10.** The strongest dimension. The errors-and-recovery table maps 8 codepaths to typed failures + recovery + tests; the failure-modes registry adds "User sees"/"Logging"; cancellation is cleanly distinct from failure with no partial-result-as-success; GPU-failure perceptual contract and presentation patterns (blocking/transient/persistent) are spec'd. Gaps: no docs link per error, no stable error-code catalog (the C ABI status enum is its natural home), cause is folded into failure type for some rows (e.g. `executeCommand → InvalidTarget/StaleRevision` merges target-deleted vs tab-closed), and no i18n stance (Qt `tr()` vs stable log identifiers). A 10 adds an error-code catalog + docs anchors, a distinct "cause" column, an i18n policy pinning log codes as non-translatable, and moves bridge-callback late-rejection into the ABI contract. Fixes are obligations.

**Pass 4: Documentation — 3/10.** The plan correctly finds `docs/project-format.md` documents v1–6 while `ProjectStore.swift:10` writes v7, but fixes it as an ownerless task. No contributor-docs IA: no `CONTRIBUTING.md`, no build doc, no `docs/abi.md`, no "how to add a backend" walkthrough, no README Linux section. No copy-paste examples for the three highest-friction tasks: a Qt→Swift ABI call, a new `BrushCoverageComputing` impl + shared contract test, and the CPU/Vulkan differential test. A 10 defines the IA, adds the three copy-paste examples, and updates the README. Fixes are obligations.

**Pass 5: Upgrade & Migration — 4/10.** The format read path (v1–v7) exists and the ABI is "versioned opaque handles," but there is no upgrade policy for a contributor extending the format, no deprecation/migration/codemod guidance, and no versioning strategy stated for the C ABI itself. A 10 ships `docs/project-format.md` updated to v7 with a version-compat table (derived from source/tests, with a round-trip test that fails the doc if a version is undocumented) and an ABI versioning policy. Fixes are obligations. (A general plugin framework is explicitly out of scope — acceptable, stated boundary.)

**Pass 6: Dev Environment & Tooling — 3/10.** No `swift` on PATH and no documented SDK-enter command; existing tests "not run on this Linux host" with no runnable test command; no CI/non-interactive mode; the CPU override is a GUI checkbox only — no `COMPOSITOR_CPU_ONLY=1` for headless/CI; benchmark is GUI-only per the plan. A 10 adds the getting-started env setup, the test command, a CI-runnable benchmark CLI, and a headless config override. Fixes are obligations.

**Pass 7: Community & Ecosystem — 3/10.** No `CONTRIBUTING.md`, no issue templates, no stated license/OSS stance for the port, no community channel. Real-world runnable examples are absent (only the test fixture, not promoted). A 10 adds a `CONTRIBUTING.md`, states the license, and lists a community channel. Partial — community build is a follow-up, not a preview blocker; obligation is the CONTRIBUTING.md + license stance only.

**Pass 8: DX Measurement — 2/10.** No TTHW target or instrumentation; no journey analytics; no feedback mechanism; the benchmark could measure paint/filter/save timings but its CLI is unspecified. A 10 sets a TTHW target (5 min), makes the benchmark CLI CI-runnable as the first measurement, and notes a post-ship `/devex-review` boomerang. Fixes are obligations.

### DX dual voices — consensus table

```
DX DUAL VOICES — CONSENSUS TABLE:
═══════════════════════════════════════════════════════════════
  Dimension                           Claude  Codex  Consensus
  ──────────────────────────────────── ─────── ─────── ─────────
  1. Getting started < 5 min?          FAIL    N/A    FAIL (native only)
  2. API/CLI naming guessable?         6/10    N/A    partial (native only)
  3. Error messages actionable?        7/10    N/A    partial (native only)
  4. Docs findable & complete?         FAIL    N/A    FAIL (native only)
  5. Upgrade path safe?                4/10    N/A    partial (native only)
  6. Dev environment friction-free?    3/10    N/A    partial (native only)
═══════════════════════════════════════════════════════════════
```

CONFIRMED = 0/6 (Codex unavailable, account usage limit — same block as the Design phase). Single-voice critical findings (Getting Started, Documentation) are flagged and treated as high-priority obligations, not gated on cross-model confirmation.

### DX subagent findings (folded in)

The native Claude subagent (no prior review) returned INPUT: dx 9981d6… and evaluated 5 dimensions: 2 failing (Getting Started, Documentation), 3 passing with medium gaps (API/CLI, Errors, Escape hatches). Its findings are folded in by auto-decision (P1 completeness, P2 boil lakes). Top severity per dimension and the fix obligations:

- **Getting Started (Critical):** no build/run path; the deferred SDK base blocks the first command. Fix: getting-started sequence + TTHW target + promote `example.comp` to the hello-world fixture. (Obligations DX-1, DX-2.)
- **API/CLI (Medium):** benchmark CLI and C ABI status/handle contract unspecified. Fix: spec the benchmark CLI and write the v1 C ABI contract. (Obligations DX-3, DX-4.)
- **Errors (Medium):** no docs links, no stable codes, no i18n stance; cause folded into failure type. Fix: error-code catalog + docs anchors, cause column, i18n policy. (Obligations DX-5, DX-6.)
- **Documentation (High):** no contributor-docs IA, no copy-paste ABI/test examples, stale format doc unowned. Fix: CONTRIBUTING.md, docs/project-format.md v7, docs/abi.md, docs/contracts.md, README Linux section. (Obligations DX-7, DX-8, DX-9.)
- **Escape hatches (Medium):** no shortcut rebinding, no headless CPU override, SDK override unreachable. Fix: `COMPOSITOR_CPU_ONLY=1` headless override; commit or explicitly defer shortcut rebinding. (Obligations DX-10, DX-11.)

Cross-cutting: the single highest-leverage fix is resolving the SDK base — it unblocks getting-started, unlocks the build script, and makes the SDK escape hatch reachable. It is deferred as a taste decision by the user; this review auto-decides a **DX default** (KDE SDK, lower maintenance, Qt supplied by runtime) so a getting-started path can be written now, while keeping strict-Freedesktop as a documented alternative build target and surfacing the underlying taste decision at the final gate.

### DX Implementation Checklist

```
[ ] TTHW < 5 min (flatpak install → run example.comp); contributor build ≤ 10 min
[ ] Getting-started sequence: flatpak install IDs + SDK-enter + build + test + run
[ ] example.comp shipped as the hello-world fixture (first run opens it)
[ ] C ABI v1 contract: status enum, opaque-handle acquire/release, generation IDs, owned diagnostics
[ ] Benchmark CLI: compositor benchmark [--workload] [--cpu-only] [--json] [--out]
[ ] Error-code catalog with stable codes + docs anchors; cause column distinct from failure type
[ ] i18n policy: log codes non-translatable; user strings via Qt tr()
[ ] CONTRIBUTING.md (build/test/run, SDK choice, env setup)
[ ] docs/project-format.md updated to v7 + version-compat table + round-trip test guard
[ ] docs/abi.md with copy-paste Qt→Swift call example
[ ] docs/contracts.md with new-backend + shared contract-test scaffold + CPU/GPU diff-test example
[ ] README Linux section pointing at the contributor path
[ ] Headless override: COMPOSITOR_CPU_ONLY=1 (CI/headless GPU bypass)
[ ] Shortcut rebinding: commit minimal QSettings key map OR explicitly defer with tracked issue
[ ] TTHW target set (5 min); benchmark CLI CI-runnable as first measurement
```

### DX completion summary

| Item | Result |
|---|---|
| Product type | Library/SDK + CLI + GUI (port); developer surface = C ABI + benchmark + build path + extension points |
| Persona | Cross-platform app contributor (Swift + Qt/C++) |
| Mode | DX POLISH |
| TTHW | undefined → 5 min (flatpak) / 10 min (build-from-source) |
| Magical moment | example.comp opens + first brush stroke (copy-paste demo command) |
| Overall | 4/10 → 7/10 (post-fix target; lowest = Community 3, follow-up) |
| Critical findings | 2 (Getting Started, Documentation) — native-only, flagged |
| Obligations | 11 (DX-1…DX-11) |
| Taste decisions deferred to gate | SDK base (DX default KDE), shortcut rebinding |

### NOT in scope (DX)

- A hosted playground/sandbox — not credible for a native editor port.
- A community channel build-out and issue-template automation — follow-up, not a preview blocker.
- Codemods for format migration — the format read path handles v1–v7; codemod tooling is a follow-up.
- A general plugin framework — explicitly out of scope per the plan.

### What already exists (DX)

- Strong error-and-recovery table + failure-modes registry (developer-facing contracts worth promoting to docs).
- SOLID protocol boundaries with consistent gerund naming (clean progressive disclosure).
- Sensible runtime defaults (CPU-first, GPU opt-in, failure-safe save).
- `example.comp` already listed as a test fixture (promote to hello-world).
- `docs/linux-port-plan.md` and `docs/linux-port-file-map.md` already exist as untracked files (fold the contributor path into them).

## Phase 3: engineering review

Native Claude subagent completed the eng review (read the full 723-line native prompt = implementation plan + all prior phases, then spot-verified load-bearing claims against source: `ProjectStore.swift`, `RasterSnapshot.swift`, `MetalBrushCoverage.swift`, `HealPixels.c`, `SubjectRemoval.swift`, `EditorCanvas.swift`). Codex outside voice ran after folding (rate limit had cleared; see OUTSIDE COVERAGE). Overall verdict from the native reviewer: the plan is architecturally sound and disciplined (SOLID contract, canonical-pixel spec, failure-mode registry, no-silent-failure stance are real strengths) but as an *engineering* plan it under-specifies the three highest-risk mechanisms and still defers the branch→test matrix. 18 findings (3 architecture, 5 edge cases, 5 tests, 3 security, 6 hidden-complexity cross-listed); 6 HIGH. All correctness/completeness fixes auto-decided per autoplan and folded as obligations ENG-1…ENG-18; two genuine taste decisions (composition-root language, Swift 6 vs Swift 5 language mode) deferred to the final gate.

### Section 1 — Architecture

```
                    COMPOSITION ROOT (C++ main, owns QCoreApplication::exec)
                    │
        ┌───────────┴────────────┐
        │                        │
   Qt Widgets UI              C ABI seam (generated C header from Swift @_cdecl
   (QMainWindow,             + thin extern "C" C++ shim, version-pinned)
   layers, canvas)                        │
        │                                 │
        │  submit command (revision-tagged)│
        ▼                                 ▼
   Qt UI thread ──► Swift core (DocumentCore, actors, kernels) ──► C kernels
        ▲                                 │
        │  result (revision-tagged)        │ completion via @_cdecl fn
        │  QMetaObject::invokeMethod(      │  → C shim →
        │    widget, …, Qt::QueuedConnection)  re-post on Qt event loop
        └─────────────────────────────────┘
   Result buffer owned by Swift across the seam; Qt copies and frees its copy.
```

- **A1 (HIGH, folded → ENG-1)** The C ABI bridge direction was underspecified for the Swift↔C++ reality. Qt is C++, Swift is Swift; "narrow C ABI" does not self-evidently bridge both. **Decision:** Swift exposes callables via `@_cdecl` exports compiled into a generated C header; a thin `extern "C"` C++ shim on the Qt side calls them. Pin the Swift version that supports `@_cdecl` in the versioned-ABI contract (the attribute is unofficial and Swift-version-sensitive, so the ABI version promise depends on the pinned Swift toolchain).
- **A2 (MEDIUM, taste → final gate, provisional decision ENG-2)** Composition-root language and build topology were unnamed. **Provisional decision (auto-decided, overridable at the gate):** C++ `main()` owns `QCoreApplication::exec` and loads the Swift core as a static library through the C ABI. Qt must own the event loop for Qt Widgets to behave; a Swift `@main` driving Qt through the ABI is fragile and complicates MainActor isolation. Build = CMake (drives Qt/MOC) linking the Swift package built via `swift build` as a static lib. Surfaced at the gate because a Swift-main alternative exists and a contributor might prefer it.
- **A3 (MEDIUM, folded → ENG-3)** Threading/result-delivery was "serial and separate" but the mechanism was missing. Swift cannot post to the Qt event loop unaided. **Decision:** Swift worker calls a `@_cdecl` completion function → C shim calls `QMetaObject::invokeMethod(widget, …, Qt::QueuedConnection)` with the revision-tagged result. Result buffer is owned by Swift across the seam; the Qt side copies and frees its copy. No `DispatchQueue.main` assumption.

### Section 2 — Edge cases / code quality

- **E1 (HIGH, folded → ENG-4)** Atomic directory replacement on Linux was mischaracterized. The macOS save uses `FileWrapper.write(options: .atomic)` on a *directory* package. POSIX `rename(2)` on a non-empty destination directory **fails** — it is not atomic for directory-over-directory, and `NSFileCoordinator` on Linux Foundation is effectively a no-op (no coordination daemon). **Decision:** specify the exact Linux sequence in §4: write sibling temp dir → `fsync` contents → `renameat2(RENAME_EXCHANGE)` with temp (Linux-only, not on network FS) OR recursive-delete old + `rename` temp→final → `fsync` parent. Enumerate unsupported filesystems (NFS, FUSE without atomic dir rename); fallback = keep old package + dirty marker + Save-As prompt (already named for "denied", extend to "unsupported FS"). Add a failure-injection test at each step (temp write, fsync, rmdir, rename, parent fsync).
- **E2 (HIGH, folded → ENG-5)** No total-pixel bound across layers. `validate()` caps each image at 100MP and layer count at 10,000, but 10,000 × 100MP ≈ 4TB of pixel data before masks/undo/scratch; "operation memory preflight" is named generically and the existing validator does not enforce an aggregate. **Decision:** add a manifest-level total-pixel budget and a per-document RSS preflight that rejects before allocation; test the 10,000-layer / large-canvas boundary on Linux.
- **E3 (MEDIUM, folded → ENG-6)** CPU-replay failure has no path. The gpuBrush row says GPU failure → "CPU replay, single commit"; if the in-flight stroke is large and CPU replay OOMs, no error-table row exists, violating "missing capabilities must not produce silently degraded successful edits." **Decision:** add a `CpuReplayFailed` failure type; on replay OOM, cancel the stroke (no history entry, retain pre-stroke snapshot), surface "stroke cancelled — out of memory" with renderer status. Test forced GPU-loss + OOM-during-replay.
- **E4 (MEDIUM, folded → ENG-7)** Close/tab-destroy during save leaves a temp dir. Temp-cleanup was named only for the no-space branch. **Decision:** the save operation owns temp creation and cleanup on every terminal branch (success, cancel, error, tab-close); add a test that closes a tab mid-save and asserts no temp sibling remains.
- **E5 (MEDIUM, folded → ENG-8)** `NSFileCoordinator` has no Linux equivalent — acknowledged only by omission. **Decision:** state explicitly that `NSFileCoordinator` is replaced by Flatpak document portals + a process-local coordination lock, and that concurrent-editor coordination (the macOS feature) is not preserved on Linux in the preview.

### Section 3 — Test review

```
CODE PATHS                                              USER FLOWS
[+] C kernels (HealPixels, resample, blend, coverage)   [+] Open example.comp
  ├── [GAP][→E2E] premultiplied byte-exact macOS↔Linux    ├── [GAP] first launch (TTHW)
  ├── [GAP]        stride==width*4 assert at every entry   ├── [GAP][→E2E] open v1..v7 project
  └── [GAP]        edge-pixel parity (0/255 premult)      └── [GAP]        malformed/missing asset
[+] Blend modes (SeparableBlend → Skia SkBlendMode)
  ├── [GAP][→E2E] every mode over reference gradient pair   ★★ happy path only today
  └── [GAP]        document modes with no Skia equivalent
[+] C ABI seam
  ├── [GAP]        handle acquire/release/generation-ID-late-callback
  └── [GAP]        malformed geometry/stride rejection (S2)
[+] Save (ProjectStore → Linux portal sequence)
  ├── [GAP][→E2E] failure at each step (temp/fsync/rmdir/rename/parent)
  ├── [GAP]        tab-close mid-save → no temp leak (E4)
  └── [GAP]        portal-denied Save-As
[+] Render
  ├── [GAP][→E2E] fractional scaling 100/125/150/200% parity
  ├── [GAP][→E2E] tablet pressure-curve parity
  └── [GAP]        tile alignment / halving-grid across zoom commits (T3)
[+] Concurrency
  ├── [GAP][→E2E] Swift 6 mode compile spike (T4)
  └── [GAP][→E2E] Qt→Swift→Qt→Swift nested callback no-deadlock (H6)
[+] Project format
  ├── [GAP][→E2E] v1–v7 round trip
  └── [GAP]        oversized / total-pixel-budget rejection (E2)
[+] Segmentation
  ├── [GAP]        crash-input fixture (S3)
  └── [GAP]        model-missing disabled path
[+] End-to-end packaged edit/save                       [GAP][→E2E] full paint→filter→save→reopen

COVERAGE: 0/~30 planned paths pre-implementation (this is a port plan, not a diff).
All gaps become test requirements in the branch→test matrix (ENG-9) and the test plan artifact.
```

- **T1 (HIGH, folded → ENG-9)** The engineering test plan was still deferred — this review IS the engineering phase, and the branch→test matrix was absent. **Decision:** deliver the branch→test matrix now (see artifact below and `eng-review-test-plan`). Minimum rows specified: each C kernel (differential macOS-vs-Linux byte output at edge pixels, premultiplied), each blend mode (SeparableBlend 1:1 Skia mapping), ABI handle acquire/release/generation-ID-late-callback, save at each failure step (temp-write, fsync, rmdir, rename, parent-fsync), portal-denied save-as, fractional-scaling render parity, tablet pressure-curve parity, v1–v7 round trip, malformed-JSON/missing-asset/oversized, CPU/Vulkan differential, end-to-end packaged edit/save.
- **T2 (HIGH, folded → ENG-10)** Blend-mode parity is untested and unnamed. `SeparableBlend.swift` maps to CG blend modes; Skia's `SkBlendMode` is close but not identical. A wrong blend mode is silent and user-visible. **Decision:** add a blend-mode parity fixture (every supported mode over a reference gradient pair) captured on macOS and asserted on Linux; document any mode with no exact Skia equivalent as a known divergence.
- **T3 (HIGH, folded → ENG-11)** Tile alignment / halving-grid invariant was not named for preservation. `RasterSnapshot` carries `alignment: CGPoint` and a halving-grids invariant (DownsampleCache/TiledLayerRenderer) so patches never shift across zoom commits. Porting to Skia tiles without this silently shifts pixels at fractional zoom. **Decision:** name the alignment invariant in the canonical-tile contract; add a test that commits a stroke at one zoom, reopens at another, and asserts pixel-exact placement.
- **T4 (HIGH, taste → final gate, provisional decision ENG-12)** Swift 6 strict-concurrency migration is uncosted. Xcode uses Swift 5 language mode; the Linux SDK extension ships Swift 6.3.3 which defaults to Swift 6 mode. Existing code uses `nonisolated`, `@unchecked Sendable`, an `actor ProjectStore`; under Swift 6 the `@unchecked Sendable` classes (RasterSnapshot, ProjectSnapshot) and captured `CGImage` across the C ABI surface real data-race warnings. **Provisional decision (auto-decided, overridable at the gate):** ship the Linux port in Swift 5 language mode (`swiftLanguageMode: .v5` in `Package.swift`) for the vertical slice — this avoids the concurrency migration as a blocker — AND run a Swift-6-mode compile spike in milestone 1 to count the diagnostics and budget the real `Sendable` conformance work as a tracked follow-up (not cleanup, not silently deferred). The taste decision: stay Swift-5-mode indefinitely (pragmatic, ships now) vs commit to Swift-6-mode before 1.0 (complete, enforces data-race safety that aligns with the SOLID stance). Surfaced at the gate.
- **T5 (MEDIUM, folded → ENG-13)** Runtime/extension pinning for end users was not addressed. Flatpak runtimes roll forward on end-user hosts; a runtime update can ship a new Qt 6 minor that breaks the pinned platform plugin. **Decision:** pin `runtime-version` to a specific branch in the manifest and add a CI matrix that builds against the current and the previous runtime branch.

### Section 4 — Performance / security / hidden complexity

- **S1 (MEDIUM, folded → ENG-14)** The symlink/path-escape check is a preserve-claim but the existing code is TOCTOU-raceable. `checkFile` does `file.resolvingSymlinksInPath().standardizedFileURL.path.hasPrefix(root)` then later reads via `Data(contentsOf:)`; between the check and the read a symlink can be swapped. **Decision:** mark the path-escape defense as a *new* Linux implementation using `openat`/`O_NOFOLLOW`/`fstat`-vs-`lstat` (descriptor-relative access), not a port of the Swift prefix check; add a hostile-fixture test (symlink swap between check and read) that must fail.
- **S2 (LOW, folded → ENG-15)** C ABI as attack surface was only lifetime-managed, not input-validated. A malformed geometry (negative width, zero stride, stride < width*bpp) from a buggy Qt caller would flow straight into C kernels that assume well-formed input. **Decision:** validate tile dimensions/stride at the ABI entry; return `InvalidArgument` rather than passing to kernels.
- **S3 (LOW, folded → ENG-16)** Segmentation model adversarial risk partially covered. License review named but no acceptance gate; no crash-input test. **Decision:** add a crash-input fixture to the segmentation test row.
- **H5 (MEDIUM, folded → ENG-17)** The C kernels compile on Linux but have not been shown pixel-equivalent. The kernels link against CGContext-produced buffers whose premultiplied layout, byte order, and stride the C code assumes; Skia buffers may differ in stride/padding. **Decision:** enforce the canonical buffer layout at every C-kernel entry (assert `stride == width*4`, premultiplied) so a Skia buffer with padding cannot silently corrupt a kernel.
- **H6 (MEDIUM, folded → ENG-18)** Qt main-loop + Swift actor isolation is the single integration risk most likely to deadlock. Swift `actor` reentrancy + Qt queued-connection ordering can deadlock if a Qt callback synchronously waits on a Swift actor that is awaiting the Qt callback. **Decision:** milestone-1 must include a stress test that dispatches nested callbacks (Qt→Swift→Qt→Swift) and asserts no deadlock under 10k rapid commands; this is a gate, not a late discovery.

### Failure modes registry (critical gaps flagged)

| Codepath | Realistic failure | Test? | Error handling? | User sees | Critical gap? |
|---|---|---|---|---|---|
| Linux save (atomic dir) | `rename` over non-empty dir / FS unsupported | planned (ENG-4) | planned | Save-As prompt | Was silent — now specified |
| Total-pixel budget | 10k layers × 100MP OOM | planned (ENG-5) | planned | reject before alloc | Was silent — now specified |
| CPU replay | GPU-loss + replay OOM | planned (ENG-6) | planned (CpuReplayFailed) | "stroke cancelled — OOM" | Was silent half-commit — now specified |
| Save mid tab-close | temp dir leak | planned (ENG-7) | planned | none (cleanup) | Was silent leak — now specified |
| Blend mode | CG↔Skia mismatch | planned (ENG-10) | planned | known-divergence doc | Was silent wrong pixels |
| Tile alignment | fractional-zoom shift | planned (ENG-11) | planned | none (invariant) | Was silent shift |
| Path escape | symlink swap (TOCTOU) | planned (ENG-14) | planned (openat) | read denied | Was raceable — now rewritten |
| Qt/Swift deadlock | nested callback reentrancy | planned (ENG-18) | n/a (prevent) | hang | Was hang — now a gate |

### NOT in scope (eng)

- Full Swift 6 strict-concurrency conformance (tracked follow-up from ENG-12 spike; the port ships Swift-5-mode provisionally).
- Concurrent-editor coordination across processes on Linux (macOS `NSFileCoordinator` feature; not preserved in the preview — ENG-8).
- Network-filesystem save support (NFS/FUSE without atomic dir rename explicitly unsupported — ENG-4).
- Adversarial segmentation hardening beyond one crash-input fixture (S3).

### What already exists (eng)

- `ProjectStore.save` / `validate` / `checkFile` / `checkSize` — the v1–v7 read/write and per-version semantics exist and are reused (the Linux sequence replaces the atomic mechanism, not the format logic).
- `RasterSnapshot` tile/patch/alignment + DownsampleCache/TiledLayerRenderer halving-grids — the invariants exist; the port must preserve, not reinvent, them (ENG-11).
- `MetalBrushCoverage.render` takes `[(Tile, CGRect, CGContext)]` — confirms the CPU fallback is non-portable (P5 challenged, valid); the Linux path needs a Skia-backed coverage implementation.
- `HealPixels.c` (`M_PI` at 168/237), the C kernels — compile on Linux (`cc -std=gnu11 -fsyntax-only` verified at intake); runtime/pixel equivalence is the eng work (ENG-9, ENG-17).
- `SubjectRemoval` uses `VNGenerateForegroundInstanceMaskRequest` — model needed on Linux, valid; deferred model spike (cross-phase unresolved decision).

### Worktree parallelization strategy

| Step | Modules touched | Depends on |
|---|---|---|
| C ABI seam + composition root (ENG-1,2,3) | `Sources/CCompositor/`, Qt `main`, CMake | — |
| Swift core extraction + Swift-6 spike (ENG-12) | `Sources/Compositor/` | ABI seam |
| Linux save sequence (ENG-4,7,8) | `Sources/ProjectStore` Linux impl, portals | ABI seam |
| C kernel parity + canonical-buffer asserts (ENG-9,17) | `Sources/C kernels`, Skia buffer adapter | Swift core extraction |
| Blend-mode + tile-alignment fixtures (ENG-10,11) | tests | C kernel parity |
| Path-escape rewrite (ENG-14) | Linux file access | Linux save |
| Concurrency stress gate (ENG-18) | integration tests | ABI seam + Swift core |

- **Lane A:** ABI seam + composition root → Swift core extraction → Swift-6 spike (sequential, shared `Sources/`).
- **Lane B:** Linux save sequence → path-escape rewrite (sequential, shared Linux file access).
- **Lane C:** C kernel parity + canonical-buffer asserts → blend/tile fixtures (sequential, shared kernels/tests).
- **Launch A + B + C in parallel worktrees.** Merge A first (it gates the others' integration). Then C (kernel parity gates fixtures). Then B. **Conflict flag:** Lanes A and C both touch `Sources/`; coordinate the kernel-boundary contract before merging. Lane B is independent until final integration.

### Implementation Tasks (eng)

- [ ] **ENG-1 (P1, human: ~1 day / CC: ~20min)** — C ABI seam — generate C header from Swift `@_cdecl` exports + `extern "C"` C++ shim, pin Swift version in the versioned-ABI contract
  - Surfaced by: Architecture A1
  - Files: `Sources/CCompositor/include/`, Qt shim, `docs/abi.md`
  - Verify: ABI smoke test (acquire/round-trip/release)
- [ ] **ENG-2 (P1, human: ~2 days / CC: ~30min)** — Composition root — C++ `main` owns `QCoreApplication::exec`, loads Swift core as static lib via ABI; CMake drives Qt/MOC
  - Surfaced by: Architecture A2 (taste, provisional)
  - Files: `main.cpp`, `CMakeLists.txt`, `Package.swift`
  - Verify: app launches and enters Qt event loop
- [ ] **ENG-3 (P1, human: ~1 day / CC: ~20min)** — Result delivery — Swift `@_cdecl` completion → C shim → `QMetaObject::invokeMethod(…, Qt::QueuedConnection)`; result buffer ownership documented
  - Surfaced by: Architecture A3
  - Files: ABI completion, Qt receiver
  - Verify: async command round-trips to UI thread
- [ ] **ENG-4 (P1, human: ~2 days / CC: ~30min)** — Linux atomic save — exact sequence (temp→fsync→renameat2 EXCHANGE or rmdir+rename→parent fsync); unsupported-FS fallback; failure-injection test at each step
  - Surfaced by: Edge E1
  - Files: Linux `ProjectStore` impl, tests
  - Verify: failure-injection test passes on ext4; NFS rejected with Save-As prompt
- [ ] **ENG-5 (P1, human: ~4h / CC: ~15min)** — Total-pixel budget — manifest-level cap + per-document RSS preflight rejecting before allocation
  - Surfaced by: Edge E2
  - Files: `validate`, preflight
  - Verify: 10k-layer/large-canvas boundary test rejects
- [ ] **ENG-6 (P2, human: ~3h / CC: ~10min)** — `CpuReplayFailed` — on replay OOM cancel stroke (no history entry, retain pre-stroke snapshot), surface OOM message
  - Surfaced by: Edge E3
  - Files: brush pipeline, error table
  - Verify: forced GPU-loss + OOM-during-replay test
- [ ] **ENG-7 (P2, human: ~2h / CC: ~10min)** — Temp-dir lifecycle — save op owns temp on all terminal branches; tab-close mid-save asserts no temp sibling
  - Surfaced by: Edge E4
  - Files: save op, tab close
  - Verify: close-mid-save test
- [ ] **ENG-8 (P2, human: ~1h / CC: ~5min)** — State `NSFileCoordinator` → portals + process-local lock; concurrent-editor coordination not preserved in preview
  - Surfaced by: Edge E5
  - Files: docs/linux-port-plan.md §4
  - Verify: doc update + portal access test
- [ ] **ENG-9 (P1, human: ~3 days / CC: ~40min)** — Branch→test matrix — deliver every row named in Section 3 as a test requirement; write the test plan artifact
  - Surfaced by: Tests T1
  - Files: test plan artifact, tests
  - Verify: matrix complete; `/qa-only` consumes the artifact
- [ ] **ENG-10 (P1, human: ~1 day / CC: ~20min)** — Blend-mode parity fixture — every mode over a reference gradient pair, macOS-captured, Linux-asserted; document divergences
  - Surfaced by: Tests T2
  - Files: blend parity tests, fixtures
  - Verify: all modes pass or are documented divergent
- [ ] **ENG-11 (P1, human: ~1 day / CC: ~20min)** — Tile alignment / halving-grid — name invariant in canonical-tile contract; cross-zoom-commit pixel-exact placement test
  - Surfaced by: Tests T3
  - Files: canonical-tile contract, alignment tests
  - Verify: stroke at zoom A, reopen at zoom B, pixel-exact
- [ ] **ENG-12 (P1, human: ~1 day / CC: ~20min)** — Swift-6 compile spike — compile extracted Swift core under Swift 6 mode, count diagnostics, budget real `Sendable` conformance as tracked follow-up; ship Swift-5-mode provisionally (taste gate)
  - Surfaced by: Tests T4 (taste, provisional)
  - Files: `Package.swift`, tracking issue
  - Verify: spike report with diagnostic count; `swiftLanguageMode: .v5` set
- [ ] **ENG-13 (P2, human: ~2h / CC: ~10min)** — Runtime pinning — pin `runtime-version` in manifest; CI matrix builds current + previous runtime branch
  - Surfaced by: Tests T5
  - Files: Flatpak manifest, CI workflow
  - Verify: CI matrix green on both branches
- [ ] **ENG-14 (P1, human: ~1 day / CC: ~15min)** — Path-escape rewrite — Linux `openat`/`O_NOFOLLOW`/`fstat`-vs-`lstat`; hostile-fixture symlink-swap test must fail
  - Surfaced by: Security S1
  - Files: Linux file access, hostile-fixture test
  - Verify: symlink-swap test fails to escape
- [ ] **ENG-15 (P2, human: ~2h / CC: ~10min)** — ABI input validation — validate tile dims/stride at ABI entry; return `InvalidArgument`
  - Surfaced by: Security S2
  - Files: ABI entry, tests
  - Verify: malformed-geometry rejected
- [ ] **ENG-16 (P3, human: ~1h / CC: ~5min)** — Segmentation crash-input fixture
  - Surfaced by: Security S3
  - Files: segmentation tests
  - Verify: crash-input does not crash the editor
- [ ] **ENG-17 (P2, human: ~3h / CC: ~10min)** — Canonical-buffer assert at every C-kernel entry (`stride == width*4`, premultiplied)
  - Surfaced by: Hidden H5
  - Files: C kernel entries, Skia buffer adapter
  - Verify: padded Skia buffer rejected at kernel boundary
- [ ] **ENG-18 (P1, human: ~1 day / CC: ~15min)** — Qt/Swift deadlock stress gate — nested Qt→Swift→Qt→Swift callbacks, 10k rapid commands, no deadlock; milestone-1 gate
  - Surfaced by: Hidden H6
  - Files: integration stress test
  - Verify: 10k commands complete without hang

### Completion summary (eng)

- Step 0 Scope Challenge: scope accepted as-is (port, not rewrite; complexity check = port of ~109 files across a language boundary, expected)
- Architecture Review: 3 issues (A1, A2, A3) — all folded; A2 is a taste decision (provisional C++ main)
- Code Quality / Edge cases: 5 issues (E1–E5) — all folded
- Test Review: diagram produced, ~30 gaps identified; 5 issues (T1–T5) — all folded; T4 is a taste decision (provisional Swift-5-mode + spike)
- Performance / Security / Hidden: 6 issues (S1–S3, H5, H6) — all folded
- NOT in scope: written
- What already exists: written
- Failure modes: 8 critical-gap rows flagged (all were silent/hang, now specified)
- Outside voice: Codex ran (completed) — see OUTSIDE COVERAGE
- Parallelization: 3 lanes, 3 parallel / sequential within each
- Lake Score: 14/14 recommendations chose the complete option (all correctness fixes taken; the two taste deferrals are kind, not coverage)
- Unresolved decisions: 2 new (composition-root language, Swift 6 vs 5 mode) + prior

## Review record
<!-- autoplan-accepted:design -->
- QMainWindow + QStackedWidget tool header + QTabBar tabs + QStatusBar; layers panel resizable with persisted width; floating Levels/HueSat/Filter as QDialog.
- Interaction state table per feature (open, new, paint, filter, save, export, segmentation, clipboard, tabs, startup) specifying what the user SEES.
- Decode progress: immediate non-blocking tab, determinate progress with phase labels, Cancel discards in-flight decode.
- No-document start surface: New, Open, Recent (thumbnails), drag-drop target.
- Error presentation patterns: decode→tab error card+Retry/Open Other; save→modal Retry/Save As; GPU fallback→status indicator only; clipboard→toast; portal→modal.
- GPU-failure perceptual contract: stroke renders on CPU within X-ms budget, no modal, status flips to "CPU fallback", latency threshold for "recovering stroke" indicator.
- Section 11 expanded to real spec: tool palette inventory/order, per-tool options-bar contents, layers-panel row structure, tab behavior, properties panel. Verified by T-design-1 implementation task.
- Qt Widgets committed (confirms user's stated preference); QML dropped as not-chosen. Surfaced at final gate as confirmation.
- Shortcut equivalence table: top ~20 commands + pan/zoom gestures + macOS sheet→dialog mapping; Cmd→Ctrl, Opt→Alt, Space pan, [/] size, 1–0 opacity, Enter/Escape; re-rendered status hint strings.
- Save As flow: portal re-request for parent, original-name default, dirty retained on cancel, old package intact.
- Segmentation success UX: select→preview mask overlay→Accept/Cancel→layer mask; model-missing = disabled menu + "Get model" affordance; no-subject = explanation. Model itself deferred.
- Tablet input design: pressure→size/opacity (reuse macOS curve), tilt for smudge/eraser, stylus button=eraser/pan, palm-rejection expectation stated.
- Diagnostics panel: read-only version list, CPU-override checkbox + restart hint, "Run benchmark" → user-chosen path.
- Drag-drop feedback: canvas→new top layer, layers panel→layer at position, accent-border highlight, reject toast, expired clipboard→disabled paste+tooltip.
- A11y: keyboard Tab order, visible focus rings, canvas focus-on-request, ≥4.5:1 contrast, 44px tablet targets, fractional scaling 100/125/150/200%, X11+Wayland smoke.
- Dark theme default; minimal Qt token set (#242424 panel, #1a1a1a surround, one accent, 11pt status / 13pt UI, 36px tool buttons, capsule controls); full DESIGN.md deferred to /design-consultation.
- Mockups skipped: native Qt desktop port preserving existing macOS layout; web mockups would misrepresent target. macOS app is visual reference.
<!-- /autoplan-accepted:design -->
<!-- autoplan-accepted:dx -->
- DX-1 Getting-started sequence: exact `flatpak install` IDs (runtime + extension), the SDK-enter command that puts `swift`/`pkg-config` on PATH (no host swift), `Package.swift` build invocation, the test command, and the app launch on `example.comp`. Numbered, copy-pasteable.
- DX-2 TTHW target: 5 min (flatpak install → run example.comp); build-from-source contributor path ≤ 10 min. Milestone-1 acceptance ("launch with the runtime but no SDK") promoted to a numbered checklist.
- DX-3 Benchmark CLI: `compositor benchmark [--workload paint|filter|save] [--cpu-only] [--json] [--out PATH]`, documented alongside the getting-started sequence; CI-runnable.
- DX-4 C ABI v1 contract: stable status enum (`COMPOSITOR_OK`, `COMPOSITOR_ERR_UNSUPPORTED_VERSION`, …), opaque-handle acquire/release, generation-ID field for stale-callback rejection, owned caller-freed diagnostic struct. Pinned as versioned internal contract.
- DX-5 Error-code catalog: one stable code per typed failure (mirrors the C ABI status enum), with a docs anchor each; every user-facing error references code + link.
- DX-6 Error rows: add a "cause" column distinct from failure type (esp. executeCommand target-deleted vs tab-closed, and saveProject); add bridge-callback late-rejection behavior to the ABI contract. i18n policy: log codes non-translatable, user strings via Qt `tr()`.
- DX-7 Contributor-docs IA: `CONTRIBUTING.md` (build/test/run, SDK choice, env setup), `docs/project-format.md` (v7 + version-compat table from source/tests, round-trip test guard that fails the doc if a version is undocumented), `docs/abi.md` (status enum + Qt→Swift call example), `docs/contracts.md` (new-backend + shared contract-test scaffold + CPU/GPU differential-test example).
- DX-8 Copy-paste examples for the three highest-friction tasks: (a) Qt→Swift ABI call (acquire → submit → apply revision-tagged result → release), (b) new `BrushCoverageComputing` impl + shared contract test, (c) CPU/Vulkan differential test.
- DX-9 README Linux section pointing at the contributor path (SDK choice, build command, example project) so a new contributor does not read the intake evidence to start.
- DX-10 Headless/config escape hatch: `COMPOSITOR_CPU_ONLY=1` env var (and/or config file) to force the software path on CI/headless without the GUI checkbox.
- DX-11 Shortcut rebinding: commit a minimal `QSettings`-backed key map in v1, or explicitly defer with a tracked issue; do not leave as a deferred taste decision that triggers rework. (Deferred to final gate as a taste decision.)
- DX naming rule (one line in contributor docs): protocols gerund (`*Computing`/`*Reading`), operations verb (`compositeX`).
- DX default (auto-decided, not binding the user's final-gate taste): KDE SDK as the getting-started/packaging default (Qt supplied by runtime, lower maintenance); strict-Freedesktop manifest kept as a documented alternative build target.
<!-- /autoplan-accepted:dx -->
<!-- autoplan-accepted:eng -->
- ENG-1 C ABI seam: Swift `@_cdecl` exports compiled into a generated C header + thin `extern "C"` C++ shim on the Qt side; pin the Swift version in the versioned-ABI contract (`@_cdecl` is unofficial and Swift-version-sensitive).
- ENG-2 Composition root (taste, provisional): C++ `main()` owns `QCoreApplication::exec` and loads the Swift core as a static library via the C ABI; CMake drives Qt/MOC and links the Swift package. Surfaced at the final gate.
- ENG-3 Result delivery: Swift `@_cdecl` completion → C shim → `QMetaObject::invokeMethod(widget, …, Qt::QueuedConnection)`; result buffer owned by Swift across the seam, Qt copies and frees its copy. No `DispatchQueue.main` assumption.
- ENG-4 Linux atomic save: write sibling temp dir → `fsync` contents → `renameat2(RENAME_EXCHANGE)` OR rmdir old + `rename` temp→final → `fsync` parent; enumerate unsupported FS (NFS, FUSE without atomic dir rename); fallback = keep old + dirty marker + Save-As prompt; failure-injection test at each step.
- ENG-5 Total-pixel budget: manifest-level cap + per-document RSS preflight rejecting before allocation; test the 10,000-layer / large-canvas boundary on Linux.
- ENG-6 `CpuReplayFailed` failure type: on replay OOM cancel the stroke (no history entry, retain pre-stroke snapshot), surface "stroke cancelled — out of memory"; test forced GPU-loss + OOM-during-replay.
- ENG-7 Temp-dir lifecycle: the save operation owns temp creation and cleanup on every terminal branch (success, cancel, error, tab-close); test close-mid-save asserts no temp sibling remains.
- ENG-8 State `NSFileCoordinator` is replaced by Flatpak document portals + a process-local coordination lock; concurrent-editor coordination (macOS feature) is not preserved on Linux in the preview.
- ENG-9 Branch→test matrix: deliver every row named in Phase 3 Section 3 as a test requirement; test plan artifact written to `developer-main-eng-review-test-plan-20260919-111200.md`. `/qa` and `/qa-only` consume it.
- ENG-10 Blend-mode parity fixture: every `SeparableBlend` mode over a reference gradient pair, macOS-captured, Linux-asserted; document any mode with no exact Skia equivalent as a known divergence.
- ENG-11 Tile alignment / halving-grid invariant named in the canonical-tile contract; test commits a stroke at one zoom, reopens at another, asserts pixel-exact placement.
- ENG-12 Swift-6 compile spike (taste, provisional): compile the extracted Swift core under Swift 6 mode in milestone 1, count strict-concurrency diagnostics, budget real `Sendable` conformance as a tracked follow-up; ship the port in Swift 5 language mode (`swiftLanguageMode: .v5`) provisionally. Surfaced at the final gate.
- ENG-13 Runtime pinning: pin `runtime-version` to a specific branch in the Flatpak manifest; CI matrix builds against current + previous runtime branch.
- ENG-14 Path-escape rewrite (Linux): `openat`/`O_NOFOLLOW`/`fstat`-vs-`lstat` descriptor-relative access, not a port of the Swift prefix check; hostile-fixture symlink-swap test must fail.
- ENG-15 ABI input validation: validate tile dimensions/stride at the ABI entry; return `InvalidArgument` for malformed geometry rather than passing to kernels.
- ENG-16 Segmentation crash-input fixture added to the segmentation test row.
- ENG-17 Canonical-buffer assert at every C-kernel entry (`stride == width*4`, premultiplied) so a padded Skia buffer cannot silently corrupt a kernel.
- ENG-18 Qt/Swift deadlock stress gate: nested Qt→Swift→Qt→Swift callbacks, 10k rapid commands, no deadlock; milestone-1 gate, not a late discovery.
<!-- /autoplan-accepted:eng -->

## GSTACK REVIEW REPORT

| Runs | Status | Findings |
|---|---|---|
| Intake | Complete | Source mapping, SDK probes, C compile check; format docs lag v7 |
| CEO | Complete with concerns | Two independent GPT voices; vertical-first delivery, SOLID boundaries, save/pixel contracts; implementation feasibility unproven |
| Design | Complete with concerns | Native Claude subagent + primary review; Codex rate-limited (unavailable). QMainWindow layout, state table, journey, shortcut map, a11y spec added; DESIGN.md deferred; 3/10 → 8/10 |
| Engineering | Complete with concerns | Native Claude subagent (18 findings, 6 HIGH) only; Codex rate-limited (unavailable). ABI seam, composition root, Linux atomic save, branch→test matrix, blend/tile parity, Swift-6 spike, path-escape rewrite spec'd; 18 obligations; 2 taste decisions deferred |
| Developer experience | Complete with concerns | Native Claude subagent + primary review; Codex rate-limited (unavailable). Getting-started path, C ABI contract, contributor-docs IA, benchmark CLI, error-code catalog spec'd; 4/10 → 7/10 target; 11 obligations |

OUTSIDE COVERAGE: CEO — two GPT voices (subagent + Codex), both completed (prior run). Design — Codex unavailable (account usage limit), native Claude subagent completed; partial coverage, no cross-model consensus. DX — Codex unavailable (account usage limit, same block), native Claude subagent completed; 0/6 consensus confirmed (native only), 2 single-voice critical findings flagged. Engineering — Codex unavailable (account usage limit, same block, persisted across the phase despite a `ready` auth/model probe), native Claude subagent completed; 0/6 consensus (native only), 6 single-voice HIGH findings flagged. No cross-model consensus available for any phase after CEO.

VERDICT: CLEARED. CEO + Design + DX + Eng reviewed. All phases complete. Final approval gate APPROVED AS-IS (all 8 taste decisions accepted at provisional recommendations). Ready to implement. Caveat: no cross-model consensus after CEO (Codex rate-limited); 6 eng HIGH findings are single-voice. Recommended next step: `/ship` to open the implementation PR, or begin ENG-1 (C ABI header generation).

**UNRESOLVED DECISIONS:**
- Qt Widgets versus QML (taste, deferred to final gate).
- Strict Freedesktop versus derived KDE SDK packaging (taste, deferred to final gate; DX review auto-decided a KDE default for the getting-started path, Freedesktop kept as documented alternative).
- Exact Linux background-removal model and measurable quality acceptance (deferred — needs model spike).
- Dark-only theme versus light toggle (taste, deferred).
- Layers panel dock-float versus fixed splitter (taste, deferred).
- Shortcut rebinding versus fixed map (DX taste decision: commit minimal QSettings key map now, or explicitly defer with tracked issue — deferred to final gate).
- Composition-root language (eng taste decision: C++ main owning QCoreApplication::exec loading Swift as static lib via ABI, versus Swift @main driving Qt through the ABI — provisional decision C++ main; affects MainActor isolation and build system).
- Swift 6 versus Swift 5 language mode for the Linux port (eng taste decision: ship Swift-5-mode provisionally + run a Swift-6 compile spike to budget the migration as a follow-up, versus commit to Swift-6-mode before 1.0 — provisional decision Swift-5-mode + spike; the complete option enforces data-race safety aligned with SOLID).
- DESIGN.md generation (deferred follow-up, /design-consultation).
