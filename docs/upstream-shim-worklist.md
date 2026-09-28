# Upstream shim work list (Phase 0 spike result)

Approach (see plan): compile upstream `Compositor/{Document,IO,Rendering}` **unmodified** and supply the Apple
APIs it imports as Linux compat modules. `scripts/upstream-probe.sh` builds upstream against stub Apple modules
and lists what is still missing; compile errors *are* the work list.

Run (inside the Flatpak SDK):
`flatpak run --command=sh --devel --filesystem="$PWD" --filesystem=/tmp org.kde.Sdk//6.10 -c 'cd "$1" && PATH=/usr/lib/sdk/swift6/bin:$PATH scripts/upstream-probe.sh /tmp/probe' sh "$PWD"`

## Result (upstream 1.1.8, 2026-09-21)

Decision gate (< ~150 missing symbols, nothing unshimmable outside Metal/UI): **PASSED**. Upstream compiled with the
port's existing CoreGraphics compat (`CGImage`, `CGContext`, `CGPath`, `CGColorSpace`, …) leaves **~82 distinct missing
symbols + 17 missing members** on existing compat types. Progress of the spike: 9,439 error lines with empty stubs →
1,351 once `CoreGraphics` was real and Foundation/Observation were re-exported → 450 distinct lines now.

| Framework | Distinct missing | Symbols |
|---|---|---|
| Accelerate | 11 | `vImage_Buffer`, `vImageScale_ARGB8888`, `vImageScale_Planar8`, `vImageTableLookUp_Planar8`, `vImageMatrixMultiply_ARGB8888`, `vImage_Error/Flags/PixelCount`, `kvImage*` (only 2 files use it) |
| AppKit / ObjC Foundation | 20 | `NSColor`, `NSImage`, `NSBitmapImageRep`, `NSGraphicsContext`, `NSCursor`, `NSSound`, `NSFont`, `NSTextStorage/LayoutManager/Container`, `NSMutableParagraphStyle`, `NSPasteboard`, `NSItemProvider`, `NSAlert`, `NSOpenPanel`, `NSView`, `NSWindow` (type only), `NSFileCoordinator`, `FileWrapper`, `autoreleasepool` |
| CoreImage | 10 | `CIImage`, `CIContext`, `CIFilter`, `CIVector`, `CIColor`, `kCIInput{Image,BackgroundImage,MaskImage,Radius,Angle}Key` |
| ImageIO | 22 | `CGImageSource{CreateWithURL,CreateWithData,CreateImageAtIndex,CreateThumbnailAtIndex,CopyPropertiesAtIndex,GetCount,GetType}`, `CGImageDestination{CreateWithData,AddImage,Finalize}`, `kCGImage*` keys |
| CoreFoundation / CoreVideo | 8 | `CFData/CFDictionary/CFString/CFURL/CFArray` (type aliases to Foundation), `CVPixelBuffer` (+ width/height) |
| CoreGraphics (gaps) | 1 + 17 members | `CGGradient`; `CGContext.{concatenate,ctm,addLine,move,setLineCap,drawLinearGradient,drawRadialGradient}`, `CGPath.{union,intersection,contains,applyWithBlock,boundingBoxOfPath}`, `CGMutablePath.addLines` |
| Vision | 2 | `VNImageRequestHandler`, `VNGenerateForegroundInstanceMaskRequest` (→ OpenCV GrabCut) |
| UniformTypeIdentifiers | 1 | `UTType` |
| Metal (override, not shim) | 2 | `MetalBrushCoverage`, `MetalLayerEffects` → same-API overrides backed by Vulkan/Skia/OpenCV/C |
| Foundation gap | 2 | `URL.start/stopAccessingSecurityScopedResource` (no-op) |
| App-level (Qt/bridge) | 3 | `ProjectController`, `ImageFileDrop`, `setAccessibilityElement` |

Remaining error lines beyond the table are cascades from those missing types (inference failures).

## Architecture issue found: the existing compat depends on the domain

`Sources/CompositorCore/CoreGraphicsCompat` references upstream domain types (`LayerBlendMode.cgMode` in `Color.swift`,
and a Swift-renderer fallback in `Context.swift` calling `LayerRenderer`/`LayerTransform`/`RasterImage`). A compat module
must sit *below* upstream (dependency inversion): move `cgMode` into an adapter file on the upstream side and inject the
Swift fallback through `RenderDeviceBinding` instead of importing the renderer. The probe neutralises both in its copy.

## Other facts established
- Upstream `Rendering/` has C pixel kernels reached through `Compositor-Bridging-Header.h` (no imports in Swift). SwiftPM
  cannot mix languages in one target, so they become a C target (`UpstreamKernels`) and Swift gets
  `-import-objc-header` (works with `unsafeFlags` in a root package).
- `EditorSession.swift` imports SwiftUI (for Observation only); on Apple platforms SwiftUI/AppKit re-export Foundation,
  CoreGraphics and Observation, so the compat modules re-export them too.
- Swift 6.3.3 ships `swift-testing` (visible in `swift test` output), so upstream `@Test` files can compile unmodified
  once the module builds.

## Next steps (Phase 1)
1. Invert the compat dependency (above) and promote `CoreGraphicsCompat` to a standalone `CoreGraphics` module.
2. Add the missing `CGContext`/`CGPath`/`CGGradient` members.
3. Add `AppKit`/`CoreImage`/`ImageIO`/`Accelerate`/`Vision`/`UniformTypeIdentifiers` compat modules in the order of the
   table (counts are per distinct symbol; start with `ImageIO`, `CGGradient`, `NSColor`/`NSImage`, then `CoreImage`).
4. Re-run the probe until `UpstreamProbe` compiles clean, then compile upstream `CompositorTests` unmodified.


## Phase 1 status (2026-09-21)

Done, all verified by `swift test` (446 tests, 0 failures; 3 Skia-only tests need `COMPOSITOR_SKIA_BRIDGE` to run):

| Module | What it provides | Notes |
|---|---|---|
| `CoreGraphics` (`Sources/Compat/CoreGraphics`) | `CGContext`/`CGImage` over Skia, Apple-shaped `CGPath`/`CGMutablePath` (curves, rounded rects, hit testing, `applyWithBlock`, `copy(using:)`, booleans via SkPathOps, stroke outlines), `CGGradient`, `CGColorSpace` name constants + failable init, `CGColor(srgbRed:…)`, `CFArray/CFDictionary/CFString/CFURL/CFData` aliases | No dependency on document types any more; the pure-Swift renderer is injected through `CGContext.softwareDraw` (`CompatBootstrap.install()`) |
| `AppKit` (`Sources/Compat/AppKit`) | `NSColor`, `NSGraphicsContext`, `NSCursor` (tokens), `NSSound` (hook), `NSImage`/`NSBitmapImageRep` (codec hook), `NSPasteboard` (change counts, backend hook), `NSItemProvider`, `NSAlert`/`NSOpenPanel`/`NSSavePanel` (injectable handlers), `NSView`/`NSWindow` placeholders, text stack stubs (`NSFont`, `NSAttributedString.boundingRect`, `NSTextStorage`, …) | Text layout is approximate and drawing is a no-op until the Skia paragraph backend (Phase 3) |
| `FoundationCompat` | `FileWrapper`, `NSFileCoordinator`, `NSErrorPointer`, security-scoped URL calls, `autoreleasepool` | Files that import only Foundation see it through `-Xfrontend -import-module FoundationCompat` |
| `UniformTypeIdentifiers` | `UTType` table with conformance, extensions, MIME, exported/imported types | |
| Skia bridge | Path elements/ops/stroke/fill/gradients ABI (`include/SkiaBridge.h`); `scripts/build-skia-pathops.sh` archives Skia's PathOps (only built with PDF otherwise) and CMake links it | Flatpak manifest builds the same archive |

Probe (`scripts/upstream-probe.sh`): 9,439 → 450 → **175 distinct error lines, 53 missing symbols**:

- **ImageIO (22):** `CGImageSource*`, `CGImageDestination*`, `kCGImage*` keys → Qt image plugins (next).
- **CoreImage (10):** `CIImage`, `CIContext`, `CIFilter`, `CIVector`, `CIColor`, input keys → Skia/OpenCV/C backend chain.
- **Accelerate (12):** `vImage_Buffer`, `vImageScale_*`, `vImageTableLookUp_Planar8`, `vImageMatrixMultiply_ARGB8888`, `kvImage*`, `Pixel_8`.
- **CoreVideo (3):** `CVPixelBuffer` (+ width/height).
- **Vision (2):** `VNImageRequestHandler`, `VNGenerateForegroundInstanceMaskRequest` → OpenCV GrabCut.
- **Metal (2, override files):** `MetalBrushCoverage`, `MetalLayerEffects` → Vulkan/Skia/OpenCV/C.
- **App level (2):** `ProjectController`, `ImageFileDrop` (Qt shell).

Known gaps carried forward: dashed strokes are recorded but not rasterised; `CGContext` blend modes/clipping in the
no-Skia fallback path are minimal; Skia in the Flatpak manifest (canvaskit 0.42.0) is older than the tree the bridge
is built against locally (`SkPathBuilder`, `SkPathIter`, `SkGradient`), so the manifest's Skia version must be bumped.


## Phase 3 status (2026-09-21): the probe builds clean

`scripts/upstream-probe.sh` now reports **0 error lines and `Build complete`**: upstream `Compositor/{Document,IO,Rendering}`
(70 Swift files, plus the C pixel kernels via the bridging header) compile **unmodified** on Linux against the compat
modules, with the Xcode project's own concurrency settings (`SWIFT_VERSION 5`, default actor isolation `MainActor`,
approachable concurrency). History: 9,439 → 450 → 175 → 53 → 31 → 11 → **0** distinct error lines.

Excluded from the probe (7 files, all view/app code the Qt shell replaces): `IO/CompositorApplicationDelegate.swift`,
`Rendering/EditorCanvas.swift`, `InlineTextEditor.swift`, `BrushCursorOverlay.swift`, `SampleRingOverlay.swift`, and the two
Metal files below.

| Module / file | Provides |
|---|---|
| `Accelerate` | `vImage_Buffer`, `vImageScale_ARGB8888/Planar8` (Lanczos-3 or tent, filtered when shrinking), `vImageTableLookUp_Planar8`, `vImageMatrixMultiply_ARGB8888` with vImage's row-major pixel×matrix convention |
| `ImageIO` | `CGImageSource*`/`CGImageDestination*` with Apple's keys; portable pure-Swift PNG codec (inflate, all filters, 8/16-bit, palette, pHYs); pluggable backend; the Qt host registers QImageReader/QImageWriter (`host/QtImageIO.cpp`, `compositor_imageio_register`) so JPEG/TIFF/WebP/BMP/GIF work; EXIF orientation and thumbnails |
| `CoreImage` | Lazily evaluated `CIImage` graph + CPU float evaluator: Gaussian/motion blur, colour matrix/clamp/cube, blend-with-mask, colour burn/dodge, perspective transform, edge-preserving upsample, crop, clamp-to-extent, affine transform, EXIF orientation; `CIContext` with managed (linear) and unmanaged modes |
| `CoreVideo` | `CVPixelBuffer` (8-bit and float mask planes) |
| `Vision` | `VNGenerateForegroundInstanceMaskRequest` / instance-mask observation / scaled masks; classical border-model segmenter as the floor; `ForegroundSegmentationRegistry` + `compositor_vision_register` for OpenCV GrabCut or a learned model |
| `SwiftUI` | `View`, `NSHostingController`, `move(fromOffsets:toOffset:)`; `AppKit` gained `NSViewController`, `NSWindow` sheets (`beginSheet`, `sheetPresenter`), `NSDocumentController`, alert/panel members |
| `Sources/Overrides/` | `MetalBrushCoverage` (same API, Vulkan → C CPU kernel via `AdaptiveBrushCoverage`), `MetalLayerEffects` (same API, `shared == nil` so upstream's CPU effects path runs), `CanvasSizeSheet`/`ImageSizeSheet`/`JPEGExportSheet` stand-ins (upstream's unmodified `ProjectController` hosts and presents them; with no host they settle as cancelled) |

Fidelity notes recorded while getting here (all fixed in the shims): `CGImage.colorSpace` and `CGContext.colorSpace` are
optional, `CGColorSpace.sRGB` is a name constant, `CGColor(colorSpace:components:)` and `CGDataProvider(data:)` are
failable, `CGPath` is `Equatable`/`Hashable`, `CGDataProviderDirectCallbacks.getBytesAtPosition` takes a non-optional
buffer, `NSItemProvider` lives in Foundation, `CVPixelBuffer` comes with Vision.

## What is next (Phase 2: the strangler)

The probe proves compilation, not behaviour. Next: (1) a real `UpstreamCore` SwiftPM target built from
`Compositor/{Document,IO,Rendering}` + `Sources/Overrides` + the Linux-original Vulkan brush backend, (2) compile
upstream's `CompositorTests/*.swift` unmodified against it and drive failures to zero (Swift Testing is available), (3)
retire the forked `Sources/CompositorCore` files module by module as their upstream equivalents pass, (4) point
`EditorBridge` at the unmodified `EditorSession`. Then Phase 4/5: Vulkan → Skia → OpenCV → C effects backend behind
`MetalLayerEffects.shared`, and OpenCV GrabCut behind the segmentation registry.
