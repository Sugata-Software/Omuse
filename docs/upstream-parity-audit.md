# Upstream parity audit — GNU_Linux vs upstream macOS (1.1.8)

Generated 2026-09-21 against `upstream/main` (c39da13). Method and honest verdict below.

## Reproducible audit commands

The old table below is a historical approximate name audit. Generate the current
upstream declaration inventory from the pinned branch with:

```sh
python3 scripts/upstream-audit.py --ref upstream/main \
  --json /tmp/compositor-upstream-audit.json \
  --csv /tmp/compositor-upstream-audit.csv
scripts/check-upstream-clean.sh
```

The generated inventory separates UI from document/data/logic declarations and
marks name matches as candidates only. A candidate is not functional parity until
the implementation mapping, shipped workflow and test evidence are recorded.
`check-upstream-clean.sh` now passes: the Linux-only buffer assertion translation
unit and umbrella header were removed from the protected upstream tree. The
remaining Linux kernel contract work belongs at Linux adapter boundaries and must
not be added back to `Compositor/Rendering`.

## Verdict

**The port is NOT at 100% of upstream.** The Linux core reproduces most of the document/data logic; the Qt UI reproduces the core editing workflow but not the newer upstream features (1.1 – 1.1.8) or several editing tools.

## 1. Symbol audit (types + functions, by name)

Every `func`/`struct`/`class`/`enum`/`protocol`/`actor` declared in upstream `Compositor/**/*.swift` was looked up by name in the port (`Sources/`, `host/`, `include/`, `backends/`, `shim/`, `linux/`, `tests/`). Symbols that only exist because of AppKit/SwiftUI/Metal plumbing (`makeNSView`, `Coordinator`, `*Sheet`, `*Controls`, Metal uniforms, app-delegate hooks) are excluded as *not applicable* — their Qt equivalents are covered by the feature matrix. Name matching is approximate: renamed equivalents show as missing, common names can show as found.

| Area | Found / applicable | Coverage |
|---|---|---|
| Document | 450 / 673 | 67% |
| IO | 35 / 58 | 60% |
| Rendering | 91 / 223 | 41% |
| UI | 70 / 195 | 36% |
| **All** | **650 / 1162** | **56%** |  

(101 macOS-only symbols excluded.)

### Per file (applicable symbols only)

| File | Found | Coverage | Missing (not N/A) |
|---|---|---|---|
| `CompositorApp.swift` | 1/1 | 100% |  |
| `ContentView.swift` | 3/12 | 25% | requestNewCanvas, PanelResizeEdge, releasesFocusOnCommit, ArrowStepper, listen, stopListening, ArrowStepping, arrowSteps… |
| `Document/AdjustmentEditing.swift` | 3/3 | 100% |  |
| `Document/BlurTool.swift` | 1/1 | 100% |  |
| `Document/BrushStroke.swift` | 42/47 | 89% | appendContinuous, flushContinuous, continuousCurve, drawTail, removeTail |
| `Document/CanvasSize.swift` | 7/7 | 100% |  |
| `Document/CloneStamp.swift` | 4/5 | 80% | setCloneSource |
| `Document/ColorPalette.swift` | 6/20 | 30% | paletteColor, setPaletteColor, swapPaletteColors, resetPaletteColors, openColorPicker, openTextColorPicker, openEffectColorPicker, closeColorPicker… |
| `Document/ContentFill.swift` | 3/3 | 100% |  |
| `Document/Crop.swift` | 17/17 | 100% |  |
| `Document/Curves.swift` | 5/5 | 100% |  |
| `Document/Distort.swift` | 23/27 | 85% | isConvex, warpFolded, DistortEffectsCache, distortedEffects |
| `Document/DocumentHistory.swift` | 11/11 | 100% |  |
| `Document/EditorSession+Brush.swift` | 4/12 | 33% | makeRasterEdit, shiftLineStart, finishBrushImmediately, commitPaintSnapshot, commitRasterEdit, typeOpacityDigit, changeBrushHardness, changeBrushSize |
| `Document/EditorSession+Projects.swift` | 0/4 | 0% | projectSnapshot, installProject, clearProject, createNewProject |
| `Document/EditorSession.swift` | 29/44 | 66% | selectTool, cycleToolMode, nudgeLayer, isInside, deleteActiveLayer, toggleLayerVisibility, beginVisibilitySwipe, setVisibilityInSwipe… |
| `Document/Filters.swift` | 17/21 | 81% | prepared, prepare, renderFilterPreview, commitBackgroundMask |
| `Document/FloatingSelection.swift` | 8/8 | 100% |  |
| `Document/Gradient.swift` | 6/13 | 46% | beginGradient, moveGradient, refreshGradient, gradientColors, endGradientDrag, cancelGradient, commitGradient |
| `Document/GuidedMatte.swift` | 6/6 | 100% |  |
| `Document/Guides.swift` | 4/19 | 21% | CanvasGuide, Axis, LayoutGrid, isMajor, GuideDrag, hitGuide, beginGuideCreation, beginGuideMove… |
| `Document/HueSaturation.swift` | 23/38 | 61% | normalize, setHandle, AdjustedPixels, setPreview, beginHueSaturation, updateHueSaturation, renderPendingPreview, commitHueSaturation… |
| `Document/ImageAdjustments.swift` | 8/8 | 100% |  |
| `Document/LayerAdjustment.swift` | 5/5 | 100% |  |
| `Document/LayerAppearance.swift` | 6/9 | 67% | displayedBlendMode, previewBlendMode, beginOpacityEdit |
| `Document/LayerEffects.swift` | 13/39 | 33% | StrokeEffect, ShadowEffect, ColorOverlayEffect, InnerShadowEffect, LayerEffects, setColor, LayerEffectKind, LayerEffectSelection… |
| `Document/LayerFlip.swift` | 3/3 | 100% |  |
| `Document/LayerGroups.swift` | 16/19 | 84% | LayerOpacity, effectiveOpacity, extendSelection |
| `Document/LayerMask.swift` | 22/30 | 73% | drawSmooth, selectLayerTarget, toggleLayerMask, canCopyMask, copyMask, toggleMaskLink, MaskDistortPreviewCache, maskDistortPreview |
| `Document/LayerMerge.swift` | 2/2 | 100% |  |
| `Document/LayerTransform.swift` | 19/19 | 100% |  |
| `Document/Levels.swift` | 14/18 | 78% | beginLevels, renderLevelsPreview, cancelLevels, commitLevels |
| `Document/LevelsAutomatic.swift` | 7/7 | 100% |  |
| `Document/LiveLayerMask.swift` | 14/16 | 88% | finishDeletingLayers, drawLayer |
| `Document/MagicWand.swift` | 7/10 | 70% | WandJob, WandResult, selectionSample |
| `Document/MaskTracing.swift` | 8/9 | 89% | whitePixels |
| `Document/ObjectSelection.swift` | 3/20 | 15% | ObjectSelectionSettings, ObjectSelection, selectAvailable, instanceIndex, edgePreservedBinaryMask, eroded, dilated, smoothed… |
| `Document/PixelAdjust.swift` | 6/6 | 100% |  |
| `Document/PixelInvert.swift` | 4/4 | 100% |  |
| `Document/ProjectWorkspace.swift` | 9/14 | 64% | confirmQuit, closeWindow, receive, receiveProviders, copyLayer |
| `Document/Selection.swift` | 17/43 | 40% | WandMode, selectionMode, lassoCursorMode, updateHeldSelectionKeys, beginLasso, dragMarquee, extendLasso, moveLassoCursor… |
| `Document/SelectionClipboard.swift` | 14/15 | 93% | sRGBCopy |
| `Document/SelectionEdits.swift` | 6/15 | 40% | deleteKeyPressed, deleteLayerOrMask, expandedUniformMask, beginPixelMove, movePixels, finishPixelMove, cancelPixelMove, nudgePixels… |
| `Document/ShapeTool.swift` | 11/16 | 69% | beginShape, dragShape, cancelShape, toggleShapeKind, finishShape |
| `Document/SmudgeLiquify.swift` | 10/10 | 100% |  |
| `Document/SubjectRemoval.swift` | 6/9 | 67% | MaskCache, vision, selectSubject |
| `Document/TypeTool.swift` | 1/16 | 6% | TextAlignment, LayerTextStyle, LayerText, TextDraft, beginText, editActiveText, applyText, finishText… |
| `IO/CanvasResizer.swift` | 2/2 | 100% |  |
| `IO/CompositorApplicationDelegate.swift` | 1/1 | 100% |  |
| `IO/ImageExporter.swift` | 6/12 | 50% | ExportError, drawLayer, pngData, ExportRaster, JPEGOptions, JPEGResult |
| `IO/ImageFileDrop.swift` | 0/3 | 0% | ImageFileDrop, importProviders, temporaryFile |
| `IO/ImageImporter.swift` | 4/4 | 100% |  |
| `IO/ImageResizer.swift` | 3/5 | 60% | applyImageSize, applyDocumentSize |
| `IO/ProjectController.swift` | 10/18 | 56% | ProjectController, saveCurrent, confirmQuit, confirmReplacement, showError, Incoming, receive, drainIncoming |
| `IO/ProjectStore.swift` | 9/13 | 69% | readPackage, validateGuides, checkSize, checkFile |
| `Rendering/AdjustmentSurface.swift` | 1/2 | 50% | AdjustmentSurface |
| `Rendering/BrushCursorOverlay.swift` | 2/3 | 67% | BrushCursorOverlay |
| `Rendering/CanvasViewport.swift` | 9/9 | 100% |  |
| `Rendering/DownsampleCache.swift` | 4/6 | 67% | evict, halve |
| `Rendering/EditorCanvas.swift` | 8/77 | 10% | CanvasView, GradientHandle, outlinedCursor, arrowCursor, fourArrowPath, selectionBadged, selectionCursor, zoomCursor… |
| `Rendering/EffectsPreviewCache.swift` | 9/12 | 75% | EffectsPreviewCache, Request, prepare |
| `Rendering/InlineTextEditor.swift` | 5/26 | 19% | CanvasTextView, InlineTextEditor, synchronize, textView, applyMirror, viewWillDraw, updateTrackingAreas, mouseEntered… |
| `Rendering/LayerEffectsSurface.swift` | 5/8 | 62% | LayerEffectsSurface, compose, drawPixels |
| `Rendering/LayerRenderer.swift` | 8/13 | 62% | Reduced, reduced, deviceScale, drawSource, sourceMapped |
| `Rendering/LiveMaskRenderer.swift` | 4/6 | 67% | LiveMaskRenderer, prepareStacks |
| `Rendering/MetalBrushCoverage.swift` | 3/3 | 100% |  |
| `Rendering/MetalLayerEffects.swift` | 5/5 | 100% |  |
| `Rendering/RasterSnapshot.swift` | 8/8 | 100% |  |
| `Rendering/SampleRingOverlay.swift` | 1/2 | 50% | SampleRingOverlay |
| `Rendering/SeparableBlend.swift` | 2/2 | 100% |  |
| `Rendering/TiledLayerRenderer.swift` | 10/26 | 38% | Piece, support, Frame, drawRaster, drawStroke, drawMaskStroke, drawLayer, drawReplacing… |
| `Rendering/TransformOverlay.swift` | 7/15 | 47% | drawLayoutGrid, drawGuides, drawSnapGuides, drawSelection, drawLassoDraft, drawTransformHandles, drawGradientLine, drawCrop |
| `UI/BlendModePicker.swift` | 2/5 | 40% | BlendModePicker, menuWillOpen, menuDidClose |
| `UI/BrushControls.swift` | 0/0 | 100% |  |
| `UI/CanvasRulers.swift` | 3/14 | 21% | CanvasRuler, CanvasRulerCorner, CanvasRulerView, CanvasRulerNSView, mouseDown, mouseDragged, mouseUp, canvasView… |
| `UI/CanvasSizeSheet.swift` | 2/2 | 100% |  |
| `UI/CanvasThumbnail.swift` | 4/7 | 57% | CanvasThumbnail, fittedSize, edgeTone |
| `UI/ColorPaletteControls.swift` | 1/2 | 50% | chooseMask |
| `UI/ColorPickerSheet.swift` | 3/6 | 50% | channelRow, commitHex, HueArrow |
| `UI/CropControls.swift` | 0/0 | 100% |  |
| `UI/CurvesControls.swift` | 1/1 | 100% |  |
| `UI/EffectsSheet.swift` | 2/2 | 100% |  |
| `UI/FilterSheet.swift` | 4/5 | 80% | flag |
| `UI/FloatingPanel.swift` | 3/5 | 60% | remember, canvasCenter |
| `UI/GradientControls.swift` | 0/0 | 100% |  |
| `UI/HueSaturationSheet.swift` | 3/5 | 60% | SpectrumEditor, nearestHandle |
| `UI/ImageSizeSheet.swift` | 2/2 | 100% |  |
| `UI/JPEGExportSheet.swift` | 0/0 | 100% |  |
| `UI/KeyboardShortcuts.swift` | 8/20 | 40% | ShortcutChord, ShortcutDefinition, ShortcutSettings, problem, canvasEvent, textEvent, configuredNativeShortcut, configuredKeyboardShortcut… |
| `UI/LassoControls.swift` | 1/2 | 50% | modifyControl |
| `UI/LayerAppearanceControls.swift` | 2/3 | 67% | applyPercentage |
| `UI/LayerMaskMenu.swift` | 0/1 | 0% | LayerMaskMenu |
| `UI/LayersPanel.swift` | 0/2 | 0% | LayersPanel, footerHitArea |
| `UI/LevelsSheet.swift` | 4/4 | 100% |  |
| `UI/NativeLayerList.swift` | 13/63 | 21% | NativeLayerList, clickedLayer, renameClickedLayer, draggedMask, draggedLayers, LayerTableView, clippingCursor, drawOutlined… |
| `UI/NavigationToolHeader.swift` | 1/4 | 25% | NavigationToolHeader, applyZoom, syncZoom |
| `UI/NewCanvasSheet.swift` | 1/3 | 33% | Field, clipboardDimensions |
| `UI/ProjectTabs.swift` | 1/13 | 8% | ProjectWorkspaceView, ProjectTabStrip, NewTabDropSlot, ProjectTabButton, NewProjectDropTarget, canReceiveDrag, ProjectTabDropDelegate, validateDrop… |
| `UI/ProjectWindowBridge.swift` | 1/3 | 33% | responds, forwardingTarget |
| `UI/ShapeControls.swift` | 0/0 | 100% |  |
| `UI/SliderSnap.swift` | 1/3 | 33% | SliderSnap, snapValue |
| `UI/ToolHeaderStyle.swift` | 0/3 | 0% | ToolHeaderStyle, unitSuffix, toolHeaderBar |
| `UI/TransformInspector.swift` | 4/7 | 57% | TransformInspector, TransformValueField, formatted |
| `UI/TypeControls.swift` | 3/8 | 38% | TypeFontPicker, FixedWidthPopUpButton, menuNeedsUpdate, menuWillOpen, menuDidClose |
## 2. Feature matrix (verified against bridge commands and Qt UI)

70 upstream features (README + changelog 1.1 – 1.1.8): **32 done (46%)**, 18 partial, **20 missing**. Counting partial as half: **59%**.

| Area | Feature | Status | Evidence / gap |
|---|---|---|---|
| Layers | Layers, folders, blend modes, layer opacity | Done | addLayer/addGroup/setBlendMode/setOpacity; layers panel |
| Layers | Folder opacity (1.1.6) | Missing | core `updateLayer` rejects opacity on groups |
| Layers | Folder duplication (1.1.5) | Missing | `duplicateLayer` is layer-only |
| Layers | Rename inline, duplicate, delete | Done | editable row, Duplicate/Delete actions |
| Layers | Reorder and nest by drag and drop; Option-drag duplicate | Missing | `moveLayer` exists in core, no drag-drop in the panel |
| Layers | Drag layers between open projects | Missing | single project per window |
| Layers | Layer masks: add reveal/hide, invert, delete, enable | Done | menu + row badge |
| Layers | Mask link/unlink; select mask target for painting/transform | Partial | `setMaskLinked`/`setMaskSelected` in core, no Qt control |
| Layers | Mask paint / fill / blur / feather | Partial | brush `mask` parameter in core; no UI to target a mask, no blur/feather |
| Layers | Clipping masks and folder masks | Partial | `LiveLayerMask` ported; no UI to create |
| Layers | Adjustment layers (Hue/Sat, Levels, Curves, Exposure, Gradient Map, Grain) | Done | new sheets via panel/menu |
| Layers | Re-edit an existing adjustment layer | Missing | no "edit adjustment" entry point in the panel |
| Layers | Merge Down / Merge Layers / Merge Group (⌘E) | Partial | `LayerMerge` plan ported; no command or menu |
| Layers | Layer effects: stroke, shadow, inner shadow, colour overlay (1.1) | Missing | `LayerEffects.swift` not ported |
| Transform | Non-destructive move / scale / rotate | Done | Move tool: handles, X/Y/W/H/Angle, Link |
| Transform | Flip layer | Done | Layer menu |
| Transform | Flip canvas | Partial | `flipCanvas` command exists, no menu item |
| Transform | Free distort (⌘-drag a handle) | Partial | core + `distortBegin/Commit`; no handle gesture |
| Transform | Transform several layers / a folder together | Missing | single active layer only |
| Transform | Snapping to canvas/layer edges and centres, guides | Missing | snap maths in core; not used by the Qt canvas |
| Transform | Exact position/size/scale/angle values | Done | options bar fields |
| Selections | Rectangle / Ellipse marquee | Done | tools + Select menu |
| Selections | Freehand and Polygonal lasso | Done | options bar toggle |
| Selections | Magic wand (tolerance, contiguous, sample all layers) | Partial | wand works; tolerance/contiguous not editable in the bar |
| Selections | Add / subtract selection modes | Done | New/Add/Subtract |
| Selections | Expand / Contract | Done | core + options bar |
| Selections | Feather / feathered modifiers (1.1.1) | Missing | not in the Linux core |
| Selections | Select All, Inverse | Missing | `invertSelection`/`selectAll` not ported |
| Selections | Move the outline; nudge with arrows | Missing | not ported |
| Selections | Move / duplicate the pixels inside a selection | Missing | only `movedSelection` helper ported |
| Selections | Load layer pixels / a mask as a selection | Missing | not ported |
| Selections | Object selection (Magic tool Object mode, 1.1.8) | Missing | needs a Vision replacement (OpenCV bridge candidate) |
| Selections | Fill / Clear selection | Done | shape-aware, anti-aliased |
| Selections | Content-Aware Fill (and extend past edges) | Partial | fill via OpenCV; extend-canvas mode not verified |
| Painting | Brush: size, hardness, opacity, colour, blend | Done | options bar |
| Painting | Brush Shift-for-straight-line | Partial | Shift handled for tablet only; no straight-line mode |
| Painting | Eraser | Done | tool |
| Painting | Spot Healing brush | Done | tool |
| Painting | Clone Stamp (Alt to set source) | Done | tool |
| Painting | Blur tool (pixels or masks) | Missing | `BlurTool` ported in core; no command or tool |
| Painting | Smudge / Liquify | Done | Tool menu |
| Painting | Gradient tool | Missing | gradient maths partly ported; no command or tool |
| Painting | Shape tool (rectangle, rounded rect, ellipse, line) | Partial | menu inserts fixed shapes; no drag tool, no line |
| Painting | Type tool (paragraph boxes, fonts, inline editing, clipping) | Missing | `TypeTool.swift` not ported |
| Painting | Eyedropper | Missing | no tool (Levels eyedroppers only) |
| Painting | Full colour picker | Done | new dialog |
| Adjust/Filter | Levels with Auto and eyedroppers | Done | new |
| Adjust/Filter | Curves | Done | graph editor |
| Adjust/Filter | Hue/Saturation with range bars | Partial | range presets; the draggable spectrum handles are not ported |
| Adjust/Filter | Exposure, Gradient Map, Grain | Done | sheets |
| Adjust/Filter | Invert | Partial | `invert` command exists, no menu item |
| Adjust/Filter | Gaussian / Motion Blur, Add Noise, Lens Correction | Done | Filter menu |
| Adjust/Filter | Remove Background | Done | Layer menu |
| Adjust/Filter | Live previews limited to the selection | Partial | previews run; selection clipping not verified |
| Canvas/Files | Multiple projects in tabs | Missing | one document; tab header is a single tab |
| Canvas/Files | Crop (snapping, Option symmetric) | Partial | crop works; no snapping or symmetric mode |
| Canvas/Files | Canvas Size / Image Size | Done | dialogs |
| Canvas/Files | Zoom / fit / actual pixels; pan | Done | View menu, Space to pan |
| Canvas/Files | Pixel grid when zoomed in; sharp downsampling | Partial | core cache ported; no pixel grid overlay |
| Canvas/Files | Rulers, guides, grid, snap (1.1.7) | Missing | `Guides.swift`, `CanvasRulers.swift` not ported |
| Canvas/Files | Import PNG/JPEG/BMP/WebP, dropped images | Done | Open Image + drag-drop |
| Canvas/Files | Import HEIC / TIFF | Partial | TIFF via Qt; HEIC gated on a hardened decoder |
| Canvas/Files | Export JPEG with live preview | Partial | export works; no preview sheet |
| Canvas/Files | Export PNG / TIFF / WebP | Done | File menu |
| Canvas/Files | Copy / Copy Merged / Cut / Paste | Done | Edit menu |
| Canvas/Files | Save / open project (.comp) | Done | atomic package IO |
| App | Undo / redo history | Done | Edit menu |
| App | Keyboard shortcuts window (1.1.6) and Photoshop-style shortcuts | Partial | 27 shortcuts bound; no shortcuts window |
| App | Crash-recovery autosave | Done | Linux addition |
| App | Command palette | Done | Linux addition |

## 3. Backend and infrastructure (already 100% for their gates)

Stages 0-12 (Flatpak, C ABI seam, 8 C kernels, Swift core, CoreGraphicsCompat, Skia raster, Qt shell, `.comp` IO, Vulkan brush + CPU failsafe, editing tools, OpenCV, XDG portals, tablet) pass their gates: 429 Swift tests, 5 Qt smoke modes, CMake build. Upstream has 50 test files; the port has 38 (some upstream tests are unported: GuideTests, TypeToolTests, SelectionFeatherTests and others).

## 4. Gaps to reach 100%, in suggested order

1. **Quick wins (UI wiring of existing core):** Invert menu, Flip Canvas menu, Merge Down/Group commands, Select All / Inverse, mask target + link control, edit-adjustment entry point, wand tolerance/contiguous fields, Move-selection nudge.
2. **Core ports:** Feather/modifiers, folder opacity + duplication, selection move/duplicate pixels, load mask/layer as selection, Blur tool, Gradient tool, Shape drag tool + line, Eyedropper.
3. **Large features:** Guides/rulers/grid/snap, Type tool (font stack, inline editor), Layer effects (GPU path → Skia), Object selection (segmenter replacement), multi-project tabs and cross-project drag, JPEG export preview, keyboard shortcuts window.
4. **Tests:** port the remaining upstream test files alongside each feature.
