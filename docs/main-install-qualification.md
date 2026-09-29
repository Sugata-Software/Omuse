# Main Omuse installation — 29 September 2026

The current main application is **Omuse 0.2.0**, public runtime
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
