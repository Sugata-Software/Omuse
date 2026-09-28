# Source provenance and acknowledgements

Omuse is a native Linux creative application developed by Sugata Software.
The current application is written in Rust, uses GPUI for its interface, and
integrates [gpui-omarchy](https://github.com/huacnlee/gpui-omarchy) for Omarchy
themes and controls.

## Application lineage

Omuse grew from [Robbie Tilton's Compositor](https://github.com/robbietilton/Compositor)
and [the earlier Linux fork](https://github.com/chiddekel/Compositor). Their work
informs the inherited `.comp` format, compatible editing behavior and portable
pixel algorithms. The original Wonder Assembly LLC MIT copyright notice is
retained in [LICENSE](../LICENSE).

The current source layout contains the Linux application and its supporting
tools. The previous Swift application, Qt port, Xcode project, Flatpak setup and
platform-specific build instructions have been removed from the current tree.
They remain recoverable in the
[complete historical source snapshot](https://github.com/Sugata-Software/Omuse/tree/4adb0e5160a2385d2fdfe952d1b027299f5e20b2).
That snapshot preserves the older implementation for reference and testing;
its build instructions and release names are historical, not Omuse installation
instructions. See the [current development guide](../rust/README.md).

The `.comp` wire-format identifiers and legacy settings migration paths are
deliberately stable. They preserve existing artwork and user preferences; see
the [compatibility contract](omuse-rename.md). Their historical names are not
the application's public identity.

## Independent pixel references

Four unmodified C reference files are retained under
[`rust/tests/reference/upstream-kernels/`](../rust/tests/reference/upstream-kernels/),
with the original licence and source hashes. They generate deterministic
Camera Raw comparison fixtures and support an isolated correctness
contribution. They are test references, not libraries linked into Omuse, and
do not require the earlier application or any Apple development tools.

The fixture generator records its source hashes in the JSON test data. The
separate [color-noise reproduction](upstream-color-noise-reproduction.md)
preserves a proposed upstream fix without changing those reference bytes.
Omuse's Rust implementation handles that scratch buffer deterministically.

## Dependency and media provenance

`rust/Cargo.lock` pins Rust dependencies. Vendored code retains its notices;
optional runtime notices are in `rust/licenses/`. The
[dependency notice findings](rust-license-findings.md) identify the two
unresolved legal-text entries that still block the planned public binary
distribution. A local source build does not close that distribution gate.

The [public media credits](media/README.md) cover the README screenshot and
separate promo media. Branding images retain embedded generation provenance
where present. Third-party code, fonts, media and optional runtime assets keep
their own terms and attribution; this repository does not change their
ownership or relicense them under the application licence.

Dated design and qualification reports describe their recorded revisions.
Use the [public release checklist](public-release-readiness.md) for the
current release status and remaining work.
