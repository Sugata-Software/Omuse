# Omuse 0.11.0 release qualification

**Published and verified — 11 October 2026.** Exact-source Linux and Windows
validation, reviewed archives, native Linux checks, rollback and the normal
Omarchy installation have passed. The
[0.11.0 release](https://github.com/Sugata-Software/Omuse/releases/tag/v0.11.0),
all four permanent assets and the public installer were verified independently.
Windows remains unsigned experimental. Earlier development results retain their
own identity below.

## Exact source

Public runtime [`e72bccdcef066cb6ab60a99faa9d7609de86c2e4`](https://github.com/Sugata-Software/Omuse/commit/e72bccdcef066cb6ab60a99faa9d7609de86c2e4)
is the verified target of `v0.11.0`, with tree `c6c43c3f92ca48b465bbbd59e0ca94a639532036`.
Its parent is public main `6520c36f90609d7b546f8a6a4d3f72b4cd4cab32`;
private development history was not published. All 698 runtime/build paths
match the prepared source by Git blob. Cargo package and lockfile identify 0.11.0.

[Validation 38130013300](https://github.com/Sugata-Software/Omuse/actions/runs/38130013300)
and [package build 38130013292](https://github.com/Sugata-Software/Omuse/actions/runs/38130013292)
both succeeded on their first attempts for this exact source.
Windows now executes retained-layout acceptance alongside editing, Create,
catalogue and motion; Linux retains its layout receipt and renders as artifacts.

## Release checks

| Check | Result and scope |
| --- | --- |
| Exact-source Linux CI | Passed: 1,599 reported application passes across 70 executable targets plus a zero-test doctest result, zero failures and five ignored. This comprises 563 library, 454 GPUI and 582 integration results; external-fixture early returns are distinguished below. |
| Exact-source Windows CI | Passed: 1,581 reported application passes across 70 executable targets plus a zero-test doctest result, zero failures and five ignored. This comprises 548 library, 452 GPUI and 581 integration results, with the same external-fixture qualification below. |
| Linux and Windows editing and acceptance journeys | Passed editing, Create/motion, recovery, all 80 template variants and retained-layout acceptance for the versioned source. |
| Linux and Windows packaged archives | Both downloaded archives passed integrity review and native runner smoke. The Linux archive also passed repeated host smoke on Omarchy. |
| Native Linux final archive | Passed 24 scripted checks on each of Wayland and XWayland, plus Cua XWayland inspection of contextual actions, layout/Undo and local AI review. |
| Published 0.10 → 0.11 → rollback → forward | Passed with the actual binaries: four installed editing self-tests, 17 projects, 62 exact pixel comparisons and six safe old-reader refusals of format 15. Complete 35-file generations and profile bytes remained unchanged. |
| Normal Omuse installation | Passed: launcher, version, exact source and binary identity verified at 0.11.0. All 35 installed files match the reviewed host-smoke installation; the installed binary also passed all 24 native Wayland checks in an isolated profile. Prior complete generations, settings and data are preserved. |
| Public source, notes, installer and download readback | Passed: [source publisher 38132530557](https://github.com/Sugata-Software/Omuse/actions/runs/38132530557) and [download publisher 38132594953](https://github.com/Sugata-Software/Omuse/actions/runs/38132594953) succeeded. Anonymous downloads of all four assets matched reviewed sizes and SHA-256 hashes; tag, notes, installer and latest declaration matched exactly. The prior 0.10 release and assets are unchanged. |

Each platform's dependency-helper suite separately passed 20 Rust cases; these
are not included in the application totals. The workflow does not configure
`OMUSE_RAW_FIXTURE`, `OMUSE_PSD_FIXTURE` or `OMUSE_PSD_REJECT_FIXTURE`: three
external RAW and two external PSD cases report passes after returning early.
These five cases are distinct from the five ignored manual benchmarks.
Checked-in and generated fixtures do run; the aggregate is not evidence of
fresh external RAW/PSD execution. Final Linux and Windows package inventories contain
**621 and 423 dependencies**, respectively, with **zero notice findings**.
The installer, bundle, source-publication and download-publication Python
harnesses passed a separate **95 tests** against the public candidate tree.

Native scripted checks began at 800 × 600 logical pixels on each Linux backend;
the later AI inspection widened the window. Cua interaction verified proportional
AI previews, original reset, Changes, Refine and exact draft preservation through
Collapse/Open. It also checked frame bounds and the dock, Ungroup immediately
followed by Ctrl+Z, layout settings and Escape. The AI fixture is local and
synthetic; no provider request was sent.

The cross-version pass used complete isolated installations. A retained Fill
layout and its rules-released format-14 copy rendered identically, while the
older application safely refused retained format 15. These checks preserve the
separate distinction between application rollback and project compatibility.
Normal user installation now selects the exact reviewed 0.11.0 executable.
All 35 files in the previous 0.10 generation and all 22 files in the older
retained generation remain unchanged. Four configuration files and 620 data
files are byte-identical to their pre-install snapshots. The installed executable
passed all 24 native Wayland checks in an isolated profile, including usable
startup and the full editor window. Normal terminal command resolution and
desktop launch configuration both select that verified 0.11.0 installation.
Public publication has its own verification, separate from this host installation.

## Published asset identities

| Asset | SHA-256 | Size |
| --- | --- | --- |
| [Linux x86_64](https://github.com/Sugata-Software/Omuse/releases/download/v0.11.0/omuse-0.11.0-linux-x86_64.tar.gz) | `733be21dde1342541f47109a43c676eef69c8471c0da8ca1eb57f498a995d1c8` | 37,858,953 bytes |
| [Windows x86_64](https://github.com/Sugata-Software/Omuse/releases/download/v0.11.0/omuse-0.11.0-windows-x86_64.zip) | `8a6def93a54d1b9052d686d5a9bbaae2107430f9a83718b1fffd7d825a7bdaac` | 26,146,961 bytes |
| [Checksums](https://github.com/Sugata-Software/Omuse/releases/download/v0.11.0/omuse-0.11.0-SHA256SUMS) | `17216cb534dc9cf7a6e7ce44281f56e79521ef9e283f15780ae342f1abf81a8e` | 291 bytes |
| [Download manifest](https://github.com/Sugata-Software/Omuse/releases/download/v0.11.0/omuse-0.11.0-downloads.json) | `4c3aacc203fdd411f4544c3ab005d5fbcec3f5a39e32c258ed91b149dd5a13a1` | 1,950 bytes |

The corresponding executable SHA-256 values are
`8ae84dd9e2f6b940fe4e77d8599a251f0fdbb5788455ed2b289470046ccf1af2`
for Linux and
`9306cb2a1158cdea1640b70c3296555eeea14813152b384bf2de9e225d059080`
for Windows. The generated [download manifest](releases/downloads/v0.11.0.json)
records the archives, inventories and runner smoke receipts. Its bytes remain
unchanged from the successful build and match the anonymously downloaded asset.
All four assets passed anonymous size and SHA-256 verification. The source
declaration was published from
[`8260aa237aa0ab18af8007185d0659f2d49de91b`](https://github.com/Sugata-Software/Omuse/commit/8260aa237aa0ab18af8007185d0659f2d49de91b);
the public tag resolves to the tested runtime, and release notes, raw installer
bytes, source pin and latest declaration matched their reviewed versions.
The previous 0.10 tag, notes and every asset ID, size and hash were unchanged.

## Historical development evidence

The [canvas workflow qualification](canvas-workflows-qualification.md) records
the pre-version-bump checkpoint on `feature/canvas-workflows`, completed on
11 October 2026. Its Linux release-profile suite reported **1,599 passes, zero
failures and five ignored manual benchmarks in 71 result summaries**, using a fresh
four-directory XDG profile, locked offline dependencies and `ui-test`.

This includes 18 layout engine tests, four layout UI tests, ten context UI
tests, AI comparison helpers and composer/routing regressions. The same
development pass recorded:

- Native editing acceptance and four mixed raster/live-text/vector layout
  fixtures, including source preservation, save/reopen, exact Undo/Redo, Fill,
  Create resize and release of rules.
- A six-page collection, targeted editing, story variant, PNG/PDF package and
  MP4/GIF export journey; all 80 template variants completed their checks.
- 24 scripted native GPUI checks and separate Cua interaction on Linux/XWayland.
  Manual interaction covered frame bounds, Ungroup followed by Ctrl+Z, layout
  settings and Escape, proportional AI comparisons, Changes, original comparison,
  Refine, Collapse and exact draft preservation after reopening.
- The installed 0.10.0 reader refusing retained format 15. A rules-released
  fixture saved as format 14 and exported through that reader with absolute
  pixel error zero against the reference.

The original development screenshots came from this checkpoint's running
application and local synthetic AI pair. The guide now uses separately identified
[final-package captures](releases/images/v0.11.0/README.md). Neither the earlier
nor final AI fixture establishes generation quality or live account access.
The development native pass used XWayland; the final archive's separate backend
checks are listed above.

Source hashes, executable identities, logs and failed intermediate runs are
retained in evidence set `canvas-workflows-2026-10-11`. The final development
run used a fresh profile after an earlier reused diagnostic profile correctly
opened recovery for old packages; recovery behavior and assertions were not
weakened. These are historical checks with their own source/binary identity,
not final 0.11.0 package qualification.

## Compatibility and remaining limits

- **Retained layout requires project format 15.** Omuse 0.10.0 and older refuse
  those projects. Keep the retained `.omuse` master, then Release rules and
  Save As a separate copy in the newer editor for older use. Other features
  retain their own format requirements; rollback does not downgrade artwork.
- **Layout preserves editable sources but controls their placement.** It
  accepts direct, unrotated photo, live text and vector children. Nested frames
  or folders, adjustment layers and clipping relationships are not supported.
  Source rectangles include transparent padding; Fill scales text rather than
  rewrapping it. Fixed frames do not clip overflow. Wrap requires a fixed
  main-axis dimension and cannot combine with Fill.
- **Layout has explicit work bounds:** 64 frames, 256 children per frame and
  2,048 total members. Resource admission is not proof of acceptable performance
  at those limits. Destructive Image Size resampling requires Release rules;
  DPI-only changes remain supported.
- **AI comparison uses bounded previews.** Difference images and percentages
  use a visibility threshold and a longest edge of at most 640 pixels. They
  are neither full-resolution measurements nor quality scores. Sources must
  have compatible canvas/page framing. No new provider, account or subscription
  capability follows from local review-path tests.
- **Contextual actions preserve existing command rules.** The dock is a stable
  canvas control, with lock, focus, selection and modal prerequisites. It does
  not change the underlying editing or Undo model.
- **Windows remains unsigned experimental.** Automated Windows runner checks
  do not establish interactive Windows 10/11, mixed-DPI, accessibility,
  clean-machine or live-provider qualification. Broader hardware and foreground
  Wayland interaction beyond the scripted checks remain separate work.

Detailed release logs and machine-specific receipts remain outside public
source in evidence set `omuse-0110-2026-10-11`. Public CI, the reviewed identities
and the generated manifest identify this release source and its archives;
the successful publishers and anonymous readback establish public availability.
Local evidence set names are not public download links.
