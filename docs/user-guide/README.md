# Omuse user manual

**Make, retouch and finish images on Linux.** This manual describes Omuse 0.3.0.
Start with a photo, keep an editable project, and export a copy when it is ready.

[Install Omuse](../install.md) · [Remove objects](remove-objects.md) ·
[Edit a photo](photo-editing.md) · [Create social content](create-content.md) ·
[Ask Omuse](../ai-experience.md) · [All shortcuts](../keyboard-shortcuts.md)

## Your first edit

1. Launch **Omuse**. Press **Ctrl+O** and open a photo.
2. Press **Ctrl+Shift+S** and save an Omuse project in your work folder. Saving a
   project preserves editable layers; exporting produces a finished image.
3. Select the photo layer in **Layers**. Press **Ctrl+J** to duplicate it before
   using a tool that changes pixels. Keep the original underneath.
4. Press **Ctrl+0** to fit the canvas. Press **Ctrl+1** to inspect detail at
   actual pixels. Hold **Space** and drag to pan.
5. Make your edit. **Ctrl+Z** undoes it; **Ctrl+Shift+Z** redoes it.
6. Save with **Ctrl+S**, then export with **Ctrl+Alt+Shift+S**. Open the exported
   file and check the result before sharing it.

If the cursor is inside a text field, keys edit that field. Finish or leave the
field before using canvas shortcuts. **Fit** and **Undo** are also on-screen
controls. Omarchy's system shortcuts and customised Omuse bindings can differ
from the defaults shown here.

## Find a tool without hunting through menus

Press **Ctrl+K**, type a command such as **Content-aware fill**, and press
**Enter** to run the highlighted result. This includes commands without a
keyboard shortcut. **Ctrl+Alt+K** opens the shortcut editor.

The left dock selects tools. The options bar changes with the selected tool.
The right inspector contains **Layers**, **Develop**, **Selection** and the
assistant. **F7** hides or shows the inspector. **Create** opens the collection
workspace for pages, templates, brand assets and content exports.

## Choose a workflow

| What you want to do | Start here |
| --- | --- |
| Remove a blemish, wire, distracting item or person | [Object removal: local and AI](remove-objects.md) |
| Copy editable groups or layers into another document | [Layer clipboard](photo-editing.md#copy-editable-artwork-between-documents) |
| Crop, resize, improve tone or sharpen | [Photo editing](photo-editing.md) |
| Cut out a product or replace its background | [Backgrounds and cutouts](photo-editing.md#cut-out-a-subject-or-change-the-background) |
| Make a branded post, carousel or small campaign | [Create social content](create-content.md) |
| Add editable type, a logo or a call to action | [Text and layout](create-content.md#add-text-a-logo-and-a-call-to-action) |
| Generate an image or ask for a reviewed edit | [Ask Omuse](../ai-experience.md) |
| Animate pages, add audio/subtitles and export an MP4 | [Motion](create-content.md#make-a-short-animation) |
| Work with RAW, 16-bit sources, filter stacks or masks | [Advanced workflows](../rust-advanced-workflows.md) |

## Save projects; export deliverables

A Create collection is a **`.omuse` package** containing pages and resources.
Keep the whole package when copying or backing it up. Existing **`.comp`**
projects also open. An exported PNG/JPEG does not replace that editable project.
Use **Save as** for an independent working copy.

Use PNG for transparency and sharp graphics; JPEG for opaque photographs;
WebP when your destination accepts it. Create's **Export content pack…** can
produce ordered pages, a multi-page PDF, captions, alt text and a manifest.
Keep a project copy even after export. Recovery is a fallback, not a backup.

## AI is optional

Local editing, healing, selection, saving and export work without an AI
subscription. **Ask Omuse → Connections** shows the installed official runtimes,
separate Assistant and Images routes, and their available operations. Provider
sign-in stays with that provider. A subscription request may use its allowance;
Omuse does not silently switch it to a separately billed API.

For ChatGPT via Codex, **Signed in** means the connection is ready. An
**Assistant · not tested** or **Images · not tested** label marks an operation
awaiting its first explicit request; no separate login or test button is needed.
To try image generation, choose **Generate image**, write the brief, select
**Generate image**, review the proposal, then select **Keep result**.

The locally qualified corrective build also lets you choose **Claude Code** as
**Assistant** in Connections for **Design & layout**. Keep **ChatGPT via Codex**
selected for **Images**. For **Enhance photo** and **Caption & alt text**, choose
Codex as Assistant too, because those tasks currently include the canvas image.
One bounded Claude Design journey passed Review, Keep, Undo and Redo, but this
path is not part of the published 0.3.0 build or installer pin.

**Unverified** means Omuse could not prove the discovered runtime's identity.
**Sign in needed** means its identity passed but its provider account is not
available. Sign in through the official provider runtime if needed, then select
**Refresh** in Connections. Omuse 0.3.0 has a known Omarchy issue where
mise-managed Codex or Claude shims can appear as **Unverified** even when the
provider is signed in. A detection fix is in development and is not released.
The corrected development discovery now recognizes an existing Claude Code
subscription login without copying a password or API key. Omuse does not fall
back to a separately billed Claude API. See the
[bounded Claude qualification](../ai-claude-qualification.md) for the exact
local candidate and remaining limits.

Read the request-context card before submitting: image edits share a canvas
image and edit mask. **Local-only** stops new remote requests for the collection.
See [AI connection and review instructions](../ai-experience.md). Availability
is specific to the provider, runtime and operation; another service's chat
subscription does not automatically provide image editing.

## When something seems wrong

| Symptom | What to check |
| --- | --- |
| Brush or removal does nothing | Select an unlocked pixel layer, check its parent group lock, and turn mask painting off if you meant to edit pixels. An editable/RAW source may need a deliberately rasterized duplicate. |
| Only a tiny part changes | A selection may still be active. Use **Ctrl+D** when the next operation should affect the whole layer. |
| A shortcut types into a field | Leave the field, then use the command; or click the visible toolbar control. |
| AI Preview is unavailable | Choose an Images route, review Connections, turn Local-only off only if you intend to send, and make the selection required by that task. |
| A saved AI result cannot be kept | Its source artwork or session changed. Review it, then use **Refine this result** for the current source. Omuse does not overwrite newer work with a stale result. |
| A local operation reports a size/work limit | Work on a smaller copy, a smaller selection or smaller strokes. [Removal limits](remove-objects.md#limits-and-quality-checks) explain the relevant ceilings. |
| A preview looks different from export | Check layer visibility, masks, output format and colour/precision settings. Inspect the exported file at actual pixels. [Advanced colour limits](../rust-advanced-workflows.md#3-precision-and-colour) apply. |
| An update misbehaves | Save your work, use the [rollback instructions](../install.md#update-rollback-and-remove), then reopen Omuse. |

For a reproducible problem, [file a bug](https://github.com/Sugata-Software/Omuse/issues/new/choose)
with the Omuse version, Linux/session details, exact steps and expected result.
Use a non-sensitive example file. The [project dashboard source](../project-guide.md)
and [release notes](../releases/README.md) distinguish supported workflows from
work still being qualified.
