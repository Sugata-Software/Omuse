# Compositor: głęboka analiza architektury macOS → GNU/Linux

Ten dokument odpowiada na 20-punktowy brief researchowy (patrz historia rozmowy/PR) dla aplikacji **Compositor**
(robbietilton.com/compositor). W odróżnieniu od typowego "research na ślepo" — w tym repozytorium znajduje się
**prawdziwy, niezmodyfikowany kod źródłowy** upstream (`Compositor/{Document,IO,Rendering,UI}`), a port na Linux
opisany w sekcjach 4–9, 15–16 i 20 **już istnieje, kompiluje się i przechodzi testy** (`Sources/Compat/*`,
`Sources/Overrides/*`, `backends/{brush,effects}/`, `host/`). Dokument nie proponuje więc innej architektury niż
już przyjęta (macOS-API-compatibility-layer + SOLID/DIP) — wyjaśnia i dokumentuje decyzje, które już zapadły i
zostały zweryfikowane testami.

**Legenda oznaczeń przy każdym twierdzeniu:**
- **[Fakt — kod]** — potwierdzone przez `grep`/odczyt rzeczywistego pliku w tym repo (ścieżka podana).
- **[Fakt — autor/strona]** — z `robbietilton.com/compositor` lub opisu repozytorium `github.com/robbietilton/Compositor`.
- **[Zrealizowane w porcie]** — nie hipoteza: ten port to implementuje, z testami.
- **[Hipoteza]** — rozsądny wniosek techniczny tam, gdzie rzeczywista implementacja macOS nie jest widoczna z
  samego kodu (np. dokładne działanie sterownika GPU) — nigdy nie podane jako fakt.
- **[Historyczne — nieaktualne]** — dane z wcześniejszych, przedforkowych dokumentów tego repo (patrz §21), które
  opisywały inną (już porzuconą) architekturę; cytowane wyłącznie jako kontekst historyczny.

---

## Kluczowa korekta wstępna: to nie jest aplikacja wideo

**[Fakt — autor/strona]** Strona projektu: *"Compositor is a free, open-source image editing application designed
as an alternative to Adobe Photoshop for macOS"* — warstwy, maski warstw, tryby mieszania, zaznaczenia, filtry
(Gaussian/motion blur, korekcja obiektywu, ziarno), liquify/smear, spot healing, clone stamp, usuwanie tła.
Deweloper: *"Adobe Photoshop costs too much and tools like GIMP don't feel familiar enough"*. Brak jakiejkolwiek
wzmianki o wideo.

**[Fakt — kod]** `grep -rl "AVFoundation\|AVKit\|VideoToolbox\|CMSampleBuffer\|AVPlayer" Compositor/` → **0 wyników**.
Jedyne trafienie na `CVPixelBuffer` jest w `Compositor/Document/ObjectSelection.swift` — to CoreVideo użyte
wyłącznie jako typ wymiany danych dla Vision (maska tła), nie do odtwarzania wideo; dokładnie to samo zastosowanie
ma już zbudowany w tym porcie `Sources/Compat/CoreVideo`. Brak `Timeline`/`Keyframe` w całym drzewie źródłowym.

**Wniosek:** Compositor to statyczny edytor obrazów (jak Photoshop), nie narzędzie do montażu/motion graphics.
Sekcje briefu dotyczące timeline, keyframes, animacji, AVFoundation/VideoToolbox, audio timing — **nie dotyczą tej
aplikacji**. Zaznaczone poniżej jako N/A zamiast wymyślonego projektu pipeline'u wideo, którego tu po prostu nie ma.

---

## 1. Analiza funkcjonalna Compositor

**[Fakt — kod]** Inwentarz cech pochodzi wprost z listy plików `Compositor/Document/*.swift` (61 plików w upstream).
Kluczowe obszary i ich implementacja (plik → co robi):

| Funkcja | Plik(i) | Zachowanie użytkownika |
|---|---|---|
| Warstwy, grupy, tryby mieszania | `LayerGroups.swift`, `LayerAppearance.swift`, `LayerMerge.swift` | Panel warstw jak w Photoshopie: opacity, blend mode, zagnieżdżanie |
| Maski warstw / clipping | `LayerMask.swift`, `LiveLayerMask.swift` | Reveal/hide, malowanie maski, link do warstwy |
| Zaznaczenia (marquee/lasso/wand) | `Selection.swift`, `MagicWand.swift`, `SelectionEdits.swift`, `SelectionClipboard.swift` | Prostokąt/owal/lasso/różdżka, add/subtract, expand/contract |
| Pędzel / gumka / stempel / naprawa | `BrushStroke.swift`, `CloneStamp.swift`, `EditorSession+Brush.swift` | Malowanie kafelkowe (256px tiles), klonowanie, spot healing |
| Smudge / Liquify | `SmudgeLiquify.swift` | Rozmazywanie pikseli po siatce przemieszczeń |
| Korekty (adjustment layers) | `Levels.swift`, `Curves.swift`, `HueSaturation.swift`, `PixelAdjust.swift`, `ImageAdjustments.swift` | Nieniszczące korekty z podglądem live |
| Filtry | `Filters.swift`, `BlurTool.swift`, `Distort.swift` | Gaussian/motion blur, ziarno, korekcja obiektywu |
| Efekty warstwy | `LayerEffects.swift` | Stroke/shadow/inner-shadow/color-overlay/outer-glow |
| Narzędzie tekstowe | `TypeTool.swift` | Ramki tekstowe, czcionki, edycja inline |
| Kształty | `ShapeTool.swift` | Prostokąt/elipsa/linia jako żywa warstwa wektorowa |
| Zaznaczanie obiektu / usuwanie tła | `ObjectSelection.swift`, `SubjectRemoval.swift`, `GuidedMatte.swift` | Segmentacja przez Vision, dopracowanie krawędzi |
| Transformacje | `LayerTransform.swift`, `LayerFlip.swift` | Przesunięcie/skala/rotacja nieniszcząca |
| Prowadnice | `Guides.swift` | Linie pomocnicze, siatka, snap |
| Historia / undo-redo | `DocumentHistory.swift` | Pełna historia edycji z nazwanymi krokami |
| Canvas/Crop | `CanvasSize.swift`, `Crop.swift` | Zmiana rozmiaru płótna/obrazu, przycinanie |
| Content-aware fill | `ContentFill.swift` | Wypełnianie na podstawie otoczenia |

**Brak (N/A dla tej aplikacji, patrz korekta wstępna):** scene graph w sensie animacyjnym, warstwy wideo, timeline,
keyframes, silnik animacji.

**Renderowanie:** kafelkowe (`Rendering/TiledLayerRenderer.swift`, `DownsampleCache.swift`) — **[Fakt — kod]**
`BrushStroke.swift` dzieli warstwę na kafelki 256px (`static let tileSize = 256`) i śledzi tylko dotknięte kafelki
(`dirtyTiles`), żeby ruch myszy nie kopiował całej warstwy przy każdym ruchu — kluczowe dla wydajności dużego płótna.

---

## 2–3. Frameworki i konkretne API Apple

**[Fakt — kod]** Zliczenie `import` w `Compositor/{Document,IO,Rendering,UI}`:

| Framework | Liczba plików | Główna rola tutaj |
|---|---:|---|
| AppKit | 60 | Kolory (`NSColor`), zdarzenia (`NSEvent`), kursory (`NSCursor`), obrazy (`NSImage`), ścieżki (`NSBezierPath`), dźwięk (`NSSound`), schowek (`NSPasteboard`/`NSItemProvider`) |
| SwiftUI | 36 | Wyłącznie `UI/` — nieużywane na Linuksie (patrz §7) |
| CoreGraphics | 27 | `CGContext`, `CGImage`, `CGPath`, `CGAffineTransform`, `CGColorSpace` — rdzeń rysowania |
| CoreImage | 16 | `CIImage`/`CIFilter` — filtry i podglądy korekt |
| UniformTypeIdentifiers | 9 | `UTType` — rozpoznawanie formatów plików |
| Observation | 5 | `@Observable` — natywne w Swift, działa bez zmian na Linuksie |
| ImageIO | 5 | `CGImageSource`/`CGImageDestination` — kodeki obrazów |
| Vision | 2 | `VNGenerateForegroundInstanceMaskRequest` — segmentacja obiektu/tła |
| Metal | 2 | `MetalBrushCoverage.swift`, `MetalLayerEffects.swift` — GPU |
| Accelerate | 2 | operacje wektorowe (dokładne API zależne od pliku) |
| Sparkle | 1 | auto-update (biblioteka strony trzeciej, nie framework Apple) |
| Combine | 1 | pojedyncze użycie, UI-only |

**[Historyczne — nieaktualne, kontekst]** Wcześniejszy audyt tej sesji (`docs/upstream-parity-audit.md`, generowany
przed usunięciem forka) naliczył **~101 unikalnych symboli Apple** w `Document/{IO,Rendering}`, z najcięższymi:
`NSColor` (68), `NSEvent` (51), `NSCursor` (44), `CIImage` (29), `NSBezierPath` (20), `NSImage` (19), `NSSound` (18),
`NSGraphicsContext` (18). Te liczby pochodzą sprzed strangler-migracji opisanej w §16 i służą tu wyłącznie jako
miara skali problemu, nie jako aktualny stan (aktualny stan: **wszystkie te symbole mają już shim**, patrz §4).

### Konkretne API — Metal (jedyne dwa pliki)

**[Fakt — kod]** `Rendering/MetalBrushCoverage.swift` i `Rendering/MetalLayerEffects.swift` — jedyne miejsca z
`import Metal`. Upstream sam traktuje Metal jako opcjonalny: `MetalLayerEffects.shared: MetalLayerEffects?`,
komentarz *"falls back to the CPU renderer when Metal isn't available"* — to jest kluczowy fakt architektoniczny,
bo oznacza, że **autor już zaprojektował ten punkt jako wymienialny**, zanim jeszcze powstał port na Linuksa.

Typowe elementy Metal w takim zastosowaniu (GPU compute do pokrycia pędzla i efektów warstwy, nie renderowanie
sceny 3D): `MTLDevice` (uchwyt GPU), `MTLCommandQueue`/`MTLCommandBuffer` (kolejka poleceń), `MTLComputePipelineState`
+ `MTLComputeCommandEncoder` (compute shadery — prawdopodobny wzorzec dla coverage/efektów, nie render pipeline),
`MTLTexture`/`MTLBuffer` (dane GPU). **[Hipoteza]** Dokładna struktura kerneli MSL nie jest widoczna (pliki `.swift`
wołają Metal, ale same shadery `.metal` nie są częścią analizowanego drzewa `Compositor/`) — założenie oparte na
typowym wzorcu "compute pass po kafelku" pasującym do architektury `BrushStroke`/`LayerEffects`.

---

## 4. Odpowiedniki GNU/Linux — tabela **[Zrealizowane w porcie]**

| macOS / Apple | Funkcja | GNU/Linux (ten port) | Typ | Uwagi |
|---|---|---|---|---|
| `CoreGraphics` (`CGContext`, `CGImage`, `CGPath`...) | Rysowanie 2D | `Sources/Compat/CoreGraphics/CoreGraphicsCompat` nad Skia | Funkcjonalny | `CanvasBackend` protocol (patrz §7); Skia jako jedyny backend na dziś |
| `AppKit` (`NSColor`, `NSEvent`, `NSCursor`, `NSBezierPath`, `NSImage`, `NSSound`, `NSPasteboard`) | Typy wartości + zdarzenia UI | `Sources/Compat/AppKit` | Częściowy→pełny per-symbol | Bezstanowe typy wartości (kolory, geometria) = bezpośredni odpowiednik; `NSSound` = no-op; `NSEvent`/`NSCursor` = inertne tokeny |
| `CoreImage` (`CIImage`, `CIFilter`) | Filtry pikselowe | `Sources/Compat/CoreImage` nad Skia/OpenCV | Funkcjonalny | Tylko ~10 konkretnych filtrów, których upstream faktycznie używa (`CIGaussianBlur`, `CIMotionBlur`, `CIColorMatrix`...) — interface segregation |
| `Vision` (`VNGenerateForegroundInstanceMaskRequest`) | Segmentacja tła/obiektu | OpenCV GrabCut (`Sources/Compat/Vision`) | Funkcjonalny (inny algorytm) | Nie ten sam model co Apple Vision, ale ten sam kontrakt API |
| `ImageIO` (`CGImageSource*/CGImageDestination*`) | Kodeki obrazów | Qt image plugins (`host/QtImageIO.cpp` + `Sources/Compat/ImageIO`) | Bezpośredni (przez Qt) | PNG/JPEG/TIFF/WebP przez Qt; HEIC ograniczone |
| `UniformTypeIdentifiers` | Rozpoznawanie typów plików | statyczna tabela `UTType` | Bezpośredni | `Sources/Compat/UniformTypeIdentifiers` |
| `Accelerate` | operacje wektorowe | podzbiór odpowiedników w `Sources/Compat/Accelerate` | Częściowy | tylko symbole, których upstream faktycznie używa |
| `CoreVideo` (`CVPixelBuffer`) | bufor pikseli dla Vision | `Sources/Compat/CoreVideo` | Bezpośredni | wyłącznie jako typ wymiany, nie do wideo |
| `Observation` (`@Observable`) | reaktywność | natywne w Swift na Linuksie | Bezpośredni | zero pracy |
| `Metal` (2 pliki) | GPU compute | **inna architektura, nie shim** — patrz §5 | Zamiennik architektoniczny | `Sources/Overrides/*` |
| `SwiftUI`/`AppKit` UI (`UI/*.swift`, 32 pliki) | interfejs użytkownika | **nie kompilowane na Linuksie** — Qt 6 (`host/`) | Zamiennik architektoniczny | patrz §7 |
| `Sparkle` | auto-update | brak | Brak odpowiednika | funkcja świadomie pominięta (dystrybucja przez Flatpak ma własny mechanizm aktualizacji) |

---

## 5. Metal → Vulkan (mapowanie ogólne + konkretna implementacja w tym porcie)

### Mapowanie pojęciowe (tło ogólne, z dokumentacji Khronos/Apple)

| Metal | Vulkan | Różnica architektoniczna |
|---|---|---|
| `MTLDevice` | `VkPhysicalDevice` + `VkDevice` | Vulkan rozdziela wybór fizycznego GPU od logicznego uchwytu urządzenia |
| `MTLCommandQueue` | `VkQueue` | podobne, ale Vulkan wymaga jawnego wyboru rodziny kolejki (graphics/compute/transfer) |
| `MTLCommandBuffer` | `VkCommandBuffer` | Vulkan wymaga jawnego `vkBeginCommandBuffer`/`vkEndCommandBuffer` i puli poleceń (`VkCommandPool`) |
| `MTLTexture` | `VkImage` + `VkImageView` | Vulkan rozdziela pamięć obrazu od widoku na nią; layouty obrazu (`VkImageLayout`) są jawne i wymagają barier |
| `MTLBuffer` | `VkBuffer` + `VkDeviceMemory` | Vulkan nie ma zunifikowanej alokacji pamięci — trzeba zarządzać nią ręcznie (lub przez bibliotekę typu VMA) |
| `MTLRenderPipelineState` | `VkPipeline` (graphics) | Vulkan "zamraża" cały stan pipeline'u w jeden obiekt przy tworzeniu (immutable state objects) |
| `MTLComputePipelineState` | `VkPipeline` (compute) | koncepcyjnie identyczne |
| `MTLSamplerState` | `VkSampler` | podobne |
| Synchronizacja niejawna (Metal śledzi zależności automatycznie w wielu przypadkach) | `VkFence`/`VkSemaphore`/`VkEvent` + jawne `VkMemoryBarrier`/`VkImageMemoryBarrier` | **kluczowa różnica**: Vulkan wymaga ręcznej, jawnej synchronizacji CPU↔GPU (fence) i GPU↔GPU (semaphore) oraz barier pamięci/layoutu — nic nie dzieje się "samo" |
| Brak odpowiednika 1:1 | `VkDescriptorSet`/`VkDescriptorSetLayout` | Vulkan wiąże zasoby (bufory/tekstury) przez jawne zestawy deskryptorów zamiast prostego `setTexture:atIndex:` |
| Kompilacja shaderów w runtime (MSL źródłowy lub prekompilowany `.metallib`) | SPIR-V (bajtkod, zawsze prekompilowany) | Vulkan nigdy nie kompiluje shaderów ze źródła w runtime — SPIR-V jest generowany offline |

**[Hipoteza — ogólna wiedza branżowa]** Największa realna trudność portu Metal→Vulkan to nie mapowanie API 1:1, tylko
przeniesienie modelu synchronizacji: Metal ukrywa wiele zależności, Vulkan wymaga ich jawnego wyrażenia. Dla aplikacji
tej skali (2 pliki, kernel do "coverage pędzla" i "efekty warstwy" — proste compute passy, nie złożony graf renderowania
3D) ryzyko to jest małe i faktycznie okazało się małe w tym porcie (patrz niżej).

### Co faktycznie zrobiono w tym porcie **[Zrealizowane w porcie]**

Zamiast cross-kompilować MSL, **Metal został zastąpiony całą inną implementacją o identycznym publicznym API**
(nie shim 1:1, tylko zamiennik zachowujący kontrakt):

- `Sources/Overrides/MetalBrushCoverage.swift`, `Sources/Overrides/MetalLayerEffects.swift` — te same nazwy
  publiczne co upstream (`MetalLayerEffects.shared`, `render(_:effects:) -> CGImage`), wykluczone z kompilacji
  `Sources/UpstreamCore` (manifest `linux/upstream-parity.json`) i podstawione 1:1.
- `backends/brush/VulkanBrushCoverage.cpp` + `backends/effects/EffectsVulkan.cpp` — realna implementacja Vulkan
  compute.
- Łańcuch failover: Vulkan → Skia image filters → OpenCV → CPU (`backends/effects/{EffectsCPU.cpp,EffectsOpenCV.cpp}`),
  analogiczny wzorzec co istniejący `AdaptiveBrushCoverage`. Sterowany `COMPOSITOR_EFFECTS=vulkan|skia|opencv|cpu|auto`.
  Testowany wymuszony fallback (`compositor_renderer_simulate_device_lost`) — nie tylko happy-path.
- Shadery SPIR-V budowane offline: `scripts/build-brush-shader.py`, `shaders/effects.comp`, `backends/effects/shaders/`.

**Dlaczego to był dobry wybór, a nie próba cross-kompilacji MSL→SPIR-V:** tylko 2 pliki, mały, dobrze
odizolowany zestaw kerneli (coverage pędzla, kompozycja efektów) — koszt napisania własnej implementacji Vulkan
był niższy niż koszt utrzymania toolchainu tłumaczącego shadery, a upstream *sam* już traktował ten komponent jako
wymienialny (opcjonalny `MetalLayerEffects?`). Dla dużo większego korpusu shaderów (dziesiątki/setki kerneli)
właściwa odpowiedź byłaby inna — patrz §6.

---

## 6. Przenośność shaderów

**[Hipoteza — ogólna wiedza branżowa]** Porównanie języków shaderów:

| Język | Cel | Uwagi |
|---|---|---|
| MSL (Metal Shading Language) | Metal | tylko Apple |
| GLSL | OpenGL/Vulkan | najbardziej dojrzały, ale mniej przenośny bez dodatkowego kroku |
| HLSL | DirectX (i przez DXC → SPIR-V także Vulkan) | dobry wybór jako "wspólny" język przy dużym korpusie shaderów, bo ma dojrzały kompilator do SPIR-V (DXC) i do MSL (przez SPIRV-Cross) |
| SPIR-V | bajtkod pośredni Khronos | *nie* pisze się w nim ręcznie — jest celem kompilacji z GLSL/HLSL |
| WGSL | WebGPU | nowszy, mniej narzędzi, głównie dla web |

Typowy przenośny pipeline: `HLSL → DXC → SPIR-V → Vulkan` oraz opcjonalnie `SPIR-V → SPIRV-Cross → MSL → Metal`
(gdyby trzeba było też wspierać macOS z tego samego źródła shaderów).

**[Zrealizowane w porcie — i dlaczego inaczej]** Ten port pisze shadery bezpośrednio w GLSL i kompiluje je do
SPIR-V (`scripts/build-brush-shader.py`), **nie** przechodząc przez HLSL/DXC ani przez MSL w ogóle — bo macOS
i tak używa swojej *oryginalnej* implementacji Metal (`Compositor/Rendering/Metal*.swift` pozostaje
niezmodyfikowany i działa tylko na macOS); Linux ma **całkiem osobną** implementację GLSL→SPIR-V→Vulkan, nie
transpilowaną z MSL. To jest spójne z decyzją z §5: przy 2 plikach/małym korpusie kerneli, jeden wspólny język
shaderów dla obu platform nie daje się uzasadnić kosztem toolchainu — każda platforma ma swoją natywną
implementację tego samego kontraktu API.

---

## 7. Architektura cross-platform **[Zrealizowane w porcie]**

Rzeczywisty układ w tym repozytorium:

```
Compositor/**                      kod źródłowy upstream (Document/IO/Rendering) — NIEZMODYFIKOWANY
    ↓ (path:+exclude: w Package.swift)
Sources/UpstreamCore                target SwiftPM kompilujący powyższe bez zmian
    ↑ zależy od (import X — nie widzi różnicy Linux/macOS)
Sources/Compat/{CoreGraphics,AppKit,CoreImage,Vision,ImageIO,
                UniformTypeIdentifiers,Accelerate,CoreVideo,
                FoundationCompat,SwiftUI}    moduły o nazwach jak frameworki Apple
Sources/Overrides/{MetalBrushCoverage,MetalLayerEffects}.swift   podmiana 1:1 (patrz §5)
Sources/LinuxBridge/UpstreamEditor.swift     cienki adapter poleceń nad EditorSession (bez logiki biznesowej)
host/ (Qt 6, C++)                            powłoka UI — jedyna warstwa faktycznie linuksowa
backends/{brush,effects}/ (C++, Vulkan/OpenCV/CPU)   implementacje GPU/CPU za protokołami
```

**Dlaczego Qt 6, a nie GTK4/Slint/Dear ImGui — [Fakt — kod + uzasadnienie]:**
- Natywne dialogi plików, gęsty, wielopanelowy interfejs pasujący do stylu Photoshopa (`host/SessionWindow.cpp`
  — panele warstw, pasek opcji, doki) — dojrzały zestaw widżetów Qt Widgets pasuje lepiej niż immediate-mode
  (Dear ImGui) czy deklaratywny, młodszy Slint.
- Jeden toolkit obsługujący zarówno Wayland, jak i X11 fallback bez dodatkowej warstwy — **[Fakt — kod]**
  `com.wonderassembly.Compositor.yaml` finish-args: `--socket=wayland` (preferowany), `--socket=fallback-x11`.
- GTK4 byłby też sensownym wyborem (podobna dojrzałość), ale Qt ma lepsze wsparcie dla natywnego GPU (Vulkan/GL
  przez `--device=dri`) i tabletów graficznych (`QTabletEvent` — nacisk, przechył, gumka) bez dodatkowych zależności.

**SOLID — jak jest egzekwowane, nie tylko deklarowane:**
- **DIP jawnie zmaterializowany**: `Sources/Compat/CompatSupport/ServiceSlot<T>` (`ServiceSlot.swift`) — każdy
  moduł kompatybilności trzyma swój "seam" jako `ServiceSlot`, nie jako goły `var` mutowalny globalnie. Host
  instaluje implementację raz przy starcie; testy podmieniają ją na czas zamknięcia (`withOverride`). Kod
  upstreamowy (który nie może przyjąć zależności przez inicjalizator — bo jest niezmieniony) i tak dostaje
  odwróconą zależność.
- **`CanvasBackend`/`CanvasBackendFactory` protocol** (`Sources/Compat/CoreGraphics/CoreGraphicsCompat/
  CanvasBackend.swift`, `SkiaCanvasBackend.swift`) — `CGContext` nie woła Skia bezpośrednio, woła protokół; dziś
  ma jeden konkretny backend (Skia), ale dodanie drugiego nie wymaga zmiany `CGContext`.
- **Interface segregation**: każdy moduł `Sources/Compat/*` eksponuje tylko symbole, których upstream faktycznie
  używa (rosną on-demand, sterowane błędami kompilacji) — nie próbuje odtworzyć całego AppKit.
- **Open/closed**: `Compositor/` jest zamknięty na modyfikacje (`scripts/check-upstream-clean.sh` w CI to
  egzekwuje), otwarty na rozszerzenie przez nowe symbole w `Sources/Compat`.

---

## 8. Co powinno być (i jest) współdzielone vs. platform-specific **[Zrealizowane w porcie]**

**Współdzielone (platform-independent), bo to i tak jest rzeczywisty stan:**
- Cały model dokumentu/logiki edycji: `Sources/UpstreamCore` = `Compositor/Document` (warstwy, historia, maski,
  zaznaczenia, korekty) — **dosłownie ten sam kod źródłowy co macOS**, nie reimplementacja.
- Format projektu (`.comp`) — `Compositor/IO/ProjectStore.swift`, niezmodyfikowany.
- Logika renderowania kafelkowego (`Rendering/TiledLayerRenderer.swift` i pokrewne) — niezmodyfikowana, bo jest
  czystym CG/CI po podłożeniu shimów.

**Platform-specific (i słusznie odseparowane):**
- Okno/UI: `host/` (Qt, C++) — jedyna warstwa napisana od zera dla Linuksa.
- GPU compute dla Metal-only kerneli: `backends/{brush,effects}/` (C++/Vulkan).
- Integracja systemowa: schowek, drag&drop, dialogi plików — przez `host/interfaces/IPlatformServices.h` +
  `host/QtPlatformServices.h` (DI — wstrzykiwane do `SessionWindow`, nie hardkodowane).

---

## 9. Wayland i X11 **[Zrealizowane w porcie]**

**[Fakt — kod]** `com.wonderassembly.Compositor.yaml`: `--socket=wayland` jako preferowany, `--socket=fallback-x11`
jako fallback, `--share=ipc` (IPC pluginu platformy Qt). Obsługa realizowana **przez framework (Qt)**, nie ręcznie
— słuszny wybór dla aplikacji tej wielkości: Qt sam wybiera odpowiedni backend platformowy (`qtwayland`/xcb) w
runtime, a ręczna implementacja Wayland/X11 od zera (jak zrobiłby to np. silnik gry pisany bezpośrednio na
`libwayland`/Xlib) nie dałaby żadnej korzyści dla edytora obrazów — tylko koszt utrzymania.

---

## 10. GPU na Linuksie

**[Hipoteza — ogólna wiedza branżowa + fakt z tej sesji]** Mesa RADV (AMD) i ANV (Intel) to open-source'owe
sterowniki Vulkan wystarczające dla compute-only kerneli tej aplikacji (brak zaawansowanych rozszerzeń ray
tracingu itp.); NVIDIA proprietary driver ma pełne wsparcie Vulkan, ale inny model debugowania/walidacji.

**[Fakt — ta sesja]** W środowisku sandboksowym (Flatpak SDK, bez fizycznego GPU) budowanie i testowanie odbywało
się z **software Vulkan (llvmpipe)** — port musi więc działać poprawnie także bez sprzętowego GPU, co wymusiło
istnienie łańcucha failover (§5) nie tylko jako "miło mieć", ale jako realnie wykonywaną ścieżkę w CI/testach.
Testowany też jawnie wymuszony fallback (`compositor_renderer_simulate_device_lost` — symuluje utratę urządzenia
GPU w trakcie działania, nie tylko brak GPU przy starcie).

---

## 11. Video pipeline — **N/A**

**[Fakt — patrz "Kluczowa korekta wstępna"]** Compositor nie ma warstw wideo, dekodowania, odtwarzania ani
osi czasu. Nie ma AVFoundation/VideoToolbox w kodzie źródłowym. Ta sekcja briefu nie dotyczy tej aplikacji —
nie projektuje się tu pipeline'u FFmpeg/GStreamer, bo nie ma czego dekodować.

---

## 12. Wydajność (dla edytora obrazów, nie wideo)

**[Fakt — kod]** Rzeczywiste techniki wydajnościowe już w kodzie upstream (niezmienionym):
- Kafelkowanie 256px (`BrushStroke.swift: static let tileSize = 256`) — ruch pędzla dotyka tylko przecinających
  się kafelków, nie całej warstwy.
- `dirtyDocumentRect`/`dirtyTiles` — śledzenie tylko zmienionego obszaru do odświeżenia podglądu.
- `RasterSnapshot` (`Rendering/RasterSnapshot.swift`) — leniwa materializacja: sparse reprezentacja (bazowy obraz +
  łatki) trzymana bez tworzenia pełnego `CGImage`, dopóki faktyczny konsument (eksport, operacja obrazu) nie
  zażąda bajtów (*"never on mouse-up"* — komentarz w źródle).
- `DownsampleCache` — cache pomniejszonych wersji do wyświetlania przy oddaleniu.

**[Zrealizowane w porcie]** Największy realny koszt wydajnościowy napotkany w tej sesji to nie GPU↔CPU transfer,
tylko **koszt budowy Skia/OpenCV od zera** (build-time, nie runtime) — zewnętrzne rate-limity na
`chromium.googlesource.com`/`skia.googlesource.com` przy `git-sync-deps` (patrz §20, "największe realne problemy").
Runtime: podgląd filtrów/korekt jest debounce'owany (120ms, `host/AdjustDialog.cpp`), żeby nie renderować przy
każdym ruchu suwaka.

---

## 13. Zależności specyficzne dla macOS — kategoryzacja

| Kategoria | Przykłady | Uzasadnienie |
|---|---|---|
| **Łatwe do zastąpienia (shim bezpośredni)** | `NSColor`, geometria (`NSPoint`/`NSRect`/`NSSize`), `NSSound` (no-op) | typy wartości bez zachowania systemowego |
| **Wymagające wrappera funkcjonalnego** | `CIFilter` (konkretne filtry), `Vision` (segmentacja), `ImageIO` (kodeki) | ten sam kontrakt API, inny silnik pod spodem |
| **Wymagające innej architektury** | `Metal` (2 pliki) | nie shim, tylko odrębna implementacja o identycznym API (§5) |
| **Praktycznie macOS-only, brak odpowiednika** | `Sparkle` (auto-update) | świadomie pominięte — Flatpak ma własny mechanizm dystrybucji/aktualizacji |
| **UI-only, nie dotyczy silnika** | `SwiftUI`/`AppKit` UI (32 pliki `UI/*.swift`) | zastąpione całą inną powłoką (Qt), nie shimem |

---

## 14. Wybór języka

Swift był **dany z góry** — to język, w którym napisany jest upstream, a zasada tego portu ("wrap, don't fork")
zakłada zerowe modyfikacje kodu upstreamowego. Pytanie brzmi więc nie "czy Swift", tylko "czy zostawić Swift na
warstwach, które port *sam* dopisuje".

**[Zrealizowane w porcie] Dlaczego współdzielona logika i warstwa Compat zostały w Swift, a nie przepisane na
C++/Rust:**
- Swift 6.3.3 ma działający toolchain na Linuksie (`org.freedesktop.Sdk.Extension.swift6`), włącznie z
  `swift-testing` — testy upstreamowe (`CompositorTests/*.swift`) kompilują się i **przechodzą bez zmian**.
  Przepisanie logiki na inny język zerwałoby tę własność (testy-jako-kontrakt).
- Warstwa Compat *musi* być w Swift, bo jest wołana przez `import AppKit`/`import CoreImage` itd. w kodzie
  upstreamowym — nie da się tego zrobić w innym języku bez dodatkowej warstwy FFI na każde wywołanie.

**[Zrealizowane w porcie] Dlaczego `backends/` i `host/` są w C++, nie w Swift:**
- Ekosystem Vulkan i Qt ma dojrzałe wiązania C++; wiązania Swift→Qt nie są na tyle dojrzałe, by uzasadnić koszt
  (Qt Widgets ma tysiące metod, ręczne/generowane bindingi w Swift byłyby dużym, kruchym przedsięwzięciem).
- Swift na Linuksie nie ma pierwszorzędnego dostępu do Vulkan C API bez własnego cienkiego C-ABI (co i tak
  musiałby zrobić C++).

**[Hipoteza]** Rust byłby uzasadnionym wyborem *gdyby* port zaczynał się od zera bez ograniczenia "nie zmieniaj
kodu macOS" — dałby bezpieczeństwo pamięci bez GC przy pisaniu `backends/` — ale przy tym ograniczeniu i tak
trzeba było użyć Swift dla warstwy Compat, więc C++ dla `backends/`/`host/` minimalizuje liczbę języków w
projekcie (Swift + C++, nie Swift + C++ + Rust) bez utraty właściwości bezpieczeństwa istotnych dla tej warstwy
(cienkie mostki C-ABI, nie duża logika biznesowa).

---

## 15. Stack technologiczny Linux — **[Zrealizowane w porcie], nie propozycja**

| Warstwa | Technologia | Plik/dowód |
|---|---|---|
| Język współdzielonej logiki | Swift 6.3.3 | `Package.swift`, `org.freedesktop.Sdk.Extension.swift6` |
| UI | Qt 6.11 (Widgets) | `host/`, `org.kde.Sdk//6.11` |
| GPU rendering | Vulkan (przez Skia Ganesh + własne compute) | `backends/effects/EffectsVulkan.cpp` |
| Rasteryzacja 2D | Skia (`canvaskit/0.42.0`) | `com.wonderassembly.Compositor.yaml` moduł `skia` |
| Computer vision (segmentacja) | OpenCV 4.14.0 (GrabCut) | moduł `opencv` w manifeście |
| Kodeki obrazów | Qt image plugins | `host/QtImageIO.cpp` |
| Dystrybucja | Flatpak (`org.kde.Platform//6.11`) | manifest główny |
| Sandbox | XDG Portals (FileChooser/Documents), brak `--filesystem=home` | `finish-args` w manifeście |

To nie jest "propozycja do przemyślenia" — to jest stack, który realnie się kompiluje i przechodzi 260 testów
upstreamowych + 96 testów warstwy Compat + testy backendów w tym repozytorium, dziś.

---

## 16. Roadmapa — jak faktycznie przebiegła (nie plan na przyszłość)

**[Zrealizowane w porcie]** Rzeczywista kolejność (z historii tego repo/sesji), strangler pattern:

1. **Faza 0 — spike wykonalności**: policzenie symboli Apple potrzebnych do skompilowania `Compositor/Document`
   bez zmian (~101 symboli) — potwierdziło wykonalność przed zaangażowaniem się w pełną migrację.
2. **Faza 1 — fundament Compat**: `CoreGraphics`, `AppKit` (headless), `Observation` (za darmo, natywne).
3. **Faza 2 — strangler**: zastąpienie ręcznie portowanego forka (`Sources/CompositorCore`, 72 pliki) przez
   kompilację `Compositor/Document` bez zmian, plik po pliku, z testami upstreamowymi jako wyrocznią.
4. **Faza 3 — pozostałe frameworki**: `CoreImage`, `Vision`, `ImageIO`, `UniformTypeIdentifiers`, `Accelerate`.
5. **Faza 4 — Metal → Vulkan**: `Sources/Overrides/*` (§5).
6. **Faza 5 — adaptery UI**: `Sources/LinuxBridge/UpstreamEditor.swift` (cienki adapter poleceń, bez logiki),
   `host/` (Qt).
7. **Faza 6 — brama CI** (`ci-guard.yml`/`ci-build.yml`) — pilnuje, żeby `Compositor/` nigdy nie zboczył od
   `upstream/main`.

**Realne ryzyka, które faktycznie wystąpiły (nie hipotetyczne):**
- Ścisłość Swift 6 (actor isolation) generuje dużo ostrzeżeń kompilatora przy wzorcu "pompowania run loop" użytym
  do synchronicznego czekania na asynchroniczne operacje upstreamu z wątku Qt — nieuniknione przy tym podejściu,
  udokumentowane, nie ukryte.
- Zewnętrzne rate-limity Google przy `git-sync-deps` Skii zablokowały jeden pełny build Flatpaka w tej sesji —
  problem infrastruktury, nie kodu.
- Fork retirement ujawnił prawdziwy błąd: stary protokół Clone Stamp w `host/SessionWindow.cpp` (offset liczony
  po stronie klienta) nie pasował do nowego mostka (offset liczony po stronie serwera) — wykryte i naprawione
  przez świadomy audyt protokołu, nie przez przypadek.

---

## 17. Macierz portowalności (per plik `Document/`, dane realne z audytu tej sesji)

Pełne dane: `docs/upstream-parity-audit.md`, `docs/feature-parity-matrix.md` — **[Historyczne — nieaktualne]**
te dwa dokumenty opisują stan **sprzed** usunięcia forka (`Sources/CompositorCore`) i skompilowania
`Compositor/Document` bez zmian. Ich liczby procentowe ("56% symbol coverage", "46% feature parity") odnoszą się
do *starej* architektury (dopasowanie nazw symboli między dwoma oddzielnymi drzewami kodu) i są dziś w dużej
mierze bezprzedmiotowe: skoro `Sources/UpstreamCore` kompiluje `Compositor/Document/{Levels,Curves,Selection,
LayerMask,PixelAdjust,HueSaturation,Distort,Guides,LayerEffects,TypeTool,ObjectSelection,ShapeTool,...}.swift`
**dosłownie bez zmian**, "pokrycie symboli" tych plików jest z definicji 100% — nie dlatego, że ktoś je
przepisał, tylko dlatego, że to jest ten sam plik.

**Co z tych dokumentów pozostaje aktualne i wartościowe:** lista brakujących **funkcji UI** (§ "Feature matrix" w
`docs/upstream-parity-audit.md`) — to nie jest kwestia kompilacji logiki (ona już działa), tylko braku
odpowiedniego przycisku/menu w `host/` który by ją wywołał. Przykład: `LayerEffects.swift` **kompiluje się i
działa** (logika jest tam, niezmieniona), ale `host/SessionWindow.cpp` może nie mieć jeszcze pełnego UI do
wszystkich jego opcji (outer glow ma, ale nie wszystkie warianty) — to jest realny, aktualny rodzaj "gapu"
portu: brakujące okablowanie Qt, nie brakująca logika.

---

## 18. Fakty vs. hipotezy — podsumowanie dyscypliny

Stosowane konsekwentnie w całym dokumencie (oznaczenia przy każdym akapicie, patrz Legenda). Najważniejsze miejsca,
gdzie hipoteza mogłaby łatwo zostać pomylona z faktem:
- **Struktura kerneli Metal (MSL)** — nie jest widoczna z analizowanego drzewa (`.metal` pliki nie są częścią
  `Compositor/`), więc dokładny kształt compute passów jest hipotezą, nie faktem.
- **"100% parity"** twierdzenia w starszych dokumentach tego repo (`docs/feature-parity-matrix.md`) — te dotyczą
  *starej* (forkowej) architektury i były częściowo zawyżone nawet wtedy (późniejszy, bardziej szczery audyt w
  `docs/upstream-parity-audit.md` pokazał realne 46-56%). Po usunięciu forka pytanie "ile % pokrycia" zmienia
  swoje znaczenie (§17) — nie należy cytować starych liczb jako aktualnego stanu.
- **Zachowanie sterowników GPU (Mesa RADV/ANV vs. NVIDIA proprietary)** dla tej konkretnej aplikacji na realnym
  sprzęcie — nie testowane w tej sesji (środowisko budowania nie miało fizycznego GPU, tylko llvmpipe) — oznaczone
  jako hipoteza oparta na ogólnej wiedzy branżowej, nie na pomiarze.

---

## 19. Źródła

- `robbietilton.com/compositor` — opis produktu, cytowany w tekście.
- `github.com/robbietilton/Compositor` — repozytorium źródłowe (skonfigurowane w tym repo jako remote `upstream`,
  aktualnie zsynchronizowane do commita `609dbea`).
- Rzeczywisty kod źródłowy `Compositor/{Document,IO,Rendering,UI}` w tym repozytorium — główne źródło faktów
  technicznych w tym dokumencie.
- Ogólna dokumentacja Khronos (Vulkan) i Apple (Metal) — wyłącznie jako tło dla tabeli pojęciowej w §5 i §6;
  nie cytowane dosłownie, oznaczone jako `[Hipoteza — ogólna wiedza branżowa]` tam, gdzie nie ma bezpośredniego
  potwierdzenia w kodzie tego repo.

---

## 20. Podsumowanie końcowe

1. **Mapa architektury macOS**: `Compositor/{Document,IO,Rendering}` (logika, CG/CI-zależna) + `Compositor/UI`
   (SwiftUI/AppKit, 32 pliki, UI-only) + 2 pliki Metal (GPU compute, opcjonalne z założenia autora).
2. **Mapa zależności Apple**: §2–§3 (AppKit 60 plików, CoreImage 16, CoreGraphics 27, Metal 2, Vision 2, itd.).
3. **Mapa odpowiedników Linux**: §4 (tabela pełna).
4. **Architektura cross-platform**: §7 — `Compositor/` niezmieniony → `Sources/Compat`/`Overrides` (seam) →
   `Sources/UpstreamCore` → `Sources/LinuxBridge` (cienki adapter) → `host/` (Qt).
5. **Stack Linux**: §15 — Swift 6.3 + Qt 6.11 + Vulkan/Skia/OpenCV + Flatpak.
6. **Diagram render pipeline**: `CGContext` (upstream, niezmieniony) → `CanvasBackend` protocol →
   `SkiaCanvasBackend` (dziś jedyny) → dla efektów warstwy: `LayerEffectsBackend` chain Vulkan → Skia → OpenCV →
   CPU, sterowana `COMPOSITOR_EFFECTS`.
7. **Diagram video pipeline**: **brak — N/A, aplikacja nie obsługuje wideo (§11)**.
8. **Roadmapa jak faktycznie przebiegła**: §16.
9. **Największe realne problemy techniczne napotkane w tej sesji**:
   - Rate-limity Google na `git-sync-deps` Skii blokujące pełny build Flatpaka (infrastruktura, nie kod).
   - Ścisłość Swift 6 (actor isolation) przy wzorcu pompowania run loop dla synchronicznego mostka Qt↔async-Swift.
   - Luka w podglądzie na żywo trwającego pociągnięcia pędzla przed commitem (zamknięta w trakcie tej sesji —
     wykorzystano *tę samą* metodę `BrushStroke.paintSnapshot()`, której używa oryginalny `EditorCanvas.swift`
     na macOS, więc nie jest to nowa logika, tylko to samo wywołanie z innego miejsca).
   - Regresja protokołu Clone Stamp po usunięciu forka — wykryta i naprawiona przez świadomy audyt, nie przez
     przypadek.
10. **Co świadomie zaprojektowano jako platform-independent od początku**: `Sources/UpstreamCore` (cały model
    dokumentu), `CanvasBackend`/`LayerEffectsBackend` protocols (DIP na granicy GPU), `ServiceSlot<T>` (DIP na
    granicy każdego modułu Compat) — nie jako "ładny dodatek", tylko jako mechanizm, który **fizycznie
    uniemożliwia** przypadkowe wprowadzenie zależności od konkretnego backendu w kodzie upstreamowym, bo ten
    kod w ogóle nie ma dostępu do niczego poza protokołem.
