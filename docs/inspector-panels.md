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
