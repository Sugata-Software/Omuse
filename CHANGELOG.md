# Omuse changelog

User-facing changes are grouped by release. Version numbers belong to Omuse;
they do not follow Compositor or OmaPhoto. See the [release policy](docs/releases/README.md)
for numbering, qualification and publication.

## Unreleased

Planned editor work and remaining release gates are tracked in the
[project guide](docs/project-guide.md).

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
