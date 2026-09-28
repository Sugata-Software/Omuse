# Linux first-use quality review

The September 25 acceptance pass found everyday document-handling defects that
previous tool-integration tests did not cover. The fixes stay in the Linux host
and compatibility adapters; protected Mac sources are unchanged.

## Corrected behavior

- Closing a window or its document tab asks Save, Discard or Cancel when edited.
  Cancel, a cancelled file chooser, or a failed save keeps the document open.
- Save remembers the current project path. Save As chooses a new path. Successful
  saves mark the shared history revision saved, so undo/redo tracks unsaved work
  correctly. The tab and window title display the project name and unsaved state.
- New Canvas creates a blank document with the requested pixel dimensions. It
  previously called Canvas Size and retained the old artwork. The toolbar plus
  button follows the same guarded path.
- Open asks about unsaved work. Invalid projects leave the current editor and
  recovery intact. Loaded projects rebind tool controls to the new editor session.
- Image import and file drops add layers and refresh the canvas, retaining the
  existing artwork. Command-line image opening creates an unsaved document.
  Imports consistently apply EXIF orientation and report decode failures.
- Recovery packages are separate for each window. Owner locks prevent startup
  recovery from stealing another live window's work. Startup offers abandoned
  recovery; choosing No keeps it. Recovered work requires an explicit save, and
  its source remains until a successful save or explicit discard.
- A failed save cleans its temporary package and retains the old destination.
  Saving over an unrelated directory or symbolic link is refused.
- The visible zoom percentage follows the actual Qt viewport when fitting,
  zooming or resizing. The document tab close button has its standard icon.
- Tool options scroll horizontally at narrow widths. The tool rail scrolls to
  its final tools. The acceptance journey checks reachable controls at 800×520.

## Verified final build

Implementation: `5e4ea87` (including `e57ce06` and `bfc2112`). The optimized
renderer-enabled run passed **443 Swift tests (102 XCTest + 341 Swift Testing)**,
**11 native tests without skips**, all five host journeys and the expanded UI
journey at **1×, 1.5× and 2×**. The exported PNG fixture is identical at all scales.
The existing documented Mac-specific exclusions remain unchanged.

The exact same binary passed all five live Wayland acceptance phases: document
and real file-dialog handling, integrated UI input, two repeated document/recovery
journeys, and codecs. The restarted local preview was verified mapped with this
binary at 1776×1075 after maximizing it. Screenshots were inspected at normal and
narrow sizes. The installed Flatpak, its launcher, and the user's recovered project
retain their original hashes.

Executable SHA-256: `e3560b30c924d4c3928d4704cd4fb2346f0ba281bb6522aa23143e83e0c55f16`.

Omarchy's default rules suppress application maximize requests. The candidate
requests maximization; the current preview was explicitly maximized through the
window manager. Super+Alt+F remains the desktop shortcut for subsequent launches.

## Acceptance evidence

The expanded Qt dialog journey covers actual Save/Open file choosers; save-path
reuse; saved-history undo/redo; close, cancellation and failed-save handling;
invalid project opening; blank New; imported pixels; controls after reopening;
independent recovery owners; startup recovery; and document-tab close.

The UI journey checks pointer/keyboard input, menus, shortcuts, layers, color,
shape, gradient, crop and text workflows, then checks narrow-window scrolling.
The native renderer regression suite rejects widget reuse across different
editor sessions or panels. Screenshots contain synthetic artwork only.

The release runner records the exact optimized binary, test logs, phase exit
codes and scaling-fixture hashes in `.release-evidence/results.json`. Desktop
acceptance results are recorded separately in `.quality-evidence/acceptance.json`.
See [release readiness](linux-release-readiness.md) for measured results and
[UI integration](linux-ui-integration.md) for supported interaction boundaries.

## Local use and remaining boundaries

Open **Compositor Candidate** from the app menu. It has separate preferences and
recovery from the older installed Flatpak. Projects are `.comp` directories:
use File → Open Project and choose the directory. Ctrl+S saves editable work;
Ctrl+Shift+E exports a flattened PNG. For a narrow tiled window, Omarchy's
Super+Alt+F gives it the full available width.

This is a tested local SDK build, not a freshly packaged general release. Mac
runtime comparison, physical tablet input, mixed-monitor behavior, power-loss
save durability and a fresh package build/install/rollback remain unverified.
Recovery covers the most recent completed autosave (normally every 60 seconds),
not every keystroke. The software cannot be claimed bug-free from these checks.
