# macOS UI/UX Parity Rules for `GNU_Linux`

> **Canonical reference:** macOS `main` at `c9f8fad82b39e11d5a69df2c288a12defe057ba2`  
> **Linux audit baseline:** `GNU_Linux` at `088a0d5a0d32d4d1dc0122b52e9b071bd9617600` before this document  
> **Status:** source-derived parity specification and discrepancy audit. This is **not** a claim of 100% visual or behavioral parity.

## 0. Purpose and non-negotiable rule

The GNU/Linux application exists to reproduce the current macOS Compositor application as closely as technically possible. The macOS application is authoritative. Linux must not redesign, simplify, modernize, reinterpret, regroup, rename, add visible convenience commands, or substitute a Qt-native visual convention when doing so creates a visible or behavioral difference.

A Linux control is not considered “at parity” merely because a similarly named control exists. Parity requires the same role in the hierarchy, geometry, visual treatment, enabled/disabled logic, state transitions, keyboard behavior, pointer behavior, command semantics, and contextual behavior.

The phrase **100% parity** is reserved for a build that has passed the screenshot and interaction verification procedure in this document. The `GNU_Linux` baseline commit message uses that phrase, but source inspection finds material differences; therefore the branch must not currently be described as verified 100% parity.

---

## 1. Source-of-truth policy

### 1.1 Canonical source precedence

When macOS and Linux differ, resolve the difference in this order:

1. Current macOS implementation on `main`.
2. Current macOS runtime behavior captured from that implementation.
3. Explicit macOS source constants and state logic.
4. Linux implementation only as evidence of the current discrepancy, never as design authority.

For every Linux UI element, the implementation must be traceable to a macOS source symbol/file, or marked **Linux/platform-only** with an explicit Category C justification.

### 1.2 Canonical macOS files

| Area | Canonical macOS source |
|---|---|
| App/window/menu composition | `Compositor/CompositorApp.swift`, `Compositor/ContentView.swift` |
| Tool enum/order/defaults | `Compositor/Document/EditorSession.swift` |
| Tool header common metrics | `Compositor/UI/ToolHeaderStyle.swift` |
| Brush-family options | `Compositor/UI/BrushControls.swift` |
| Selection/Wand/Lasso options | `Compositor/UI/LassoControls.swift` |
| Move/transform header | `Compositor/UI/TransformInspector.swift` |
| Crop | `Compositor/UI/CropControls.swift` |
| Gradient | `Compositor/UI/GradientControls.swift` |
| Shape | `Compositor/UI/ShapeControls.swift` |
| Type | `Compositor/UI/TypeControls.swift` |
| Hand/Zoom | `Compositor/UI/NavigationToolHeader.swift` |
| FG/BG swatches | `Compositor/UI/ColorPaletteControls.swift` |
| Tabs | `Compositor/UI/ProjectTabs.swift` |
| Layers panel | `Compositor/UI/LayersPanel.swift`, `LayerAppearanceControls.swift`, `BlendModePicker.swift`, `NativeLayerList.swift`, `LayerMaskMenu.swift` |
| Rulers/guides | `Compositor/UI/CanvasRulers.swift`, `Compositor/Rendering/EditorCanvas.swift` |
| Color picker | `Compositor/UI/ColorPickerSheet.swift`, `FloatingPanel.swift` |
| New canvas | `Compositor/UI/NewCanvasSheet.swift` |
| Canvas/Image size | `Compositor/UI/CanvasSizeSheet.swift`, `ImageSizeSheet.swift` |
| Filters | `Compositor/Document/Filters.swift`, `Compositor/UI/FilterSheet.swift` |
| Keyboard model | `Compositor/UI/KeyboardShortcuts.swift` |

### 1.3 Linux files under audit

The primary Linux shell is `host/SessionWindow.cpp` / `host/SessionWindow.h`, launched by `host/host_run.cpp`. Related UI implementations include `host/LayerItemDelegate.h`, `host/SizeDialog.cpp`, `host/AdjustDialog.cpp`, `host/FilterDialog.cpp`, `host/SessionDialogs.cpp`, `host/ColorPickerDialog.h`, and platform services under `host/`.

### 1.4 Source-derived vs screenshot-derived values

The tables below use exact source values wherever the macOS source defines them. AppKit/SwiftUI semantic colors, native control internals, glyph rasterization, menu metrics, and system typography can vary with macOS. Those values must be **resolved by reference screenshots** rather than guessed as hard-coded hex values.

Do not replace a semantic macOS value with an arbitrary “close” Qt color. Capture the resolved reference value on the canonical test Mac and store it in Linux parity tokens where necessary.

---

## 2. Platform parity categories

Every component must be tagged as one of these categories.

### A — Must match macOS exactly

Compositor-owned geometry and behavior: tool rail, option bars, tabs, layers panel, canvas chrome, status bar, custom swatches, row geometry, custom icons, context menus, tool state, command names/order, selection rules, panel dimensions, custom dialogs, in-canvas overlays.

### B — Must visually/behaviorally match but requires Qt implementation

SwiftUI/AppKit components whose native implementation is unavailable on Linux: segmented pickers, capsule controls, `NSTableView` layer list, `NSPanel` floating tools, SF Symbol-based icons, responder-chain command dispatch, macOS drag pasteboard behavior. Rebuild these in Qt; **do not accept the default Fusion/GNOME/KDE rendering if it visibly diverges**.

### C — Cannot literally match because it belongs to macOS

Examples: global/app menu integration, macOS application menu, traffic-light window controls, native file chooser internals, macOS hide/show commands, AppKit accessibility role implementation, system color-management/rasterization details. Use the closest Linux desktop convention while preserving Compositor command semantics.

Category C is not permission to change Compositor-owned UI.

---

## 3. Global geometry tokens

All values are **logical points/pixels**. At 100% scale 1 logical px maps to 1 device px; at HiDPI it maps through device pixel ratio.

| Token | macOS canonical value | Linux rule |
|---|---:|---|
| Window default size | `1180 × 780` | Initial normal window size must be 1180×780 logical px. |
| Window minimum content | `800 × 520` | Enforce equivalent minimum. |
| Tool rail width | `56` | Fixed 56 logical px. |
| Tool item visual frame | `36 × 36` | Button content/hit visual must occupy this frame. |
| Tool stack spacing | `10` | No separator bars between tool groups. |
| Tool stack top padding | `16` | Exact. |
| Tool stack bottom padding | `12` | Exact. |
| Tool icon nominal size | `17` system icon; `18 × 18` custom | Reproduce bounds, not Qt’s current 22×22 icon box. |
| Selected tool radius | `7` | Exact. |
| Tool option/header height | `42` | Fixed. |
| Tool header horizontal padding | `18` | Exact unless a canonical header has explicit variant. |
| Tool header common spacing | `12` | Use component-specific exceptions below. |
| Crop header spacing | `14` | Exact. |
| Status bar height | `30` | Fixed. |
| Status horizontal padding | `18` | Exact. |
| Status item spacing | `16` | Exact. |
| Status zoom width | `62` | Leading aligned. |
| Layers panel default width | `252` | Persist user width. |
| Layers panel min/max | `202…352` | Clamp resize to this range. |
| Panel resize hit target | `8` | Transparent hit area centered on divider. |
| Layer base row height | `52` | Exact. |
| Layer row intercell vertical spacing | `2` | Preserve equivalent separation. |
| Effect subrow height | `24` | Add per effect. |
| Layers header padding | `18` | Exact. |
| Layers appearance padding | `12` | Exact. |
| Layers footer outer padding | H `8`, V `4` | Icon hit areas add H 8, V 12. |
| Ruler thickness | `18` | Horizontal and vertical. |
| Project tab strip height | `34` | Exact. |
| Project tab height | `28` | Exact. |
| Project tab inter-tab gap | `6` | Exact. |
| Project tab text area | min `35`, max `155` | Before internal horizontal padding/close control. |
| Tab leading/trailing padding | `11 / 8` | Text/select button. |
| Tab close button | `16 × 28`, trailing pad `5` | Exact. |
| Tab edge fade | `28` | Right always; left only when scrolled from start. |
| Canvas ruler major tick target | ~`70` points | `majorStep` chooses 1–2–5 style step. |

### 3.1 Coordinate formulas

For the editor body, Linux must reproduce this logical hierarchy rather than relying on dock/tool-bar layout side effects:

```text
window content
└─ VStack spacing 0
   ├─ tool options/header: 42 high
   ├─ HStack spacing 0
   │  ├─ tool rail: 56 wide
   │  ├─ 1-device-pixel divider
   │  ├─ canvas/ruler viewport: remaining width
   │  ├─ resizer: visual divider + 8 logical px hit region
   │  └─ layers panel: persisted width 202…352
   ├─ 1-device-pixel divider
   └─ status: 30 high
```

The macOS toolbar/tab strip sits in window toolbar chrome above this content; the Linux reconstruction may be inside the client area, but its **visible geometry must match the captured macOS reference**.

### 3.2 Divider rule

SwiftUI/AppKit `Divider` thickness is effectively one device pixel at the active backing scale. Custom Qt dividers must snap to device pixels:

```text
hairlineLogical = 1 / devicePixelRatio
```

Do not blindly draw a 1-logical-pixel line at 125%, 150%, or 200% if it produces a 1.25/1.5/2-device-pixel blur.

---

## 4. Typography

### 4.1 Canonical macOS typography

The app primarily uses the macOS system UI family (San Francisco/SF Pro through SwiftUI/AppKit), not a bundled font.

| Role | macOS source specification |
|---|---|
| Tool header title | system `13`, semibold |
| Tool header control text | system `12`, regular |
| Layers title | system `12`, semibold |
| Layer name | system `13`, regular |
| Layer dimensions/subtitle | system `10`, secondary |
| Layer effect label | system `11` |
| Project tab | system `12`, active semibold, inactive medium |
| Tab close glyph | system symbol `9`, semibold |
| Status | system `11`, tabular/monospaced digits where specified |
| Ruler labels | monospaced-digit system font `8`, regular |
| Empty layers callout | callout medium; explanatory text caption |
| Color picker hex | system body, monospaced design |

Unless the macOS source explicitly changes letter spacing, use **0 tracking adjustment**. Unless a fixed line height is specified, use the font’s native line metrics and size the containing control to the canonical geometry.

### 4.2 Linux font fallback

Apple’s system font must not be copied or redistributed with Linux Compositor unless licensing explicitly permits it. There is no guaranteed open-source drop-in that is metrically identical to SF Pro.

Use this preference order:

1. `Inter` for general UI text when installed/bundled under its license.
2. `Noto Sans`.
3. distro system sans only as last fallback.
4. `Noto Sans Mono`/`DejaVu Sans Mono` only for roles that are actually monospaced, not for all numeric fields.

Expected difference: glyph widths, vertical metrics, kerning and hinting will not be pixel-identical to SF Pro. Compensate **component widths/padding**, not font scaling, to keep geometry aligned. Do not reduce font size merely to force text into a mismatched Qt control.

For tabular numbers, enable OpenType `tnum` if the selected Linux font supports it.

### 4.3 Truncation

- Project tab titles: one line, constrained to max 155 logical px text frame, tail truncation.
- Type font picker: fixed 210 logical px, single line, tail truncation; selecting a long font must not widen the control.
- Layer names: single line, tail truncation.
- Layer/effect labels: tail truncation within remaining row width.

---

## 5. Color and state tokens

### 5.1 Explicit source colors

These values are directly specified by macOS source and may be reproduced exactly in sRGB unless screenshot/color-management testing proves a conversion is required.

| Token | Canonical value |
|---|---|
| Main editor background | `white 0.14` (`#242424` nominal sRGB) |
| Ruler background | `white 0.20` (`#333333`) |
| Ruler tick | `white 0.62` (`#9E9E9E` nominal) |
| Ruler label | `white 0.78` (`#C7C7C7` nominal) |
| Ruler boundary | `white 0.08` (`#141414` nominal) |
| Selected tool fill | white at `12%` alpha |
| Selected tool border | white at `14%` alpha |
| Active tab fill | white at `12%` alpha |
| Inactive tab fill | white at `3.5%` alpha |
| Active tab border | white at `22%` alpha |
| Inactive tab border | white at `8%` alpha |
| Targeted tab fill | accent at `30%` alpha |
| Layer row hairline | white at `6%` alpha, exactly one device pixel |
| Selected layer effect row | system accent at `30%` alpha |
| Swatch outer border | black, 1 px |
| Palette swatch inner border | white, 1.5 px inset 1 |

### 5.2 Semantic macOS colors

`.primary`, `.secondary`, `.tertiary`, `.quaternary`, `.accentColor`, `.labelColor`, `.secondaryLabelColor`, and native selected-row colors must be captured from the canonical macOS build in dark appearance.

**Rule:** do not map semantic roles directly to Qt palette roles and accept whatever the current desktop theme produces. Define Compositor-owned Qt tokens based on the reference capture.

### 5.3 Interactive states

For every custom component, validate these states independently: normal, hover, pressed, checked/selected, focused, disabled, inactive-window.

General rules:

- `buttonStyle(.plain)` controls do not acquire generic Qt button chrome.
- Tool rail hover must not use the current global Qt toolbar hover treatment unless a macOS capture shows an equivalent visible fill.
- Checked/selected state is persistent; pressed is transient and must not be conflated with selection.
- Focus rings belong only where macOS shows focus. Do not add a blue Qt border to every focused custom field if the reference differs.
- Disabled text/icon opacity must be captured from macOS semantic disabled rendering. Avoid Qt theme-dependent disabled colors.
- Destructive actions only use destructive color if macOS does; the Layers trash icon is normally secondary, not permanently red.

---

## 6. Top-level application structure

### 6.1 Window

Canonical macOS:

- default size 1180×780;
- minimum editor content 800×520;
- dark appearance;
- unified compact toolbar with title hidden;
- first-launch placement may fill the visible screen;
- project title is the window title even though toolbar title is hidden.

Linux currently calls `resize(1200, 800)` and uses a client-side Qt menu/header/toolbars/docks. Required: visible client geometry must be normalized to the canonical reference; initial normal size must be 1180×780 logical px.

### 6.2 Toolbar / document tabs

Canonical macOS toolbar order:

1. New canvas button.
2. fixed navigation spacer.
3. project tab strip.
4. flexible spacer.
5. `Fit`.
6. `100%`.
7. zoom out icon.
8. zoom in icon.

The tab strip is 34 high. Individual tabs are 28 high capsules with 6 gap. Active/inactive fill and border tokens are defined above. Modified tabs show a 5×5 dot before the title. Drop targeting changes the capsule to accent fill/border. Scrolled edges fade over 28 logical px.

The current Linux `QTabBar` with rectangular rounded tabs is not sufficient. It also initializes a single `Untitled 1` tab and does not reproduce the macOS `ProjectWorkspace` switching/drop behavior. Implement a custom tab strip or heavily customized Qt view that matches geometry and interactions.

### 6.3 Tool rail

Canonical rail:

- 56 wide, vertically scrollable, hidden scrollbar;
- `VStack(spacing: 10)`;
- 36×36 tool visual frames;
- no separator bars;
- custom/system glyph bounds 17–18;
- selected fill white 12%, radius 7, border white 14%;
- foreground/background control below tools with 8 top padding;
- top/bottom rail padding 16/12.

### 6.4 Canvas viewport

The viewport occupies all remaining space between tool rail and Layers panel. Optional 18-point rulers reduce the canvas area. The macOS implementation owns viewport transforms, zoom, panning, guide interaction, selection overlays, transform controls and cursor state. Linux must call the same core semantics and reproduce overlay geometry; painting only the final composited image is not UI parity.

### 6.5 Status bar

Canonical: 30 high, HStack spacing 16, horizontal padding 18, system 11. It displays zoom, dimensions, `sRGB · Transparent`, busy/import state, and tool-specific interaction hints. Linux’s current QStatusBar minimum height 24, per-label 8 px padding and vertical separators diverge; set the canonical 30 height/18 outer padding/16 gaps and remove separators not present on macOS.

---

## 7. Tool parity

### 7.1 Canonical tool rail order and model

The exact `NavigationTool.allCases` visible order is:

| # | Canonical rail tool | Shortcut | State/mode notes | macOS source |
|---:|---|---|---|---|
| 1 | Move / Transform | `V` | Default selected tool | `EditorSession.swift` |
| 2 | Marquee | `M` | Rectangle/Ellipse are modes of one rail tool; repeated M cycles | `EditorSession.swift`, `LassoControls.swift` |
| 3 | Lasso | `L` | Freehand/Polygonal modes; repeated L cycles | same |
| 4 | Magic | `W` | Wand/Object modes; Tab cycles current tool mode | same |
| 5 | Crop | `C` | preview then Apply/Cancel | `CropControls.swift` |
| 6 | Brush | `B`; Eraser mode `E` | Eraser is Brush mode, not separate rail item | `BrushControls.swift` |
| 7 | Spot Healing Brush | `J` | healing mode controls | same |
| 8 | Clone Stamp | `S` | Option/Alt-click source | same |
| 9 | Smear | `R` | blur/smudge/liquify family modes | same |
| 10 | Gradient | `G` | dedicated options and interactive edit | `GradientControls.swift` |
| 11 | Shape | `U`; Shift-U cycles kind | Rectangle/Ellipse/Line | `ShapeControls.swift` |
| 12 | Type | `T` | live text editing | `TypeControls.swift` |
| 13 | Eyedropper | `I` | Sample Ring toggle | `ContentView.swift` |
| 14 | Hand | `H`; Space hold temporary | pans viewport | `NavigationToolHeader.swift`, `EditorCanvas.swift` |
| 15 | Zoom | `Z` | click zoom; Alt/Option reverses; header zoom entry | same |

`idle` exists in the enum but is not shown as a rail tool.

### 7.2 Current Linux tool-rail differences

The baseline Linux rail exposes separate Rectangular and Elliptical Marquee actions and a separate Eraser action; inserts separators; uses 22×22 generated icons; defaults Brush selected; and has 44-ish toolbar styling rather than the canonical 56 rail. These are structural parity failures, not acceptable platform differences.

### 7.3 Tool cursor rules

- Move: transform/move/resize/rotation cursors must follow macOS hit regions and modifier state.
- Marquee/Lasso/Wand: crosshair/selection-mode cursor semantics must match; held modifiers affect selection mode without permanently mutating the selected segmented mode.
- Brush-family: brush outline reflects logical brush diameter at current zoom; clone source state has its own feedback; cursor remains sharp at HiDPI.
- Eyedropper: eyedropper cursor and Sample Ring behavior.
- Hand: open hand at rest, closed/grabbing while panning if reference shows it; Space temporarily invokes Hand and restores the prior tool on release.
- Zoom: zoom-in normally; zoom-out with Alt/Option equivalent.
- Type: I-beam/text placement/edit cursors where macOS shows them.

---

## 8. Tool-specific option bars

All bars are 42 high. Unless stated otherwise: outer horizontal padding 18, primary HStack spacing 12, control font system 12, title system 13 semibold, regular control size.

### 8.1 Move / Transform

Canonical controls in order:

1. title `Transform` or `Transform Mask`, leading 18;
2. `Auto Select`;
3. `Show Controls`;
4. horizontal scrolling numeric group with 12 spacing and inner H padding 18:
   - X 85 wide;
   - Y 85;
   - W 85;
   - H 85;
   - aspect lock button;
   - Scale 110 with `%`;
   - rotation 75 with `°` label;
   - Sampling picker 170;
   - Flip H;
   - Flip V;
5. Cancel, Escape shortcut;
6. Apply, Return shortcut;
7. trailing 18.

Current Linux differences: title `Move / Transform`; extra `Ignore Transparent Pixels`; 64-wide spinboxes; no Scale, Sampling, Flip H/V, Apply, Cancel; `Link` text checkbox rather than link button; no mask-specific title; different value ranges/commit model. Replace the Linux composition with the canonical sequence.

### 8.2 Brush / Eraser / Spot Healing / Clone / Smear

Canonical shared controls:

- dynamic title;
- brush Paint/Erase segmented mode only for Brush;
- Smear family mode segmented control;
- Spot Healing type segmented control;
- Clone: Aligned toggle + Sample segmented `This Layer` / `All Layers`;
- Size field width 48, `px`, range 1…2000;
- Hardness slider width 100 + field width 42 + `%`;
- Opacity/Strength slider width 100 + field width 42 + `%`; canonical editable minimum is 1%;
- Brush Smoothing slider width 100 + field width 42;
- Color swatch for applicable paint/heal tools: 34×18, radius 4, inner white and outer black borders;
- mask mode substitutes 180-wide Black/Hide vs White/Reveal picker;
- Clone without a source shows secondary guidance.

Current Linux has 80-wide Hardness/Opacity sliders, 54-wide spin fields, a 36×20 color button, no smoothing, no complete Smear modes, and clone/healing option pages that omit shared brush controls. More importantly, mouse command construction currently hard-codes Clone `aligned=1`, `sampleAllLayers=0` and Spot Healing mode, so displayed controls are not authoritative. Wire every displayed value to the core command.

### 8.3 Marquee / Lasso / Magic

Canonical:

- title is `Marquee`, `Lasso`, or `Magic`;
- Marquee segmented Rectangle/Ellipse;
- Lasso segmented Freehand/Polygonal;
- Magic segmented Wand/Object;
- selection-mode segmented control, with held Shift/Option reflected transiently;
- Wand: Tolerance field width 44, Sample Size picker, Sample This Layer/All Layers, Contiguous;
- Object: Sample picker and Edge field width 40, −10…10 px;
- Anti-alias only for applicable modes (not rectangular marquee);
- divider height 18;
- Expand/Contract amount fields 40 wide, valid 1…500;
- Feather field 48 wide, max 250;
- Deselect shown according to selection state.

Current Linux exposes separate rail marquee tools, uses generic title `Selection`, spacing 8, omits Magic Object mode and Sample Size, omits Feather in the option bar, and its Magic tool command currently sends hard-coded tolerance `32`, contiguous `1`, sample-all `0` regardless of visible widgets. This is P0 behavioral divergence.

### 8.4 Crop

Canonical HStack spacing 14: title, Ratio picker fixed 170 (`Free`, `Original`, `1:1`, `4:3`, `16:9`), live dimensions text when a crop rect exists, Spacer, Cancel, Apply Crop.

Crop dragging creates/edits a preview rectangle; it does **not** immediately crop the canvas on mouse release. Linux currently executes `cropCanvas` directly on release. Required: maintain a pending crop edit and apply only via Apply/Return; Escape/Cancel restores unchanged document.

### 8.5 Gradient

Canonical includes Linear/Radial segmented picker, a 56×18 gradient swatch (radius 3, checkerboard underlay, 1 px black/50% border), Colors picker, Reverse toggle, Opacity slider 100 + field 42 `%`, mask indicator where applicable, and Cancel/Apply while an edit is active.

Linux currently sends Gradient to the idle options page and has no canvas behavior in the main mouse switch. Implement the full canonical tool before calling it parity-complete.

### 8.6 Shape

Canonical: segmented Rectangle/Ellipse/Line; line Width slider 100 + field 48 px; rectangle Radius slider 100 + field 48 px; Fill swatch 36×18 radius 3, black 50% 1 px border.

Linux currently has no Shape options page or canvas action. Implement all modes and Shift-U cycling.

### 8.7 Type

Canonical horizontal options:

- font picker 210 fixed width, tail truncation;
- Size field 52 + px;
- color swatch 36×18;
- alignment buttons: each 30×26, 2 gap, selected white 14% fill radius 4;
- Tracking field 45;
- Leading field 52; empty/0 displays `Auto`, meaning 120% font size;
- Cancel/Done during text draft, otherwise Edit Text when an active live-text layer exists.

Linux currently has no Type option page and main mouse handler does nothing for Type. This is P0.

### 8.8 Eyedropper

Canonical: title `Eyedropper`, header spacing 16, `Sample Ring` toggle, Spacer. Linux currently samples from the displayed composited QImage and shows idle options. Implement the header and ring behavior.

### 8.9 Hand

Canonical header title `Pan`. Hand drags must change viewport offset, including temporary Space-hand behavior. Linux currently marks Hand mouse-down as painting but its mouse-move switch does not pan; this is P0.

### 8.10 Zoom

Canonical header title `Zoom`; percentage field width 72, rounded, trailing alignment + `%`, valid 0.1…3200%, applies on submit/focus loss, Up/Down step 1%, Shift step 10%.

Linux currently has no Zoom options page. Click zoom uses 1.25/0.8 factors; verify against macOS viewport zoom steps and pointer-centered behavior before acceptance.

---

## 9. Foreground/background palette control

Canonical `ColorPaletteControls`:

- overall frame 36×36;
- foreground swatch 24×24 at (0,0);
- background swatch 24×24 at (+12,+12);
- swatch corner radius 6;
- inner white border 1.5 inset by 1, outer black border 1;
- swap icon frame 12×12, offset (27,−3), symbol size 9 medium, rotated 45°;
- reset icon frame 12×12, offset (−1,27), symbol size 7.5 medium;
- default colors: foreground black, background white;
- X swaps, D resets;
- if a mask target is selected, swatch click opens mask Black/Hide vs White/Reveal popover instead of the ordinary color picker.

Current Linux uses 22×22 swatches, 36×42 container, radius 4, text glyphs `⇄` / `⟲`, different offsets, and baseline foreground red. Required: implement the canonical geometry and default state.

---

## 10. Layers panel

### 10.1 Panel shell

- width default 252, range 202…352;
- no independent floating/closable dock chrome;
- header HStack padding 18: `Layers` system 12 semibold; count caption with tabular digits, tertiary;
- divider;
- appearance controls;
- divider;
- layer list or empty state;
- divider;
- footer.

Current Linux QDockWidget minimum width is 252 and adds dock behavior/title chrome. Use a fixed right-side panel with only the macOS-visible header; resizing occurs via the left edge only.

### 10.2 Appearance controls

VStack spacing 8, padding 12:

- Blend caption + grouped BlendMode popup. The popup groups modes with separators and previews the highlighted mode while the menu is open, reverting on cancel and committing on selection.
- Opacity row spacing 6: caption, slider, 44-wide rounded text field + `%` with 2 gap.
- opacity edit is a single undo transaction from drag begin to drag end.
- field Up/Down ±1%, Shift ±10%, focus release returns keyboard focus to canvas.

A plain Qt combo that commits only on activation is not behaviorally equivalent to the macOS highlighted-item live preview.

### 10.3 Layer list geometry

Canonical table:

- base row 52;
- effect rows add 24 each;
- table intercell height 2;
- multiple and empty selection enabled;
- vertical scroller;
- row bottom hairline white 6%, one device pixel.

Canonical base row positions include:

- eye: leading 8, width 20, height 32, center Y 26;
- disclosure: width 16, height 24, center Y 26;
- group nesting: 24 logical px per depth, max visual depth 8;
- clipping adds another 24 indentation;
- layer thumbnail slot 36 wide/high region centered at Y 26;
- mask gap 5;
- mask thumbnail nominal 30;
- mask link button 9×20;
- name: system 13, leading 5 after mask slot, trailing 8, top 9;
- dimensions: system 10, 3 below name;
- thumbnails radius 3.

Current Linux row height happens to be 52 but omits the canonical mask thumbnail/link composition and effect subrows, uses a subtitle such as `· mask`, uses different eye/text geometry, and hard-codes selected/hover colors. Reproduce the macOS row structure rather than merely retaining 52 px height.

### 10.4 Layer interactions

Required:

- select on mouse-down without delaying drag;
- native multi-selection semantics;
- selected layers remain selected after reorder;
- empty selection is allowed;
- double-click name renames inline;
- double-click a text/adjustment control edits its content instead of renaming;
- layer thumbnail click targets layer; mask thumbnail targets mask;
- Cmd/Ctrl-click appropriate thumbnail loads pixels/mask as selection; modifiers add/subtract as canonical;
- Option/Alt-drag layer duplicates while preserving hierarchy;
- drop onto folder inserts into folder; drop between rows reorders;
- Option/Alt-drag mask copies mask to target row;
- Option/Alt-drag effect copies the effect to target layer;
- effect row single-click selects; double-click edits;
- eye click toggles visibility; dragging across eyes applies the same visibility state as one undo transaction;
- Option/Alt interaction in clipping zone creates/releases clipping mask with dedicated cursor;
- collapsed folder IDs persist across refresh; refresh must not force `expandAll()`;
- refresh must preserve multi-selection, not reduce it to only the active layer.

### 10.5 Layer footer

Canonical left-to-right:

1. New blank layer (`plus.square`).
2. New folder/group (`folder.badge.plus`).
3. Add layer mask (`rectangle.inset.filled`).
4. Layer effects menu (`sparkles`).
5. Adjustment layer menu (`circle.lefthalf.filled`).
6. Spacer.
7. Delete (`trash`).

HStack spacing 0; each icon supplies H 8/V 12 clickable padding; footer outer padding H 8/V 4; secondary foreground.

Linux currently lacks the canonical layer-effects footer control and uses custom 30×28 buttons/spacing.

### 10.6 Layer context menu

Canonical row menu order:

`Rename…`; `Hide/Show Layer`; `Add White Mask`; `Add Black Mask`; `Enable/Disable Mask`; `Delete Mask`; `Release Clipping Mask`; `Move Out of Folder`; `Delete Layer / Folder`.

Titles/enabled states must reflect the target row. Do not substitute the top-level Layer menu for this context menu.

---

## 11. Rulers, guides, grid and canvas chrome

### 11.1 Rulers

- thickness 18;
- background white 0.20;
- major ticks 8, midpoint ticks 5, minor ticks 3;
- tick white 0.62;
- label white 0.78, monospaced-digit system 8;
- one-device-pixel boundary white 0.08;
- major tick spacing selected to approximately 70 logical points using the canonical 1/2/5 sequence;
- ruler drag creates guide; dragging back over ruler deletes it;
- vertical guide cursor left-right; horizontal guide cursor up-down.

Current Linux main canvas shell has no equivalent visible ruler implementation in `SessionWindow.cpp`; menu checkboxes alone are not parity.

### 11.2 Canvas surround

The macOS editor background is white 0.14. Do not use Linux’s current `#242528` merely because it is close; use the canonical token/captured reference. Checkerboard size/colors, document border/shadow, pixel-grid threshold, selection marching ants, transform handles and guide colors must be screenshot-compared against macOS rather than invented.

### 11.3 Zoom/pan behavior

Verify:

- Fit and Actual Pixels produce the same document rect at the same logical viewport size;
- zoom keeps the same anchor behavior as macOS;
- wheel/trackpad modifiers match; ordinary wheel should not be consumed for Ctrl-only behavior if canonical macOS uses direct scroll/pan;
- Hand and Space-hand pan continuously;
- zoom and pan update rulers/guides/status synchronously;
- 800% pixel grid threshold exactly matches the View command text/behavior.

---

## 12. Menus and commands

### 12.1 Top-level order

Linux must expose exactly this Compositor-owned top-level order:

`File · Edit · View · Select · Image · Filter · Layer · Window · Help`

The macOS application menu itself is Category C and is not added as another Linux top-level menu.

### 12.2 Modifier translation

For platform-equivalent shortcuts:

- macOS Command (`⌘`) → Linux Control (`Ctrl`);
- macOS Option (`⌥`) → Linux Alt (`Alt`);
- Shift remains Shift;
- unmodified tool keys remain identical.

Do not change command keys merely because Qt defines a different standard binding. Where macOS uses responder-chain behavior in text fields, Linux must first route standard text editing and invoke canvas commands only when the canvas/layer responder context owns the shortcut.

### 12.3 File

Canonical order:

1. New Canvas… — Cmd/Ctrl-N
2. Open Project… — Cmd/Ctrl-O
3. Import Images…
4. separator
5. Save — Cmd/Ctrl-S
6. Save As… — Shift-Cmd/Ctrl-S
7. separator
8. Export PNG… — Shift-Cmd/Ctrl-E
9. Export JPEG… — Option/Alt-Shift-Cmd/Ctrl-S
10. separator
11. Close Project — Cmd/Ctrl-W

Linux-visible `Export TIFF…` and `Export WebP…` have **no macOS menu equivalent** and must not appear in parity mode. The codec APIs may remain internal. `Quit` belongs to macOS’s app menu; on Linux it is Category C. If retained in File as the Linux desktop equivalent, document it explicitly as platform-only and exclude it from screenshot comparison, or expose it only through desktop integration rather than inserting it between canonical items.

### 12.4 Edit

Canonical command sequence includes dynamic Undo/Redo, Cut/Copy/Copy Merged/Paste, Keyboard Shortcuts…, Fill with Foreground Color, Fill with Background Color, Clear Selection Pixels, Content-Aware Fill… with canonical separators.

Linux currently adds `Command Palette…`, which has no canonical macOS menu equivalent. Hide/remove it from parity UI.

Responder behavior is part of parity: Undo/Redo/Cut/Copy/Paste/Select All/fill shortcuts must behave as normal text editing when a native text field owns focus; they must not destructively modify the canvas.

### 12.5 View

Canonical order/state:

- Fit Canvas;
- Actual Pixels;
- Zoom In;
- Zoom Out;
- Pixel Grid (800% and above);
- Snap state where canonical source places it;
- Show Transform Controls only in Move context;
- Show submenu: Grid, Guides;
- Rulers;
- separator;
- Snap command (Shift-Cmd/Ctrl-;) and `Snap To` submenu: Guides, Grid, Layers, Document Bounds;
- separator;
- Lock Guides;
- Clear Guides.

Every toggle must actually alter canvas/ruler behavior. Linux currently creates several checkable actions without connecting all of them to rendering/guide state; visible checked items with no effect fail parity.

### 12.6 Select

Canonical:

`All`; `Deselect`; `Inverse`; `Layer’s Pixels`; `Subject`; `Mask’s Black Areas`; separator; `Expand…`; `Contract…`; `Feather…`.

Linux currently appends `Rectangle Selection` and `Ellipse Selection` for automation compatibility. These have no macOS equivalent and must not be user-visible. Preserve automation through hidden actions/test APIs instead.

### 12.7 Image

Canonical:

`Curves…`; `Levels…`; `Hue/Saturation…`; `Black & White…`; `Color Balance…`; `Exposure…`; `Gradient Map…`; `Grain…`; `Invert` / `Invert Mask`; separator; `Canvas Size…`; `Image Size…`; separator; `Flip Canvas Horizontal`; `Flip Canvas Vertical`.

Dynamic Invert title and enabled state must mirror macOS target state.

### 12.8 Filter

Canonical `FilterKind` excludes Content-Aware Fill and image adjustments from this menu, leaving:

- Gaussian Blur…
- Motion Blur…
- Add Noise…
- Lens Correction…
- Remove Background…

Linux currently has the same visible set, but Remove Background must use the same panel/quality/settings flow as macOS where applicable rather than an immediate fixed command if macOS presents editable settings.

### 12.9 Layer

Canonical order:

1. New Adjustment Layer submenu.
2. Edit Adjustment…
3. separator.
4. dynamic `Transform Selection` / `Transform Layer` — Cmd/Ctrl-T.
5. dynamic `Duplicate Layer` / `Layer via Copy` — Cmd/Ctrl-J.
6. separator.
7. dynamic `Create Clipping Mask` / `Release Clipping Mask` — Option/Alt-Cmd/Ctrl-G.
8. separator.
9. Group Selected Layers — Cmd/Ctrl-G.
10. Move Out of Folder.
11. New Blank Layer — Shift-Cmd/Ctrl-N.
12. Rename Layer…
13. dynamic Show/Hide Layer.
14. separator.
15. Move Layer Up — Cmd/Ctrl-].
16. Move Layer Down — Cmd/Ctrl-[.
17. dynamic Merge title — Cmd/Ctrl-E.
18. separator.
19. Flip Layer Horizontal.
20. Flip Layer Vertical.
21. separator.
22. dynamic Delete: selected effect / mask / multiple layers / layer.

Linux currently adds a `Layer Mask` submenu not present in the macOS top-level Layer menu. Mask commands remain available through the layer footer/context menu; remove the extra top-level submenu in parity mode.

Dynamic Delete must include selected-effect state (`Delete <Effect>`), not just layer/mask/multi-layer state.

### 12.10 Window and Help

macOS Window/Help are substantially system-owned (Category C). Linux’s `Reset Workspace Layout`, `Tools`, `Options Bar`, `Layers`, persistent `Adjustments` dock toggles do not correspond to canonical custom commands and make the UI hierarchy diverge. Do not expose them in parity mode unless the current macOS app adds equivalent commands.

`About Compositor`, `Check for Updates…`, Quit, Hide and related application-menu behavior are platform placement exceptions. Preserve the functionality using the closest Linux convention, but do not use those exceptions to introduce arbitrary extra editor commands.

### 12.11 Dynamic menu state rules

At minimum reproduce:

- `Undo <name>` / `Redo <name>`; when text/native editing owns focus, delegate to text responder and use native semantics;
- `Invert` / `Invert Mask`;
- `Transform Layer` / `Transform Selection`;
- `Duplicate Layer` / `Layer via Copy`;
- `Create Clipping Mask` / `Release Clipping Mask`;
- `Show Layer` / `Hide Layer`;
- `Merge Down` / `Merge Layers` / `Merge Group` according to `session.mergeTitle`;
- `Delete <Effect>` / `Delete Layer Mask` / `Delete Layers` / `Delete Layer`;
- selection-only commands enabled only when canonical capability says so.

Do not infer state from the Qt selection model if the canonical session exposes a stronger state value; use the session as authority.

---

## 13. Keyboard parity

Canonical configurable shortcuts include the menu shortcuts plus:

- A Select/idle tool; V Move; H Hand; Z Zoom; B Brush; E Eraser mode; J Spot Healing; S Clone; T Type; G Gradient; U Shape; I Eyedropper; M Marquee/cycle; W Magic; L Lasso/cycle; R Smear family; C Crop;
- X swap colors; D reset colors; Tab cycle tool mode; Space temporary Hand;
- Delete contextual delete; Return apply; Escape cancel;
- `[`/`]` brush size; Shift-`[`/`]` hardness;
- Shift-`-`/`=` previous/next blend mode;
- Shift-U cycle shape kind;
- numeric opacity shortcuts including two-digit entry;
- arrows nudge layer 1 px; Shift arrows 10 px;
- Cmd/Ctrl arrows move selected pixels 1 px; Shift-Cmd/Ctrl arrows 10 px;
- text-editing-specific tracking/leading shortcuts.

Linux’s current `Keyboard Shortcuts…` is an informational `QMessageBox`; macOS provides an editable shortcut configuration panel with conflict/reserved-key validation. Rebuild that panel and route translated events only at canvas/layer boundaries so text fields retain normal editing behavior.

---

## 14. Dialogs, floating panels, popovers and color controls

### 14.1 Floating tool panels

Canonical tool dialogs use a movable non-modal `NSPanel`:

- titled + closable;
- floating;
- hides on app deactivation;
- first open centered over canvas;
- subsequent opens restore last top-left position for the app session;
- content change does not move panel;
- close button is treated as Cancel where applicable;
- panel can regain key focus after sampling the canvas.

Qt implementation should use a non-modal tool window with equivalent parent/activation semantics, not a blocking `QDialog::exec()` unless the macOS counterpart is actually modal.

### 14.2 New Canvas

Canonical `NewCanvasSheet`:

- VStack spacing 24, padding 28, max width 500;
- title `New canvas`, title2 semibold;
- subtitle secondary;
- Width/Height controls separated by multiply glyph, H spacing 16;
- dimension inner field padding 12, radius 7;
- validation text;
- actions: Open project, Import image, Spacer, Create canvas;
- Create uses Return and is prominent;
- default 1920×1080 unless clipboard image dimensions provide a valid suggestion;
- width field initially focused.

Linux File > New Canvas currently opens `SizeDialog` configured as canvas resizing, which is not the New Canvas UI and derives dimensions from current session state. Replace with a dedicated canonical New Canvas implementation.

### 14.3 Image Size

Canonical width 430, padding 24, VStack spacing 18, title2 bold. Preserve Units, Width, Height, Lock aspect ratio, Resolution, Resample, Sampling, explanatory text, result/error text, Cancel/Resize order and Return/Escape behavior exactly.

### 14.4 Canvas Size

Canonical width 450, padding 24, VStack spacing 16. It includes current dimensions and uncompressed memory text, Units, Width/Height, Relative toggle, `Lock original aspect ratio`, result summary, a **3×3 25×25 anchor grid with 3 px gaps**, textual anchor explanation, Canvas extension choices `Transparent`, `Foreground`, `Background`, `Black`, `White`, `Custom`, optional custom picker, then Cancel/OK.

Linux currently uses a ComboBox for anchor and lacks Foreground/Background extension choices. Replace it with the canonical 3×3 control and complete option list.

### 14.5 Selection Expand/Contract/Feather panel

Canonical custom panel: padding 24, width 380, VStack spacing 16; row HStack spacing 10; label min width 60; slider + 56-wide field + px; validation callout; divider; Cancel Escape and OK Return prominent.

Linux currently uses `QInputDialog::getInt`, which is not visual or interaction parity.

### 14.6 Color picker

Canonical picker:

- non-modal floating panel;
- outer HStack top alignment, spacing 14, padding 20, fixed size;
- saturation/brightness field exactly 256×256;
- selector circle 12×12, white 1.5 and black 0.75 outline;
- hue strip 20 wide with 7 horizontal padding, overall 34 wide, 256 high;
- right column width 180, height 256;
- preview 64×64, radius 5, black 60% border 1;
- OK/Cancel button column width 90, large control size, spacing 8;
- RGB fields width 52; hex field width 84; grid H spacing 8/V 6;
- live canvas sampling while picker remains open;
- closing title-bar button cancels.

A generic `QColorDialog` is not acceptable. Linux `ColorPickerDialog` must be verified component-by-component against this geometry/behavior.

---

## 15. Interaction parity rules

### 15.1 Focus

- Canvas is the normal keyboard-command owner.
- Committing or escaping property fields releases field focus and returns focus to canvas where macOS does.
- Text editing must retain native text shortcuts and not trigger canvas commands.
- Floating adjustment/color panels can be key while the canvas remains clickable for sampling where canonical allows.

### 15.2 Selection and multi-selection

- Row selection must support ordinary single, additive and range selection using Linux equivalents of Command/Shift.
- Clicking a layer name while its mask is targeted retargets the layer itself.
- Selection refreshes must not recreate thumbnails unnecessarily or discard multi-selection.
- Effect selection is separate from layer selection.

### 15.3 Drag and drop

Reproduce allowed source/target combinations and operation semantics exactly. In particular, Option/Alt changes copy semantics for layers/masks/effects and external file/image drags can target current canvas, an existing project tab, or new tab as macOS permits. Drag targeting must show the same border/capsule feedback.

### 15.4 Double-click

Layer name → rename. Text/adjustment control region → edit content. Polygonal lasso double-click → close selection according to canonical behavior. Do not apply a generic double-click action to the entire row.

### 15.5 Escape / Return

These are contextual commands, not global dialog aliases:

- pending transform: Cancel / Apply;
- pending crop: Cancel / Apply;
- gradient/text/shape operations as canonical;
- floating dialogs: cancel/accept where explicitly configured;
- text fields: honor local editing semantics before canvas routing.

### 15.6 Undo/Redo

A continuous interaction that macOS records as one edit must remain one Linux undo step: brush stroke, opacity drag, visibility eye swipe, transform drag, guide drag, etc. Dynamic menu title must expose the same edit name.

### 15.7 Wheel/trackpad/tablet

- Reproduce macOS wheel/trackpad pan/zoom semantics; do not require Ctrl if macOS does not.
- Maintain pointer/document anchor while zooming where canonical does.
- Tablet pressure may be a platform implementation detail, but resulting brush size/opacity behavior must match canonical input semantics.

---

## 16. macOS-to-Qt implementation rules

1. **Do not use one global QSS as the design system.** The current global rules impose hover/focus/button chrome that differs from many `buttonStyle(.plain)` and AppKit controls. Use component-specific styles/delegates.
2. Build a `ParityMetrics`/`ParityPalette` source in Qt with the constants in this document. Avoid repeated magic numbers in `SessionWindow.cpp`.
3. Replace `QToolBar` layout dependence for the left rail with a fixed-width custom widget. QToolBar adds platform spacing, margins, separator metrics and overflow behavior that are difficult to make pixel-stable.
4. Replace the Layers `QDockWidget` with a fixed right panel in the main editor layout. Dock floating/closing/title chrome is not canonical.
5. Use a custom tab strip rather than default `QTabBar` painting.
6. Use `QStyledItemDelegate`/custom view painting for layer rows, but mirror the macOS hierarchy (mask thumbnail, link, effect rows), not only colors.
7. Use vector/path icons with canonical 17–18 logical bounds. SF Symbols cannot be redistributed as assets; implement visually equivalent custom paths/icons (Category B) and verify overlays.
8. Route state through the Swift/session core wherever possible. Qt widgets must not hold an independent “looks selected” state that can diverge from `EditorSession` state.
9. Segmented controls must behave like the canonical macOS segments: one group, contiguous shared shape, selected state, keyboard/Tab behavior. Separate pill buttons with 8 px gaps are not equivalent.
10. Dialog modality must follow macOS; use `show()` tool windows for floating panels, not `exec()` by default.

---

## 17. HiDPI and fractional scaling

### 17.1 Required scales

Verify at 100%, 125%, 150%, and 200%.

### 17.2 Logical geometry

All canonical geometry remains in logical units. A 56 logical-px rail remains 56 at every scale; Qt maps it to device pixels.

### 17.3 Qt policy

Explicitly set and test a high-DPI scale factor rounding policy before `QApplication` construction. Prefer pass-through fractional scaling so 1.25 and 1.5 are not silently rounded to another scale. Do not rely on the desktop environment default.

### 17.4 Pixel alignment

For custom painting:

- obtain the target device DPR;
- hairlines are 1 device pixel (`1 / DPR` logical);
- snap rectangle/stroke edges to device-pixel centers where needed;
- avoid half-device-pixel boundaries unless antialiasing is canonical;
- raster caches/pixmaps must carry correct DPR;
- generate icons at device resolution or use vectors; do not scale a 22×22 1x QPixmap at 150%.

### 17.5 Acceptance at fractional scales

Geometry tolerance remains 0–1 **logical** px. A one-device-pixel antialiasing difference is acceptable only if logical bounds match and the difference is attributable to rasterizer/font rendering rather than incorrect geometry.

---

## 18. Current `GNU_Linux` discrepancy ledger

Severity: **P0** = fundamental structure/behavior blocks parity; **P1** = major visible/interaction mismatch; **P2** = localized visual/metric mismatch; **P3** = platform-only/verification cleanup.

| Severity | Area | macOS behavior | Current Linux behavior | Required Linux behavior | macOS source | Linux source |
|---|---|---|---|---|---|---|
| P0 | Startup document | Editor can be documentless and shows New Canvas/welcome flow | Constructor creates 64×64 document and red demo stroke | Remove demo document/stroke; reproduce documentless startup flow | `ContentView.swift`, `NewCanvasSheet.swift` | `SessionWindow.cpp` ctor |
| P0 | Default tool | Move selected | Brush selected/default | Default Move | `EditorSession.swift` | `SessionWindow.h/.cpp` |
| P0 | Tool model | One Marquee rail tool; rectangle/ellipse modes | Separate Rect/Ellipse rail actions | Merge to one rail action and mode state | `EditorSession.swift`, `LassoControls.swift` | `SessionWindow.cpp` |
| P0 | Tool model | Eraser is Brush mode | Separate Eraser rail action | One Brush rail item; E selects Brush+Erase | same | same |
| P0 | Gradient | Full options + canvas edit | Idle options; mouse switch no operation | Implement canonical tool | `GradientControls.swift`, `EditorCanvas.swift` | `SessionWindow.cpp` |
| P0 | Shape | Rectangle/Ellipse/Line, editable options | Idle options; mouse switch no operation | Implement canonical tool | `ShapeControls.swift` | `SessionWindow.cpp` |
| P0 | Type | Live text placement/edit | Idle options; mouse switch no operation | Implement canonical Type flow | `TypeControls.swift` | `SessionWindow.cpp` |
| P0 | Hand | Drag pans viewport; Space temporary hand | Mouse-down sets painting; move does not pan | Implement panning and temporary-hand restoration | `NavigationToolHeader.swift`, `EditorCanvas.swift` | `SessionWindow.cpp` |
| P0 | Crop | Non-destructive pending crop until Apply | Crops immediately on mouse release | Pending crop edit with Apply/Cancel | `CropControls.swift`, `EditorCanvas.swift` | `SessionWindow.cpp` |
| P0 | Wand controls | UI values/modes drive command | command hard-codes tolerance 32, contiguous 1, sampleAll 0 | Wire state exactly; add Object/Sample Size | `LassoControls.swift` | `SessionWindow.cpp` MagicWand case |
| P0 | Clone controls | Aligned/Sample + brush settings drive stroke | command hard-codes aligned 1/sampleAll 0; options incomplete | Wire all canonical controls | `BrushControls.swift` | `SessionWindow.cpp` |
| P0 | Spot Healing | Healing type + shared brush controls | options incomplete; mode hard-coded | Implement/wire canonical modes | `BrushControls.swift` | `SessionWindow.cpp` |
| P0 | Layers masks/effects | Separate image/mask targets, links, effect rows | mask represented in subtitle; no equivalent effect rows | Rebuild row composition/selection | `NativeLayerList.swift` | `LayerItemDelegate.h` |
| P0 | Layer refresh | Preserve folder expansion and multi-selection | refresh uses active row and expands hierarchy | Preserve exact collapsed IDs + selected IDs | `NativeLayerList.swift` | `SessionWindow.cpp::refreshLayers` |
| P0 | Project tabs | real workspace switching/drop/new-tab semantics | cosmetic single QTabBar `Untitled 1` | Implement workspace tab semantics | `ProjectTabs.swift` | `SessionWindow.cpp::setupHeaderBar` |
| P0 | New Canvas | dedicated 1920×1080/clipboard-aware sheet | File New uses resize `SizeDialog` | Dedicated canonical New Canvas dialog | `NewCanvasSheet.swift` | `SessionWindow.cpp`, `SizeDialog.cpp` |
| P1 | Window size | 1180×780 | 1200×800 | 1180×780 | `CompositorApp.swift` | `SessionWindow.cpp` |
| P1 | Tool rail width | 56 | QToolBar styling width 44-ish | fixed custom 56 | `ContentView.swift` | `SessionWindow.cpp::applyDarkTheme` |
| P1 | Tool spacing | 10, no separators | 2-ish + explicit separators | 10; remove separators | `ContentView.swift` | `SessionWindow.cpp` |
| P1 | Tool icon bounds | 17/18 | generated 22×22 | 17/18 visual bounds | `ContentView.swift` | `makeToolIcon` |
| P1 | Palette | 24 swatches/36 frame; black/white default | 22 swatches/36×42; red/white default | canonical geometry/state | `ColorPaletteControls.swift` | `SessionWindow.cpp` |
| P1 | Transform header | canonical sequence incl Scale/Sampling/Flip/Apply/Cancel | extra Ignore Transparent, missing multiple controls | Replace composition | `TransformInspector.swift` | `setupOptionsBar` |
| P1 | Selection header | mode-dependent canonical controls | generic selection page; missing Feather/Object/Sample Size | Rebuild canonical header | `LassoControls.swift` | `setupOptionsBar` |
| P1 | Brush header | spacing/fields/smoothing and family state | different widths/ranges, no smoothing | canonical metrics/state | `BrushControls.swift` | `setupOptionsBar` |
| P1 | Layers panel resize | 202…352 | 252…352 | 202…352 | `LayersPanel.swift` | `SessionWindow.cpp` |
| P1 | Layers shell | fixed panel, no dock chrome | QDockWidget | fixed parity panel | `LayersPanel.swift` | `SessionWindow.cpp` |
| P1 | Layers footer | six canonical actions incl effects | different button geometry; effects missing | exact order/geometry | `LayersPanel.swift` | `SessionWindow.cpp` |
| P1 | Layer context menu | rich target-dependent row menu | no equivalent canonical row menu | implement exact menu | `NativeLayerList.swift` | layers Qt view |
| P1 | Visibility swipe | drag across eyes toggles same state in one undo | simple per-eye toggle | implement swipe tracking | `NativeLayerList.swift` | `LayerItemDelegate.h` |
| P1 | Status geometry | 30 high, 18 outer pad, 16 gaps | min 24, 8 label pads, separators | canonical metrics | `ContentView.swift` | `applyDarkTheme`, `updateStatusTelemetry` |
| P1 | Rulers | 18-point interactive rulers | menu actions without equivalent canvas ruler | implement ruler widgets/guide drag | `CanvasRulers.swift` | `SessionWindow.cpp` |
| P1 | Edit menu | no Command Palette | adds Command Palette | hide/remove in parity UI | `CompositorApp.swift` | `createMenus` |
| P1 | File menu | PNG/JPEG only | adds TIFF/WebP + File Quit | hide extras; category-C Quit handling | `CompositorApp.swift` | `createMenus` |
| P1 | Select menu | no Rect/Ellipse automation items | exposes them visibly | move to hidden automation API | `CompositorApp.swift` | `createMenus` |
| P1 | Layer menu | no Layer Mask submenu | adds Layer Mask submenu | remove from top-level parity menu | `CompositorApp.swift` | `createMenus` |
| P1 | Window menu | system/native behavior | custom workspace/panel toggles | remove unless mac equivalent exists | `CompositorApp.swift` | `createMenus` |
| P1 | Keyboard Shortcuts | editable floating configuration | informational QMessageBox | implement editor/validation | `KeyboardShortcuts.swift` | `createMenus` |
| P1 | Selection dialogs | custom 380-wide tool panel | QInputDialog integer dialogs | canonical panel | `LassoControls.swift` | `createMenus` |
| P1 | Canvas Size | 3×3 anchor grid; FG/BG extension options | anchor combo; reduced extension choices | exact canonical UI | `CanvasSizeSheet.swift` | `SizeDialog.cpp` |
| P1 | Floating dialogs | non-modal, position-preserving panels | several blocking `exec()` QDialogs | match modality/placement | `FloatingPanel.swift` | `SessionWindow.cpp`, dialog files |
| P1 | Blend popup | grouped + live hover preview | normal Qt combo | grouped previewing popup | `BlendModePicker.swift` | `SessionWindow.cpp` |
| P1 | Status/tool hints | canonical context text/state | independently authored hints | source same state/hint semantics | `ContentView.swift` | `updateStatusTelemetry` |
| P2 | Tool option padding | 18 | options toolbar uses 8 | 18 | tool control files | `setupOptionsBar` |
| P2 | Tool title font | 13 semibold | 12 px semibold | match metrics | `ToolHeaderStyle.swift` | `setupOptionsBar` |
| P2 | Header spacing | 12/14 canonical | mostly 8 | component-specific values | control files | `setupOptionsBar` |
| P2 | Brush color swatch | 34×18 | 36×20 | 34×18 where BrushControls applies | `BrushControls.swift` | `setupOptionsBar` |
| P2 | Canvas surround | white 0.14 | `#242528` | canonical token | `ContentView.swift` | `canvasPaintEvent` |
| P2 | Layer row divider | white 6%, one device px | opaque `#2a2b2e` logical line | canonical hairline | `NativeLayerList.swift` | `LayerItemDelegate.h` |
| P2 | Layer row hover | native/canonical list behavior | custom `#2c2d30` | screenshot-match | `NativeLayerList.swift` | `LayerItemDelegate.h` |
| P2 | Font family | macOS system/SF | QSS includes unavailable `-apple-system` then Segoe/Roboto | use explicit licensed fallback + calibration | multiple | `applyDarkTheme` |
| P2 | Header tabs | capsule 28 high in 34 strip | QTabBar radius 5/padding | custom canonical tab visuals | `ProjectTabs.swift` | `setupHeaderBar` |
| P3 | App menu | macOS app menu | no true equivalent | platform mapping only | `CompositorApp.swift` | Qt shell |
| P3 | Window chrome | macOS unified titlebar/traffic lights | Linux WM decorations | closest desktop equivalent | `CompositorApp.swift` | Qt/WM |
| P3 | Native file dialogs | AppKit panels | portal/Qt dialogs | preserve semantics, accept platform chrome | app/platform | `QtPlatformServices.h` |
| P3 | Font rasterization | CoreText | FreeType/Qt | metric/layout parity; raster diff tolerance | system | Qt |

This ledger is intentionally not exhaustive of every pixel until screenshot capture has been run. Any newly detected mismatch must be added before it is fixed so regressions remain auditable.

---

## 19. Screenshot verification methodology

### 19.1 Reference environment

Record with every capture:

- macOS version/build;
- Compositor `main` commit;
- display scale/DPR;
- color profile;
- window logical size;
- exact document fixture and session state;
- selected tool/mode/layer/mask/effect;
- pointer location when hover/cursor matters.

Linux captures must use the same logical window size and equivalent scale.

### 19.2 Capture matrix

At minimum capture:

1. documentless/new-canvas state;
2. normal one-document editor, Move selected;
3. every rail tool selected;
4. every tool mode that changes its option bar;
5. active transform; crop; gradient; text edit; polygonal lasso;
6. selection present/absent and add/subtract modifier states;
7. layers: raster, text, adjustment, folder, mask selected, disabled mask, clipping, effects, multi-selection, rename;
8. Layers panel min/default/max widths;
9. rulers/guides/grid/snap states;
10. each menu open, including every dynamic title state;
11. context menus;
12. color picker and each custom dialog/panel;
13. active and inactive window;
14. disabled/busy/importing states;
15. 100/125/150/200% scale.

### 19.3 Image comparison procedure

For each state:

1. Capture macOS reference.
2. Capture Linux at the identical logical window size.
3. Crop/normalize only unavoidable OS-owned outer window chrome; never crop Compositor-owned UI.
4. Align using stable content landmarks; do not scale one screenshot independently to “make it fit.”
5. Produce a 50% alpha overlay.
6. Produce absolute RGB difference image.
7. Produce an edge/geometry difference image.
8. Record each mismatch by component and logical bounding box.
9. Correct Linux.
10. Repeat until tolerances pass.

The existing Linux `COMPOSITOR_GRAB_PATH` / `COMPOSITOR_GRAB_DIALOG` hooks should be extended into deterministic named-state capture rather than replaced with manual-only screenshots.

### 19.4 Suggested automated diff metrics

Use two masks:

- **geometry mask:** edges/control bounds; strict 0–1 logical px tolerance;
- **raster mask:** text/icon pixels; allows renderer differences but rejects systematic offset/scale/color errors.

Record at least:

- max edge displacement;
- mean/95th percentile color delta outside text antialias fringes;
- number of pixels outside tolerance;
- bounding boxes of connected diff regions.

Do not use one global perceptual score as proof of parity; a missing 20×20 control can be hidden by a large mostly-identical canvas.

---

## 20. Acceptance tolerances

| Attribute | Acceptance |
|---|---|
| Major layout geometry | 0–1 logical px |
| Control dimensions | 0–1 logical px |
| Margins/padding/gaps | 0–1 logical px |
| Divider/hairline position | same logical edge; one device px thickness |
| Icon visual bounds | 0–1 logical px |
| Corner radius | exact source value where explicit; otherwise screenshot-equivalent |
| Text baseline | ≤1 logical px after fallback-font calibration |
| Typography | closest licensed metric rendering; no clipping/wrapping difference |
| Explicit source colors | exact sRGB/alpha barring measured color-management transform |
| Semantic colors | match resolved canonical screenshot within color-management tolerance |
| Dynamic menu text/order | exact wording/order/separators |
| Enabled/disabled states | exact for every tested session state |
| Keyboard command result | behaviorally identical using Linux modifier translation |
| Drag/drop result | identical document and selection state |
| Undo grouping/title | identical semantic edit grouping/title |
| Tool/canvas cursor | same semantic cursor and hotspot within 1 logical px where custom |

### 20.1 What does not qualify as 100% parity

- same menu names but different ordering or extra visible commands;
- controls present but not wired;
- same tool names with different rail grouping;
- screenshots taken at different logical sizes;
- one screenshot with no interaction-state coverage;
- Qt-native styling described as “close enough” without overlay evidence;
- passing engine tests while UI state/geometry differs;
- a commit message asserting parity without reference captures and diffs.

---

## 21. Implementation order

To minimize rework:

1. **P0 state model first:** tool model, documentless startup, tabs/workspace state, pending operations, layer target/effect/multi-selection state.
2. **Main shell geometry:** custom 56 rail, 42 header, fixed right panel, 30 status, 1180×780 baseline.
3. **Tools/options:** one-to-one canonical pages and command wiring.
4. **Layers:** row hierarchy, masks/effects, drag/drop, context behavior.
5. **Menus/shortcuts:** remove extras, responder-aware routing, dynamic titles.
6. **Dialogs/panels:** exact custom layouts and modality.
7. **Canvas chrome:** rulers/guides/grid/cursors/overlays.
8. **Typography/colors/icons.**
9. **HiDPI.**
10. **Screenshot-diff closure.**

Do not spend time tuning QSS colors around a shell whose geometry/state model is still wrong.

---

## 22. Parity checklist

### Source and scope

- [ ] macOS reference commit is recorded and unchanged for the test run.
- [ ] Linux target commit is recorded.
- [ ] Every Linux-visible component has a macOS source mapping or Category C note.
- [ ] No branch commit is labeled 100% parity before the acceptance suite passes.

### Window and structure

- [ ] Initial normal size 1180×780.
- [ ] Minimum content 800×520.
- [ ] Tool rail exactly 56 wide.
- [ ] Tool header exactly 42 high.
- [ ] Status exactly 30 high.
- [ ] Layers default 252, resize 202…352.
- [ ] No noncanonical dock chrome.
- [ ] Documentless startup matches macOS.

### Tools

- [ ] Exact 15 visible rail tools/order.
- [ ] Move is default.
- [ ] Marquee is one rail item with modes.
- [ ] Eraser is Brush mode, not rail item.
- [ ] No separator bars in rail.
- [ ] Every tool options bar matches control order, dimensions and state.
- [ ] Gradient/Shape/Type/Hand are fully functional.
- [ ] Crop is preview/apply/cancel.
- [ ] Wand/Clone/Healing visible controls actually drive command parameters.
- [ ] Temporary Space-hand works and restores previous tool.

### Colors and palette

- [ ] FG black / BG white default.
- [ ] Swatches 24×24 with canonical offsets/borders/radius.
- [ ] X and D behavior matches.
- [ ] Mask color popover behavior matches.

### Layers

- [ ] Header/appearance/footer geometry matches.
- [ ] Layer rows 52; effect rows +24.
- [ ] Mask thumbnail/link controls match.
- [ ] Multi-selection persists across refresh.
- [ ] Folder collapsed state persists.
- [ ] Context menu matches.
- [ ] Eye swipe works as one undo edit.
- [ ] Layer/mask/effect copy drag behavior matches.
- [ ] Footer includes Effects and Adjustment menus in canonical order.

### Menus and shortcuts

- [ ] Top-level order exact.
- [ ] File extras hidden from parity UI.
- [ ] Command Palette hidden.
- [ ] automation-only selection entries hidden.
- [ ] extra Layer Mask submenu removed.
- [ ] dynamic titles exact.
- [ ] text-field responder behavior prevents canvas shortcut leakage.
- [ ] editable Keyboard Shortcuts panel matches macOS semantics.

### Canvas

- [ ] Canvas surround color matches.
- [ ] rulers are 18 and interactive.
- [ ] guides/grid/snap toggles actually change behavior.
- [ ] Fit/100% match reference document rect.
- [ ] pan/zoom/wheel/trackpad semantics match.
- [ ] overlays/cursors are verified.

### Dialogs and panels

- [ ] New Canvas exact.
- [ ] Canvas Size exact including 3×3 anchor.
- [ ] Image Size exact.
- [ ] selection amount panels exact.
- [ ] Color Picker exact 256 field/geometry.
- [ ] floating panels are non-modal where macOS panels are non-modal.
- [ ] Return/Escape/close-button behavior matches.

### HiDPI

- [ ] 100% screenshots pass.
- [ ] 125% screenshots pass.
- [ ] 150% screenshots pass.
- [ ] 200% screenshots pass.
- [ ] no blurred cached icons at fractional DPR.
- [ ] hairlines remain one device pixel.

### Verification

- [ ] Every major state has paired reference screenshots.
- [ ] 50% overlays generated.
- [ ] absolute diff images generated.
- [ ] geometry differences recorded and closed.
- [ ] interaction test matrix passed.
- [ ] only documented Category C differences remain.

---

## 23. Definition of done

The GNU/Linux version may be called **macOS UI/UX parity complete** only when:

1. no unresolved P0/P1 discrepancy remains;
2. every visible Linux control maps to the canonical macOS UI or a documented Category C platform equivalent;
3. menu order, titles, separators, shortcuts and enablement pass state-by-state comparison;
4. tool selection/modes/options/cursors and canvas interactions pass behavioral tests;
5. layers, dialogs, popovers and context menus pass interaction tests;
6. screenshot overlays at 100%, 125%, 150% and 200% satisfy the tolerances above;
7. semantic-color/font raster differences are documented with evidence and do not cause geometry, wrapping or hierarchy differences;
8. a reviewer can reproduce the capture/diff suite from a clean checkout.

Until then, describe progress by the verified scope (for example, “menu command parity verified” or “tool rail geometry within 1 px”), not as global 100% parity.
