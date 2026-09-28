# Creating content in Omuse

Open **Create** from the header, or press **Ctrl+Shift+E**. A Create collection keeps its pages, brand kits, reusable components and packaged resources together in a `.omuse` package. Keep the whole package when moving work between machines. Existing `.comp` projects still open.

This guide describes the implemented local preview. Its final candidate has local evidence for the 636-test Rust suite, six-page Create recovery, all 80 template variants, media artifacts, native/XWayland journeys and one bounded Codex subscription journey. The installed native and desktop-launch checks have also passed. See the [qualification record](omuse-create-qualification.md) for the exact candidate and remaining public-release gates.

Create is organised into **Pages**, **Design**, **Brand**, **Assets**, **Motion** and **Export**.

## Build a branded set

1. In **Brand**, choose the Sugata starter or define your own colour roles, heading and body fonts, and spacing tokens.
2. In **Design**, choose one of the 20 editable templates. Their text, shapes and image frames remain native layers.
3. Use **Pages** to add, duplicate, name and reorder pages. The page strip keeps a collection in reading order; **Alt+PageUp** and **Alt+PageDown** move between pages.
4. Edit text and layout directly on the canvas. **Design** also contains live-text fitting, alignment, distribution, text backgrounds and page resizing controls.
5. Save a repeated layer as a component when a logo, badge, footer or call to action should recur. Insert it on another page, give an instance its own text or visibility override, then use **Update all** when the shared definition changes.
6. Use **CSV BULK PREVIEW** for approved template fields. Plain fields supply text; `image:field`, `visible:field` and `alt:field` bind packaged project resources, visibility and image descriptions. Validate and preview a row before choosing **Create all valid**. CSV bindings never load image paths or URLs from the file.

The canvas has its own undo history. Collection operations have a separate undo/redo history; the ordinary Undo and Redo commands use the canvas history first, then the collection history. A structure undo preserves later canvas edits on pages that still exist; a collection-wide content undo asks you to undo newer canvas changes first.

## Work with assets and restoration

**Assets** contains the local library and **PHOTO RESTORATION · ON THIS COMPUTER**.

Import images into the library, then search, tag, favourite, place or replace a selected frame. Replacing a frame keeps its crop and layout. The library records provenance and packages the selected source with the project when it is placed.

For an image already on the page, use restoration controls to set **Denoise** and **Sharpen**, choose **1× pixels**, **2× pixels** or **4× pixels**, and select **Preview restoration**. Compare the draft with the source before **Keep restoration**. The original remains in the project as a hidden layer, so one undo restores the prior document. Enlargement interpolates pixels; it does not claim to reconstruct detail that was not present.

Choose **Place and remove background…** on a library asset to place it and open the local subject workflow. Refinement, healing and effects remain editable workflows in the photo editor.

## Animate in Motion

Use the dedicated **Motion** tab to animate pages and layers, then export locally with FFmpeg.

Set **Page seconds** and **Frames / second**, select a layer, then apply **Fade**, **Fade out**, **Rise** or **Slow pan**. The page scrubbing controls inspect its animation; **Play project** plays the collection with page transitions. Use **Return to editable canvas** to leave preview mode. A page can crossfade to the next page or use a directional **Slide** transition. Native playback and GIF previews show visual animation; verify audio and subtitles in an exported MP4.

For an MP4, choose an audio file and set its offset, source start, length and volume. You can import an SRT or WebVTT subtitle file, edit each cue’s start, end and text, and choose a position, font size and outline. Export subtitles as a selectable soft track or burn the styled text into the video. The editable cues remain part of the collection.

For a short supplied clip, choose the file, use **Probe** to read its duration, set **Trim start ms** and **Trim end ms**, then prepare a trimmed copy. Set **Split at ms** and choose **Prepare two clips…** to make two safe output clips. These operations create new output files and leave the source clip alone.

Use **GIF…** or **MP4…** for motion output. **Export visual preview GIF…** also asks where to save its result. Cancel stops Omuse’s current export work; retain the source project and inspect a completed file before replacing anything in a wider workflow.

## Check and export

In **Export**, add a **Caption** and **Alt text**, then choose **Apply caption & alt text**. **Phone preview** and **Show content guide** help assess readable type, contrast and framing at a small size. The guides are conservative composition aids; destination controls vary by app, device, caption and placement.

Choose **Export content pack…** to publish ordered pages together as PNG, JPEG and WebP, with a multi-page PDF, captions, alt text and a manifest. Export runs in the background and only publishes the package once it finishes. If you edit during a save, the completed snapshot is saved while your later edits remain marked unsaved.

## Ask Omuse

Open **Ask Omuse** with **Ctrl+Shift+J**. **Connections** shows installed provider runtimes and their state, including sign-in and capability checks. Choose the **Assistant** and **Images** routes separately. Omuse does not change providers or activate separately billed API access on a failed request.

Connections distinguishes three states. **Verified** means that exact provider/runtime/capability has a local receipt. **Not yet tested** offers a matching **Try ...** action when the selected route can attempt the capability; submitting it uses that provider's allowance under the displayed billing label. **Unavailable** means the route cannot provide the operation. Local-only separately blocks remote submission. Omuse must not silently substitute another provider.

The eligible first-use actions are **Try image generation**, **Try replacement**, **Try background edit**, **Try canvas expansion**, **Try removal**, **Try caption draft** and **Try design assistant**. A successful bounded request stores a private qualification receipt for that provider, runtime version, authentication route and capability; it stores no prompt, artwork, account identifier or credential. Changing runtime version or route requires qualification again. The qualification record covers a retained real Generate result, a new Background request and a new native assistant carousel through the Codex subscription route; other providers and operations remain unavailable or explicit **Try** paths until their own evidence exists. A receipt is local runtime evidence, not public provider support.

The composer shows the actual route and billing label separately for **Assistant** and **Images**, including **Uses your subscription allowance** and **Remaining allowance unavailable** when the runtime cannot report a balance. Omuse never switches a request to a separately billed API route automatically.

**Local-only · on** is saved with the collection. It stops new remote AI submission, requests cancellation of active remote work and skips unsent variations while keeping local editing, saved drafts and export available. Provider work already accepted may continue. Turning Local-only off does not bypass provider verification or the first-operation check.

When a qualified image route is available, the panel offers **Generate image**, **Replace selection**, **Background**, **Expand canvas** and **Remove selection**. Select an area before Replace or Remove; for Background, select the subject to preserve. **Expand canvas** accepts independent left, top, right and bottom margins from 0 to 4096 pixels, subject to the document bounds. Add only the reference images you intend to send. Layout requests share editable layer details; image edits share a staged canvas image and the current selection.

For a protected-subject background result, open the collapsed **Product finishing** controls to add an optional local **Soft** or **Contact** shadow and a **Subtle** reflection. **Apply local finishing** and **Remove local finishing** are undoable native edits. These choices are not sent to the provider.

Use **Design with me** for a proposed native edit plan, or **Improve layout** for a focused composition request. A reviewed plan can select an existing page, place an image already packaged as a project resource and insert a reusable component with bounded text or visibility overrides. It cannot introduce an arbitrary file path or URL. Image results stay in a review group until you keep one. You can compare 1, 2 or 4 requested variations, refine a result, choose **Another direction**, add a kept image to the canvas, or save it to the local library with its provenance. A result made against an older document is retained for review rather than silently applied to newer artwork.

Use **Draft caption & alt text** when the selected provider can help prepare export copy from the current canvas. Review the actual proposed wording before **Keep**. Use the **Export** fields to revise it afterwards, or refine the assistant draft before keeping it.

Provider sign-in remains with the provider’s official client. Omuse does not put credentials in artwork. Requests can consume the selected provider’s allowance or credits according to that account; cancellation may not reverse already-used allowance. Ordinary local editing, restoration, saving and export remain usable without an AI connection.

## Release qualification

This guide describes the implemented local preview; it does not certify a public release. The candidate's local evidence includes the 636-test Rust suite, a six-page branded Create recovery qualification with six save/reopen revisions and staged-save SIGKILL snapshots, all 80 native template variants with save/reopen/pixel checks, Create/media/native journeys, actual 800×600 XWayland and strict-splash checks, plus the bounded Codex subscription work described above. The isolated `omuse-preview` launcher passed installed self-test and native editing checks from outside the checkout. Normal and desktop-entry launches opened the copied sample projects in `~/Pictures/Omuse Preview`.

The [qualification record](omuse-create-qualification.md) is the source of current gate status. Before public distribution, complete physical keyboard/pointer/tablet and multi-DPI checks, clean-container/CI qualification, the four licence-text findings, remaining manual visual/accessibility and destination review, and qualification of each advertised provider/runtime/capability. First-use receipts remain local runtime evidence, not a public release certificate. Unsupported routes must be **Unavailable**; implemented but unevidenced routes may be explicit **Try** actions only when the selected provider can attempt them and the user has accepted the displayed billing/data route.
