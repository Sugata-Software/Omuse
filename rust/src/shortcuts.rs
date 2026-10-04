//! Searchable, validated, per-user editor shortcuts for Linux.
//!
//! Control is the application menu modifier. Super is deliberately left to the
//! desktop so Omuse works with Omarchy and other Linux window managers.

use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub default: &'static str,
    pub category: &'static str,
    /// Space-separated search aliases. These are intentionally user-facing.
    pub keywords: &'static str,
    /// Short contextual guidance for the command palette and shortcut editor.
    pub notes: &'static str,
}

macro_rules! define_commands {
    ($(command!($id:literal, $label:literal, $default:literal, $category:literal, $keywords:literal, $notes:literal);)+) => {
        pub const CATALOG: &[CommandDefinition] = &[
            $(CommandDefinition {
                id: $id,
                label: $label,
                default: $default,
                category: $category,
                keywords: $keywords,
                notes: $notes,
            },)+
        ];

        /// Compatibility view used by the existing key-binding and settings UI.
        pub const DEFINITIONS: &[(&str, &str, &str)] = &[
            $(($id, $label, $default),)+
        ];
    };
}

// Keep each command on one line: scripts/generate-keyboard-shortcuts.py reads
// this block to produce the checked-in keyboard reference.
define_commands! {
    command!("command-search", "Search commands", "ctrl-k", "Application", "command palette quick open find action", "Search and run every command, including unbound commands.");
    command!("shortcuts", "Keyboard shortcuts", "ctrl-alt-k", "Application", "key bindings preferences remap customize", "Open the shortcut editor.");
    command!("ask-omuse", "Ask Omuse", "ctrl-shift-j", "Application", "assistant ai help", "Open the in-app assistant.");
    command!("create-workspace", "Create workspace", "ctrl-alt-n", "Application", "content design pages artboards", "Open the Create workspace.");
    command!("toggle-panels", "Toggle inspector panel", "f7", "Application", "hide show chrome dock inspector distraction", "Show or hide the inspector panel.");
    command!("quit", "Close", "ctrl-q", "Application", "quit exit close window", "Close Omuse, prompting when work is unsaved.");

    command!("new", "New canvas", "ctrl-n", "File", "document project create", "Create a new canvas.");
    command!("open", "Open", "ctrl-o", "File", "document project load", "Open an image or Omuse project.");
    command!("open-recent", "Open recent project", "ctrl-alt-o", "File", "history recent reopen canvas collection", "Search recently opened or saved projects; Enter opens the selection.");
    command!("save", "Save", "ctrl-s", "File", "document project write", "Save to the current project location.");
    command!("save-as", "Save as", "ctrl-shift-s", "File", "document project copy", "Save the project to a new location.");
    command!("import", "Import", "ctrl-shift-o", "File", "place add image photo", "Import an image into the current document.");
    command!("export", "Export", "ctrl-alt-shift-s", "File", "render publish png jpeg jpg webp tiff", "Export the finished artwork.");
    command!("import-report", "Import conversion report", "", "File", "compatibility conversion notes psd", "Show conversions made while importing the current document.");
    command!("previous-page", "Previous page", "alt-pageup", "File", "artboard earlier back", "Activate the previous page in a Create project.");
    command!("next-page", "Next page", "alt-pagedown", "File", "artboard later forward", "Activate the next page in a Create project.");

    command!("undo", "Undo", "ctrl-z", "Edit", "history revert", "Undo the most recent edit.");
    command!("redo", "Redo", "ctrl-shift-z", "Edit", "history repeat", "Redo the most recently undone edit.");
    command!("copy", "Copy", "ctrl-c", "Edit", "clipboard editable layers", "Copy selected pixels, or editable layer trees when no pixel selection is active. Other apps receive PNG.");
    command!("copy-merged", "Copy merged", "ctrl-shift-c", "Edit", "clipboard visible composite", "Copy the visible composite inside the selection.");
    command!("cut", "Cut", "ctrl-x", "Edit", "clipboard remove", "Cut selected pixels or layers.");
    command!("paste", "Paste", "ctrl-v", "Edit", "clipboard insert editable layers", "Paste editable layers from this Omuse session, or a new image layer from an external clipboard.");
    command!("delete-content", "Delete selection or layer", "backspace", "Edit", "clear erase remove", "Clear selected pixels, remove a mask, or delete selected layers.");
    command!("delete-forward", "Delete selection or layer (forward Delete)", "delete", "Edit", "clear erase remove forward", "Use the forward Delete key to clear selected pixels or delete selected layers.");

    command!("select-all", "Select all", "ctrl-a", "Selection", "canvas everything", "Select the full canvas.");
    command!("deselect", "Deselect", "ctrl-d", "Selection", "clear selection none", "Clear the current pixel selection.");
    command!("invert-selection", "Invert selection", "ctrl-shift-i", "Selection", "reverse selection", "Select pixels outside the current selection.");
    command!("transform-selection", "Transform selection", "", "Selection", "float move pixels", "Float selected pixels for independent transformation.");
    command!("commit-selection", "Commit floating selection", "", "Selection", "apply finish floating pixels", "Commit a floating pixel selection.");
    command!("cancel-selection", "Cancel floating selection", "", "Selection", "discard floating pixels", "Cancel a floating selection and restore its source.");
    command!("select-subject", "Select subject", "", "Selection", "foreground person object automatic local", "Find a foreground subject locally and refine its selection.");
    command!("remove-background", "Remove background", "", "Selection", "subject foreground cutout transparency automatic", "Find a foreground subject locally and remove its background.");
    command!("luminosity-range", "Luminosity range", "", "Selection", "brightness tonal highlights shadows mask", "Create a selection from a tonal range.");
    command!("color-range", "Colour range", "", "Selection", "color colour hue saturation sampled mask", "Select colours by RGB distance or hue, with softness and grey protection.");
    command!("feather-selection", "Feather selection", "shift-f6", "Selection", "soften edge blur", "Soften the edge of the current selection.");
    command!("grow-selection", "Expand selection", "", "Selection", "grow enlarge", "Expand the current selection by a chosen radius.");
    command!("shrink-selection", "Contract selection", "", "Selection", "shrink reduce", "Contract the current selection by a chosen radius.");
    command!("content-fill", "Content-aware fill", "", "Selection", "inpaint remove heal selected", "Fill selected pixels from surrounding image content.");

    command!("add", "New layer", "ctrl-shift-n", "Layer", "paint layer create", "Add an empty paint layer.");
    command!("duplicate", "Duplicate selected layers", "ctrl-j", "Layer", "copy layer", "Duplicate the selected layer roots.");
    command!("delete", "Delete selected layers", "", "Layer", "remove layer", "Delete the selected layer roots.");
    command!("group", "Group selected layers", "ctrl-g", "Layer", "folder collect", "Put the selected layers in a group.");
    command!("ungroup", "Ungroup folder", "", "Layer", "folder release", "Release an ordinary folder while preserving child order and placement.");
    command!("merge", "Merge down", "ctrl-e", "Layer", "combine layer below", "Merge the active layer into the layer below.");
    command!("flatten", "Flatten image", "", "Layer", "composite merge all visible", "Flatten the document's visible artwork.");
    command!("rename", "Rename layer", "", "Layer", "name label", "Rename the active layer.");
    command!("visibility", "Toggle layer visibility", "", "Layer", "show hide eye", "Show or hide the active layer.");
    command!("lock", "Toggle layer lock", "", "Layer", "protect unlock", "Lock or unlock the active layer.");
    command!("clipping", "Toggle clipping mask", "ctrl-alt-g", "Layer", "clip layer below alpha", "Clip the active layer to the pixel layer below.");
    command!("blend", "Cycle blend mode", "", "Layer", "compositing multiply screen overlay", "Advance the active layer to the next blend mode.");
    command!("up", "Move layer up", "", "Layer", "raise forward stack", "Move selected sibling layers one step up.");
    command!("down", "Move layer down", "", "Layer", "lower backward stack", "Move selected sibling layers one step down.");
    command!("nest", "Move layers into group", "", "Layer", "folder parent nest", "Choose a group for the selected layers.");
    command!("unnest", "Move layers out of group", "", "Layer", "folder parent unnest", "Move selected layers to the document root.");
    command!("select-layer-below", "Select layer below", "alt-[", "Layer", "previous lower stack", "Select the adjacent layer below.");
    command!("select-layer-above", "Select layer above", "alt-]", "Layer", "next upper stack", "Select the adjacent layer above.");
    command!("reorder-layer-down", "Reorder layer down", "ctrl-[", "Layer", "lower send backward stack", "Move selected layers one step down.");
    command!("reorder-layer-up", "Reorder layer up", "ctrl-]", "Layer", "raise bring forward stack", "Move selected layers one step up.");

    command!("add-mask", "Add layer mask", "", "Mask", "mask reveal alpha", "Add a revealing mask to the active layer.");
    command!("remove-mask", "Delete layer mask", "", "Mask", "mask discard", "Remove the active layer's mask without applying it.");
    command!("apply-mask", "Apply layer mask", "", "Mask", "mask bake commit", "Bake the active layer's mask into its pixels.");
    command!("invert-mask", "Invert layer mask", "", "Mask", "mask reverse black white", "Invert the active layer's mask.");
    command!("mask-enable", "Toggle layer mask", "", "Mask", "mask disable enable visibility", "Enable or disable the active layer's mask.");
    command!("mask-paint", "Toggle mask painting", "", "Mask", "mask pixels target brush", "Switch brush editing between layer pixels and the layer mask.");
    command!("mask-view", "Inspect layer mask", "", "Mask", "mask preview inspect alt click", "Temporarily view the active layer mask without changing the document.");
    command!("mask-link", "Toggle mask link", "", "Mask", "mask unlink placement", "Link or unlink mask and layer placement.");
    command!("mask-transform", "Place mask", "", "Mask", "mask move resize rotate transform", "Edit the active mask's placement.");
    command!("live-mask", "Live mask source", "", "Mask", "mask nondestructive source", "Choose a live source for the active mask.");
    command!("vector-mask", "Vector mask", "", "Mask", "path bezier nondestructive", "Open the vector-mask workspace.");

    command!("transform", "Transform layer", "ctrl-t", "Object", "move resize rotate scale", "Transform the active layer numerically.");
    command!("distort", "Distort corners", "", "Object", "perspective warp transform", "Move the active layer's four corners.");
    command!("sampling", "Cycle sampling quality", "", "Object", "nearest smooth interpolation", "Cycle transform resampling quality.");
    command!("edit-object", "Edit text, shape or vector artwork", "", "Object", "live object properties vector scene", "Edit the selected live text, shape or vector artwork.");
    command!("new-text", "Create text object", "", "Object", "type typography live text", "Create editable text with detailed settings.");
    command!("rasterize", "Rasterize object", "", "Object", "convert pixels bake", "Convert an editable object to pixels; Undo restores it.");
    command!("image-trace", "Image trace / retrace", "", "Object", "bitmap vectorize convert logo outline palette colours simplify points", "Convert an image into editable vector artwork on the main canvas. Compare Source/Trace, adjust detail and point limits, then Keep vectors. The original image is retained.");
    command!("vector-path", "Vector pen tool", "p", "Object", "bezier pen curves nodes svg import export", "Click to place corners; drag to draw smooth curves. Click the first anchor to close. Click a segment to add a point. A edits nodes; V moves objects.");
    command!("vector-scene", "Vector artwork on canvas", "shift-p", "Object", "illustration objects scene rectangle ellipse bezier svg pdf gradient stroke dash text curve typography", "Draw and edit objects on the main canvas with the shared layer stack. Enter keeps edits; Escape discards them.");
    command!("vector-nodes", "Vector node tool", "a", "Object", "direct selection bezier handles curves anchors", "Drag anchors or handles on the main canvas. Double-click a segment to add a point; Alt frees a handle, Shift constrains to 45 degrees. Delete removes points.");
    command!("vector-group", "Group vector selection", "", "Vector", "objects folder collect", "Group selected vector objects. Ctrl+G also groups objects while editing artwork.");
    command!("vector-ungroup", "Ungroup vector selection", "ctrl-shift-g", "Vector", "objects folder release", "Release selected outer vector groups while retaining object order and geometry.");
    command!("vector-outline", "Toggle vector outline view", "ctrl-y", "Vector", "wireframe paths precision", "Inspect vector contours at the current zoom. This viewing mode does not change exports.");
    command!("vector-union", "Unite vector shapes", "", "Vector", "boolean union pathfinder combine", "Combine selected filled shapes; the bottom shape supplies the style. Undo restores the originals.");
    command!("vector-subtract", "Subtract vector shapes", "", "Vector", "boolean difference cut pathfinder", "Cut upper selected filled shapes out of the bottom selected shape.");
    command!("vector-intersect", "Intersect vector shapes", "", "Vector", "boolean overlap pathfinder", "Keep the shared filled area of selected shapes.");
    command!("vector-exclude", "Exclude vector overlap", "", "Vector", "boolean xor pathfinder", "Keep areas covered by an odd number of selected shapes.");
    command!("vector-divide", "Divide vector shape", "", "Vector", "boolean split pathfinder", "Partition the bottom selected shape using the upper selected shapes.");
    command!("vector-align-left", "Align vectors left", "", "Vector", "objects arrange", "Align selected objects or groups to the left edge of the selection.");
    command!("vector-align-center-x", "Centre vectors horizontally", "", "Vector", "objects align horizontal center", "Align selected objects or groups to their shared horizontal centre.");
    command!("vector-align-right", "Align vectors right", "", "Vector", "objects arrange", "Align selected objects or groups to the right edge of the selection.");
    command!("vector-align-top", "Align vectors top", "", "Vector", "objects arrange", "Align selected objects or groups to the top of the selection.");
    command!("vector-align-center-y", "Centre vectors vertically", "", "Vector", "objects align vertical center", "Align selected objects or groups to their shared vertical centre.");
    command!("vector-align-bottom", "Align vectors bottom", "", "Vector", "objects arrange", "Align selected objects or groups to the bottom of the selection.");
    command!("vector-distribute-x", "Space vectors horizontally", "", "Vector", "objects distribute horizontal gaps", "Distribute three or more objects or groups with equal horizontal gaps.");
    command!("vector-distribute-y", "Space vectors vertically", "", "Vector", "objects distribute vertical gaps", "Distribute three or more objects or groups with equal vertical gaps.");
    command!("vector-same-fill", "Select vectors with same fill", "", "Vector", "matching color colour appearance", "Select visible vector objects matching the active object's fill, including alpha.");
    command!("vector-same-stroke", "Select vectors with same stroke", "", "Vector", "matching outline width appearance", "Select visible vector objects matching the active object's stroke colour and width.");
    command!("vector-same-opacity", "Select vectors with same opacity", "", "Vector", "matching transparency appearance", "Select visible vector objects matching the active object's opacity.");
    command!("vector-import-svg", "Import editable SVG artwork", "", "Vector", "objects groups paths file", "Add supported solid-paint SVG objects and groups to the current artwork without replacing existing objects.");
    command!("vector-export-svg", "Export editable SVG artwork", "", "Vector", "objects groups paths file", "Export the complete vector artwork with supported groups, solid paint and opacity.");
    command!("rotate", "Rotate layer 90 degrees", "", "Object", "turn clockwise", "Rotate the active layer clockwise by 90 degrees.");
    command!("flip", "Flip layer horizontally", "", "Object", "mirror horizontal", "Flip the active layer horizontally.");
    command!("nudge-left", "Nudge left", "left", "Object", "move one pixel", "Move selected layers left by one pixel.");
    command!("nudge-right", "Nudge right", "right", "Object", "move one pixel", "Move selected layers right by one pixel.");
    command!("nudge-up", "Nudge up", "up", "Object", "move one pixel", "Move selected layers up by one pixel.");
    command!("nudge-down", "Nudge down", "down", "Object", "move one pixel", "Move selected layers down by one pixel.");
    command!("nudge-left-large", "Nudge left 10 pixels", "shift-left", "Object", "move large ten", "Move selected layers left by ten pixels.");
    command!("nudge-right-large", "Nudge right 10 pixels", "shift-right", "Object", "move large ten", "Move selected layers right by ten pixels.");
    command!("nudge-up-large", "Nudge up 10 pixels", "shift-up", "Object", "move large ten", "Move selected layers up by ten pixels.");
    command!("nudge-down-large", "Nudge down 10 pixels", "shift-down", "Object", "move large ten", "Move selected layers down by ten pixels.");

    command!("resize-image", "Resize image", "ctrl-alt-i", "Image", "dimensions resample scale resolution", "Resize the image and all of its content.");
    command!("resize", "Resize canvas", "ctrl-alt-c", "Image", "dimensions crop extend anchor", "Change the canvas bounds without scaling content.");
    command!("crop", "Crop canvas", "c", "Image", "trim selection canvas ratio square portrait social", "Preview a movable crop with photo and social ratios. Enter applies; Escape cancels. Outside pixels are retained.");
    command!("trim", "Trim canvas", "", "Image", "remove transparent border", "Trim canvas edges using chosen criteria.");
    command!("invert", "Invert pixels", "ctrl-i", "Image", "negative adjustment", "Invert selected pixels or the active mask.");
    command!("gray", "Convert to grayscale", "ctrl-shift-u", "Image", "black white desaturate monochrome", "Apply a grayscale pixel adjustment; Undo restores the previous pixels.");
    command!("blur", "Blur pixels", "", "Image", "soften filter", "Apply a small blur pixel adjustment.");
    command!("sharpen", "Sharpen pixels", "", "Image", "detail filter", "Apply a small sharpen pixel adjustment.");
    command!("brighter", "Increase brightness", "", "Image", "lighten exposure adjustment", "Increase pixel brightness.");
    command!("darker", "Decrease brightness", "", "Image", "darken exposure adjustment", "Decrease pixel brightness.");
    command!("contrast", "Increase contrast", "", "Image", "adjustment", "Increase pixel contrast.");
    command!("saturation", "Increase saturation", "", "Image", "color colour vivid adjustment", "Increase pixel saturation.");
    command!("filter", "Pixel filters", "", "Image", "effects adjustment blur sharpen", "Open the destructive pixel-filter gallery.");
    command!("filter-levels", "Levels", "ctrl-l", "Image", "pixel filter tonal black white gamma", "Open Levels settings; Apply changes pixels and Undo restores them.");
    command!("filter-curves", "Curves", "ctrl-m", "Image", "pixel filter tonal points", "Open Curves settings; Apply changes pixels and Undo restores them.");
    command!("filter-hsl", "Hue and saturation", "ctrl-u", "Image", "pixel filter hsl color colour", "Open Hue and Saturation settings; Apply changes pixels and Undo restores them.");
    command!("filter-color-balance", "Colour balance", "ctrl-b", "Image", "pixel filter color colour shadows highlights", "Open Colour Balance settings; Apply changes pixels and Undo restores them.");
    command!("camera-raw", "Camera Raw", "ctrl-shift-a", "Image", "develop photo exposure curves color", "Open nondestructive Camera Raw controls for a pixel layer.");

    command!("zoom-in", "Zoom in", "ctrl-=", "View", "magnify closer plus", "Use the next zoom stop while keeping the viewport centre fixed on the artwork.");
    command!("zoom-in-plus", "Zoom in (+)", "ctrl-+", "View", "magnify closer plus", "Increase canvas magnification with the Plus key.");
    command!("zoom-out", "Zoom out", "ctrl--", "View", "magnify farther minus", "Decrease canvas magnification.");
    command!("fit", "Fit canvas", "ctrl-0", "View", "zoom window screen", "Fit the full canvas in the viewport.");
    command!("actual", "Actual pixels", "ctrl-1", "View", "zoom 100 percent", "Show one image pixel per logical pixel.");
    command!("grid", "Toggle grid", "ctrl-'", "View", "show hide alignment", "Show or hide the canvas grid.");
    command!("grid-spacing", "Change grid spacing", "", "View", "grid spacing pixels settings", "Cycle the canvas grid's major spacing.");
    command!("grid-subdivisions", "Change grid subdivisions", "", "View", "grid subdivisions minor lines settings", "Cycle the canvas grid's minor subdivisions.");
    command!("guides", "Toggle guides", "ctrl-;", "View", "show hide alignment", "Show or hide guides.");
    command!("rulers", "Toggle rulers", "ctrl-r", "View", "show hide measurements", "Show or hide canvas rulers.");
    command!("snapping", "Toggle snapping", "ctrl-:", "View", "align grid guides snap", "Enable or disable snapping to the grid and guides.");
    command!("auto-select", "Toggle auto-select", "", "View", "move tool layers click", "Choose layers from the canvas when using Move.");
    command!("transform-box", "Toggle transform box", "", "View", "handles bounds move tool", "Show or hide Move-tool transform handles.");
    command!("add-guide", "Manage guides", "", "View", "guide alignment position", "Add, clear, or configure guides.");

    command!("tool-brush", "Brush tool", "b", "Tools", "paint freehand", "Paint freehand strokes.");
    command!("tool-pencil", "Pencil tool", "shift-b", "Tools", "paint hard pixel", "Paint hard-edged freehand strokes.");
    command!("tool-eraser", "Eraser tool", "e", "Tools", "erase brush", "Erase pixels with a brush.");
    command!("tool-fill", "Fill tool", "shift-g", "Tools", "bucket flood paint", "Fill a connected colour region.");
    command!("tool-gradient", "Gradient tool", "g", "Tools", "blend ramp fill", "Draw a configurable colour gradient.");
    command!("tool-rectangle", "Rectangle selection tool", "m", "Tools", "marquee select", "Draw rectangular pixel selections.");
    command!("tool-ellipse", "Ellipse selection tool", "shift-m", "Tools", "marquee oval circle select", "Draw elliptical pixel selections.");
    command!("tool-lasso", "Lasso tool", "l", "Tools", "freehand select", "Draw freehand pixel selections.");
    command!("tool-wand", "Wand tool", "w", "Tools", "magic color colour select", "Select connected pixels of similar colour.");
    command!("tool-object", "Connected subject tool", "", "Tools", "object select subject local", "Select the connected foreground subject under the pointer.");
    command!("tool-move", "Move tool", "v", "Tools", "transform position layers", "Move and transform layers on the canvas.");
    command!("tool-hand", "Hand tool", "h", "Tools", "pan canvas", "Pan the canvas; holding Space temporarily activates panning.");
    command!("tool-picker", "Eyedropper tool", "i", "Tools", "color colour sample", "Sample a colour from the canvas.");
    command!("tool-clone", "Clone stamp tool", "s", "Tools", "retouch copy pixels", "Paint from a sampled source; Alt-click chooses the source.");
    command!("tool-spot-heal", "Spot healing tool", "j", "Tools", "retouch blemish remove", "Heal small areas from nearby pixels.");
    command!("tool-heal", "Healing clone tool", "shift-j", "Tools", "retouch sampled source", "Heal using an explicitly sampled source.");
    command!("tool-blur-brush", "Blur brush tool", "", "Tools", "retouch soften paint", "Paint localized blur.");
    command!("tool-smudge", "Smudge tool", "", "Tools", "retouch smear paint", "Smear pixels with a brush.");
    command!("tool-liquify", "Liquify tool", "ctrl-shift-x", "Tools", "warp push pixels", "Warp pixels with a brush.");
    command!("text", "Text tool", "t", "Tools", "type typography", "Create or edit text directly on the canvas.");
    command!("tool-shape-rect", "Rectangle shape tool", "u", "Tools", "vector live shape", "Draw editable rectangle shapes.");
    command!("tool-shape-ellipse", "Ellipse shape tool", "shift-u", "Tools", "vector live oval circle shape", "Draw editable ellipse shapes.");
    command!("tool-line", "Line tool", "", "Tools", "vector live shape", "Draw editable line shapes.");
    command!("tool-settings", "Tool settings", "", "Tools", "brush wand fill options", "Open detailed settings for the active tool.");
    command!("brush-smaller", "Decrease brush size", "[", "Tools", "brush radius smaller", "Decrease the active brush diameter.");
    command!("brush-larger", "Increase brush size", "]", "Tools", "brush radius larger", "Increase the active brush diameter.");
    command!("brush-softer", "Decrease brush hardness", "{", "Tools", "brush soft edge", "Make the active brush edge softer.");
    command!("brush-harder", "Increase brush hardness", "}", "Tools", "brush hard edge", "Make the active brush edge harder.");
    command!("brush-opacity-10", "Set brush opacity to 10%", "1", "Tools", "brush transparency flow", "Set brush opacity to 10 percent.");
    command!("brush-opacity-20", "Set brush opacity to 20%", "2", "Tools", "brush transparency flow", "Set brush opacity to 20 percent.");
    command!("brush-opacity-30", "Set brush opacity to 30%", "3", "Tools", "brush transparency flow", "Set brush opacity to 30 percent.");
    command!("brush-opacity-40", "Set brush opacity to 40%", "4", "Tools", "brush transparency flow", "Set brush opacity to 40 percent.");
    command!("brush-opacity-50", "Set brush opacity to 50%", "5", "Tools", "brush transparency flow", "Set brush opacity to 50 percent.");
    command!("brush-opacity-60", "Set brush opacity to 60%", "6", "Tools", "brush transparency flow", "Set brush opacity to 60 percent.");
    command!("brush-opacity-70", "Set brush opacity to 70%", "7", "Tools", "brush transparency flow", "Set brush opacity to 70 percent.");
    command!("brush-opacity-80", "Set brush opacity to 80%", "8", "Tools", "brush transparency flow", "Set brush opacity to 80 percent.");
    command!("brush-opacity-90", "Set brush opacity to 90%", "9", "Tools", "brush transparency flow", "Set brush opacity to 90 percent.");
    command!("brush-opacity-100", "Set brush opacity to 100%", "0", "Tools", "brush transparency flow full", "Set brush opacity to 100 percent.");
    command!("fill-foreground", "Fill with foreground colour", "alt-backspace", "Tools", "paint color colour", "Fill the selection with the foreground colour.");
    command!("fill-background", "Fill with background colour", "ctrl-backspace", "Tools", "paint color colour", "Fill the selection with the background colour.");
    command!("default-colors", "Default colours", "d", "Tools", "black white foreground background reset", "Reset foreground and background to black and white.");
    command!("swap-colors", "Swap colours", "x", "Tools", "foreground background exchange", "Exchange the foreground and background colours.");

    command!("live-adjustment", "Add adjustment layer", "", "Adjustments", "nondestructive color colour tone", "Add a nondestructive adjustment layer.");
    command!("edit-adjustment", "Edit adjustment", "", "Adjustments", "nondestructive layer settings", "Edit the selected adjustment layer.");
    command!("effects", "Layer effects", "", "Adjustments", "shadow glow stroke overlay", "Edit nondestructive effects on the active layer.");
    command!("clear-effects", "Delete layer effects", "", "Adjustments", "remove styles", "Remove every effect from the active layer.");
    command!("filter-stack", "Editable filter stack", "", "Adjustments", "nondestructive blur sharpen nodes target colour color uniformity reference match palette", "Build a reorderable stack, including reference-photo colour matching and colour uniformity.");
    command!("dither", "Dither and halftone", "", "Adjustments", "retro print pixel atkinson floyd steinberg bayer dots lines diamonds patterns ascii", "Preview ten retro finishes with pixel, cell and palette controls; Apply is one undo step.");
    command!("bloom-glow", "Bloom into transparency", "", "Adjustments", "highlight glow bloom margin transparent", "Preview highlight glow that can spread into existing transparent layer margins.");
    command!("vignette-overlay", "Vignette overlay", "", "Adjustments", "vignette blank empty layer frame edge colour", "Preview a coloured edge overlay, including on an empty paint layer.");
    command!("local-contrast", "Local tonal contrast", "", "Adjustments", "clarity detail spatial radius shadow midtone highlight", "Preview radius-based detail contrast with independent tonal-zone strength.");
    command!("blend-if", "Blend If", "", "Adjustments", "tonal blend range", "Control layer visibility using tonal ranges.");
    command!("colour-management", "Precision and colour", "", "Adjustments", "color colour profile bit depth working space", "Open precision and colour-management controls.");

    command!("advanced-retouch", "Frequency and tonal retouch", "", "Workspaces", "frequency separation dodge burn", "Open the advanced retouch workspace.");
    command!("controlled-removal", "Controlled content-aware removal", "", "Workspaces", "inpaint patch remove", "Open the controlled removal workspace.");
    command!("editable-warp", "Editable mesh and pin warp", "", "Workspaces", "deform puppet mesh", "Open the nondestructive warp workspace.");
    command!("refine-workspace", "Selection refinement workspace", "", "Workspaces", "mask matte edge", "Open advanced selection refinement.");
    command!("brush-studio", "Brush studio", "", "Workspaces", "brush dynamics preset", "Open advanced brush design controls.");
    command!("smart-source", "Smart source", "", "Workspaces", "linked embedded image refresh", "Open the editable smart-source workspace.");
    command!("editable-raw", "Develop embedded RAW", "", "Workspaces", "photo develop nondestructive", "Develop an embedded RAW source nondestructively.");
    command!("automation", "Recipes and batch processing", "", "Workspaces", "macro actions batch", "Open recipe recording and batch processing.");
    command!("multi-image", "Focus, HDR and panorama merge", "", "Workspaces", "stack bracket stitch", "Open multi-image merge workflows.");
}

/// Temporary and pointer gestures that are intentionally not remappable.
pub const GESTURES: &[(&str, &str)] = &[
    ("Temporarily pan the canvas", "Hold Space and drag"),
    ("Pan the canvas from any tool", "Middle-button drag"),
    ("Zoom around the pointer", "Scroll over the canvas"),
    (
        "Adjust a crop frame",
        "C, then drag corners to resize or inside to move",
    ),
    (
        "Move a crop precisely",
        "Arrow keys; Shift+Arrow moves 10 pixels",
    ),
    ("Finish or cancel a crop", "Enter applies; Escape cancels"),
    (
        "Pan the canvas with a wheel",
        "Shift+Scroll over the canvas",
    ),
    ("Choose a Clone or Heal source", "Alt+Click the canvas"),
    ("Constrain a transform", "Shift+Drag a transform handle"),
    ("Transform from the centre", "Alt+Drag a transform handle"),
    ("Distort a transform corner", "Ctrl+Drag a transform corner"),
    (
        "Add or remove a canvas layer selection",
        "Move tool + Auto-select: Ctrl+Click a layer on the canvas",
    ),
    (
        "Extend a canvas layer selection",
        "Move tool + Auto-select: Shift+Click a layer on the canvas",
    ),
    (
        "Copy layers while dropping",
        "Alt+Drop layers in the Layers panel",
    ),
    ("Copy a mask or effect", "Drag it onto another layer"),
    (
        "Draw vector corners and curves",
        "P: click a corner, or drag to create smooth handles",
    ),
    (
        "Add a vector point without changing its curve",
        "P: click a segment; A: double-click a segment",
    ),
    ("Close a vector contour", "P: click its first anchor"),
    ("Break vector handle alignment", "A: Alt+Drag a handle"),
    (
        "Select several vector objects",
        "V: Shift+Click objects, or drag empty canvas to enclose them",
    ),
    (
        "Select one object inside a vector group",
        "V: Ctrl+Click the object",
    ),
    (
        "Select or clear vector objects",
        "Ctrl+A selects visible artwork; Ctrl+D clears the object selection",
    ),
    (
        "Group or duplicate vector objects",
        "Ctrl+G groups; Ctrl+Shift+G ungroups; Ctrl+J duplicates",
    ),
    (
        "Constrain vector points and handles",
        "A: Shift+Drag for 45-degree increments",
    ),
    (
        "Keep or cancel an image trace",
        "Enter keeps a ready trace; Escape cancels (canvas focused)",
    ),
];

pub fn catalog() -> &'static [CommandDefinition] {
    CATALOG
}

pub fn definition(id: &str) -> Option<&'static CommandDefinition> {
    CATALOG.iter().find(|command| command.id == id)
}

pub fn category(id: &str) -> &'static str {
    definition(id).map_or("Other", |command| command.category)
}

pub fn contextual_notes(id: &str) -> &'static str {
    definition(id).map_or("", |command| command.notes)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandMatch {
    pub definition: &'static CommandDefinition,
    pub chord: String,
    pub score: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Shortcuts {
    pub overrides: BTreeMap<String, String>,
}

impl Shortcuts {
    /// Return the effective chord. Explicit user choices win over newly added
    /// defaults; a colliding new default is left unbound instead of erasing an
    /// older customization.
    pub fn chord(&self, id: &str) -> &str {
        if let Some(chord) = self.overrides.get(id) {
            return chord;
        }
        let default = definition(id).map_or("", |command| command.default);
        if default.is_empty()
            || self
                .overrides
                .iter()
                .any(|(other, chord)| other != id && !chord.is_empty() && chord == default)
        {
            ""
        } else {
            default
        }
    }

    /// Search labels, categories, IDs, aliases and effective (custom) chords.
    /// Every whitespace-separated query term must match. Results are stable and
    /// prefer exact labels/IDs, then prefixes, aliases, categories and chords.
    pub fn search(&self, query: &str) -> Vec<CommandMatch> {
        let query = normalize_query(query);
        let terms: Vec<&str> = query.split_whitespace().collect();
        let mut matches: Vec<_> = CATALOG
            .iter()
            .filter_map(|command| {
                let chord = self.chord(command.id).to_string();
                let score = search_score(command, &chord, &query, &terms)?;
                Some(CommandMatch {
                    definition: command,
                    chord,
                    score,
                })
            })
            .collect();
        matches.sort_by(|a, b| {
            a.score
                .cmp(&b.score)
                .then_with(|| {
                    a.definition
                        .label
                        .to_ascii_lowercase()
                        .cmp(&b.definition.label.to_ascii_lowercase())
                })
                .then_with(|| a.definition.id.cmp(b.definition.id))
        });
        matches
    }

    /// Assign a chord. An empty chord explicitly leaves the command unbound.
    pub fn assign(&mut self, id: &str, chord: &str) -> Result<()> {
        let command = definition(id).ok_or_else(|| anyhow::anyhow!("Unknown command: {id}"))?;
        if chord.trim().is_empty() {
            self.overrides.insert(id.into(), String::new());
            return Ok(());
        }
        let chord = canonical_chord(id, chord)?;
        for other in CATALOG {
            ensure!(
                other.id == id || self.chord(other.id) != chord,
                "Already assigned to {}",
                other.label
            );
        }
        let mut candidate = self.clone();
        candidate.overrides.insert(command.id.into(), chord);
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    /// Explicitly unbind a command while preserving that choice across upgrades.
    pub fn clear(&mut self, id: &str) -> Result<()> {
        self.assign(id, "")
    }

    /// Remove a customization and return to the built-in default.
    pub fn reset(&mut self, id: &str) -> Result<()> {
        ensure!(definition(id).is_some(), "Unknown command: {id}");
        let mut candidate = self.clone();
        candidate.overrides.remove(id);
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let value: Self = serde_json::from_slice(
            &std::fs::read(path).with_context(|| format!("Read {}", path.display()))?,
        )
        .with_context(|| format!("Parse {}", path.display()))?;
        value.validate()?;
        Ok(value)
    }

    /// Validate before touching the existing file, then publish with an atomic
    /// rename. A failed validation or write leaves the previous file intact.
    pub fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write as _;
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt as _;

        self.validate()?;
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Missing settings directory"))?;
        std::fs::create_dir_all(parent)?;
        let temp = parent.join(format!(".shortcuts-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut options = std::fs::OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            options.mode(0o600);
            let mut file = options.open(&temp)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            std::fs::rename(&temp, path)?;
            omuse::durable_fs::sync_path(parent)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.overrides.len() <= CATALOG.len(),
            "Too many shortcut overrides"
        );
        let mut explicit = BTreeSet::new();
        for (id, chord) in &self.overrides {
            ensure!(definition(id).is_some(), "Unknown shortcut command: {id}");
            if chord.is_empty() {
                continue;
            }
            let canonical = canonical_chord(id, chord)?;
            ensure!(canonical == *chord, "Shortcut is not canonical: {chord}");
            ensure!(
                explicit.insert(chord),
                "Conflicting custom shortcut: {chord}"
            );
        }

        let mut defaults = BTreeSet::new();
        for command in CATALOG {
            if command.default.is_empty() {
                continue;
            }
            let canonical = canonical_chord(command.id, command.default)
                .with_context(|| format!("Invalid default for {}", command.id))?;
            ensure!(
                canonical == command.default,
                "Non-canonical default for {}",
                command.id
            );
            ensure!(
                defaults.insert(command.default),
                "Conflicting default shortcut: {}",
                command.default
            );
        }

        let mut effective = BTreeSet::new();
        for command in CATALOG {
            let chord = self.chord(command.id);
            ensure!(
                chord.is_empty() || effective.insert(chord),
                "Conflicting effective shortcut: {chord}"
            );
        }
        Ok(())
    }
}

pub fn display_chord(chord: &str) -> String {
    if chord.is_empty() {
        return "Unbound".into();
    }
    let Ok(parsed) = gpui_kit::Keystroke::parse(chord) else {
        return chord.into();
    };
    let mut parts = Vec::new();
    if parsed.modifiers.function {
        parts.push("Fn".to_string());
    }
    if parsed.modifiers.control {
        parts.push("Ctrl".to_string());
    }
    if parsed.modifiers.alt {
        parts.push("Alt".to_string());
    }
    if parsed.modifiers.platform {
        parts.push("Super".to_string());
    }
    if parsed.modifiers.shift {
        parts.push("Shift".to_string());
    }
    let key = match parsed.key.as_str() {
        "left" => "Left".to_string(),
        "right" => "Right".to_string(),
        "up" => "Up".to_string(),
        "down" => "Down".to_string(),
        "pageup" => "Page Up".to_string(),
        "pagedown" => "Page Down".to_string(),
        "backspace" => "Backspace".to_string(),
        "delete" => "Delete".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        "{" => "Shift+[".to_string(),
        "}" => "Shift+]".to_string(),
        ":" => "Shift+;".to_string(),
        key if key.len() == 1 => key.to_ascii_uppercase(),
        key => key.to_string(),
    };
    parts.push(key);
    parts.join("+")
}

fn canonical_chord(id: &str, chord: &str) -> Result<String> {
    let parsed = gpui_kit::Keystroke::parse(chord.trim())
        .map_err(|error| anyhow::anyhow!("Invalid shortcut: {error}"))?;
    let chord = parsed.unparse();
    ensure!(
        !parsed.modifiers.platform,
        "Super is reserved for the Linux desktop"
    );
    ensure!(
        !matches!(
            parsed.key.as_str(),
            "escape" | "enter" | "return" | "tab" | "space"
        ),
        "Escape, Enter, Tab and Space are reserved for dialogs, navigation or temporary panning"
    );
    ensure!(
        !(parsed.modifiers.control
            && parsed.modifiers.alt
            && parsed.key.starts_with('f')
            && parsed.key[1..]
                .parse::<u8>()
                .is_ok_and(|number| (1..=12).contains(&number))),
        "Ctrl+Alt+F1–F12 are reserved for Linux virtual terminals"
    );
    let bare_printable = parsed.key.chars().count() == 1
        && !parsed.modifiers.control
        && !parsed.modifiers.alt
        && !parsed.modifiers.function;
    ensure!(
        !bare_printable || allows_plain_key(id),
        "Unmodified printable keys are limited to tools and direct canvas controls"
    );
    if matches!(id, "command-search" | "save" | "save-as") {
        let function_key = parsed
            .key
            .strip_prefix('f')
            .and_then(|value| value.parse::<u8>().ok())
            .is_some_and(|number| (1..=35).contains(&number));
        ensure!(
            parsed.modifiers.control
                || parsed.modifiers.alt
                || parsed.modifiers.platform
                || function_key,
            "Search and Save need Ctrl, Alt or a function key so text navigation stays available"
        );
    }
    Ok(chord)
}

fn allows_plain_key(id: &str) -> bool {
    id.starts_with("tool-")
        || id.starts_with("brush-")
        || matches!(
            id,
            "text"
                | "crop"
                | "vector-path"
                | "vector-scene"
                | "vector-nodes"
                | "default-colors"
                | "swap-colors"
        )
}

fn normalize_search(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric()
                || matches!(character, '[' | ']' | '{' | '}' | ':' | '=' | '\'' | ';')
            {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_query(value: &str) -> String {
    let mut query = normalize_search(value);
    let lower = value.trim().to_ascii_lowercase();
    let looks_modified = ["ctrl", "control", "alt", "shift", "super"]
        .iter()
        .any(|modifier| lower.contains(modifier));
    if (lower == "-" || (looks_modified && lower.ends_with('-')))
        && !query.split_whitespace().any(|term| term == "minus")
    {
        if !query.is_empty() {
            query.push(' ');
        }
        query.push_str("minus");
    }
    if lower == "+" || (looks_modified && (lower.ends_with("++") || lower.ends_with("-+"))) {
        if !query.is_empty() {
            query.push(' ');
        }
        query.push_str("plus");
    }
    query
}

fn chord_search_terms(chord: &str) -> String {
    let Ok(parsed) = gpui_kit::Keystroke::parse(chord) else {
        return normalize_search(chord);
    };
    let mut terms = Vec::new();
    if parsed.modifiers.function {
        terms.push("fn function".to_string());
    }
    if parsed.modifiers.control {
        terms.push("ctrl control".to_string());
    }
    if parsed.modifiers.alt {
        terms.push("alt option".to_string());
    }
    if parsed.modifiers.platform {
        terms.push("super meta win".to_string());
    }
    if parsed.modifiers.shift {
        terms.push("shift".to_string());
    }
    terms.push(match parsed.key.as_str() {
        "-" => "minus -".into(),
        "=" => "equals plus =".into(),
        "+" => "plus shift =".into(),
        "{" => "shift [ {".into(),
        "}" => "shift ] }".into(),
        ":" => "shift ; : colon".into(),
        "pageup" => "pageup page up".into(),
        "pagedown" => "pagedown page down".into(),
        key => normalize_search(key),
    });
    terms.join(" ")
}

fn search_score(
    command: &CommandDefinition,
    chord: &str,
    query: &str,
    terms: &[&str],
) -> Option<u32> {
    if terms.is_empty() {
        return Some(0);
    }
    let fields = [
        (normalize_search(command.id), 0u32),
        (normalize_search(command.label), 2),
        (normalize_search(command.keywords), 5),
        (normalize_search(command.category), 8),
        (
            format!(
                "{} {}",
                chord_search_terms(chord),
                normalize_search(&display_chord(chord))
            ),
            10,
        ),
    ];
    let mut score = 0;
    for term in terms {
        let best = fields
            .iter()
            .filter_map(|(field, weight)| {
                if field == term {
                    Some(*weight)
                } else if field.split_whitespace().any(|word| word.starts_with(term)) {
                    Some(weight + 20)
                } else if field.contains(term) {
                    Some(weight + 40)
                } else {
                    None
                }
            })
            .min()?;
        score += best;
    }
    if normalize_search(command.id) == query {
        score = 0;
    } else if normalize_search(command.label) == query {
        score = 1;
    } else if normalize_query(chord) == query {
        score = score.min(3);
    }
    Some(score)
}

pub fn settings_path() -> std::path::PathBuf {
    omuse::identity::config_file_path("shortcuts.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_and_default_chords_are_unique_and_valid() {
        assert!(CATALOG.len() >= 140, "catalog unexpectedly shrank");
        let ids: BTreeSet<_> = CATALOG.iter().map(|command| command.id).collect();
        assert_eq!(ids.len(), CATALOG.len());
        Shortcuts::default().validate().unwrap();
        assert!(CATALOG.iter().all(|command| !command.notes.is_empty()));
        assert!(CATALOG.iter().all(|command| !command.category.is_empty()));
    }

    #[test]
    fn search_is_case_insensitive_multiword_ranked_and_uses_custom_chords() {
        let mut shortcuts = Shortcuts::default();
        shortcuts.assign("save", "ctrl-alt-s").unwrap();
        assert_eq!(shortcuts.search("SAVE")[0].definition.id, "save");
        let results = shortcuts.search("layer mask");
        assert!(
            results
                .iter()
                .any(|found| found.definition.id == "add-mask")
        );
        assert_eq!(shortcuts.search("CTRL ALT S")[0].definition.id, "save");
        assert_eq!(shortcuts.search("magic wand")[0].definition.id, "tool-wand");
        assert_eq!(shortcuts.search("Ctrl+-")[0].definition.id, "zoom-out");
        assert_eq!(
            shortcuts.search("control minus")[0].definition.id,
            "zoom-out"
        );
        assert_eq!(
            shortcuts.search("Alt Page Up")[0].definition.id,
            "previous-page"
        );
        assert_eq!(
            shortcuts.search("ctrl plus")[0].definition.id,
            "zoom-in-plus"
        );
    }

    #[test]
    fn clearing_resetting_and_conflicts_are_atomic() {
        let mut shortcuts = Shortcuts::default();
        shortcuts.clear("save").unwrap();
        assert_eq!(shortcuts.chord("save"), "");
        shortcuts.reset("save").unwrap();
        assert_eq!(shortcuts.chord("save"), "ctrl-s");
        let previous = shortcuts.clone();
        assert!(shortcuts.assign("save", "ctrl-o").is_err());
        assert_eq!(shortcuts.overrides, previous.overrides);
        assert!(shortcuts.assign("save", "super-s").is_err());
        assert!(shortcuts.assign("save", "shift-enter").is_err());
        assert!(shortcuts.assign("save", "ctrl-alt-f4").is_err());
        assert!(shortcuts.assign("save", "q").is_err());
        for id in ["save", "save-as", "command-search"] {
            for key in [
                "home",
                "end",
                "pageup",
                "pagedown",
                "backspace",
                "shift-home",
            ] {
                assert!(
                    shortcuts.assign(id, key).is_err(),
                    "{id} must not intercept {key} in text"
                );
            }
        }
        shortcuts.assign("save", "f6").unwrap();
        shortcuts.assign("tool-brush", "Q").unwrap();
        assert_eq!(shortcuts.chord("tool-brush"), "shift-q");
    }

    #[test]
    fn prior_override_wins_over_a_new_colliding_default() {
        let shortcuts = Shortcuts {
            overrides: BTreeMap::from([("open".into(), "ctrl-k".into())]),
        };
        shortcuts.validate().unwrap();
        assert_eq!(shortcuts.chord("open"), "ctrl-k");
        assert_eq!(shortcuts.chord("command-search"), "");
    }

    #[test]
    fn saved_recording_reloads_and_invalid_save_preserves_previous_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("keys.json");
        let mut shortcuts = Shortcuts::default();
        shortcuts.assign("save", "ctrl-alt-s").unwrap();
        shortcuts.clear("open").unwrap();
        shortcuts.save(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let loaded = Shortcuts::load(&path).unwrap();
        assert_eq!(loaded.chord("save"), "ctrl-alt-s");
        assert_eq!(loaded.chord("open"), "");

        let mut invalid = loaded;
        invalid.overrides.insert("save".into(), "super-s".into());
        assert!(invalid.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn display_chords_are_readable() {
        assert_eq!(display_chord("ctrl-alt-shift-s"), "Ctrl+Alt+Shift+S");
        assert_eq!(display_chord("shift-left"), "Shift+Left");
        assert_eq!(display_chord(""), "Unbound");
        assert_eq!(display_chord("{"), "Shift+[");
        assert_eq!(display_chord("ctrl-:"), "Ctrl+Shift+;");
        let shortcuts = Shortcuts::default();
        assert_eq!(shortcuts.search("Shift+[")[0].definition.id, "brush-softer");
        assert_eq!(shortcuts.search("{")[0].definition.id, "brush-softer");
        assert_eq!(shortcuts.search("Ctrl++")[0].definition.id, "zoom-in-plus");
    }
}
