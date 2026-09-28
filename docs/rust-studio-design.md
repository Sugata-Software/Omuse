# Omuse Studio interface

The native Rust editor uses an artwork-first workspace with compact, consistent controls. Its shell follows the active Omarchy palette through the pinned upstream `gpui-omarchy` integration. Colour in the artwork, transparency checkerboard and exported pixels is independent of the interface palette.

## Visual language

- Use the theme's background, surface and inset to separate the options bar, panels and canvas. Dividers are derived from the theme's foreground. Selected tools and active tabs use the accent; warnings and save state use the semantic status colours.
- Keep geometry quiet: 3 px control corners, thin separators, a 48 px header, a 44 px context bar and a 30 px status bar. Most controls use 11–13 px text; numeric readouts use the theme's monospace face.
- Use one Lucide line-icon family. Every icon button has an accessible name; tool and command icons also have delayed hover labels. Shortcut labels reflect the user's current mappings.
- Preserve a useful workspace at the existing 800 × 600 minimum. All 23 tools fit in the 88 px dock. The 292 px inspector can be hidden to expand the canvas. Inspector tabs remain visible while their contents scroll.

## Workflows

The header exposes document creation, open/import, undo/redo, save/save-as and export. The contextual options bar identifies the current tool, shows its relevant controls and hints, and retains access to the shared Omarchy colour picker and detailed tool settings. Spot-healing modes remain directly selectable when that tool is active.

The inspector has four tabs:

- **Layers:** independently scrolling layer rows, visibility, cached previews, live-object type labels, compositing, masks and transforms. Group collapse, multiple selection, drag reordering/nesting and mask/effect dragging retain their existing commands.
- **Develop:** Camera Raw, pixel filters, editable adjustment layers, layer effects and clearly identified direct pixel adjustments.
- **Select:** subject/background operations, selection refinement, content fill, cropping and floating-selection controls.
- **Canvas:** dimensions and actual colour precision, view/alignment toggles, guides and background colour. Shortcut configuration is always available from the status bar's keyboard button.

Tool and tab selection return focus to the editor so keyboard commands continue to work. Existing modal focus traps and save-before-replacement guards remain in place. This is a presentation change; it does not add a general live-filter stack or higher-precision editing.

Visual QA also exposed a text-rendering defect: the project's pixel tracking values were passed directly to cosmic-text's em-based letter-spacing API. The renderer now converts pixels to ems, matching the saved text model and the original editor's kern units. A regression checks that 2 px tracking adds the same inter-glyph distance at 24 px and 72 px font sizes. Existing project raster assets are preserved on open; editing or regenerating text uses the corrected spacing.

## Layer preview ownership

Thumbnails sample at most 1,024 source pixels into a 32 × 32 aspect-fit preview. They do not clone or retain the source image. Identical preview bytes reuse the same render-image identity; changed, removed, collapsed or hidden previews retire their textures. The view releases its cache on teardown. A maximum of 256 cached previews bounds ownership; additional rows retain their layer-type icon until capacity becomes available. Sampling is nearest-neighbour, so these are navigation aids, not colour/detail proofs or composited effect previews.

## Verification

`scripts/test-rust.sh` includes deterministic minimum-window interaction tests for every tool, inspector commands, modal cancellation, focus, light/dark theme geometry, artwork preservation and inspector visibility. The normal editing, recovery, document and renderer tests remain part of the same gate.

`scripts/native-rust-check.py` runs the existing synthetic Wayland journey and captures only its own identified window. `--minimum-window` runs the journey at 800 × 600; `--theme light` or `--theme dark` changes only that disposable process. Inspector captures use `--panel layers|develop|selection|canvas`; the existing Camera Raw and text inspection modes remain available. Native tests dispatch GPUI events; they do not prove physical tablet input or Cua input-driver operation.

`cargo run --manifest-path rust/Cargo.toml --release --example studio_preview -- NEW_DIRECTORY` generates an original layered illustration for visual inspection. The example saves/reopens the project and checks composite pixels. It is a QA fixture, not user artwork or a shipped editing feature.

Icons are bundled offline. Lucide and its Feather-derived icon notices are retained in `rust/assets/studio-icons/LICENSE-LUCIDE` and included in installations through `rust/licenses/LUCIDE-LICENSE.txt`.

## Verified installation — 27 September 2026

This is a fixed pre-rename evidence record. Its product name, launcher paths,
hashes and filenames are retained exactly as observed at that checkpoint.

The production release build is installed through the separate `~/.local/bin/compositor-rust` launcher. Its SHA-256 is `0ca15d3bd1f3efbe1ce7bf8ab7f5a18300a7a9ff1328100a99b4a25206b3d6e7`, matching the candidate used for native validation. The prior installed executable is preserved as `~/.local/opt/compositor-rust/compositor-rust.fb0c9ce` and `.previous`. The desktop entry validates, and the installed Lucide notice matches the repository copy.

- The full Rust gate passed 353 tests, with three benchmark tests intentionally ignored. Real RAW, layered PSD, unsupported PSD-depth and local U2NETP fixtures were configured. The disposable editing/save/reopen/export journey also passed. Evidence: `rust/evidence/studio-final-validation.log`.
- Seven installer tests passed. Evidence: `rust/evidence/studio-installer-tests.log`.
- Both 13-check native Wayland journeys passed: a light Layers layout at 800 × 600 and a dark Develop layout at 1776 × 1075 logical pixels. Both window captures were visually inspected. Evidence: `rust/evidence/studio-native-light-20260927/` and `rust/evidence/studio-native-develop-20260927/`.
- The native display comparison passed across 30 captures and five zoom/pan cases on this host at scale 1.6. It checks tiled/reference seams and black/white-derived alpha using the existing fractional-scale tolerance; whole-viewport checkerboard comparisons remain diagnostic. This does not establish rendering behaviour on every GPU or scale. Evidence: `rust/evidence/studio-display-20260927/verified-display-results.json`.
- The installed launcher opened the saved four-layer preview project. Cua captured its identified window, and the screenshot was visually inspected: `rust/evidence/studio-installed-preview-20260927/studio-window.png`. This was capture-only Cua validation; the Omarchy input plugin remains inactive in the current compositor session. The test process used isolated application data and was closed afterwards.

The preview project and exported image are under `rust/evidence/studio-preview-final-20260927/`. Evidence directories are local, ignored artifacts; the example and test harnesses are the reproducible sources.
