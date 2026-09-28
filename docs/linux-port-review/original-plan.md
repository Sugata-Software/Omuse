# Plan portowania Compositor z macOS na GNU/Linux

## 0. Podstawa audytu

Audyt dotyczy gałęzi `main` na commitcie:

`a19db9011282399785dc18efcfded904627bdcc2`
z 18 września 2026, commit „Publish update feed for Compositor 1.0.4”.

GitHub zwrócił kompletne, nietruncated drzewo repozytorium. Produkcyjny kod jest podzielony przede wszystkim na `Document`, `IO`, `Rendering`, `UI`, plus `CompositorApp.swift` i `ContentView.swift`; repo ma również obszerny zestaw testów jednostkowych.

### Główny wniosek

Najkrótsza droga do Linuxa to:

```text
                  existing Compositor
               Swift core + existing C
                         │
            ┌────────────┴────────────┐
            │ minimal compatibility  │
            │ + platform boundaries  │
            └────────────┬────────────┘
                         │
        ┌────────────────┼────────────────┐
        │                │                │
        ↓                ↓                ↓
   Qt Widgets      CoreGraphicsCompat   XDG/Qt
   UI + input            │             platform I/O
                         ↓
                   thin C ABI
                         ↓
                       Skia
                    ┌────┴────┐
                    ↓         ↓
                 Vulkan     Raster
                   GPU        CPU

       existing C ────────────────┐
                                  ↓
                       image processing
                           + OpenCV
                         only if useful
```

**Nie proponuję przepisywania Swift do C++.** C++ powinien istnieć tylko po drugiej stronie cienkiego C ABI, tam gdzie trzeba dotknąć Qt, Skia i Vulkan.

---

# 1. Inwentaryzacja aktualnych zależności Apple

## 1.1 Najważniejsze odkrycie: Metal nie jest rendererem Compositor

Metal **występuje**, ale jest lokalny. `Rendering/MetalBrushCoverage.swift` używa `MTLDevice`, `MTLCommandQueue`, `MTLComputePipelineState`, buforów, command bufferów i wbudowanego kernela MSL do przyspieszania obliczeń coverage pędzla.

Nie znalazłem w audytowanej warstwie renderingu odpowiednika „cały canvas renderowany przez Metal/MetalKit”. Główne `LayerRenderer` i `TiledLayerRenderer` są oparte na CoreGraphics.

**Wniosek:** nie ma sensu projektować migracji całego renderera `Metal → Vulkan`. Realny podział to:

```text
CoreGraphics renderer -> CoreGraphicsCompat -> Skia -> Vulkan/Raster

MetalBrushCoverage
    -> CPU fallback już istniejący
    -> opcjonalnie mały Vulkan Compute backend później
```

To jest znacznie tańsze.

---

## 1.2 Macierz Apple API

| Apple API / mechanizm              | Aktualne miejsca / funkcja                                                                                                              | GNU/Linux replacement                                                           | Metoda                     | Koszt zmian                                                                                                   |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------- |
| **SwiftUI**                        | `CompositorApp.swift`, `ContentView.swift`, większość `UI/`, SwiftUI wrapper `EditorCanvas`; aplikacja, menu, toolbary, sheets, layout. | Qt Widgets                                                                      | `REWRITE` tylko UI         | **Wysoki nowy kod**, ale prawie zero zmian w istniejącym UI: pliki macOS zostają i są wyłączone z Linux build |
| **AppKit**                         | `ProjectController`, `CompositorApplicationDelegate`, `EditorCanvas`, `NativeLayerList`, clipboard/cursors/windows.                     | Qt + XDG                                                                        | `ADAPTER`                  | Średni–wysoki nowy kod; nie warto shimować AppKit                                                             |
| **CoreGraphics geometry**          | `CGPoint`, `CGSize`, `CGRect`, `CGAffineTransform` w modelu, transformacjach, selekcjach, rendererze                                    | mały `CoreGraphicsCompat` w Swift                                               | `SHIM`                     | **Niski per existing file**; bardzo duża oszczędność zmian                                                    |
| **CGImage**                        | centralny typ bitmapy/document assets/history/rendering                                                                                 | kompatybilny `CGImage` wrapper → opaque Skia image/pixel store                  | `SHIM`                     | Średni nowy kod, minimalna ingerencja w wywołania                                                             |
| **CGContext**                      | `LayerRenderer`, `TiledLayerRenderer`, bitmap editing, eksport, maski                                                                   | `CGContext`-podobny wrapper → SkCanvas/SkSurface                                | `SHIM`                     | Średnio-wysoki shim, ale oszczędza rewrite kilkunastu algorytmów                                              |
| **CGPath / clipping / masks**      | tiled renderer, selections, mask rendering                                                                                              | SkPath + SkPathOps                                                              | `SHIM`                     | Średni                                                                                                        |
| **CGBlendMode**                    | layer appearance/rendering; własna korekta części blend modes                                                                           | SkBlendMode                                                                     | `SHIM`                     | Niski–średni + pixel parity tests                                                                             |
| **CoreImage**                      | `Filters`, `PixelAdjust`, importer, background removal, separable blend                                                                 | Skia per operation, OpenCV tam gdzie korzystniejsze                             | `ADAPTER`                  | Średni; nie shimować całego CI object graph                                                                   |
| **ImageIO**                        | importer, exporter, project assets                                                                                                      | `PlatformImageCodec`; Skia codecs + mały codec backend dla brakujących formatów | `ADAPTER`                  | Średni                                                                                                        |
| **UniformTypeIdentifiers**         | import/drop/file filters/project UTI                                                                                                    | mały enum/MIME compatibility + shared-mime-info                                 | `SHIM` / `ADAPTER`         | Niski                                                                                                         |
| **Metal**                          | tylko potwierdzony `MetalBrushCoverage.swift` compute path                                                                              | opcjonalny Vulkan Compute/SPIR-V                                                | `ADAPTER`                  | Niski dla v1 — można początkowo wyłączyć                                                                      |
| **MetalKit**                       | brak potwierdzonego użycia w audytowanym rendererze                                                                                     | —                                                                               | `KEEP` / N/A               | zero                                                                                                          |
| **Accelerate/vImage**              | `DownsampleCache.swift`, Lanczos scaling                                                                                                | Skia resampling; OpenCV tylko jeśli testy jakości tego wymagają                 | `WRAPPER`                  | Niski — zmienia się pojedynczy backend skalowania                                                             |
| **Vision**                         | `SubjectRemoval.swift`, foreground-instance mask                                                                                        | Linux segmentation backend; opcjonalnie OpenCV DNN + model                      | `ADAPTER`                  | **Wysoki**, lokalny do jednej funkcji                                                                         |
| **NSImage**                        | cursory/część clipboard/UI                                                                                                              | Qt QImage/QPixmap po stronie platformowej                                       | `ADAPTER`                  | Niski–średni                                                                                                  |
| **NSColor**                        | głównie bridge/UI w `ColorPalette.swift`                                                                                                | neutralny RGBA/HSB + QColor tylko w Qt shell                                    | `SHIM`                     | Niski                                                                                                         |
| **NSWindow**                       | project/window management                                                                                                               | QMainWindow/QWindow                                                             | `ADAPTER`                  | Średni                                                                                                        |
| **NSPasteboard**                   | `NativeLayerList`, `SelectionClipboard`                                                                                                 | QClipboard/QMimeData                                                            | `ADAPTER`                  | Niski–średni                                                                                                  |
| **NSOpenPanel / NSSavePanel**      | `ProjectController`                                                                                                                     | QFileDialog → portal                                                            | `ADAPTER`                  | Niski                                                                                                         |
| **NSApplication**                  | entry point, commands, quit/reopen                                                                                                      | QApplication                                                                    | `ADAPTER`                  | Niski                                                                                                         |
| **NSFileCoordinator / package IO** | `ProjectStore`, exporter                                                                                                                | `ProjectPackageIO` + portal-granted URLs                                        | `ADAPTER`                  | Średni                                                                                                        |
| **security-scoped URLs**           | macOS file access                                                                                                                       | Documents/FileChooser portal                                                    | `ADAPTER`                  | Niski                                                                                                         |
| **drag-and-drop**                  | SwiftUI `.onDrop`, `NativeLayerList`, `ImageFileDrop`                                                                                   | Qt QDrag/QMimeData/drop events                                                  | `ADAPTER`                  | Średni                                                                                                        |
| **mouse/keyboard**                 | duża część `EditorCanvas.swift` oparta o NSEvent                                                                                        | QMouseEvent/QKeyEvent                                                           | `ADAPTER`                  | Średni–wysoki UI shell                                                                                        |
| **tablet**                         | nie znalazłem obecnie dedykowanej logiki pressure/tablet w kluczowym input path                                                         | QTabletEvent, gdy zostanie potrzebne                                            | `ADAPTER`                  | Niski później                                                                                                 |
| **color management**               | obecny format/codec path jest zasadniczo sRGB                                                                                           | SkColorSpace sRGB                                                               | `SHIM`                     | Niski dla parity obecnej wersji                                                                               |
| **Sparkle**                        | macOS updater/app delegate                                                                                                              | aktualizacja Flatpak                                                            | `REWRITE` platform feature | praktycznie zero kodu aplikacji na Linux                                                                      |

Qt ma własną obsługę clipboardu, MIME DnD i zdarzeń tabletowych; `QTabletEvent` przekazuje m.in. pressure i tilt.

---

# 2. Co można zachować w Swift

Podział jest ważniejszy niż liczba `import` w obecnych plikach. Część plików importuje SwiftUI/AppKit tylko dlatego, że korzysta z jednego typu.

## A. Portable Swift — zachować prawie bez zmian

Najbardziej wartościowa część portu.

Należą tutaj przede wszystkim:

* model dokumentu i warstw,
* transformacje i snapping,
* grupy i hierarchia,
* history/undo/redo,
* logika selekcji,
* logika narzędzi,
* adjustment state,
* krzywe/levels/hue-saturation math,
* project workspace,
* guided matte algorithm,
* mask/link/group semantics.

`DocumentHistory.swift` ma logikę historii niezależną od GUI; jego główna platformowa zależność to typ obrazów.

`LayerTransform.swift` jest zasadniczo algorytmem geometrii opartym na typach CoreGraphics — idealny przypadek dla shima.

`GuidedMatte.swift` implementuje właściwy algorytm w Swift i potrzebuje jedynie zastąpienia dostępu do bitmap.

### Strategia

```text
existing Swift logic
       ↓
CGPoint/CGImage/etc compatibility names
       ↓
Linux implementation
```

Nie zmieniać nazw i typów w całym projekcie tylko po to, żeby wyglądały bardziej „cross-platform”.

---

## B. Platform-dependent Swift

Przykłady:

* `ProjectController.swift`
* platformowe fragmenty `ProjectStore.swift`
* `ImageImporter.swift`
* `ImageExporter.swift`
* `ImageFileDrop.swift`
* `SelectionClipboard.swift`
* `CompositorApplicationDelegate.swift`

`ProjectStore` jest dobrym przykładem pliku mieszanego: manifest, Codable, walidacja formatu i duża część ścieżek są przenośne, a `NSFileCoordinator`, pakietowy zapis i ImageIO są platformowe.

**Nie przepisywać całego `ProjectStore`.** Wyciągnąć tylko:

```text
ProjectPackageIO
PlatformImageCodec
```

---

## C. UI-dependent Swift

Najmniej sensowne do shimowania:

* `CompositorApp.swift`
* `ContentView.swift`
* `UI/*.swift`
* AppKit shell w `EditorCanvas.swift`
* `NativeLayerList.swift`
* `ProjectWindowBridge.swift`

`NativeLayerList.swift` jest silnie AppKitowy: `NSTableView`, `NSScrollView`, pasteboard, custom cells i drag-and-drop.

Te pliki **pozostawiłbym bez zmian dla macOS**.

Linux dostaje nowe Qt UI.

To oznacza:

```text
macOS:
UI/*.swift     -> SwiftUI/AppKit

Linux:
linux/ui/*.cpp -> Qt Widgets

oba:
                 ↓
          ten sam Swift core
```

---

## D. Rendering-dependent Swift

Najważniejsze:

* `LayerRenderer.swift`
* `TiledLayerRenderer.swift`
* `LiveMaskRenderer.swift`
* `RasterSnapshot.swift`
* `DownsampleCache.swift`
* renderingowa część `EditorCanvas.swift`
* bitmapowe fragmenty brush/mask/selection/filter code.

To nie powinno być przepisane na Skia call-by-call w każdym pliku.

**Kluczowy mechanizm portu: CoreGraphicsCompat.**

---

# 3. CoreGraphicsCompat — największa dźwignia całego portu

Zamiast zmieniać:

```swift
CGImage
CGContext
CGPoint
CGRect
CGAffineTransform
CGBlendMode
```

na nowe nazwy w kilkudziesięciu miejscach, Linux build powinien oferować minimalny moduł o **tych samych lub prawie tych samych nazwach**.

Przykładowo:

```swift
#if canImport(CoreGraphics)
import CoreGraphics
#else
import CoreGraphicsCompat
#endif
```

Jeszcze lepiej: Linux build może automatycznie wstrzykiwać moduł tak, aby w wielu plikach nawet ten `#if` nie był potrzebny.

## Minimalny zakres shima

```text
CGPoint
CGSize
CGRect
CGAffineTransform

CGImage
CGContext
CGPath

CGBlendMode
CGInterpolationQuality
CGColorSpace

saveGState / restoreGState
translate / rotate / scale / concat
clip / clip(to:mask:)
draw(image:)
clear
fill
alpha
blendMode
interpolation
makeImage
```

Tylko API **rzeczywiście wymagane przez Compositor**.

Nie implementować klona CoreGraphics.

### Backend

```text
Swift CoreGraphicsCompat
          │
          ↓ C ABI
     SkiaBridge.cpp
          │
          ↓
     SkCanvas / SkSurface
       ┌───────┴────────┐
       ↓                ↓
   Vulkan surface    Raster surface
```

Skia ma zarówno oficjalne raster surfaces, jak i backend Vulkan. `SkSurfaces::Raster` daje bezpośrednio CPU-backed `SkCanvas`; backend Vulkan jest wspierany na Linuxie przez `skia_use_vulkan=true`.

## Ważna decyzja: dokument pozostaje CPU-addressable

Nie przenosić stanu dokumentu do GPU.

```text
document pixels
    ↓
CPU RGBA8 / Gray8
    ↓
CGImageCompat / SkImage
    ↓
Skia uploads/cache when needed
```

To daje cztery korzyści:

1. istniejące C nadal dostaje zwykłe wskaźniki do pamięci,
2. save/load/undo nie zależą od GPU,
3. Vulkan → Raster fallback jest prosty,
4. brak kosztownego readbacku GPU dla każdej operacji C.

**GPU jest backendem renderowania, a nie właścicielem modelu dokumentu.**

To jedna z najważniejszych decyzji minimal-change.

---

# 4. Istniejące C

Bridging header eksportuje osiem modułów C.

Audyt nie pokazał w nich zależności od frameworków Apple.

| Moduł          | Obecna funkcja                     | Decyzja Linux |
| -------------- | ---------------------------------- | ------------- |
| `AdjustPixels` | adjustments / gradient map / grain | **KEEP 1:1**  |
| `BrushPixels`  | bitmap/alpha helpers               | **KEEP 1:1**  |
| `HealPixels`   | healing/patch processing           | **KEEP 1:1**  |
| `LevelsPixels` | histogram/levels                   | **KEEP 1:1**  |
| `WandPixels`   | flood-fill/magic wand              | **KEEP 1:1**  |
| `NoisePixels`  | noise generation                   | **KEEP 1:1**  |
| `LensPixels`   | radial distortion/bilinear sample  | **KEEP 1:1**  |
| `ContentFill`  | content-aware fill                 | **KEEP 1:1**  |

`HealPixels.c` używa zwykłych `math.h`, `stdlib.h`, `string.h` oraz własnych algorytmów.

`LensPixels.c` to również zwykłe C z lokalną interpolacją biliniową.

Pozostałe moduły mają ten sam charakter.

### Linux build

```text
clang -std=c11
existing *.c
existing *.h
-lm
```

Plus module map/C target importowany przez Swift.

### Nie robić

```text
HealPixels -> OpenCV rewrite
WandPixels -> OpenCV rewrite
LensPixels -> cv::remap rewrite
NoisePixels -> OpenCV RNG
```

Nie daje to korzyści dla celu portu.

---

# 5. Metal → Vulkan: rzeczywisty zakres

Potwierdzony Metal path to `MetalBrushCoverage.swift`.

Ma już znaczenie pomocnicze względem brush algorithm.

Dlatego kolejność powinna być:

```text
Linux v1
Brush logic
   ↓
existing CPU implementation
```

dopiero potem, jeśli profiling pokaże potrzebę:

```text
BrushCoverageAccelerator
        │
        ├── macOS Metal
        │
        └── Linux Vulkan Compute
```

To jest jeden z niewielu przypadków, gdzie **direct Vulkan** jest uzasadniony.

Nie przepisywać tego na Skia, jeśli Skia nie oferuje tej specjalistycznej funkcji.

Nie robić natomiast:

```text
LayerRenderer.swift -> VkCommandBuffer/VkImage/VkPipeline
```

Skia ma ukryć tę warstwę.

---

# 6. Mandatory CPU failsafe

Kontrakt powinien wyglądać tak:

```text
RendererDevice
 ├── SkiaVulkanDevice
 └── SkiaRasterDevice
```

A start aplikacji:

```text
RendererDeviceFactory.create()
       │
       ↓
try Vulkan
       │
       ├─ OK ───→ SkiaVulkanDevice
       │
       └─ fail ─→ SkiaRasterDevice
```

Fallback obejmuje:

* brak fizycznego urządzenia,
* brak wymaganej queue family,
* błąd tworzenia `VkDevice`,
* błąd swapchain,
* niezgodny sterownik,
* brak ICD,
* błędy integracji Flatpak/driver,
* device lost.

`QVulkanWindow` potrafi zarządzać device/graphics queue/swapchain i zawiera obsługę scenariuszy device-lost; można go również osadzić w QWidget UI.

Jednocześnie Skia przy imporcie Vulkanowego `VkImage` wymaga poprawnego zewnętrznego synchronization/layout management.

Dlatego Stage 8 musi mieć osobny spike:

```text
Qt swapchain image
     ↕ synchronization
Skia Vulkan surface
```

Jeżeli `QVulkanWindow + Skia` okaże się zbyt kruche, wymieniany jest wyłącznie:

`SkiaVulkanDevice.cpp`

na własny mały `QWindow + VkSwapchainKHR`.

**Nie zmienia to Swift, LayerRenderer ani UI.**

---

# 7. Qt Widgets kontra Qt Quick/QML

## Decyzja: Qt Widgets

| Kryterium                    | Qt Widgets                     | Qt Quick/QML                              |
| ---------------------------- | ------------------------------ | ----------------------------------------- |
| desktop menus/toolbars       | naturalne                      | dodatkowa QML warstwa                     |
| panels/dialogs               | naturalne                      | konieczny declarative model               |
| layer tree                   | QTreeView pasuje bardzo dobrze | dodatkowy QML model/delegate              |
| custom raster/Vulkan canvas  | QWidget/QWindow                | trzeba integrować ze scene graph          |
| mapping z obecnego AppKit UI | bezpośredni                    | dalszy semantycznie                       |
| bridge do Swift              | proste callbacks               | Q_PROPERTY/QML registrations/model bridge |
| nowy kod                     | **mniej**                      | więcej                                    |
| stan aplikacji               | pozostaje w Swift              | ryzyko duplikacji w QML                   |

Qt Quick byłby uzasadniony, gdyby projekt celował w nowy, animowany/deklaratywny UI.

Tutaj cel jest odwrotny.

### Mapping UI

```text
NSWindow / SwiftUI Window   -> QMainWindow
menus                       -> QAction + QMenu
toolbar                     -> QToolBar
floating panel              -> QDockWidget / QDialog
layers                      -> QTreeView
tabs                        -> QTabBar / QTabWidget
canvas                      -> QWidget / QWindow
slider                      -> QSlider
numeric control             -> QDoubleSpinBox
clipboard                   -> QClipboard
drag/drop                   -> QMimeData / QDrag
```

---

# 8. Stage 0 — Flatpak jako fundament

## 8.1 Runtime

Baseline:

```yaml
runtime: org.freedesktop.Platform
runtime-version: '26.08'
sdk: org.freedesktop.Sdk
```

Aktualna linia Freedesktop SDK to 26.08; point release `26.08.1` został opublikowany 15 września 2026 i m.in. zaktualizował Mesa do 26.2.2.

W manifeście pozostaje branch:

```text
26.08
```

a nie `26.08.1`.

## 8.2 Swift

Baseline: **Swift 6.4**, oficjalnie wydany 15 września 2026. Swift 6.4 dodatkowo rozwija Linux tooling oraz interoperacyjność C/C++.

Nie uzależniałbym pierwszego portu od niezweryfikowanej, zewnętrznej Swift SDK extension.

### Baseline Stage 0

```text
sdk-extensions: []
```

Swift 6.4 Linux toolchain jest osobnym, przypiętym źródłem Flatpak build module:

```text
official Swift 6.4 Linux archive
        ↓
unpack inside builder
        ↓
swiftc / swift
        ↓
build CompositorCore
        ↓
copy required Swift runtime libs to /app/lib
```

Toolchain jako compiler nie trafia do końcowego runtime.

Jeżeli utrzymywana `org.freedesktop.Sdk.Extension.swift*` dla branch 26.08 zostanie później zweryfikowana, można ją podstawić bez zmiany źródeł.

## 8.3 Qt

Aktualnym stabilnym, opublikowanym wydaniem jest **Qt 6.11.2** z 18 sierpnia 2026; `6.11.3` było dopiero planowane. Oficjalny download index nadal pokazuje 6.11.2 jako najnowszy dostępny release i ma późniejsze security patches dla 6.11.

Baseline:

```text
Qt 6.11.2
+ aktualne security patches z 6.11 branch
```

Minimalne moduły:

```text
qtbase
 ├── Core
 ├── Gui
 ├── Widgets
 ├── DBus
 └── xcb platform plugin

qtwayland
qtsvg        jeśli potrzebne SVG
qtimageformats opcjonalnie później
```

Flatpak oficjalnie dopuszcza użycie `org.freedesktop.Platform` jako podstawy i bundlowanie tylko potrzebnych części Qt.

## 8.4 Skia

Skia nie ma typowego semver. Oficjalny model to regularne milestone branches; dla reprodukowalnego Flatpaka należy przypiąć **konkretny commit ze stabilnego milestone**, nie śledzić `main`.

Stage 0 powinien zapisać:

```text
SKIA_COMMIT=<tested SHA>
```

po przejściu testu Raster + Vulkan na Freedesktop 26.08.

Wymagane:

```text
skia_use_vulkan=true
```

i oczywiście raster.

## 8.5 OpenCV

Na dzień audytu są dostępne OpenCV 5.0.0 oraz 4.14.0.

Dla minimalnego portu wybrałbym początkowo:

```text
OpenCV 4.14.0
```

jako konserwatywny branch 4.x, ponieważ OpenCV będzie izolowany za adapterem i nie jest rendererem.

Budować wyłącznie potrzebne moduły:

```text
core
imgproc
```

oraz później:

```text
dnn
```

tylko jeżeli Linuxowy Remove Background tego wymaga.

## 8.6 Flatpak permissions

Baseline:

```yaml
finish-args:
  - --share=ipc
  - --socket=wayland
  - --socket=fallback-x11
  - --device=dri
```

To jest standardowy układ native Wayland + XWayland fallback + GPU. `--device=dri` udostępnia GPU graphics i compute devices.

**Nie dodawać:**

```text
--filesystem=home
--filesystem=host
--share=network
```

bez konkretnej potrzeby.

Sparkle nie istnieje w Linux build, więc aplikacja nie potrzebuje sieci do sprawdzania aktualizacji.

## 8.7 Mesa/Vulkan

Nie bundlować własnego sterownika Mesa ani ICD.

```text
Flatpak Freedesktop runtime
      +
host-compatible GL/Vulkan driver extension
      +
--device=dri
```

Aplikacja bundluje bibliotekę Skia i swoje dependencies, ale korzysta z runtime'owego Vulkan loadera i odpowiednich driver extensions.

## 8.8 Portals

Qt ma własny `xdgdesktopportal` platform-theme file dialog backend; jego implementation woła `org.freedesktop.portal.Desktop`.

Dlatego pierwsza implementacja `PlatformFileDialog` może być bardzo cienka:

```text
PlatformFileDialog
      ↓
QFileDialog
      ↓
Qt xdgdesktopportal helper
      ↓
FileChooser portal
```

Dokumentacja Flatpak wprost zaleca dla Qt używanie `QFileDialog` i niewymuszanie `DontUseNativeDialog`; Qt/KDE potrafi transparentnie użyć portali w sandboxie.

FileChooser portal może nadać sandboxowi trwały dostęp do wybranego dokumentu przez Documents portal.

### Ważne ryzyko `.comp`

Aktualny `.comp` jest **directory package** z `manifest.json` i `images/`, a nie pojedynczym plikiem.

Portal umie wybierać katalog (`directory=true`), a Documents portal obsługuje również eksport directories.

Natomiast zachowanie **Save As dla nowego package directory** trzeba sprawdzić na żywo na GNOME i KDE.

To powinien być Stage-0/Stage-7 risk spike.

Nie zmieniałbym formatu `.comp` na ZIP, dopóki test nie pokaże, że portal nie potrafi bezpiecznie obsłużyć obecnego modelu.

## 8.9 Clipboard / DnD

Clipboard nie wymaga osobnego custom portalu:

```text
PlatformClipboard -> QClipboard/QMimeData
```

Qt udostępnia clipboard systemowy i używa tego samego `QMimeData` co DnD.

`ImageFileDrop.swift` pokazuje, że obecny kod rozróżnia dropped file URL oraz dane obrazu z innych aplikacji. To zachowanie należy zachować w adapterze Qt.

## 8.10 MIME/Desktop

Nowe pliki:

```text
linux/flatpak/com.wonderassembly.compositor.yml

linux/desktop/
  com.wonderassembly.compositor.desktop
  com.wonderassembly.compositor.metainfo.xml
  com.wonderassembly.compositor.xml      # shared-mime-info
```

Ikony: użyć istniejących PNG z `Assets.xcassets`, nie tworzyć nowego zestawu. Repo zawiera gotowe rozmiary 16–1024 px.

### Definition of Done Stage 0

```text
flatpak run com.wonderassembly.compositor
```

uruchamia executable, który potrafi:

* wykonać kod Swift,
* wywołać istniejącą funkcję C,
* stworzyć QApplication,
* utworzyć Skia Raster surface,
* znaleźć Vulkan loader i spróbować enumerować urządzenia,
* załadować OpenCV,
* działać także wtedy, gdy nie ma urządzenia Vulkan.

**Bez jeszcze istniejącego UI Compositor.**

---

# 9. Granica Swift ↔ C++

Nie wystawiać typów Qt/Skia do Swift.

Najstabilniejsza granica:

```c
typedef struct CompImage CompImage;
typedef struct CompCanvas CompCanvas;
typedef struct CompRenderer CompRenderer;
```

i mały zestaw `extern "C"`.

```text
Swift
 ↓
C ABI
 ↓
C++ implementation
 ├── Qt
 └── Skia/Vulkan
```

Swift 6.4 dalej rozwija C/C++ interoperability, ale port nie powinien być uzależniony od najbardziej złożonego reverse Swift→C++ modelu.

### Dlaczego C ABI jest właściwe

* zero C++ template types w Swift,
* zero Qt headers w core,
* łatwe testowanie,
* stabilność ABI,
* można wymienić Skia/Vulkan bez zmian Swift,
* można zachować istniejące C dokładnie tak jak dziś.

---

# 10. Kolejność implementacji

## Stage 0 — Flatpak / Freedesktop

**Zakres:** manifest, dependency builds, stub executable.

**Existing bez zmian:** cały repo.

**Modified:** zero istniejących plików źródłowych.

**Nowe:**

```text
linux/flatpak/*
linux/CMakeLists.txt
linux/bootstrap/*
linux/main/stub.cpp lub stub.swift
```

**Typ:** build/platform addition.

**Dependencies:** Freedesktop 26.08, Swift 6.4, Qt 6.11.2, pinned Skia, Vulkan loader, OpenCV.

**Ryzyka:** Swift toolchain compatibility; Skia offline dependency vendoring; `.comp` directory portal semantics.

**CPU/GPU:** CPU obowiązkowy, GPU niewymagany.

**DoD:** opisany wyżej dependency smoke test.

---

## Stage 1 — Swift + C + C++ interoperability

**Zakres:** docelowa granica ABI.

**Existing bez zmian:** cały model i algorytmy.

**Modified:** ewentualnie bridging/build configuration, nie logika.

**Nowe:**

```text
linux/include/CompositorBridge.h
linux/platform/QtBridge.cpp
linux/swift/LinuxBridge.swift
```

**Model komunikacji:**

```text
Qt event
  ↓
C callback/POD
  ↓
Swift EditorSession

Swift render
  ↓
C CompCanvas*
  ↓
Skia
```

**Ryzyko:** ownership/refcount crossing ABI.

**DoD:** Swift tworzy session; C++ wywołuje Swift action; Swift wywołuje C++ graphics function; valgrind/ASan nie wykazuje leaków na boundary.

---

## Stage 2 — istniejące moduły C

**Zakres:** wszystkie osiem modułów.

**Existing bez zmian:** wszystkie `.c`/`.h`.

**Modified:** tylko jeśli compiler ujawni realny portability issue.

**Nowe:** module map/CMake target.

```text
CompositorPixels
```

**Dependencies:** libc, libm.

**GPU:** brak.

**DoD:** Linux Swift test wywołuje publiczne funkcje każdego modułu.

---

## Stage 3 — maksymalny Swift bez UI

**Zakres:** zbudować jak największą część `Document` + platform-neutral `IO` bez SwiftUI/AppKit.

**KEEP / minimal changes:**

* history,
* transforms,
* layer state,
* groups,
* selections,
* adjustment state,
* workspace,
* tool state,
* C call sites.

`EditorSession.swift` jest centralnym stanem/model-em i powinien zostać zachowany; jego zależność od SwiftUI należy redukować do Foundation/Observation zamiast przenosić model do Qt.

**Zmiany typowe:**

```swift
import SwiftUI
```

→

```swift
import Foundation
import Observation
```

tam gdzie plik nie używa rzeczywistego SwiftUI.

**Nowe:** Linux source list/build target.

**DoD:** portowalne testy modelu/history/layers działają w Flatpaku bez Qt window.

---

## Stage 4 — CoreGraphicsCompat

**Zakres:** geometry + images + bitmap contexts.

**Existing użyte bez algorytmicznych zmian:**

* `LayerTransform.swift`
* `DocumentHistory.swift`
* duża część masks/selections/brush code.

**Existing changes:** zwykle import/conditional compilation.

**Nowe:**

```text
linux/swift/CoreGraphicsCompat/
  Geometry.swift
  Transform.swift
  Image.swift
  Context.swift
  Path.swift
  Color.swift

linux/graphics/
  SkiaBridge.h
  SkiaBridge.cpp
```

**GPU:** brak wymagania.

**DoD:** geometry, transform, bitmap, clipping i image tests przechodzą przez Linux shim.

---

## Stage 5 — Skia Raster CPU

To najważniejszy milestone techniczny.

**Existing do zachowania:**

* `LayerRenderer.swift`
* `TiledLayerRenderer.swift`
* `LiveMaskRenderer.swift`
* `RasterSnapshot.swift`
* renderująca logika masek/layers.

`LayerRenderer` używa ograniczonego zestawu semantyki `CGContext`, więc jest znacznie tańszy do zachowania niż przepisania.

**Modified:** minimalnie importy/API edge cases.

**Nowe:**

```text
SkiaRasterDevice.cpp
SkiaPath.cpp
SkiaImage.cpp
PlatformResampler.cpp
```

`DownsampleCache` zachowuje cache/pyramid/LRU, wymienia się jedynie wywołanie vImage.

**CPU:** obowiązkowy.

**GPU:** zero.

**DoD:**

* headless render wielowarstwowego dokumentu,
* alpha/masks,
* transform,
* clipping,
* tiled render bez seams,
* blend mode tests,
* export do bitmapy,
* porównanie golden images z tolerancją tam, gdzie renderer różni się roundingiem.

---

## Stage 6 — minimalny Qt shell

**Zakres:**

```text
window
canvas
keyboard
mouse
basic menu
```

**Existing nie zmieniać:**

* `CompositorApp.swift`
* `ContentView.swift`
* `UI/*.swift`

Pozostają macOS-only.

**Mixed file:** `EditorCanvas.swift`.

Ten plik ma ~109 KB i miesza renderer orchestration z `NSView`/`NSEvent`/cursor logic.

Nie przepisywać całego pliku.

Wydzielić tylko to, co naprawdę wspólne:

```text
CanvasSceneRenderer.swift
```

jeśli dzięki temu oba frontendy mogą korzystać z istniejącej logiki renderowania.

Nowe:

```text
linux/ui/MainWindow.cpp
linux/ui/CanvasWidget.cpp
linux/ui/InputAdapter.cpp
```

**DoD:**

* blank document,
* render Raster,
* mouse,
* keyboard,
* zoom,
* pan,
* native Wayland,
* XWayland fallback.

---

## Stage 7 — dokumenty, layers, rendering

**Zakres:**

* project tabs,
* layer tree,
* basic dialogs,
* import PNG/JPEG,
* save/load `.comp`,
* masks,
* opacity/blend modes.

**Qt:**

```text
NativeLayerList behavior
       ↓
QTreeView
+ QAbstractItemModel
+ delegate
```

Nie przenosić layer modelu do `QAbstractItemModel`.

Qt model ma być tylko view-adapterem do Swift `EditorSession`.

**ProjectStore:** zachować manifest/validation; wymienić package IO/codec.

**DoD:**

* utworzenie dokumentu,
* kilka warstw,
* reorder,
* hide/show,
* opacity,
* mask,
* save,
* close,
* reopen,
* identyczna semantyka dokumentu.

---

## Stage 8 — Skia + Vulkan

**Zakres:** nowy backend `SkiaVulkanDevice`.

Żadnych zmian w:

```text
LayerRenderer
TiledLayerRenderer
Document
Tools
```

Idealny diff Stage 8 powinien być niemal wyłącznie:

```text
linux/graphics/SkiaVulkanDevice.cpp
RendererDeviceFactory.cpp
Canvas Vulkan presentation integration
```

**Startup:**

```text
try Vulkan
catch / fail
→ Raster
```

**Runtime loss:**

```text
device lost
→ stop GPU submissions
→ discard GPU-only caches
→ create Raster device
→ redraw from CPU-backed document state
```

**DoD:**

* ten sam fixture Raster/Vulkan,
* brak zmian higher-level source,
* uruchomienie bez `/dev/dri` nadal działa,
* błędny/wyłączony Vulkan ICD nadal działa,
* fallback można wymusić testowo.

---

## Stage 9 — kolejne narzędzia

Kolejność według najmniejszego ryzyka:

```text
move / zoom / pan
transform
layers/groups
masks
brush CPU
selection
magic wand
adjustments
filters
healing
content-aware
clone/smudge/liquify
```

Nie zaczynać od GPU brush acceleration.

Obecny brush code ma CPU ścieżkę, więc Linux v1 nie musi implementować `MetalBrushCoverage` equivalent od razu.

**DoD:** istniejące unit tests kolejnych narzędzi przechodzą na Linux core.

---

## Stage 10 — OpenCV

OpenCV dodawać **dopiero per operation**.

### KEEP existing C

```text
noise          -> NoisePixels
lens           -> LensPixels
healing        -> HealPixels
magic wand     -> WandPixels
content fill   -> ContentFill
```

`Filters.swift` już dziś korzysta z istniejącego C dla części operacji; należy zachować ten podział.

### Skia first

Próbować Skia dla:

* Gaussian blur,
* mask filters,
* resize/warp, jeśli semantyka pasuje,
* compositing.

### OpenCV candidates

* motion blur, jeśli Skia wymaga dużo custom code,
* morphology/refinement,
* selected geometric warp,
* segmentation postprocess,
* DNN subject segmentation.

### Subject Removal

To najtrudniejsza pojedyncza funkcja parity.

Aktualnie używa Apple Vision `VNGenerateForegroundInstanceMaskRequest`.

Linux:

```text
SubjectRemoval
      ↓
SubjectSegmentationBackend
      ↓
OpenCV DNN / model
      ↓
existing GuidedMatte
```

Czyli rewrite dotyczy **tylko generatora coarse mask**, nie refinement ani reszty funkcji.

**DoD:** każda zależność OpenCV ma uzasadnienie „mniej kodu / lepsza parity”, a OpenCV nie posiada żadnego canvas backendu.

---

## Stage 11 — pełna integracja Flatpak/XDG

Zakres:

* FileChooser portal,
* persistent document access,
* Save As,
* directory `.comp` tests,
* clipboard,
* image DnD,
* URI/file activation,
* Wayland,
* XWayland,
* HiDPI,
* `.desktop`,
* AppStream metadata,
* MIME,
* icons.

**DoD:**

```text
flatpak run
```

z brakiem szerokich filesystem permissions obsługuje normalny workflow użytkownika.

Test matrix:

```text
GNOME Wayland
KDE Plasma Wayland
X11
XWayland fallback
Vulkan Intel/AMD
Vulkan NVIDIA Flatpak driver extension
forced CPU mode
```

---

## Stage 12 — feature parity

Na koniec:

* HEIC,
* TIFF,
* exact JPEG metadata/resolution,
* Remove Background,
* remaining filters,
* custom cursor parity,
* remaining floating panels,
* layer DnD between projects,
* tablet pressure/tilt, jeśli ma być funkcją,
* performance parity,
* opcjonalny Vulkan brush compute.

**DoD:** jawna feature-parity matrix względem macOS 1.0.4, z tylko udokumentowanymi platform differences.

---

# 11. Image codecs

## PNG/JPEG

Pierwszy wybór:

```text
PlatformImageCodec
       ↓
Skia codec
```

Obecny `ImageImporter` używa ImageIO/CoreImage m.in. do EXIF orientation i normalizacji do RGBA8/sRGB.

Interfejs powinien zwracać dokładnie to, czego potrzebuje core:

```text
width
height
RGBA8
orientation-normalized pixels
sRGB
```

Bez ujawniania Skia.

## HEIC/TIFF

Nie zakładać, że Skia zapewni oba.

Po Stage 7 wybrać najtańszy codec backend:

```text
TIFF -> QtImageFormats lub libtiff
HEIC -> libheif
```

za tym samym `PlatformImageCodec`.

To jest lepsze niż rozbudowywanie shima ImageIO.

---

# 12. Project format / filesystem

Obecny format `.comp` zawiera manifest i immutable image/mask assets; model jest dobrze izolowany od UI.

Dlatego:

**KEEP:**

```text
manifest schema
validation
layer IDs
image naming
mask naming
group semantics
version handling
limits
```

**REPLACE tylko:**

```text
FileWrapper
NSFileCoordinator
security-scoped URL
```

Linux:

```text
ProjectStore
    ↓
ProjectPackageIO
    ↓
Foundation FileManager + portal-authorized path
```

Jeżeli directory package naprawdę okaże się niepraktyczny w portalach, dopiero wtedy rozważyć:

```text
.comp single-file archive
    zawierające:
      manifest.json
      images/...
```

ale storage container powinien być wówczas tylko Linux-specific `ProjectPackageIO`, a manifest i core pozostają bez zmian.

To jest **plan B**, nie baseline.

---

# 13. Minimalna warstwa platformowa

Nie potrzeba frameworka z dziesiątkami klas.

Wystarczy około siedmiu granic:

```text
PlatformApplication
PlatformFileDialog
PlatformClipboard
PlatformImageCodec
PlatformCanvas
PlatformInput
PlatformPaths
```

plus:

```text
RendererDevice
```

Nie tworzyłbym:

```text
AbstractPlatformServiceFactory
AbstractPlatformRenderManager
BasePlatformBackendProvider
...
```

### Pragmatyczny SOLID

**DIP:** Swift core nie importuje Qt/Skia/Vulkan.

**ISP:** codec nie zna clipboardu; canvas nie zna file chooser.

**OCP:** `SkiaVulkanDevice ↔ SkiaRasterDevice` za jednym kontraktem.

**SRP:** Qt adapter przekazuje input, nie przetwarza bitmap.

**LSP:** Raster realizuje dokładnie ten sam render contract co Vulkan.

Ale:

```swift
#if os(Linux)
...
#endif
```

jest całkowicie akceptowalne dla kilku drobnych platform differences.

---

# 14. Docelowa struktura nowych plików

Przykładowo:

```text
linux/
├── flatpak/
│   └── com.wonderassembly.compositor.yml
├── desktop/
│   ├── com.wonderassembly.compositor.desktop
│   ├── com.wonderassembly.compositor.metainfo.xml
│   └── com.wonderassembly.compositor.xml
├── include/
│   ├── CompositorBridge.h
│   └── SkiaBridge.h
├── graphics/
│   ├── SkiaBridge.cpp
│   ├── SkiaRasterDevice.cpp
│   ├── SkiaVulkanDevice.cpp
│   └── RendererDeviceFactory.cpp
├── platform/
│   ├── QtApplication.cpp
│   ├── QtFileDialog.cpp
│   ├── QtClipboard.cpp
│   ├── QtInput.cpp
│   └── QtPaths.cpp
├── ui/
│   ├── MainWindow.cpp
│   ├── CanvasWidget.cpp
│   ├── LayersView.cpp
│   ├── ToolBar.cpp
│   └── dialogs/...
└── swift/
    ├── CoreGraphicsCompat/
    ├── LinuxPlatformServices.swift
    └── LinuxUIBridge.swift
```

Nie tworzyć osobnego C++ odpowiednika katalogu `Document`.

---

# 15. Macierz migracji

| Existing                     | GNU/Linux                                   | Strategia po audycie                            |
| ---------------------------- | ------------------------------------------- | ----------------------------------------------- |
| SwiftUI                      | Qt Widgets                                  | nowy Linux UI; macOS SwiftUI bez zmian          |
| AppKit                       | Qt/XDG                                      | cienkie adaptery + nowy shell                   |
| CoreGraphics                 | CoreGraphicsCompat + Skia                   | **SHIM**, klucz do minimal-change               |
| CoreImage                    | Skia/OpenCV per operation                   | adapter, nie pełny shim                         |
| ImageIO                      | PlatformImageCodec                          | adapter                                         |
| UniformTypeIdentifiers       | MIME + tiny compatibility                   | shim/adapter                                    |
| MetalBrushCoverage           | CPU istniejący → opcjonalnie Vulkan Compute | backend replacement tylko dla tej funkcji       |
| brak Metal canvas renderer   | Skia Vulkan                                 | nie wykonywać fikcyjnej migracji Metal renderer |
| brak Vulkan / failure        | Skia Raster                                 | obowiązkowy failsafe                            |
| Accelerate/vImage            | PlatformResampler/Skia                      | mały wrapper                                    |
| Apple Vision                 | segmentation backend                        | adapter; parity później                         |
| istniejące C                 | istniejące C                                | **KEEP**                                        |
| selected image processing    | OpenCV                                      | tylko po pomiarze kosztu                        |
| macOS dialogs                | QFileDialog/XDG portal                      | adapter                                         |
| NSPasteboard                 | QClipboard/QMimeData                        | adapter                                         |
| NSWindow/NSEvent             | Qt                                          | Linux-specific shell                            |
| `.comp` manifest/schema      | ten sam format logiczny                     | KEEP                                            |
| package filesystem machinery | portal-aware package IO                     | adapter                                         |
| Sparkle                      | Flatpak update                              | usunięte z Linux build                          |
| DMG/notarization             | Flatpak                                     | replacement                                     |

---

# 16. Najważniejsze testy migracyjne

Istniejące repo ma już szerokie testy brush, transforms, masks, layers, selections, filters, import/export, history, tiled rendering itd.

To powinien być główny **behavior oracle** portu.

## Test pyramid

```text
existing unit tests
      ↓
Linux core tests
      ↓
Skia Raster render parity
      ↓
Raster ↔ Vulkan parity
      ↓
Qt interaction smoke tests
      ↓
Flatpak portal/integration tests
```

Nie przepisywać wszystkich XCUITestów 1:1.

Qt UI tests powinny sprawdzać przede wszystkim:

* otwarcie okna,
* podstawowe command routing,
* keyboard shortcuts,
* mouse interactions,
* layers DnD,
* dialogs.

---

# 17. Minimal-change map

## 17.1 KEEP / praktycznie bez zmian

Z wysoką pewnością:

```text
Rendering/*.c
Rendering/*.h
```

wszystkie osiem istniejących algorytmów C.

Z wysoką lub średnio-wysoką pewnością zachowana zostaje **większość algorytmicznego Swift** pod `Document/`, w szczególności:

```text
DocumentHistory
LayerTransform
LayerGroups
CanvasSize
CloneStamp logic
Crop logic
Curves logic
Gradient logic
GuidedMatte
Levels logic
Mask tracing/state
Selection state/geometry
ProjectWorkspace
```

Mogą wymagać importu shima, ale nie rewrite'u algorytmu.

---

## 17.2 Conditional compilation / bardzo małe zmiany

Typowo:

```text
EditorSession.swift
BrushStroke.swift
ColorPalette.swift
LayerAppearance.swift
EditorSession+*.swift
```

Przykład:

```swift
#if canImport(AppKit)
...
#endif
```

dla `NSColor`, Metal accelerator, sound/cursor helpers itp.

---

## 17.3 SHIM

Największa grupa oszczędności:

```text
LayerRenderer.swift
TiledLayerRenderer.swift
LiveMaskRenderer.swift
RasterSnapshot.swift
LayerTransform.swift
mask/selection rendering code
brush bitmap operations
```

Shim:

```text
CGImage
CGContext
CGPoint/Size/Rect
CGAffineTransform
CGPath
CGBlendMode
interpolation
color spaces
```

---

## 17.4 ADAPTER

Nie opłaca się emulować frameworka:

```text
ImageImporter.swift       -> PlatformImageCodec
ImageExporter.swift       -> PlatformImageCodec
ProjectStore.swift        -> ProjectPackageIO
ProjectController.swift   -> PlatformFileDialog/application
SelectionClipboard.swift  -> PlatformClipboard
DownsampleCache.swift     -> PlatformResampler
Filters.swift             -> per-operation backend
PixelAdjust.swift         -> image operation adapter
SubjectRemoval.swift      -> segmentation backend
```

---

## 17.5 Existing macOS files pozostawione, nie portowane w miejscu

To ważne: „Linux rewrite UI” nie oznacza usuwania obecnego UI.

```text
CompositorApp.swift
ContentView.swift
UI/*.swift
CompositorApplicationDelegate.swift
AppKitowa część EditorCanvas.swift
```

pozostają działającą implementacją macOS.

Linux ma równoległy Qt shell.

---

## 17.6 Rzeczywiście konieczny rewrite

Najmniejszy możliwy zakres:

1. Linux Qt UI/presentation layer.
2. AppKit input/window shell.
3. Vision foreground segmentation backend.
4. opcjonalnie Metal compute accelerator → Vulkan compute.

**Nie ma powodu przepisywać modelu, undo, layers, selections ani C algorithms.**

---

# 18. Minimal Linux v1

Najmniejsza wersja, którą uznałbym już za realnie używalny Compositor:

### Platform

* Flatpak/Freedesktop 26.08,
* Qt Widgets,
* native Wayland,
* XWayland fallback.

### Rendering

* Skia Raster — zawsze,
* Skia Vulkan — preferowany jeśli działa,
* automatyczny fallback.

### Dokumenty

* create/open/save `.comp`,
* PNG/JPEG import,
* PNG/JPEG export,
* multi-layer,
* groups,
* visibility,
* opacity,
* podstawowe blend modes,
* raster masks.

### Editing

* undo/redo,
* pan/zoom,
* move,
* scale,
* rotate,
* flip,
* brush na obecnym CPU algorithm,
* rectangle/ellipse/lasso/magic-wand selections,
* basic mask editing,
* layers reorder.

### Image processing

Tam gdzie istniejący kod praktycznie „przychodzi za darmo”:

* Levels,
* curves,
* exposure/adjustments,
* noise,
* lens correction,
* existing healing,
* existing content-aware fill.

### Można odłożyć poza v1

* Remove Background / Vision parity,
* HEIC,
* TIFF, jeżeli codec integration okaże się nietrywialne,
* Vulkan brush compute,
* perfekcyjne odwzorowanie wszystkich custom cursorów,
* tablet pressure/tilt,
* drobne UI polish,
* wszystkie cross-project DnD edge cases.

To daje prawdziwy edytor, a nie „demo portu”.

---

# 19. Koszt migracji

Nie ma sensu podawać liczby typu „73,4%”, ponieważ repo nie zostało jeszcze przepuszczone przez Linux compiler ze wszystkimi shim headers.

## Zachowanie istniejącego kodu

### Existing C

**~95–100% zachowania**, pewność wysoka.

Najbardziej prawdopodobny wynik: 100% algorytmów, ewentualnie drobne build/header fixes.

### Swift algorytmiczny/modelowy

**~75–90% zachowania**, pewność średnio-wysoka.

Zwłaszcza:

* dokument,
* layers,
* history,
* selections,
* transforms,
* tool state,
* znaczna część image processing logic.

### Cały produkcyjny source razem z UI

Orientacyjnie **~60–75% istniejącego kodu może pozostać bez rewrite'u**, pewność średnia.

Spadek wynika przede wszystkim z tego, że obecna warstwa SwiftUI/AppKit musi dostać nowy Linuxowy odpowiednik.

Ale ważniejsza miara brzmi:

> **około 80–90% obecnej logiki domenowej i algorytmicznej powinno dać się zachować.**

---

# 20. Najdroższe obszary

W kolejności ryzyka/kosztu:

### 1. Qt UI + `EditorCanvas` input shell

Nie z powodu modelu, lecz liczby interakcji desktopowych.

### 2. CoreGraphics compatibility semantics

Nie chodzi o samo `drawImage`, lecz dokładność:

* premultiplied alpha,
* clipping,
* mask coordinate systems,
* transforms,
* interpolation,
* tiled seams,
* blend modes,
* path operations.

To jest najważniejszy shim do zrobienia dobrze.

### 3. Vulkan/Skia/window synchronization

Technicznie lokalne, ale wymagające.

### 4. file/project portal + directory `.comp`

Mało kodu, potencjalnie podstępna semantyka sandboxu.

### 5. Vision Remove Background

Największa funkcjonalność bez naturalnego 1:1 odpowiednika.

---

# 21. Gdzie shim oszczędza najwięcej zmian

## Zdecydowanie shimować

```text
CGPoint / CGSize / CGRect
CGAffineTransform
CGImage
CGContext
CGPath
CGBlendMode
CGInterpolationQuality
basic CGColorSpace
```

Koszt jednego shima jest niższy niż zmiana każdego renderera/narzędzia.

## Małe wrappers

```text
PlatformResampler
PlatformClipboard
PlatformPaths
ProjectPackageIO
```

## Adaptery

```text
PlatformImageCodec
PlatformFileDialog
RendererDevice
SubjectSegmentationBackend
```

---

# 22. Czego nie opłaca się shimować

Nie próbować implementować:

```text
SwiftUICompat
AppKitCompat
NSViewCompat
NSTableViewCompat
NSWindowCompat
CoreImageCompat
ImageIOCompat
VisionCompat
SparkleCompat
```

To byłoby więcej pracy niż małe platform-specific implementations.

Najlepszy przykład:

```text
NSOpenPanel
```

Nie pisać klasy imitującej `NSOpenPanel`.

Napisać:

```text
PlatformFileDialog.open(...)
    ↓
QFileDialog
```

---

# 23. Rekomendowany pierwszy branch implementacyjny

Najmniej ryzykowny pierwszy ciąg commitów:

```text
1. linux/ Flatpak skeleton
2. Swift 6.4 compiler inside Flatpak
3. existing C target
4. Linux core Swift compile target
5. CoreGraphicsCompat geometry only
6. CGImage/CGContext raster
7. Skia Raster headless
8. LayerRenderer
9. TiledLayerRenderer
10. golden render tests
11. Qt main window
12. raster canvas
13. layer view
14. open/save/import
15. Vulkan backend
```

**Nie implementować Vulkan wcześniej niż poprawny Raster renderer.**

To eliminuje ogromną liczbę zmiennych z debugowania.

---

# 24. Ostateczna rekomendacja architektoniczna

Najmniej zmian da wariant:

```text
macOS
─────────────────────────────────────────────
existing SwiftUI/AppKit
existing CoreGraphics/CoreImage
existing MetalBrushCoverage
existing C
        │
        └── existing Swift core


GNU/Linux
─────────────────────────────────────────────
new Qt Widgets shell
        │
        ↓
existing Swift core
        │
        ├── existing C 1:1
        │
        ├── CoreGraphicsCompat
        │          ↓
        │        Skia
        │      ┌───┴───┐
        │      ↓       ↓
        │   Vulkan   Raster
        │
        ├── PlatformImageCodec
        ├── PlatformFileDialog
        ├── PlatformClipboard
        └── selected OpenCV backend
```

Najważniejsze jest to, czego **nie** należy robić:

```text
nie: Swift -> C++ rewrite
nie: LayerRenderer -> direct Vulkan rewrite
nie: existing C -> OpenCV rewrite
nie: SwiftUI model -> Qt model rewrite
nie: CoreImage -> gigantyczny compatibility framework
nie: OpenCV canvas renderer
nie: drugi niezależny renderer CPU
```

Docelowo zarówno GPU, jak i CPU mają przechodzić przez **ten sam Skia-based rendering contract**.

---

# Trzy najważniejsze wyniki

## 1. Minimal-change map

**KEEP:** istniejące osiem modułów C i większość `Document` logic.

**Conditional:** `EditorSession`, brush accelerator selection, NSColor/platform helpers.

**SHIM:** cały obszar CoreGraphics image/geometry/canvas/path/blend.

**ADAPTER:** codecs, project package filesystem, file dialogs, clipboard, resampler, CoreImage-per-operation, Vision segmentation.

**Linux-specific rewrite:** wyłącznie GUI/AppKit shell, nie model.

Najbardziej wartościową pojedynczą inwestycją jest **CoreGraphicsCompat → Skia**.

---

## 2. Minimal Linux v1

Używalne v1 to:

```text
Flatpak
Qt Widgets
Wayland + XWayland
Skia Vulkan
Skia Raster failsafe

existing document/layer/history Swift
existing C

create/open/save
PNG/JPEG
layers/groups
masks
transform
zoom/pan
brush
selection
undo/redo
podstawowe filters/adjustments
```

Remove Background, HEIC/TIFF parity i GPU brush acceleration mogą wejść po pierwszej używalnej wersji.

---

## 3. Orientacyjny koszt

**Zachowanie całego istniejącego produkcyjnego source:** około **60–75%** bez rewrite'u, średnia pewność.

**Zachowanie modelu/logiki/algorytmów:** około **80–90%**, średnio-wysoka pewność.

**Istniejące C:** około **95–100%**, wysoka pewność.

Najdroższy będzie **nowy Qt UI**, nie core.

Największą redukcję liczby zmienionych linii daje:

```text
CG-shaped compatibility shim
+
CPU-backed document images
+
Skia backend abstraction
```

Najmniej opłaca się shimować AppKit, SwiftUI, CoreImage object graph, Vision i Sparkle — tam małe Linux-specific komponenty są tańsze.

### Konkluzja

Compositor jest **dobrym kandydatem do compatibility portu**, ponieważ duża część jego faktycznych algorytmów jest już oddzielona od UI, istniejące C jest przenośne, a Metal nie stanowi fundamentu głównego renderera.

Najbardziej zgodna z celem migracji architektura to:

> **zachować Swift + C, zachować obecne algorytmy renderowania na poziomie Swift, podstawić wąski CoreGraphics-compatible frontend nad Skia, a Qt ograniczyć do UI/platformy.**

Wtedy Vulkan staje się wymiennym backendem Skia, Raster niezawodnym failsafe, a Linux port pozostaje portem obecnego Compositor — a nie drugim, napisanym od nowa edytorem.
