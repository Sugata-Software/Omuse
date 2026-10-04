# Omuse changelog

User-facing changes are grouped by release. Version numbers belong to Omuse;
they do not follow Compositor or OmaPhoto. See the [release policy](docs/releases/README.md)
for numbering, qualification and publication.

## Unreleased

No changes recorded yet.

## [0.9.0](https://github.com/Sugata-Software/Omuse/releases/tag/v0.9.0) — 2026-10-04

**Photo detail, precise control · Linux / Omarchy source pre-release · unsigned experimental Windows**

- Reconstruct new controlled-removal results from surrounding, unselected
  context inward, using only the allowed sampling area. Refuse an edit without
  enough donor context before committing artwork. Saved recipes record the
  algorithm; older recipes keep their original behavior. Omuse 0.8 can display
  a saved result, but editing or resaving its new recipe there can lose the
  algorithm choice: preserve the original and use a separate rasterized copy
  for backward editing.
- Preserve a typed numeric value when moving focus to another control, while
  still rejecting invalid values or edits whose target has changed.
- Preserve a valid long stroke when mouse-up repeats its last recorded point;
  refuse an over-limit path explicitly before altering artwork.
- Retouch the source pixels of scaled, rotated or flipped photo layers with
  Blur, Smudge and Liquify, retaining the layer's pixel dimensions and placement.
  Soft selections limit the edited area. Mask retouch retains the mask's extent
  and outside coverage. These tools require an 8-bit raster layer or an existing
  mask and are bounded to 16 MP; oversized work and recoverable buffer-allocation
  failures leave the edit unapplied.
- Set Blur's **Radius px** independently of brush size, from **0.25–128 canvas
  pixels**. Blur preserves source alpha; Smudge carries colour along the stroke,
  and Liquify samples the original pixels through the completed displacement.
- Configure major grid spacing and minor subdivisions in **Canvas → View &
  alignment**. Grid drawing and snapping share the chosen spacing; dense lines
  simplify at low zoom. Hold Shift to bypass grid snapping for a gesture.
- Alt-click a **MASK** badge, or run **Inspect layer mask**, for a temporary
  grayscale inspection view. Return to the artwork before painting the mask;
  inspection adds no Undo step and does not change exported artwork.
- Release an ordinary layer folder with **Ungroup folder**, retaining child
  order, editable contents and placement in one Undo step. The command refuses
  folders with their own appearance settings, locks or clipping relationships
  that would change the result. **Ctrl+Shift+G** still ungroups vector objects.
- Keep inline text Apply/Cancel within the compact panel and wrap its keyboard
  hint at the minimum supported window size.
- Preview installed fonts on selected letters in the canvas text editor, search
  font names, commit with Enter or restore the original with Escape. With a caret,
  choose the font for subsequent typing without restyling the existing text.
  Long font lists retain readable row heights while scrolling.

Qualification passed 1,405 Linux and 1,387 Windows application tests (four
ignored on each), final native compact controls and installed-launcher journeys,
and exact cross-version package/export checks. See the
[release notes](docs/releases/v0.9.0.md) and [limits](docs/release-090-qualification.md).

## Download publication — 2026-10-04

The reviewed 0.8.0 Linux and experimental Windows archives were published with
permanent checksums and completed anonymous-download verification. This was a
publication update, separate from the 0.9 editing changes above.

## [0.8.0](https://github.com/Sugata-Software/Omuse/releases/tag/v0.8.0) — 2026-10-04

**Colour, curves and control · Linux / Omarchy source pre-release · unsigned experimental Windows**

- Build and use the editor on Windows 11 x86_64 from source with
  `scripts/build-rust.ps1`: open, edit, save and reopen canvases and Create
  collections, with settings in `%APPDATA%\omuse`, data and recovery in
  `%LOCALAPPDATA%\omuse`, and save/recovery locks. Camera RAW, local subject
  selection and MP4/GIF motion export work with `scripts/prepare-rust-assets.ps1`
  and FFmpeg on `PATH`, including lossy and deflate-compressed DNG files;
  `omuse.exe` opens without a console window, carries the Omuse icon and needs
  no Visual C++ Redistributable for the core editor. The optional ONNX subject
  runtime still needs its documented Visual C++ runtime. The window follows
  the Windows light or dark app mode, save dialogs start in the Pictures or
  Videos folder (also when redirected into OneDrive), long paths are supported,
  and saves wait for sync clients and scanners that briefly hold project files.
  CI runs the test suite and editing and motion journeys on Windows and keeps
  an unsigned package zip. This is an
  unsigned experimental build. Ask Omuse runs on Windows with Codex
  (including npm installs) and native Claude Code, using workspaces only the
  current user can open. The Windows port uses the same project formats as
  Linux. Native desktop and live-provider qualification remain separate from CI.

- Prevent Image Trace from panicking at image edges in development builds
  with integer overflow checks enabled.

- Select several vector objects on the main canvas with Shift-click or a
  marquee. Move, nudge, duplicate, group, ungroup and change their shared style;
  align, distribute, rotate and scale through the Layers inspector or command
  search. Nested groups retain their editable children and stacking order.
- Construct filled shapes with Unite, Subtract, Intersect, Exclude and Divide.
  Operations run in the background, preserve Undo and refuse unsupported open
  or stroke-only inputs. Curves become bounded polygonal paths in the result.
- Add editable linear/radial fills with 2–16 colour/alpha stops, geometry,
  Pad/Repeat/Reflect spread and gradient fitting. Strokes gain butt/round/square
  caps, miter/round/bevel joins, dash/gap patterns, offsets and miter limits.
  Controls apply in the shared Layers inspector; variable widths, stroke
  expansion, pattern and mesh fills remain planned.
- Import and export multiple SVG objects with supported gradients and stroke
  settings, names, organizational groups, transforms and object opacity.
  Main-canvas import appends to existing artwork. SVG text import, clipping,
  group opacity, paint-server strokes and external resources remain unsupported.
- Create editable text along a single curve in the Layers inspector, with font,
  size, tracking, start position, alignment and reverse direction. Retain the
  source curve, update text from a separately edited curve, or explicitly
  convert the text to glyph outlines. Saved outlines preserve appearance;
  subsequent text edits can use available fallback fonts. SVG/PDF export keeps
  the outlines, not an editable text recipe.
- Export the active vector artwork or legacy path as a vector PDF at document
  DPI. The bounded exporter preserves supported paths, transparency and Pad
  gradients, and refuses Repeat/Reflect rather than changing their appearance.
  It does not export the complete mixed photo composition, embed editable text,
  import PDF/AI files, or establish print-ready CMYK output.
- Find objects with matching fill, stroke or opacity; toggle an outline view
  with Ctrl+Y. Supported simple documents rerender vector editing previews at
  up to 4× resolution within a 16-million-pixel preview budget.
- Preserve soft selection coverage when making reveal/hide masks, including
  fractional or rotated layer placement, and when masking editable filters.
  New group/adjustment masks retain the correct canvas extent without moving
  their children. Imported PSD masks retain their
  outside coverage. Read Photoshop Levels gamma correctly, distinguish master
  Hue/Saturation from Colorize and report unsupported selective bands; malformed
  adjustment/mask records fail explicitly.
- Read external format-11 UTF-16 font runs alongside colour runs without
  changing cached artwork on open. Invalid or split-character ranges and
  overlaps within the same run type are refused. This does not add mixed-style
  Photoshop text conversion.
- Add **Hue range** beside RGB distance in Colour range: sampled hue, tolerance,
  softness, minimum saturation and inversion, with selection, add, subtract,
  intersect or editable layer-mask output. The saved mask retains pixels;
  the hue settings are not a persistent mask-generation recipe.
  The preview shows the combined selection for Add/Subtract/Intersect and
  refuses a draft if the underlying selection has changed.
- Inspect the actual encoded JPEG with **Fit** and **100%** views, drag/arrow
  panning, quality, DPI and matte controls. Preview stays bounded to 16 MP and
  does not change the working image. Compact controls keep the preview and
  export actions visible at the minimum supported window size.
- Restore editor keyboard focus after applying Camera Raw or a refined subject
  selection/mask, so **Ctrl+Z / Ctrl+Shift+Z** work immediately. A cancelled or
  failed background operation does not steal focus from the current control.
- Preserve exact 16-bit blend rounding when a masked edit keeps the original
  alpha, including partially transparent pixels and hidden colour values.
- Improve Blur, Smudge and Liquify strokes with fractional footprints and
  consistent spacing. Smudge carries the evolving paint; Liquify samples the
  untouched stroke source through accumulated displacement. Work limits reject
  excessive strokes without committing partial results. These remain raster
  tools, with broader real-photo quality evaluation still required.
- Add **Match reference colour** to the editable Filter stack and Precision &
  colour workspace. Load a bounded raster reference, set Amount and Preserve
  lightness, and retain the result as a reversible recipe with optional node
  masking. Alpha-weighted Oklab statistics support 8-bit and retained 16-bit
  sRGB/Display P3 evaluation; saved recipes keep statistics rather than the
  reference filename or pixels. This is global palette transfer, not semantic
  matching, camera calibration or HDR look matching.
- Save grouped scenes as canvas format **12**, gradients/advanced strokes as
  **13**, and editable text on curves as **14**. Flat legacy scenes retain
  format 11; ordinary canvases retain format 10. Formats 12–14 require this
  0.8.0 reader. Older releases also cannot read the new reference-match
  operation; use **Save As** to preserve a compatible original.

The exact-source Linux/Windows validation passed, alongside local photo,
recovery and native checks. Preview downloads have their own reviewed manifest
and publication step; interactive Windows, live Windows AI and broader hardware
qualification remain open.

See the [development qualification record](docs/vector-workflow-qualification.md)
and [workflow instructions](docs/user-guide/photo-vector.md) for the tested scope
and remaining limits.


## [0.7.0](https://github.com/Sugata-Software/Omuse/releases/tag/v0.7.0) — 2026-10-02

**Photos and vectors, one studio · Arch Linux / Omarchy · x86_64 source pre-release**

- Unify buttons and text controls around the toolbar's tighter 3-pixel corners
  across editing, Create, AI, dialogs, command search and colour pickers. Shared
  styling replaces local overrides. Compact headers retain the document's
  status indicator and full-name tooltip; long command/provider labels use
  ellipsis instead of clipping into neighbouring controls.
- See live colour swatches beside vector Fill/Stroke, brand colours, finishing
  colours and the Target colour uniformity reference. Vector swatches show
  transparency and disabled strokes, with inline invalid-hex feedback. Style
  fields use consistent inspector spacing; unavailable path actions are disabled
  and SVG import guidance matches the main canvas's Done button.
- Convert bitmap layers into editable vector artwork with **Image trace** on the
  main canvas: Logo/Illustration/Photo art presets, colour/gray/B&W modes,
  detail, smoothing, corner protection, noise, resolution and point budgets.
  Compare Source/Trace, keep or cancel, retain the original bitmap, and reopen
  saved trace settings. Keeping creates one document Undo step.
- Draw smooth Bézier points by dragging with the Pen tool. Move anchors and
  handles, use Alt to break handle alignment and Shift for angle constraints,
  click the first point to close a contour, and insert points on existing
  segments without changing the curve.
- Edit vector artwork directly on the **main canvas**, with object and style
  controls in the shared **Layers** inspector. **P** draws paths, **A** edits
  nodes and **V** moves objects; double-click in Move mode for its nodes. The photo,
  layer placement, masks and blending remain visible in the settled preview.
- Use Undo/Redo for individual vector edits while the canvas session stays open.
  **Done/Enter** keeps the session as one document Undo step; **Cancel/Escape**
  discards it. Switching editing tools/layers or saving keeps valid work before
  continuing. Shared zoom/pan, command search and shortcut help remain available.
- Build several editable paths, rectangles and ellipses in one artwork layer
  with **Shift+P**. Pick objects on the canvas or in the inspector, arrange their
  order/visibility and change fill, stroke and opacity. Valid style changes
  preview automatically. Geometry shares one derived image per artwork layer.
- Save vector artwork in **canvas format 11** with bounded, validated geometry
  and a checked display cache. Ordinary canvases continue to use format 10.
  Omuse 0.6.0 cannot open format 11; use Save As to retain an older copy.
  Pixel tools require explicit Rasterize, and replacing artwork clears its
  obsolete geometry rather than leaving an inconsistent editable object.
- Add editable **Target colour uniformity** to Filter stack: reference colour,
  hue range/falloff and independent hue, saturation and lightness strengths,
  with retained 16-bit evaluation, opacity and selection masks. New recipe
  nodes require 0.7.0; retain a Save As copy for 0.6.0.
- Exchange one styled editable SVG path through the path editor, preserving
  curves and compound holes. Import fits the SVG viewport proportionally;
  export uses a new filename and retains existing files. Unsupported artwork
  is refused explicitly. General SVG raster import remains available.
- Add corner nodes, exact curve-midpoint insertion, reverse/new subpaths and
  fill-rule controls. Fix compound-hole previews and stale path-dialog jobs;
  Apply remains one Undo step and compact controls remain scrollable.
- Use scale-aware, premultiplied Lanczos sampling for high-quality 16-bit
  transformed exports. Keep explicit Smooth and Nearest choices and exact
  identity samples; integrate reduced masks into source sampling to prevent
  masked-out colour leakage. Quality/resource limits are documented in the
  [photo/vector guide](docs/user-guide/photo-vector.md).
- Preserve hidden RGB in fully transparent editable-node blends, and check
  cancellation during retained-source promotion, colour conversion and
  high-quality sampling. Interrupted exports preserve existing destinations.

The approved long-term [photo/vector roadmap](docs/photo-vector-roadmap.md)
includes the remaining precision, masking, restoration, illustration and
interchange work. See the [release notes](docs/releases/v0.7.0.md) and
[qualification record](docs/release-070-qualification.md). This release does
not claim full Photoshop/Illustrator parity.

## [0.6.0](https://github.com/Sugata-Software/Omuse/releases/tag/v0.6.0) — 2026-10-01

**More control over every edit · Arch Linux / Omarchy · x86_64 source pre-release**

- Preview text on the artwork through the committed rendering path, colour
  selected letters, and preserve Unicode formatting while typing. Cancel keeps
  the original; applying a text draft is one Undo step.
- Search ten recent canvases and collections with **Ctrl+Alt+O** or the clock icon.
  Successful opens/saves persist across launches; Clear history leaves files
  intact. Import retains its existing **Ctrl+Shift+O** shortcut.
- Adjust brush, layer and transform values by dragging their labels, typing,
  or using arrow keys. Shift gives finer control, double-click resets, and
  Escape cancels a drag. Layer-opacity previews commit as one Undo step.
- Add ten Dither styles, explicit Bloom Glow, Vignette Overlay and Local
  Contrast with reversible previews, selection-aware Apply and Undo. These
  finishing tools target 8-bit raster copies and preserve retained masters.
- Import bounded PSB and SVG/SVGZ, choose SVG raster dimensions in Open/Import,
  and recover supported editable Photoshop text while retaining cached artwork
  for unsupported records. Import reports describe conversion limits.
- Grow painted, filled and gradient masks while preserving their placement,
  source artwork and outside coverage. New saves use **canvas format 10**;
  older Omuse versions cannot read them. Save As retains an older project copy.
  Read versions 1–10 and validate upstream UTF-16 colour runs before converting
  them into Omuse's native rich text. New files continue to use `.omuse`.
- Review changes made to a saved project by another process. Clean, idle
  projects can reload; local edits offer Keep editing, Save a copy or explicit
  discard/reload. Repeated saves coalesce the latest snapshot, and delayed
  saves/reloads preserve newer edits and unrelated dialog drafts.
- Generate selection outlines in background work with zoom-aware detail and
  bounded point counts. Exact selection masks remain unchanged. Add a 3:4 crop
  preset and explain unavailable layer actions before execution; Merge Down
  refuses source/effect bounds that would be lost outside the canvas.
- Pick neutral white balance and green/purple fringes in Camera Raw, and drag
  sampled tones or colours to adjust curves and the colour mixer. Stage-aware
  samples account for earlier grading, while cancellation restores the draft.
  Coalesced preview work retains only one active job and the newest request.

- Keep finishing choices and fixed actions usable at the minimum window size,
  including scrolling and typing into lower effect controls.
- Preserve raster Eraser and sampled Clone/Heal behavior with a transparent
  foreground colour, with exact pixels and compact reversible history.

The [qualification record](docs/release-060-qualification.md) records 1,105
application cases, exact-source CI, production desktop checks, previous-release
compatibility and complete installation rollback. See the [release notes](docs/releases/v0.6.0.md).

## [0.5.0](https://github.com/Sugata-Software/Omuse/releases/tag/v0.5.0) — 2026-09-30

**One project, one extension · Arch Linux / Omarchy · x86_64 source pre-release**

- Use `.omuse` for all new editable projects, Save/Save As defaults, recovery
  snapshots, examples and CLI instructions. Existing `.comp` projects still
  open; their first Save offers an `.omuse` copy and preserves the original.
- Confirm replacement of an existing Save As destination and check that exact
  disk version again before publication. A legacy project with an existing
  `.omuse` sibling cannot silently replace it.
- Save Create collections as schema version 2 with nested `.omuse` pages and
  components. Read version 1 collections and preserve lazy pages, resources,
  metadata and undo snapshots during atomic migration. Earlier releases cannot
  read version 2; use Save As to keep a version 1 copy when needed.
- Include single-canvas and collection `.omuse` packages in batch recipes.
  Collections use their saved active page; old `.comp` inputs remain supported.

The [qualification record](docs/release-050-qualification.md) records 970 passing
application tests, migration interoperability, native checks and complete rollback.
See the [release notes](docs/releases/v0.5.0.md).

## [0.4.0](https://github.com/Sugata-Software/Omuse/releases/tag/v0.4.0) — 2026-09-30

**Your AI, a clearer studio · Arch Linux / Omarchy · x86_64 source pre-release**

- Refine the right-hand editing, Create and AI panels with consistent widths,
  grouped sections, clearer primary actions and aligned controls. Create uses
  two rows of equal tabs, while AI tasks use a two-column grid. Connections gets
  its own full-height view and preserves the current brief and result when
  returning. Colours and focus styling continue to follow the Omarchy theme.
  Unavailable editing actions show disabled states, mask and clipping controls
  display their state, and short windows use a compact AI prompt area.
  See the [panel guide](docs/inspector-panels.md).
- Choose **Auto** or pin a subscription provider separately for each Ask Omuse
  task. Set preferred Assistant and Images roles, exclude connections from Auto,
  and see first-use routing before submitting. Pins and failed requests never
  fall back silently, and separately billed API access remains disabled.
- Optionally finish one image or image-edit request with editable layout and a
  caption/alt-text step. The bounded sequence uses one brief, at most three
  displayed subscription requests and one final review/Undo transaction. Stop,
  Local-only, stale source state or an invalid step prevents unsent work; saved
  replay guards the exact generated layer identities used by later edits.
- Name image layers created by earlier steps when reopening a saved workflow
  review, while retaining warnings for missing artwork references.
  See the [routing qualification record](docs/ai-routing-qualification.md) for
  the exact tested candidates and remaining release checks.
- Resolve Omarchy's mise-managed provider shims to the installed runtime before
  checking its identity. This fixes signed-in Codex installations appearing as
  Unverified after launching Omuse from the desktop. Wrapper, account and
  isolation checks remain enforced. Clarify first-use connection labels in the
  manual; see the [connection fix record](docs/ai-desktop-connection-fix.md).
- Recognize Claude Code's internal schema-delivery `StructuredOutput` call only
  for a requested structured assistant result. Continue rejecting every other
  tool route, require the final validated result and refuse unvalidated prose
  JSON. The preceding connection-fix candidate completed one installed Claude
  Design journey with five editable operations, Review, Keep, Undo and Redo;
  that historical live receipt retains its original runtime identity.

The [0.4.0 qualification record](docs/release-040-qualification.md) identifies
the combined runtime and test scope. See the [release notes](docs/releases/v0.4.0.md)
for update instructions, provider limits and remaining qualification work.

## [0.3.0](https://github.com/Sugata-Software/Omuse/releases/tag/v0.3.0) — 2026-09-30

**Crop, zoom and editable clipboard · Arch Linux / Omarchy · x86_64 source pre-release**

- Preview a movable crop with Free, Original, square, portrait and landscape
  ratios. Swap orientation, resize corners, nudge with arrows, and Apply or
  Cancel. Original layer pixels remain available and Apply is one undo step.
- Keep the inspected artwork point fixed when changing keyboard zoom stops,
  viewing actual pixels, or zooming with the wheel around the pointer.
- Copy and paste complete editable layer trees within the running Omuse session,
  retaining groups, type, masks, transforms, locks and advanced sources. Internal
  mask links receive fresh identities; missing dependencies and oversized
  payloads refuse before changing the clipboard or document.
- Publish image clipboard formats correctly on both Wayland and XWayland so
  other applications receive PNG instead of empty text.
- Send Wayland clipboard data in bounded nonblocking chunks so slow image
  consumers cannot hold the UI in a blocking write.
- Refuse pixel Cut when masks, opacity, live appearances or resampling would
  omit source content from the copied image. Refused pixel/mask cuts preserve
  the existing clipboard and artwork; complete editable layer Cut stays available.
- Reject delayed paste completions after the document, selection, clipboard or
  editing interaction changes. Update the manual and searchable shortcuts.

The exact public runtime passed 901 application cases and the complete GitHub
workflow, including editing/Create/motion journeys and all 80 template variants.
Production Wayland, production XWayland and the installed Wayland launcher each
passed 24 native checks. Foreground editing, exact PNG clipboard exchange and
complete 19-file rollback to 0.2.1 and back passed. See the
[qualification record](docs/editing-workflows-qualification.md) and
[release notes](docs/releases/v0.3.0.md) for identities and limits.

## [0.2.1](https://github.com/Sugata-Software/Omuse/releases/tag/v0.2.1) — 2026-09-29

**Safer AI image editing · Arch Linux / Omarchy · x86_64 source pre-release**

### AI image editing

- Protect locked artwork when building replacement, removal and background
  masks, including locked groups whose clipped layers depend on unlocked bases
  or hidden mask sources. The unlocked base remains editable outside the
  protected clipped contribution.
- Refuse canvas expansion when reevaluating a live blur, motion blur, noise or
  grain adjustment would change pixels in the original canvas. Pointwise
  adjustments and layer-local effects remain editable when the original
  translated pixels are preserved.
- Keep completed image results and retained references available through review
  and refinement if local history persistence fails, then remove their private
  workspace after its final consumer releases it. Discard still works when the
  history store is unavailable, without replacing damaged history.
- Restore same-session image results against a newly drawn selection while
  continuing to block Keep after artwork, document, project or session changes.
  Generate and Expand no longer react to unrelated selection changes.
- Restore visible shadow and reflection controls when refining a background
  result. Custom historical values that the preset controls cannot represent
  leave the current controls unchanged.
- Stop an image-variation batch when a candidate is invalid, stale, not retained
  in history or not locally qualified, rather than dispatching and relabeling a
  later request as qualified.

- Return editor focus after Apply/Remove local finishing so Ctrl+Z immediately
  undoes the artwork transaction.
- Add a practical [user manual](docs/user-guide/README.md), including local/AI
  object removal, photo editing, backgrounds, social content, motion and export.
  Correct the older Create guide's shortcut and task labels.

858 application cases and the final full GitHub workflow passed. Production
Wayland/XWayland and installed Wayland each passed 24 checks; full payload
rollback passed. Five live image operations passed on the preceding image
runtime; the final focus correction passed UI/native/Cua checks separately.
See the [qualification record](docs/ai-image-editing-qualification.md) and
[release notes](docs/releases/v0.2.1.md) for exact identities and scope.

## [0.2.0](https://github.com/Sugata-Software/Omuse/releases/tag/v0.2.0) — 2026-09-29

**Task-based Ask Omuse · Arch Linux / Omarchy · x86_64 source pre-release**

### AI creation and photo editing

- Choose a task in Ask Omuse: design, photo enhancement, captions, generation,
  replacement, removal, backgrounds or expansion. Starter briefs stay editable;
  the primary action and Ctrl+Enter always submit the selected task.
- Preview native exposure, brightness, contrast and saturation adjustments as
  editable layers. Original photo pixels remain intact; Keep is one undo step.
- See what will be shared, choose whether a design request includes the canvas,
  compare Before/After, copy captions and alt text, or refine a saved proposal.
- Keep the original task and newly selected references when refining a result.
  Photo and caption tasks reject empty or out-of-scope responses before Keep.

### Reliability

- Preserve early provider responses and enforce forbidden-operation checks even
  when messages arrive before the submission acknowledgement.
- Check connections concurrently with bounded, cancellable probes. Show Codex
  allowance snapshots when the official runtime provides them, without charging
  a separate API or consuming reset credits.
- Reject inconsistent Claude subscription evidence. Preserve AI history across
  concurrent windows, failed writes and large retained plans.
- Close Omuse from a focused text field using the configured Close shortcut;
  unsaved-work protection still applies.

### Documentation

- Track OmaPhoto's whole repository, including unreleased commits, pull requests,
  issue reports, tests and packaging, in the [maintained comparison](docs/omaphoto-comparison.md).
- Establish versioned release notes and automated publication from reviewed
  notes tied to an exact tested public source revision.
- Correct the project-format guide to identify the current version-9 writer.

The exact public runtime at `3dc3e46` passed 843 application tests, the complete
editing/Create/export journeys, 24 native checks on Wayland and 24 on XWayland,
and [GitHub Rust/installer validation](https://github.com/Sugata-Software/Omuse/actions/runs/36528288193).
Live ChatGPT-via-Codex checks used a synthetic still life; the exact runtime
passed history reopening, safe refinement, readable change review,
Before/After, Keep/Undo and Ctrl+Q. Claude, Grok and the broader image-operation
set are not live-qualified by this release. Direct API billing remains disabled.

[Full release notes and limitations](docs/releases/v0.2.0.md) ·
[AI qualification](docs/ai-experience-qualification.md)

## [0.1.0](https://github.com/Sugata-Software/Omuse/releases/tag/v0.1.0) — 2026-09-29

**First public source release · Arch Linux / Omarchy · x86_64**

- Native Rust/GPUI editor following the active Omarchy theme.
- Layered photo editing, masks, clipping, live adjustments, editable text and shapes.
- Camera Raw controls, RGB histogram, hue/saturation vectorscope and background preview processing.
- PSD import, retained 16-bit photo workflows, and local subject selection within documented limits.
- Ctrl+K search across 171 commands, 101 default shortcuts and custom shortcut recording.
- Reliable group movement, frontmost Auto Select, middle-button pan and Eraser smoothing.
- Editable multipage content, 20 templates / 80 size variants, brand resources and CSV variants.
- Raster/PDF content packs, bounded MP4/GIF exports and an optional reviewed assistant workflow.
- Staged saves and exports, recovery, and a one-command source installer with complete-generation rollback.

The runtime at `9c99e50` passed 808 application tests, editing/Create journeys,
native Wayland/XWayland checks and installed rollback. This is an early source
release; downloadable binaries and broader hardware/portability qualification
remain open. GitHub marks it as a pre-release; the app is named **Omuse**.

[Full release notes and limitations](docs/releases/v0.1.0.md) ·
[Installation](docs/install.md) · [Release evidence](docs/main-install-qualification.md)
