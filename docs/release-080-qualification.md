# Omuse 0.8.0 candidate qualification

This is a development record, not a release declaration. The installed and
published source release remains 0.7.0 while the candidate is being checked.

Public checkpoint `f40d37d7932ec321f5dacbcdd65ae94773481bc8` on
`release/0.8.0` completed its Linux/Windows library checks, but both GUI suites
found the same two failures. The JPEG panning fixture was smaller than its
viewport on GPUI's 2× test display; the Hue output controls still clipped at
minimum size. The revised candidate enlarges the JPEG fixture, reduces the
compact range preview height and explicitly checks control visibility.

The first `ebadd073` checkpoint failed compilation because the range inspector
exposed Intersect before the selection engine defined it. Soft coverage
intersection and its tests are now present. No failed checkpoint is a release.

Final review also corrected nearest-pixel sampling in transformed editable
filter masks, mismatched extents when adding masks to image-less groups or
adjustments, and range previews that ignored Add/Subtract/Intersect. Focused
before/after fixtures reproduce those defects; the combined suite must verify
all fixes before promotion. Final public source and workflow receipts will be
recorded after that run.

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
Those were corrected before the candidate. The startup test also encountered
the developer's real recovery prompt; the unchanged test passed with an isolated
XDG directory. The complete release suite uses isolated XDG directories.

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
ICC profiles. Final native package jobs must generate and verify their own
exact inventory again.

## Remaining qualification

- Complete optimized library, integration and GPUI interaction tests, editing,
  Create/motion and all template journeys for the combined candidate.
- Capture and inspect the new native interfaces in both themes and a compact
  window; verify save/reopen, cancellation, Undo and export behavior.
- Check sustained sessions, interrupted saves, production-feature builds,
  isolated installation, replacement and rollback.
- Review exact-source Linux/Windows CI and actual native package receipts before
  declaring the source release and permanent unsigned preview downloads.
- Live Windows Codex/Claude jobs, clean-machine acceptance, mixed-DPI,
  accessibility and broader hardware checks require their separate environment
  and evidence. No Windows machine is connected to this development session.

No current test establishes complete Photoshop/Illustrator parity, universal
photo quality, press-ready colour, tablet qualification or a signed stable
release. Calendar/scheduling work remains outside Omuse's scope.
