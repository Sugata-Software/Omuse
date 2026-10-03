# Omuse keyboard shortcuts

This reference is generated from Omuse's command catalog. Press **Ctrl+K** in the editor to search and run any of the 200 commands. 106 commands have a default shortcut; every unbound command remains searchable and executable.

Open **Keyboard shortcuts** with **Ctrl+Alt+K** to assign, clear, or reset a binding. Custom bindings are stored per user. Omuse reserves **Super** for Omarchy and other Linux desktop shortcuts, and rejects Linux virtual-terminal chords such as Ctrl+Alt+F3.

The command palette has **All**, **Bound**, **Unbound**, and **Gestures** views. Type to filter by command, category, ID, alias, or the current custom shortcut; use Up/Down or Page Up/Page Down to move, Enter to run, and Escape to close. Ctrl+K remains available while typing in sidebar fields; other canvas shortcuts stay suppressed there.

Choose **Edit shortcut** from command search, or open the shortcut editor directly, then use **Record**, **Clear**, or **Default**. **Apply** validates and saves the complete map atomically; **Cancel** discards the draft. Omuse keeps explicit custom and unbound choices when later releases add defaults.

Dialogs reserve Escape, Enter, Tab, and Space. Inline text uses **Ctrl+Enter** to commit, **Escape** to cancel, and Enter for a newline. Save and Save As honour your current bindings and commit the text draft first. Search and Save bindings require Ctrl, Alt or a function key to preserve text navigation.

**Ask Omuse** focuses its prompt when opened from the toolbar, shortcut or command search. In that prompt, **Ctrl+Enter** sends one assistant request; **Enter** inserts a newline. Submission keeps the same connection, local-only and busy checks as the assistant button. Canvas shortcuts remain inactive while typing.

Shifted punctuation is shown using US-layout key names: Shift+[ produces {, Shift+] produces }, and Ctrl+Shift+; produces Ctrl+:. Linux binds the resulting symbols, including Ctrl++ for zoom. On another layout, use Record to choose comfortable keys.

The familiar single-key tools and several editing chords are inspired by Adobe's [Photoshop shortcut guidance](https://helpx.adobe.com/photoshop/desktop/get-started/settings-and-preferences/view-keyboard-shortcuts.html) and [printable shortcut reference](https://helpx.adobe.com/content/dam/help/en/photoshop/using/default-keyboard-shortcuts/photoshop-keyboard-shortcuts.pdf), adapted for native Linux and Omuse's actual commands.

Legacy default changes: Export moved from Ctrl+E to **Ctrl+Alt+Shift+S**, Create workspace moved to **Ctrl+Alt+N**, and Ctrl+E now means Merge down. Tool defaults now include **H** Hand, **J** Spot healing, **P** Vector path workspace, and **Shift+B** Pencil.

## Application

| Command | Default | What it does |
|---|---:|---|
| Search commands | Ctrl+K | Search and run every command, including unbound commands. |
| Keyboard shortcuts | Ctrl+Alt+K | Open the shortcut editor. |
| Ask Omuse | Ctrl+Shift+J | Open the in-app assistant. |
| Create workspace | Ctrl+Alt+N | Open the Create workspace. |
| Toggle inspector panel | F7 | Show or hide the inspector panel. |
| Close | Ctrl+Q | Close Omuse, prompting when work is unsaved. |

## File

| Command | Default | What it does |
|---|---:|---|
| New canvas | Ctrl+N | Create a new canvas. |
| Open | Ctrl+O | Open an image or Omuse project. |
| Open recent project | Ctrl+Alt+O | Search recently opened or saved projects; Enter opens the selection. |
| Save | Ctrl+S | Save to the current project location. |
| Save as | Ctrl+Shift+S | Save the project to a new location. |
| Import | Ctrl+Shift+O | Import an image into the current document. |
| Export | Ctrl+Alt+Shift+S | Export the finished artwork. |
| Import conversion report | Unbound | Show conversions made while importing the current document. |
| Previous page | Alt+Page Up | Activate the previous page in a Create project. |
| Next page | Alt+Page Down | Activate the next page in a Create project. |

## Edit

| Command | Default | What it does |
|---|---:|---|
| Undo | Ctrl+Z | Undo the most recent edit. |
| Redo | Ctrl+Shift+Z | Redo the most recently undone edit. |
| Copy | Ctrl+C | Copy selected pixels, or editable layer trees when no pixel selection is active. Other apps receive PNG. |
| Copy merged | Ctrl+Shift+C | Copy the visible composite inside the selection. |
| Cut | Ctrl+X | Cut selected pixels or layers. |
| Paste | Ctrl+V | Paste editable layers from this Omuse session, or a new image layer from an external clipboard. |
| Delete selection or layer | Backspace | Clear selected pixels, remove a mask, or delete selected layers. |
| Delete selection or layer (forward Delete) | Delete | Use the forward Delete key to clear selected pixels or delete selected layers. |

## Selection

| Command | Default | What it does |
|---|---:|---|
| Select all | Ctrl+A | Select the full canvas. |
| Deselect | Ctrl+D | Clear the current pixel selection. |
| Invert selection | Ctrl+Shift+I | Select pixels outside the current selection. |
| Transform selection | Unbound | Float selected pixels for independent transformation. |
| Commit floating selection | Unbound | Commit a floating pixel selection. |
| Cancel floating selection | Unbound | Cancel a floating selection and restore its source. |
| Select subject | Unbound | Find a foreground subject locally and refine its selection. |
| Remove background | Unbound | Find a foreground subject locally and remove its background. |
| Luminosity range | Unbound | Create a selection from a tonal range. |
| Colour range | Unbound | Select colours by RGB distance or hue, with softness and grey protection. |
| Feather selection | Shift+F6 | Soften the edge of the current selection. |
| Expand selection | Unbound | Expand the current selection by a chosen radius. |
| Contract selection | Unbound | Contract the current selection by a chosen radius. |
| Content-aware fill | Unbound | Fill selected pixels from surrounding image content. |

## Layer

| Command | Default | What it does |
|---|---:|---|
| New layer | Ctrl+Shift+N | Add an empty paint layer. |
| Duplicate selected layers | Ctrl+J | Duplicate the selected layer roots. |
| Delete selected layers | Unbound | Delete the selected layer roots. |
| Group selected layers | Ctrl+G | Put the selected layers in a group. |
| Merge down | Ctrl+E | Merge the active layer into the layer below. |
| Flatten image | Unbound | Flatten the document's visible artwork. |
| Rename layer | Unbound | Rename the active layer. |
| Toggle layer visibility | Unbound | Show or hide the active layer. |
| Toggle layer lock | Unbound | Lock or unlock the active layer. |
| Toggle clipping mask | Ctrl+Alt+G | Clip the active layer to the pixel layer below. |
| Cycle blend mode | Unbound | Advance the active layer to the next blend mode. |
| Move layer up | Unbound | Move selected sibling layers one step up. |
| Move layer down | Unbound | Move selected sibling layers one step down. |
| Move layers into group | Unbound | Choose a group for the selected layers. |
| Move layers out of group | Unbound | Move selected layers to the document root. |
| Select layer below | Alt+[ | Select the adjacent layer below. |
| Select layer above | Alt+] | Select the adjacent layer above. |
| Reorder layer down | Ctrl+[ | Move selected layers one step down. |
| Reorder layer up | Ctrl+] | Move selected layers one step up. |

## Mask

| Command | Default | What it does |
|---|---:|---|
| Add layer mask | Unbound | Add a revealing mask to the active layer. |
| Delete layer mask | Unbound | Remove the active layer's mask without applying it. |
| Apply layer mask | Unbound | Bake the active layer's mask into its pixels. |
| Invert layer mask | Unbound | Invert the active layer's mask. |
| Toggle layer mask | Unbound | Enable or disable the active layer's mask. |
| Toggle mask painting | Unbound | Switch brush editing between layer pixels and the layer mask. |
| Toggle mask link | Unbound | Link or unlink mask and layer placement. |
| Place mask | Unbound | Edit the active mask's placement. |
| Live mask source | Unbound | Choose a live source for the active mask. |
| Vector mask | Unbound | Open the vector-mask workspace. |

## Object

| Command | Default | What it does |
|---|---:|---|
| Transform layer | Ctrl+T | Transform the active layer numerically. |
| Distort corners | Unbound | Move the active layer's four corners. |
| Cycle sampling quality | Unbound | Cycle transform resampling quality. |
| Edit text, shape or vector artwork | Unbound | Edit the selected live text, shape or vector artwork. |
| Create text object | Unbound | Create editable text with detailed settings. |
| Rasterize object | Unbound | Convert an editable object to pixels; Undo restores it. |
| Image trace / retrace | Unbound | Convert an image into editable vector artwork on the main canvas. Compare Source/Trace, adjust detail and point limits, then Keep vectors. The original image is retained. |
| Vector pen tool | P | Click to place corners; drag to draw smooth curves. Click the first anchor to close. Click a segment to add a point. A edits nodes; V moves objects. |
| Vector artwork on canvas | Shift+P | Draw and edit objects on the main canvas with the shared layer stack. Enter keeps edits; Escape discards them. |
| Vector node tool | A | Drag anchors or handles on the main canvas. Double-click a segment to add a point; Alt frees a handle, Shift constrains to 45 degrees. Delete removes points. |
| Rotate layer 90 degrees | Unbound | Rotate the active layer clockwise by 90 degrees. |
| Flip layer horizontally | Unbound | Flip the active layer horizontally. |
| Nudge left | Left | Move selected layers left by one pixel. |
| Nudge right | Right | Move selected layers right by one pixel. |
| Nudge up | Up | Move selected layers up by one pixel. |
| Nudge down | Down | Move selected layers down by one pixel. |
| Nudge left 10 pixels | Shift+Left | Move selected layers left by ten pixels. |
| Nudge right 10 pixels | Shift+Right | Move selected layers right by ten pixels. |
| Nudge up 10 pixels | Shift+Up | Move selected layers up by ten pixels. |
| Nudge down 10 pixels | Shift+Down | Move selected layers down by ten pixels. |

## Vector

| Command | Default | What it does |
|---|---:|---|
| Group vector selection | Unbound | Group selected vector objects. Ctrl+G also groups objects while editing artwork. |
| Ungroup vector selection | Ctrl+Shift+G | Release selected outer vector groups while retaining object order and geometry. |
| Toggle vector outline view | Ctrl+Y | Inspect vector contours at the current zoom. This viewing mode does not change exports. |
| Unite vector shapes | Unbound | Combine selected filled shapes; the bottom shape supplies the style. Undo restores the originals. |
| Subtract vector shapes | Unbound | Cut upper selected filled shapes out of the bottom selected shape. |
| Intersect vector shapes | Unbound | Keep the shared filled area of selected shapes. |
| Exclude vector overlap | Unbound | Keep areas covered by an odd number of selected shapes. |
| Divide vector shape | Unbound | Partition the bottom selected shape using the upper selected shapes. |
| Align vectors left | Unbound | Align selected objects or groups to the left edge of the selection. |
| Centre vectors horizontally | Unbound | Align selected objects or groups to their shared horizontal centre. |
| Align vectors right | Unbound | Align selected objects or groups to the right edge of the selection. |
| Align vectors top | Unbound | Align selected objects or groups to the top of the selection. |
| Centre vectors vertically | Unbound | Align selected objects or groups to their shared vertical centre. |
| Align vectors bottom | Unbound | Align selected objects or groups to the bottom of the selection. |
| Space vectors horizontally | Unbound | Distribute three or more objects or groups with equal horizontal gaps. |
| Space vectors vertically | Unbound | Distribute three or more objects or groups with equal vertical gaps. |
| Select vectors with same fill | Unbound | Select visible vector objects matching the active object's fill, including alpha. |
| Select vectors with same stroke | Unbound | Select visible vector objects matching the active object's stroke colour and width. |
| Select vectors with same opacity | Unbound | Select visible vector objects matching the active object's opacity. |
| Import editable SVG artwork | Unbound | Add supported solid-paint SVG objects and groups to the current artwork without replacing existing objects. |
| Export editable SVG artwork | Unbound | Export the complete vector artwork with supported groups, solid paint and opacity. |

## Image

| Command | Default | What it does |
|---|---:|---|
| Resize image | Ctrl+Alt+I | Resize the image and all of its content. |
| Resize canvas | Ctrl+Alt+C | Change the canvas bounds without scaling content. |
| Crop canvas | C | Preview a movable crop with photo and social ratios. Enter applies; Escape cancels. Outside pixels are retained. |
| Trim canvas | Unbound | Trim canvas edges using chosen criteria. |
| Invert pixels | Ctrl+I | Invert selected pixels or the active mask. |
| Convert to grayscale | Ctrl+Shift+U | Apply a grayscale pixel adjustment; Undo restores the previous pixels. |
| Blur pixels | Unbound | Apply a small blur pixel adjustment. |
| Sharpen pixels | Unbound | Apply a small sharpen pixel adjustment. |
| Increase brightness | Unbound | Increase pixel brightness. |
| Decrease brightness | Unbound | Decrease pixel brightness. |
| Increase contrast | Unbound | Increase pixel contrast. |
| Increase saturation | Unbound | Increase pixel saturation. |
| Pixel filters | Unbound | Open the destructive pixel-filter gallery. |
| Levels | Ctrl+L | Open Levels settings; Apply changes pixels and Undo restores them. |
| Curves | Ctrl+M | Open Curves settings; Apply changes pixels and Undo restores them. |
| Hue and saturation | Ctrl+U | Open Hue and Saturation settings; Apply changes pixels and Undo restores them. |
| Colour balance | Ctrl+B | Open Colour Balance settings; Apply changes pixels and Undo restores them. |
| Camera Raw | Ctrl+Shift+A | Open nondestructive Camera Raw controls for a pixel layer. |

## View

| Command | Default | What it does |
|---|---:|---|
| Zoom in | Ctrl+= | Use the next zoom stop while keeping the viewport centre fixed on the artwork. |
| Zoom in (+) | Ctrl++ | Increase canvas magnification with the Plus key. |
| Zoom out | Ctrl+- | Decrease canvas magnification. |
| Fit canvas | Ctrl+0 | Fit the full canvas in the viewport. |
| Actual pixels | Ctrl+1 | Show one image pixel per logical pixel. |
| Toggle grid | Ctrl+' | Show or hide the canvas grid. |
| Toggle guides | Ctrl+; | Show or hide guides. |
| Toggle rulers | Ctrl+R | Show or hide canvas rulers. |
| Toggle snapping | Ctrl+Shift+; | Enable or disable snapping to the grid and guides. |
| Toggle auto-select | Unbound | Choose layers from the canvas when using Move. |
| Toggle transform box | Unbound | Show or hide Move-tool transform handles. |
| Manage guides | Unbound | Add, clear, or configure guides. |

## Tools

| Command | Default | What it does |
|---|---:|---|
| Brush tool | B | Paint freehand strokes. |
| Pencil tool | Shift+B | Paint hard-edged freehand strokes. |
| Eraser tool | E | Erase pixels with a brush. |
| Fill tool | Shift+G | Fill a connected colour region. |
| Gradient tool | G | Draw a configurable colour gradient. |
| Rectangle selection tool | M | Draw rectangular pixel selections. |
| Ellipse selection tool | Shift+M | Draw elliptical pixel selections. |
| Lasso tool | L | Draw freehand pixel selections. |
| Wand tool | W | Select connected pixels of similar colour. |
| Connected subject tool | Unbound | Select the connected foreground subject under the pointer. |
| Move tool | V | Move and transform layers on the canvas. |
| Hand tool | H | Pan the canvas; holding Space temporarily activates panning. |
| Eyedropper tool | I | Sample a colour from the canvas. |
| Clone stamp tool | S | Paint from a sampled source; Alt-click chooses the source. |
| Spot healing tool | J | Heal small areas from nearby pixels. |
| Healing clone tool | Shift+J | Heal using an explicitly sampled source. |
| Blur brush tool | Unbound | Paint localized blur. |
| Smudge tool | Unbound | Smear pixels with a brush. |
| Liquify tool | Ctrl+Shift+X | Warp pixels with a brush. |
| Text tool | T | Create or edit text directly on the canvas. |
| Rectangle shape tool | U | Draw editable rectangle shapes. |
| Ellipse shape tool | Shift+U | Draw editable ellipse shapes. |
| Line tool | Unbound | Draw editable line shapes. |
| Tool settings | Unbound | Open detailed settings for the active tool. |
| Decrease brush size | [ | Decrease the active brush diameter. |
| Increase brush size | ] | Increase the active brush diameter. |
| Decrease brush hardness | Shift+[ | Make the active brush edge softer. |
| Increase brush hardness | Shift+] | Make the active brush edge harder. |
| Set brush opacity to 10% | 1 | Set brush opacity to 10 percent. |
| Set brush opacity to 20% | 2 | Set brush opacity to 20 percent. |
| Set brush opacity to 30% | 3 | Set brush opacity to 30 percent. |
| Set brush opacity to 40% | 4 | Set brush opacity to 40 percent. |
| Set brush opacity to 50% | 5 | Set brush opacity to 50 percent. |
| Set brush opacity to 60% | 6 | Set brush opacity to 60 percent. |
| Set brush opacity to 70% | 7 | Set brush opacity to 70 percent. |
| Set brush opacity to 80% | 8 | Set brush opacity to 80 percent. |
| Set brush opacity to 90% | 9 | Set brush opacity to 90 percent. |
| Set brush opacity to 100% | 0 | Set brush opacity to 100 percent. |
| Fill with foreground colour | Alt+Backspace | Fill the selection with the foreground colour. |
| Fill with background colour | Ctrl+Backspace | Fill the selection with the background colour. |
| Default colours | D | Reset foreground and background to black and white. |
| Swap colours | X | Exchange the foreground and background colours. |

## Adjustments

| Command | Default | What it does |
|---|---:|---|
| Add adjustment layer | Unbound | Add a nondestructive adjustment layer. |
| Edit adjustment | Unbound | Edit the selected adjustment layer. |
| Layer effects | Unbound | Edit nondestructive effects on the active layer. |
| Delete layer effects | Unbound | Remove every effect from the active layer. |
| Editable filter stack | Unbound | Build a reorderable stack, including reference-photo colour matching and colour uniformity. |
| Dither and halftone | Unbound | Preview ten retro finishes with pixel, cell and palette controls; Apply is one undo step. |
| Bloom into transparency | Unbound | Preview highlight glow that can spread into existing transparent layer margins. |
| Vignette overlay | Unbound | Preview a coloured edge overlay, including on an empty paint layer. |
| Local tonal contrast | Unbound | Preview radius-based detail contrast with independent tonal-zone strength. |
| Blend If | Unbound | Control layer visibility using tonal ranges. |
| Precision and colour | Unbound | Open precision and colour-management controls. |

## Workspaces

| Command | Default | What it does |
|---|---:|---|
| Frequency and tonal retouch | Unbound | Open the advanced retouch workspace. |
| Controlled content-aware removal | Unbound | Open the controlled removal workspace. |
| Editable mesh and pin warp | Unbound | Open the nondestructive warp workspace. |
| Selection refinement workspace | Unbound | Open advanced selection refinement. |
| Brush studio | Unbound | Open advanced brush design controls. |
| Smart source | Unbound | Open the editable smart-source workspace. |
| Develop embedded RAW | Unbound | Develop an embedded RAW source nondestructively. |
| Recipes and batch processing | Unbound | Open recipe recording and batch processing. |
| Focus, HDR and panorama merge | Unbound | Open multi-image merge workflows. |

## Canvas and pointer gestures

These temporary gestures are fixed so they remain available while other shortcuts are customized.

| Action | Gesture |
|---|---:|
| Temporarily pan the canvas | Hold Space and drag |
| Pan the canvas from any tool | Middle-button drag |
| Zoom around the pointer | Scroll over the canvas |
| Adjust a crop frame | C, then drag corners to resize or inside to move |
| Move a crop precisely | Arrow keys; Shift+Arrow moves 10 pixels |
| Finish or cancel a crop | Enter applies; Escape cancels |
| Pan the canvas with a wheel | Shift+Scroll over the canvas |
| Choose a Clone or Heal source | Alt+Click the canvas |
| Constrain a transform | Shift+Drag a transform handle |
| Transform from the centre | Alt+Drag a transform handle |
| Distort a transform corner | Ctrl+Drag a transform corner |
| Add or remove a canvas layer selection | Move tool + Auto-select: Ctrl+Click a layer on the canvas |
| Extend a canvas layer selection | Move tool + Auto-select: Shift+Click a layer on the canvas |
| Copy layers while dropping | Alt+Drop layers in the Layers panel |
| Copy a mask or effect | Drag it onto another layer |
| Draw vector corners and curves | P: click a corner, or drag to create smooth handles |
| Add a vector point without changing its curve | P: click a segment; A: double-click a segment |
| Close a vector contour | P: click its first anchor |
| Break vector handle alignment | A: Alt+Drag a handle |
| Select several vector objects | V: Shift+Click objects, or drag empty canvas to enclose them |
| Select one object inside a vector group | V: Ctrl+Click the object |
| Select or clear vector objects | Ctrl+A selects visible artwork; Ctrl+D clears the object selection |
| Group or duplicate vector objects | Ctrl+G groups; Ctrl+Shift+G ungroups; Ctrl+J duplicates |
| Constrain vector points and handles | A: Shift+Drag for 45-degree increments |
| Keep or cancel an image trace | Enter keeps a ready trace; Escape cancels (canvas focused) |

## Maintenance

Regenerate this file after changing the catalog:

```sh
python3 scripts/generate-keyboard-shortcuts.py
python3 scripts/generate-keyboard-shortcuts.py --check
```

The check also rejects duplicate command IDs and duplicate default chords.
