# Omuse changelog

User-facing changes are grouped by release. Version numbers belong to Omuse;
they do not follow Compositor or OmaPhoto. See the [release policy](docs/releases/README.md)
for numbering, qualification and publication.

## Unreleased

### Documentation

- Track OmaPhoto's whole repository, including unreleased commits, pull requests,
  issue reports, tests and packaging, in the [maintained comparison](docs/omaphoto-comparison.md).
- Establish versioned release notes and automated publication from reviewed
  notes tied to an exact tested public source revision.
- Correct the project-format guide to identify the current version-9 writer.

No application changes have been added after the 0.1.0 runtime in this documentation pass.
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
