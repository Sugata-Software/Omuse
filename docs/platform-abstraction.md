# Platform abstraction (SOLID) — swapping macOS APIs for Linux and back

The editor's domain logic never names a platform API. Everything platform-specific sits
behind a small interface, injected at the composition root, so macOS AppKit/CoreGraphics
and Linux Qt/Skia implementations are interchangeable (and test doubles are trivial).

| Concern | macOS original | Interface | Linux implementation |
|---|---|---|---|
| 2D raster drawing | CoreGraphics `CGContext` | `CanvasBackend` / `CanvasBackendFactory` (`CanvasBackends.factories`, tried in order, pure-Swift fallback last) | `SkiaCanvasBackend` (Skia C ABI); Vulkan / OpenCV / test doubles plug in the same way |
| Layer effects (stroke / shadow / overlay / inner shadow) | Metal (`MetalLayerEffects`) | `LayerEffectsBackend` chain behind the same-signature override | Vulkan compute (`backends/effects/shaders/effects.comp`) -> portable C++ tier (`EffectsCPU.cpp`); Skia image-filter tier (opt-in `COMPOSITOR_EFFECTS=skia`); OpenCV tier not built |
| Brush coverage GPU | Metal | `BrushCoverageComputing` (Swift) | Vulkan compute, CPU fallback |
| Foreground segmentation | Vision | `ForegroundSegmenter`, `MatteRefiner` (Swift) | OpenCV bridge |
| Image codecs | ImageIO | `IImageExporter` (C++) | Qt image plugins |
| Stylus input | `NSEvent` tablet | `ITabletHandler` (C++) | `QTabletEvent` |
| Open/Save/Export dialogs | `NSOpenPanel`/`NSSavePanel` | `IFileDialogService` | `QFileDialog` (XDG portal in Flatpak) |
| Clipboard | `NSPasteboard` | `IClipboardService` | `QClipboard` |
| Colour choice | `NSColorPanel` | `IColorPickerService` | `ColorPickerDialog` |
| User warnings | `NSAlert` | `IUserNotifier` | `QMessageBox` |
| App data / recovery dir | `FileManager` Application Support | `IStorageLocator` | `QStandardPaths::AppDataLocation` |
| Document editing | `EditorSession` | `LayerManipulating`, `MaskManipulating`, `BrushPainting`, `CanvasOperations`, `SubjectMatteOperations` | same Swift core |

## How the principles are applied

- **S** — one reason to change per interface (files, clipboard, colour, notification, storage).
- **O** — a new platform is a new implementation of the same interfaces; `SessionWindow`,
  `AdjustDialog` and `SizeDialog` are not edited.
- **L** — all implementations share one contract: empty `QString` / null `QImage` / invalid
  `QColor` = cancelled or unavailable.
- **I** — five narrow roles instead of one `Platform` interface. `AdjustDialog` and
  `SizeDialog` receive only `IColorPickerService`.
- **D** — `SessionWindow` takes a `PlatformServices` bundle by constructor injection
  (`host/interfaces/IPlatformServices.h`). The Qt defaults live in `host/QtPlatformServices.h`.
  The interfaces use value types only (`QString`, `QImage`, `QColor`), so no widget parents
  leak through and a non-Qt implementation needs no Qt Widgets.

## Adding another platform (e.g. macOS shell)

1. Implement the five interfaces with AppKit (`NSOpenPanel`, `NSPasteboard`, …).
2. Build a `PlatformServices` with those and pass it to the shell's constructor.
3. Nothing in the editor, dialogs or core changes.

## Verification

`--dialog-smoke` runs a `SessionWindow` against fake file dialog, clipboard, notifier and
storage (see `host/DialogJourney.cpp`): export goes to the injected path, a failed export
reaches the injected notifier, Copy lands in the injected clipboard, autosave writes under
the injected storage root.

## Known remaining debt

- `SessionWindow` is still a large class (window, canvas events, tool logic, layers panel).
  The next SRP step is extracting the layers panel, options bar and tool controllers.
- `MainWindow` (the C++-only build path's shell) still calls `QFileDialog`/`QMessageBox`
  directly.

## Replaceable services inside the Apple-API layer (`CompatSupport.ServiceSlot`)

Upstream's unmodified code cannot take a service through an initialiser, so each compat module keeps its seam in a
`ServiceSlot<Interface>` instead of a bare global: the host `install`s an implementation at start-up, tests use
`withOverride` (restored afterwards, even on throw), and an optional fallback supplies the built-in one.

| Seam | Interface | Slot accessors |
|---|---|---|
| 2D rasterizer | `CanvasBackend` / `CanvasBackendFactory` | `CanvasBackends.factories`, `withFactories` |
| Image codecs (ImageIO) | `ImageCodecBackend` | `ImageCodecRegistry.host`, `withHost` |
| Clipboard | `NSPasteboard.Backend` | `NSPasteboard.backend`, `withBackend` |
| Foreground segmentation (Vision) | `ForegroundSegmentationBackend` | `ForegroundSegmentationRegistry.backend`, `withBackend` |

`CompCanvasBridge` (the Skia dlopen binding) is internal to `SkiaCanvasBackend`. Event-style hooks (alert and panel
handlers, `onBeep`, cursor changes) stay closures: they are one-way notifications, not services with a lifetime.

## Layer effects chain

`Sources/Overrides/MetalLayerEffects.swift` keeps upstream's surface (`MetalLayerEffects.shared`, `render(_:effects:)`) and
runs `LayerEffectsBackends.chain`, each tier failing over to the next. `COMPOSITOR_EFFECTS=auto|vulkan|skia|opencv|cpu` chooses;
the C++ tier is always last. `auto` (default) is a hardware Vulkan device when there is one, then OpenCV when the build has
it, then the C++ tier (a software Vulkan device such as llvmpipe is skipped). `skia` and `vulkan` put that tier first.

| Tier | Where | Checked against |
|---|---|---|
| C++ (reference) | `backends/effects/EffectsCPU.cpp`, the nine Metal passes, multithreaded | upstream's own CoreImage renderer: largest difference 10/255, mean < 0.4 |
| Vulkan | `backends/effects/EffectsVulkan.cpp` + `shaders/effects.comp` (bundled SPIR-V, `scripts/build-brush-shader.py --check`) | C++ tier: bit-identical on llvmpipe |
| OpenCV | `backends/effects/EffectsOpenCV.cpp` on the pinned OpenCV 4.14.0 (`third_party/opencv.pinned`; static core + imgproc; built locally into `build/opencv/install` with the manifest's options, or `/app` in Flatpak): rectangular dilate/erode, `warpAffine`, `GaussianBlur` on float planes | C++ tier: within one level |
| Skia | `compositor_skia_effects_render` in `SkiaBridge.cpp`: image filters over float surfaces (dilate/erode, bilinear translate, clamped Gaussian, arithmetic blends, colour-matrix tint, source-over compose) | C++ tier: within one level |

Upstream's own tests never call layer effects, so these checks live in `Tests/LinuxOverrideTests`.
Timing (release, 2400x1600 layer, stroke + shadow + inner shadow): OpenCV 0.19 s, C++ 0.77 s, Vulkan on llvmpipe 1.6 s, Skia raster 5.2 s.
Hence OpenCV is in `auto`, Skia is opt-in (with a GPU-backed Skia it becomes the tier to try after Vulkan). Not done:
device-local staging buffers for discrete GPUs.
