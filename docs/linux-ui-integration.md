# Linux UI integration follow-up

This work connects visible Qt input to the existing shared editor. It does not
change the protected Mac source trees or establish full Mac runtime parity.

## Implemented paths

| Area | Linux behavior | Regression evidence |
|---|---|---|
| Keyboard shortcuts | Edit → Keyboard Shortcuts records a single key combination, rejects conflicts, saves or cancels drafts, restores defaults, and updates menu accelerators and canvas responders. Ctrl is the command modifier; Alt and Super have Linux labels. | Actual key capture; conflicting assignment rejected; displaced key suppressed; reset/cancel; a separate process reads the saved assignment. |
| Layer dragging | Native tree drop indicators; reorder above/below siblings; drop into folders or out to root; multi-layer moves; Ctrl-drag copies within the document; folders carry descendants. Shared hierarchy validation rejects cycles and excessive depth before editing. | Native drag-enter/move/drop events; nesting, cycle rejection and undo; model tests for multi-layer transactions, descendant handling and copying. |
| Color pickers | Shared brush/shape/text buttons present the Linux RGB/HSV/hex picker. Commit uses the shared target, Cancel discards it, swatches show the resulting RGB, and foreground/background swaps update the shared palette. | Visible shared-button → modal picker → cancel/commit; hex input; displayed swatch color; foreground/background target tests. |
| Context menus and input | Layer context menus reuse the enabled application actions. Generic SwiftUI context menus, tap/double-tap and text-submit handlers reach Qt events. Wheel panning and native pinch events reach the viewport. | Visible layer popup; native renderer tests for menu actions, taps, submit and callback lifetime. Physical trackpad pinch remains a hardware check. |
| Drawing tool settings | Shape kind/radius/line width and modifier drags use shared shape operations. Gradients preview and apply/cancel with shared settings. Smear uses the selected Blur/Smudge/Liquify path. Wand settings and Object mode reach selection input. Crop ratios update the shared crop frame. Type opens a text editor using shared typography defaults. | Shape pointer creation and undo; gradient exported pixels, cancel and undo; crop ratio picker; editable text creation; model checks for line width and implicit String picker tags. |

The shortcut dialog exposes commands with implemented Qt responder/menu paths.
It does not advertise every Mac-only responder command as configurable. Application
and desktop-reserved keys remain constrained by the editor and window manager.

The main layer list uses native Qt drag-and-drop. Cross-window/document drops and
Mac mask/effect-row copying are not implemented by this layer MIME route. Generic
SwiftUI `gesture`, `simultaneousGesture`, `onDrop` and popover compatibility remain
incomplete; the concrete workflows above use explicit Linux integrations.

Type uses a Linux text dialog, rather than the Mac inline editing surface. Matching
paragraph manipulation and glyph rendering still requires separate Mac comparison.
The color picker commits its chosen target on OK; this integration does not add
live effect/gradient-map preview while dragging inside the modal picker.

## Validation

`UIIntegrationTests` covers shared command validation and undo. The expanded
`--ui-smoke` runs the visible journeys, including a separate process for shortcut
persistence. `CompositorCore.SwiftUIRenderer` checks generic UI handlers and menu
lifetime. Run `scripts/linux-release-check.py` in the documented SDK for the
optimized suite and UI journeys at 1×, 1.5× and 2×. See
[release readiness](linux-release-readiness.md) for measured results and excluded
Mac-only tests.

The installed application is separate from this contribution candidate. A fresh
Flatpak package build and installation/rollback check remain release gates.
