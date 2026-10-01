# Draw, trace and refine artwork

**Omuse 0.7.0** keeps photos and editable vector artwork on the same canvas.
Use **P** to draw, **A** to refine points and **V** to move objects. Trace a bitmap
when you want a simpler graphic, or use the colour tools to refine a photo.

[User manual](README.md) · [Photo editing](photo-editing.md) ·
[Create content](create-content.md) · [Current interface gallery](../releases/v0.7.0-gallery.md)

## Make a product colour more consistent

Target colour uniformity brings nearby hues and saturation closer to a chosen
reference while letting you retain tonal texture. It is a local editing tool;
it does not use an AI account or identify skin, clothing or products for you.

1. Select an unlocked image layer. If other parts of the picture share the same
   colour, make a selection around the area you want to change first.
2. Open **Develop → Filter stack**, or search **uniformity** with **Ctrl+K**.
3. Choose **Target colour uniformity**. Enter the reference as `#RRGGBB`.
4. Set the hue **range** and **falloff** in degrees. Range gives full coverage
   on each side of the reference hue (20° means ±20°); falloff fades the
   adjustment to zero beyond it. Their sum cannot exceed 180°.
5. Set hue, saturation and lightness uniformity independently from **0 to 1**.
   Start with lightness at **0** to retain the source's HSL lightness and texture.
   Grays and near-neutral colours are protected from arbitrary hue changes.
6. **Add effect**, attach **Use selection mask** if wanted, then inspect
   **Refresh preview**. **Apply** commits one Undo step. Reopen the stack and use
   **Update selected effect** to revise it later.

The original pixels, parameters, opacity and node mask remain editable in the
`.omuse` project. The calculation uses encoded-sRGB HSL, including for selected
colours in P3 sources; it is not perceptual luminance matching or a complete
wide-gamut colour-grading engine. Broader photographic quality remains under
evaluation. Target-colour nodes are new: **0.6.0 cannot open a project containing
one**. Use Save As to keep a copy readable by that release.

![Target Colour Uniformity controls with live reference swatch](../releases/images/v0.7.0/05-target-colour.png)

*The reference swatch shows the target colour. Range and falloff limit the hues
affected; the three strength controls preserve as much variation as you choose.*

## Build several objects in one artwork layer

1. Press **Shift+P** or search **Vector artwork on canvas** with **Ctrl+K**.
   Artwork opens on the main canvas, with its object list and controls in the
   shared **Layers** inspector. There is no separate artwork dialog. Starting
   from a photo or ordinary layer creates a new artwork layer when you finish;
   selecting an existing artwork layer edits that scene.
2. Choose **Rectangle**, **Ellipse** or **New path**. Click an object in
   the inspector list, use **Previous** / **Next**, or press **V** and click its
   visible artwork on the canvas. The list shows the topmost object first.
3. Press **V** to drag an object, **A** to edit its nodes, or **P** to draw a path.
   In Move mode (**V**), double-click an object to edit its nodes. **Alt-drag** moves the selected
   object, including an object with transparent paint that is visible only as
   an editing outline.
4. Set fill/stroke colours as `#RRGGBB` or `#RRGGBBAA`, stroke width in
   layer-source pixels (0–4096), and **Opacity** (0–100%). Valid field
   changes update the preview automatically; **Update style** also applies the
   current fields. Each colour field includes a live swatch; its checkerboard
   shows transparency. Click the swatch or hex value to focus the field.
   A fill alpha of `00` gives no visible fill; stroke width **0** disables the
   stroke and shows **Stroke · none** with a crossed swatch. An incomplete or
   invalid hex value shows a question mark and keeps the last valid artwork.
5. Use **Lower**, **Raise**, **Duplicate**, **Hide object** / **Show object** and
   **Remove** to arrange the artwork. Removing the last object leaves an empty
   path ready for drawing.
6. Press **Enter** with the canvas focused, or choose **Done**, to keep the
   complete edit as one document Undo step. **Escape** or **Cancel** discards
   the draft. Select the artwork layer and press **Shift+P**, **P** or **A** to
   edit it again.

Switching between **P**, **A** and **V** stays within the same draft. Choosing
another editing tool, selecting another layer or saving first keeps the draft,
then continues the requested action. An invalid style field prevents finishing
and leaves the draft open: correct it, or use **Cancel** to discard the edit.

While editing, **Ctrl+Z** and **Ctrl+Shift+Z** undo and redo individual draft
changes without leaving the canvas session. Draft history is bounded to 64
entries and 32 MiB. Once you finish, the entire session becomes one document
Undo step.

Navigation is shared with photo editing: hold **Space** and drag, or drag with
the **middle mouse button**, to pan; **scroll** to zoom around the pointer.
**Shift+scroll** pans. The canvas displays the draft through the normal document
compositor, so its settled preview includes the surrounding photo layers and
the artwork layer's placement, masks, opacity, blend and supported effects.
Node outlines follow interaction while the background preview updates.

All objects share one scene and one display image. This keeps multiple shapes
from each retaining a full-canvas photo-processing source and result. Limits
remain 1,024 objects, 100,000 total anchors, 4,096 subpaths and 16,777,216 source
pixels, with additional rendering-work and document budgets. Complex artwork
can reach a budget before the object count. Previews run in the background and
keep only the newest requested update.

Save as **`.omuse`** to retain objects, styles and their order. A canvas containing
this artwork uses **format 11**, which **0.6.0 cannot open**.
Use **Save As** to keep a compatible original. Other canvases still use format
10. **Rasterize** explicitly converts the layer to pixels before painting or
destructive filters; Undo can restore its editable objects. Layer placement,
masks and supported layer effects remain available around the shared artwork.

The scene still uses an 8-bit display cache. Gradients, boolean construction,
text objects inside the scene, a persistent visible-tile cache and complete
multi-object SVG exchange remain unavailable. Exporting artwork through the
16-bit path promotes its existing 8-bit cache; it does not create additional
colour precision.

![Multiple editable objects in one vector artwork layer](../releases/images/v0.7.0/02-vector-artwork.png)

*Choose an object in the artwork inspector to change its fill, stroke, opacity
or position in the scene. The whole scene stays in one artwork layer.*

## Draw and refine an editable path

Press **P** to draw on the main canvas. On a photo or ordinary layer, this starts
a new artwork layer; you do not need to create a blank layer first. On existing
vector artwork it opens that scene in Pen mode. **A** enters Node mode, and
**V** selects and moves objects. The inspector's path controls affect the
selected object. Zoom in when nearby anchors and handles overlap so you can
pick each one precisely.

![Pen tool and editable Bézier curves on the main canvas](../releases/images/v0.7.0/01-vector-pen.png)

*Click or drag with **P** to place corners and smooth Bézier points; switch to
**A** to move anchors and handles in the shared canvas.*

- In **Pen** mode, click to place a sharp corner or **click-drag** to draw a
  smooth point with opposing Bézier handles. Click the first anchor of an open
  contour to close it. Choose **New path** or **New subpath** to start another.
- In **Nodes** mode, drag an anchor to move it together with its handles, or drag
  a handle dot to reshape the curve. Smooth handles stay aligned; hold **Alt**
  while dragging a handle to break that alignment. **Shift** constrains anchor
  movement and handle directions to 45-degree increments.
- In Pen mode, click near a segment to add a point. In Nodes mode,
  **double-click a segment**. The curve is divided exactly at that location,
  so inserting a point does not change its shape.
- **Smooth node** creates handles; **Clear handles** makes a corner. The
  inspector reports the current path's anchor count and selected point.
- **Insert midpoint** divides the segment after the selected anchor while
  preserving its curve. At the end of an open path, it divides the preceding one.
- **Open/Close path** and **Reverse path** act on the selected subpath, or the
  last subpath when no node is selected.
- **New subpath**, followed by Pen-mode clicks, starts another contour.
  **Fill: even-odd** makes overlapping contours into holes. **Fill: non-zero**
  uses their winding direction instead.
- **Arrow keys** nudge the selected anchor and its handles by one layer-source
  pixel; **Shift+Arrow** nudges by ten. With no anchor selected, they move the
  selected object's entire path.
- **Delete node** removes the selected anchor. **Delete** also removes an
  anchor when one is selected; otherwise it removes the selected object.

Geometry and styles remain editable in `.omuse`. **Done** / **Enter** keeps the
whole draft as one document Undo step; **Cancel** / **Escape** discards it.

### Older retained paths and vector masks

Older single-path recipes and vector masks keep their existing retained model;
opening a project does not silently convert them into a multi-object scene.
Select an older retained-path layer and use **Ctrl+K → Edit text, shape or vector
artwork** to edit its existing recipe. These compatibility path/mask dialogs
retain **Apply** / **Cancel**; **P** opens the modern scene workflow instead.
In a legacy path dialog, Apply replaces that layer's artwork with the path;
its checkerboard preview does not show an old photograph beneath it. Retained
effects appear after Apply. **Ctrl+K → Vector mask** opens the mask workflow,
which retains editable geometry and applies it as coverage over the layer's
image.

## Turn an image into editable vector artwork

Select a pixel image layer and choose **Image trace** in **Layers**, or use
**Ctrl+K → Image trace / retrace**. The trace uses the ordinary canvas and right
inspector. It runs locally without an AI provider or subscription.

1. Start with **Logo** for a black silhouette, **Illustration** for flat colour,
   or **Photo art** for a stylised photographic result. You can then switch
   between **Colour**, **Gray** and **B&W**.
2. Use **Source** and **Trace** above the canvas to compare. The document stays
   unchanged during preview. You can zoom, pan and open command search; keep or
   cancel the trace before another edit.
3. Adjust the controls below. New settings replace older pending previews;
   **Keep vectors** becomes available only when the current valid preview is
   ready. An invalid field or budget error keeps the last preview visible and
   explains what needs changing.
4. Choose **Keep vectors**. Omuse adds an editable artwork layer directly above
   the image in the same group, preserves its placement and supported layer
   treatment, and hides the original bitmap. Node editing opens immediately.
   **Cancel** or **Escape** before keeping leaves the document unchanged.
5. Use **V** or the inspector's object list to choose a coloured shape, **A**
   to edit its points, and **P** to add curves. Same-colour regions can share
   one compound object. Finish that editing session before using document Undo:
   keeping the trace is one Undo step, and any subsequent point-edit session is
   another.
6. Save an **`.omuse`** project to retain the source, geometry and trace settings.
   Select the vector layer later and choose **Retrace original image** to revise
   its settings. Retracing replaces manual point edits in that layer; duplicate
   the vector layer first if you want to keep that version. Source and trace
   remain separate layers.

| Control | What it changes |
| --- | --- |
| Colours · 1–32 | Maximum palette size for Colour and Gray. Fewer colours make simpler artwork. |
| Detail · % | Higher values follow the pixel contours more closely; lower values reduce anchors. |
| Smoothing · % | Adds and adjusts Bézier handles instead of retaining only straight segments. |
| Keep corners · % | Higher values protect sharper turns from smoothing. |
| Noise · px² | Merges smaller connected regions into a neighbouring region; measured at processing resolution. |
| Threshold · 0–255 | In B&W, pixels darker than this become black foreground. Lighter pixels are omitted. |
| Process edge · px | Longest processing edge, 16–4096; never enlarges the source. The engine also caps working pixels and estimated work. |
| Point limit | Safety ceiling, 4–100,000 anchors. If exceeded, lower Detail, Colours or Process edge, or increase Noise. It does not silently discard shapes to meet a target. |
| Ignore white | Omits near-white regions in Colour/Gray. |
| Ignore border colour | Omits the dominant border colour when it covers at least half the border. Inspect the result because foreground can share that colour. |

The inspector shows editable points, paths and reduction from the original
pixel contours. Point reduction is a geometry count, not a visual-quality
score. Logos, silhouettes and flat illustrations are the best starting
material. Photo art produces solid-colour shapes, not a lossless photograph,
editable text, gradients or a reconstruction of the original design.
The current canvas preview uses the source image's pixel dimensions; zooming
into a small source can look pixelated even though the retained curves are editable.

Tracing accepts source layers up to **16,777,216 pixels**. Processing is bounded
at four megapixels, two million raw contour edges, 100,000 anchors and 4,096
subpaths, plus work and whole-document budgets. Higher colour counts can reach
the work limit before the edge limit. Lower resolution or simplify the source
if a budget is exceeded. Masks and supported layer effects are applied around
the trace; they are not converted into vector contours. The retained original
makes this use more project storage than replacing the bitmap would.

This workflow uses format 11. **0.6.0** cannot open it. Its
measured checks and remaining limits are recorded in the [trace and curve
qualification record](../image-trace-qualification.md); those measurements remain
historical. The [0.7.0 qualification](../release-070-qualification.md) records
the final production build and fresh release checks.

![Local Image Trace controls on the main canvas](../releases/images/v0.7.0/03-image-trace.png)

*Compare Source and Trace above the canvas. Use the inspector to adjust colours,
detail and point limits; scroll it to reach the remaining settings.*

## Exchange an SVG path

In the artwork inspector or legacy path dialog, choose **Import SVG path**.
The SVG's complete viewport is fitted proportionally into the artwork's source
dimensions and centred, preserving empty space and compound holes. Review the
draft, then choose **Done** on the canvas or **Apply** in a legacy dialog, or
**Cancel** to discard it. A vector-mask draft imports geometry only.

In **Vector artwork**, import replaces the selected object's path and style;
create a **New path** first to preserve the selected object. Export writes only
the selected object and requires **100% object opacity**. Fill/stroke alpha is
supported; grouped object opacity is refused by this single-path exporter.

This explicit importer accepts **one painted path or basic shape**, including
multiple subpaths, cubic curves, solid colours, fill/stroke opacity and supported transforms.
Strokes must use round caps and joins, with a uniform transform. It refuses
multiple painted objects, text, embedded images, gradients, pattern fills,
effects, clipping/masking and external resources. The ordinary Open/Import SVG
workflow remains available when a rasterized image is what you need.

**Export path SVG** writes the draft geometry and solid style in layer-source
coordinates. Choose a new `.svg` filename; existing files are preserved. This
does not export the complete composition, photos, filter effects or the layer's
placement on the canvas. Save the `.omuse` project to retain those elements.
Both editable SVG directions are bounded to 4 MiB, 100,000 anchors and the
document's resource limits. A limit or unsupported feature produces an error
rather than silently flattening the artwork.

Cancel before export publication discards the staged file. Once the status
reports the path was exported, closing the draft keeps that completed SVG.

## Export retained 16-bit detail

**Develop → Precision & colour → Export 16-bit PNG/TIFF** renders retained
masters without first reducing them to the 8-bit display cache. For transformed
layers, **High quality** uses scale-aware Lanczos reconstruction and
premultiplied alpha; **Smooth** uses bilinear sampling and **Nearest** retains
hard pixel edges. High quality costs more CPU time. Its reduction footprint is
bounded at a 3/32 scale floor, so extreme reductions remain approximate.
Reduced local/folder masks are integrated with the source samples to avoid
leaking masked-out colours. The 8-bit canvas still filters those masks
independently, so check the exported file when reducing intricate masked detail.

This does not make the complete application a high-precision/wide-gamut editor.
The Camera Raw compatibility node still passes through 8-bit data, previews are
8-bit, ordinary painting remains raster work, and traditional live adjustment
layers/effects remain unsupported by 16-bit export. See the
[precision limits](../rust-advanced-workflows.md#3-precision-and-colour).

## Reproduce the synthetic acceptance artwork

The [foundation record](../photo-vector-foundations-qualification.md),
[vector scene checkpoint](../vector-scene-qualification.md),
[unified-canvas record](../unified-vector-canvas-qualification.md) and
[trace record](../image-trace-qualification.md) retain historical development
measurements and captures. The [0.7.0 release record](../release-070-qualification.md)
covers the released runtime. Future work belongs in the
[photo/vector roadmap](../photo-vector-roadmap.md).

Developers can generate disposable colour and compound-path examples:

```sh
cargo run --manifest-path rust/Cargo.toml --release --locked \
  --example photo_vector_acceptance -- /tmp/omuse-photo-vector-artwork
```

The destination must not exist. The example writes before/after PNGs, a 16-bit
PNG, an editable SVG and two `.omuse` projects. It checks source preservation,
unaffected colours, Undo/Redo and persistence. These synthetic fixtures establish
specific invariants; they do not establish quality on independent photographs.

The multi-object scene example creates five coloured objects on a 4096×4096
canvas, checks Undo/Redo and save/reopen, and records retained allocations:

```sh
cargo run --manifest-path rust/Cargo.toml --release --locked \
  --example vector_scene_acceptance -- /tmp/omuse-vector-scene-artwork
```

Choose a new destination. Its JSON compares scene geometry plus one RGBA8 cache
with independently allocated 16-bit source/result buffers and an 8-bit proxy.
It excludes the initial blank layer, history and other process allocations;
it is not an RSS, peak-memory or rendering-speed measurement.
