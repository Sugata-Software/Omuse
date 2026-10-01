# Unified photo and vector canvas: development qualification

Development checkpoint, 1 October 2026, on `feature/photo-vector-studio`.
The implementation and complete regression candidate is `061602d`.
Source `63048e4` changes only four style-field labels to prevent wrapping in
compact inspectors; its optimized build and both native visual checks passed.
This extends the procedural scene engine checkpoint at
[4e1da33](vector-scene-qualification.md). It is not a public release or
installation update; installed/public Omuse 0.6.0 remains unchanged.

## Interaction contract

- Modern vector artwork shares the main photo canvas, layer stack, zoom/pan,
  inspector, command search and shortcut reference. It opens no artwork modal.
- **P** draws paths; **A** edits anchors/handles; **V** picks and moves objects.
  In Move mode, double-click an object to edit nodes. Picking respects object
  order, fills, holes, strokes, visibility and opacity. Node hit targets use
  screen coordinates under rotation, flips and scale. Entering Move clears node
  selection, so arrows/Delete act on the object rather than an invisible node.
- Valid style input changes preview automatically. Objects can be added,
  duplicated, reordered, hidden or removed. The compact shared inspector puts
  shape creation and styling above advanced controls. Existing retained
  single-path and vector-mask dialogs remain compatibility paths through
  Edit object / Vector mask.
- Draft-local Undo/Redo retains geometry only, capped at 64 entries / 32 MiB
  across both stacks. Done/Enter or an editing-tool/layer/file transition keeps
  a changed session as one document transaction. Cancel/Escape discards it.
  Empty/unchanged edits do not add document history. Invalid fields keep the
  edit open. Same-layer clicks and auxiliary command/shortcut dialogs preserve
  the session. Closing during an owned background operation queues the close
  request; the normal unsaved-work guard still runs.
- Preview jobs use the normal document compositor, retaining outer transforms,
  masks, blending, effects and surrounding layers. Obsolete geometry rendering
  is cancelled; only the newest valid result is presented. A separate transient
  display leaves committed pixels, file output and document history untouched.
  Geometry outlines follow the pointer while the full-resolution preview settles.
- SVG exchange restores main-canvas focus and resumes queued previews.
  Publication is recorded when the file is committed, so cancelling a draft
  does not claim to undo an already-exported file.

## Qualification results

| Check | Source and result |
| --- | --- |
| Complete optimized regression suite | `061602d`: **1,177 passed** — 485 library, 342 UI and 350 integration cases. Four timing benchmarks remain excluded. |
| Editing, saved-project and export journey | `061602d`: passed; disposable projects, exact save/reopen and PNG/JPEG/WebP/TIFF editing/export checks. |
| Complete Create/media/template run | `061602d`: passed; editable social/story projects, PNG/PDF output, native motion preview, MP4/GIF and all 80 template variants. |
| Native GPUI interaction and minimum layout | `061602d`: **27 checks per backend** passed on Wayland/dark and XWayland/light at an 800×600 logical viewport. Owned windows used isolated XDG directories. |
| Final label polish | `63048e4`: optimized build and **27 native checks on each backend** passed again at 800×600. Dark/light screenshots were inspected and all four style fields align and remain visible. The only runtime-source difference from `061602d` is shorter Fill/Stroke/Width/Opacity labels; regression evidence is reused for unchanged behaviour. |
| Source hygiene | Formatting, generated shortcut reference and project-guide freshness checks passed. |

Focused automated cases cover on-canvas object picking, local Undo/Redo, shared
navigation, exact node dragging through rotated/flipped/nonuniform layer
placement, automatic tool/layer exits, cancellation and document identity
fences, same-layer and auxiliary-dialog passthrough, legacy paths/masks, scene
hit testing and preview equality against committed rendering with
masks/effects/occlusion. Additional cases exercise native SVG chooser responses,
canvas focus and preview resumption, export followed by Escape, empty-draft
Undo/Redo isolation, and Nodes → Move → arrow/Delete semantics.

The final native executable SHA-256 is
`92aad22fc1fdee89572a09099ca081763a544b76352f7c045e72d3098641a45c`.

These are local automated and visual checks, not physical-input latency,
long-session photographic quality, portability or installer qualification.
Transformed placement is covered by the headless interaction and compositor
equality cases. The native smoke fixture checks a shared canvas and settled
overlay; it does not independently exercise a transform matrix.

Local evidence paths (not included in the public source tree):

- `rust/evidence/unified-vector-final-source.txt`
- `rust/evidence/unified-vector-final-full.log` and `.exit`
- `rust/evidence/unified-vector-final-native-dark/`
- `rust/evidence/unified-vector-final-native-light/`
- `rust/evidence/unified-vector-polished-source.txt`
- `rust/evidence/unified-vector-polished-build.log` and `.exit`
- `rust/evidence/unified-vector-polished-native-dark/`
- `rust/evidence/unified-vector-polished-native-light/`

The complete run retained its isolated outputs at
`/tmp/omuse-tests.HrqwUVo8/`. The earlier `10bc940` run is retained separately;
its results are superseded by the regression and mode-safety checks above.

## Remaining scope

This adds no scene file-format or pixel-precision change. Scene artwork still
uses format 11 and one RGBA8 source cache; ordinary canvases remain format 10.
Existing public 0.6.0 cannot open format 11. Persistent visible/damaged-tile
caching, gradients, boolean construction, broader editable SVG exchange and
16-bit procedural rendering remain on the roadmap. Full-resolution settled
preview composition is background work; this is not a new latency, peak-memory,
physical-device or installer qualification.
