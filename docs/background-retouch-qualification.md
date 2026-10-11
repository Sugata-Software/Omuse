# Cancellable background retouch — development qualification

**Historical development evidence.** The implemented changes below are included
in Omuse 0.10.0. The [0.10 release qualification](release-0100-qualification.md)
records the final source, CI, packages and publication status. Measurements,
source identities and open-gate statements below describe their original
checkpoint; they are not silently rerun or promoted by that release.

This is the next development change after published Omuse 0.9.0 runtime
`65c95cc165f9d730a9f0bcea51c51a84cf4adb7e`. It is not included in those packages
and does not modify their release notes, assets or qualification receipts.

## Behavior

Mouse release captures the native Blur, Smudge or Liquify stroke and starts a
background worker. The same path handles the active raster and existing masks.
Source pixels remain shared and immutable. Computation and large raster
allocations run off the UI thread. Completed output is committed as one Undo
step only after checking the editor instance, revision/change sequence,
selection contents and revision, active layer and document/source identity.
The UI also checks page epoch, dialog generation and mask/raster target.

Escape or the footer's **Cancel · Esc** button requests cancellation and leaves
artwork unchanged. Escape is captured even when a child input has focus. The
editor window's image-operation admission slot remains occupied until the worker exits, so
repeated cancellation cannot queue additional retouch workers. Closing the
owned window cancels the job. A stale completion cannot clear a newer job's
busy state or overwrite its status. A stroke already committed before Escape
is reversed with ordinary Undo instead.

## Scope and limits

Existing native retouch semantics and source/mask dimensions are preserved.
The same 16 MP surface limit, 256 MiB stroke working-buffer limit and work
admission remain. Transferred pointer and copied selection buffers count
toward stroke admission. Shared source ownership, structural metadata, Undo
history, the final composite and display surfaces are not a process-memory cap.
Selection capture and comparison, history publication and the final full
canvas refresh remain on the UI thread. No general latency or throughput
improvement is claimed without an end-to-end measurement.

Spot Heal, Clone, Healing and controlled removal use their existing paths.
This change does not alter the project format or image algorithm, establish
new photographic quality, or qualify new platforms/hardware/AI providers.

## Evidence

Local Linux checks on 5 October 2026 cover source
`fd7a8d7e5862574fd370a2069d6fb27f7bd27f22` on
`feature/cancellable-retouch`. This is an unreleased development checkpoint.

- The optimized application suite passed **1,422 tests**, with no failures and
  four ignored manual benchmarks. The ten background-retouch UI tests cover
  the actual mouse-release worker path for all three tools on rasters and
  masks, exact Undo/Redo, footer cancellation at 800 × 600, Escape with a
  numeric field focused, window disposal, page switching away and back,
  selection/target changes, repeated admission, stale completions and no-ops.
  Escape in another window must leave the original worker alone.
- The earlier **41 focused engine tests** also passed. These are a subset of
  the full suite, not an additional test total. They include source transforms,
  soft selections, legacy folder-mask placement, imported mask coverage,
  allocation failure, cancellation after computation starts, project
  save/reopen and the existing golden retouch fixtures.
- The complete `scripts/test-rust.sh` run passed, including the disposable
  editing journey, the six-page Create collection and editable story variant,
  PNG/PDF export, native motion preview, MP4/GIF export and **80 template
  variants** with saved/reopened editable text and rendered-pixel comparisons.
- Cua Driver exercised the final executable in a fresh XDG profile using
  XWayland on this host: select Smudge, drag a visible stroke, Undo, Redo, Undo
  again, then close the clean test window. Each action was inspected from a
  fresh screenshot. This native smoke does not claim native cancellation or
  every mask/tool path; those are covered by the engine and headless UI tests.
- Source formatting, generated shortcut freshness and changed-document links
  passed. The installed 0.9.0 binary remained unchanged.

The first full run exposed Escape being consumed by a focused input's bound
action before raw key capture. Cancellation now intercepts the keystroke
before action dispatch and is scoped to the owning window. A separate disposal
fixture needed to flush GPUI entity-release effects after dropping its final
test-owned view handle; no production disposal repair was required. Both tests
and the added cross-window regression passed in the final application run.

The final native executable SHA-256 is
`b8076d38b49894bd6e902976e456468d3f9e6a690be8445c5576879693d8abf6`.
The 535-file source manifest SHA-256 is
`f4f2dfa1158fdaf47d0c7aa32e2ca35d303ee1b3c3a2f597a2188a5c3e2c2269`.
Local logs, source manifests and screenshots are retained in the development
host's `retouch-background-2026-10-05` evidence directory. Earlier failed runs
remain separately recorded. The native pixel kernel and selection blending
math are unchanged from the parent source; this change alters scheduling,
admission and publication, not the rendering algorithm.

Windows, other native backends, clean-machine packaging, longer sessions and
end-to-end latency/peak-memory measurements remain separate qualification.
No installed-app replacement, version tag or public release was performed.
