# Main Omuse installation — 2 October 2026

## Current Omuse 0.7.0 release

The normal launcher and curl installer select clean public source
`5b3daefbb5258afff4a74a2ff3db5247b074972d`. Its [exact-source GitHub validation](https://github.com/Sugata-Software/Omuse/actions/runs/36910650216)
passed. The [qualification](release-070-qualification.md) and
[receipt](release-070-receipts.json) record 1,206 application tests, editing,
Create/media, 80 template variants, tracing/vector acceptance and recovery.

- Production SHA-256: `fdb1e47366fec20668d19e5969c34182c9c934dc90946993715c6820b8db05ea`.
- Current generation: `install-gowujrk8`, complete 20-file Omuse 0.7.0 payload.
- Previous generation: `install-brzel9z6`, complete 19-file Omuse 0.6.0 payload.
- Wayland and XWayland each passed 27 production checks at 800×600 logical
  size. The installed normal launcher passed 24 checks at that same minimum.
- Ten more production journeys passed and produced the inspected
  [interface gallery](releases/v0.7.0-gallery.md).
- Both rollback directions passed isolated editing self-tests, and every
  payload hash stayed unchanged. The normal command reports **Omuse 0.7.0**;
  the desktop entry validates and the source receipt is clean.

Existing windows retain their running executable until reopened. User artwork,
settings and provider profiles were preserved. New format-11 vector scenes and
Target Colour Uniformity recipes require 0.7.0; use Save As to preserve an
older compatible original. App rollback does not downgrade artwork. This is
development-host installation evidence, not a clean-machine or portable-binary
qualification. All sections below are historical snapshots.

## Historical Omuse 0.6.0 release

The normal launcher and curl installer select clean public source
`a8b7ac70e7513d305a671673a347eecaf2d6cc4c`. The [exact-source GitHub workflow](https://github.com/Sugata-Software/Omuse/actions/runs/36784517002) passed.
The [qualification](release-060-qualification.md) and
[compact receipt](release-060-receipts.json) record 1,105 application cases,
editing/Create/motion, 80 template variants, compatibility and native checks.

- Production SHA-256: `bf3b8f5f510865fd832d3305b1b2ea40505c5eab6c93bf9fa2d49dc629b41a46`.
- Current generation: `install-brzel9z6`, complete 19-file Omuse 0.6.0 payload.
- Previous generation: `install-p5qqp974`, complete 19-file Omuse 0.5.0 payload.
- Wayland, XWayland and the installed Wayland launcher each passed 24 native
  checks at the 800×600 logical minimum viewport with reduced motion.
- Both rollback directions passed isolated editing self-tests, with every
  payload hash preserved. The normal command reports **Omuse 0.6.0** and the
  desktop file validates.

Existing windows keep their running executable until reopened. Original artwork,
settings and provider profiles remain intact. New format-10 saves cannot be read
by 0.5.0 or earlier; app rollback does not downgrade files. Use Save As to retain
older artwork. The [0.5.0 record](release-050-qualification.md) and all sections
below are historical evidence for their own candidates.

## Historical Omuse 0.4.0 release

The normal launcher selects public source `3f5ece38ba1ee84af2d80062412bd9e11e26fbe3` with a clean source
receipt. The curl installer pins the same tested runtime. Complete GitHub
[validation](https://github.com/Sugata-Software/Omuse/actions/runs/36669607056), 949 local application cases, editing/Create/motion
journeys and all 80 template variants passed.

- Production SHA-256: `b5d58b871b1c99dd7239e4a0033df80f303683f8a54fa1befb756c04ca02cd51`.
- Current generation: `install-fohyiqbu`, all 19 files.
- Previous generation: `install-kf3h5ptm`, public panel
  candidate `521f9f1`, retained with all 19 files.
- Production Wayland, production XWayland and installed Wayland each passed
  24 native checks at the 800×600 minimum viewport with reduced motion.
- Rollback and the reverse switch passed, retaining every payload hash.
- The normal command reports **Omuse 0.4.0**; desktop-file validation passed.

The [qualification](release-040-qualification.md) and
[compact receipt](release-040-receipts.json) record identities and boundaries.
Existing windows keep their running executable until reopened. User artwork,
settings and provider profiles were preserved. This development-host promotion
does not establish clean-host or portable-binary qualification. All sections
below retain historical measurements and installation states.

## Historical local AI connection candidate

At this earlier qualification, the development host selected public candidate
`31170322e0241380a10c337c3cf18ab11fb41ef0`, tree
`98351ef8e7c0a43d5c95d098e985ac20e0c7dedd`. This is a local corrective build;
it still reports Omuse 0.3.0 but is not the published 0.3.0 release.

- Production executable SHA-256:
  `598ebb81d475406fedbb3e8ba3a362c5c7ccce16ec4ac8cd96267c2d59061de8`.
- Release build completed with two locked build jobs.
- Active generation: `install-hytfk9hx`, clean source receipt, 19 payload files.
- Previous complete generation: `install-o_h4lt_d`, public runtime `ea187a9`,
  retained with all 19 payload files.
- All 387 library tests passed, including 14 Claude and seven discovery cases.
- Production native Wayland passed all 24 checks at 800×600 with reduced motion.
- The normal launcher no longer needs the temporary provider PATH workaround.
- Exact installed XWayland Cua input passed one Claude Design journey with
  Review, Keep, toolbar Undo and Redo. See the
  [bounded Claude qualification](ai-claude-qualification.md).

At that point the public release and curl installer remained pinned to `ea187a9`. Draft pull
request [#1](https://github.com/Sugata-Software/Omuse/pull/1) was not yet merged;
the exact candidate passed complete
[GitHub validation](https://github.com/Sugata-Software/Omuse/actions/runs/36612969908)
with 910 passing application tests and the editing, motion, recovery, installer
and dependency checks. This local installation is not a release promotion,
clean-host receipt or portable-binary qualification.

## Historical public 0.3.0 release baseline

The application for that promotion was **Omuse 0.3.0**, public runtime
`ea187a900c06ecc68c8ea70635f2abb09cf933b2`. Its complete
[GitHub validation](https://github.com/Sugata-Software/Omuse/actions/runs/36594138936)
passed. The [editing qualification](editing-workflows-qualification.md) records
901 passing application cases, editing/Create/motion journeys, 80 template
variants, native/Cua checks and exact source-pixel clipboard exchange.

- Production executable SHA-256: `c61f4ff4544d73343ad3c68184b152131abd8517741055c27ef611a714d5fa03`.
- Public baseline generation: `install-o_h4lt_d`, clean public-source receipt.
- Previous complete generation: `install-ac5gtmjb`, Omuse 0.2.1 (`189f3e7`).
- All 19 payload hashes matched through rollback to 0.2.1 and the reverse switch.
- Production Wayland, production XWayland and the installed normal Wayland
  launcher each passed 24 native checks at an 800×600 logical minimum viewport.
  The installed run used reduced motion. Crop/theme captures were inspected.
- The wrapper resolves the qualified payload; `omuse --version` prints 0.3.0.
  The normal desktop entry passed desktop-file validation. Cua separately
  launched and drove the exact production binary in isolated XWayland fixtures.
  Native Wayland PNG publication passed a separate exact-window input check.

At that point the curl installer pinned this source. The public generation
remained the rollback target while the development host selected the local
corrective candidate above. Installation did not
terminate windows containing user work; documents, settings and provider
profiles were preserved. Development-host qualification does not establish
clean-target or portable binaries. The [compact receipt](editing-workflows-receipts.json)
records the public production identity and its two historical payload generations.

## AI image-editing installation (historical 0.2.1 baseline)

The application for this promotion was **Omuse 0.2.1**, public runtime `189f3e77e33f1b9fae7a7a5d19cf8a2a3bd930dd`.
Its complete [GitHub validation](https://github.com/Sugata-Software/Omuse/actions/runs/36558893513)
passed. The [image-editing qualification](ai-image-editing-qualification.md)
records the 858-case test scope, production/native checks, separate live-image
runtime and final Cua checks.

- Production executable SHA-256: `fd5a5400c16f3f1462132de950a01cceee7a31675d9802577316c50e69da6012`.
- Active generation: `install-ac5gtmjb`, clean public-source receipt.
- Previous complete generation: `install-hly2uob3`, Omuse 0.2.0 (`3dc3e46`).
- All 19 payload hashes matched through rollback to 0.2.0 and the reverse switch.
- Production Wayland, production XWayland and the installed normal Wayland
  launcher each passed 24 native checks at an 800×600 logical minimum viewport.
  The installed run used reduced motion. Captures were inspected.
- The wrapper resolves the qualified payload; `omuse --version` prints 0.2.1.
  The normal desktop entry passed desktop-file validation. Cua separately
  launched and drove the exact production binary in isolated XWayland fixtures.

At that promotion the normal command, desktop entry and curl installer selected
the same source. Documents, settings and provider profiles were preserved.
Development-host qualification did not establish clean-target or portable binaries.

## Ask Omuse installation (historical 0.2.0 baseline)

The main application for this promotion was **Omuse 0.2.0**, public runtime
`3dc3e46310e40dcf109b1bc695bb4e9dda6d2d24`. Its complete
[GitHub validation](https://github.com/Sugata-Software/Omuse/actions/runs/36528288193)
passed, with 843 application tests and the recorded editing/Create journeys.
The [Ask Omuse qualification](ai-experience-qualification.md) records the
production build, live refinement and all installation evidence.

- Executable SHA-256: `2b4e19a54553ad1836f49da23373d1038420369c298d2384d0ca1bdc5bae16dc`.
- Active generation: `install-hly2uob3`, clean public-source receipt.
- Previous generation: `install-y4k24szz` (`9c99e50`).
- All 19 payload hashes matched after rollback and the reverse switch.
- Production Wayland, production XWayland and the installed main Wayland
  launcher each passed 24 native checks; the installed run used reduced motion.
- Desktop-file validation and normal desktop launch passed. Compositor and
  process readback verified the mapped Wayland window and exact executable.

The normal Omuse command and desktop entry select this tested build; the curl
installer pins the same source. The previous complete generation remains
available. User documents, settings and provider profiles were preserved.
These development-host checks do not qualify clean-target or portable binaries.

## OmaPhoto comparison promotion (historical `9c99e50` baseline)

The following record describes the preceding same-day installation.

The application for this promotion was the OmaPhoto comparison candidate
`9c99e50684afd0854c8094a139b43205a263c33d`. Its full [GitHub validation](https://github.com/Sugata-Software/Omuse/actions/runs/36492555743)
passed, alongside **808 local tests**, editing/Create export journeys and all
80 template variants. The [comparison](omaphoto-comparison.md) records the new
Camera Raw scopes, background preview compositing, canvas/group interaction
fixes, Eraser smoothing and stronger PSD regressions.

- Production executable SHA-256:
  `19bd5d4c6c682462f1a916ec7f04c86aa3a6d50da9c5d4ba75e435d0be4e19d6`.
- Installed generation: `install-y4k24szz`, with clean public source receipt.
- Previous generation: `install-mjdrhhwp` (`1d5ccaa`), retained for rollback.
- All 19 payload files were verified after a complete rollback and reverse
  switch; both versions passed isolated editing self-tests.
- All 36 installer regressions passed after advancing the public source pin:
  25 source/manager cases and 11 bundle cases.
- Production passed 24 native checks on Wayland at 800×600 logical pixels and
  on XWayland at its recorded 1264×766.7 viewport. Light/dark Camera Raw captures
  were inspected. Compact dialogs scroll their body while retaining the footer.
- The installed main launcher separately passed all 24 Wayland checks. Its
  larger dark-theme capture shows the complete curve graph and scopes together.
  The harness hashes the launcher; the underlying executable was hashed
  independently as recorded above.
- The unchanged earlier window was closed normally. A launch through the
  `omuse.desktop` entry mapped a native Wayland window running the exact new
  executable. Desktop-file validation passed; there remains one main Omuse entry.

The curl installer selects this same tested source. User documents, settings and
provider profiles were preserved. These are development-host and bounded native
checks, not a clean-target or universal physical-input claim.

## Initial main-app promotion (historical `1d5ccaa` baseline)

The record below describes the earlier same-day promotion; its original hashes
and evidence are retained for traceability.

This promotion made the laptop's normal **Omuse** command and desktop entry
open the tested Rust/GPUI application. The separate **Omuse Preview** launcher and the older
Compositor menu entries were archived. Their application data was preserved.
The unchanged older window was closed through a normal Wayland close request;
the promoted main application was then launched and verified.

### Installed identity and rollback

The complete payload was promoted without recompiling:

- Public application source: `1d5ccaa4d0037f2353d698611497ce5aff46a577`.
- Source receipt: clean, Linux x86_64.
- Executable SHA-256: `df38a2dfa2f57cd6d663358a56e4d9ca3ef805214ca819ab93fee460d4d4d48b`.
- Every payload file, including the two runtime libraries, subject model,
  icons, notices and source receipt, matched the tested installation.
- The earlier flat main installation became a complete rollback generation.
  Its executable SHA-256 is
  `10e8ca5b55182fd03c8f09b2bd2c41d6a0ea7041026ee997ea4c606de0ad80cf`.
- Rollback and the reverse switch both passed their isolated editing self-test
  and selected the expected complete generation. The final active generation
  is the new application.

The command is `~/.local/bin/omuse`; the desktop name is **Omuse** and the desktop
file is `omuse.desktop`. `omuse-manage rollback` retains the existing recovery
path. No project, settings or provider profile was merged or deleted.

### Desktop checks

The installed main launcher passed all **24 native Wayland checks** at the
800×600 logical viewport. These include painting, Undo/Redo, unsaved-work
protection, save/reopen pixel equality, theme following, text, adjustments,
effects, selection masks, retained 16-bit import/export, photo editing/export
and keyboard command-search execution with focus restoration. Its command
panel capture was visually inspected.

The native harness records the launcher script's hash in its executable field.
The underlying production executable was hashed independently, as recorded
above; those two identities must not be confused.

The desktop file passed `desktop-file-validate`. A launch through the standard
GTK desktop launcher produced a mapped native Wayland window whose process
resolved to the new managed executable with the expected hash. Cua's app list
showed one Omuse desktop entry after the duplicate launchers were retired.
Cua 0.29.1 could not resolve the quoted desktop command directly or confirm the
native activation handoff; compositor/process readback established that the
GTK launch succeeded. This does not add native Cua input qualification.

### Public installer channel

This promotion changed the curl installer to fetch the exact tested public application revision
instead of following development changes on `main`. It checks the resulting
Git HEAD before building. A fresh fetch from the public GitHub repository
returned the expected source commit and tree. The explicit `--source` development override remains
available. The [installation guide](install.md#tested-source-channel) records
the selected source and its completed remote runtime workflows.

**36 installer tests passed locally:** 25 source/manager cases and 11 bundle
cases. New real-Git fixtures prove that both a fresh install and a cached update
ignore a newer unqualified `main`, and that an unavailable source revision
leaves the installed application active. A cached upgrade to the next tested
revision passed and retained the previous source and executable for rollback.
Reachable updates that fail during build or editing self-test preserve every
installed file and symlink, with the previous application still runnable.
Existing failure, rollback,
uninstall, path-escaping and archive-integrity checks still pass.

This is development-host installation evidence. It does not establish a clean
machine installation, a portable binary package, all provider operations or
physical display/input coverage. The remaining [binary release gates](public-release-readiness.md)
still apply. Historical qualification records retain the former Preview name
to identify the isolated installation actually used at that time.
