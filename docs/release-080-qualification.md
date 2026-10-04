# Omuse 0.8.0 release qualification

**4 October publication update:** [publication run 37200321434](https://github.com/Sugata-Software/Omuse/actions/runs/37200321434)
completed successfully. All four permanent assets downloaded anonymously and
matched the reviewed manifest sizes and SHA-256 hashes. The source tag remains
`eb558dc59ccdf88a3d62dfc2d706a6b836da1eda`; published release notes are unchanged.
The public Linux archive passed isolated installation, same-version replacement,
rollback and profile-preservation checks. Windows archive verification passed;
this update adds no local Windows desktop or live-AI qualification.
The earlier "pending" statements below describe the record before publication.


This is the qualification record for the 0.8.0 source release. The GitHub source
release is published as a prerelease; permanent binary assets remain pending and
this record does not claim stable, hardware or broad Windows GUI qualification.

The published source release is `eb558dc59ccdf88a3d62dfc2d706a6b836da1eda`
tagged `v0.8.0`, with source tree
`68e406316b92d5817abfe810fbdc7840e57b5d17`. It changes only the dependency
helper and its four regression checks/workflow wiring; the Rust application
source is byte-identical to b47/d2. The b47 Linux and Windows application
suites and journeys passed. The repaired helper passed 20 Rust and 4 Python
checks, Windows dependency metadata passed, and the exact-source validation and
download runs completed successfully: [validation run 37161936414](https://github.com/Sugata-Software/Omuse/actions/runs/37161936414)
and [download run 37161936378](https://github.com/Sugata-Software/Omuse/actions/runs/37161936378).
This candidate includes all earlier compact JPEG, Camera Raw/Subject Refine
focus, 16-bit blend and external-format-11 fixes.
The 0.8.0 source tag and prerelease are published at
[GitHub](https://github.com/Sugata-Software/Omuse/releases/tag/v0.8.0). The
permanent release asset array is empty, while the installed launcher now selects
0.8.0 and the preserved 0.7.0 generation remains available for rollback.

The earlier public `f40d37d7932ec321f5dacbcdd65ae94773481bc8` checkpoint is
historical. Its Linux/Windows GUI runs exposed the undersized JPEG panning
fixture and clipped Hue controls; those findings drove the fixes above.

The first `ebadd073` checkpoint failed compilation because the range inspector
exposed Intersect before the selection engine defined it. Soft coverage
intersection and its tests are now present. No failed checkpoint is a release.

Final review also corrected nearest-pixel sampling in transformed editable
filter masks, mismatched extents when adding masks to image-less groups or
adjustments, and range previews that ignored Add/Subtract/Intersect. Focused
before/after fixtures reproduce those defects and passed in the combined suite.
The source release and workflow receipts are now recorded;
permanent asset publication remains a separate release-owner step.

## Implemented scope

- Correct PSD Levels gamma, channel/output levels and explicit mask coverage;
  preserve antialiased selections when mapping them into transformed masks.
- Read supported external format-11 UTF-16 font runs without replacing cached
  artwork or discarding undeclared native vector sidecars.
- Improve Smudge carry, sample Liquify from original stroke pixels and restrict
  brush Blur to the affected area plus its required sampling halo.
- Add hue-based range masks with grey protection and selection combinations.
- Retain an editable reference-image palette match, with strength/lightness
  controls, masks, cancellation and native 16-bit working-space evaluation.
- Integrate multiselection, organizational groups, boolean construction,
  editable SVG gradients/strokes, text on curves and vector PDF export.
- Inspect the actual encoded JPEG at Fit or physical-pixel 100% with panning.
- Build immutable Linux/Windows archives with exact notices, native smoke
  receipts and a separately reviewed permanent-publication manifest.

## Evidence accumulated before the final run

The earlier combined `73a2d6b` checkpoint passed 545 library/integration cases,
including independent PSD pixels, soft mask mapping, external font runs,
gradient rendering and styled project persistence. Later focused engine checks
cover reference matching, transformed curved text, fallback fonts, SVG/PDF
outline export and high-zoom paints/dashes. These smaller runs do not substitute
for the final combined suite.

The exploratory debug GUI run found clipped JPEG/range controls and a semantic
round-trip comparator that did not normalize the new, validated scene inventory.
Those were corrected before the current candidate. Camera Raw and Subject
Refine focus handling and 16-bit blend rounding were corrected afterward; the
native Camera Raw Apply → keyboard Undo/Redo journey passed. The installed
Subject Refine journey also passed Apply → immediate Ctrl+Z/Ctrl+Shift+Z with
the bundled local model on one public portrait; residual flag pixels remained
and the editable mask needs cleanup, so this is not a general model-quality
claim. A new compact-window resize capture was unavailable. The startup test also
encountered the developer's real recovery prompt; the unchanged test passed
with an isolated XDG directory. The complete release suite uses isolated XDG
directories.

The current installed 0.8.0 launcher passed its isolated-XDG editing journey with
binary SHA-256 `d47feb79e0cc893786f5aa8cd60dc3ef2afdb4975b2ea0cb6f19f3dcbe9abf56`;
the complete prior 0.7.0 20-file payload was preserved for rollback.

The final release evidence packet records 16/16 real-photo cases passed in
`real-photo-d2dac18/results.json`, 14 recovery-worker checks with a 12-revision
session and SIGKILL at revision 4 passed in `recovery-qualified-d2dac18/results.json`,
and the optimized local suite passed 1,352 cases (516 library, 378 GPUI and
458 integration; four ignored). The b47ba4d Linux cross-version report passed
0.7.0 → 0.8.0 → rollback → forward with four installed-launcher journeys.
Native b47 Camera Raw Apply → keyboard Undo/Redo passed; compact JPEG behavior
passes GPUI, while a new native compact-resize capture was unavailable and the
full-size native screenshot is the applicable evidence.

The earlier b47ba4d application suites and journeys passed on Linux and Windows.
The eb558dc exact-source validation and download workflows completed
successfully. Linux and Windows package checks passed with zero notice findings
(619 and 421 dependency entries respectively). The source release is published;
permanent binary asset publication remains separate and pending.

Native gradient inspection exposed a conservative stroke-work estimate that
rejected a simple supersampled scene. Styled stroke accounting now follows its
scan-conversion path, and a high-resolution preview that exceeds its budget
falls back to the valid settled image. Paint matrices, dash lengths and retained
text transforms follow the preview scale. Work, geometry and memory limits
remain enforced.

Independent SVG and Poppler PDF rendering compared three fixtures: rotated
curved text, styled curved text, and a translucent radial gradient with dashed
stroke. Portable geometry and paint survived; edge antialiasing and small
gradient differences remain renderer-dependent. PDF does not support Repeat or
Reflect gradients in this implementation; export reports an error for them.

Five alternating isolated 8-megapixel retouch runs on the development host
measured median Blur kernel time of 9550 ms before / 37.7 ms after, Smudge
216.6 / 61.2 ms, and Liquify 83.1 / 32.6 ms. Blur process RSS was 199884 /
68548 KiB in that harness. These are synthetic CPU-kernel measurements under
host load, not pointer latency, whole-application memory or a general speedup.

The `73a2d6b` dependency inventories report zero unresolved findings: 619 broad
Linux packages, 536 Linux release-build packages and 417 Windows target
release-build packages. Complete upstream notices include svg2pdf's embedded
ICC profiles. The final eb558dc native package jobs separately generated and
verified their exact inventories: 619 Linux and 421 Windows entries, with zero
notice findings.

## Remaining qualification

- Publish reviewed Linux/Windows binary assets only after the authenticated
  download publication workflow completes; the source release itself is already
  published as a prerelease.
- Retain the native compact-window resize capture as an optional broader-evidence
  gap; the initial qualification has GPUI compact JPEG coverage and a full-size
  native screenshot.
- Keep Windows GUI/AI, mixed-DPI, clean-machine and broader hardware acceptance
  outside this initial unsigned preview qualification.
- Live Windows Codex/Claude jobs, clean-machine acceptance, mixed-DPI,
  accessibility and broader hardware checks require their separate environment
  and evidence. No Windows machine is connected to this development session.

No current test establishes complete Photoshop/Illustrator parity, universal
photo quality, press-ready colour, tablet qualification or a signed stable
release. Calendar/scheduling work remains outside Omuse's scope.
