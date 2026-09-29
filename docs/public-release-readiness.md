# Public release readiness

**29 September 2026: Omuse source installation is available; downloadable binaries are not yet qualified.**

The first numbered source release is **0.1.0**. Its
[release notes](releases/v0.1.0.md) identify the tested runtime, installation,
changes and limitations; the [release policy](releases/README.md) keeps future
versions and evidence consistent. It is marked as a GitHub pre-release while
stable-release gates remain open. This does not add a separate Preview app.

The canonical public repository is
[Sugata-Software/Omuse](https://github.com/Sugata-Software/Omuse). The first
source release is scoped to **Arch/Omarchy, Linux x86_64**. Broader Linux support
requires its own packaging and test evidence.

The application source is published. Its first complete source commit is
`d8c926e`, with Git tree `580d583266db090d18ca6159813a3ea0b343237f`, exactly
matching the locally qualified source snapshot. The project-guide workflow
passed. The first [Rust validation run](https://github.com/Sugata-Software/Omuse/actions/runs/36426033617)
passed on 28 September, including the complete headless suite, editing/Create
journeys, recovery checks, offline installer and dependency inventory.
The Linux cleanup and installer candidate `971c419` also passed its complete
[Rust validation run](https://github.com/Sugata-Software/Omuse/actions/runs/36439517375),
including the separate installer/reference job and the full Rust job.
GitHub App access and source publication are no longer blockers.

## Current application and installation

The tested application source is `9c99e50`. Its [full GitHub Rust/installer
workflow](https://github.com/Sugata-Software/Omuse/actions/runs/36492555743) passed. Local qualification records
808 tests, editing/Create exports, all 80 size variants of 20 templates,
production Wayland/XWayland checks, and complete installed rollback. The
[OmaPhoto comparison](omaphoto-comparison.md) explains the resulting editing
improvements and remaining gaps. The public installer pins this runtime.
The bounded live Codex assistant receipt remains tied to the earlier `1d5ccaa`
candidate; this pass did not repeat provider requests.

The laptop's main **Omuse** launcher now selects that exact tested executable
and complete runtime assets. The prior main installation is retained as a
rollback generation. See [main installation qualification](main-install-qualification.md).
Historical records below retain the former **Omuse Preview** name for the
isolated installation used during testing; it is not a separate public product.

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
| Clean CI build | Installed runtime `9c99e50`: the [full Rust/installer run](https://github.com/Sugata-Software/Omuse/actions/runs/36492555743) passed; guide/reference checks are current. Later runtime changes require their own run | Green Rust workflow for the exact candidate commit, retained artifacts and notice inventory |
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

The promo is an Omuse feature trailer. Preserve its photo and music credits
when publishing it, and do not infer release readiness from the trailer.
