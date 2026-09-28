# Design Review: Linux Compositor (Qt Widgets & Desktop Experience)

**Review Mode:** COMPATIBILITY & DESKTOP FIDELITY  
**Oracle:** macOS Compositor 1.0.4 (`a19db90`)  
**Target:** GNU/Linux desktop environments (GNOME Wayland, KDE Plasma Wayland, X11/XWayland)

---

## 1. Executive Summary & Design Principles

Compositor on Linux must feel like a first-class, responsive, professional desktop application, not an alien macOS port. It preserves the exact semantic behaviors, geometric accuracy, and interaction mental models of macOS Compositor 1.0.4 while adhering to native Linux desktop conventions (XDG, FreeDesktop, Qt 6 Widgets).

### Core Design Principles
1. **Respect Desktop Environment Aesthetics:** Automatically adapt to system light/dark themes (Adwaita, Breeze) through Qt palette integration. Avoid hardcoded macOS-specific colors or foreign title bar hacks.
2. **Visual Parity Where It Matters:** Canvas overlays (brush size ring, transform bounding box, rotation pivot, selection marching ants, crop guides) must match macOS visual weights and anti-aliasing sharpness exactly.
3. **Fluid Direct Manipulation:** Pan, zoom, brush strokes, and layer dragging must have zero noticeable latency, sub-pixel accuracy, and rock-solid state machine isolation (no stuck mouse grabs).
4. **Predictable Feedback & State Visibility:** Always indicate dirty document state (`*` in tab and title), show current zoom level and dimensions in the status bar, and provide clear visual indication of whether the renderer is running on Vulkan GPU or CPU Raster.

---

## 2. Desktop Environment Integration & Layout

### 2.1 Main Window Geometry & Hierarchy
```text
┌────────────────────────────────────────────────────────────────────────┐
│ Main Menu: File   Edit   Image   Layer   Select   Filter   View   Help │
├────────────────────────────────────────────────────────────────────────┤
│ Tool Options Toolbar: [Size: 24px] [Opacity: 100%] [Hardness: 80%] ... │
├──────┬─────────────────────────────────────────────────┬───────────────┤
│ Tool │ Document Tabs: [Untitled-1.comp *] [photo.png]  │ Docks:        │
│ Bar  ├─────────────────────────────────────────────────┤ ┌───────────┐ │
│      │                                                 │ │ Layers    │ │
│ [V]  │                                                 │ │ Opacity:  │ │
│ [M]  │                                                 │ │ [====80%=]│ │
│ [L]  │                   CANVAS                        │ │ Blend:    │ │
│ [W]  │              (Active Document)                  │ │ [Normal ▾]│ │
│ [C]  │                                                 │ │ ───────── │ │
│ [B]  │                                                 │ │ [👁] Layer2│ │
│ [E]  │                                                 │ │ [👁] Layer1│ │
│ [S]  │                                                 │ └───────────┘ │
│ [G]  │                                                 │ ┌───────────┐ │
│ [T]  │                                                 │ │ History   │ │
│ [H]  │                                                 │ └───────────┘ │
│ [Z]  │                                                 │ ┌───────────┐ │
│ ──── │                                                 │ │ Channels  │ │
│[🎨]  │                                                 │ └───────────┘ │
├──────┴─────────────────────────────────────────────────┴───────────────┤
│ Status Bar: 3840×2160 px | 100% | X: 1240, Y: 890 | [GPU: Vulkan]      │
└────────────────────────────────────────────────────────────────────────┘
```

- **Main Menu Bar:** Standard `QMenuBar` with desktop standard shortcuts (`Ctrl` instead of `Cmd`, `Alt` instead of `Opt`).
- **Tool Options Bar:** Context-sensitive `QToolBar` docked beneath the menu. Updates controls dynamically when the active tool changes.
- **Tools Palette:** Vertical `QToolBar` (dockable left or right) with tool grouping, keyboard shortcuts on tooltips, and active tool highlight.
- **Document Tab Bar:** `QTabBar` supporting multi-document workflows, close buttons per tab, dirty indicator (`*`), and middle-click to close.
- **Dockable Panels (`QDockWidget`):**
  - *Layers Dock:* Central workflow hub. Includes blend mode selector, opacity slider, group/lock/mask buttons, and the layer hierarchy tree.
  - *History / Undo Dock:* Linear undo stack showing state snapshots with instant step-back navigation.
  - *Color / Swatches Dock:* Foreground/background color selection with numeric inputs (RGB, HSV, Hex).
- **Status Bar:** Displays canvas dimensions, active cursor pixel coordinates, color under cursor, current zoom percentage, and renderer status badge (`[GPU: Vulkan]` or `[CPU: Raster]`).

---

## 3. Canvas & Direct Manipulation

### 3.1 Viewport Navigation
- **Panning:**
  - `Middle Mouse Button + Drag` (standard Linux DCC convention).
  - `Spacebar + Left Mouse Button + Drag` (standard graphic design convention).
  - Two-finger drag on precision touchpad (Wayland gesture events).
  - Continuous scrollbars around canvas with proportional thumbs.
- **Zooming:**
  - `Ctrl + Mouse Wheel` (zooms centered at current cursor position).
  - Pinch-to-zoom on touchpad (native Wayland gesture).
  - `Ctrl + Plus` / `Ctrl + Minus` (discrete zoom steps: 12.5%, 25%, 50%, 100%, 200%, 400%, 800%, 1600%, 3200%).
  - `Ctrl + 0`: Fit to window (maintains aspect ratio, centered with margin).
  - `Ctrl + 1`: 100% actual pixels.

### 3.2 Overlays & Visual Feedback
- **Brush Cursor:**
  - High-precision circular ring showing exact brush diameter at current viewport zoom.
  - Inner center point crosshair for sub-pixel precision.
  - Inverted or dual-tone contour (white outline with black drop shadow) to ensure visibility over any canvas background (black, white, or checkered transparent).
- **Transform Box (`Ctrl + T`):**
  - 8 resize handles (4 corners, 4 mid-edges) drawn at constant 8×8 display pixels regardless of canvas zoom.
  - Rotation handle extending from top center with stem.
  - Interactive cursor change when hovering over handles (`Qt::SizeAllCursor`, `Qt::SizeFDiagCursor`, `Qt::SizeBDiagCursor`, rotate cursor).
  - Shift key constrains aspect ratio; Alt key scales from center.
- **Selection Marquee:**
  - Marching ants pattern animated at 30 fps via low-overhead dashed stroke shader or line stipple.
  - High-contrast 1px black-and-white dash pattern.

---

## 4. Layer Tree & Hierarchy (`QTreeView` Adapter)

The macOS `NativeLayerList` (`NSTableView`) is replaced by a high-performance `QTreeView` backed by a custom `QAbstractItemModel` that views into the authoritative Swift `EditorSession`.

### 4.1 Visual Components per Layer Row
```text
┌────┬────┬──────┬──────────────────────────┬──────┐
│ 👁 │ 🔒 │ [🔲] │ Layer Name               │ [M]  │
└────┴────┴──────┴──────────────────────────┴──────┘
  │    │    │      │                          │
  │    │    │      │                          └─ Raster Mask Thumbnail (if attached)
  │    │    │      └─ Editable Label (Double click to rename)
  │    │    └─ Layer Content Thumbnail (32×32 px with alpha checkerboard)
  │    └─ Lock status toggle
  └─ Visibility eye toggle (click or drag over multiple rows to batch toggle)
```

### 4.2 Interaction Rules
- **Reordering & Grouping:**
  - Drag-and-drop between layers: shows horizontal insertion bar indicator.
  - Drag into a folder: folder row highlights, layer becomes child in group.
  - Drop validation: prevents cyclic nesting or dragging parent into its own child.
- **Mask Interaction:**
  - Clicking layer content thumbnail activates pixel editing mode.
  - Clicking mask thumbnail activates mask editing mode (indicated by highlight border around the thumbnail).
  - Right-click context menu: "Add Mask", "Delete Mask", "Disable Mask", "Apply Mask".
- **Multi-Selection:**
  - `Ctrl + Click` toggles individual layers.
  - `Shift + Click` selects contiguous range.
  - Operations like move, delete, group, or merge apply to all selected layers.

---

## 5. Keyboard & Mouse Parity Matrix

| Action | macOS Shortcut | Linux Qt Shortcut | Scope & Behavior |
|---|---|---|---|
| New Document | `Cmd + N` | `Ctrl + N` | Opens New Canvas dialog |
| Open Document | `Cmd + O` | `Ctrl + O` | System FileChooser portal |
| Save Document | `Cmd + S` | `Ctrl + S` | Atomic `.comp` directory save |
| Save As | `Cmd + Shift + S` | `Ctrl + Shift + S` | Choose new location/name |
| Export Image | `Cmd + E` | `Ctrl + E` | Export dialog (PNG, JPEG, WebP) |
| Undo | `Cmd + Z` | `Ctrl + Z` | Reverts one history step |
| Redo | `Cmd + Shift + Z` | `Ctrl + Shift + Z` / `Ctrl + Y` | Reapplies reverted step |
| Cut Selection | `Cmd + X` | `Ctrl + X` | Copies to clipboard & deletes |
| Copy Selection | `Cmd + C` | `Ctrl + C` | Copies pixels to system clipboard |
| Paste Layer | `Cmd + V` | `Ctrl + V` | Creates new layer from clipboard |
| Free Transform | `Cmd + T` | `Ctrl + T` | Activates transform handles |
| Commit Transform | `Return` | `Return` / `Enter` | Commits pending transform |
| Cancel Transform | `Escape` | `Escape` | Reverts draft transform |
| Select All | `Cmd + A` | `Ctrl + A` | Selects entire canvas bounds |
| Deselect | `Cmd + D` | `Ctrl + D` | Clears active selection |
| Invert Selection | `Cmd + Shift + I` | `Ctrl + Shift + I` | Inverts mask/selection |
| Decrease Brush Size | `[` | `[` | Shrinks diameter by step |
| Increase Brush Size | `]` | `]` | Enlarges diameter by step |
| Soften Brush | `Shift + [` | `Shift + [` | Decreases brush hardness |
| Harden Brush | `Shift + ]` | `Shift + ]` | Increases brush hardness |
| Reset Colors | `D` | `D` | Foreground: Black, Background: White |
| Swap Colors | `X` | `X` | Swaps Foreground and Background |

---

## 6. Dialogs, Adjustment Sheets & Inspectors

1. **Non-destructive Adjustments (Levels, Curves, Hue/Saturation):**
   - Implemented as floating `QDialog` inspector tools with live canvas preview.
   - Histogram widget in Levels/Curves displays real-time 256-bin luminance/RGB distribution.
   - Cancel button immediately restores unmodified canvas state without recording an undo step.
   - OK button records exactly one atomic undo step.
2. **Resize Dialogs (Image Size & Canvas Size):**
   - Preserves 3×3 anchor positioning widget for Canvas Size expansion/crop.
   - Width and height spinboxes with link/unlink aspect ratio toggle.
   - Interpolation algorithm selector: Nearest Neighbor, Bilinear, Bicubic (Smooth).
3. **Filter Windows (Gaussian Blur, Noise, Lens Distortion, Content Fill):**
   - Radius/amount sliders with interactive numeric inputs.
   - Progress bar for expensive operations with background cancellation support.

---

## 7. Accessibility, HiDPI & Wayland Readiness

- **Fractional DPI Scaling:** Full support for `QT_SCALE_FACTOR` and Wayland wp-fractional-scale-v1 protocol. Handles render at exact device pixels without blurry upscaling.
- **Dark Mode Auto-Detection:** Queries `org.freedesktop.appearance.color-scheme` through XDG Desktop Portal; switches Qt palette dynamically between light and dark variants without restart.
- **Tablet & Stylus Support:** Prepares `QTabletEvent` mapping for pressure-sensitive drawing tablets (Wacom, Huion, XP-Pen) on Wayland and X11.

---

## 8. Design Approval & Criteria for Done

- [x] UI uses clean Qt Widgets without foreign macOS chrome.
- [x] Standard Linux keyboard shortcuts mapped 1:1 from macOS intentions.
- [x] Layer dock provides full tree, visibility, mask, and reordering feedback.
- [x] Visual overlays (transform, brush, selection) render crisply at all scale factors.
- [x] Real-time viewport navigation (pan/zoom) matches macOS smoothness.
