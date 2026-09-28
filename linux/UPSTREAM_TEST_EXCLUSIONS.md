# Upstream tests not run on Linux

Upstream's `CompositorTests/*.swift` are compiled **unmodified** (target `CompositorUpstreamTests`). A test file may only be
excluded here, with a reason; individual tests are never edited.

| File | Reason |
|---|---|
| `CameraRawSliderTests.swift` | Tests AppKit `GradientSliderCell`/`CameraRawSliderView` and native knob hit-testing, which the Linux slider adapter does not implement. Camera Raw image-processing tests remain included. |
| `SliderSnapTests.swift` | Tests `UI/SliderSnap.swift`, which swizzles `NSSliderCell` through the Objective-C runtime |
| `FloatingPanelTests.swift` | Tests `UI/FloatingPanel.swift` (`NSPanel` controller) |
| `CanvasThumbnailTests.swift` | Tests `UI/CanvasThumbnail.swift` (SwiftUI view) |
| `LayerTests.swift` | Drives `NativeLayerList`/`LayerTableView` (`NSTableView` subclass counting delegate calls) |
| `CursorTests.swift` | Builds `LayersPanel` + `LayerTableView` to check cursor routing through the AppKit responder chain |
| `GuideTests.swift` | Tests `CanvasRulerNSView` label/step layout (UI/CanvasRulers.swift) |
| `LevelsTests.swift` | Renders `LevelsSheet` (SwiftUI) through `NSHostingView` |
| `CanvasEntryTests.swift` | Tests `NewCanvasSheet.clipboardDimensions` and `NSPasteboard.withUniqueName` |
| `ColorPickerTests.swift` | Tests `ColorPickerPanelController` windows |
| `SelectionTests.swift` | Upstream bug: references `NavigationTool.objectSelection`, which upstream's own `EditorSession` no longer has (removed when Object mode merged into the Magic tool, 376e02a), so the file does not compile even on macOS at this commit |
| `BlendShortcutTests.swift` | Upstream regression: asserts wrapping backward from Normal lands on `.colorBurn` (the historical end of `LayerBlendMode` in v1.0.4), but upstream expanded `LayerBlendMode` with `.hue`, `.saturation`, `.color`, `.luminosity` without updating this test |
| `TransformPressTests.swift` | Upstream test oversight: moving by (20, 10) in a 400x300 canvas shifts layer center to (220, 160); with canvas snapping tolerance `10 / pointsPerPixel = 14.7`, `abs(160 - 150) <= 14.7` snaps midY back to canvas center (150), moving `origin.y` from 110 back to 100. |

## Per-test skips (files stay compiled; pass these to `swift test --skip`)

| Test | Reason (verified by reading upstream's logic; nothing platform-specific is involved) |
|---|---|
| `gaussianBlurSoftensAHardEdgeWithoutFadingTheBordersAsOneUndoStep` (`FilterTests`) | Stale upstream test. `Filters.swift` blurs unclamped and the layer is first trimmed to its visible pixels, so the result grows by the blur margin (a 40x20 layer with a 20x20 opaque half becomes 36x36) and its edge fades. The test still expects the pre-change "border stays 255" behaviour. The shim's profile is a correct sigma-3 Gaussian (row peak 0.8 two pixels inside the edge, symmetric, ~0 at the ends). |
| `moveToolNumberKeysSetSelectedLayersOpacityAsOneUndo` (`LayerAppearanceTests`) | Stale upstream test. `setSelectedLayersOpacity` sets every id in `selectedLayerIDs`, and the test selects the folder explicitly, so the folder becomes 0.5; the test expects 1 (pre-folder-opacity, commit `391042d`). The other 7 tests in these two files pass. |
| `perspectiveMappingHitsTheCornersAndTwistedShapesAreRefused` (`DistortTests`) | Stale upstream test. `DistortWarp.isUsable` documents that a folded shape (bow-tie) is now accepted and "warped as two triangles instead" (`Distort.swift`, `isUsable`/`isConvex`); the test still expects bow-ties refused. Pure geometry, no platform involvement. |
| `distortingWarpsTheLayerIntoTheShapeAsOneUndoStep` (`DistortTests`) | Same cause: it previews a twisted shape and expects it ignored (`corners[1] == (30, 10)`), but twisted shapes are now valid input. The other two Distort tests pass. |


## Runtime platform exclusion in the release runner

`BrushTests.bracketKeysReachTheBrushWhereverFocusIsExceptTextFields` requires a
real `NSTableView` inside `NSHostingView<LayersPanel>`. The Linux host uses Qt.
`scripts/linux-release-check.py` skips only that test and runs `--ui-smoke` against
the actual Qt layer tree and text editor, including keyboard delivery, pointer
pixels, focus preservation and window teardown. All other BrushTests run.
