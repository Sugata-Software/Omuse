# Omuse changelog

User-facing changes are grouped by release. Version numbers belong to Omuse;
they do not follow Compositor or OmaPhoto. See the [release policy](docs/releases/README.md)
for numbering, qualification and publication.

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
