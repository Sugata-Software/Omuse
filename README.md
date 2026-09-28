# Omuse

<p align="center">
  <img src="rust/assets/omuse.svg" alt="Omuse logo" width="144">
</p>

[![Rust validation](https://github.com/Sugata-Software/Omuse/actions/workflows/rust-validation.yml/badge.svg)](https://github.com/Sugata-Software/Omuse/actions/workflows/rust-validation.yml)

**Development preview.** Omuse is preparing for its first public release.
Source is available for evaluation; a supported public binary release has not
been qualified yet. See the [release-readiness checklist](docs/public-release-readiness.md)
for the remaining gates and the evidence behind this status.

Omuse is a native image editor for Linux, built in Rust with GPUI and designed
to feel at home on Omarchy. It combines a compact studio interface with layered
editing, painting, selections, live adjustments, editable text and shapes,
Camera RAW support, colour-managed export, and advanced non-destructive
workflows. The Create workspace adds editable branded pages, templates,
carousels, content packs and motion, with an optional in-app AI assistant.

The application follows the current Omarchy theme through
[`gpui-omarchy`](https://github.com/huacnlee/gpui-omarchy), while keeping artwork
colours and the transparency checkerboard independent of desktop styling.
Editing and local subject selection run on your machine. Optional AI requests
send the selected context to the connection you choose; the assistant identifies
the subscription route and its usage. Separate API billing is currently disabled.
Omuse does not change the desktop theme.

![Omuse editing a photograph with live exposure and colour-balance layers](docs/media/omuse-photo-editor.png)

*Actual native preview on Omarchy. [Image credit](docs/media/README.md).*

## Highlights

- Layers, groups, masks, clipping, blend modes, live adjustments and effects.
- Brush, Pencil, Eraser, Fill, Gradient, Clone, Heal and selection tools.
- Editable text and shapes, Bézier paths, vector masks and transform workflows.
- Layered `.comp` documents and packaged multi-page `.omuse` projects.
- Brand kits, 20 editable templates, rich text, reusable components, image
  frames, CSV variants and a searchable local asset library.
- Social previews, ordered raster/PDF content packs, captions and image descriptions.
- Layer and page animation, MP4/GIF export, audio, editable subtitles and clip tools.
- Optional subscription-aware AI drafts with review, protected-area compositing,
  provenance and Undo; provider availability is qualified separately.
- PNG, JPEG, WebP and TIFF export, including retained 16-bit workflows.
- Layered PSD import, Camera RAW development and local subject selection within
  their documented limits.
- Omarchy-aware controls, colours, theme watching and native Linux file dialogs.

The categorized [project guide](docs/project-guide.md) tracks current features,
planned work and release blockers, with a static visual snapshot for the
conversation. The [Rust rewrite status](docs/rust-rewrite-status.md) has the
detailed parity record. Advanced workflows and their
limits are described in [Advanced workflows](docs/rust-advanced-workflows.md).
The [Create guide](docs/omuse-create-guide.md) explains the new workflows; the
[Create and AI coverage map](docs/omuse-create-and-ai-plan.md) separates source
coverage from completed release qualification. The [real-photo qualification
report](docs/photo-release-qualification.md) records independent photo checks,
RAW/precision corrections and measured large-document limits. The [photo
editing hardening report](docs/photo-editing-hardening.md) records the earlier
editing fixes and installed-preview checks. The earlier [Create
qualification record](docs/omuse-create-qualification.md) covers that workflow's
initial qualification; [release readiness](docs/public-release-readiness.md)
tracks the remaining public-release gates. Calendars and scheduling are outside
Omuse's scope.

## Build and run

Omuse requires a Linux Rust development environment, a C/C++ toolchain,
`pkg-config`, Wayland development libraries, libxkbcommon with X11 support,
Fontconfig and LittleCMS 2 development libraries. A working Wayland or X11
desktop graphics stack is required to open the application window.
Motion export and clip tools require FFmpeg. Subscription AI uses a supported,
signed-in official provider runtime; an API key is not required for local editing.

From the repository root:

```sh
git clone https://github.com/Sugata-Software/Omuse.git
cd Omuse
scripts/build-rust.sh
rust/target/release/omuse
```

Open a project directly:

```sh
rust/target/release/omuse /path/to/Artwork.comp
```

For development through Cargo:

```sh
cargo run --manifest-path rust/Cargo.toml --release --locked
```

See the [complete Rust build, test and development guide](rust/README.md) for
toolchain details, optional runtime assets, native checks and the source map.

## Install

Build and install the per-user application:

```sh
scripts/build-rust.sh
scripts/install-rust.sh
```

The installer places the application at `~/.local/opt/omuse/omuse`, creates the
`~/.local/bin/omuse` command and installs an **Omuse** desktop entry. Optional
Camera RAW and subject-selection assets can be prepared before installation:

```sh
scripts/prepare-rust-assets.sh
scripts/install-rust.sh rust/target/release/omuse
```

For transition compatibility, installation also provides the previous terminal
command as an alias to `omuse`; new documentation and automation should use the
canonical command.

For offline packaging and isolated installation, see the
[runtime bundle guide](docs/rust-bundle.md).

## Projects and user data

Omuse retains the existing `.comp` project format and its format identifiers so
projects remain portable across compatible implementations. Existing projects
are opened in place only when requested; recovery snapshots do not overwrite a
saved project.
Multi-page Create projects use `.omuse` packages, retaining native objects,
brand definitions and referenced assets. Their additional collection metadata
is specific to Omuse.

New settings and application data use the XDG locations
`$XDG_CONFIG_HOME/omuse` and `$XDG_DATA_HOME/omuse` (normally
`~/.config/omuse` and `~/.local/share/omuse`). The rename keeps compatibility
with earlier settings, shortcuts, brushes and recovery data. See the
[rename compatibility contract](docs/omuse-rename.md) for the migration rules
and the older names that remain intentionally in the repository.

## Test and release evidence

Run the local Rust checks with:

```sh
scripts/test-rust.sh
```

The complete test journey includes MP4/GIF export and requires FFmpeg.

Automated headless checks do not replace native display, tablet, portal,
mixed-DPI or optional-backend qualification. The required evidence is listed in
[Rust release gates](docs/rust-release-gates.md) and
[save/recovery qualification](docs/rust-release-qualification.md).

## History and attribution

Omuse is developed by [Sugata Software](https://github.com/Sugata-Software).
Contributions and carefully scoped bug reports are welcome; start with
[CONTRIBUTING.md](CONTRIBUTING.md). Use copies of important artwork while
evaluating this preview.

Omuse grew from the open-source Compositor project. Its original Swift/Qt
README, build notes and release instructions are preserved verbatim in
[the legacy Compositor README](docs/legacy-compositor-readme.md). The legacy
implementation, source identifiers and license notices remain in the tree for
compatibility, attribution and reference; they are not the active Omuse build
path.

The public repository starts from a source snapshot. Its origin and retained
third-party notices are described in [source provenance](docs/source-provenance.md).

## License

Application source: MIT — see [LICENSE](LICENSE), including the retained
original copyright notice. Third-party code, fonts, images and optional runtime
assets retain their own licences. Runtime notices are under `rust/licenses/`;
the [dependency notice review](docs/rust-license-findings.md) records unresolved
binary-distribution findings. The source licence does not relicense third-party
media.
