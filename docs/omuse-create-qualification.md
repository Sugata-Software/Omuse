# Omuse Create + AI qualification

**28 September 2026 — local preview qualification. Public release remains
gated.** The exact candidate passed the automated, artifact, native-window and
bounded live-provider checks recorded below. The separate preview installation
also passed checks outside the source checkout. These results do not replace
the physical, portability and provider-operation gates at the end of this record. See the
[Create guide](omuse-create-guide.md) for use and the
[preview release notes](omuse-create-release-notes.md) for scope.

## Candidate identity

- Branch: `feat/omuse-create-ai`.
- Application source: `b178ad273ae9627836b753d584925f3bf7e1a003`.
- Executable SHA-256:
  `eeec22afb6c4eb5cf7f6840c4cece6b50c1c9827b21c3d929ba994cab1b73dca`.
- Qualification tools and clean bundle source revision:
  `ee55127d3aa18ddb1344a9059ca126d2ffebe17b`. Only the recovery fixture and
  native-window harness changed after the application revision; runtime Rust
  source, dependency manifests and vendored runtime code are identical.
- Bundle SHA-256:
  `02b641918e1c04fc2a27b281ee31a9118c9da62ece47e5a8d1d107c1769f944a`.
- Evidence set: `create-ai-20260927`, retained in the maintainer's local
  release-evidence directory. It is not included in the public source import.
- The installed earlier Omuse and `rewrite/rust-gpui` baseline are preserved.

## Automated and artifact gates

| Area | Result and evidence |
| --- | --- |
| Regression and self-test | **Pass.** `release-gate-b178ad2-resumed.log` and `automated-b178ad2.json` record 636 passed, 0 failed and 3 ignored benchmark tests across 20 groups, with exit code 0. The candidate self-test passed. All three ignored benchmarks were then run explicitly and passed in `benchmarks-b178ad2.log`. |
| Six-page Create journey | **Pass.** `gate-work-b178ad2/omuse-tests.UZcEpfOG/create-journey` contains the editable `.omuse` project, six PNGs, matching multipage PDF, editable story variant, MP4 and GIF. All six decoded PNG pixel buffers match the reviewed prior output; the PDF file is byte-identical to its reviewed predecessor. |
| Template catalog | **Pass.** The exact candidate rendered 80 variants across 20 templates with native text and pixels preserved through save/reopen. `catalog-visual-continuity-b178ad2.json` records decoded-pixel matches for the 76 unchanged variants. The four changed Video title variants were reviewed at native resolution and all passed containment, clipping and contrast checks. |
| Media | **Pass.** `media-b178ad2/results.json` covers two editable 640×480 pages over 3.2 s at 12 fps. Soft- and burned-subtitle MP4 containers are 3.200 s, with 3.167 s video streams and 3.200 s audio; the decoded 48 kHz tone measured 439.995 Hz. The soft export retains a selectable subtitle track and the burned export visibly renders captions without one. Trim produced 1.250 s, and split produced two 1.583 s clips, all with audio. |
| Collection recovery | **Pass.** `create-recovery-qualified-b178ad2/results.json` ran six revisions. Sustained save/reopen completed in 8.047 s with destination and recovery SHA-256 values equal. A staged-save SIGKILL at revision 3 completed in 5.022 s and preserved complete old/new package states. This coordinated interruption is not a power-loss simulation. |
| Advanced recovery | **Pass.** `advanced-recovery-b178ad2/results.json` passed the recovery worker, a 12-revision 768×512 sustained session with three advanced layers in 14.950 s, and a SIGKILL save interruption in 6.633 s that retained visible revision 3 plus recovery revision 4. This is filesystem/process evidence, not power-loss qualification. |
| Bounded benchmarks | **Pass on this host.** For 80 measured 24×24 edits on a 2048² surface, regional conversion had a 150.401 µs median versus 5.704 ms for legacy full-clone/swap and 6.078 ms for full-tile creation. A 2048² small-patch raster case measured 0.022 ms regionally versus 147.916 ms full-frame. Snapshot median was 0.000427 ms; 20 metadata edits took 0.085999 ms and retained 27,530 bytes. These CPU/allocation figures exclude GPU work and are not cross-machine guarantees. |
| Preview installation | **Pass.** `installed-preview.json` records the verified archive, installed binary and unchanged original binary. The isolated `omuse-preview` launcher passed help, self-test and library resolution from `/tmp`. `installed-native/verified-native.json` passed native editing, save/reopen, themes and 16-bit preservation using the installed executable. `installed-normal-launch.json` and `installed-desktop-launch.json` verify normal and desktop-entry launch with the two copied sample collections. Settings, cache, history and recovery use separate preview XDG paths; `HOME` remains unchanged. |

## Completed native and provider checks

| Area | Result and evidence |
| --- | --- |
| Wayland panels | **Pass.** `native-motion-system-b178ad2`, `native-assistant-light-b178ad2`, `native-create-dark-b178ad2` and `native-export-light-b178ad2` exercise their named panels plus the native editing journey. Motion and Assistant ran at an actual 800×600 GPUI viewport; the corrected Motion layout keeps **Play project** fully visible. |
| XWayland minimum window | **Pass.** The corrected external harness in `native-xwayland-qualified-b178ad2` records an actual 800×600 GPUI viewport. The compositor window is 1125×844 at monitor scale 1.6; its X drawable is 1800×1350 at GPUI scale 2.25. |
| Splash and reduced motion | **Pass.** `native-startup-qualified-b178ad2` captures differing animated splash frames; `native-xwayland-qualified-b178ad2` captures byte-identical reduced-motion frames after geometry settles. |
| Generate review/history | **Pass for bounded restoration.** `native-ai-b178ad2` restores the retained real Codex generation through the actual history/review path, preserving its original provider/runtime/result identity and 1122×1402 dimensions. It rejects applying that stale result to a new document. The original Generate request was made on the prior candidate; it was not charged again for this check. |
| Live background edit | **Pass.** The same native AI record proves a new subscription-backed background result was reviewed and kept. The protected subject was byte-exact; native source and brand survived save/reopen; standard Undo restored the source. This is one bounded fixture, not a claim about every photograph or edit intent. |
| Live editable assistant | **Pass.** A new Codex assistant request produced a six-page native carousel with captions and alt text. Native text and brand survived Keep, save/reopen, Undo and Redo. All six rendered pages passed visual review. Its 2-second fade plan was applied and saved, but this assistant journey did not separately render that animation. |
| CUA discovery/capture | **Partial.** `cua-native-b178ad2` verifies exact-window discovery and capture through the installed Omarchy-specific driver in a disposable nested Hyprland session. Pointer and keyboard input were refused with `foreground_pointer_resources` / `foreground_keyboard_resources`; neither counts as a pass. The child session was removed and the main desktop preserved. AT-SPI exact-window coverage remains unqualified. |

The native regression journeys dispatch GPUI input in process. They provide
application evidence but do not replace physical keyboard, pointer or tablet
acceptance. No physical-input pass is claimed from the unsuccessful CUA attempt.

## Installed preview

Launch **Omuse Preview** from the app launcher or run `omuse-preview`.
The executable is at
`~/.local/opt/omuse-preview/opt/omuse/omuse`. Its original runtime libraries and
subject model were copied with matching hashes; the installed ONNX Runtime and
LibRaw libraries both load successfully. This is installation evidence, not a
new RAW-file or subject-inference quality test.

Editable sample copies and an export pack are in
`~/Pictures/Omuse Preview/`. Both normal sample windows were opened through the
installed preview and left available. The existing `omuse` command and
`~/.local/opt/omuse/omuse` remain unchanged; its executable SHA-256 is
`10e8ca5b55182fd03c8f09b2bd2c41d6a0ea7041026ee997ea4c606de0ad80cf`.
Returning to the previous app means launching **Omuse**. Keep new Create
packages intact, since older builds may not understand their metadata.

## Measurements and their limits

The tested native panel runs peaked at 283,756–357,488 KiB resident memory;
the live AI journey peaked at 376,336 KiB. These are sampled Omuse-process
resident/high-water values and exclude provider children and GPU allocations.
On the live AI launch, first splash paint was recorded at 67 ms, editor-ready at
315 ms and dismissal at 1,160 ms. These are single-host observations under the
current workload, not cross-machine performance guarantees.

## Provider qualification boundary

The verified live route is `codexSubscription` through `codex-cli 0.151.0`.
Generation review, background editing and a native assistant carousel have the
bounded evidence above. Real-provider selection replacement, asymmetric
expansion, removal, reference continuity, multiple variations/cancellation,
layout critique, assistant-led resizing and standalone caption drafting still
need individual acceptance journeys. Source paths and automated invariants are
not live capability or visual-quality proof.

Claude's official runtime is signed out. Grok's official runtime identity and
isolated operation are unverified. Optional direct OpenAI API access remains
disabled pending the user's key/billing-route choice. There is no automatic
fallback from an unavailable subscription to a separately billed API.

## Remaining public-release gates

- Physical keyboard/pointer acceptance, tablet input and multiple real display
  and DPI configurations; the current CUA input refusal remains visible.
- Clean-container build/package and CI results. Docker access was unavailable;
  no clean-container or remote CI pass is claimed.
- Resolution of the four dependency licence-text findings in
  [the licence record](rust-license-findings.md) before public binary release.
- The remaining live-provider journeys above, manual long-copy/font-fallback,
  frame/cutout quality, a representative CSV batch and large-library checks.
- Provider sign-in/capability qualification for any additional connection
  advertised as supported.

The preview bundle targets this Arch/Omarchy x86_64 host and its glibc 2.44
environment. It is not qualified as a portable binary for all Linux
distributions. Calendars, scheduling, publishing and analytics remain excluded.
