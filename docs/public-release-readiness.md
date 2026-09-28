# Public release readiness

**29 September 2026: source preview; public binary release is not yet qualified.**

The canonical public repository is
[Sugata-Software/Omuse](https://github.com/Sugata-Software/Omuse). The first
release should be scoped to **Arch/Omarchy, Linux x86_64**. Broader Linux support
requires its own packaging and test evidence.

The source preview is published. Its first complete source commit is
`d8c926e`, with Git tree `580d583266db090d18ca6159813a3ea0b343237f`, exactly
matching the locally qualified source snapshot. The project-guide workflow
passed. The first [Rust validation run](https://github.com/Sugata-Software/Omuse/actions/runs/36426033617)
passed on 28 September, including the complete headless suite, editing/Create
journeys, recovery checks, offline installer and dependency inventory.
The Linux cleanup and installer candidate `971c419` also passed its complete
[Rust validation run](https://github.com/Sugata-Software/Omuse/actions/runs/36439517375),
including the separate installer/reference job and the full Rust job.
GitHub App access and source publication are no longer blockers.

## What is established

The [photo integrity hardening pass](photo-integrity-hardening.md) passed
757 Rust tests, including 23 new regressions for clipping/masks, transform
gestures, collection admission and recovery format transitions. Production
Wayland/XWayland and the installed preview each passed 22 native checks, and
complete rollback passed in both directions. The exact source is `b2f6e85`;
its full remote Rust run remains pending.

The [desktop ownership qualification](desktop-history-qualification.md)
records its 734 passing Rust tests, 80 editable template variants,
production Wayland/XWayland journeys and matching preview installation with
rollback. Create releases its redundant page raster owners between edits;
complete save/export/recovery snapshots remain immutable. Editing during a
same-path background save no longer leaves fully cached snapshots unusable.
All eighteen paired 12/24/48 MP cases passed with exact undo/redo/recovery pixels.
At 48 MP, paint-core median fell from 85.69 to 0.32 ms and the headless
frame/refresh median from 257.88 to 165.38 ms. Peak memory was essentially
unchanged. Recovery was idle before each measured stroke; in-flight snapshots
and full end-of-stroke refresh remain limitations.

The earlier [region-history qualification](large-photo-history-qualification.md)
at `a69a5f6` records a different, isolated 100-stroke workload. At 48 MP its
routine-stroke median fell from 90.43 to 0.42 ms and retained undo rose from
one to 100 steps. Keep that evidence separate from the later desktop comparison.
Neither fixture measures physical input latency or establishes a general
performance/portability claim.
The earlier [real-photo qualification](photo-release-qualification.md) records
the import/colour corrections and 16 independent photo cases at `ac8874d`.
That external corpus was not repeated for the history/ownership changes.
The [photo editing hardening record](photo-editing-hardening.md) describes the
earlier `f066238` baseline. The earlier `b178ad2`
[Create qualification record](omuse-create-qualification.md) remains historical
evidence for:

- 636 passing automated tests, with three benchmarks run separately;
- 80 template variants, editable Create collections and raster/PDF/media export;
- save/reopen, undo and staged-save interruption checks;
- an isolated preview installation and native Wayland/XWayland journeys;
- bounded Codex subscription background editing and an editable assistant
  carousel, with source-preserving review and Undo.

The initial source snapshot also passed remote Ubuntu CI. Neither result establishes
physical input support, all advertised provider operations or Linux portability.

The [one-command installer](install.md) now builds the preview locally on
Arch/Omarchy, with dependency setup, verified optional assets and complete
installation rollback. The active tree contains Linux Omuse; old platform
implementations remain in Git history with attribution. This improves access
to the source preview without declaring a prebuilt binary release qualified.

## Before a downloadable public preview

| Gate | Current state | Completion evidence |
| --- | --- | --- |
| Clean CI build | Photo-integrity candidate `b2f6e85` passed guide and installer/reference checks; its full Rust run is pending. The preceding `971c419` passed the full suite | Green Rust workflow for the exact candidate commit, retained artifacts and notice inventory |
| Dependency legal texts | Two unresolved entries in the locked graph; upstream texts recovered for `seahash` and `simd_helpers` | Resolve `hexf-parse` and `mac` by verifiable upstream terms or tested dependency changes; see the [notice review](rust-license-findings.md) |
| Reproducible release identity | The hardening pass records tested source hashes, the production executable, native runs, installation and rollback; a clean public build/archive remains pending | One clean release commit/tag, fresh build, source-to-binary-to-archive hash ledger and reproducible packaging instructions |
| Clean target installation | Passed only on the development host | Install, launch, upgrade and rollback on a clean supported Arch/Omarchy system |
| Physical desktop acceptance | Native in-process journeys pass; the installed Omarchy CUA plugin is inactive in this desktop session | Real foreground keyboard/pointer, clipboard, file dialogs and at least the supported display/DPI configurations |
| Feature claims | Local editor and selected Codex journeys have evidence | Each advertised AI operation has its own acceptance receipt; unqualified routes remain unavailable or explicitly experimental |
| User edge cases | Broad automated coverage; manual gaps remain | Missing fonts, long copy, invalid CSV rows, large libraries, damaged assets, offline use, cancellation and restart |
| Public support and security | Contribution and bug-report entry points prepared | Establish a private vulnerability-reporting channel, owner-approved security policy, maintenance scope and triage ownership |
| Supply-chain review | Locked dependencies and notices inventoried | Dependency vulnerability review, focused review of untrusted imports/provider boundaries, checksums and release attestations/signing decision |

The two missing legal texts are a **binary-distribution gate**. Publishing this
source repository does not certify the resulting third-party dependency bundle
for redistribution.

## Before calling it stable or portable

- Test multi-monitor and mixed-DPI behaviour on real hardware. Tablet support
  is deferred and is not a gate for this preview; pressure/tilt are not qualified.
- Test RAW development and local subject inference on representative inputs;
  loading an optional shared library is not image-quality qualification.
- Run multi-machine long sessions and large-document/library workloads, with
  recovery after interruption and recorded resource use.
- Validate `.comp` interchange using independently reviewed macOS fixtures.
- Add an Omuse-native distribution package and AppStream metadata. The obsolete
  platform packages have been removed from the active source tree.
- Test each supported distribution. The existing preview bundle was built on
  Arch with glibc 2.44 and is not a portable Linux binary.

## Provider boundaries

The bounded live route is a signed-in official Codex runtime. Claude is signed
out in the current evidence; Grok's official runtime and isolated operation are
unverified. Optional API billing remains disabled. These routes must not be
advertised as generally supported solely because an adapter exists in source.
No account credentials, provider sessions or user artwork belong in this repo.

## Evidence curation

Publish a small final-evidence manifest tied to the release commit. Keep only
the exact candidate's final receipts and synthetic fixtures. The local working
evidence directory contains superseded failures and personal filesystem paths;
it must not be uploaded wholesale. The detailed qualification record identifies
the corrected native and template receipts.

The promo is a preview feature trailer. Preserve its photo and music credits
when publishing it, and do not infer release readiness from the trailer.
