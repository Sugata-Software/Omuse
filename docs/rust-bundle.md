# Omuse offline runtime bundle

To install Omuse, start with the [one-command installer](install.md).
This guide is for developers preparing and verifying offline artifacts.

The runtime bundle is an offline, per-user package for the already-built Rust executable. Qualification is specific to a recorded executable on the project's Linux x86_64 Omarchy/Arch test host. The [rename checkpoint](omuse-rename.md) records verification of the Omuse package. A dynamically linked executable still requires compatible system libraries on another Linux distribution.

Create a full-feature archive with:

```sh
scripts/package-rust.sh \
  --binary /absolute/path/to/omuse \
  --output /absolute/path/to/omuse-linux-x86_64.tar.gz
```

Packaging requires all three prepared runtime assets: `libonnxruntime.so`, `libraw.so`, and `u2netp.onnx`. It also includes the shared installation manager, the project MIT license, native-runtime and model notices from `rust/licenses/`, the application icon, the exact source revision, and checksums for every bundled file. The packager runs `scripts/rust-license-inventory.py` offline against the locked Linux x86_64 Cargo graph and adds its JSON provenance inventory and combined dependency notice text as flat files under `licenses/`. The script performs no build or download. It rejects a dirty source tree by default so the revision can identify the source used for the executable. `OMUSE_ALLOW_DIRTY=1` permits an explicitly labelled dirty local-development bundle; do not use that override for release artifacts. The old variable name remains a compatibility alias during the rename transition.

The packager records the checked-out source revision; it cannot inspect how an arbitrary supplied executable was built. Release evidence must independently tie that executable's SHA-256 to a build from the recorded clean revision. The generated inventory is review evidence, not a compliance certification. The 28 September review records two unresolved legal-text findings in the 608-package locked graph; see [the current notice review](rust-license-findings.md). Those findings must be resolved before public binary distribution.

Install the archive without network access or administrator privileges:

```sh
scripts/install-rust-bundle.sh /path/to/archive.tar.gz
```

The default prefix is `~/.local`. For an isolated verification that cannot modify the normal user installation, pass an absolute prefix:

```sh
scripts/install-rust-bundle.sh /path/to/archive.tar.gz --prefix /tmp/omuse-bundle-test
```

Spaces, dollar signs, backticks and percent signs in a prefix are escaped for both the shell launcher and desktop entry. A prefix containing a single quote or backslash is rejected before installation because Freedesktop `Exec` parsing cannot represent those paths consistently across validators.

The installer requires Python 3 and `sha256sum`, and rejects hosts or bundle manifests other than Linux x86_64. Before extraction, Python's standard `tarfile` reader validates archive headers one at a time, rejects path escapes, duplicate names, links and special files, and enforces compressed-size, expanded-size and member-count limits. It then writes only the validated directories and regular files into a private temporary directory. The checksum manifest must cover every extracted regular file exactly, and every checksum is verified before the installer writes under the prefix. It installs `opt/omuse`, a `bin/omuse` launcher, an `omuse-manage` maintenance command, the icon, and `omuse.desktop`. Fresh installs create only the Omuse identity. The shared installer runs an isolated editing self-test before activating a complete executable/runtime generation. Updates keep the previous generation, and `omuse-manage rollback` atomically exchanges current and previous installations. A previous flat installation is retained with its own runtime assets. Preserved upstream executables and installations are not changed.

This is a native dynamically linked bundle rather than a hermetic container. The target host must provide a compatible glibc and the executable's system libraries, plus the dependencies reported by `ldd` for the executable and bundled libraries. On the qualified Arch host these include the desktop/graphics stack used by GPUI and the JPEG, zlib and LittleCMS libraries used by LibRaw and color management. Check a candidate host before installation with `ldd` and treat any `not found` entry as incompatible. Flatpak or distribution-specific packaging remains the appropriate route for a broader compatibility promise.
