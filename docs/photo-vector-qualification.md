# Photo and vector development qualification

**Historical development evidence.** The implemented changes below are included
in Omuse 0.10.0. The [0.10 release qualification](release-0100-qualification.md)
records the final source, CI, packages and publication status. Measurements,
source identities and open-gate statements below describe their original
checkpoint; they are not silently rerun or promoted by that release.

**Unreleased development checkpoint, 10–11 October 2026.** These additions are
not included in the published 0.9.0 downloads. Final Linux validation completed
with **1,554 reported passes, no failures and five ignored benchmarks**, plus
the bounded native and independent-reader checks below. No new release,
package, normal installation or public deployment is qualified here.

Use the [development workflow guide](user-guide/photo-vector-development.md)
for controls and the [roadmap](photo-vector-roadmap.md) for remaining scope.
Receipts and screenshots are retained outside the repository in evidence sets
`photo-vector-2026-10-10` and `texture-removal-2026-10-10`. Filenames below refer
to those sets; they are evidence identifiers, not public download links.

## Implemented scope and boundaries

| Area | Development behavior and important limits |
| --- | --- |
| Path geometry | Bézier booleans, tolerance-based Simplify, Offset and Outline strokes; split at a node and join endpoints within one object. Fitting is approximate; work limits can refuse complex input. Translucent fill/stroke combinations are refused where expansion would change compositing. |
| Shape Builder | Main-canvas region merge/Alt-erase for 2–8 consecutive, opaque filled paths without strokes; holes, gradients, traversed regions and draft Undo/Redo. Results are editable geometry, not persistent live boolean recipes. |
| Snapping | Node/handle targets, guides and grid measured within six screen pixels, accounting for layer transforms. Shift bypasses snapping while preserving angle constraints. Cached targets and bounded queries can fall back to an unsnapped drag; this does not replace every existing snapping path. |
| Repeats | Grid and radial previews generate independent editable copies, preserving originals and using normal draft history. Saved copies are not linked instances or a persistent repeat effect. |
| SVG | Supported text/tspan becomes glyph outlines with font/conversion warnings. Supported gradient or nonuniformly transformed strokes become filled outlines. Single-painted-child group opacity is supported; general group compositing, clipping, filters and external resources remain unsupported. |
| PSD/PSB | Converted 8-bit layered PSD export, complete recorded export warnings, and bounded 16-bit RGB merged-composite import with retained precision/ICC conversion. Text/vector/effects are rendered for export; unsupported placement/compositing is refused. See [Photoshop exchange](psd-exchange.md), including file/memory limits and Save As → Flatten recovery. |
| Removal | Opt-in **Texture · experimental** adds bounded texture search; **Context** remains the default. Existing algorithms and missing-version behavior are retained. The application admits sources up to 16,777,216 pixels; the texture region is limited to 4,000,000 pixels and 250,000 selected pixels. This is not AI reconstruction or PhotoCraft's full solver. |
| Assistant control | Bounded native geometry commands can be proposed against identified vector objects, then reviewed and kept with Undo. This establishes a command implementation, not fresh live-provider qualification. |

## Final candidate and completed validation

`source-final.json` records **642 frozen source paths**; its SHA-256 is
`20266ba6e1ae8c2fae6e66294d047a119d0e78de1f3e867b4ada0ab99d8ba39d`.
The final native application SHA-256 is
`b0ef1f3a485adf1e670f17bdb4591cb4467f55326f52350efcb399693eb1bea7`.
These identities supersede the earlier snapshots and binaries, not the
published release identities.

| Final check | Result and evidence |
| --- | --- |
| Optimized Cargo suite | Exit 0; 1,554 reported passes, 0 failures, 5 ignored benchmarks across 69 test targets in `cargo-final-clean-profile.log`. |
| Optional fixture scope | `external_layered_fixture_when_configured` and `external_unsupported_fixture_when_configured` returned early because `OMUSE_PSD_FIXTURE` and `OMUSE_PSD_REJECT_FIXTURE` were unset. They are reported passes, not exercised external-fixture checks or counted ignores. Checked-in PSD/PSB and synthetic codec/ICC cases ran separately. |
| Doctests | `doctests-final.log` succeeded with 0 tests; this adds no tested examples. |
| Editing journey | `editing-final/results.json` passed paint, Undo/Redo, save/reopen, PNG/JPEG/WebP/TIFF, gradient and adjustment checks. |
| Mixed composition/exchange | All checks in `workflow-final/receipt.json` passed: one-step edits, exact Undo/Redo, saved scene/composite, native text, source/history preservation, layered PSD reimport and SVG reopening. |
| Create and motion | `create-final.log` completed `--motion`, producing the editable project/story variant, social PNG/PDF package, motion preview, MP4 and GIF. This is output generation, not playback/device or creative-quality acceptance. |
| Template catalogue | `catalog-final/results.json` passed 80 editable variants with native text/pixels preserved. Fifteen variants retain preflight notices; a pass does not mean every layout is warning-free. |
| Repository checks | Shortcut-reference check, Cargo formatting and `git diff --check` passed. The wrapper's first trailing Python check encountered mise trust under isolated XDG paths; rerunning with `/usr/bin/python3` succeeded without changing trust configuration. |

## Focused local evidence

| Check | Recorded result | Evidence and scope |
| --- | --- | --- |
| Geometry | 15 passed; 0 failed/ignored | `geometry-focused/receipt.json`, `test-output.txt`; current geometry/boolean sources compiled with cached shared scene/path types. Includes straight-cubic simplification regressions, holes, transformations and bounded refusal. |
| Shape Builder | 11 passed; 0 failed/ignored | `builder-focused/receipt.json`, `test-output.txt`; isolated production modules and cached shared types. Covers source preservation, paint order, holes, fast drags, cancellation and work limits. |
| PSD export | 9 passed; 0 failed/ignored | `psd-alpha-fix/receipt.json`; current adapter and publication helper, cached Omuse library. Includes safe no-replace publication, 300 layers producing all 301 warnings, and an independently specified transparent-edge encoding case. The earlier `psd-focused` run had eight tests and preceded the matte correction. |
| Snapping | 13 passed; 0 failed/ignored | `omuse-vector-snap-check.log`; screen-space tolerance, deterministic ties, guides/grid, excluded anchors, dense-query refusal and source preservation. |
| Texture kernel | 6 passed; 0 failed | Texture evidence `qualification.json` and `unit-tests-final.log`; isolated production kernel, not the integrated app. |

The geometry, Builder and PSD harnesses use the recorded shared-library SHA-256
`948200a8810719b8b837e73411ab2b3ce144fb9360107647d739f07f90af761b`.
Their receipts identify separately compiled production sources and executables.
Focused checks do not substitute for the final integrated suite.

The earlier kernel log also records passes for repeat (10), split/join (4),
16-bit PSD import (7), Texture recipe persistence (1) and SVG exchange (5).
It retains initial failures in straight-cubic simplification and assistant
parameter rejection. The corrected cases passed the final integrated run.
A completed diagnostic run reported 1,543 passes, eight
failures and five ignored benchmarks: two new matte tests linked against the
earlier library, five PSD UI checks stopped at an invalid history fixture,
and one handle-coupling fixture needed snapping disabled to isolate exact
coordinates. Those fixtures are corrected and retain their original safety
assertions and passed the final run.
Earlier seven/eight-test PSD runs predate the latest
transparent-edge regression and must not be presented as the nine-test result.

The subsequent `cargo-final.log` reported 1,553 passes and one startup test
failure. Its reused validation profile contained 78 retained recovery packages;
the legitimate recovery modal blocked the fixture's inspector click after the
splash disappeared. The unchanged frozen source passed in a fresh
`validation-clean-profile`, matching the fresh-profile contract of
`scripts/test-rust.sh`. This was not fixed by weakening the assertion or adding
a production delay. Both diagnostic logs remain evidence.

`source-before-validation.json` is an **earlier** 634-path snapshot predating
geometry/command, transparency and fixture corrections. Use `source-final.json`
for this completed checkpoint.

## Composition, preservation and native interaction

`workflow-early/receipt.json` records a synthetic 1000 × 700 composition with
34 vector objects and a nine-layer PSD. It checks one-step document edits,
exact Undo/Redo, exact saved scene/composite, retained native text, unchanged
source/history during exchange and exact PSD composite reimport through Omuse.
SVG was reopened/rendered. Poppler independently rendered the single-page
vector PDF at 1000 × 700 (`vector-pdf-poppler.png`); visual review found its
geometry intact. Against the white-composited PNG, normalized RGB RMSE was
0.00585504, including renderer antialiasing differences. This small vector-only
fixture does not qualify mixed-document PDF export or Photoshop interoperability. The helper executable SHA-256 is
`32703cbf8f9d63694ab77109f7bff1938d799f2649eb3744fde9b8d61716bd03`.
It is an **early workflow binary**, not the final integrated candidate.

The final workflow repeated these preservation checks successfully.
`workflow-final/receipt.json` identifies its separate helper executable as
`0c293890329d1efb42bb13497cb0b04b2c1ee1cb30fe0d20f183cfd8c6932533`.
The final native application also opened the resulting composition; the
retained text, vector artwork and layout were visually reviewed in
`final-29-composition.png`.

An isolated Linux/XWayland Cua journey used an early application executable,
SHA-256 `2e48827ac098c8b224a8a8891c226adf3837354a331b11f23291ff1152af0bc3`.
It predates the straight-cubic, assistant-schema, warning-count and Builder-name
fixes. The normal installed application and user profile were left untouched.

- `native-01`–`native-09` show opening the fixture, vector selection, Shape
  Builder merge, draft Undo/Redo, keeping artwork and document Undo.
- `native-10`–`native-16` show command search, five generated grid copies,
  Undo restoring the original two objects, and repeat controls in the shared
  inspector.
- `native-17`–`native-21` show normal editing, PSD destination entry, successful
  export status and discovery of **Export conversion report**. The resulting
  33,942-byte PSD was independently identified as a 480 × 320 merged image plus
  one pixel layer. `native-22-export-report.png` shows the readable report.
- `native-23`–`native-32` show opening the car evidence photo, rectangle
  selection, finding controlled removal, choosing Texture, Preview, Apply
  creating a separate removal layer, and one Undo restoring the original.
  The original file was not overwritten. These checks used the same early
  executable; they do not qualify the later fixes.

The **final application binary** was then exercised in a separate isolated
Linux/XWayland profile, retaining screenshots `final-01`–`final-29`:

- `final-01`–`final-09`: Shape Builder merge with the corrected **Merged shape**
  label, draft Undo/Redo, Enter to keep the artwork and document Undo.
- `final-10`–`final-14`: a fresh PSD export and **Ctrl+K → Export conversion
  report**, showing the destination and readable conversion warnings.
- `final-15`–`final-26`: car-photo selection, **Texture · experimental** at a
  64-pixel search radius, Preview, Apply to a separate editable removal layer
  with the original retained, then Undo restoring the original. Completed
  status and layer count were checked; `final-24` is the intermediate preparing
  state and `final-25` is the completed edit.
- `final-27`–`final-29`: opening and visually reviewing the final composition.

The normal installed application and user profile remained unchanged. Native
cancellation while synthesis was in flight was **not** exercised; automated
cancellation tests do not replace that missing interaction check.

These are observed visual states and bounded interactions, not exhaustive UI
or accessibility acceptance. Background Ctrl+A did not select; a background
drag timed out while attaching its input device. Foreground input worked. One
accessibility-token Export click targeted the wrong pixel location without
exporting; a fresh screenshot-guided foreground click succeeded. Delayed
captures/text entry sometimes needed fresh state. Screenshots establish the
observed interaction states, not exact source pixels or performance. Final
claims above are tied to the final binary; early screenshots remain historical.

Texture evidence includes ten independently decoded PNG outputs whose recorded
pixel hashes match and whose unselected pixels remain exact. A complete car
selection produced plausible asphalt with a remaining curb-edge defect; a
portrait retained a visible seam. Held-out foliage reconstruction error was
worse for Texture than Context (RGB MSE 80.0536 versus 71.7148). A small patch
on an unresized 4310 × 2868 image preserved all 12,360,056 outside pixels.
An incomplete car selection is retained as a failed demonstration. These
results support preview-driven choice, not a general quality or speed win.
See the [full texture record](texture-removal-qualification.md).

## Independent transparency defect and correction

The first native PSD exported exact straight layer RGBA, but an independent
ImageMagick read exposed dark fringes in the merged compatibility preview:
834 partially transparent edge pixels differed. Stored merged planes were exact
copies of straight RGBA, which was the defect: Photoshop-compatible merged RGB
uses a white matte when transparency is declared. Omuse's own layer-based
round trip could not expose this error.

The adapter now mattes only the merged preview; layer pixels and alpha remain
unchanged. `psd-alpha-fix/imagemagick-verification.json` records maximum
alpha-weighted RGB error falling from 59.86 to 0.82 on a 0–255 scale, within a
conservative one-step bound. Remaining differences are expected 8-bit matte
quantization, not an exact merged-preview claim. This convention is supported
by the [pinned psd-tools writer and reader](https://github.com/psd-tools/psd-tools/blob/b58704c1c9c9b2459f961560b1e368dfb102b513/src/psd_tools/api/numpy_io.py).
The importer now removes that matte before ICC conversion for declared 8-bit
fallback and 16-bit merged transparency, preserving alpha and zero-alpha hidden
RGB. The isolated importer harness reported 36 passes; two optional external-file
checks returned early because their inputs were unconfigured (34 exercised
checks). It covers PSD/PSB, all four codecs, source-space ICC ordering, safe
clamping, saved precision and unchanged layer-channel interpretation.
`psd-alpha-import-2026-10-11/independent-reader.json` records ImageMagick exactly
matching six independently authored 16-bit samples, both with normal unblending
and with unblending disabled to inspect the stored planes. These changes also
passed the final integrated run; optional external-file checks remain
unexercised as listed above.

`final-psd-independent.json` verifies the final native export and final workflow
outputs using independent decoders, with inputs unchanged:

| Reader | Final result and limits |
| --- | --- |
| ImageMagick PSD | The 480 × 320 individual layer is exact RGBA; merged alpha and opaque/transparent pixels are exact. Six hundred partial-alpha pixels differ after unmatting, with maximum alpha-weighted RGB error 0.8118 on the 0–255 scale, below the one-step bound. This is a bounded rounding result, not an exact merged-preview claim. |
| librsvg SVG | The final vector-only 1000 × 700 artwork rendered with matching extent and objects in visual review. Against the white-composited reference, 5,778 pixels differ; all lie within two pixels of reference edges. Omuse's own SVG reopen PNG is exact. |
| Poppler PDF | One vector-only 500 × 350-point page rendered at 1000 × 700 with matching geometry in visual review. There are 5,630 differing pixels, all within two pixels of reference edges. The PDF excludes the card's separate text/photo layers. |

These checks qualify the named synthetic fixtures and decoder paths. They do
not establish Photoshop/Affinity behavior, general PDF interchange or universal
agreement between rasterizers.

## Gates still open

- Qualify Windows compilation/packages and interactive behavior for this
  candidate, plus clean-machine installation, release assets and signing.
- Compare exchanged files in Photoshop/Affinity and broaden independent-reader,
  ICC, font, non-Latin, malformed-file and large-document coverage beyond the
  fixtures above. Supply the two optional external PSD fixtures where relevant.
- Exercise live providers and subscriptions separately; measure latency and
  peak process memory under controlled load. Kernel observations and allocation
  estimates are not complete-application performance results.
- Continue photographic removal/seam evaluation and native cancellation-in-flight
  checks. The completed Preview/Apply/Undo journey does not justify a
  default-method change.
- Keep live repeats/booleans, symbols, variable-width strokes, mesh/envelope
  tools, paragraph flow, general SVG clipping/compositing, mixed vector PDF,
  print colour and broader camera/RAW/HDR work on the roadmap.

Dependency source/license notices are retained, with receipt checks for the
new curated notices. That source-level check alone does not qualify a newly
built distribution. Calendars, social scheduling and tablet qualification
remain outside this batch.
