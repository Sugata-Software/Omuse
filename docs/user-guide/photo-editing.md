# Edit and finish a photo

[User manual](README.md) · [Remove objects](remove-objects.md) · [Social content](create-content.md)

## Open, protect and inspect

Use **Ctrl+O** to open a photo, then **Ctrl+Shift+S** to save an editable project.
Use **Ctrl+J** to duplicate the selected photo before direct pixel retouch.
Check the layer is unlocked, including its parent group. **Ctrl+0** fits the
canvas; **Ctrl+1** shows actual pixels; hold **Space** and drag to pan.
**Ctrl++ / Ctrl+-** use fixed zoom steps around the centre of your view. Scroll
over a detail to zoom around the pointer. **Ctrl+0** recentres the whole canvas.

For RAW or 16-bit work, keep the retained source. Rasterizing a copy permits
ordinary pixel painting but gives up that copy's original precision/editing
model. [Advanced workflows](../rust-advanced-workflows.md) explains these limits.

## Crop and resize

**Crop:** press **C** to preview a crop. An existing pixel selection supplies
the starting bounds; otherwise the frame starts at the full canvas. Choose
**Free**, **Original**, **1:1**, **4:5**, **3:2** or **16:9** in the top bar.
**Swap** changes portrait/landscape orientation, including **9:16** for stories.
Drag a corner to resize or inside the frame to move it. Arrow keys move one
pixel; **Shift+Arrow** moves ten. Space-drag and middle-drag still pan the view.

Choose **Apply** or press **Enter** when the composition looks right. **Cancel**
or **Escape** leaves the artwork and selection unchanged. Outside pixels are
retained in their original layers; **Ctrl+Z** restores the previous canvas.
Cropping changes the canvas bounds, not the resolution of the underlying photo.

**Resize the image:** use **Ctrl+Alt+I** (**Resize image**) to change the image
and content dimensions. Use this for a smaller deliverable. Inspect fine text
and detail after reducing it.

**Change canvas space:** use **Ctrl+Alt+C** (**Resize canvas**) to change the
bounds without scaling the artwork. To invent content in new space instead,
use **Ask Omuse → Expand canvas**, set the extra pixels for each edge, describe
what should continue, and review before keeping it. Expand can refuse a document
whose live blur/noise adjustment would change the original pixels after growth;
it does not flatten those adjustments automatically.

## Copy editable artwork between documents

1. Press **Ctrl+D** to clear a pixel selection. Select a layer, several layers,
   or a group in **Layers**.
2. Press **Ctrl+C**. Groups, live type/shapes, masks, filters, transforms and
   retained precision stay editable. **Ctrl+X** also removes the selected roots
   if they are unlocked and have no external mask dependents.
3. Open or create the destination document in the same running Omuse session
   and press **Ctrl+V**. The copied roots appear above the active root branch,
   at their original canvas coordinates. Each paste has fresh layer identities
   and is one **Ctrl+Z** step. Copying locked artwork preserves its locks.

With an active pixel selection, **Ctrl+C** copies the selected pixels instead.
**Ctrl+Shift+C** always copies the visible composite. Other applications receive
PNG; editable structure is retained only while this Omuse process owns the
matching clipboard. Separate Omuse processes, clipboard managers and app
restarts do not transport editable trees. A changed external image cannot
silently reuse an older group or mask.

Select live mask sources together with their dependent layers. If a dependency
is missing, Omuse explains the problem instead of attaching to an unrelated
destination layer. Whole-layer copying is capped at **256 MiB** of retained
content and a **16 MP source canvas** for its full-resolution PNG. A refusal
leaves the clipboard and artwork unchanged. For larger work, save/open an
editable project or use Duplicate within the current document.

## Improve tone and colour

For a quick pixel adjustment on a duplicate:

| Adjustment | Default shortcut |
| --- | --- |
| Levels | Ctrl+L |
| Curves | Ctrl+M |
| Hue and saturation | Ctrl+U |
| Colour balance | Ctrl+B |
| Camera Raw controls | Ctrl+Shift+A |

Preview the settings before Apply. The ordinary Levels, Curves, Hue/Saturation
and Colour Balance commands change pixels; Undo restores them. For a retained
source and reorderable effects, search **Filter stack** with **Ctrl+K**, add the
needed effects, preview, and Apply. Reopen the stack to adjust its recipe.

For an assisted edit, select an ordinary photo layer, open **Ask Omuse → Enhance
photo**, and describe the tonal result. For example: “Lift the exposure slightly,
keep the highlights gentle, and reduce the saturation a little.” Review the
proposed changes and Before/After, then **Keep result**. This task creates native
editable adjustment layers. It affects the whole active layer, not a selected
patch, and currently controls exposure, brightness, contrast and saturation.

## Remove distractions

Use **J** for a small spot, **Content-aware fill** for a selection, or **Ask Omuse →
Remove object** for a reviewed AI fill. Follow the [object removal tutorial](remove-objects.md)
for sampling controls, protected areas, shadows and cleanup.

## Cut out a subject or change the background

### Transparent cutout on this computer

1. Select the photo layer and search **Remove background** with **Ctrl+K**.
2. Inspect the local subject result, particularly hair, glass, fur and narrow gaps.
3. For finer control, use **Select subject**, then **Refinement workspace** from
   command search. Paint foreground/background corrections and compare the
   preview on black, white or checkerboard.
4. Apply the refined cutout. The refinement workflow creates a separate result
   and preserves the original layer. Hide an opaque original/background layer
   when you want transparency to show.
5. Export **PNG** (or another format that supports alpha). JPEG cannot preserve
   a transparent background.

The full installer supplies the local subject model. A core-only installation
may need the runtime assets before automatic subject selection is available.

### New scene around a product with AI

1. Select the **subject to keep**. This is the opposite of object removal,
   where you select the unwanted object.
2. Open **Ask Omuse → New background**. Describe the new setting, light and mood.
3. Review the Images connection and request context, then **Preview background**.
4. Compare the result and inspect the subject boundary before **Keep result**.
5. For optional local presentation, open **Product finishing**, choose a **Soft**
   or **Contact** shadow and optionally a **Subtle** reflection. Keep the subject
   selected, then choose **Apply local finishing**. **Remove local finishing**
   removes these Omuse-created layers; Undo restores either action.

Local finishing settings are not sent to the provider. An AI background is a
new image composite; keep your editable project and original layers.

## Finish and export

Use **Create → Assets** for local restoration if needed: choose denoise/sharpen
settings and 1×, 2× or 4× pixels, then **Preview restoration** and **Keep restoration**.
Enlargement interpolates pixels; it does not recover detail that was never captured.

Save the project, then **Ctrl+Alt+Shift+S** to export. Check the exported file's
dimensions, edges, transparency and colour at its intended viewing size. For
16-bit PNG/TIFF use **Precision & colour**, review its supported layer types,
and keep an editable project alongside the output.
