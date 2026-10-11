# Progressive photo previews and colour regression

**Historical development evidence.** The implemented changes below are included
in Omuse 0.10.0. The [0.10 release qualification](release-0100-qualification.md)
records the final source, CI, packages and publication status. Measurements,
source identities and open-gate statements below describe their original
checkpoint; they are not silently rerun or promoted by that release.

Unreleased development work, 10 October 2026, based on `1c987c1` and the existing
content-workflow changes in `Omuse-next-studio`. Published 0.9.0 packages do not
contain this change. The tested candidate was subsequently installed locally on
10 October, as recorded below. The complete follow-on direction is in the
[photo development roadmap](rawmakase-roadmap.md).

## Behaviour

Camera Raw can show a quick, sampled draft before the existing full-resolution
preview on a single ordinary canvas-sized raster, with no selection, mask,
transform, live content or active metadata effects. Normal opacity and document
background are retained. Known inert saved-layer and source-profile metadata
are admitted. Unknown metadata, Blend If and unsupported compositions use the
existing full-resolution path.

Exposure, white balance, tone, curves, colour mixer, grading and calibration
use the same pointwise calculations in both preview stages. Other settings, including detail,
geometry, optics, clarity, texture and grain, retain full rendering. The draft
samples source-pixel centres rather than averaging colours before nonlinear
operations. Admitted draft pixels therefore match sampled full-grade pixels;
fine patterns can still alias during temporary display minification.

Drafts require a source of at least 262,144 pixels, reduced physical-pixel scale
and at least a halving of the graded pixel count. Drafts are capped at 1,536
pixels on the long edge and 1,048,576 pixels total. The one-entry source cache
shares the original and adds at most 4 MiB of sampled RGBA pixels. Graded output,
composites, display textures, the document, history and sampling references are
additional memory; this is not a process-memory cap.

The status says **Quick preview · refining full detail…** until the full worker
finishes. Scopes are withheld during the draft and colour gestures remain
disabled until the full-size stage reference is ready. Apply always uses the
original pixels, original selection/placement rules and one Undo transaction.
Editable vector scenes now require explicit raster conversion before Camera
Raw instead of accepting a stale derived raster cache.

Draft and full stages share the existing one-active/one-newest-queued grading
lifecycle. Cancellation and source/document/page/selection/dialog checks guard
both stages. A new request takes precedence over a completed old draft; Apply
cannot be displaced by a later preview. Cache reuse requires the same source
allocation and dimensions, and closing the dialog clears the cache. Cancellation
restores committed pixels even before the native-close Unsaved prompt. Failure
after a displayed draft also restores the original view. The separate opening-scope
analysis remains a background task; the grading limit is not a global-worker cap.

## Colour corpus

The [synthetic colour corpus](../rust/tests/reference/photo-color/README.md)
records Omuse rendering references separately from independently validated
colour-difference calculations. It is a regression guard, not proof of camera
calibration, Adobe parity, display calibration or photographic preference.
Reference changes require explicit generation into a new directory and review.
Ordinary tests never overwrite the committed reference images.

The first candidate references were rejected: lifting near-black colours with
the legacy luminance ratio could create saturated spikes, and strong shadow
lifts/highlight reductions could reverse a neutral gradient. New Camera Raw
dialogs explicitly select `SmoothV1`: luminance moves colours toward white or
black within gamut, and shadow/highlight compression stays monotonic across
the slider range. A neutral recipe still returns exact original RGBA bytes.

Saved recipes without `toneMapping` retain `Legacy` behaviour, including its
limitations. The legacy implementation is not retuned. New versioned recipes
require a build that understands them; older builds reject the unknown field
and can refuse to open the containing project. Preserve the original and share
a separate copy with those editable layers rasterized. Ordinary Camera Raw
Apply produces raster pixels and keeps its
existing document compatibility. Unknown future versions are rejected.

## Validation

The development working tree includes earlier content-workflow work. The base
commit alone does not identify this candidate. Local evidence lives under
`Omuse-release-evidence/photo-preview-2026-10-10` and
`Omuse-release-evidence/photo-color-corpus-2026-10-10`, outside the repository.

| Identity | SHA-256 |
| --- | --- |
| All 561 Git-visible Rust paths, including untracked tests and references | `e55035b709f7b49f5fefb928941335e38271f970bb47dc6aee6c49736deaaf79` |
| Native release executable, without `ui-test` | `267bc1e59f8e01c625c629d7b5163d5e25bd944075eb2ed8f2a52bb3b94391cf` |

The before/after source manifests match exactly. The tested Rust tree did not
change during qualification. The qualification pass did not install, push or publish a package. The later
user-requested local installation is recorded separately below.

Focused release checks passed: **9 colour-corpus tests**, **18 Camera Raw library
tests**, and **5 preview integration tests**. The separate manual preview
benchmark was then explicitly run three times. The preserved C-kernel checks
remain intact. All fifteen legacy corpus recipes also matched their saved
pre-change RGBA output exactly. The independent colour-difference calculation
passed all 34 published CIEDE2000 reference pairs, with maximum absolute error
0.0000495 against their rounded expected values.

The full Linux release/`ui-test` run reported **1,449 passes, zero failures and
five intentionally ignored benchmarks**: 556 library, 410 UI and 483 integration
passes. One optional unsupported-PSD test returned early because its external
fixture was unavailable, so **1,448 test bodies were exercised**. All eleven
Camera Raw UI lifecycle tests passed, including full-size Apply/Undo, failed
refinement, queued-request guards and restoring the committed display before
a native-close prompt.

Real Nikon NEF tests used LibRaw 0.22.2 and exercised exposure development,
retained 16-bit opening and re-development after removal of the external source.
The configured PSB file-open check used the existing one-layer fixture; this
does not add broad external PSD coverage. These are existing import regression
checks, not completion of the retained-RAW roadmap phase.

`scripts/test-rust.sh` completed successfully, including shortcut documentation
and formatting checks, the test suite, eleven editing self-test operations,
Create acceptance with PNG/PDF, story variant, MP4/GIF outputs, and all **80
template variants** with native text/pixels preserved after reopen. Test
documents are retained in `/tmp/omuse-tests.tWulI4gs`; the full log and execution
receipt are retained with the candidate evidence. No fresh live AI-provider
request or visual review of all template variants is implied by these checks.

### CPU measurement

On this Linux x86-64 laptop (Intel i7-4980HQ, 16 GB RAM, Rust 1.98.1), three
sequential release runs graded a synthetic 4000 × 3000 RGBA source with
`SmoothV1`, exposure +0.7, contrast 15 and shadows 20. The draft was 1182 × 886.
No compiler was running during these measurements.

| Run | Sample preparation + draft grade | Full-size grade |
| --- | ---: | ---: |
| 1 | 535.540 ms | 6362.664 ms |
| 2 | 552.988 ms | 6165.858 ms |
| 3 | 533.361 ms | 6077.117 ms |
| Median | **535.540 ms** | **6165.858 ms** |

This is about 11.5 times less elapsed CPU-stage time for the sampled work. It
does **not** measure input-to-display latency, compositing, scopes, texture
upload, cold-file loading or total time to settle. Full refinement still follows
the draft. It is not a GPU benchmark or a comparison with another application.

### Native Linux inspection

A separate normal release executable opened an existing, developed 4310 × 2868
Nikon D90 photograph in an isolated profile through XWayland. Cua Driver 0.29.1
captured its 1891 × 1150 window at fitted zoom. Native preview visibly entered
the quick stage and later completed full refinement. Apply, immediate keyboard
Undo/Redo, completed-preview Cancel and a Cancel request after the displayed
quick stage were exercised. Later inspection found no republished stale image.

The displayed photo after Redo exactly matched the applied screenshot. Undo
differed from the initial screenshot in one channel by one byte; this is a
display comparison, not a source-pixel equality assertion. All three Cancel
screenshots exactly matched the pre-preview photo rectangle. Source/history
equality is checked separately by automated tests.

During a 15-minute observation window covering the first preview and history
journey, the candidate's largest sampled RSS was 465.2 MiB and its reported
process high-water RSS reached 511.6 MiB. This includes the document, history,
textures and GUI; it does not establish a worst-case memory bound, leak freedom
or memory usage of other document sizes. Compilation coexisted during GUI
inspection, so native timing was not used as a performance result.

Foreground coordinate controls and background Ctrl shortcuts worked. A
synthetic background Escape did not dismiss the dialog, and accessibility-token
clicks were rejected as stale; neither is counted as a successful check.
The visible Cancel button succeeded. The isolated test window/session was
closed, and the installed application's binary hash remained unchanged.

Two licensed photographic holdouts (Nikon foliage/sky and a Canon racing car)
were also visually compared under moderate exposure and shadow/highlight edits.
The revised mapping avoided the previously observed washed-out colour without
an obvious new discontinuity at the inspected scale. This is limited visual
review, not calibrated colour accuracy or broad photographic qualification.

## Subsequent local installation — 10 October 2026

The user requested installation after qualification. The frozen normal executable
above was packaged with the existing LibRaw, ONNX Runtime and U2Net assets and
installed through `install-rust-bundle.sh` as the normal Omuse application. No
Rust source or executable changed. The local development bundle SHA-256 is
`d5183f13d332129354ccb4087b38d550f5bc8fa51ececcc0657939236f31d8fa`.
Its source metadata explicitly records a dirty tree; the exact source manifest
and executable hashes above identify the candidate. The embedded version still
reads 0.9.0. This is not a new public release or a replaced public asset.

Installation verified the complete checksum manifest and ran the candidate's
self-test in an isolated profile before activation. The installed executable
matches the qualified binary. All 22 files in each older 0.9.0 and 0.8.0
generation remain byte-identical; the rollback pointer now selects 0.9.0.
Existing configuration, saved data and state files were unchanged by the
installer, checked before the normal-profile launch. Rollback is available with
`omuse-manage rollback`; it was not invoked during this installation.

The normal launcher started the installed executable with the normal profile on
Wayland, verified through the running process path and binary hash. Cua could
not discover that Wayland window, and its desktop capture failed with an X11
GetImage error. No visual Wayland qualification is claimed. A separate isolated
XWayland launch through the same installed launcher opened a real Nikon D90 NEF
at 4310 × 2868. Command search opened **Develop embedded RAW**; exposure +0.7
and **Apply** completed with **Editable source updated** and a visibly brighter
photo. The result was saved as an `.omuse` project with the original NEF bytes
retained exactly and exposure +0.7 persisted. The installed CLI reopened that
project and exported a 4310 × 2868 PNG. The test window was then closed. The
bundled LibRaw also passed the real-camera exposure-control test.
This exercises the existing retained-RAW path, separately from the new 8-bit
Camera Raw preview pipeline.

Installation receipts, process hashes, profile hash inventories and screenshots
are retained in the qualification evidence directory's `install/` subdirectory.
The private project guide was updated to distinguish the installed development
build from the unchanged public release.

## Remaining boundaries

The Camera Raw dialog still operates on 8-bit raster layers within the 16 MP
limit. This change does not unify it with retained sensor-RAW/16-bit development,
add GPU grading, persist preview caches, reinterpret unversioned saved recipes or
accelerate complex compositions. Zoom or DPI changes during a running request
can temporarily magnify its draft; automatic full refinement remains exact.
The initial draft adds some work before full refinement and may improve first
feedback while increasing total settling time. Measure these separately.

The current modal obscures much of the canvas, and its pre-white-balance colour
reference is not the final graded image. Hiding/restoring that reference during
processing can shift the visible controls. A photo-visible development layout,
stable control positions and native Escape/focus qualification remain planned
UX work. These observations prevent describing this as finished Camera Raw
interaction polish. Windows, native Wayland, calibrated displays and GPU image
processing were not qualified in this pass.

The implementation is original Omuse code. RAWmakase supplied architectural
inspiration; no upstream source, profile or model was copied into this change.
