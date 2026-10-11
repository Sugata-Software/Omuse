# Texture removal development qualification

**Historical development evidence.** The implemented changes below are included
in Omuse 0.10.0. The [0.10 release qualification](release-0100-qualification.md)
records the final source, CI, packages and publication status. Measurements,
source identities and open-gate statements below describe their original
checkpoint; they are not silently rerun or promoted by that release.

Status: **experimental, opt-in development work; not a public release**.
The user-facing method is **Texture · experimental** in **Controlled
content-aware removal**. **Context** remains the default. The immediate
Content-aware Fill and Spot Healing kernels are unchanged.

This record separates implemented behavior, isolated kernel checks and actual
photo inspection from the completed integration and native workflow checks.
The [photo/vector qualification](photo-vector-qualification.md) records the
final development candidate and broader validation scope. This record does not
qualify Texture as a general replacement for Context, AI object removal or
PhotoCraft's full solver.

## Behavior and compatibility

TextureV2 initializes from known boundaries, compares colour and local texture
gradients, then runs three alternating patch-field refinement passes. Final
pixels use a single covering source patch rather than averaging unrelated
textures. The search keeps immutable donor coordinates; it does not use the
removed object's hidden colours as evidence.

The working region is the target's bounding box expanded by the search radius,
patch radius and texture-feature context. Field allocations use that region,
not the entire photograph. The full-size masks are still scanned to find the
region. All source patches, including feature support, must lie within the
allowed, unselected sampling region. A narrow sampling rectangle may therefore
need enlarging even when a few individual donor pixels appear available.

Preview and Apply use the existing background removal workflow. Preview leaves
the document unchanged; Apply creates a derived **Removal · editable** layer
with source and recipe retained, in one Undo transaction. Cancellation is
checked during mask scans, within long rows, between searches and during final
copying. Failed or cancelled synthesis emits no unfinished field. Callers stage
the final copy and publish it only after success, because cancellation or a
callback failure may also occur during that copy.

Unselected pixels receive no removal writes. For retained 16-bit sources, an
8-bit matching preview selects coordinates; the existing precision path copies
or blends original 16-bit donor samples. This design preserves source precision
without claiming that matching itself takes place at 16-bit precision. The
dedicated persisted-recipe test passed in the canonical Cargo run described
below.

`ContentAwareAlgorithm::TextureV2` serializes as `textureV2`. `Legacy` and
`ContextualV1` implementations are unchanged; missing algorithm fields still
select Legacy. Unknown versions are rejected. Existing projects are not
silently re-rendered with Texture. A Texture recipe requires a compatible build;
do not assume an older build can open or preserve it. Keep the editable source
project and provide a separate rasterized copy for backward editing.

## Bounds

| Resource | Current bound |
| --- | --- |
| Source image through Omuse's advanced-operation path | 16,777,216 pixels |
| Expanded working region | 4,000,000 pixels |
| Selected pixels | 250,000 |
| Search radius | 1–64 source pixels |
| Patch radius | 0–4; zero still uses immediate context |
| Compared patch cells | 256,000,000 |
| Kernel scratch estimate | 256 MiB maximum, with fallible reservations |
| Refinement passes after initialization | 3 |

The kernel's standalone input guard permits 67,108,864 source pixels; the
enclosing application's stricter source limit still applies. The patch-cell
limit is not a wall-clock deadline: preparation has separate linear work over
the bounded source, region and selection. Candidate counts adapt to the patch
budget, with a minimum quality budget that can refuse work before the other
limits are reached. Widely separated objects can exceed the region limit even
with a small selected-pixel count.

The scratch estimate does not include the caller's source, full-size masks,
precision results, history, compositor or GPU textures. It is not a total
process-memory cap. Missing suitable source patches, an excessive region or an
insufficient work budget returns an error without publishing an edit.

## Local checks — 10 October 2026

Evidence is retained outside the repository at:

```text
Omuse-release-evidence/texture-removal-2026-10-10/
```

The final isolated kernel source SHA-256 is
`78bf0b1957878ff93d678a7205ef8207ec2dd0d8c4d7f9dcf6c67279e20a23d5`.
`comparison-build-final.json` identifies the standalone helper, exact kernel
source and dependency-library hashes. These helpers include the new production
kernel source and link the retained Omuse library for ContextualV1 comparisons;
they are not a substitute for compiling the fully integrated application.

Six isolated Rust module tests passed. They cover deterministic output, texture
retention in a synthetic fixture, no influence from hidden target colours,
allowed original donors, soft coverage, protected pixels, cancellation,
invalid/work-budget refusal, disconnected/border targets and a small region
within a source larger than four million pixels. The log is
`unit-tests-final.log`. Source formatting checks also passed.

Ten real-photo output files passed an independent ImageMagick PNG decode,
matching their recorded raw-pixel hashes. The same independent check compared
all unselected pixels against the original and found them exact. See
`independent-png-audit.json`. Successful comparisons also passed a deterministic
repeat inside the Rust harness.

The new integration test,
[`texture_removal_recipe.rs`](../rust/tests/texture_removal_recipe.rs), covers
versioned serialization, exact original 16-bit donor words that are not aligned
to expanded 8-bit values, protected pixels, cancellation without history
changes, one Undo/Redo, and save/reopen with exact source/result/recipe and
re-evaluation. It passed in the final integrated run below.

## Integrated and native checks — 11 October 2026

Final integration evidence is retained at:

```text
Omuse-release-evidence/photo-vector-2026-10-10/
```

`cargo-final-clean-profile.log` reports **1,554 passed, zero failed and five
ignored** across the complete suite. It includes the passing canonical
`texture_removal_recipe` test. These are reported test counts, not a claim that
every optional external fixture or ignored check was exercised.

The final native binary's SHA-256 is
`b0ef1f3a485adf1e670f17bdb4591cb4467f55326f52350efcb399693eb1bea7`.
Native Cua screenshots `final-17-photo-original.png` through
`final-26-photo-undo.png` record this bounded real-photo workflow:

1. Open the retained car working copy and select the whole car and shadow.
2. Find Controlled content-aware removal with **Ctrl+K**; choose **Texture ·
   experimental**, search radius **64**, patch radius **2** and feather **0**.
3. Run Preview and reach the ready state without committing a document edit
   (`final-23-removal-preview.png`).
4. Apply the result as a separate **Removal · editable** layer with the original
   retained and hidden (`final-24-removal-applied.png` and
   `final-25-removal-kept.png`).
5. Use one Undo to restore the original document and the corrected footer state
   (`final-26-photo-undo.png`).

Native in-flight cancellation was **not** exercised in this run; cancellation
coverage here remains the kernel and integrated engine tests. The earlier
photographic corpus was not rerun on this final binary. This native car check
confirms the preview/Apply/Undo workflow, not a new photographic quality or
performance comparison. The existing mixed results and seam limitations below
remain applicable. No release package, normal installation or public deployment
is qualified by these checks.

## Real-photo comparisons

The portrait is the retained 512 × 512 NASA Eileen Collins image. The Canon
racing-car and Nikon foliage photographs come from the pinned CC0 fixtures in
[`photo-sources.json`](../rust/tests/fixtures/photo-sources.json).
`working-copy-provenance.json` records source and derived-file hashes for the
640-pixel-wide comparisons. The separate native-size Nikon check uses the
4310 × 2868 image without resizing. These are already-developed RGBA8 images;
this exercise does not qualify RAW decoding or colour management.

| Case | Result and limitation |
| --- | --- |
| Portrait mission patch; 6,092 selected pixels | Both methods remove the badge. Texture produces stronger folds and a visible lower seam; it is not a clear visual improvement over Context. All 256,052 outside pixels remain exact. |
| Same portrait, both methods at Feather 0.25 | Deterministic and outside-exact, but the lower Texture seam is not substantially improved. Blending is not a demonstrated remedy for this mismatch. |
| Complete racing car and shadow; 22,736 selected pixels | Texture plausibly replaces the car with asphalt. A small lower-left curb-edge pinch remains. Context refuses its minimum work budget at these settings. All 249,904 outside pixels remain exact. |
| Deliberately held-out foliage patch; 1,280 selected pixels | RGB reconstruction MSE is 71.7148 for Context and 80.0536 for Texture. This example does not establish a numerical quality gain. Both preserve all 271,360 outside pixels. |
| Native-size Nikon; 1,024 selected pixels | Texture uses a small region on a 12,361,080-pixel image and preserves all 12,360,056 outside pixels. Context refuses its four-million-pixel source cap. This establishes bounded-region behavior and integrity for this example. |

An earlier car selection accidentally left the rear wing and shadow outside
the mask. Both methods copied unwanted residual context. That failed example
is retained under `car-removal/`, and remains visible in
`comparison-sheet.png`; it is not counted as successful complete-object removal.
The corrected full selection is recorded separately under
`car-removal-complete-selection/`. No failed image was retouched to conceal a
problem.

Inspect `car-before-after.png` for the complete-car comparison, and the
`portrait-mission-patch/`, `portrait-feather-025/`,
`foliage-reconstruction/` and `nikon-native-size-texture-reconstruction/`
directories for full outputs, masks, parameters and receipts.
`qualification.json` collects the source identity, checks and limitations.

### Timing observations

Single first-run observations, in milliseconds, were recorded under the host
load at the time. Each successful result was repeated to check determinism;
these numbers are not medians from an isolated benchmark.

| Case | ContextualV1 | TextureV2 |
| --- | ---: | ---: |
| Portrait mission patch | 4,445 | 927 |
| Initial incomplete car selection | 1,836 | 2,257 |
| Complete car and shadow | Work-budget refusal | 4,915 |
| Held-out foliage patch | 1,564 | 77 |
| Native-size Nikon small patch | Source-size refusal | 94 |

The mixed observations do not support a blanket speed claim. The harness also
performs different caller-side setup for the two methods. These measurements
exclude file decoding/saving and do not measure native input-to-preview latency,
GPU upload, precision editing or complete application memory usage.

## Attribution and remaining work

This is an original, bounded implementation informed by the texture-feature,
patch-field and best-covering-patch ideas in PhotoCraft's
[`nonlocal.rs` at 7722172](https://github.com/storytold/photocraft/blob/7722172585a01cbdb93c06f0f5ff2634fcb17999/crates/algo/src/nonlocal.rs).
It does not copy the full solver or implement its multiscale and gradient-domain
seam corrections. The PhotoCraft MIT notice is retained alongside the kernel
and in [`rust/licenses/PHOTOCRAFT-MIT.txt`](../rust/licenses/PHOTOCRAFT-MIT.txt),
which the existing Linux and Windows packaging scripts include.

Native in-flight Cancel inspection and broader photographic comparisons remain
open before considering a default change or release claim. Portrait seams,
edge continuity, repetitive patterns,
large structures and the quality-versus-cost tradeoff remain areas for further
work. A texture search cannot reliably infer a missing face, lettering or scene
geometry. Keep the preview-driven choice and preserved source.
