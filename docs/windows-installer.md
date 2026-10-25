# Install Omuse on Windows

**[Download Omuse 0.12.0 Setup](https://github.com/Sugata-Software/Omuse/releases/download/v0.12.0/omuse-0.12.0-windows-x86_64-setup.exe)**
— 20.1 MB (20,057,949 bytes), Windows x64, unsigned experimental.
[Checksums](https://github.com/Sugata-Software/Omuse/releases/download/v0.12.0/omuse-0.12.0-windows-x86_64-setup.exe.sha256) ·
[Portable ZIP](https://github.com/Sugata-Software/Omuse/releases/download/v0.12.0/omuse-0.12.0-windows-x86_64.zip)

The 0.12 setup wizard passed all 14 automated native lifecycle checks on
25 October 2026. Windows remains unsigned experimental.

## Setup experience

The installer targets Windows 10/11 on Intel/AMD x64. It installs for your
account under `%LOCALAPPDATA%\Programs\Omuse`, adds a Start menu shortcut and
offers an optional desktop shortcut. The complete application, Camera RAW
library and licence notices stay together. Rust and Visual Studio are not
required, and Omuse setup does not request administrator rights or change your
PATH.

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

Use **Settings → Apps → Omuse → Uninstall** (under Installed apps or Apps & features) to remove the app
and its shortcuts. Projects, `%APPDATA%\omuse` settings and
`%LOCALAPPDATA%\omuse` recovery/data remain. Uninstall does not downgrade project
formats. If you need an older app, retain separate compatible project copies.

Existing portable copies remain separate: installing setup does not delete
their folders. Close the portable app before launching the installed one.

## Optional features

| Feature | Additional setup |
| --- | --- |
| Local subject selection | Microsoft's [Visual C++ Redistributable (x64)](https://aka.ms/vs/17/release/vc_redist.x64.exe); its installer may request administrator permission. |
| Guided subject mask — experimental | Download the optional guided-mask model explicitly from Omuse's guided-mask controls, then review its separate size, checksum, licence and provenance. The weights are not bundled in the portable ZIP or setup installer. |
| MP4 / GIF export | FFmpeg and ffprobe on PATH, for example `winget install Gyan.FFmpeg`. |
| Ask Omuse | The official Codex CLI or native Claude Code CLI, signed in, then **Ask Omuse → Connections**. Test the operation you intend to use. |

Omuse setup does not install these separate tools, download the optional guided
model or connect provider accounts.

## Keep a compatible project master

Omuse 0.12 can retain project features requiring document formats 16–19:
editable vector clipping, embedded colour looks, advanced paints or SVG
metadata, and Native16 Camera Raw recipe nodes. Omuse 0.11 and older cannot read
those newer project formats. Keep the editable 0.12 `.omuse` master and export a
separate raster copy (PNG/TIFF) for an older application. Save As alone does not
remove newer retained features. Installing or uninstalling an app version does
not convert project files.

## Build and verification

The 0.12.0 setup wraps the **exact published, reviewed Windows
ZIP** from public source
[`9c684cae4a4ccdb763d036b65adc440cc61419c2`](https://github.com/Sugata-Software/Omuse/commit/9c684cae4a4ccdb763d036b65adc440cc61419c2).
The reviewed ZIP SHA-256 is
`c0ea2b218ab80962e219dbfdfe912bd373ccb025f562d3e3a5b41c7c4825884b`;
its packaged executable SHA-256 is
`a550a1ae06596413a37db4cfd190117f20adf7d4e4cbccee3cfdf3c2fdb408f3`.
The setup builder verifies that declaration before compilation. It does not
rebuild the application or change its release tag.

The Windows workflow built old/current installers and checked actual install,
0.11 → 0.12 upgrade, same-version repair, downgrade refusal, shortcut targets,
Add/Remove Programs registration, installed editing journey, DLL loading,
uninstall and preservation of project/profile sentinels. The reviewed receipt
binds the compiler, installer, portable archive and native smoke hashes to
the exact packaging commit and CI run. Windows Server CI does not establish
interactive Windows 10/11, clean standard-user, SmartScreen or live AI behavior.

[Native Windows build 40163791906](https://github.com/Sugata-Software/Omuse/actions/runs/40163791906),
attempt 1, passed all 14 lifecycle checks. The
[reviewed receipt](releases/installers/v0.12.0.json) binds the 20,057,949-byte
`omuse-0.12.0-windows-x86_64-setup.exe`, SHA-256
`1e88601c148b0fd94fc7a832e7bbb62a91e7c16e6796c7efc3dc7b2c1112e990`,
to the exact portable payload, pinned Inno Setup 6.7.1 compiler and native smoke
receipt. [Publication 40163989927](https://github.com/Sugata-Software/Omuse/actions/runs/40163989927)
succeeded. Anonymous downloads of all seven release files matched their reviewed
sizes and SHA-256 hashes; setup receipt and checksum bytes matched exactly.
The runtime tag, numbered notes and original archive assets remain unchanged.
The prior 0.11 release and all its asset identities are preserved.

Publication is a separate, explicit step after reviewing the generated receipt
under `docs/releases/installers/vX.Y.Z.json`. It adds the installer, its receipt
and checksums to the existing release without replacing the ZIP, prior checksums,
notes, tag or other assets.

Local packaging on Windows with Inno Setup 6 and Python:

```powershell
./scripts/package-windows-installer.ps1 -Version 0.12.0 -Output candidate
```

The builder downloads only the declared portable release; `-Archive PATH` uses
an already downloaded copy with the same mandatory verification. Use a fresh
output directory. The native lifecycle test deliberately requires an isolated
Windows CI account because it exercises real user shortcuts and uninstall
registration.

## Historical 0.11.0 measured result

The [native Windows run](https://github.com/Sugata-Software/Omuse/actions/runs/38193823269)
passed all 14 lifecycle checks, including fresh installs of both versions and
0.10 to 0.11 upgrade. The [reviewed receipt](releases/installers/v0.11.0.json)
records the 19,563,421-byte installer and exact compiler, payload and test hashes.
[Publication run 38196166372](https://github.com/Sugata-Software/Omuse/actions/runs/38196166372)
succeeded. Anonymous downloads of the installer, receipt and checksum file
matched the reviewed bytes exactly. The original release assets retain their
IDs, sizes and hashes, and the release notes and runtime tag are unchanged.
