# Public release readiness

**4 October 2026: Omuse 0.8.0 is published as a GitHub prerelease and selected by
the source installer. Permanent downloadable binary assets are still pending;
0.7.0 remains preserved for rollback.**

The [0.8.0 qualification record](release-080-qualification.md) tracks photo
fidelity, gradients and curved text, reference colour matching, selective masks,
JPEG inspection and native Linux/Windows archives. The dependency replacements
close the candidate's missing legal-text findings, and exact-source validation
and download workflows completed successfully. The source release is published,
while permanent downloadable assets remain pending. None of this changes the
preserved 0.7.0 evidence or rollback payload.

The current numbered source release is **0.8.0 — Colour, curves and control**,
published as a GitHub prerelease at the [v0.8.0 release](https://github.com/Sugata-Software/Omuse/releases/tag/v0.8.0).
Its source tag points to `eb558dc59ccdf88a3d62dfc2d706a6b836da1eda`. The release
has no permanent binary assets yet. The supported source-install scope remains
**Arch/Omarchy Linux x86_64**; stable-release and broader portability gates stay
open.

The canonical public repository is
[Sugata-Software/Omuse](https://github.com/Sugata-Software/Omuse).
The published source runtime is `eb558dc59ccdf88a3d62dfc2d706a6b836da1eda`,
tree `68e406316b92d5817abfe810fbdc7840e57b5d17`; its Rust subtree is
byte-identical to the b47/d2 application source. The [validation run](https://github.com/Sugata-Software/Omuse/actions/runs/37161936414)
and [download run](https://github.com/Sugata-Software/Omuse/actions/runs/37161936378)
completed successfully. Local qualification passed **1,352 application tests**,
all **80 editable template variants**, editing/Create/media, photo/recovery and
native/package checks. The 0.7.0 runtime and its 1,206-test record below remain
historical evidence for the preserved previous release.

## Current application and installation

The normal launcher and curl installer now select **0.8.0**, source
`eb558dc59ccdf88a3d62dfc2d706a6b836da1eda`. The installed production binary
SHA-256 is `d47feb79e0cc893786f5aa8cd60dc3ef2afdb4975b2ea0cb6f19f3dcbe9abf56`.
Its isolated-XDG editing self-test passed, and the prior 0.7.0 20-file payload
remains preserved for rollback. The [release receipt](release-080-receipts.json)
records the production installation and source-publication evidence.

The reviewed 0.8.0 format and compatibility matrix is maintained in
[user-guide/README.md](user-guide/README.md). The 0.7.0 format-11 and older-reader
limitations are historical evidence for that release; Save As remains the
way to retain an older compatible copy, and application rollback does not
downgrade artwork.

A historical 0.7.0 Cua Driver 0.29.1 check opened the installed production
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
implementations remain in Git history with attribution. The 0.8.0 source
release is published, while permanent prebuilt binary assets remain pending.

## Before a downloadable binary release

| Gate | Current state | Completion evidence |
| --- | --- | --- |
| Clean CI build | Candidate `eb558dc` validation run [37161936414](https://github.com/Sugata-Software/Omuse/actions/runs/37161936414) and download run [37161936378](https://github.com/Sugata-Software/Omuse/actions/runs/37161936378) completed successfully | Permanent binary asset publication by the release owner |
| Dependency legal texts | Candidate Linux and Windows package inventories report zero notice findings, with 619 and 421 dependency entries | Retain the exact candidate inventory and notice review; see the [notice review](rust-license-findings.md) |
| Reproducible release identity | Published tag `v0.8.0` identifies exact runtime `eb558dc`; clean CI builds and reviewed archive/binary/inventory hashes are recorded in the [download manifest](releases/downloads/v0.8.0.json) and [release receipt](release-080-receipts.json) | Preserve this identity when attaching the already verified archives; independent bit-for-bit build reproducibility is not established |
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
