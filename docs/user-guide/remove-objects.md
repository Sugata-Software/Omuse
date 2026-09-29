# Remove unwanted objects

[User manual](README.md) · [Photo editing](photo-editing.md) · [Shortcuts](../keyboard-shortcuts.md)

Use a local tool for small repairs with usable nearby texture. Use **Ask Omuse →
Remove object** when the missing background needs a more substantial reconstruction.
Both need visual review: a clean-looking fill can still invent the wrong detail.

## Choose the right method

| Situation | Method | What it does |
| --- | --- | --- |
| Dust, a small blemish, a tiny distraction | **Spot healing — J** | Samples nearby pixels; no source click or network needed. |
| Repeating texture or a small selected object | **Content-aware fill** | Rebuilds the selection from surrounding pixels on the active layer. |
| You want to control where replacement texture comes from | **Controlled content-aware removal** | Uses an allowed sampling rectangle; previews a new editable result. |
| A straight edge, pattern or exact feature must continue | **Clone stamp — S**, or **Healing clone — Shift+J** | You choose a source with Alt-click, then paint. Healing blends local tone. |
| A larger distracting object needs a plausible new background | **Ask Omuse → Remove object** | Sends the canvas/mask to the selected capable AI route; review before Keep. |

Removing an object fills the hole. The Eraser/Delete controls clear pixels,
which is useful for transparency but does not reconstruct a background.

## Prepare a good selection

1. Save an editable project. Select the photo layer. For local pixel tools,
   duplicate it with **Ctrl+J** and work on the duplicate.
2. Use **L** for the Lasso or **M** for a Rectangle selection. Include the entire
   unwanted object and a small border of surrounding background.
3. Include its unwanted shadow or reflection too. Leaving these behind is a
   common reason a removal still looks wrong.
4. Keep nearby faces, type, logos and important edges outside the selection.
   Work on one object at a time when it gives you a cleaner boundary.
5. Inspect at **Ctrl+1**. A modest **Feather selection** (**Shift+F6**) can soften
   a seam, but excessive feathering can leave a ghost of the object.

For AI removal, locked artwork is additionally protected, including supported
clipped layers and groups. A locked photo containing the unwanted object can
therefore prevent the edit: use a working copy with only the elements you want
protected locked. Do not select the whole photograph unless you intend the
whole photograph to be editable.

## Small blemish: Spot Healing

1. Select the duplicated, unlocked pixel layer. Turn mask painting off.
2. Press **J**. Set a brush slightly wider than the blemish using **[** and **]**
   or the tool settings.
3. Begin with **Content aware** in the options bar. Click a spot or draw a short
   stroke over the distraction. Omuse applies the healing when the stroke ends.
4. Inspect the repair. **Ctrl+Z** restores that stroke. Retry with a smaller
   stroke or use an explicitly sampled source if nearby texture is unsuitable.

The options bar cycles **Content aware**, **Create texture** and **Proximity
match**. Try the other modes on a small area and compare; they are local pixel
algorithms, not AI requests. Prefer several short strokes to one enormous pass.

## Selected object: Content-aware Fill

1. Select the unwanted area on the duplicated pixel layer.
2. Press **Ctrl+K**, search **Content-aware fill**, then press **Enter**.
3. The operation applies immediately. Inspect it; **Ctrl+Z** restores the
   previous pixels in one step. Redraw the selection if a fragment remains.
4. Use **Ctrl+D** to deselect when finished, then save.

Pixels outside the selection are preserved; feathered boundaries blend with the
original. This command changes the active layer's pixels. It does not offer a
separate Preview/Keep stage, so keep your original layer underneath.

## Choose the sampling area: Controlled Removal

1. Make the target selection, then use **Ctrl+K → Controlled content-aware
   removal**. It is also available in the **Selection** inspector.
2. Set **Sampling rectangle X**, **Y**, **Sampling width** and **Sampling
   height** to a clean area of the source image. These values are source-image
   pixels, not screen coordinates. The rectangle must fit the image.
3. Start with **Search radius 32**, **Patch radius 2**, **Feather 0**. The default
   sampling rectangle covers the image; restrict it if another object is being
   copied into the repair. Selected target pixels are excluded automatically.
4. Preview the result. Increase the search radius only when suitable texture is
   nearby; a larger search costs more work and does not guarantee a better fill.
5. Choose **Apply** if satisfied, or cancel the draft. Apply creates a new
   **Removal · editable** result with the source and recipe retained. Undo
   restores the previous document.

This is deterministic patch replacement. It cannot reliably invent a missing
face, lettering or complex scene structure. Use Clone for precise edges or AI
for a reviewed generative reconstruction.

## Precise repair: Clone or Healing

1. Duplicate the photo layer. Press **S** for Clone stamp or **Shift+J** for
   Healing clone.
2. **Alt-click** a clean source area with matching texture or geometry.
3. Paint short strokes over the unwanted item. Sample again when the texture,
   lighting or perspective changes. Adjust size, hardness and opacity in the
   tool settings.
4. Undo a bad stroke immediately. Compare the result at actual pixels and fit
   view so you catch both seams and obvious repeated patterns.

For a separate retouch layer, open **Advanced retouch** from command search and
choose **New healing layer**. It creates a transparent layer and enables
all-layer source sampling. Alt-click a source before painting. The strokes are
undoable raster edits; they are not a live link that follows future source edits.

## Larger object: AI Removal

1. Select the unwanted object, not the subject you want to keep.
2. Open **Ask Omuse** with **Ctrl+Shift+J**. Choose **Remove object**.
3. In **Connections**, select an available **Images** route. Omuse shows whether
   the operation is verified locally, available for an explicit first try, or
   unavailable. Current public live evidence covers ChatGPT through Codex;
   other subscriptions need their own operation support.
4. Describe the result you need. For example:

   > Remove the bin and its shadow. Continue the pavement and the wall behind
   > it, matching the existing perspective and lighting. Add no new objects.

5. Check the request context and allowance label. Choose **Preview removal**
   (or the explicitly labelled first-use removal action). **Ctrl+Enter** submits
   the selected task too. Choosing a task or starter alone sends nothing.
6. Compare **Before/After**. Check the object's former outline, nearby edges,
   shadows, texture and lettering. The document remains unchanged during review.
7. Choose **Keep result** if it works. Keep is one undo step. Otherwise discard
   it or choose **Refine this result**, describe the remaining problem, and
   explicitly submit the follow-up. Each submitted request may use allowance.
8. Save the project and inspect a finished export.

If you change the selection before keeping a same-session result, Omuse rebuilds
its masked candidate. If the artwork or project changed, Keep is blocked; refine
against the current source. A failed or stopped request is not automatically
retried. Stop also clears unsent variations; already accepted provider work may
still consume allowance.

## Limits and quality checks

- Local **Content-aware fill** accepts layers up to 16,777,216 pixels and at most
  250,000 selected pixels per operation.
- **Controlled removal** has a 4,000,000-pixel source limit, search radius 1–64,
  patch radius 0–4, and a bounded work budget. Large work may be refused even
  within those dimensions. Use a smaller copy or smaller target.
- Direct pixel tools cannot paint onto a retained editable/RAW source. Preserve
  that source and deliberately rasterize a duplicate if you need pixel retouch.
- A broad feather blends original object pixels back in. Reduce it if you see a
  halo. A repeated pattern means the sample is unsuitable: choose another
  sampling rectangle or a new Clone source.
- AI removal preserves the protected mask through local compositing; it does
  not promise that the invented content is accurate. Review faces, hands,
  architecture and text particularly carefully.
- [The 0.2.1 image-editing record](../ai-image-editing-qualification.md) includes
  a live synthetic removal with exact protected-pixel, Keep, save/reopen and
  Undo/Redo checks. That establishes the integration and integrity of that
  example, not perfect removal on every photograph.

Finish by checking at actual pixels, at fit view and at the intended output
size. Save the editable project as well as the exported image.
