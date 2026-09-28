# Color-noise zero-alpha scratch-read reproduction

`Compositor/Rendering/AdjustPixels.c` leaves `chroma[index]` uninitialized when a source pixel has zero alpha, then passes the entire plane to `box_blur_plane`. The blur reads those entries and feeds them into the saturation calculated for neighboring visible pixels.

Run the standalone check from the repository root:

```sh
scripts/reproduce-upstream-color-noise.sh
```

The script compiles the preserved C implementation through its public `adjust_camera_raw_detail` API. It fills every allocation with two controlled byte patterns and exercises three inputs:

- a transparent-only image, which must remain byte-for-byte unchanged;
- transparent pixels around visible colored neighbors, whose visible output must not depend on prior scratch contents;
- an opaque image, whose output must be identical before and after the candidate fix.

The current implementation produces different visible-neighbor results for the two allocation patterns. The candidate patch initializes zero-alpha chroma to zero, after which both runs agree. Alpha and transparent bytes remain unchanged in every run, and the opaque result is identical across current and patched builds. When Clang MemorySanitizer is available, the script also builds the unmodified source without the controlled allocator and requires a `use-of-uninitialized-value` report.

The proposed source-only change is in `patches/color-noise-zero-alpha.patch`. It deliberately initializes the missing entries in the population loop rather than relying on allocator behavior. Zero is consistent with the existing luma-plane treatment of transparent pixels. The patch does not change blur radius, edge clamping, visible-pixel HSL conversion, alpha handling, or allocation-failure behavior.

This harness proves a native C undefined read and its influence on neighboring output under controlled scratch contents. It does not establish what all production allocators will happen to return, measure visual quality at transparent edges, or replace macOS integration tests. The patch artifact is review-only and is applied only to a temporary source copy by the script; the protected source tree is not modified.

## Local verification

The check passed on the Linux development host on 26 September 2026 with Clang. MemorySanitizer reported the uninitialized value in `adjust_camera_raw_detail` at `AdjustPixels.c:878`. The protected source SHA-256 was `d189e641ad4cbf036f0f4912d27be2ac63ab16d14f27a21c8d455863f6ffb2be`. Machine-local output is retained in the release evidence. Nothing has been submitted upstream.
