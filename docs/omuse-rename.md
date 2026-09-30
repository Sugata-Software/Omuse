# Omuse rename and compatibility contract

Omuse is the active product name for the native Rust/GPUI Linux application.
The application-facing names and paths use Omuse while preserving access to
existing projects, user data and truthful repository history.

## Canonical names

| Surface | Canonical value |
| --- | --- |
| Product and desktop entry | `Omuse` |
| Editable project extension | `.omuse` for single canvases and collections |
| Terminal command and executable | `omuse` |
| Default build output | `rust/target/release/omuse` |
| Per-user installation | `~/.local/opt/omuse/omuse` |
| Per-user launcher | `~/.local/bin/omuse` |
| Desktop file | `omuse.desktop` |
| Configuration | `$XDG_CONFIG_HOME/omuse` |
| Application data and recovery | `$XDG_DATA_HOME/omuse` |
| Environment-variable prefix | `OMUSE_` |
| Development checkout directory | `Omuse` |

Documentation and scripts should use paths relative to the repository root
where possible. The development checkout can live anywhere; its location is
not a runtime requirement.

## Existing projects and user data

The canonical editable project extension is **`.omuse`** for both single
canvases and Create collections. These are directory packages, not flat image
files; move or back up the complete directory. Package contents identify the
kind: a canvas has `manifest.json`, while a collection has `project.json`.
The shared extension does not flatten a collection into one canvas.

Legacy `.comp` projects remain readable. Opening one does not rewrite or rename
it. Its first UI **Save** offers an `.omuse` copy and leaves the original intact;
subsequent saves use the new project location. Retained layer format identifiers,
including `com.compositor.project`, and unknown source metadata remain stable.
Changing the public extension does not require rewriting those wire records.

Create collections now save schema version **2**, with `.omuse` packages for
their nested pages and reusable components. The reader also accepts version
**1** collections containing `.comp` pages/components. Saving a version 1
collection upgrades it to version 2. Earlier Omuse releases cannot read version
2 collections: use **Save As** to a new location before saving if an older
release must still open the original. Reading an existing collection alone
does not upgrade it.

Settings, custom shortcuts, brushes and recovery sessions from the previous
Rust application remain discoverable. Omuse prefers the canonical `omuse` XDG
directories and migrates legacy data non-destructively when it is first needed.
Migration must not delete the legacy copy or overwrite newer canonical data.
Recovery snapshots remain separate from saved projects and never replace them
implicitly.

`OMUSE_` is the canonical prefix for runtime, test and packaging overrides.
The corresponding legacy-prefixed variables remain accepted as aliases so
existing local workflows continue to function. When both forms are set, the
canonical `OMUSE_` value takes precedence.

## Intentional legacy references

Older names remain in the repository when changing them would break
compatibility or falsify the record:

- [Source provenance](source-provenance.md) attributes the original Compositor
  project and earlier Linux fork. Their full application trees and build
  instructions remain in Git history, outside the current source layout.
- The independent C reference kernels retain their original identifiers,
  source hashes and copyright notice under `rust/tests/reference/`.
- The `com.compositor.project` layer wire identifier, retained metadata and
  legacy `.comp` read paths remain for compatibility. Version 1 collection
  readers are retained; new collection writes use version 2.
- Legacy executable, environment and XDG names may appear in migration and
  alias code.
- Dated validation receipts, hashes, screenshots and fixed-checkpoint reports
  retain the names that were true when that evidence was recorded.

New product documentation, commands, package contents and desktop presentation
use Omuse. A remaining older name should therefore be attributable to one of
the compatibility or historical cases above, rather than serving as the active
application name.

Fresh installations create the `omuse` command and desktop entry. An existing
legacy command may be retained only as an upgrade compatibility alias; it is
not a second public application or an additional command users need to learn.

## Verification checkpoint — 27 September 2026

The renamed source passes 474 automated Rust tests and the headless
paint/undo/redo/save/reopen/PNG/JPEG/WebP/TIFF journey. The three manual timing
benchmarks are intentionally excluded from this gate. The real RAW, layered
PSD and local ONNX fixtures were enabled. Eleven installer regression tests
also pass.

The added migration checks exercise canonical-setting precedence, one-time
copying without restoring reset settings, legacy symlink rejection, canonical
environment-variable precedence and discovery of unlocked legacy recovery
packages while another legacy session is active. Native installation and
package receipts are recorded separately under ignored `rust/evidence/`.

The normal release executable and installed `omuse` launcher each pass all
19 native Wayland acceptance checks. Startup is maximized at 1776×1075 logical
pixels on this host; the production journey also passes at 800×600. The Muse
header was inspected in dark and light themes. An isolated native launch copies
all three legacy settings files byte-for-byte and leaves their originals intact.

A project saved by the previous executable produces identical RGBA pixels in
Omuse, with its original package unchanged. Seven CLI batch checks pass. The
save/recovery qualification completes twelve revisions with three retained
16-bit layers; after killing a writer during revision 4, project revision 3 and
recovery revision 4 both reopen correctly. This is process-interruption evidence,
not a power-loss or long-session guarantee.

The installed launcher resolves to `~/.local/bin/omuse`, reports `Omuse 0.1.0`
and exports the DNG fixture to a fully decoded 3024×4032 PNG with an embedded ICC
profile using adjacent runtime assets. The previous command forwards to Omuse.
The pre-rename executable is retained both at its original path and as
`~/.local/opt/omuse/omuse.previous`; the preserved Swift/Qt launchers are unchanged.
The old Rust desktop entry is backed up as `.desktop.retired-by-omuse` and replaced
by `omuse.desktop`. Installed icons and optional runtimes match their source files.

The selected brand is [Sunset Muse](branding/sugata-retro.md): the Sculpted
Muse profile with Sugata-inspired plum, paper and sunset colours. The launcher
uses the colour SVG/PNG; the header glyph takes its colour from Omarchy.
