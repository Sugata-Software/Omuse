# Public release readiness

**2 October 2026: Omuse 0.7.0 is available through the source installer. Downloadable binaries remain unqualified.**

The current numbered source release is **0.7.0 — Photos and vectors, one studio**.
Its [release notes](releases/v0.7.0.md), [qualification](release-070-qualification.md)
and [receipt](release-070-receipts.json) identify the exact tested runtime,
installation, changes and limits. GitHub marks the release as a pre-release
while stable-release gates remain open; there is one normal application named
Omuse. The supported source-install scope is **Arch/Omarchy Linux x86_64**.

The canonical public repository is
[Sugata-Software/Omuse](https://github.com/Sugata-Software/Omuse).
The tested runtime is `5b3daefbb5258afff4a74a2ff3db5247b074972d`, tree
`f9b4326d6ef37ea3edc56002566e1df399e47e69`. Its complete
[GitHub validation](https://github.com/Sugata-Software/Omuse/actions/runs/36910650216)
passed. Local qualification passed **1,206 application tests**, all **80
editable template variants**, editing/Create/media, four synthetic tracing
fixtures, vector/photo acceptance and interrupted-save recovery.

## Current application and installation

The normal launcher and curl installer select **0.7.0**. The production binary
passed 13 native journeys: minimum-size Wayland and XWayland, the installed
normal launcher, and ten [gallery views](releases/v0.7.0-gallery.md). The light
and dark captures were inspected. The installed payload contains 20 files and
a clean public-source receipt. Complete rollback to 0.6.0 and back passed with
every old and new payload hash unchanged. See the
[installation record](main-install-qualification.md).

The previous-release synthetic project rendered identically with both readers
and remained unchanged. **Vector scenes use format 11; ordinary canvases stay
format 10.** Omuse 0.6.0 cannot read format-11 artwork or the new Target Colour
Uniformity recipe. Use Save As to retain an older compatible copy; application
rollback does not downgrade artwork.

A bounded Cua Driver 0.29.1 check independently opened the installed production
app and a saved test project on XWayland, then visually verified foreground
Ctrl+K opening command search. Background key delivery had no visible effect.
This does not qualify all physical devices or accessibility routes.

No new live AI request was sent for this release. Earlier
[Claude Design](ai-claude-qualification.md),
[Codex image editing](ai-image-editing-qualification.md) and
[Cua AI checks](cua-ai-qualification.md) retain their own source identities.
The [manual](user-guide/README.md) documents photo/vector workflows and limits.
Broader hardware, clean-machine installation, independent interchange/image
quality and the binary/stable-release gates below remain open.

The preceding [0.6.0 qualification](release-060-qualification.md) and the records
below are historical evidence for their own candidates.

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
implementations remain in Git history with attribution. This improves access
to Omuse without declaring a prebuilt binary release qualified.

## Before a downloadable binary release

| Gate | Current state | Completion evidence |
| --- | --- | --- |
| Clean CI build | 0.7.0 runtime `5b3daef`: the [exact-source Rust/installer run](https://github.com/Sugata-Software/Omuse/actions/runs/36910650216) passed. Later runtime changes require their own run | Green Rust workflow for the exact candidate commit, retained artifacts and notice inventory |
| Dependency legal texts | Two unresolved entries in the locked graph; upstream texts recovered for `seahash` and `simd_helpers` | Resolve `hexf-parse` and `mac` by verifiable upstream terms or tested dependency changes; see the [notice review](rust-license-findings.md) |
| Reproducible release identity | The hardening pass records tested source hashes, the production executable, native runs, installation and rollback; a clean public build/archive remains pending | One clean release commit/tag, fresh build, source-to-binary-to-archive hash ledger and reproducible packaging instructions |
| Clean target installation | Passed only on the development host | Install, launch, upgrade and rollback on a clean supported Arch/Omarchy system |
| Physical desktop acceptance | Native in-process journeys pass; bounded Cua foreground XWayland input works. The native Omarchy plugin has a package/compiler compatibility mismatch | Real foreground keyboard/pointer, clipboard, file dialogs and at least the supported display/DPI configurations |
| Feature claims | Local editor and selected Codex journeys have evidence | Each advertised AI operation has its own acceptance receipt; unqualified routes remain unavailable or explicitly experimental |
| User edge cases | Broad automated coverage; manual gaps remain | Missing fonts, long copy, invalid CSV rows, large libraries, damaged assets, offline use, cancellation and restart |
| Public support and security | Contribution and bug-report entry points prepared | Establish a private vulnerability-reporting channel, owner-approved security policy, maintenance scope and triage ownership |
| Supply-chain review | Locked dependencies and notices inventoried | Dependency vulnerability review, focused review of untrusted imports/provider boundaries, checksums and release attestations/signing decision |

The two missing legal texts are a **binary-distribution gate**. Publishing this
source repository does not certify the resulting third-party dependency bundle
for redistribution.

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

The unreleased routing candidate `f972adb` adds per-task provider selection and
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
