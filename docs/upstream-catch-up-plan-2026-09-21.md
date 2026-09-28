# Catch up with macOS through API compatibility

Date: 2026-09-21. Status: implementation plan; only the Brush opacity fix and its regression tests were implemented in this continuation.

## 1. Objective and comparison baseline

Run the macOS editor's document, editing, persistence, and rendering orchestration code on GNU/Linux without maintaining a second implementation of that logic. Keep platform differences below the Apple API boundary or in thin Qt presentation adapters. Translate the two Metal implementations behind their existing interfaces; prefer Vulkan acceleration and retain correct CPU paths using Skia, OpenCV, and the existing kernels where each operation fits.

The upstream reference was refreshed with `git fetch upstream` during this review:

- `upstream/main`: `c39da13b5db11bc8678ec04a7a748e1e0a589244`, macOS 1.1.8.
- Linux HEAD: `7b4241cfea3824bc6c7bcad925aa2bff40b11890`, branch `GNU_Linux`, plus substantial uncommitted work from Claude's last session.
- Comparison includes that working tree, not just the last commit.
- The earlier design and implementation were reviewed through the masked-opacity investigation; the relevant findings are recorded below.

The existing historical [feature matrix](feature-parity-matrix.md) targets macOS 1.0.4. Its “100%” labels are not evidence of parity with 1.1.8. The later [upstream audit](upstream-parity-audit.md) is useful as a feature inventory, but its percentages also need regeneration after the current migration.

## 2. What Claude actually completed

| Area | Verified implementation | Remaining gap / decision |
|---|---|---|
| Compile original Swift | `Package.swift` builds module `Compositor` using `Sources/UpstreamCore` directory links into `Compositor/{Document,IO,Rendering}`. | Keep this foundation. A compiling module does not mean the Qt app uses it. |
| Original tests | `CompositorUpstreamTests` compiles `CompositorTests` directly. Ten files have explicit exclusions. | Fix wrapper semantics; separate genuine macOS-only tests from stale upstream tests. |
| API modules | CoreGraphics, AppKit, SwiftUI subset, CoreImage, ImageIO, Accelerate, CoreVideo, Vision, FoundationCompat, UTType exist. | Behaviour remains incomplete in several modules. Avoid recreating modules already built. |
| Brush GPU | Same-API `MetalBrushCoverage` override calls existing Vulkan compute with adaptive C CPU fallback. | Preserve it; audit failure lifetime and concurrent access. Real hardware qualification remains separate from software Vulkan tests. |
| Keyboard routing | `HostedNativeContent`, hosted table stand-in, subtree window notifications, CGEvent key translation, shared text field editor. | Brush routing passes. A headless hosted table is not a finished Qt Layers panel implementation. |
| Masked opacity | Claude investigated but did not finish the fix. | This continuation fixed duplicate context-alpha multiplication in the Linux CGContext wrapper. |
| Layer effects GPU | `Sources/Overrides/MetalLayerEffects.swift` preserves the entry point. | `shared` is uninitialized/nil and `render` throws. Upstream's CPU path is used; no Vulkan effects implementation exists here yet. |
| Canvas GPU | Vulkan device setup and failover infrastructure exist. | `linux/graphics/SkiaBridge.cpp:vulkan_render_rgba` still returns `-2`; it always requests raster fallback. Device creation is not GPU rendering. |
| CoreImage | A lazy image graph with a Swift CPU evaluator implements the consumed filters. | It is not yet a general Vulkan/Skia/OpenCV execution chain. Fix pixel correctness before accelerating it. |
| Vision | Same-API observations, mask plumbing, registration seam, classical border-colour segmenter. | OpenCV GrabCut/model inference is not wired into this seam. API compatibility does not prove Apple Vision segmentation quality. |
| Type tool | AppKit font/text API shapes compile. | `NSLayoutManager.drawGlyphs` is empty; glyph count is zero. Needs real text layout and rasterization. |
| Qt application | Mature shell, platform-service interfaces, smoke journeys. | `hostMain.swift` still imports `CompositorCore`; its C bridge constructs the forked `EditorSession`. |
| Keep upstream pristine | Original Swift, C kernels and `CompositorTests` now match refreshed upstream; the new guard passes. | Linux kernel contract checks still need to be enforced at Linux adapter boundaries without editing upstream sources. |
| Drift protection | Proposed in Claude's plan. | `scripts/check-upstream-clean.sh`, override manifest, signature test referenced in a comment, and `.github/workflows` are absent at review time. |
| Release build | Flatpak/CMake/SwiftPM definitions exist. | Local verification uses KDE SDK 6.10 and scratch-built libraries; shipping manifest uses 6.11 and an older Skia archive. Reproducibility is a release gate. |

**Brush evidence:** baseline rerun reproduced 8 masked-opacity assertions; the 4 keyboard assertions from the historical 12-issue cluster already passed. Two new wrapper tests failed before the fix, showing alpha 32 instead of 64 through a half mask and half opacity. After the fix: **19/19 upstream Brush tests and 15/15 CoreGraphics compatibility tests pass**. No macOS source or upstream test was changed in this continuation.

Fresh full-suite verification: **478/478 Linux/core tests pass; 214/232 upstream tests pass, with 18 failed tests and 30 failed assertions**. All 18 failing test names were present in Claude's earlier session. The full run included the slow tiled-render test, which passed. Results and remaining failures are recorded in [the upstream test baseline](upstream-test-baseline.md). Test totals, failed tests, failed assertions, and exclusions must remain separate metrics.

## 3. Architecture to keep

```text
Qt presentation and platform services
        |
        | stable C ABI: commands, state, pixels, completion
        v
Linux integration sources compiled into the Compositor module
        |
        v
Original EditorSession / document / IO / rendering orchestration
        |
        +-- imports Apple-shaped compatibility modules
        |       +-- CoreGraphics --> Skia Vulkan or Skia raster
        |       +-- CoreImage ----> capable filter backend --> CPU reference
        |       +-- Vision -------> segmentation provider / refinement
        |       +-- ImageIO ------> Qt codecs / libheif / portable codecs
        |       +-- AppKit -------> event, text, clipboard, dialog adapters
        |
        +-- same-API MetalBrushCoverage --> Vulkan compute --> C coverage
        +-- same-API MetalLayerEffects --> Vulkan effects --> CPU effects
```

Preserve original access control. `EditorSession` and many model types are internal. A separate production target cannot simply `import Compositor` and call them. Place thin Linux bridge source files in a Linux-owned directory and include them in the same `Compositor` module. Export only the public C ABI. Do not make upstream declarations public or ship a bridge relying on `@testable import`.

Keep backend interfaces narrow. Reuse `BrushCoverageComputing`, the canvas C ABI, `ForegroundSegmentationBackend`, codec registration, and Qt platform services. Introduce an effects/filter interface only when there are concrete alternate implementations. Compatibility modules must never import the editor domain or the legacy `CompositorCore`.

Use SOLID at the boundary:

- Single responsibility: API semantics in the shim; algorithms/resources in the backend; widgets in Qt.
- Open/closed: upstream updates remain unedited; Linux grows supported API surface.
- Substitution: outputs, errors, ownership, cancellation, and side effects satisfy the same contract.
- Interface segregation: independent brush, raster, effects, segmentation, codec, and platform-service capabilities.
- Dependency inversion: composition selects implementations; business code does not select drivers or codecs.

## 4. Correct the fallback design

“Metal → Vulkan → Skia → OpenCV” describes a preference, not one universal pipeline. Skia itself supports a Vulkan GPU backend; OpenCV is useful for selected image operations, not a general replacement for a canvas or brush-density algorithm. Skia's own backend is the first choice for GPU canvas work ([Skia Vulkan documentation](https://skia.org/docs/user/special/vulkan/)).

| Operation | Accelerated route | Correct fallback |
|---|---|---|
| Brush density/coverage | Existing custom Vulkan compute | Existing C coverage kernel; no Skia/OpenCV detour |
| Canvas, paths, clipping, blends | Skia Ganesh/Vulkan through CGContext's bridge | Skia raster, same API and pixel contract |
| Metal layer effects | Vulkan implementations of upstream effects passes | Existing upstream CG/CI CPU effects; use Skia/OpenCV implementations only after equivalence tests |
| CoreImage filters | A capable Skia GPU or dedicated Vulkan backend | Skia CPU/OpenCV per supported operation, then current CPU evaluator or existing C kernel |
| Object selection/background removal | Qualified segmentation provider; optional GPU inference later | OpenCV/refinement or classical provider, with explicit quality status |
| Text | Skia text shaping/layout and raster drawing | CPU text rendering using the same font and shaping contract |

Do not silently select a numerically different algorithm because its backend is available. Unsupported operation, unavailable backend, device loss, invalid input, cancellation, and allocation failure are distinct outcomes. Invalid input does not trigger another renderer. Cancellation does not restart expensive work.

Every attempt receives immutable inputs and produces private output. Publish only after complete success. On device loss, discard failed output, retire the failed device/resources, and replay the unchanged operation on CPU. Do not retry Vulkan for each tile after a device-wide failure. Device loss needs explicit lifecycle handling ([Vulkan device-loss specification](https://docs.vulkan.org/spec/latest/chapters/devsandqueues.html)).

Record the backend that actually executed, driver/device type, fallback reason, and duration in developer diagnostics. Tests requesting Vulkan must fail if they silently run raster. Software Vulkan validates compute correctness, not hardware speed.

## 5. Ordered implementation batches

Each batch below is a bounded change with an independent acceptance gate. This is a multi-batch migration, not one large rewrite.

### P0 — Establish a truthful, reproducible baseline

1. Pin upstream SHA, tested Linux revision/working-tree state, SDK, compiler, library revisions, drivers, test commands, and exclusions.
2. Preserve the current uncommitted work in a reviewed checkpoint before migration; do not bulk-stage unrelated logs or changes.
3. Record fresh full-suite results; mark the old 1.0.4 feature matrix as historical.
4. Add a machine-readable upstream/source/override/exclusion manifest. Generate inclusion lists and validation from it so Package.swift and CI cannot drift independently.
5. Add a pristine-upstream check against the pinned SHA; also inspect untracked additions under protected paths. Merely running `git diff` does not detect untracked files.
6. Relocate Linux-only kernel contract checks, umbrella header, and failure handler into Linux wrapper/build support. Keep original C signatures and original files intact; validate canonical buffers at adapter boundaries.
7. Add compile probes for consumed override APIs and a declaration-change report against upstream. A source build checks used members; it does not prove every upstream signature still matches.

**Files:** `Package.swift`, `CMakeLists.txt`, proposed `linux/upstream.json`, `scripts/check-upstream-clean.sh`, Linux C adapter support, CI, baseline documentation.

**Gate:** original Swift/C/header/test trees match the pinned upstream; added Linux files are outside them; a deliberately altered upstream fixture fails the guard; a clean checkout builds and runs the recorded baseline.

### P1 — Finish API correctness before moving the application

Brush is complete for the current test cluster. Work through remaining failures by root cause in the shim, with a small failing contract test before each fix:

1. CoreGraphics: alpha, mask polarity, premultiplication, byte order, clipping, blend-mode selection, transparency-layer state, transform orientation and sampling.
2. CoreImage: extent versus crop, edge clamping, linear versus encoded colour, premultiplied output, hue sampling, perspective validity and interpolation.
3. AppKit events: focus transitions, field editor exclusion, window-local monitors, shortcut modifiers, view geometry and responder routing.
4. Text: replace empty layout/drawing methods with real shaping/rasterization; use deterministic bundled test fonts, multiline/Unicode coverage, baseline metrics, clipping and transparent export.
5. UI-related excluded tests: enable those supportable by real wrapper behaviour; add Qt interaction equivalents for intentionally macOS-only presentation tests.

Before calling a failure a Linux defect, run or obtain the same test at the same SHA on macOS. For example, the remaining blend shortcut expects `.colorBurn` after cycling backwards, while the current result is `.luminosity`; the selection exclusion also reports a removed enum case. These are candidates for stale tests, not permission to bend the Linux API to pass an outdated expectation.

Keep original tests unchanged. A confirmed upstream issue gets an explicit SHA-scoped external quarantine, reproducer, owner and expiry/recheck condition. Passing counts must not hide quarantines.

**Gate:** all applicable original tests pass, or a remaining upstream defect has a macOS reproducer; no unexplained newly excluded tests; new wrapper regression tests pass with the intended backend.

### P2 — Move the Linux application onto the original editor

1. Inventory every exported C function and Qt command/state field. Preserve their public contracts during the cutover.
2. Build the thin adapter inside module `Compositor`; map calls to original `EditorSession`, controller, persistence and renderer methods. Move decoding/encoding and handle lifetime support out of the fork without carrying duplicate editing algorithms.
3. Resolve main-actor execution explicitly. The current synchronous, lock-based legacy bridge cannot be transplanted onto an `@MainActor` editor unchanged. Use one serialized editor owner and asynchronous operation/completion tokens for long tasks; make Qt completion handling nonblocking. Prototype the Qt/Swift event-loop integration before porting the whole dispatch table.
4. Establish a vertical slice: create → paint → mask → render → undo/redo → save → reopen → export. Extend to transforms, selections, adjustments, effects, text, object mode and multi-document operations.
5. During qualification, build separate legacy and upstream-backed binaries/tests. Link exactly one owner of the `compositor_session_*` symbols into each process. Do not dual-write one document through two cores.
6. Change `CompositorHostBootstrap` and packaging to link the upstream-backed adapter only after the complete host contract passes.
7. Move reusable brush/backend sources out of the legacy tree: the current upstream target links `BrushCoverage.swift` and `NativeBrushCoverage.swift` from `Sources/CompositorCore`. Remove this dependency before deleting forks.

**Gate:** the installed Qt app demonstrably edits through the original editor; C ABI journey tests, all existing Qt smoke modes, save interoperability, cancellation, late completion after close, and undo boundaries pass. No actor deadlock, UI freeze, or duplicate ABI symbols.

### P3 — Complete the API-backed Qt feature surface

Use upstream's current tool/command definitions as an inventory. Mixed SwiftUI source cannot automatically be reflected into Qt: use a generated, SHA-checked extraction where feasible, and a small audited binding manifest otherwise. Do not duplicate tool business rules in C++.

Bind missing interactions in user-journey order:

1. Layer/mask target selection, linking, clipping, folder opacity, duplication, reorder/nest and merge operations.
2. Selection all/inverse, feather, load alpha/mask, outline movement and pixel movement/duplication.
3. Multi-layer/folder transforms, distortion gestures, snapping, guides and rulers.
4. Re-edit adjustments, sampling/eyedroppers, all filter settings, effects editing and preview/cancel/apply.
5. Type entry and typography, Magic Object mode, background removal.
6. Tabs/multiple projects, cross-project layer drag, clipboard, shortcuts and file/export dialogs.

For each action record: upstream method, bridge entry, Qt control, enabled-state source, shortcut/focus rule, undo behaviour and test. New screens still require Qt presentation work; an API wrapper cannot make an unknown screen appear automatically.

**Gate:** every in-scope macOS 1.1.8 action is reachable in Qt or explicitly classified as platform-specific. Run real pointer/key journeys as well as command tests; headless stand-ins do not qualify UI parity.

### P4 — Implement Metal layer effects behind the existing override

Keep `MetalLayerEffects.shared` and `render(_:effects:)` source-compatible. Reuse the existing CPU result as a correctness baseline while retaining macOS golden images as the oracle.

Port the nine kernels present at the pinned SHA: `effects_alpha`, `effects_spread_rows`, `effects_spread_columns`, `effects_ring`, `effects_shift`, `effects_blur_rows`, `effects_blur_columns`, `effects_inside`, `effects_compose`. Start with alpha/compose, then spread/ring, then shift/blur/inside; verify each group before enabling a combined render.

Create `backends/effects` only for these backend operations. Generalize shader compilation from `scripts/build-brush-shader.py`, with pinned compiler options and checked shader artifacts. Specify buffer strides, uniforms/alignment, barriers, allocation bounds, output extents/insets, and alpha/colour conventions at the C ABI.

Test zero/large radii, inner/outer strokes and shadows, clipped/disabled effects, semitransparent edges, masks, blend modes, non-square images, allocation failure and device loss between passes. Preserve cache correctness when parameters, source pixels, masks or device generation change.

**Gate:** original layer-effects tests pass on forced CPU and forced Vulkan; macOS golden tolerances are documented per operation; failure injection produces one correct result and one undo action. Do not enable an accelerated path until it meets these gates.

### P5 — Make canvas acceleration real

Use the existing Skia bridge rather than creating a second canvas API. Replace `vulkan_render_rgba`'s placeholder with real Ganesh/Vulkan rendering and then route CGContext surfaces through the same backend capability. Completing a small RGBA copy path alone is not GPU canvas parity.

Own device, queue, context, surfaces and cleanup in the backend. Make cache/device generation explicit. Preserve Skia raster as the complete CPU route. First qualify an offscreen GPU render/readback, then integrate presentation with Qt. Defer zero-copy interop until measurement justifies its complexity.

**Gate:** a GPU-required integration test proves an actual drawing operation executed on Vulkan; images agree with raster/macOS fixtures; device-loss injection preserves the document and renders on CPU; Wayland and X11 resize/scale/presentation journeys work. Record hardware GPU and software Vulkan separately.

### P6 — Complete image-processing and segmentation providers

Retain the current CoreImage evaluator as the reference while fixing its semantics. Add accelerated implementations operation by operation behind a capability contract; do not rewrite filter orchestration in the editor. Exercise crop/extent, border treatment, premultiplication, colour space, threading and deterministic errors in every backend.

Wire OpenCV into the existing Vision registration seam, including instance labels, selected-instance masks, image orientation, dimensions and refinement. GrabCut needs initialization and is not equivalent to Apple's learned object segmentation; evaluate it on representative images before claiming parity ([OpenCV GrabCut documentation](https://docs.opencv.org/4.12.0/d8/d83/tutorial_py_grabcut.html)). If the required quality cannot be reached, retain the API and add a qualified model provider rather than rewriting upstream selection logic. Model licensing/distribution and execution requirements become part of that provider's acceptance gate.

**Gate:** original object-selection/background-removal journeys work through the same API; labelled instances remain stable within a result; quality is measured on a fixed corpus; provider failure is explicit and leaves the document unchanged. No claim of full Vision quality from synthetic mask tests alone.

### P7 — Make the release build reproduce the tested app

Align the Flatpak runtime/compiler with the qualification environment. Pin and verify actual source checksums and dependency revisions. Build from a clean tree using shipped dependencies; do not depend on a session-specific `/tmp/claude-1000` library path.

Fix concrete packaging gaps: the current OpenCV `BUILD_LIST=core,imgproc` omits `photo`, used by `cv::inpaint`; add it if that backend remains required. Build/package Skia text/shaping libraries and fonts needed by P1. Align the Skia archive with bridge APIs and verify installed headers/archive layout. Verify HEIF codec/container selection rather than treating an AV1 container with a `.heic` suffix as full HEIC compatibility.

**Gate:** install the built Flatpak in a clean environment, run original tests and Qt smoke journeys with packaged libraries, verify import/export types and project round trips, and test native Wayland/X11/tablet/clipboard/dialog flows. GPU-absent systems must still edit and export correctly.

### P8 — Retire forks and make upstream updates routine

Delete legacy document/rendering implementations only after the upstream-backed application passes all dependent journeys. Keep Linux-specific adapters, backend tests and host tests; migrate their imports without weakening assertions. Remove legacy build targets and source links last.

For each upstream update: fetch → pin candidate SHA → compare protected source/API/command inventory → build original code → run original and wrapper tests → run backend and Qt journeys → publish the parity report for review. Use an isolated checkout for the sync rehearsal; do not automatically merge a failing candidate into the working branch.

**Gate:** one editor implementation ships; no legacy domain imports remain in the app dependency graph; the next upstream change is handled with wrapper/backend/UI changes only. New API semantics and new screens still cost work, but unchanged domain behaviour arrives directly from upstream.

## 6. Validation and failure coverage

```text
Pinned upstream + pristine-source check
              |
         compile original targets
              |
        original tests + API regression tests
              |
 Qt input --> C ABI --> actor owner --> original session
              |                         |
         reject invalid              immutable operation
              |                         |
       no mutation/undo        backend capability selection
                                        |
                         +--------------+--------------+
                         |                             |
                     GPU success               unavailable/device lost
                         |                             |
                         |                    discard + replay CPU
                         +--------------+--------------+
                                        |
                              publish once / one undo
                                        |
                         preview = export = save/reopen
```

| Test lane | Required evidence |
|---|---|
| Upstream source/API | Protected files unchanged; consumed APIs compile; declaration drift reported; overrides/exclusions accounted for. |
| Original behaviour | Original Swift Testing suite at pinned SHA; failures and quarantines listed separately; same-SHA macOS comparison for ambiguous failures. |
| Wrapper contracts | XCTest fixtures for RGBA/gray, alpha, mask, clip, transforms, colour/extent, text, events and codec semantics. |
| Backend equivalence | Force each supported provider; exact equality for structural/integer contracts, documented tolerances for floating rendering; no silent skips in required lanes. |
| Recovery | Initialization failure, shader error, allocation failure, device loss, cancellation, stale handle, window close during async work; no partial mutation or duplicate undo. |
| Host journeys | Existing five smoke modes plus each new interactive feature; focus/text entry, pointer gestures, dialogs, multiple documents. |
| Packaging | Clean Flatpak build/install; actual driver/library versions; CPU-only, software Vulkan, and real GPU runs. |
| Performance | Brush latency, 4K render/filter time, peak memory, cache behaviour and fallback time, with cold/warm and debug/release separated. |

Baseline existing workloads before setting numerical performance thresholds. The full-run diagnostic backtrace in this continuation found the tiled renderer spending time in `Accelerate.vImageResample8` through `DownsampleCache.halve`; this is a concrete profiling target, not evidence of a deadlock. The slow tiled render and selection-invert tests must remain in a full lane; fast local lanes may omit them only with the omission clearly reported. Do not relax upstream timing assertions to hide debug-build differences.

## 7. Critical path, risks and decisions

Critical path: **P0 → P1 → P2 → P3/P7 → P8**. P4/P5 acceleration can proceed after P1 establishes the relevant pixel contracts. P6 provider work follows its API tests and is required before object-selection quality is declared complete. GPU speed must not block validating the shared editor on a correct CPU path, but it remains required for the requested accelerated release.

Highest risks and containment:

1. **Actor/event-loop integration:** prototype one asynchronous host journey before moving the full bridge. A lock is not a main-actor executor.
2. **Pixel semantics:** the Brush bug demonstrated that correct-looking API signatures still produced half the expected opacity. Require numeric tests and macOS fixtures.
3. **Upstream test drift:** verify suspect expectations on macOS; keep quarantines external and SHA-scoped.
4. **False GPU readiness:** backend creation and fallback success are insufficient. Require proof of the backend that executed the actual render.
5. **Text/segmentation fidelity:** fonts and learned-model behaviour are separate quality problems; compile success cannot close them.
6. **Build divergence:** qualify the packaged dependencies, not only local scratch libraries.
7. **Hidden fork dependencies:** extract reusable brush support before deleting the legacy core; enforce dependency direction in CI.

No generic Metal emulator, SwiftUI reimplementation, wholesale renderer rewrite, or second Linux domain model is proposed. Keep the two same-API Metal substitutes and narrow framework wrappers. Additional file-level overrides require a concrete unsupported boundary and a drift test; they must not become copies of editor logic.

## 8. Source evidence for the main gaps

These are observed facts at the reviewed working tree, not inferred completion percentages:

| Evidence | Meaning |
|---|---|
| `Sources/CompositorHostBootstrap/hostMain.swift:19`: `import CompositorCore` | The host still depends on the legacy core. |
| `Sources/CompositorCore/EditorBridge.swift:56`: `session = EditorSession(coverageFactory: coverageFactory)` | The legacy module's editor still owns C ABI sessions. |
| `linux/graphics/SkiaBridge.cpp:469`: `return -2;` in `vulkan_render_rgba` | Canvas Vulkan rendering requests fallback instead of executing. |
| `Sources/Overrides/MetalLayerEffects.swift:13`: `static var shared: MetalLayerEffects?` with no initializer | No accelerated effects instance is installed by this override. |
| `Sources/Compat/AppKit/Text.swift:75`: `drawGlyphs(...) {}` | Type rasterization is a stub. |
| `Sources/Compat/CoreImage/CoreImage.swift:327`: `image.node.eval(region, Env(linear: linearWorkingSpace))` | Current CI execution uses the graph's CPU evaluator. |
| `com.wonderassembly.Compositor.yaml:99`: `-DBUILD_LIST=core,imgproc` | The manifest does not request OpenCV's photo module. |
| `Sources/Overrides/MetalBrushCoverage.swift:7` references `OverrideSignatureTests.swift`, but that path is absent | The comment describes a contract test that has not been delivered. |
| `scripts/check-upstream-clean.sh` | Protected `Compositor` and `CompositorTests` trees now match `upstream/main`; future Linux-only edits are rejected. |

## GSTACK REVIEW REPORT

This document is a source- and test-informed engineering plan, not a completed interactive approval review or a parity certificate. Scope and API-boundary direction come directly from the user's instructions. Architecture, duplication, tests, performance and distribution risks are addressed above; macOS golden results and physical GPU qualification remain external verification requirements. No migration, release, or upstream merge was performed while writing the plan.
