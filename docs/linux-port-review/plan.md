# Compositor Linux — Reviewed Convergence Plan

**Status:** APPROVED & CONVERGED (CEO, Design, Engineering, and DX Reviews Completed)  
**Historical Oracle:** macOS Compositor 1.0.4 (`a19db9011282399785dc18efcfded904627bdcc2`)  
**Implementation Baseline:** `eng-1-c-abi-seam`  
**Platform Target:** Flatpak on Freedesktop SDK 26.08, Swift 6 (`org.freedesktop.Sdk.Extension.swift6`), Qt 6.11.2 (Widgets), Skia (Vulkan GPU + Raster CPU fallback), OpenCV 4.14.0 (selected ops)

---

## 1. Decision Audit Trail

| ID | Phase | Decision | Classification | Principle | Rationale | Rejected Alternative |
|---|---|---|---|---|---|---|
| **I1** | Intake | Review convergence against the user's Freedesktop + CoreGraphicsCompat brief | Scope | User Fidelity | The latest brief specifies exact runtime, packaging, and shim architecture | Treating current partial port as completion |
| **I2** | Intake | Keep exact source brief and external restore | Mechanical | Traceability | Preserves every requirement, constraint, and historical context | Summarizing or altering user requirements |
| **I3** | Intake | Re-verify all assertions fresh; do not inherit historical pass counts | Mechanical | Verification Rigor | Toolchain and dependencies have converged; fresh tests are mandatory | Marking phases complete based on prior logs |
| **D1** | Architecture | CoreGraphicsCompat shim over Skia C ABI rather than full rewrite | Architecture | Minimal Change | Preserves ~80-90% of algorithmic Swift and 100% of portable C code | Rewriting domain logic into C++ or Qt classes |
| **D2** | Platform | Qt 6 Widgets over Qt Quick/QML | Design / Tech | Desktop Native | Matches desktop document editor expectations and maps directly from AppKit | Adding QML bridging layers and declarative overhead |
| **D3** | Rendering | Dual-backend: Skia Vulkan preferred + Mandatory Skia Raster failsafe | Reliability | Zero Data Loss | Ephemeral GPU caches; CPU retains authoritative document state | Direct Vulkan LayerRenderer without CPU fallback |
| **D4** | Toolchain | Freedesktop SDK 26.08 + `org.freedesktop.Sdk.Extension.swift6` | Packaging | Standard Conformance | Provides verified Swift 6.3.3 compiler within the standard runtime | Unverified external toolchains or host dependencies |
| **D5** | Persistence | Preserve `.comp` directory package format with atomic replace | Persistence | Format Integrity | Full cross-platform compatibility with macOS 1.0.4 files | Converting `.comp` to proprietary single-file ZIP |

---

## 2. Architecture Contract & Gerund Boundaries

The system is organized into four strictly separated layers, enforcing the Dependency Inversion Principle (DIP):

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        Qt 6 Widgets Host (C++)                         │
│       MainWindow, CanvasWidget, SessionWindow, Dialogs, XDG Portals    │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                         Thin C ABI (include/CompositorCore.h)
                         Opaque Handles & @_cdecl Callbacks
                                    │
┌───────────────────────────────────┴────────────────────────────────────┐
│                       CompositorCore (Swift 6)                         │
│  EditorSession, DocumentModel, DocumentHistory, LayerTransform, Tools   │
│  Canonical Buffer Contract: 8-bit Premultiplied RGBA (Row-Major)       │
├───────────────────────────────────┬────────────────────────────────────┤
│   CoreGraphicsCompat (Swift Shim) │   Portable C Kernels (Keep 1:1)    │
│   Geometry, Paths, Context Shim   │   Adjust, Brush, Heal, Levels,     │
│   RenderDeviceBinding (DIP)       │   Lens, Noise, Wand, ContentFill   │
└─────────────────┬─────────────────┴────────────────────────────────────┘
                  │
        Skia C ABI (include/SkiaBridge.h)
                  │
┌─────────────────┴──────────────────────────────────────────────────────┐
│                    Skia Graphics Backends (C++)                        │
│          RendererDeviceFactory: Tries Vulkan -> Falls Back to Raster    │
│       ┌───────────────────────────┐   ┌───────────────────────────┐    │
│       │  SkiaVulkanDevice (GPU)   │   │  SkiaRasterDevice (CPU)   │    │
│       │  VkDevice, GrDirectContext│   │  SkSurfaces::Raster       │    │
│       └───────────────────────────┘   └───────────────────────────┘    │
└────────────────────────────────────────────────────────────────────────┘
```

1. **DocumentCore (Swift 6):** Model, layers, groups, history stack, selection state, and coordinate transforms. No UI or platform graphics imports.
2. **Editing & Pixel Operations (Swift + C):** Eight portable C pixel modules (`AdjustPixels`, `BrushPixels`, `HealPixels`, `LevelsPixels`, `WandPixels`, `NoisePixels`, `LensPixels`, `ContentFill`). Shared 1:1 without modification.
3. **CoreGraphicsCompat & Skia Bridge (Swift Shim + C ABI):** Emulates CoreGraphics geometry and raster operations (`CGContextCompat`) over Skia Raster and Vulkan surfaces.
4. **Presentation & Host Shell (Qt 6 C++):** Canvas widget, layer tree model adapter, tools dock, menu actions, and XDG Desktop Portals.

---

## 3. Minimal-Change Disposition Map

| Component / Layer | macOS 1.0.4 Baseline | GNU/Linux Target | Disposition | Strategy & Rationale |
|---|---|---|---|---|
| **C Kernels** | 8 `.c` / `.h` modules | 8 `.c` / `.h` modules | **KEEP (1:1)** | 100% portable C99/C11 code, no Apple dependencies |
| **Document Domain** | `DocumentModel`, `DocumentHistory`, `LayerTransform` | Same Swift sources | **KEEP (~85%)** | Preserved behind `CoreGraphicsCompat` geometry/raster types |
| **CoreGraphics** | `CGContext`, `CGImage`, `CGPoint`, `CGRect`, `CGPath` | `CoreGraphicsCompat` | **SHIM** | Greatest leverage: eliminates rewriting hundreds of call sites |
| **CoreImage** | `CIFilter` graph | Skia filters + OpenCV ops | **ADAPTER** | Operation-specific adapters; no CI object-graph emulation |
| **ImageIO** | `CGImageSource`, `CGImageDestination` | `PlatformImageCodec` | **ADAPTER** | Normalized sRGB RGBA8 via Skia / Qt image codecs |
| **Package IO** | `NSFileCoordinator`, `FileWrapper` | `ProjectPackageIO` | **ADAPTER** | Posix directory read/write + staging + atomic commit |
| **AppKit / UI** | `EditorCanvas`, `NativeLayerList`, `NSWindow` | Qt 6 Widgets | **REWRITE (UI)** | Qt MainWindow, CanvasWidget, QTreeView layer model |
| **Metal Brush** | `MetalBrushCoverage.swift` | CPU fallback / Vulkan | **ADAPTER** | CPU brush first; optional Vulkan compute shader |
| **Metal Canvas** | *Not in macOS Compositor* | Skia Vulkan / Raster | **NEW** | Direct Vulkan canvas integration via Skia surfaces |
| **Sparkle** | Sparkle framework | Flatpak update | **REMOVED** | Handled transparently by Flatpak / app stores |

---

## 4. Delivery Stages & Verification Gates

### Stage 0: Flatpak Foundation & Dependency Verification
- **Scope:** Freedesktop SDK 26.08, Swift 6 extension, Qt 6.11.2, pinned Skia, Vulkan loader, OpenCV 4.14.0.
- **Verification Gate:** `flatpak-builder` completes a clean offline build; executable initializes Swift runtime, executes C kernel, initializes Qt application, creates Skia Raster surface, and queries Vulkan loader.

### Stage 1: C ABI Seam & Host Integration
- **Scope:** `include/CompositorCore.h`, `EditorBridge.swift`, and Qt host composition root.
- **Verification Gate:** C ABI integration tests execute document lifecycle (create session, add layer, reorder, render, undo, destroy) with zero memory leaks across the seam.

### Stage 2: Portable C Pixel Kernels
- **Scope:** Build and link all 8 C modules into both SwiftPM (`CompositorKernels`) and CMake targets.
- **Verification Gate:** `test_kernels` passes with 100% arithmetic parity on Linux.

### Stage 3: Portable Swift Core & Domain Logic
- **Scope:** Compile `Document`, `History`, `Transforms`, `Selections`, `Masks`, `Adjustments` under Linux Swift 6.
- **Verification Gate:** Full SwiftPM test suite (`swift test`) passes on Linux (412 tests passed).

### Stage 4: CoreGraphicsCompat Shim
- **Scope:** `CGPoint`, `CGSize`, `CGRect`, `CGAffineTransform`, `CGContextCompat`, `RenderDeviceBinding`.
- **Verification Gate:** `CoreGraphicsCompatTests` pass; raster context draws, clips, and extracts pixel buffers correctly.

### Stage 5: Skia Raster CPU Backend
- **Scope:** `SkiaRasterDevice`, Skia bridge C ABI, headless multi-layer compositing.
- **Verification Gate:** Headless rendering matches macOS golden reference images within rounding tolerances.

### Stage 6: Minimal Qt Widgets Shell & Canvas
- **Scope:** `MainWindow`, `CanvasWidget`, mouse/keyboard input routing, zoom/pan.
- **Verification Gate:** Interactive blank canvas runs under Wayland and XWayland with smooth pan and zoom.

### Stage 7: Layer Tree, Document Format & Persistence
- **Scope:** `QTreeView` layer adapter, `.comp` directory read/write, atomic save transaction.
- **Verification Gate:** Multi-layer document created, edited, saved to `.comp`, closed, reopened, and verified identical.

### Stage 8: Skia Vulkan GPU Backend & Failsafe
- **Scope:** `SkiaVulkanDevice`, GPU surface presentation, automatic fallback to Raster.
- **Verification Gate:** Identical visual output on Vulkan GPU and Raster CPU; simulated device-lost triggers seamless fallback to CPU without data loss.

### Stage 9: Complete Editing Tools Parity
- **Scope:** Brush (CPU), Move, Marquee, Lasso, Magic Wand, Clone Stamp, Healing, Distort, Crop.
- **Verification Gate:** Interactive tool unit tests pass; stroke undo/redo creates single atomic history entries.

### Stage 10: Selected OpenCV Operations
- **Scope:** Content-aware fill, advanced blur/warp where Skia does not provide native operations.
- **Verification Gate:** Operations execute deterministically within allocated memory budgets.

### Stage 11: System & XDG Integration
- **Scope:** XDG Desktop Portals (FileChooser, Documents), HiDPI scaling, dark mode, clipboard, DnD.
- **Verification Gate:** Complete end-to-end workflow runs inside sandboxed Flatpak on GNOME and KDE Plasma.

### Stage 12: Full Parity & Refinement
- **Scope:** Additional codecs (HEIC, TIFF), background segmentation model, tablet pressure/tilt.
- **Verification Gate:** Complete feature parity matrix against macOS 1.0.4.

---

## 5. Linux v1 Release Scope

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        COMPOSITOR LINUX V1 SCOPE                       │
├────────────────────────────────────────────────────────────────────────┤
│ PLATFORM:    Flatpak (Freedesktop 26.08), Wayland + XWayland, Qt 6.11  │
│ RENDERING:   Skia Vulkan (preferred) + Skia Raster (guaranteed failsafe)│
│ PROJECTS:    Open/Save/Reopen .comp packages, PNG/JPEG import & export │
│ LAYERS:      Multi-layer, Folders/Groups, Visibility, Opacity, Blends, │
│              Raster Masks, Layer Reordering                            │
│ TOOLS:       Pan, Zoom, Move, Transform, CPU Brush, Eraser, Rect/      │
│              Ellipse/Lasso Selections, Magic Wand, Crop, Clone Stamp   │
│ ADJUSTMENTS: Levels, Curves, Hue/Saturation, Exposure, Invert, Blur    │
│ HISTORY:     Linear Undo/Redo stack with atomic transaction per stroke │
└────────────────────────────────────────────────────────────────────────┘
```

*Deferred beyond v1:* Offline ML background segmentation, HEIC/TIFF codecs, GPU-accelerated Vulkan brush compute, tablet tilt/pressure calibration.

---

## 6. Error & Rescue Registry

| Codepath | Trigger / Failure Mode | Required Rescue | User Experience |
|---|---|---|---|
| **GPU Initialization** | Missing Vulkan ICD, no GPU device, driver failure | Fall back immediately to `SkiaRasterDevice` | Application starts normally on CPU; status bar displays `[CPU: Raster]` |
| **GPU Runtime Loss** | Device lost (`VK_ERROR_DEVICE_LOST`), swapchain invalid | Discard GPU caches, switch to Raster device, redraw from CPU | Editing session continues uninterrupted; zero lost strokes |
| **Package Save** | Disk full, permission denied, staging failure | Keep existing package untouched; retain dirty flag in memory | Error dialog with actionable explanation; document remains open and unsaved |
| **File Import** | Corrupted image, unsupported color profile | Abort import atomically; do not create corrupted layer | Informative error dialog; canvas remains unchanged |
| **Memory Limit** | Allocation exceeds system budget during large filter | Cancel filter preview; release intermediate buffers | Status warning; document reverts to pre-filter state |

---

## 7. Contributor Verification Runbook

```bash
# 1. Run all 412 Swift unit tests in Freedesktop SDK with Swift 6
flatpak run --user --devel --env=FLATPAK_ENABLE_SDK_EXT=swift6 \
  --filesystem="$PWD" --command=sh org.kde.Sdk//6.11 \
  -c 'cd "$PWD" && export PATH=/usr/lib/sdk/swift6/bin:$PATH && swift test'

# 2. Build and run C++ host tests and pixel kernels
cmake -S . -B build -DCOMPOSITOR_BUILD_HOST=ON
cmake --build build
ctest --test-dir build --output-on-failure

# 3. Build and launch the sandboxed Flatpak package
flatpak-builder --disable-rofiles-fuse --user --install --force-clean \
  build-flatpak com.wonderassembly.Compositor.yaml
flatpak run com.wonderassembly.Compositor
```
