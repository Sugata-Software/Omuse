# Proposal: Omuse, a native Rust Linux edition

Omuse is an independently runnable Rust editor for Linux, using the existing
`.comp` format and direct Omarchy controls/theme watching. It grew from the
Compositor codebase while avoiding the platform compatibility layers required
by that macOS implementation. It is a substantial Linux port for maintainer
review, with remaining compatibility work explicitly tracked, and does not
claim completed Mac parity. This is a local proposal; nothing has been
submitted or published.

## Scope and architecture

The proposed application lives under `rust/`, with its own Cargo manifest,
lockfile, tests, build scripts and `omuse` launcher. It does not embed Swift, Qt
or the old theme-provider process. `Compositor/` and `CompositorTests/` remain
unchanged against the preserved source baseline. Existing Mac and Swift/Qt
build paths can therefore continue independently; accepting Omuse need not
replace either legacy application or convert user documents.

The domain model, document I/O, editor transactions and CPU raster pipeline are separate from the GPUI interface. GPUI supplies graphics-backed presentation; expensive image operations still use bounded CPU buffers and row parallelism. `gpui-omarchy` supplies actual controls, colors and the system-theme watcher. Document saving stages complete packages before atomic replacement; recovery uses separate UUID sessions and process-held ownership locks. Unsupported or malformed file semantics fail explicitly, and unknown source metadata is retained.

The [implementation matrix](rust-rewrite-status.md), [source parity audit](rust-parity-audit-20260926.md) and [Rust README](../rust/README.md) define the review scope. They cover layers/masks, live objects, adjustments/effects, selections, retouch, Camera Raw, PSD/RAW import, subject tools, exports and desktop interactions. Text content has a canvas-anchored editor; rotated pixel-aligned editing and text-box handles remain differences. CPU full-surface rendering remains an architectural limit. Snapshot images now share immutable allocations with copy-on-write editing and bounded retained-history accounting; tile-granular storage is still pending.

## Reproduce the Linux checks

Use a Linux Rust environment with C/C++ tools, Python 3, `pkg-config`, Wayland, libxkbcommon including X11, Fontconfig and LittleCMS 2 development libraries. Development has used Rust 1.98.0; the complete application's minimum Rust version is not yet qualified. Run from the repository root:

```sh
scripts/build-rust.sh
OMUSE_TEST_KEEP=1 scripts/test-rust.sh
UPSTREAM_REF=75c4219 scripts/check-upstream-clean.sh
python3 scripts/generate-rust-kernel-fixtures.py
git diff --exit-code -- rust/tests/fixtures
```

The test script uses `--release --locked --features ui-test`, checks formatting, runs domain and headless GPUI interactions, then performs a synthetic editing/save/reopen/export journey. It isolates XDG data/config/cache/state and retains generated evidence when requested; it does not replace `HOME` or edit existing artwork. The fixture generator recompiles preserved C kernels and embeds their source hashes. Its scope excludes Apple-specific processing.

For a separate native window on the supported Omarchy/Hyprland desktop:

```sh
mkdir -p rust/evidence
native_evidence=$(mktemp -d "$PWD/rust/evidence/reviewer-native.XXXXXXXX")
python3 scripts/native-rust-check.py \
  "${CARGO_TARGET_DIR:-rust/target}/release/omuse" \
  "$native_evidence" --require-usable-startup --capture
```

This harness uses Hyprland's Lua dispatch API, focuses only its synthetic test process and checks native interaction, persistence and theme invariants. It is not a generic desktop test or proof of physical input-device support. Fresh evidence directories prevent stale results passing.

Optional runtime tests return early when fixture variables are absent. To include them, prepare checksum-pinned assets with `scripts/prepare-rust-assets.sh`, set these variables to absolute paths, then rerun `scripts/test-rust.sh`:

| Variables | Required assets / fixture |
| --- | --- |
| `OMUSE_ONNX_RUNTIME`, `OMUSE_SUBJECT_MODEL`, `OMUSE_SEGMENTATION_SAMPLE` | ONNX shared library, U2NETP model, subject image with foreground and background |
| `OMUSE_LIBRAW`, `OMUSE_RAW_FIXTURE` | LibRaw shared library and supported camera file |
| `OMUSE_PSD_FIXTURE`, `OMUSE_PSD_REFERENCE` | Supported layered 8-bit RGB PSD with at least two raster layers and an independently rendered reference PNG |
| `OMUSE_PSD_REJECT_FIXTURE` | Unsupported-depth or other explicitly unsupported PSD |

[Runtime setup](rust-runtime-assets.md) records download provenance, hashes, notices and the attributed DNG fixture. Portable JSON references are committed under `rust/tests/fixtures/`; downloaded models, camera/PSD samples and native libraries are not.

Do not transfer test totals or binary hashes between revisions. The ignored `rust/evidence/` installation/release receipts and referenced logs are the authority for a particular run. Before sharing a review packet, confirm its source revision and executable SHA-256 match the proposed commit, record which optional fixtures actually ran, and attach sanitized copies of the receipt, logs and app captures. Older receipts may describe earlier checkpoints; they are not automatically evidence for the current tree.

## Dependencies and distribution review

`gpui-kit` is pinned to `=0.6.6` (Apache-2.0); `gpui-omarchy` to Git revision `3625c6cb5f02ec38a970939e52cfdb0c374c7791` (MIT). `Cargo.lock` fixes the remaining resolved graph. The project uses the repository's MIT license; cosmic-text and image declare MIT-or-Apache-2.0. The targeted `gpui-pre-macros` release-profile workaround is recorded in `Cargo.toml`; no dependency source patch is applied.

Optional runtime preparation is currently Linux x86_64 only: ONNX Runtime 1.23.2, U2NETP and LibRaw 0.22.2 have pinned downloads/checksums. Existing runtime notices are in `rust/licenses/`; LibRaw retains its dual-license notices and source/build recipe. Before binary distribution, complete a notice inventory for the entire locked Cargo graph as well as native libraries/model provenance and corresponding-source requirements. The current notice directory alone is not a complete distribution review.

## Qualification still required

- Actual macOS `.comp` open/edit/save comparisons, including fonts, text metrics, effects, masks, transforms and Core Image geometry/resampling.
- Explicit evaluation of platform substitutions: LibRaw versus Apple RAW, U2NETP versus Vision, Linux fonts/cosmic-text, and CPU filter/interpolation approximations. Settings compatibility does not establish equal output.
- Physical theme changes, tablet pressure/tilt, clipboard/portal/file-dialog behavior, mixed-DPI and color-managed displays, graphics fallback, large documents, interrupted saves and sustained sessions.
- A clean Linux CI environment, supported toolchain/platform matrix and reproducible packaging. The dedicated Rust workflow has been added but its first remote Ubuntu run remains unverified. Existing Swift/Qt CI is not evidence that Rust runs on a clean runner.

## Suggested contribution sequence

1. Offer the independent portable C correctness issue below with a focused regression test. It can benefit Mac and Linux without adopting the rewrite.
2. Contribute documented format contracts and deterministic portable-kernel references, with their narrow comparison claims.
3. Agree with maintainers whether `rust/` belongs as an experimental edition in the existing repository or in the Linux fork. Present the domain/I/O/rendering changes, then UI/desktop/runtime integration as reviewable dependent changes; keep existing application entry points intact.
4. Attach commit-matched Linux evidence and establish Rust CI before advertising a supported release. Complete the Mac/hardware comparison matrix before making broader parity claims.

## Separate candidate: color-noise scratch reads at zero alpha

Source inspection confirms a specific correctness issue in [`Compositor/Rendering/AdjustPixels.c`](../Compositor/Rendering/AdjustPixels.c#L842): the color-noise path allocates `chroma` with `malloc` at line 844, skips alpha-zero pixels at line 852 without initializing their entries, then blurs the entire plane at line 859. `box_blur_plane` reads those entries at lines 368–372; the filtered chroma feeds visible-pixel saturation at lines 871–877. Transparent input can therefore introduce uninitialized values into neighboring visible output.

A focused candidate change is to initialize every chroma entry, including alpha-zero pixels, before blur (for example, zero initialization consistent with the luma path and current Rust implementation). Pair it with transparent-only and transparent/visible-neighbor cases, unchanged alpha assertions, repeated allocator-state runs and an uninitialized-read detector where available. Keep opaque-image output unchanged. The standalone [reproduction](upstream-color-noise-reproduction.md) and `patches/color-noise-zero-alpha.patch` now provide an isolated review packet. The harness applies the patch only to temporary source, compares controlled scratch-memory cases, and checks MemorySanitizer when available. The protected C source is deliberately unchanged in this branch.

The advanced C reference currently excludes alpha-zero inputs because undefined scratch contents cannot serve as a deterministic golden result. Basic/calibration fixtures still cover zero alpha. After an accepted C fix, extend the advanced color-noise reference to zero-alpha cases rather than treating the present exclusion as completed coverage.
