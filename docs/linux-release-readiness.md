# Linux release candidate verification

This is a candidate review, not a declaration of full Mac parity. The installed
application is kept separate from this development worktree.

## First-use acceptance follow-up (2026-09-25)

See [the first-use quality review](linux-first-use-quality.md) for the document
safety, recovery and small-window defects found after the integration pass,
and the expanded acceptance checks. The final `5e4ea87` build passes 443 Swift
tests, 11 native tests, all release journeys, and five live Wayland acceptance
phases. Earlier results below identify earlier
binaries and must not be substituted for the latest release evidence.

## UI integration follow-up (2026-09-25)

The [UI integration report](linux-ui-integration.md) describes the new shortcut
recorder, native layer dragging/nesting, shared color-picker presentation and
connected tool gestures/settings, with explicit remaining boundaries.

## What the initial pass changes

- Qt action notifications are tied to the receiving window's lifetime and queued
  outside control signals. Closing a window cancels pending notifications.
- Structurally unchanged tool headers reuse widgets, preserving typed text,
  focus, selection and slider capture. Toolbar actions, rather than just their
  child widgets, select between shared controls and the native fallback.
- Generic SwiftUI pickers preserve typed tags and dispatch selections to bindings.
  Accessibility labels become accessible names rather than tooltips.
- Shared brush settings reach pointer painting; starting a stroke retains
  smoothing. Native brush setters also update the shared settings.
- Layer-list bracket shortcuts use the shared Mac brush size/hardness rules;
  text entry and non-brush tools retain their normal key handling.
- Shared marquee combine modes and polygonal lasso settings reach canvas input;
  completed selections enable the shared Expand/Contract controls. The selection
  journey now uses visible shared controls.
- Command-line `.comp` packages follow project loading rather than image import.

`--ui-smoke` exercises real Qt key and mouse events against the shared controls,
checks visibility on screen, exported pixels and one-step undo, and closes a
second window with a pending callback. It uses synthetic documents only.

## Completed integration release check (2026-09-25)

The final candidate passes **102 XCTest + 341 Swift Testing tests (443 total)**,
all **11 native tests**, the five existing host journeys, and the expanded UI
journey at **1×, 1.5× and 2×**. Exports are identical across those scales. The same
optimized executable passes the complete UI journey on the live Wayland desktop.
The separate local candidate launcher passes a session smoke and ordinary desktop
startup, with a mapped visible window and isolated preferences/recovery.

The final UI checks include shortcut capture/conflict/cancel/reset/restart,
native tree drop and context-menu events, shared picker commit/cancel and both
visible swatches, shape/gradient pixels and undo, Tab mode cycling, crop ratio,
and the actual font size saved in an editable text layer. App-owned screenshots
were inspected. Native tests also exercise tap, double-tap, submit and context
menu lifetime. These results retain the documented Mac-only test exclusions.

Implementation commit: `a10df77`. Executable SHA-256:
`7c7daa289085dc190922d3c9439d986a178d0ab54f23a624a7a07be3621444d7`.
This remains a local SDK-backed candidate, not a newly built Flatpak release.

## Completed local release check (2026-09-23)

The optimized renderer-enabled run completed with 102 XCTest compatibility tests
and 335 Swift Testing tests passing. All 11 native tests passed without skips.
The session, file I/O, layers/selection, brush and dialog journeys passed, as did
the visible UI journey at 1×, 1.5× and 2× scaling. The three exported PNG fixtures
are byte-identical. The final optimized executable also passed the visible UI
journey on the real Wayland session at the display's native 2× scale.

The initial debug run completed too: it
identified the AppKit-only table test and two stale Linux adapter expectations
for the initial document layer; those are explicitly handled in this candidate.

## Reproduce the local optimized check

Use the KDE 6.11 SDK with the Swift 6 extension on PATH. Build the pinned Skia
raster library at `build/skia-src/out/Raster/libskia.a` as described in the fork's
build instructions, then run:

```sh
PATH=/usr/lib/sdk/swift6/bin:$PATH python3 scripts/linux-release-check.py
```

For a Git worktree, grant the SDK read access to the parent repository's Git
metadata as well as write access to the worktree. The runner records every
phase, exit code, duration and executable hash in `.release-evidence/results.json`
and preserves logs even after a failed phase. It runs the optimized native and
Swift tests, all five existing host journeys, and the new UI journey at 1×,
1.5× and 2× scaling. Preferences and recovery paths are isolated.

The single runtime test exclusion requires `NSTableView`; its Qt counterpart is
covered by the UI journey. Compile-time exclusions are separately enumerated in
[UPSTREAM_TEST_EXCLUSIONS.md](../linux/UPSTREAM_TEST_EXCLUSIONS.md). A passing
local run does not imply those excluded Mac tests passed.

This run uses the cached raster Skia dependency. A fresh Flatpak manifest build,
portal/permission tests of the resulting package, and installation/rollback of
that exact package remain separate gates. GitHub CI has not run these changes.

## Mac comparison boundary

The shared `Compositor/` and `CompositorTests/` trees match Mac commit `75c4219`.
The fetched Mac `main` is `679e66f` (v1.2.4); its six-file delta adds standalone
Vignette, Bloom / Glow and Tonal Contrast filters and associated tests. Those
changes have not been merged into this candidate. Source equality at the older
revision cannot establish current-version feature or visual parity.

The images in `docs/references` are adjustment-dialog design references. They do
not identify a reproducible Compositor build/document/scale combination and must
not be counted as a completed Mac screenshot comparison.

A Mac counterpart must use a recorded commit/build, the same synthetic `.comp`
fixture, installed fonts and color profile. Check both directions:

1. Open the Linux fixture on Mac; verify dimensions, layers, masks, effects,
   text editability and successful export. Save a separate Mac copy.
2. Open the Mac copy on Linux and repeat the checks. Preserve both manifests and
   composite PNGs; do not overwrite the originals.
3. Compare equal-size exports as RGBA with explicit per-channel tolerances;
   separately inspect alpha edges and text. Explain each accepted difference.
4. Capture app-owned windows at the same logical size and document zoom on each
   system. Check panel geometry, tool state, controls and pointer/key journeys.
   Native window chrome differences are platform exceptions; custom UI is not.

No Mac runtime was available for this pass, so this gate is unverified.

## Physical hardware boundary

The checked Linux machine has one connected internal display, reported by Qt as
1440×900 logical pixels at 2× scaling and approximately 60 Hz. AMD radeonsi OpenGL
4.5 initializes under the real Wayland session. Vulkan enumerates Intel Iris Pro
P5200 and llvmpipe, with the driver warning that Haswell support is incomplete.
No AMD Vulkan device is exposed. OpenGL availability does not prove the app's
Skia canvas uses GPU acceleration; this candidate uses the raster Skia build.

No pen tablet or second display was connected. Fractional-scale event tests do
not establish physical stylus pressure/tilt/eraser behavior, hotplug, mixed-DPI
monitor movement, or long-session driver stability.

## Remaining release blockers

Beyond external Mac/tablet/monitor checks, the source audit still identifies
unimplemented shortcut recording, layer drag/nesting integration, gesture and
context-menu adapters, shared color-picker presentation, and some native host
versus shared tool-setting routes. These require separate visible-UI journeys.
The broader inventory remains in [linux-parity-status.md](linux-parity-status.md).
An earlier renderer-free parallel test run aborted with heap corruption; this
serial renderer-enabled run does not resolve that separate failure.
A green regression run is useful evidence for these focused fixes; it must not
be advertised as completion of those features or a polished general release.
