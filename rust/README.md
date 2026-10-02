# Omuse for Linux

Omuse is a native Rust/GPUI image editor for Linux, with direct Omarchy theme
integration. This guide is for building and contributing to the application.
For normal use on Arch/Omarchy, run the [one-command installer](../README.md#install):
it prepares dependencies and builds the tested application revision locally. The
first installation compiles the application and takes longer than an update.

See the [feature and verification matrix](../docs/rust-rewrite-status.md),
[document compatibility contract](../docs/omuse-rename.md), and
[source acknowledgements](../docs/source-provenance.md). Earlier application
versions remain in Git history for format comparison and compatibility work.

## Build and run

Use a Linux Rust development environment with a C/C++ toolchain, `pkg-config`, Wayland development libraries, libxkbcommon including its X11 library, Fontconfig, and LittleCMS 2 development libraries. A working desktop graphics driver and Wayland or X11 session are required to open the actual window. File browsing uses desktop portals where available; the application also accepts explicit paths.

The development build has been exercised with Rust 1.98.0 on Arch/Omarchy. The locked `cosmic-text` dependency declares Rust 1.89 as its minimum; the complete application's minimum supported Rust version has not been established on older toolchains. Use the working development toolchain or validate another toolchain with the test script.

From the repository root:

```sh
scripts/build-rust.sh
cargo run --manifest-path rust/Cargo.toml --release --locked
# Open a project copy directly:
cargo run --manifest-path rust/Cargo.toml --release --locked -- /path/to/Artwork.omuse
```

The scripts default to two build jobs. Set `CARGO_BUILD_JOBS` to change this. Cargo's optional `CARGO_TARGET_DIR` is honored without embedding a machine-specific cache path. With Cargo's default target directory, the executable is `rust/target/release/omuse`. A clean build downloads and compiles GPUI's substantial dependency graph; incremental builds reuse it.

The build uses Rust and the Linux libraries listed above. The current source
tree contains one application and one development toolchain.

For an offline installation of an already built candidate, see [the runtime bundle instructions](../docs/rust-bundle.md). Bundle qualification is specific to an executable and this Omarchy/Arch x86_64 host; it does not establish compatibility with every Linux distribution. The [rename checkpoint](../docs/omuse-rename.md) records verification of the Omuse identity.

## Build on Windows (in development)

Windows support is being brought up in stages and is not a release target.
On Windows 11 x86_64 the editor opens, edits, saves and reopens canvases and
Create collections, keeps settings and shortcuts, runs recovery, develops
Camera RAW, selects subjects locally and exports MP4/GIF motion. Ask Omuse does
not work there yet. Linux remains the reference platform, and Omarchy theme
following has no Windows equivalent; the window uses the built-in Tokyo Night
theme. Tablet pressure is Linux-only.

Settings are stored in `%APPDATA%\omuse`; data, recovery and state are in
`%LOCALAPPDATA%\omuse`. Set the `XDG_*` variables to override either, as the
tests do. Windows has no atomic directory exchange, so saving over a project
first moves the previous package to a hidden `.<name>.previous-<id>` folder
beside it and then publishes the new one. If Omuse stops between those two
renames, the previous project is complete under the hidden name.

Install [Rust with rustup](https://rustup.rs) and Visual Studio Build Tools with
**Desktop development with C++** (MSVC and a Windows SDK). Rustup selects Rust
1.98.0 from `rust-toolchain.toml`. LittleCMS is compiled from the source bundled
with the `lcms2-sys` crate, and `.cargo/config.toml` links the C runtime
statically, so `omuse.exe` needs no Visual C++ Redistributable.

From the repository root in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-rust.ps1
# Optional Camera RAW and subject-selection assets in rust\runtime:
powershell -ExecutionPolicy Bypass -File scripts\prepare-rust-assets.ps1
rust\target\release\omuse.exe
```

`prepare-rust-assets.ps1` downloads the pinned u2netp model and ONNX Runtime
1.23.2 Windows archive, verifies their SHA-256 digests, and builds LibRaw 0.22.2
from its pinned source with MSVC and a static C runtime. The editor looks for
`lib\libraw.dll`, `lib\onnxruntime.dll` and `models\u2netp.onnx` beside the
executable; `OMUSE_LIBRAW`, `OMUSE_ONNX_RUNTIME` and `OMUSE_SUBJECT_MODEL`
point a development build at `rust\runtime` instead. Microsoft's
`onnxruntime.dll` needs the Visual C++ 2015-2022 Redistributable. Motion export
finds `ffmpeg.exe` and `ffprobe.exe` on `PATH`, for example after
`winget install Gyan.FFmpeg`.

`omuse.exe` is a Windows GUI program, so opening it shows no console window.
Command-line modes print to the terminal that started them; pipe them so
PowerShell waits, for example `rust\target\release\omuse.exe --help | Out-Host`.

To package a committed build with its runtime assets and notices:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\package-rust.ps1 -Binary rust\target\release\omuse.exe
```

The `windows` job in `.github/workflows/rust-validation.yml` prepares the
runtime assets, installs pinned FFmpeg 9.0.2, runs the test suite, the editing
self-test and the Create, template catalog and motion journeys, then keeps the
unsigned package zip as a 14-day workflow artifact. The package carries a
Windows dependency inventory; its remaining licence findings are recorded in
[the dependency notice review](../docs/rust-license-findings.md). Put platform
differences behind `cfg` attributes so Linux behaviour and file formats stay
unchanged.

## Omarchy integration

The application uses [gpui-omarchy](https://github.com/huacnlee/gpui-omarchy) directly for themed controls and surfaces. `gpui_omarchy::init` loads the current theme and installs the upstream theme watcher. Buttons, text inputs, the shared color picker, focus behavior, and application colors use this common theme source.

Dependencies are pinned as follows:

- `gpui-kit = 0.6.6` exactly.
- `gpui-omarchy` Git revision `3625c6cb5f02ec38a970939e52cfdb0c374c7791`.
- `gpui-pre` and `gpui-pre-linux` 0.3.6 are vendored under `rust/vendor` with a small Wayland tablet-v2 extension; their Apache license and patch provenance are in `rust/licenses`.
- The remaining resolved graph is tracked in `Cargo.lock`; normal build/test commands use `--locked`.

The library follows the current Omarchy theme and falls back to Tokyo Night if a valid theme is unavailable. Theme changes alter application presentation, not artwork pixels or the transparency checkerboard. The application does not change desktop theme files.

A [Sunset Muse launch screen](../docs/branding/omuse-splash.md) paints before background document preparation, with a subtle animated accent and a short fade into the editor. It respects reduced motion and adds no minimum loading delay.

The [Studio interface](../docs/rust-studio-design.md) provides a compact icon tool dock, contextual options, layer thumbnails and four inspector tabs: Layers, Develop, Select and Canvas. The top-right panel button expands the canvas by hiding the inspector. Hover over an icon for its action and current shortcut; the keyboard button in the status bar opens shortcut settings.

The Select tab includes [luminosity and colour ranges](../docs/rust-range-masks.md): sample a colour or tonal interval, preview soft coverage, then create/combine a selection or replace a layer mask. Cancel preserves the document; Apply has undo support. Generated layer masks remain editable after saving and reopening.

The Develop and Select inspectors also expose [advanced editing workspaces](../docs/rust-advanced-workflows.md): editable filter stacks, source/backdrop Blend If, 16-bit masters and PNG/TIFF export, working/display colour controls, embedded and linked sources, editable RAW, Bézier paths and vector masks, retouch layers, controlled removal, brush dynamics, mesh/pin warp, recipes and multi-image merges. That guide states each workflow's limits. Physical tablet and calibrated-display qualification remain outstanding.

The [Create workspace](../docs/omuse-create-guide.md) adds branded content, carousels, editable templates, CSV variants, reusable components, local assets and motion exports. Ask Omuse controls supported subscription connections and reviews reversible drafts inside the app. The [Create and AI plan](../docs/omuse-create-and-ai-plan.md) distinguishes source coverage from verified capabilities and outstanding release gates. Calendars and scheduling are excluded.

## Editing and files

New documents start with a transparent paint layer. Use Brush, Pencil, Eraser, Fill, Gradient, selections, layers and groups, masks, image imports, live adjustments/effects, editable text and shapes, retouch strokes, Camera Raw, local subject masks, and selection transforms. `Ctrl+S` saves an editable `.omuse` package for both single canvases and Create collections; `Ctrl+Alt+Shift+S` exports PNG, JPEG, WebP, or TIFF. JPEG is flattened against the chosen matte because it cannot store transparency. Clone and Heal use Alt-click to choose their source.

Use Type (`T`) to click existing text, click empty canvas for point text, or drag a paragraph box. Alt-click starts new text even over another text layer. Type directly in the canvas-anchored editor, including multiple lines; Ctrl+Enter or Apply commits one document undo step, Escape cancels, and Ctrl+S commits before saving. Clicking outside applies without also activating the control underneath. Empty new drafts create no layer. Edit Object still provides detailed font, spacing, alignment and color settings. Text edits retain scale, rotation, flips and the transformed upper-left anchor instead of compressing longer text into its old bounds.

Tool Settings provides exact brush diameter, hardness, opacity and smoothing, fill tolerance, wand tolerance/sample size/contiguous/all-layer options, and aligned/all-layer clone sampling. Selection modes are Replace, Add and Subtract; hold Shift to add or Alt to subtract during a selection gesture. Alt-click resets the Clone/Heal source; aligned cloning retains its source-to-target offset across separate strokes.

Drag a gradient to preview it, choose linear/radial, foreground-to-background or foreground-to-transparent, reverse and opacity, then Apply or Cancel. Escape cancels the preview without altering the document. The two shared color pickers set foreground/background; D restores black/white and X swaps them.

Image Size changes pixel dimensions, document DPI and interpolation together. Resizing bakes live text and shapes, preserves off-canvas layer extents and independently positioned mask assets, scales guides, and is undoable. Changing only DPI preserves live objects. Export DPI can be chosen independently without changing the working document.

Connected subject selects the detected foreground component under a click using the wand's active/all-layer sampling choice. It runs locally and Escape discards a pending result. Touching subjects may be joined because U2NETP supplies a foreground matte rather than Apple's per-instance labels. Use Select Subject and Refine for soft edges.

Press `Ctrl+K` to search and execute commands by name, category or current binding. There are 171 catalogued commands and 101 default shortcuts, with Photoshop-inspired tools, photo adjustments, layers, brush controls and navigation. `Ctrl+Alt+K` records, clears and restores custom shortcuts; the [generated keyboard reference](../docs/keyboard-shortcuts.md) includes every command and gesture. Tooltips show the effective binding after customization. Scroll zooms; Shift-scroll pans. Native file dialogs have an explicit path-entry fallback. New windows request a maximized desktop view, with an 800×600 minimum, so dense tiling does not leave the canvas unusable.

An `.omuse` project is a directory package identified by its manifest: `manifest.json` for a single canvas, `project.json` for a Create collection. Copy the whole directory. Layer manifests retain the `com.compositor.project` wire identifier, including live text/shape source records, adjustment layers, effects, groups, transforms, opacity, blend modes, linked/unlinked masks and live clipping sources. Unknown metadata is preserved; malformed or unsupported visual semantics fail explicitly. Live text and shapes stay editable until you choose Rasterize or resize the entire image. Layered 8-bit RGB PSD import preserves supported records and retains cached pixels with conversion notes for selected unsupported objects.

Legacy `.comp` projects still open; the first UI Save offers an `.omuse` copy and keeps the original. New collection saves use schema version 2 with nested `.omuse` pages/components. Version 1 collections can be read, but saving upgrades them; earlier releases cannot open version 2. Choose Save As to a new location when retaining an older-release copy. See the [compatibility contract](../docs/omuse-rename.md).

Camera RAW and local subject tools require the optional assets described in [runtime setup](../docs/rust-runtime-assets.md). Run `scripts/prepare-rust-assets.sh` before installation to include LibRaw, ONNX Runtime and U2NETP. Your images are processed locally. Camera Raw is also available as an adjustment to ordinary raster layers; preview/cancel does not change source pixels. These Linux backends are not pixel-identical substitutes for Apple RAW/Vision.

Recovery uses a separate application data location under
`$XDG_DATA_HOME/omuse/recovery` (normally
`~/.local/share/omuse/recovery`). Settings use `$XDG_CONFIG_HOME/omuse`.
Recovery copies do not overwrite the user's saved project. Save remains the way
to choose a durable project location. Existing settings, shortcut, brush and
recovery locations are migrated or read through the compatibility paths
described in the [rename contract](../docs/omuse-rename.md); legacy `.comp`
documents remain readable without renaming them first.

## Test

```sh
scripts/test-rust.sh
# Preserve generated test documents and journey results in the printed temp directory:
OMUSE_TEST_KEEP=1 scripts/test-rust.sh
# Run the machine-dependent CPU timing harness explicitly:
cargo test --manifest-path rust/Cargo.toml --release --locked --test raster_benchmark -- --ignored --nocapture
```

The test script checks formatting, runs domain and headless GPUI tests with `--features ui-test`, then executes a synthetic draw/undo/save/reopen/export journey and a six-slide Create exercise including PDF, MP4 and GIF. Install FFmpeg to run the complete gate. It creates fresh temporary XDG directories and uses disposable test projects, leaving the user's theme and existing artwork unchanged. `HOME` is not changed.

Headless GPUI interaction tests exercise real element hit-testing and event dispatch, but they do not substitute for physical tablet, driver, clipboard portal, file chooser, mixed-DPI, or monitor testing. The [save/recovery qualification harness](../docs/rust-release-qualification.md) adds bounded repeated saves and interruption checks using generated artwork. Final verified test counts and hardware observations belong in the release evidence, not in a guessed parity claim. The dedicated Rust workflow checks the locked build and headless suite on Ubuntu; its [first published run passed](https://github.com/Sugata-Software/Omuse/actions/runs/36426033617). It does not certify native display behavior or optional backends. See [release gates](../docs/rust-release-gates.md).

## Source map

| Module | Responsibility |
| --- | --- |
| `model.rs` / `shared_image.rs` | Bounded document and layer model, copy-on-write straight-alpha RGBA pixels |
| `document.rs` | Single-canvas `.omuse` packages, legacy `.comp` reads, bounded imports, validation and metadata preservation |
| `editor.rs` | Tool operations, selection, layers, masks, transactions and bounded undo/redo |
| `raster.rs` | CPU reference compositing, pass-through folders, transforms, blend modes and exports |
| `filters.rs` | Validated native image adjustments and filters |
| `advanced.rs`, `advanced16.rs`, `advanced_ops.rs`, `editor_advanced.rs` | Retained source recipes, bounded operations and editor transactions |
| `precision.rs`, `raster16.rs`, `proofing.rs` | Tiled 16-bit masters, document compositing/export and display-only ICC proofing |
| `smart_source.rs`, `vector_path.rs`, `refinement.rs` | Embedded/linked sources, editable paths and selection edge correction |
| `brush_dynamics.rs`, `editor_dynamics.rs`, `tablet_ui.rs` | Custom brush stamps, pressure/tilt and Wayland stylus adapter |
| `recipes.rs`, `multiframe.rs` | Portable batch recipes and bounded multi-image processing |
| `advanced_ui.rs`, `workflow_ui.rs`, `vector_ui.rs` | Advanced editing workspaces |
| `objects.rs` | Editable source-compatible text/shapes and cached rasterization |
| `camera_raw.rs` | Camera Raw settings and ordered development pipeline |
| `effects.rs` | Non-destructive layer effects and adjustments |
| `segmentation.rs`, `matte.rs` | Local ONNX inference and bounded guided mask refinement |
| `raw_import.rs`, `psd.rs`, `color_management.rs` | Camera RAW, layered PSD and ICC conversion |
| `ui.rs` | Native GPUI window, controls, gestures, dialogs and image presentation |
| `startup.rs` | First-frame splash, background document preparation and accessible handoff |
| `recovery.rs` | Separate background recovery snapshots |
| `create_project.rs`, `create.rs`, `create_history.rs`, `save_guard.rs` | Multi-page `.omuse` packages, version 1 migration, brand/layout operations, collection undo and guarded saves |
| `content_export.rs`, `social_preview.rs`, `motion.rs`, `restoration.rs` | Content packs, preflight, local video export and reversible restoration |
| `ai/`, `ai_ui.rs`, `ai_edits.rs`, `ai_history.rs` | Official subscription runtimes, staged artwork, review and retained provenance |

GPUI presents the interface and canvas through its graphics backend. Image compositing and editing currently execute on the CPU. GPU presentation should not be confused with GPU-accelerated image filters or tiled large-document processing.

## Install

```sh
scripts/build-rust.sh
scripts/install-rust.sh
# Or pass an already-built executable explicitly:
scripts/install-rust.sh /path/to/omuse
```

The per-user desktop entry is **Omuse** and the terminal command is `omuse`.
The executable is installed under `~/.local/opt/omuse`, with the launcher at
`~/.local/bin/omuse`. Fresh installations expose only this Omuse command.
An existing legacy command may remain as an upgrade compatibility alias.
The public installer supports updates and rollback; use the
[installation guide](../README.md#install) for the supported commands.

The native synthetic test runs in a separate process and uses only generated
artwork. On this Hyprland machine, to record a verified window capture:

```sh
python3 scripts/native-rust-check.py rust/target/release/omuse \
  rust/evidence/native-check --capture
```

Use a fresh evidence directory for each run. The test waits for its own window,
focuses only the matching PID, exercises native pointer/action dispatch and
records save/reopen and theme invariants. `--small-window` also resizes the
owned test window for visual inspection. Screenshots can include desktop
notification overlays; those are not part of the application UI.


For Omarchy's Lua-based Hyprland configuration, an image editor should remain
opaque even when the desktop applies translucent windows. This machine uses
this per-application rule in `~/.config/hypr/hyprland.lua`:

```lua
o.window("^omuse$", {
  tag = "-default-opacity",
  opacity = "1 override 1 override 1 override",
  maximize = true,
})
```

The maximization rule is needed on this host because Omarchy suppresses applications’ maximize requests by default. It applies when new Omuse windows open. This follows the [current Hyprland window-rule documentation](https://wiki.hypr.land/Configuring/Basics/Window-Rules/).
The installer does not modify desktop configuration on other machines. Check
the installed Hyprland version before adopting the rule; after editing Lua,
run `hyprctl reload` and `hyprctl configerrors`.

The parity pass also adds multi-layer Group/Duplicate/Delete/Nudge and drag/drop. Drop onto a group to nest, or use the narrow regions above/below a row to reorder; hold Alt to copy. Mask badges and individual effect rows copy onto another layer. Right-click keeps an existing multi-selection, and **Move into group** asks for its destination.

The Type tool places text where you click; its dialog searches installed font families. Tool Settings controls new shape radius and line width. Canvas Size offers nine anchors, relative dimensions, physical/percentage units and extension colors; Trim supports transparent or corner-color borders and individual sides. Text/shape/effect color dialogs keep the foreground color unchanged.
