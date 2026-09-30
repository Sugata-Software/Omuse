# Editing improvements following the OmaPhoto 1.3.3 review

Work branch: `feature/omaphoto-133-editing`, based on Omuse 0.5.0.
The implemented backlog is released as Omuse 0.6.0. The complete installed
0.5.0 remains the rollback baseline; final evidence is linked below.

Released application version: **0.6.0**. New saves write canvas format **10**,
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
- [x] Run focused regressions, full suite and production/native journeys; review
  the built UI and qualify installation/rollback before replacing the live app.

## Completed qualification

Public source `a8b7ac70e7513d305a671673a347eecaf2d6cc4c` passed the [complete GitHub workflow](https://github.com/Sugata-Software/Omuse/actions/runs/36784517002).
The [0.6.0 qualification](release-060-qualification.md) and
[compact receipt](release-060-receipts.json) record 1,105 local application cases:
457 unchanged library, 321 unchanged integration and the final 327-case UI run.
Editing/Create/motion, 80 template variants, three 24-check native journeys,
previous-reader compatibility and complete 19-file installation rollback passed.

The combined review fixed ordinary text deletion restoring old styles, queued
saves closing newer dialogs, stale crop geometry after reload, dangling merge
mask links, changed SVG sources, cached Photoshop-text rasterization, optics-stage
Camera Raw sampling, transparent-foreground Eraser/Clone/Heal and compact-window
controls. Lower finishing controls are reached through real scrolling and native
field typing in the final UI regression.

Small MIT-licensed independent Photoshop PSB and TypeTool fixtures include hashes
and provenance. They do not establish embedded-ICC support or complete Photoshop
interchange. The earlier Cua walkthrough is recorded separately from final
production/native qualification.

## Boundaries

Keep existing provider behaviour, original artwork and `.omuse` naming. Retain
compatibility failures as explicit errors rather than silently dropping content.
Cross-app performance claims, physical hardware not available on this host,
macOS-specific behaviour and portable binary distribution require their own
evidence. Calendars, scheduling and tablet qualification remain out of scope.
