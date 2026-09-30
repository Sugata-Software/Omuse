# Install Omuse

The Omuse installer supports **Omarchy / Arch Linux on x86_64**.
Run this in a terminal as your regular desktop user:

```sh
curl -fsSL https://raw.githubusercontent.com/Sugata-Software/Omuse/main/install.sh | bash
```

Open **Omuse** in your application launcher when installation completes. For a
terminal launch, use `~/.local/bin/omuse`. You can [read the installer](../install.sh)
first, or download and inspect it before running `bash install.sh`.

## What happens

1. The installer checks the platform and installs missing system build/media
   packages through pacman. Only this step uses sudo, and pacman shows its usual
   confirmation unless you explicitly pass `--yes`.
2. It downloads the tested source revision from Sugata Software's public GitHub
   repository and uses the pinned Rust toolchain. A matching installed compiler
   is reused. If needed, Rust is installed for the user without replacing distro
   Rust or editing shell startup files.
3. It builds the editor locally, then fetches checksum-pinned Camera RAW and
   local subject-selection assets. Image editing and these local tools need no
   AI subscription or API key.
4. It stages the complete application, runs an editing self-test with isolated
   settings and synthetic artwork, and only then activates the installation.
5. It adds the Omuse command, desktop entry, icons and maintenance command.

The first build is substantial: allow time and about **12 GB of free disk
space**. Two build jobs are used by default. Later installs reuse Cargo outputs
and verified runtime assets. A stable source cache avoids unnecessary recompilation;
updates refuse to overwrite local edits in that cache. This installer **builds
from source**; a downloadable binary release remains subject to the
[release qualification gates](public-release-readiness.md).

## Tested source channel

The current numbered source release is [Omuse 0.6.0](releases/v0.6.0.md).
Its Git tag identifies the tested runtime; the normal curl command follows the
current tested channel and may advance to later qualified releases.

The public installer selects commit
[`a8b7ac70e7513d305a671673a347eecaf2d6cc4c`](https://github.com/Sugata-Software/Omuse/commit/a8b7ac70e7513d305a671673a347eecaf2d6cc4c).
It fetches that exact revision and checks the checkout before building.
The [full Rust/installer workflow](https://github.com/Sugata-Software/Omuse/actions/runs/36784517002) passed for this source.
The [0.6.0 qualification](release-060-qualification.md) records 1,105 application
cases, editing/Create/motion journeys, all 80 template variants, three production
and installed native journeys with 24 checks each, and complete 19-file rollback.

All new projects use `.omuse`. Older projects remain readable and are not
rewritten merely by opening them. **0.6.0 writes canvas format 10**, including
pages in collections; **0.5.0 and earlier cannot read those new saves**. Use
**Save As** to retain an older copy. App rollback does not downgrade artwork.
Collection schema v2 is separate and requires 0.5.0 or later. No new live AI
request was sent for this editing release; earlier provider receipts keep their
own runtime identities. Start with the [user manual](user-guide/README.md).

The curl command downloads `install.sh` from `main`, so changes to the installer
script take effect immediately. The application checkout is pinned separately:
application-source changes on `main` are not built until the pin advances.
Maintainers advance `omuse_release_revision`
in `install.sh` only after the new public runtime commit passes its automated and
applicable desktop checks. A failed fetch or revision mismatch preserves the
installed application. Developers can explicitly select their own checkout with
`--source`; that path bypasses the tested source channel.

The installation receipt is `~/.local/opt/omuse/current/SOURCE-REVISION`.
It records the application source, even when the installer or documentation has
a newer commit. This is a source selection guarantee, not a claim of reproducible
binary output across different machines.

## Update, rollback and remove

Run the same curl command to install the latest tested revision. A failed build
or self-test leaves the installed application active. Updates keep the previous executable and its
matching runtime assets together, so rollback does not mix library versions:

```sh
~/.local/bin/omuse-manage rollback
```

Close and reopen Omuse to use the restored version. Rollback can be repeated to
switch back. The first installation has no previous version to restore.

Remove the installed application:

```sh
~/.local/bin/omuse-manage uninstall
```

Uninstall preserves projects, settings, recovery data and build caches. It
removes tracked launchers/icons only if their contents still match the install
manifest; locally modified files are reported and kept. An older flat Omuse
installation is copied into a rollback generation during the first upgrade;
its original support files may remain after uninstall. Other applications are
not removed.

## Locations and options

The default prefix is `~/.local`. The launcher is `bin/omuse`; app generations
live under `opt/omuse/releases/`. The `current` and `previous` links select
complete installations. Build outputs, runtime downloads and timestamped logs
are under `${XDG_CACHE_HOME:-~/.cache}/omuse/installer/`. Old generations stay
available until uninstall so running applications can retain their own assets.

For another prefix or a lower-memory build:

```sh
curl -fsSL https://raw.githubusercontent.com/Sugata-Software/Omuse/main/install.sh | bash -s -- --prefix "$HOME/Apps/Omuse" --jobs 1
```

A custom prefix's desktop entries may be outside your desktop's normal search
path; launch its `bin/omuse` directly. Maintenance commands use that same prefix.
The default prefix provides normal per-user desktop integration.

Use `--help` to inspect all options. `--no-deps` skips pacman when prerequisites
are already present; it is also the manual path for other Linux distributions,
which need separate qualification. `--no-runtime-assets` installs only the core
editor without Camera RAW or local subject tools. `--source /path/to/Omuse`
builds an existing checkout. `--yes` accepts pacman's package prompts but does
not bypass sudo authentication. The installer never refreshes package databases
alone or performs a system upgrade.

`CARGO_BUILD_JOBS`, absolute `CARGO_TARGET_DIR`, `OMUSE_INSTALL_PREFIX` and
`OMUSE_RUNTIME_DIR` overrides are available for development. Settings and
recovery remain in the [normal XDG locations](../README.md#projects-and-user-data).

## If installation stops

The installer prints the failed step and its log location. Rerun the same
command after correcting the cause; cached source dependencies are reused.
If pacman reports stale mirrors, update your Arch system through your normal
system-update workflow, then retry. If a compiler is killed for lack of memory,
close memory-intensive applications and retry with `--jobs 1`.

A successful headless self-test does not establish graphics-driver or display
compatibility. For a window that fails to open, run `~/.local/bin/omuse` from a
terminal and include the error, source revision, GPU and session type in a
[bug report](https://github.com/Sugata-Software/Omuse/issues/new/choose). Review
logs for personal information before sharing them.

Developers can use the [Linux build guide](../rust/README.md) and
[offline bundle guide](rust-bundle.md). The remaining release gates are tracked
in [public release readiness](public-release-readiness.md).
