# Upcoming 0.9 editing workflows

[User manual](README.md) · [Photo editing](photo-editing.md) · [Text and layout](create-content.md)

**Development preview:** these instructions cover work being prepared for
Omuse 0.9.0. They are not a claim that 0.9.0 is released or that its desktop
checks are complete. The published 0.8.0 downloads do not include these new
controls. Native-resolution retouch integration is still in progress.

## Align artwork with a configurable grid

1. Open **Canvas → View & alignment** in the right inspector. Turn on
   **Pixel grid** and **Snapping**. **Ctrl+'** toggles the grid; command search
   (**Ctrl+K**) also finds **Toggle grid** and **Toggle snapping**.
2. Click **Grid spacing** to cycle the major spacing through **4, 8, 16 and
   32 canvas pixels**. Click **Grid subdivisions** to cycle **1, 2, 4 or 8**
   subdivisions. These are preset buttons, not free-form numeric fields.
3. For example, **16 px** spacing with **4** subdivisions puts minor lines
   **4 px** apart. Snapping uses the minor lines. Enable **Guides** when you
   also want nearby guides to attract the pointer.
4. Drag with **Move**, or draw a rectangular/elliptical selection, rectangle,
   ellipse, line or gradient. Snapping acts when the relevant pointer is near
   a configured grid line or visible guide; it does not change every brush stroke or
   automatically realign existing artwork.
5. Hold **Shift** to temporarily bypass **grid** snapping. Visible guides can
   still snap, and Shift keeps its other tool behaviour, such as selection
   combination or transform constraints. Turn **Snapping** off for an entirely
   unsnapped placement.

Grid spacing, subdivisions and visibility are remembered as workspace
preferences. Dense grids simplify their drawing as you zoom out; snapping
retains the configured minor spacing. They do not add an Undo step or appear in exported artwork.
**Change grid spacing** and **Change grid subdivisions** are also searchable
commands without default keys; assign your own with **Ctrl+Alt+K**.

## Inspect a mask before refining it

1. In **Layers**, locate a layer with a **MASK** badge. If it has no mask,
   create one first using the existing [cutout workflow](photo-editing.md#cut-out-a-subject-or-change-the-background).
2. **Alt-click the MASK badge** to inspect that mask in grayscale. White
   represents revealed coverage, black hidden coverage and gray partial
   coverage. The badge highlights the inspected mask.
3. Inspect edges and holes at **100%** with **Ctrl+1**, or fit the canvas with
   **Ctrl+0**. This is an inspection view: canvas painting is blocked while
   it is active.
4. **Alt-click the same badge again** to return to the artwork. Alternatively,
   select that layer and run **Inspect layer mask** through **Ctrl+K** to
   toggle the view. **Escape** also leaves inspection when the canvas has focus.
5. To change the mask, first leave inspection, then click its badge normally.
   Normal clicking selects the layer and enables mask painting. Paint white
   to reveal or black to hide, then inspect again.

Inspection does not alter the photo, save a new mask, or add a document Undo
step. Mask inspection and mask painting are separate modes. Leave inspection
before judging the composite or exporting a finished image.

## Release an ordinary folder without flattening its children

1. Select one folder in **Layers**. Finish or cancel any floating pixel
   selection first.
2. Open **Layer actions → Ungroup folder**, or search **Ungroup folder** with
   **Ctrl+K**.
3. Its children replace the folder at the same position in the layer tree,
   retaining their order, placement and editable contents. They become the
   selection. **Ctrl+Z** restores the folder in one step.

The folder must be visible and unlocked, with unlocked parents and children,
**100% opacity**, **Normal** blending, no mask or effects, and no placement
transform. An empty folder cannot be ungrouped. A refusal leaves the artwork
intact; keep an appearance-bearing folder rather than removing settings that
contribute to the result just to enable the command.

This command operates on one ordinary layer folder. It has no default key.
**Ctrl+Shift+G** remains **Ungroup vector selection** for vector objects; it is
not a shortcut for releasing a photo-layer folder.

## Preview a font on selected letters

1. Press **T** and click existing live text, or create a text box. Select the
   letters you want to restyle in the inline text field. Use **Ctrl+A** in that
   field when you intend to change the whole text.
2. Click **Font · …** below the field. Type part of an installed font name
   to filter the list. A selection containing different fonts is labeled
   **Mixed fonts**.
3. Hover a font, or use **Up/Down**, to preview it on the captured selection.
   Click a font or press **Enter** to keep that choice in the text draft and
   return focus to the selected letters.
4. To abandon the font preview, press **Escape**, click the font button again,
   or click outside the popup and text box. The previous formatting and text
   selection return; this does not cancel the entire text edit.
5. When the complete text is ready, choose **Apply** or press **Ctrl+Enter**.
   The complete edit becomes one document Undo step. After closing any font
   popup, **Cancel** or **Escape** abandons the text draft.

With only a caret and no selected letters, a chosen font applies to text you
type next. It does not restyle all existing letters. Moving the caret follows
the surrounding text style again. Choose the font explicitly before applying
the text: an unconfirmed hover is restored when the text draft finishes.

The list uses fonts installed on this computer and shows up to 64 matches at
a time; narrow the search if needed. This control edits live text, not lettering
already baked into a photo or an unsupported imported Photoshop text layer.

## Control blur strength separately from brush size

**Integration in progress:** the new **Radius px** field is present in the
development options bar; its native-resolution retouch path is being integrated
and reviewed. The following describes the intended 0.9 workflow, not a newly
qualified operation in the published app.

1. Duplicate the photo layer with **Ctrl+J**. Select a visible, unlocked pixel
   layer; its parent folders must also permit editing. Keep retained RAW,
   16-bit and other editable sources, and rasterize a duplicate deliberately
   when pixel painting is needed.
2. Use **Ctrl+K → Blur brush tool**. Adjust **Size px**, **Hardness %** and
   **Opacity %** in the options bar to choose the painted area and strength.
3. Set **Radius px** independently of brush size. Its range is **0.25–128
   canvas pixels**, starting at **2 px**. A larger brush covers more area;
   a larger radius spreads the blur farther around each affected pixel.
4. Paint short strokes, release to apply, and inspect the result at actual
   pixels. **Ctrl+Z** reverses a completed stroke. Changing the radius setting
   alone does not alter the image.

The native-resolution work also covers **Smudge tool** and **Liquify tool**
(**Ctrl+Shift+X**). The target is to retain the original pixel dimensions when
retouching a scaled, rotated or flipped layer, rather than first reducing it to
the canvas resolution. This does not increase the source photograph's detail.

A pixel selection limits where results are written, including its soft edge.
Neighbouring colour outside the selection can still supply the blur, smudge or
warp; a selection is not an excluded sampling region. Blur preserves the
source alpha channel. Clone and Healing remain separate tools with their
existing [Alt-click sampling workflow](remove-objects.md#precise-repair-clone-or-healing).

For mask retouch, select the **MASK** badge normally after leaving inspection,
then use Blur, Smudge or Liquify. These retouch operations retain the existing
mask bitmap extent. To extend mask coverage, use ordinary
[mask painting or a gradient](photo-editing.md#extend-a-painted-mask).

The native path bounds work to source or mask surfaces up to **16 MP**, a
limited stroke length and a **256 MiB working-buffer budget**. A very large
brush, blur radius, transformed footprint or long drag can be refused; reduce
the size/radius or use shorter strokes. These bounds do not promise that any
operation will fit on every machine. Use short strokes and Undo after
application. Cancellation while a released stroke is computing is still under
integration; do not rely on Escape interrupting that work in the native UI.
