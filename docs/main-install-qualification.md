# Main Omuse installation — 29 September 2026

The laptop's normal **Omuse** command and desktop entry now open the tested
Rust/GPUI application. The separate **Omuse Preview** launcher and the older
Compositor menu entries were archived. Their application data and the running
older session were preserved.

## Installed identity and rollback

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

## Desktop checks

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

## Public installer channel

The curl installer now fetches the exact tested public application revision
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
