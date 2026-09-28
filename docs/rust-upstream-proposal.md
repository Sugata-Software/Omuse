# Contributing improvements upstream

Omuse is an independent native Linux creative application with its own
repository, Rust/GPUI implementation and release process. Its inherited
`.comp` compatibility and portable algorithms benefit from earlier work in
Compositor. [Source provenance](source-provenance.md) records that lineage and
preserves the original notices.

Focused, reproducible fixes can give back to those projects without requiring
their maintainers to adopt Omuse's architecture or product direction. No
upstream submission has been made for the candidate below.

## Portable correctness contribution

The independent
[`AdjustPixels.c` reference](../rust/tests/reference/upstream-kernels/AdjustPixels.c)
contains a color-noise defect: alpha-zero pixels leave scratch chroma entries
uninitialized before the entire plane is blurred. The values can affect
neighboring visible pixels. Omuse's Rust implementation initializes this
scratch plane deterministically.

The [standalone reproduction](upstream-color-noise-reproduction.md) and
[review-only patch](../rust/tests/reference/color-noise/color-noise-zero-alpha.patch)
provide a small contribution packet. Run from the repository root:

```sh
scripts/reproduce-upstream-color-noise.sh
```

The harness compares transparent-only, mixed-alpha and opaque cases under
two controlled allocation patterns. It preserves transparent bytes and alpha,
verifies unchanged opaque output, and checks MemorySanitizer when available.
The patch is applied only to temporary source, leaving the original reference
bytes and provenance unchanged. This is a portable C correctness check, not a
claim that macOS integration has been tested.

## Format and pixel references

The [document format](project-format.md) records supported manifest semantics,
validation and resource bounds. The independent C kernels retain their exact
upstream revision, source hashes and licence under `rust/tests/reference/`.
The portable Camera Raw reference data can be reproduced with:

```sh
python3 scripts/generate-rust-kernel-fixtures.py
git diff --exit-code -- rust/tests/fixtures
```

The advanced color-noise fixtures exclude alpha-zero inputs because undefined
scratch values cannot serve as deterministic expected output. Basic and
calibration fixtures still cover zero alpha. After an accepted reference fix,
extend the advanced cases with that source change and its provenance together.

## Preparing a contribution

Keep each proposed change small enough to review independently, explain the
concrete failing input and resulting behavior, and include commands that
reproduce the failure and verify the fix. Link the exact source revision used.
Run the upstream project's own relevant tests when its environment is
available, and distinguish those results from Linux reference checks.

For contributions to Omuse itself, use the
[contributor guide](../CONTRIBUTING.md), [development guide](../rust/README.md)
and [release checklist](public-release-readiness.md). The
[first public Linux CI run passed](https://github.com/Sugata-Software/Omuse/actions/runs/36426033617);
native desktop, optional backends and binary distribution retain their
separate qualification requirements.
