# Install Omuse on Windows

The Windows setup wizard for Omuse 0.11.0 passed native lifecycle checks on
12 October 2026 and is awaiting publication. Until a verified
installer download is linked here, use the [published portable ZIP](https://github.com/Sugata-Software/Omuse/releases/download/v0.11.0/omuse-0.11.0-windows-x86_64.zip).

## Setup experience

The installer targets Windows 10/11 on Intel/AMD x64. It installs for your
account under `%LOCALAPPDATA%\Programs\Omuse`, adds a Start menu shortcut and
offers an optional desktop shortcut. The complete application, Camera RAW
library, subject-selection model and licence notices stay together. Rust and
Visual Studio are not required, and Omuse setup does not request administrator
rights or change your PATH.

1. Save your work and close Omuse before installing an update.
2. Run the versioned `omuse-X.Y.Z-windows-x86_64-setup.exe` from our GitHub release.
3. Follow the setup wizard, then open **Omuse** from Start.

Windows builds remain **experimental and unsigned**. SmartScreen may identify
an unknown publisher. Check that the download came from
`github.com/Sugata-Software/Omuse`; the accompanying `.sha256` file lets you verify
the installer with PowerShell's `Get-FileHash -Algorithm SHA256`.

## Updates, repair and uninstall

Run the newer installer to update the same installation. Running the same
version again repairs missing packaged files. Setup does not force-close the
editor; save and close it first. An older installer refuses to overwrite a
newer registered version.

Use **Settings → Apps → Installed apps → Omuse → Uninstall** to remove the app
and its shortcuts. Projects, `%APPDATA%\omuse` settings and
`%LOCALAPPDATA%\omuse` recovery/data remain. Uninstall does not downgrade project
formats. If you need an older app, retain separate compatible project copies.

Existing portable copies remain separate: installing setup does not delete
their folders. Close the portable app before launching the installed one.

## Optional features

| Feature | Additional setup |
| --- | --- |
| Local subject selection | Microsoft's [Visual C++ Redistributable (x64)](https://aka.ms/vs/17/release/vc_redist.x64.exe); its installer may request administrator permission. |
| MP4 / GIF export | FFmpeg and ffprobe on PATH, for example `winget install Gyan.FFmpeg`. |
| Ask Omuse | The official Codex CLI or native Claude Code CLI, signed in, then **Ask Omuse → Connections**. Test the operation you intend to use. |

Omuse setup does not install these separate tools or connect provider accounts.

## Build and verification

The installer wraps the **exact published, reviewed Windows ZIP**, identified
by the matching declaration in `docs/releases/downloads/`. Every archive member,
payload checksum, source revision and licence inventory is checked before
compilation. It does not rebuild the application or change its release tag.

The Windows workflow builds old/current installers and checks actual install,
0.10 → 0.11 upgrade, same-version repair, downgrade refusal, shortcut targets,
Add/Remove Programs registration, installed editing journey, DLL loading,
uninstall and preservation of project/profile sentinels. The candidate receipt
binds the compiler, installer, portable archive and native smoke hashes to the
exact packaging commit and CI run. Windows Server CI does not establish
interactive Windows 10/11, clean standard-user, SmartScreen or live AI behavior.

The [native Windows run](https://github.com/Sugata-Software/Omuse/actions/runs/38193823269)
passed all 14 lifecycle checks, including fresh installs of both versions and
0.10 to 0.11 upgrade. The [reviewed receipt](releases/installers/v0.11.0.json)
records the 19,563,421-byte installer and exact compiler, payload and test hashes.

Publication is a separate, explicit step after reviewing the generated receipt
under `docs/releases/installers/vX.Y.Z.json`. It adds the installer, its receipt
and checksums to the existing release without replacing the ZIP, prior checksums,
notes, tag or other assets.

Local packaging on Windows with Inno Setup 6 and Python:

```powershell
./scripts/package-windows-installer.ps1 -Version 0.11.0 -Output candidate
```

The builder downloads only the declared portable release; `-Archive PATH` uses
an already downloaded copy with the same mandatory verification. Use a fresh
output directory. The native lifecycle test deliberately requires an isolated
Windows CI account because it exercises real user shortcuts and uninstall
registration.
