# Edit and finish a photo

[User manual](README.md) · [Remove objects](remove-objects.md) · [Social content](create-content.md)

## Open, protect and inspect

Use **Ctrl+O** to open a photo, then **Ctrl+Shift+S** to save an editable project.
Use **Ctrl+J** to duplicate the selected photo before direct pixel retouch.
Check the layer is unlocked, including its parent group. **Ctrl+0** fits the
canvas; **Ctrl+1** shows actual pixels; hold **Space** and drag to pan.

For RAW or 16-bit work, keep the retained source. Rasterizing a copy permits
ordinary pixel painting but gives up that copy's original precision/editing
model. [Advanced workflows](../rust-advanced-workflows.md) explains these limits.

## Crop and resize

**Crop:** press **M**, drag a rectangle around the composition you want, then
press **C** (**Crop to rectangle selection**). Inspect and use **Ctrl+Z** if the
crop is wrong. The C command performs a crop; it is not a separate crop tool.

**Resize the image:** use **Ctrl+Alt+I** (**Resize image**) to change the image
and content dimensions. Use this for a smaller deliverable. Inspect fine text
and detail after reducing it.

**Change canvas space:** use **Ctrl+Alt+C** (**Resize canvas**) to change the
bounds without scaling the artwork. To invent content in new space instead,
use **Ask Omuse → Expand canvas**, set the extra pixels for each edge, describe
what should continue, and review before keeping it. Expand can refuse a document
whose live blur/noise adjustment would change the original pixels after growth;
it does not flatten those adjustments automatically.

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
