# Omuse changelog

User-facing changes are grouped by release. Version numbers belong to Omuse;
they do not follow Compositor or OmaPhoto. See the [release policy](docs/releases/README.md)
for numbering, qualification and publication.

## Unreleased

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

Planned editor work belongs in the [project guide](docs/project-guide.md).

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
