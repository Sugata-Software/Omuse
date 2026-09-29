# Create a post, carousel or short animation

[User manual](README.md) · [Photo editing](photo-editing.md) · [Detailed Create guide](../omuse-create-guide.md)

## Build a branded post

1. Open **Create** from the header or press **Ctrl+Alt+N**. Create contains
   **Pages**, **Design**, **Brand**, **Assets**, **Motion** and **Export**.
2. In **Brand**, choose a starter or set your colours, heading/body fonts and
   spacing. Use a small consistent set so the final content feels coherent.
3. In **Design**, choose an editable template that fits your content. Its text,
   shapes and image frames are native layers you can change.
4. Replace the placeholder copy and images. Keep one clear headline, a short
   supporting line and a readable call to action.
5. In **Assets**, import your image into the local library, then place it or
   replace a selected frame. Replacing a frame keeps its crop and layout.
6. Save the collection as a **`.omuse` package**. Keep the entire package.

Templates and guides are composition aids. Check the destination's current
size/cropping requirements before making the final export.

## Add text, a logo and a call to action

Press **T** and click or drag a text box. Click existing text to edit it;
**Alt-click** starts a new object. Use the text controls to set the font, size,
colour and alignment. **Move — V** and **Ctrl+T** position or size the active
layer. Arrow keys nudge it by one pixel; Shift+arrow nudges by ten.

Use **Ctrl+Shift+O** to import a logo into the current document. Keep logos and
text on separate layers. Lock finished brand elements to protect them from
accidental edits. Use Design's alignment, distribution, live-text fitting and
text-background controls for consistent spacing. Inspect at phone size: a
button-shaped graphic needs centred, readable type and sufficient padding.

For a badge, footer or CTA you will reuse, save the layer as a component. Insert
instances on other pages, give them text/visibility overrides where needed, and
use **Update all** when the shared definition changes.

## Turn it into a carousel

1. Use **Pages** to duplicate or add pages. Name and order them deliberately.
2. Use one idea per page: opening promise, detail/example, and a final action.
3. **Alt+PageUp** and **Alt+PageDown** move between pages. Check type, margins and
   image framing on every page; a longer heading may need a different fit.
4. In **Export**, use **Phone preview** and **Show content guide** to check
   legibility and composition.
5. Choose **Export content pack…** for ordered PNG/JPEG/WebP pages, a multi-page
   PDF, captions, alt text and a manifest. Inspect the completed pack.

Canvas edits and collection operations have separate histories. Ordinary Undo
uses the canvas history first, then the collection history. Collection changes
that conflict with later canvas edits ask you to undo those edits first.

## Use AI to help with content

Open **Ask Omuse** with **Ctrl+Shift+J**. Choose a task instead of putting every
request through a single generic chat:

- **Design & layout:** ask for editable text/shapes/layout or content pages.
- **Caption & alt text:** draft export copy and an image description; copy or
  Keep after checking it against the artwork.
- **Generate image:** describe the image, style and intended use; review it
  before insertion.
- **Remove object / Replace selection / New background:** make the appropriate
  selection first. See [object removal](remove-objects.md) and [backgrounds](photo-editing.md#cut-out-a-subject-or-change-the-background).

For example: “Create a three-page launch carousel: introduce the product, explain
three benefits, and end with a concise call to action. Use our current brand
colours and keep the text editable.” Review the proposed operations and every
page before keeping them. AI can misread text, invent facts or choose a poor
layout. Keep your own product facts and final wording authoritative.

Choosing a task or a starting point does not send anything. Read the context
card, then explicitly submit. [Ask Omuse](../ai-experience.md) explains provider
connections, allowance, Before/After, history and refinement.

## Make a short animation

1. In **Motion**, set **Page seconds** and **Frames / second**.
2. Select a layer and try **Fade**, **Fade out**, **Rise** or **Slow pan**. Scrub
   the page to inspect it, then **Play project** to check page transitions.
3. Use **Return to editable canvas** to resume normal editing. Choose crossfade
   or a directional Slide transition where it helps the sequence.
4. For MP4 audio, choose an audio file you have permission to use and set its
   offset, source start, length and volume.
5. Import SRT/WebVTT subtitles or edit the cues, then choose a soft subtitle
   track or burned-in text as required.
6. Export **MP4…** or **GIF…**. Open the completed file to verify timing, audio
   and subtitles; native/GIF previews do not prove the MP4 soundtrack is right.

## Make several approved variations

Use **CSV BULK PREVIEW** only after your template is ready. Plain fields fill
text; `image:field`, `visible:field` and `alt:field` bind packaged resources,
visibility and image descriptions. Preview representative rows, including the
longest text, before **Create all valid**. CSV bindings do not fetch arbitrary
image paths or URLs.

## Final check

Save the editable collection. Check spelling, brand colours, image rights,
contrast, text alignment, safe margins and every page at its output size.
Inspect the exported files, including audio/subtitles for video. Omuse creates
content; it does not require a calendar or publishing schedule.
