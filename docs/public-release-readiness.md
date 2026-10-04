# Public release readiness

**4 October 2026: Omuse 0.9.0 is published as a source prerelease with verified
Linux and unsigned experimental Windows downloads. The curl installer selects
tested source `65c95cc1`; the normal app is verified at 0.9.0 with its complete
0.8.0 rollback preserved. Broader stable-release gates remain open.**

The [0.9 release](https://github.com/Sugata-Software/Omuse/releases/tag/v0.9.0)
retains the exact tested source and reviewed notes. [Source publication 37209968347](https://github.com/Sugata-Software/Omuse/actions/runs/37209968347)
and [download publication 37210058961](https://github.com/Sugata-Software/Omuse/actions/runs/37210058961) succeeded. All four permanent assets passed
anonymous size/SHA-256 verification, including a byte-identical reviewed
[download manifest](releases/downloads/v0.9.0.json). Existing 0.8.0 assets and
source identity were unchanged.

## Previous 0.8.0 publication — historical record

The [0.8.0 qualification record](release-080-qualification.md) tracks photo
fidelity, gradients and curved text, reference colour matching, selective masks,
JPEG inspection and native Linux/Windows archives. The dependency replacements
close the candidate's missing legal-text findings, and exact-source validation
and download workflows completed successfully. Publication run
[37200321434](https://github.com/Sugata-Software/Omuse/actions/runs/37200321434)
then attached all four reviewed assets. Anonymous downloads matched their sizes
and SHA-256 hashes, and the download manifest matched the reviewed declaration
byte for byte. The source tag and published notes were unchanged. None of this
changes the preserved 0.7.0 evidence or rollback payload.

The previous numbered source release was **0.8.0 — Colour, curves and control**,
published as a GitHub prerelease at the [v0.8.0 release](https://github.com/Sugata-Software/Omuse/releases/tag/v0.8.0).
Its source tag points to `eb558dc59ccdf88a3d62dfc2d706a6b836da1eda`. See the
[download instructions](downloadable-releases.md) and
[reviewed manifest](releases/downloads/v0.8.0.json) for the permanent archives.
The supported source-install scope remains **Arch/Omarchy Linux x86_64**;
stable-release and broader portability gates stay open.

The canonical public repository is
[Sugata-Software/Omuse](https://github.com/Sugata-Software/Omuse).
The previous published source runtime was `eb558dc59ccdf88a3d62dfc2d706a6b836da1eda`,
tree `68e406316b92d5817abfe810fbdc7840e57b5d17`; its Rust subtree is
byte-identical to the b47/d2 application source. The [validation run](https://github.com/Sugata-Software/Omuse/actions/runs/37161936414)
and [download run](https://github.com/Sugata-Software/Omuse/actions/runs/37161936378)
completed successfully. Local qualification passed **1,352 application tests**,
all **80 editable template variants**, editing/Create/media, photo/recovery and
native/package checks. The 0.7.0 runtime and its 1,206-test record below remain
historical evidence for the preserved previous release.

The public Linux archive passed integrity, notice, editing/runtime, isolated
installation, same-version replacement/rollback and profile-preservation
checks. This publication check did not repeat cross-version migration. The
Windows archive was verified on Linux; its automated Windows execution remains
tied to download run 37161936378. Publication added no local Windows desktop,
mixed-DPI or live-AI qualification.

## Qualified 0.9.0 source and editing workflows

The [0.9 release notes](releases/v0.9.0.md) and
[illustrated guide](user-guide/retouch-and-controls.md) cover native-resolution
retouch, independent Blur radius, controlled removal, configurable grid/mask
controls, safe folder ungrouping and selected-letter fonts. Typed values survive
focus changes; readable font rows and a compact text footer improve direct editing.

Published runtime [`65c95cc165f9d730a9f0bcea51c51a84cf4adb7e`](https://github.com/Sugata-Software/Omuse/commit/65c95cc165f9d730a9f0bcea51c51a84cf4adb7e),
tree `abd6296cf31078e278ab7bb73dc799948443de4d`, has completed the recorded qualification.
[Validation 37207585304](https://github.com/Sugata-Software/Omuse/actions/runs/37207585304)
and [download build 37207585335](https://github.com/Sugata-Software/Omuse/actions/runs/37207585335)
completed successfully on their first attempts. Linux passed **1,405 application
tests** and Windows **1,387**, each with zero failures and four ignored. Their
complete journeys and notice gates passed. Both package smoke receipts passed
nine checks, and the unmodified generated [download manifest](releases/downloads/v0.9.0.json)
passed canonical, archive and receipt verification. Their separate source and asset publication steps have also completed, with anonymous verification.

The final Linux binary passed **24 native checks** at 800 × 600 logical /
1200 × 900 device pixels through XWayland. A visual review confirmed text
Apply/Cancel stay inside the panel. The exact archive passed **four**
0.8 → 0.9 → rollback → forward journeys with both complete **22-file**
generations preserved and **35 identical exports across nine projects**.
Curved format-14 text and cached ContextualV1 removal results were included.

The unchanged photo library passed **16 cases and 128 retouch operations**:
source integrity at full resolution, retouch on copies no wider than 1,024
pixels. Controlled removal separately preserved protected pixels, one-step
Undo/Redo and exact saved recipe re-evaluation. Some repaired texture mismatch
remains; automatic Spot Heal can copy unsuitable nearby detail. These checks
do not establish universal photographic quality or a speed improvement.

The [0.9 qualification record](release-090-qualification.md) and
[receipt ledger](release-090-receipts.json) distinguish final package/native
results, retained engine evidence and broader limits. Earlier 39243caa passed
its suites and native controls but failed real-photo removal review; that
quality finding blocked it and led to the current context-guided algorithm.
Its historical checks are not relabeled as final-source results.

## Current installation and compatibility

The normal development-host launcher is now verified at **0.9.0**, exact source
`65c95cc165f9d730a9f0bcea51c51a84cf4adb7e`, with binary SHA-256
`da39ef88d0c6f08074051784974668841e62a2ac9c56c527b73c9ba4685e4ec6`.
All **22 files** from its prior 0.8.0 generation and profile/settings bytes were
preserved. The installed launcher passed **24 native checks** in an isolated
profile. The public curl installer and latest declaration select this same tested
source; the numbered prerelease and all four permanent download assets are
published and verified. Installation and publication retain separate receipts.

New controlled-removal recipes use **ContextualV1**; absent algorithm fields
retain **Legacy** behavior. **0.8 can display cached new results, but editing or
resaving there can lose the algorithm choice.** Keep the 0.9 original and use a
separate copy rasterized in 0.9 for backward editing. The cross-version export
check did not exercise same-project editing/resaving or native settings
migration; preserving bytes is not proof an older version understands them.
[Compatibility steps](user-guide/retouch-and-controls.md#keep-new-removal-recipes-safe-when-trying-an-older-version).

Published 0.8.0 receipts and earlier 0.7.0/0.6.0 measurements remain historical.
No new live AI request was sent for this editing qualification. Earlier
[Claude](ai-claude-qualification.md), [Codex image editing](ai-image-editing-qualification.md)
and [Cua AI](cua-ai-qualification.md) results retain their own source identities.
Clean-machine, wider hardware/accessibility and live Windows AI work remain open.

## What is established

The [keyboard and command search pass](keyboard-qualification.md) passed
777 automated tests. It exposes 171 searchable commands and 101 defaults,
with custom bindings, input isolation and Linux punctuation-event coverage.
Production Wayland and XWayland each passed 24 native checks, including
command search execution and Undo; both keyboard panels were visually
inspected at the minimum viewport. The installed Preview passed the same
24 checks with reduced motion; its prior complete generation and runtime
assets remain available. The later [Cua and AI GUI pass](cua-ai-qualification.md)
adds foreground XWayland input evidence; broader physical-device acceptance
remains open.

The [photo integrity hardening pass](photo-integrity-hardening.md) passed
757 Rust tests, including 23 new regressions for clipping/masks, transform
gestures, collection admission and recovery format transitions. Production
Wayland/XWayland and the installed preview each passed 22 native checks, and
complete rollback passed in both directions. The exact source is `b2f6e85`;
its [full remote Rust run](https://github.com/Sugata-Software/Omuse/actions/runs/36454128430) passed.

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

The [one-command installer](install.md) builds a tested Omuse revision locally on
Arch/Omarchy, with dependency setup, verified optional assets and complete
installation rollback. The active tree contains Linux Omuse; old platform
implementations remain in Git history with attribution. The 0.8.0 source
release and its reviewed prebuilt archives are published.

## Downloadable prerelease and remaining release gates

| Gate | Current state | Completion evidence |
| --- | --- | --- |
| Current 0.9 publication | Exact-source validation 37207585304 and download build 37207585335 passed; source publisher 37209968347 and download publisher 37210058961 succeeded. Four permanent assets matched anonymous size/hash checks. | Completed for this unsigned prerelease; broad portability and stable-release gates remain open |
| Clean CI build and publication | Runtime `eb558dc` validation [37161936414](https://github.com/Sugata-Software/Omuse/actions/runs/37161936414), download build [37161936378](https://github.com/Sugata-Software/Omuse/actions/runs/37161936378) and publication [37200321434](https://github.com/Sugata-Software/Omuse/actions/runs/37200321434) succeeded; all four permanent assets passed anonymous size/hash verification | Completed for 0.8.0; every later candidate needs its own exact-source evidence and reviewed publication |
| Dependency legal texts | Candidate Linux and Windows package inventories report zero notice findings, with 619 and 421 dependency entries | Retain the exact candidate inventory and notice review; see the [notice review](rust-license-findings.md) |
| Recorded release identity | Published tag `v0.8.0` identifies exact runtime `eb558dc`; clean CI builds and reviewed archive/binary/inventory hashes are recorded in the [download manifest](releases/downloads/v0.8.0.json) and [release receipt](release-080-receipts.json); publication left tag and notes unchanged | Preserve the immutable release identity; independent bit-for-bit build reproducibility is not established |
| Clean target installation | Passed only on the development host | Install, launch, upgrade and rollback on a clean supported Arch/Omarchy system |
| Physical desktop acceptance | Native in-process journeys pass; bounded Cua foreground XWayland Camera Raw and Subject Refine Apply/Undo/Redo checks passed | Broader foreground keyboard/pointer, clipboard, file dialogs and supported display/DPI configurations |
| Feature claims | Local editor and selected Codex journeys have evidence | Each advertised AI operation has its own acceptance receipt; unqualified routes remain unavailable or explicitly experimental |
| User edge cases | Broad automated coverage; manual gaps remain | Missing fonts, long copy, invalid CSV rows, large libraries, damaged assets, offline use, cancellation and restart |
| Public support and security | Contribution and bug-report entry points prepared | Establish a private vulnerability-reporting channel, owner-approved security policy, maintenance scope and triage ownership |
| Supply-chain review | Locked dependencies and notices inventoried | Dependency vulnerability review, focused review of untrusted imports/provider boundaries, checksums and release attestations/signing decision |

The historical 0.7.0 dependency graph still has the two missing legal texts;
its existing receipts are not retroactively changed. The 0.8.0 inventory applies
only to runtime eb558dc and does not alter the historical 0.7.0 record. Candidate
notice inventory success applies only to the exact graph inspected, and does not by itself
qualify a package, establish licence compatibility or certify redistribution.

## Before calling it stable or portable

- Test multi-monitor and mixed-DPI behaviour on real hardware. Tablet support
  is deferred and is not a gate for the initial release; pressure/tilt are not qualified.
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

The earlier routing candidate `f972adb` introduced per-task provider selection and
optional image/layout/caption sequences. Its complete public workflow passed
945 application tests and the editing, export, recovery and installer checks.
See the [routing validation](ai-routing-qualification.md). Claude remains
limited to text/layout without image input; Grok remains unqualified. Optional
API billing is disabled. Automated results do not establish general provider
quality or clean-host support.
No account credentials, provider sessions or user artwork belong in this repo.

## Evidence curation

The [0.3.0 evidence manifest](editing-workflows-receipts.json) ties the tested
source to the production executable, complete payloads, final CI and native
checks. Keep only the exact candidate's final receipts and synthetic fixtures.
The local working
evidence directory contains superseded failures and personal filesystem paths;
it must not be uploaded wholesale. The detailed qualification record identifies
the corrected native and template receipts.

The promo is an Omuse feature trailer. Preserve its photo and music credits
when publishing it, and do not infer release readiness from the trailer.
