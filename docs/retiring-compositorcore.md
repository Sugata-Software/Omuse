# Retiring the forked CompositorCore

`Sources/CompositorCore` is the original hand-edited port. The wrap-don't-fork decision replaces it with upstream's
unmodified `Compositor/{Document,IO,Rendering}` (target `Compositor`, built through the Apple-API shims). The shell talks to
the session through a C command interface (`EditorBridge`); the replacement is `Sources/LinuxBridge/UpstreamEditor.swift`,
an adapter that lives inside the `Compositor` module (via `Sources/UpstreamCore/LinuxBridge`) so it can reach upstream's
internal types without editing them. It holds no editing logic: every command calls an upstream method.

## Status: done

The fork is gone. `Sources/CompositorCore` (72 files) was deleted; the Qt shell now runs on upstream's unmodified
`EditorSession` end to end:

- `Sources/LinuxBridge/SessionABI.swift` exports the same `compositor_session_*` C ABI (create, close, command, state, render,
  import_rgba, manifest and layer export/import) from the `Compositor` module; `Sources/LinuxBridge/UpstreamEditor.swift` is the
  adapter. `CompositorHostBootstrap` and the static `CompositorCore` library product now link `Compositor`.
- Every command the host sends is mapped (about 77): documents, layers, groups, masks, transform and distort, selection,
  fills, clipboard, brush (paint, erase, mask, spot healing, clone stamp), warp, magic wand, filters, adjustments, shapes,
  canvas/image size and crop, remove background, content fill, project manifest and layer pixels. Clone Stamp's source
  point and aligned offset are tracked server-side (`EditorSession.setCloneSource`/`cloneOffset`), so the host only sends
  a `cloneSetSource` command on Option-click and a plain `brushBegin` with `"kind":"Clone"` after — no offset math on the
  Qt side. Unmapped commands (`smartMatte` refinements beyond remove background) return -7.
- Render composites what the canvas shows: pending transforms and live filter/levels/hue previews are applied, and
  background preview tasks are given a bounded moment to finish (`settle`).
- Verified by: the host bootstrap journey (create, new, paint, render, undo, redo, close through the C ABI, then Qt host entry)
  and all four Qt smoke modes (dialog: canvas/image size, filter preview, commit, undo, command palette, autosave,
  save/reopen; io; layers; brush — including a real Clone Stamp/Spot Healing stroke through `SessionWindow`'s own
  `cloneStroke`/`healStroke`/`setCloneSource`, the same methods `mousePressEvent` calls), plus
  `Tests/LinuxOverrideTests/UpstreamEditorTests` (15 tests, began as differential tests against the fork), upstream's own
  260 tests (up from 222 after the 1.2.0-1.2.2 merge; two more count now that
  `translucentStrokesDrawLikeOneImage`/`maskStrokesDrawLikeOneMask` are no longer skipped — the 25° tile-clip fix
  earlier this session covered them, a stale skip in later test runs just hadn't been dropped), and 96 compat-layer
  tests (`Tests/CompatTests`).

Divergences from the old fork, upstream being the target: finer undo names and File > New undoable; `cut` is copy + clear;
Gaussian blur grows the layer by 3 sigma; a soft mask stroke hides only the stroke; invalid sizes return -1; cloning from a
transparent source onto opaque pixels correctly does nothing (ordinary source-over, not a replace/copy blend — matches
Photoshop, tripped up the first version of the clone smoke test).

Threading: sessions are main-actor objects entered from the shell's main thread; async upstream work is awaited by pumping
the run loop from that plain callback (`awaitOnMain`), never from inside a main-actor job (tests use `commandAsync`).

Follow-ups: live in-progress brush stroke is composited only when the stroke ends (the fork drew it live); Flatpak build
not re-run here (the product name `CompositorCore` is unchanged).
