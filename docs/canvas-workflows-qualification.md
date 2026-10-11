# Canvas workflows: development qualification

Development after Omuse 0.10.0 on `feature/canvas-workflows`. This record covers
the contextual canvas dock, retained auto-layout and local AI comparison UI.
The published and installed 0.10.0 release remains separate.

## Local qualification completed — 11 October 2026

The Linux release-profile suite passed **1,599 tests, zero failures and five
ignored manual benchmarks across 71 targets**, using `--locked --offline`,
`--features ui-test`, one test thread and a fresh four-directory XDG profile.
The run includes 18 layout engine tests, four layout UI tests, ten context UI
tests, the AI comparison helpers and composer/routing regressions.

| Check | Result and scope |
| --- | --- |
| Editing acceptance | Passed the native engine editing journey. |
| Retained layout | Four mixed raster/live-text/vector fixtures preserve pixels, rules and editable sources through save/reopen; exact Undo/Redo, Fill, Create resize and release of rules passed. |
| Create and motion | Six-page collection, targeted editing, story variant, PNG/PDF package and MP4/GIF exports passed. |
| Template catalogue | 80 editable variants passed native text/pixel preservation and export checks. |
| Native XWayland | 24 scripted GPUI checks passed, including painting, text, filters, 16-bit source/export, photo workflow, palette and keyboard focus. |
| Cua interaction | Verified frame bounds, Ungroup followed by foreground Ctrl+Z, Auto-layout dialog and Escape. Verified proportional AI pairs, Changes, original comparison, Refine, Collapse and exact typed draft after reopening. |
| Older reader | The actual installed 0.10.0 binary rejects retained format 15. Releasing rules produces format 14; its older-reader PNG matches exactly (absolute pixel error 0). |

Cua screenshots are captured from the running development app, not mockups.
The three guide screenshots show the frame dock, settings and a synthetic
local AI variation pair. The AI fixture makes no provider requests. This pass
used XWayland; production Wayland and Windows interaction are not newly qualified.
Background synthetic key delivery was ineffective for one native Undo check;
foreground delivery succeeded, matching the tested focus behavior.

Local evidence is under `Omuse-release-evidence/canvas-workflows-2026-10-11`:
`full-tests-final.log`, `full-tests-final-summary.json`, `source-identity.json`,
`editing-final.log`, `layout-final/results.json`, `create-final.log`,
`catalog-final/results.json`, `native/ai-final/native-results.json`,
`native/final-review.json` and `older-reader-final.json`. Source hashes bind the
runtime changes to the checked worktree; binaries and native capture scope are
recorded in the evidence directory. Five ignored benchmarks and unconfigured
optional external PSD fixtures are not additional qualification.

## Review findings addressed

- Ordinary floating-selection transforms were incorrectly reported as refused
  while their transaction was still open. Deferred edits now report success
  without creating a second history entry or reflowing a temporary layer.
- Format-15 photo-only documents inherited a vector-inventory requirement from
  formats 12–14. The reader now applies that requirement to the intended versions.
- Frame selection handles used the document dimensions instead of the retained
  fractional extent. Move and Ungroup now use valid frame semantics while
  retaining the existing lock, clipping, mask and effect restrictions.
- Floating-pixel growth can reposition or resize a managed source. Its retained
  selection now follows the same placement change; regressions compare selected
  coverage against rendered pixels for Hug, Fixed and Fill growth in five directions.
- Native inspection exposed a tall fixed AI composer obscuring the comparison.
  An idle review now uses a compact footer, preserving the prompt entity, draft,
  references and focus on expansion. Busy requests retain progress and Stop.
- Composer visibility now stays latched while editing, so mouse-down focus
  changes cannot remove Send or Collapse before their click completes.
- The earlier AI thumbnail path resized rectangular images to a square.
  Review thumbnails now preserve aspect ratio within a 640-pixel longest edge
  and leave smaller images at their original dimensions. Provider upload
  preparation is outside this preview change.
- Context actions return focus to the canvas before command dispatch. Commands
  can then focus their own dialog or editor. Ungroup and Hide no longer strand
  keyboard focus when their clicked dock control disappears.
- A new AI history fixture used an invalid serialized intent. It now uses the
  real tagged intent and asserts the prepared result before checking comparison UI.

The failed initial and intermediate suites are retained alongside the successful
final run so the repaired failures remain traceable.

A reused diagnostic XDG profile also contained 39 eligible, unlocked recovery
packages. The startup test encountered the real recovery modal, which correctly
intercepted its inspector click. The final run uses a fresh profile, matching
`scripts/test-rust.sh`; no recovery behavior or startup assertion was weakened.

## Deliberate boundaries

- Context actions use a stable dock rather than a draggable selection-following
  bar. Existing keyboard commands, view focus and prerequisites remain authoritative.
- Layout supports direct unrotated leaf layers, with 64 frames, 256 children
  per frame and 2,048 total members. Nested layout and general constraints are
  deferred. Sizing uses source rectangles, including transparent padding;
  text remains editable but Fill scales rather than rewrapping it.
- Retained-layout projects require native format 15. Ordinary documents retain
  earlier format requirements. Older readers must refuse new-format projects.
- AI visual differences are bounded previews with a visibility threshold, not
  full-resolution metrics or quality judgements. No new account, provider or
  subscription capability is established by local comparison tests.
- Windows interaction, broader hardware and performance at resource limits are
  separate work. No new public release or installation is claimed here.

See [workflow instructions](user-guide/canvas-workflows.md) and
[upstream source boundaries](omastrator-adoption.md).
