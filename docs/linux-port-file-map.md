# Linux port: file-by-file disposition

Baseline: a19db90, 2026-09-19. Derived from the code graph, tracked-source inventory, import audit and detailed inspection of critical boundaries. Disposition is a planning classification, not a claim that every function was ported or proven equivalent. `Keep` means reuse intent and implementation where compatible; indirect Apple dependencies must still be tested. Original macOS files remain supported. Targets describe responsibilities, not one-for-one Apple API emulation.

## Application

| Source | Lines* | Direct imports | Disposition | Linux responsibility |
|---|---:|---|---|---|
| `Compositor/Compositor-Bridging-Header.h` | 9 | C/header | Apple replacement / adaptation | Replace Linux build boundary with module maps and narrow C ABI; keep Xcode header |
| `Compositor/CompositorApp.swift` | 271 | SwiftUI, Sparkle | Apple replacement / adaptation | Rewrite entry point and actions in Qt; retain macOS app |
| `Compositor/ContentView.swift` | 387 | SwiftUI, UniformTypeIdentifiers | Apple replacement / adaptation | Rewrite workspace layout in Qt Widgets |
| `Compositor/Document/AdjustmentEditing.swift` | 111 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/BlurTool.swift` | 42 | AppKit, CoreImage | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/BrushStroke.swift` | 900 | AppKit | Apple replacement / adaptation | Keep sampling/smoothing and tile algorithm; replace CGContext/CGImage storage and paint operations |
| `Compositor/Document/CanvasSize.swift` | 83 | Foundation, CoreGraphics | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/CloneStamp.swift` | 50 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/ColorPalette.swift` | 217 | AppKit, Observation | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/ContentFill.swift` | 28 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/Crop.swift` | 200 | Foundation, CoreGraphics | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/Curves.swift` | 43 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/Distort.swift` | 292 | AppKit, CoreImage | Apple replacement / adaptation | Keep transform intent; replace CI perspective/filter backend, verify sampling |
| `Compositor/Document/DocumentHistory.swift` | 120 | Foundation, CoreGraphics, Observation | Apple replacement / adaptation | Keep immutable history semantics; replace snapshot image/geometry dependencies |
| `Compositor/Document/EditorSession+Brush.swift` | 202 | AppKit | Apple replacement / adaptation | Keep stroke orchestration; port image/mask handoff |
| `Compositor/Document/EditorSession+Projects.swift` | 56 | Foundation | Apple replacement / adaptation | Keep manifest mapping; replace image asset adapters |
| `Compositor/Document/EditorSession.swift` | 690 | SwiftUI | Apple replacement / adaptation | Extract Swift commands/state; remove SwiftUI and platform image types |
| `Compositor/Document/Filters.swift` | 474 | AppKit, CoreImage, Observation | Apple replacement / adaptation | Keep settings/validation/preview transactions; replace named CoreImage filters with Skia/OpenCV/custom kernels |
| `Compositor/Document/FloatingSelection.swift` | 160 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/Gradient.swift` | 110 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/GuidedMatte.swift` | 119 | CoreGraphics, Foundation | Apple replacement / adaptation | Keep refinement algorithm; replace CoreGraphics buffer adapters |
| `Compositor/Document/HueSaturation.swift` | 575 | AppKit, CoreImage | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/ImageAdjustments.swift` | 135 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/LayerAdjustment.swift` | 113 | AppKit, CoreImage | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/LayerAppearance.swift` | 89 | Foundation, CoreGraphics | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/LayerFlip.swift` | 80 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/LayerGroups.swift` | 191 | Foundation | Keep/adapt | Keep hierarchy validation/traversal; verify any indirect model dependencies |
| `Compositor/Document/LayerMask.swift` | 420 | Foundation, CoreGraphics | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/LayerMerge.swift` | 73 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/LayerTransform.swift` | 236 | CoreGraphics, Foundation | Apple replacement / adaptation | Keep transformation math and serialization; replace graphics types/paths as needed |
| `Compositor/Document/Levels.swift` | 229 | AppKit, Observation | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/LevelsAutomatic.swift` | 85 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/LiveLayerMask.swift` | 231 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/MagicWand.swift` | 139 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/MaskTracing.swift` | 93 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/PixelAdjust.swift` | 66 | AppKit, CoreImage | Apple replacement / adaptation | Replace CoreImage rendering/context with named imaging operations |
| `Compositor/Document/PixelInvert.swift` | 47 | AppKit, Accelerate, CoreImage | Apple replacement / adaptation | Keep numerical intent; replace Accelerate/CoreImage image operations |
| `Compositor/Document/ProjectWorkspace.swift` | 211 | AppKit, Observation, UniformTypeIdentifiers | Apple replacement / adaptation | Keep tab/document identity and dirty-state semantics; replace AppKit integration |
| `Compositor/Document/Selection.swift` | 311 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/SelectionClipboard.swift` | 205 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/SelectionEdits.swift` | 206 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/ShapeTool.swift` | 158 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/SmudgeLiquify.swift` | 211 | AppKit | Keep logic; replace Apple operations | Retain Swift settings/math/commands; substitute portable geometry/pixels and Skia/OpenCV operations where used |
| `Compositor/Document/SubjectRemoval.swift` | 111 | Vision, CoreImage | Apple replacement / adaptation | Replace Vision inference with validated offline segmentation model/runtime; preserve refinement workflow |
| `Compositor/IO/CanvasResizer.swift` | 73 | Foundation, CoreGraphics | Apple replacement / adaptation | Keep placement semantics; replace raster construction |
| `Compositor/IO/CompositorApplicationDelegate.swift` | 44 | AppKit, Sparkle | Apple replacement / adaptation | Keep macOS-only; Qt entry and Flatpak updates replace Linux responsibilities |
| `Compositor/IO/ImageExporter.swift` | 147 | Foundation, CoreGraphics, ImageIO, UniformTypeIdentifiers | Apple replacement / adaptation | Replace ImageIO encoder; keep flattened export and resolution behavior |
| `Compositor/IO/ImageFileDrop.swift` | 56 | SwiftUI, UniformTypeIdentifiers | Apple replacement / adaptation | Rewrite with Qt MIME/drop events and scoped document access |
| `Compositor/IO/ImageImporter.swift` | 64 | Foundation, CoreGraphics, CoreImage, ImageIO, UniformTypeIdentifiers | Apple replacement / adaptation | Replace Apple decoding/color conversion; bounded codec adapter to canonical pixels |
| `Compositor/IO/ImageResizer.swift` | 118 | Foundation, CoreGraphics | Apple replacement / adaptation | Keep sizing semantics; replace resampling backend |
| `Compositor/IO/ProjectController.swift` | 300 | AppKit, UniformTypeIdentifiers, SwiftUI | Apple replacement / adaptation | Rewrite dialogs, close/save coordination and open-file integration through Qt/portals |
| `Compositor/IO/ProjectStore.swift` | 226 | Foundation, CoreGraphics, ImageIO, UniformTypeIdentifiers | Apple replacement / adaptation | Keep Codable schema and validation; replace PNG and coordinated directory persistence |
| `Compositor/Rendering/AdjustPixels.c` | 97 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/AdjustPixels.h` | 20 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/AdjustmentSurface.swift` | 18 | CoreGraphics | Apple replacement / adaptation | Keep affected-region semantics; replace CGContext surfaces |
| `Compositor/Rendering/BrushCursorOverlay.swift` | 96 | AppKit | Apple replacement / adaptation | Rewrite using Qt cursor/overlay painting |
| `Compositor/Rendering/BrushPixels.c` | 51 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/BrushPixels.h` | 11 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/CanvasViewport.swift` | 73 | Foundation, CoreGraphics | Apple replacement / adaptation | Keep geometry; remove CoreGraphics import/use portable value types |
| `Compositor/Rendering/ContentFill.c` | 94 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/ContentFill.h` | 5 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/DownsampleCache.swift` | 113 | Accelerate, CoreGraphics, Foundation | Apple replacement / adaptation | Keep cache policy; replace vImage resampling and CGImage storage |
| `Compositor/Rendering/EditorCanvas.swift` | 1815 | AppKit, SwiftUI | Apple replacement / adaptation | Rewrite AppKit canvas/event bridge with Qt; preserve interaction behavior via journey tests |
| `Compositor/Rendering/HealPixels.c` | 260 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/HealPixels.h` | 20 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/LayerRenderer.swift` | 174 | CoreGraphics | Apple replacement / adaptation | Replace CoreGraphics compositing with Skia and exact custom blend kernels |
| `Compositor/Rendering/LensPixels.c` | 38 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/LensPixels.h` | 13 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/LevelsPixels.c` | 29 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/LevelsPixels.h` | 5 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/LiveMaskRenderer.swift` | 146 | Foundation, CoreGraphics, CoreImage | Apple replacement / adaptation | Keep mask graph semantics; replace CoreGraphics/CoreImage operations |
| `Compositor/Rendering/MetalBrushCoverage.swift` | 163 | AppKit, Metal | Metal→Vulkan | Metal→Vulkan Compute on Linux; retain Metal on macOS and implement portable CPU fallback |
| `Compositor/Rendering/NoisePixels.c` | 42 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/NoisePixels.h` | 13 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/RasterSnapshot.swift` | 177 | AppKit | Apple replacement / adaptation | Keep immutable tile replacement/lazy materialization; implement portable buffers |
| `Compositor/Rendering/SampleRingOverlay.swift` | 30 | AppKit | Apple replacement / adaptation | Rewrite using Qt overlay painting |
| `Compositor/Rendering/SeparableBlend.swift` | 43 | CoreGraphics, CoreImage | Apple replacement / adaptation | Keep blend formulas; replace CoreImage/CoreGraphics integration |
| `Compositor/Rendering/TiledLayerRenderer.swift` | 420 | CoreGraphics, Foundation | Apple replacement / adaptation | Keep tile selection/alignments; replace CoreGraphics drawing with Skia |
| `Compositor/Rendering/TransformOverlay.swift` | 328 | AppKit | Apple replacement / adaptation | Rewrite Qt handles/guides; retain transform math |
| `Compositor/Rendering/WandPixels.c` | 175 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/Rendering/WandPixels.h` | 22 | C/header | Keep | Reuse C kernel/interface; run cross-platform pixel tests and sanitizer checks |
| `Compositor/UI/BlendModePicker.swift` | 58 | SwiftUI, AppKit | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/BrushControls.swift` | 121 | SwiftUI, AppKit | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/CanvasSizeSheet.swift` | 114 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/CanvasThumbnail.swift` | 80 | AppKit | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/ColorPaletteControls.swift` | 80 | SwiftUI, AppKit | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/ColorPickerSheet.swift` | 186 | SwiftUI, AppKit | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/CropControls.swift` | 25 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/CurvesControls.swift` | 70 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/FilterSheet.swift` | 164 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/FloatingPanel.swift` | 102 | AppKit, SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/GradientControls.swift` | 104 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/HueSaturationSheet.swift` | 194 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/ImageSizeSheet.swift` | 121 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/JPEGExportSheet.swift` | 88 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/LassoControls.swift` | 129 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/LayerAppearanceControls.swift` | 55 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/LayerMaskMenu.swift` | 15 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/LayersPanel.swift` | 77 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/LevelsSheet.swift` | 145 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/NativeLayerList.swift` | 897 | AppKit, SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/NavigationToolHeader.swift` | 65 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/NewCanvasSheet.swift` | 88 | SwiftUI, AppKit, ImageIO | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/ProjectTabs.swift` | 179 | SwiftUI, UniformTypeIdentifiers, AppKit, Combine | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/ProjectWindowBridge.swift` | 63 | AppKit, SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/ShapeControls.swift` | 50 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/SliderSnap.swift` | 43 | AppKit, ObjectiveC | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/ToolHeaderStyle.swift` | 27 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |
| `Compositor/UI/TransformInspector.swift` | 113 | SwiftUI | Rewrite Linux UI | Qt Widgets counterpart; preserve controls, shortcuts, cancel/commit and accessibility; keep macOS view |

## Tests

| Source | Lines* | Direct imports | Disposition | Linux responsibility |
|---|---:|---|---|---|
| `CompositorTests/AdjustmentLayerTests.swift` | 215 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/BlendShortcutTests.swift` | 43 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/BrushIntersectionTests.swift` | 108 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/BrushPerformanceTests.swift` | 61 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/BrushTests.swift` | 457 | AppKit, SwiftUI, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/CanvasEntryTests.swift` | 52 | AppKit, Testing, UniformTypeIdentifiers | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/CanvasSizeTests.swift` | 108 | AppKit, Testing, UniformTypeIdentifiers | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/CanvasThumbnailTests.swift` | 67 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/CloneStampTests.swift` | 79 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/ColorPickerTests.swift` | 87 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/CompositorTests.swift` | 84 | Testing, CoreGraphics | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/CropTests.swift` | 167 | AppKit, Testing, UniformTypeIdentifiers | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/CursorTests.swift` | 200 | AppKit, SwiftUI, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/DistortTests.swift` | 89 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/DownsampleTests.swift` | 82 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/ExportTests.swift` | 77 | AppKit, ImageIO, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/FilterTests.swift` | 118 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/FloatingPanelTests.swift` | 78 | AppKit, SwiftUI, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/GradientTests.swift` | 154 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/GroupTests.swift` | 110 | AppKit, Testing, UniformTypeIdentifiers | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/GroupingSelectionTests.swift` | 71 | Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/HistoryTests.swift` | 160 | AppKit, UniformTypeIdentifiers, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/HueSaturationTests.swift` | 274 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/ImageAdjustmentTests.swift` | 192 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/ImageImportTests.swift` | 144 | Foundation, CoreGraphics, ImageIO, UniformTypeIdentifiers, AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/ImageSizeTests.swift` | 83 | AppKit, ImageIO, UniformTypeIdentifiers, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/JPEGExportTests.swift` | 64 | AppKit, ImageIO, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/LayerAppearanceTests.swift` | 134 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/LayerMaskTests.swift` | 305 | AppKit, ImageIO, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/LayerTests.swift` | 244 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/LevelsTests.swift` | 200 | AppKit, SwiftUI, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/LiveMaskTests.swift` | 131 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/MagicWandTests.swift` | 148 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/MaskTransformTests.swift` | 135 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/ProjectTests.swift` | 182 | AppKit, UniformTypeIdentifiers, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/ProjectWorkspaceTests.swift` | 106 | AppKit, UniformTypeIdentifiers, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/RasterSnapshotTests.swift` | 136 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/SelectionClipboardTests.swift` | 218 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/SelectionEditTests.swift` | 411 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/SelectionTests.swift` | 468 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/ShapeToolTests.swift` | 106 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/SliderSnapTests.swift` | 68 | AppKit, Combine, SwiftUI, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/SmartEditTests.swift` | 82 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/SpotHealingTests.swift` | 62 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/TiledLayerTests.swift` | 297 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/TransformPressTests.swift` | 60 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorTests/TransformTests.swift` | 305 | AppKit, Testing | Keep assertions; port fixtures | Swift Testing for core; Qt Test for interactions; replace Apple image/window fixtures |
| `CompositorUITests/CompositorUITests.swift` | 54 | XCTest | Rewrite harness | Qt UI automation; preserve launch/journey intentions |
| `CompositorUITests/CompositorUITestsLaunchTests.swift` | 36 | XCTest | Rewrite harness | Qt UI automation; preserve launch/journey intentions |

*Line counts use newline-delimited segments, including the trailing empty segment where present. All 158 tracked Swift/C/header files are listed: 109 application files and 49 test files. Application source is approximately 17,751 such lines. Counts are scale indicators, not rewrite estimates.

## Build, distribution and resources

| Existing artifact | Disposition |
|---|---|
| Compositor.xcodeproj and Config/Info.plist/Compositor.entitlements | Keep macOS build; add independent Linux CMake + Swift package and Flatpak manifest |
| Assets.xcassets PNG icons | Reuse source images; install Linux icon sizes and desktop/AppStream metadata |
| scripts/release.sh, scripts/publish.sh, scripts/ExportOptions.plist, scripts/dmg/, appcast.xml | Keep macOS release route; add Linux bundle build and artifact validation, not Sparkle on Linux |
| docs/project-format.md | Update to actual v7 plus mask-placement and shape fields; derive schema from code and round-trip fixtures |
| docs/brush-performance.md | Keep macOS baseline history; append Linux hardware-specific CPU/Vulkan measurements |
| docs/references/ | Use existing adjustment/canvas-size visuals as behavioral references |
| README.md, LICENSE, .gitignore | Add Linux build/run instructions, dependency notices and isolated build outputs; preserve MIT project license |

## Effort shape

These are preliminary engineering ranges, not measured delivery promises. Human estimates assume one experienced graphics/desktop engineer; assisted ranges assume sustained agent work plus that engineer's review and access to macOS reference execution. Parallel work overlaps; totals are not sums.

| Workstream | Human effort | Agent-assisted active engineering | Main uncertainty |
|---|---|---|---|
| First packaged open/paint/undo/save/reopen/export journey | 2–4 weeks | 4–10 days | Swift/Qt event loop, C ABI and directory permissions |
| Portable model/history/schema and raster contracts | 2–5 weeks | 5–12 days | Indirect Apple dependencies and isolation semantics |
| Qt shell and complete interaction parity | 3–6 weeks | 1–3 weeks | Canvas gestures, shortcut conflicts, layers drag/drop |
| Skia compositing, masks, color, filters, codecs | 5–10 weeks | 2–5 weeks | Pixel parity, resampling and codec metadata |
| Vulkan brush with portable CPU fallback | 1–3 weeks | 3–8 days | Synchronization, opacity integration and device loss |
| Offline background removal | 1–3 weeks | 3–10 days | Model redistribution, inference and quality acceptance |
| Cross-platform fixtures, QA, performance and packaging | 3–6 weeks | 1–3 weeks | macOS access, diverse drivers and large documents |

Overall order of magnitude: 3–6 engineer-months for a dependable full port, or roughly 5–10 calendar weeks of closely supervised assisted work with continuous verification. Re-estimate after the first working journey. Vulkan is not the dominant workstream. No runtime integration, pixel equivalence or release timing has been demonstrated yet.

