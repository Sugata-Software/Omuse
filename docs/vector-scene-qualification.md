# Procedural vector artwork: development qualification

Development checkpoint, 1 October 2026, on `feature/photo-vector-studio`.
This extends the [first photo/vector foundation batch](photo-vector-foundations-qualification.md).
It is not a public release or installation update. Installed/public Omuse 0.6.0
remains unchanged. The [user guide](user-guide/photo-vector.md) explains the
controls; the [roadmap](photo-vector-roadmap.md) retains the remaining scope.

The tested application source is
`4e1da33734de8164417fe83c8f1eb28164b54d1a`. The optimized regression and
native-window checks, complete Create/media acceptance and the allocation/reader
comparison have passed locally.

## Implemented scope

- **Shift+P → Vector artwork** creates several paths, rectangles and ellipses
  in one layer. Previous/Next selects an object; controls edit stacking order,
  visibility, solid fill/stroke and opacity. Alt-drag moves the selected object;
  ordinary node/handle controls edit its geometry. Apply is one Undo step.
- The authoritative scene retains bounded paths, styles, transforms, order and
  object identities. All objects share one derived RGBA8 layer image. Rendering
  uses bounded temporary tiles and object culling, with flattened-segment work
  admission and cancellation. Draft previews retain one active and the newest
  requested job, rejecting stale results.
- Canvas format **11** stores bounded compressed scene geometry and verifies
  dimensions, the geometry checksum and the saved cache's raw-RGBA SHA-256.
  The saved cache preserves appearance without rendering untrusted geometry
  during load. Ordinary
  canvases still write format **10**. Older readers refuse scene projects.
- Painting, cut, bake and destructive photo edits require explicit Rasterize.
  Scene/cache edits commit together, reject stale revisions and respect
  document budgets. Clipboard/history share retained sources. Image replacement
  detaches obsolete scenes; Create document identity includes geometry even
  when cached pixels are equal.

## Regression and native evidence

The optimized suite passed **1,165 tests**: 477 library, 338 UI and 350
integration cases. Four explicitly ignored timing benchmarks were not run.
The complete `scripts/test-rust.sh` wrapper exited **0**. The standalone editing
journey, editable Create project, PNG/PDF package, story variant, MP4/GIF motion
exports and all **80 editable template variants** passed. The template run
compares saved/reopened native text and rendered pixels exactly.

Focused coverage includes tiled-versus-full pixels, curved stroke work limits,
even-odd holes, straight alpha and object opacity, cancellation, stale Apply,
cache/source invariants, clipboard sharing, scene-aware Create history and
layout resizing, explicit raster conversion, format/version handling, swapped
geometry files, digest/schema refusal, cumulative expanded geometry and
trailing compressed data. The compact UI journey uses clicked inputs, real
headless keystrokes, object controls, Alt-drag, Apply, reopen and Undo.

Both **Wayland/dark** and **XWayland/light** native runs passed the same **24
common editing checks** at an 800×600 GPUI viewport. These are repeated backend
checks, not 48 distinct new vector tests. The workspace captures were visually
inspected: object controls and the complete artwork preview appear before the
scrolling style/path settings; Apply and Cancel stay in the fixed footer.
XWayland's compositor capture is 1200×900 because its display scale is 1.5.

Both runs used the optimized **ui-test-enabled development binary**, SHA-256
`eaf0eaf538bd507f1af8201aea9b55cccaa987e79952c86cdf7dba5811e00c6e`.
This does not qualify a production installer, physical input devices, a new
provider connection or another hardware configuration.

## Allocation and saved-project comparison

The optimized `vector_scene_acceptance` example creates five coloured objects
on a 4096×4096 canvas, checks one Undo/Redo and saves/reopens the scene with exact
geometry and cache equality. It accounts for the retained scene representation
and compares it with independently allocated raster-backed buffers:

| Representation | Accounted bytes | MiB |
| --- | ---: | ---: |
| Five-object geometry plus one RGBA8 cache | 67,110,590 | 64.002 |
| Independent tiled 16-bit source, result and RGBA8 proxy | 335,675,392 | 320.125 |

The new scene's geometry accounts for 1,726 bytes and its image for 67,108,864
bytes. This is about **80% less retained representation storage** in this
synthetic comparison. It excludes the document's initial blank layer, history,
transient allocations and other process memory. It is not a peak-RSS or speed
benchmark, and does not compare Omuse with another application.

The development CLI reopened the saved format-11 project and exported a PNG
whose RGBA pixels match its saved scene cache exactly. The exported artwork was
visually inspected. The installed production **Omuse 0.6.0** reader refused
version 11 with exit code 1, created no output and left every project file
unchanged. Its executable SHA-256 remains
`bf3b8f5f510865fd832d3305b1b2ea40505c5eab6c93bf9fa2d49dc629b41a46`.
Ordinary format-10 save behaviour is covered separately by integration tests.

Machine-local evidence:

- `rust/evidence/vector-scene-summary.json`: source/binary identities, counts,
  allocation measurement and reader results.
- `rust/evidence/vector-scene-full.log` and `.exit`: complete optimized run.
- `rust/evidence/vector-scene-native-dark/` and `vector-scene-native-light/`:
  native receipts and owned-window captures.
- `rust/evidence/vector-scene-release-artwork/`: format-11 project, exact PNG
  export, `vector-scene-measurements.json` and `reader-compatibility.json`.
- `/tmp/omuse-tests.50dOhYMh/`: retained editing, Create/motion and template
  outputs from isolated test data/configuration directories.

## Limits

This is the first Q01/Q02 scene slice. It still retains one full-source 8-bit
image per scene; persistent visible/damaged-region caching is not implemented.
Scene hierarchy, direct-canvas selection, boolean construction, gradients,
advanced strokes and typography remain roadmap work. Editable SVG exchange is
still one selected object; grouped opacity is refused by its exporter.

Limits include 1,024 objects, 100,000 total anchors, 4,096 subpaths and
16,777,216 source pixels per scene, a 256 MiB aggregate retained scene budget,
bounded compressed/expanded storage, a 256 MiB cumulative expanded-JSON budget
per project and an aggregate render-work ceiling.
General affine fills are supported; stroked objects require uniform orthogonal
transforms. Additional document/cache limits apply.

The scene cache is 8-bit. High-precision export promotes that cache and does not
add colour precision. Legacy path/filter recipes retain their existing model.
Owned-allocation accounting is not process RSS, peak memory, latency or broad
artwork-quality evidence. No comparison with another editor's speed is claimed.
