# Omuse `.comp` project format, versions 1–9

A `.comp` project is a directory package containing `manifest.json` and an
`images/` directory of `<layer UUID>.png` assets. Omuse reads and writes the
JSON manifest and asset layout through its native Rust document layer.

The manifest retains the legacy `com.compositor.project` identifier for wire
compatibility. Omuse writes version `9` for new saves (versions `1`–`8` remain
readable) and records the sRGB working space. It stores document UUID, pixel
dimensions, active layer UUID, and layers in bottom-to-top order. Each layer
stores its UUID, name, visibility, transform (origin, size, clockwise rotation,
flips, sampling), and optional image filename. Blank layers have no image
asset. The identifier is format metadata, not the active product name; see the
[rename compatibility contract](omuse-rename.md).

Readers supporting only older format versions cannot open a new version-9
save. The application version (currently 0.1.0) and project format version are
independent. A shared format number is not proof of complete cross-app
interchange; independently authored fixtures remain a qualification gap.

Embedded PNGs preserve source pixels and transparency; transforms remain
separate. Projects survive moving or deleting imported source photos. Omuse
validates manifest data, writes package assets into a staged sibling and
replaces the destination only after the package is complete. The reader rejects
unsupported versions, invalid
metadata, missing assets, unsafe paths and oversized data before replacing the
live document.

Limits: 30,000 pixels per canvas/image side, 100 million total source pixels,
10,000 layers, 4 MiB manifest, 512 MiB per encoded asset. The active reader is
implemented in `rust/src/document.rs`. The inherited format contract is
attributed in [source provenance](source-provenance.md).

Undo history, pixel selections, and viewport are session-only. Opening fits the canvas, restores the active layer, and starts with clean history. Future editable features must extend the schema and round-trip tests. PNG export is a flattened derivative and does not mark project edits saved.

Image Size adds optional `resolution` (pixels/inch, 1–9600). Older manifests without it default to 72. This additive field retains version 1 compatibility. Both PNG and JPEG exports include document resolution metadata. Resampling stores the new layer pixels and bounds; undo retains the prior sources only during the current session.

Version 2 adds optional `parentID` and `isGroup` on layer records. A group has no image file. Root nodes have no parent; children refer to an existing group. Array order defines bottom-to-top sibling order; renderers traverse each group as a contiguous subtree. Visibility is inherited without changing child flags. Cycles, missing/non-group parents, image-bearing groups, and nesting beyond 64 ancestor levels are rejected. Group ancestors permit room for leaf nodes at the deepest level. Group metadata survives image/canvas resizing and cropping. Older app builds reject version 2 rather than misrender grouped documents. Collapse state is not serialized.

Version 3 adds optional per-layer `opacity` (finite 0–1) and `blendMode` (Normal, Multiply, Screen, Overlay, Darken, Lighten, Difference, Color Dodge, Color Burn). Missing fields default to full opacity and Normal. Group records required those defaults until version 8, which lets a folder carry its own opacity; a folder's opacity multiplies into every layer inside it, while its blend mode stays Normal because folders are pass-through. Effects are applied during compositing and retained as metadata when resizing sources. Files declaring older versions cannot contain non-default appearance values.

Version 4 adds optional `maskFile` and `maskEnabled` fields to individual layers. Mask filenames must be `<layer UUID>.mask.png` under `images/`; enabled defaults to true when a mask exists. Records without masks omit both fields. Groups cannot carry masks in this version. Files declaring versions 1–3 cannot contain mask metadata.

Masks store 8-bit grayscale coverage without alpha (white reveals, black hides). Their normalized extent matches the image’s local rectangle, so the same layer transform applies to both. A uniform 1×1 mask is valid and avoids allocating full-resolution pixels before painting. Nonuniform mask pixels and a thumbnail are immutable assets shared by history. Image Size resamples them with the image transform; Canvas Size and Crop preserve their pixels. Up to 100 million mask pixels may be stored in addition to the existing 100 million image pixels; per-side and per-file limits also apply to masks. Disabled masks remain embedded and editable but do not affect compositing. Image-versus-mask target selection is session-only and reopens on image pixels.

Version 5 adds optional `maskSourceID`: the UUID of a non-group layer supplying live alpha in document coordinates. It multiplies the target’s alpha alongside its enabled raster mask. Source pixels, transform, opacity, raster mask and upstream live masks contribute coverage; visibility and RGB color do not. Sources remain independent layers. Missing references, self-links, cycles, group endpoints and chains over 256 nodes are rejected. Deletion can bake the live coverage into dependent image pixels (retaining their raster masks) or remove the links, as one undoable operation. Links survive image/canvas resize and crop. Older versions default to no live mask; older app builds reject v5.

UI terminology: these alpha links are clipping masks. Alt-click assigns the lower sibling’s base or releases the connection. Multiple clipped layers share one base, show indented above it, and release when moved outside the contiguous stack. The underlying `maskSourceID` representation is unchanged.

Version 6 allows `maskFile` and `maskEnabled` on group records. A folder has no image, so its mask covers the folder's own transform rectangle (the canvas size when the folder was created); Image Size resamples it through that transform, and Canvas Size and Crop preserve its pixels, exactly as for layer masks. Groups are pass-through, so an enabled folder mask multiplies the coverage of every descendant layer, together with that layer's own mask and any enclosing folders' masks; clipping-mask coverage is unaffected. Files declaring versions 1–5 cannot give a group a mask, and older app builds reject v6.

Version 7 adds `adjustment` to non-group records without an `imageFile`. Its `kind` is `Hue/Saturation`, `Levels`, `Curves`, `Exposure`, `Gradient Map`, or `Grain`. The payload contains the corresponding settings. The legacy hue/saturation/lightness/colorize fields remain readable; optional `hsvSettings` takes precedence when present. Optional `exposureSettings`, `gradientMapSettings`, and `grainSettings` default to their identity settings. Adjustment records are rejected in versions 1–6. They retain the layer's placement, appearance, and mask metadata.

Current records also support these additive fields:

| Field | Meaning and default |
|---|---|
| `maskPlacement` | A transform locating a mask independently in document coordinates; omitted means the mask covers its layer's grid. Requires `maskFile` and a valid transform. |
| `maskLinked` | Whether moving the layer also carries the mask; omitted means `true`. A disabled mask retains its placement, linking state, and pixels. |
| `shape` | Optional raster-backed editable shape metadata: `kind` (`Rectangle` or `Ellipse`), `red`, `green`, `blue`, and `cornerRadius` in document pixels. Rounded rectangles use `Rectangle` with a nonzero radius. |

A shape still embeds an ordinary image. Loading associates its style with that image; painting or filtering replaces the image and ends the editable-shape association. Canvas resizing keeps the association; image resizing rasterizes it and drops the shape field. These fields are additive within the current schema; the validator does not impose a separate version gate on shape metadata or mask placement/linking.

An optional `effects` record contains independent `stroke`, `shadow`, `colorOverlay`, `innerShadow` and `outerGlow` records. Stroke carries a size (0–500 layer pixels), a color, an opacity and an `inside` flag choosing which side of the edge it sits on; drop shadow and inner shadow each carry an angle, a distance, a blur, a color and an opacity; color overlay carries a color and an opacity; outer glow carries a size (0–500 layer pixels), a color and an opacity. Each supports optional `enabled` visibility (missing means visible); hidden effects keep all parameters and remain listed under their layer. Effects, including their visibility, are saved and participate in document undo. Canvas previews run on a serial background worker with a shared pixel budget; exports render the full-resolution effects. A record omitting an effect means that layer does not have it, so older readers see the effects they understand and ignore the rest.

| Version | Required reader capability |
|---|---|
| 1 | Canvas, raster/blank layers, separate transforms, optional resolution |
| 2 | Layer hierarchy and groups |
| 3 | Per-layer opacity and blend modes |
| 4 | Raster layer masks |
| 5 | Live alpha links (`maskSourceID`) |
| 6 | Folder masks |
| 7 | Adjustment layers |
| 8 | Document guides and pass-through group opacity |
| 9 | Current writer baseline; full feature validation still applies |

Omuse's Rust reader, writer and validation live in `rust/src/document.rs`, with
round-trip coverage under `rust/tests/`. Run `scripts/test-rust.sh` for the
current document and synthetic edit/save/reopen/export checks. The inherited
schema sources and the earlier implementation remain available through the
[historical source snapshot](source-provenance.md#application-lineage).
