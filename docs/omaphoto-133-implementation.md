# Editing improvements following the OmaPhoto 1.3.3 review

Work branch: `feature/omaphoto-133-editing`, based on Omuse 0.5.0.
The installed 0.5.0 remains the rollback baseline. This file records work in
progress; checked-in source is not release qualification.

Candidate application version: **0.6.0**. New saves write canvas format **10**,
because explicit mask outside coverage and grown folder-mask placement cannot
be represented losslessly for the version-9 reader. Read versions 1–10;
`.omuse` remains the only new project suffix. App rollback does not downgrade
project files; Save As keeps an older copy.

## Implementation checklist

- [x] Read format-10 projects with validated Unicode colour-run conversion;
  preserve originals and use `.omuse` for new saves.
- [x] Accurate reversible live text/effects previews and direct selected-text
  colour editing with picker cancellation and focus restoration.
- [x] Persisted, searchable recent canvases and collections.
- [x] Shared numeric scrubbing, keyboard/typed precision, reset and slider polish.
- [x] Save/open/reload/close lifecycle checks, external-change decisions and
  preservation of existing locked, staged publication.
- [x] Dither: diffusion, Bayer, halftone, patterns and ASCII with colour controls,
  bounded previews, selection/alpha correctness and Undo.
- [x] Explicit richer Bloom, empty-layer Vignette and spatial Tonal Contrast.
- [x] Bounded PSB and SVG/SVGZ import and supported editable Photoshop text,
  with conversion notices and retained fallback artwork.
- [x] Brush/fill/gradient mask growth with placement, outside coverage and history
  preservation.
- [x] Missing crop presets and capability-aware layer actions.
- [x] Responsive selection outlines at low zoom without changing selection data.
- [x] Add stage-aware Camera Raw white-balance/defringe sampling and targeted
  curve/HSL drags with draft cancellation, coalesced work and Apply guards.
- [x] Update user instructions, shortcuts, changelog and project guide from
  implemented/tested behaviour.
- [ ] Run focused regressions, full suite and production/native journeys; review
  the built UI and qualify installation/rollback before replacing the live app.

The checked items above describe implemented source. The remaining release
qualification item is separate and is not implied by those checks.

## Focused evidence so far

- Fourteen new mask-growth and sixteen existing mask-engine regressions passed.
- Thirteen finishing-effect, three Dither helper, six outline, five crop and
  two selection-generation cases passed.
- Twenty-four PSD/text and eight SVG cases passed before an additional real
  Photoshop TypeTool padding fixture repair. Final rebuild remains required.
- Twenty-five installer cases passed, including the new image MIME associations.
- A separate integration review found and repaired style restoration on ordinary
  deletion, queued saves closing newer dialog drafts, and crop geometry surviving
  external reload. Their regressions are part of the pending combined UI suite.

An initial combined UI pass passed 314 cases and found four regressions in
modal numeric dragging/typing, the Nest chooser and minimum-width header.
Those were repaired; the final full run is in progress. A library pass passed
453 cases, with one outdated future-version fixture corrected for format 10.
Later review also repaired dangling mask links after merges, changed SVG source
identity, cached Photoshop-text rasterization and Camera Raw optics sampling.
These later changes require the final rebuilt suite.

The source includes small MIT-licensed independent Photoshop PSB and TypeTool
fixtures with hashes and provenance. Derived untagged PSD/PSB smoke inputs do
not establish embedded-ICC import; that remains an explicit refusal.

## Boundaries

Keep existing provider behaviour, original artwork and `.omuse` naming. Retain
compatibility failures as explicit errors rather than silently dropping content.
Cross-app performance claims, physical hardware not available on this host,
macOS-specific behaviour and portable binary distribution require their own
evidence. Calendars, scheduling and tablet qualification remain out of scope.
