# AI image editing and local removal qualification

29 September 2026 · Omuse 0.2.1 · Arch/Omarchy Linux x86_64.

## Final release identity

- Public runtime: [`189f3e77e33f1b9fae7a7a5d19cf8a2a3bd930dd`](https://github.com/Sugata-Software/Omuse/commit/189f3e77e33f1b9fae7a7a5d19cf8a2a3bd930dd).
- Git tree: `a4591728466431bc6cb834cf7c4686f468075ad7`.
- Normal production executable SHA-256: `fd5a5400c16f3f1462132de950a01cceee7a31675d9802577316c50e69da6012`.
- [Complete GitHub Rust/installer validation 36558893513](https://github.com/Sugata-Software/Omuse/actions/runs/36558893513): passed.
- Normal installed app: **Omuse 0.2.1**, clean public-source receipt;
  generation `install-ac5gtmjb`. Previous complete generation `install-hly2uob3`
  is 0.2.0 and remains available through rollback.

The five live image requests ran on the immediately preceding runtime
`9100ef4858f85cb179c7ba1d233229f6511a4f25`, tree
`bf6434de2a5205c00980bec5f72bbd330a9cc93f`, production SHA-256
`ae5e3f1940aef9b62212ec36b437d8022978f2b6cd110ff81067332a23ea20b8`.
Its [complete CI](https://github.com/Sugata-Software/Omuse/actions/runs/36535386162)
also passed. The final runtime changes only the local-finishing focus handoff,
UI debug selectors and its regression. It does not change the provider or image
pipeline. Live receipts retain their original identity; those requests were not
repeated on the final focus-corrected build.

## Implemented safeguards

### Protected editing masks

Replacement, removal and background preparation now evaluates locked artwork
through the complete layer tree. A locked clipped layer may depend on an
unlocked clipping base or a hidden live-mask source, so those dependencies stay
available while the protected composite is rendered. Only the visible locked
contribution is removed from the editable mask; the unlocked base is not
mistakenly protected across its whole footprint. Invalid protection renders
fail before a request is submitted instead of weakening the mask.

### Expansion preserves the original rectangle

Expand masks the generated image to preserve the original rectangle at its
translated offset. Preparation now also checks the source document
before submission when it contains live Gaussian blur, motion blur, noise or
grain adjustments. Omuse expands a temporary copy, reevaluates that copy, and
compares every translated source pixel with the original composite. If the
adjustment would change any original pixel, expansion is refused with guidance
to keep the current canvas size or explicitly flatten an unlocked duplicate.

This guard does not reject an adjustment merely because of its name. It allows
inactive or actually unchanged spatial/noise adjustments, pointwise live
adjustments, and layer-local filters/effects when the exact original pixels are
preserved. Frame metadata is shifted with the expanded artwork.

### Review, refinement and history

- A same-session saved image result can be rebuilt against the current mask
  after a selection-only change. Keep remains unavailable after artwork,
  document, project or AI-session identity changes.
- Generate and Expand ignore selection-only changes because neither operation
  consumes the current selection. Replace and Background still validate and
  rebuild against the changed mask before Keep.
- Background refinement restores representable shadow and reflection presets
  and displays those controls with the continuing result. Unrepresentable
  custom historical settings do not silently rewrite the current controls, and
  refining another task does not alter background-finishing state.
- If history persistence fails after a provider returns a valid image, the
  private result and retained reference files stay alive through review and
  follow-up staging. Their workspace is removed when no review, reference or
  pending request still owns it. Discard dismisses an unretained review even
  when the history store is unavailable, without replacing a damaged index.

### Variations and cancellation boundary

A variation batch continues only after the completed candidate is usable,
still belongs to the current source, was retained in local history, and passed
the route's local qualification receipt step. An invalid, stale, unretained or
unqualified candidate clears the unsent batch while preserving its warning.
This prevents a later request from being dispatched or relabeled as verified
after an unusable first-use result.

Stopping a request cancels the work Omuse still owns and clears unsent
variations. A provider may already have consumed subscription allowance before
it observes cancellation; this work does not claim provider-side cancellation.

### Keyboard focus after local finishing

Apply local finishing and Remove local finishing return focus to the editor
after a successful transaction. Ctrl+Z therefore immediately targets artwork
history. Failure keeps the existing focus. A real-button UI regression begins
with the AI prompt focused and checks both actions, exact document/pixel
restoration, and one undo transaction.

## Automated and desktop evidence

- The image-integrity runtime passed **857 application tests**: 372 library,
  240 UI and 245 integration, plus editing/Create PNG/PDF/MP4/GIF journeys and
  all 80 template variants. Four manual timing benchmarks were excluded.
- The final focus correction passed all **241 UI tests locally**. With the
  unchanged library/integration cases, the final suite contains **858 tests**;
  the complete final GitHub workflow passed, including editing/Create exports,
  motion/recovery checks and dependency inventory. This is not a claim that the
  two local phases were one full-suite invocation.
- **15 focused AI editing engine tests**, **25 installer tests** and **11 bundle
  tests** passed. Regressions cover clipped/locked dependencies, invalid mask
  renders, adjustment-aware expansion, selection-aware history, invalid batch
  termination and failed-history result/workspace lifetime.
- Final normal production build: **24 Wayland + 24 XWayland** native checks,
  at an 800×600 logical minimum viewport. System/dark and light captures were
  inspected. The installed normal launcher separately passed **24 Wayland
  checks with reduced motion**. It hashes the wrapper; the payload hash above
  was verified independently.
- All **19 installed payload files** were verified through rollback to 0.2.0
  and the reverse switch. Version output and desktop-file validation passed.
- **Cua 0.29.1 foreground XWayland** verified Contact shadow Apply → Ctrl+Z,
  Redo, and Remove local finishing → Ctrl+Z on the final production binary.
- Cua also duplicated the synthetic source with Ctrl+J, fitted it with Ctrl+0,
  selected Spot Healing with J, removed the blemish locally and restored it
  with one Ctrl+Z. A rectangle selection followed by Ctrl+K → Content-aware
  fill → Enter removed the same object, and one Ctrl+Z restored it. The unsaved
  close guard appeared; only the owned temporary duplicate was discarded.

The Cua checks submitted no new generation/edit requests. Connection discovery
in the background inspector may query the installed runtime. Only the owned
test windows were closed; unrelated user windows and artwork were preserved.

## Live image matrix

Each operation used **Codex CLI 0.158.0**, the existing subscription, a synthetic
64×80 layered document, **one variation and zero automatic retries**. No
separately billed API or reset credit was used. See the
[curated machine-readable receipts](ai-image-editing-receipts.json) for result
IDs, masks, dimensions and render hashes.

| Operation | Observed result | Integrity checks |
| --- | --- | --- |
| Generate | New coral/plum/gold abstract image; provider asset 1122×1402 | Review leaves source unchanged; editable source and brand retained |
| Replace | Selected plum blemish changed to a plum/gold detail; 1122×1402 | Every protected pixel byte-exact; staged current mask recorded |
| Remove | Selected blemish removed, warm field continued; 1122×1402 | Every protected pixel byte-exact; staged current mask recorded |
| Background | New warm geometric background; 1122×1402 | Selected subject and locked title/mark byte-exact |
| Expand | Asymmetric margins 11 left, 7 top, 17 right, 5 bottom; 92×92 kept canvas from 1254×1254 asset | Every original pixel byte-exact at its translated offset; visible new outer content |

All five passed **Keep, save/reopen, Undo and Redo**. Canvas-render hashes
establish Before = Undo and candidate = Keep = Redo = reopened. Source layers
and native project/brand data survive. These hashes compare artwork renders,
not desktop screenshots.

On the live-image runtime, Cua reopened saved Background history, observed the
stale-source Keep guard, refined the task, selected the subject and applied
local Contact/Subtle finishing. That session exposed the keyboard-focus defect
corrected and rechecked on the final runtime above. It did not submit a further
provider refinement request.

## User documentation

The [user manual](user-guide/README.md) covers first edits, saving versus export,
[local and AI object removal](user-guide/remove-objects.md), photo adjustment,
cutouts/backgrounds, social pages, text, motion, export and troubleshooting.
Command names and defaults were checked against the source. The existing Create
guide now uses Ctrl+Alt+N and current task labels, with historical evidence kept
separate from current instructions.

## Limits

- These are bounded synthetic integration/integrity checks. They do not prove
  photographic taste, semantic accuracy, large-photo removal quality or provider
  reliability across accounts. Review every result at actual pixels and output size.
- Multi-variation cancellation, provider-side interruption, failure/restart and
  longer refinement chains still need their own live workload evidence.
- Claude and Grok need separate exact-operation qualification. Direct API
  billing remains disabled.
- Controlled local removal is a bounded patch algorithm with a 4 MP input
  ceiling, not generative reconstruction. Ordinary content-aware fill supports
  up to 16,777,216 source pixels and 250,000 selected pixels per operation.
- Expansion refuses actual original-pixel changes from live spatial/noise
  adjustment reevaluation. Flattening a duplicate is an explicit user action.
- Clean supported-target installation, broader displays/physical inputs,
  accessibility, two dependency legal texts and public-binary qualification
  remain separate [release gates](public-release-readiness.md).

Private provider state, paths, raw logs and screenshots are excluded from the
public source. Curated receipts and source identities are retained for review.
