# Right-hand panels

Omuse 0.4.0 uses a shared inspector layout across the editor,
Create and Ask Omuse. Colours, fonts, active states and focus styling follow
the active Omarchy theme through `gpui-omarchy`.

- **Layers, Develop, Select and Canvas:** equal navigation tabs, grouped
  controls, clearer primary actions and consistent spacing. The Layers panel
  groups its layer stack and compositing controls. Mask and clipping controls
  show their current state; unavailable actions have a disabled appearance.
- **Create:** six tabs arranged in two equal rows. Page, brand and layout
  fields have explicit labels; related controls and export actions are grouped.
- **Ask Omuse:** tasks use a two-column grid. The prompt, provider selection
  and send action stay together. Long follow-on steps show the task and provider
  on separate lines. Short windows use a compact prompt area to leave more space
  for tasks and results.
- **Connections:** a dedicated full-height view, with compact capability
  indicators. Back restores the brief and result. Opening Connections does not
  send a request or discard work. Stop request stays visible while preparation
  or a request is active.

Panels share a 360-pixel width in roomy windows and use 320 pixels below a
1000-pixel viewport. Panel content scrolls without compressing its controls;
the keyboard command search remains available for direct access to actions.

Regression coverage exercises navigation and scrolling at 800 × 600, preserves
the brief and reviewed result across Connections, activates review controls,
and cancels pending preparation from the Connections footer. It also checks
contextual action availability and theme changes without modifying artwork.

The [0.4.0 qualification](release-040-qualification.md) records the combined
runtime checks. A shared style and bounded local checks do not establish
accessibility coverage for every theme, display scale or input method.

## Colour-field polish — 0.7.0

The qualified 0.7.0 source release is
`5b3daefbb5258afff4a74a2ff3db5247b074972d`. It includes the colour swatches
from `466f7ab` and the shared control geometry from `e525124`; measurements
below retain their original source identities.

Source `466f7ab` on `feature/photo-vector-studio` adds live colour swatches
beside vector Fill/Stroke, Brand Paper/Ink/Accent, finishing Dark/Light/Overlay
colours and the Target colour uniformity reference. At that checkpoint it
remained outside the installed/public 0.6.0 release.

Swatches use the existing hex parser for each tool. A neutral checkerboard
shows alpha, a diagonal mark identifies no paint, and a question mark indicates
incomplete or invalid input. They are prefixes inside the native text field:
clicking focuses the hex editor and does not create an extra keyboard stop or
open a colour picker. Brand swatches repaint on typing without applying the kit.

Vector controls now use the shared 32-pixel inspector input height, two flexible
columns, consistent label spacing and inline invalid-hex guidance. Width 0 is
labelled **Stroke · none** with a crossed swatch and a short explanation. Open/Close
and Reverse are disabled until the active contour has a segment. Imported SVG
guidance names **Done** when editing on the main canvas.

Compiler, formatting and generated keyboard-reference checks passed. All **351
existing UI regressions passed**, with zero failures and two ignored benchmarks.
These include visible hex-field typing, object switching, Undo/Redo, save/reopen,
finishing and Create workflows. Native **Wayland/dark** and **XWayland/light**
journeys passed **27 checks each** at an 800×600 logical viewport. Screenshots
were inspected for complete hex values, aligned fields, swatch visibility and
the disabled-stroke presentation. The native runs used isolated test profiles.

Tested executable SHA-256:
`62ff5b17ed92972ac1edfd32aaae57d4eb250fbe380a625b322311f72ed13af3`.
At the `466f7ab` checkpoint, the installed 0.6.0 executable remained
unchanged. The earlier full 1,206-case application qualification belongs to
`3fbca66`; this presentation follow-up reran the UI suite and native journeys,
not that complete release script. The final 0.7.0 release evidence is recorded
in [release-070-qualification.md](release-070-qualification.md), the
[receipt](release-070-receipts.json), and the [ten-image gallery](releases/v0.7.0-gallery.md).

Local evidence: `rust/evidence/colour-polish-source.txt`,
`colour-polish-ui-tests.log`, `colour-polish-check.log`, and
`colour-polish-native-{dark,light}-host/verified-native.json`, all under
`rust/evidence/`. The first sandbox launch could not access the compositor;
the recorded passing runs used the desktop context.

See the [dark fill/stroke screenshot](user-guide/images/vector-colours-dark.png)
and [light no-stroke screenshot](user-guide/images/vector-colours-light.png).

## Shared control geometry — 0.7.0

Source `e525124` on `feature/photo-vector-studio` makes **3 logical pixels** the
single corner radius for buttons and text controls. The value lives in
`rust/src/control_style.rs` and matches the tighter studio toolbar. All controls
continue using `gpui-omarchy` colours, fonts and native interaction states.

The source review covered every application control constructor and local radius
override, including these presentation paths:

| Area | Shared styling and review |
| --- | --- |
| Toolbar, tool dock and contextual controls | Central button builder; removed per-button overrides. |
| Layers, Develop, Select and Canvas | Inspector buttons/inputs, blend control, numeric fields, layer and mask targets. |
| Vector editing and Image Trace | Shared inspector controls, matching toolbar and context actions. |
| Create, templates, brand, assets, export and motion | Shared controls across all Create tabs and workspaces. |
| Ask Omuse, Connections, provider routing and review | Shared buttons and prompt field; provider names truncate without crowding status. |
| Editing dialogs, save/recovery and recent projects | Shared action/field builders and consistent recent-project selection rows. |
| Command and shortcut search | Shared result buttons; long labels show ellipsis. |
| Colour-picker popovers and inline text | Shared trigger, Apply button, hex input and text-field radius. |

The colour-picker presenter retains the pinned Omarchy implementation's focus,
hex validation, slider, Apply and cancellation logic. Its internal controls now
call Omuse's shared constructors; the upstream MIT notice is retained. Review
this small presentation adapter alongside future `gpui-omarchy` upgrades.

At compact widths, the toolbar keeps a status dot with a full document-name
tooltip instead of a clipped fragment of the name. Wider windows show the
name with ellipsis when needed. Cards, document imagery, circular indicators
and the phone-shaped preview remain separate visual elements.

Compiler, formatting and shortcut-reference checks passed. All **351 UI
regressions passed**, with zero failures and two ignored benchmarks. A second
independent source pass found no remaining button or text-control constructor
bypassing the shared presentation. At the `e525124` checkpoint, the
installed/public 0.6.0 release was unchanged. The 0.7.0 release passed the
recorded native/gallery checks; all captures were visually inspected. See the
[release qualification](release-070-qualification.md) and [gallery](releases/v0.7.0-gallery.md).

Eight native journeys passed at an **800×600 logical viewport**. Dark captures
used Wayland; light captures used XWayland at 1.5× display scale. Both vector
editor journeys passed 27 checks each; Create, Ask Omuse, command search, the
open colour picker, Camera Raw curves and motion passed 24 checks each.
Every capture was inspected for control corners, text fit and alignment. The
AI screen used an isolated profile without sending a provider request. These
are representative visual checks plus a whole-source control audit, not live
AI qualification or a claim that every application state was manually exercised.

Tested executable SHA-256:
`cf5c7de15e9df85e2ba87436cd92a74f255266eb778b670007cbafeeda68b63c`.
Evidence is retained locally under `rust/evidence/`: `control-shape-source.txt`,
`control-shape-check.log`, `control-shape-ui-tests.log`,
`control-shape-native-summary.json`, and the corresponding
`control-shape-native-*/verified-native.json` files and captures.

Screenshots: [editor, dark](user-guide/images/controls-editor-dark.png),
[editor, light](user-guide/images/controls-editor-light.png),
[Create](user-guide/images/controls-create-dark.png),
[Ask Omuse](user-guide/images/controls-assistant-light.png),
[command search](user-guide/images/controls-commands-dark.png),
[colour picker](user-guide/images/controls-picker-light.png),
[Camera Raw](user-guide/images/controls-curves-dark.png),
and [motion](user-guide/images/controls-motion-light.png).

The two vector-colour captures above and the eight shared-control captures are
historical `466f7ab`/`e525124` evidence. The final 0.7.0 gallery contains ten
production views: Pen, vector objects, Image Trace, curves, target-colour
uniformity, Create, Ask Omuse, export, motion and command search. See the
[0.7.0 gallery](releases/v0.7.0-gallery.md) and its [qualification record](release-070-qualification.md).
