# Build a checksum-verifiable, full-feature Windows x86_64 zip.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\package-rust.ps1 -Binary PATH [-Output ZIP] [-RuntimeDir DIR]
#
# Packages an already-built omuse.exe. This script does not build, download,
# install, sign, or publish anything. A dirty source tree is rejected; set
# OMUSE_ALLOW_DIRTY=1 only for an explicitly local development package.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $Binary,
    [string] $Output,
    [string] $RuntimeDir
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $RuntimeDir) {
    $RuntimeDir = if ($env:OMUSE_RUNTIME_DIR) { $env:OMUSE_RUNTIME_DIR } else { Join-Path $repoRoot 'rust\runtime' }
}
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { throw "Executable not found: $Binary" }
foreach ($tool in 'git', 'python', 'cargo') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool is required." }
}

$revision = (& git -C $repoRoot rev-parse --verify HEAD).Trim()
$dirty = & git -C $repoRoot status --porcelain --untracked-files=normal
if ($dirty -and $env:OMUSE_ALLOW_DIRTY -ne '1') {
    throw 'Refusing to package a dirty source tree. Commit/stash all changes, or set OMUSE_ALLOW_DIRTY=1 for a local development package.'
}
$required = @(
    (Join-Path $RuntimeDir 'lib\libraw.dll'),
    (Join-Path $RuntimeDir 'lib\onnxruntime.dll'),
    (Join-Path $RuntimeDir 'models\u2netp.onnx'),
    (Join-Path $repoRoot 'LICENSE'),
    (Join-Path $repoRoot 'scripts\rust-license-inventory.py')
)
foreach ($path in $required) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Required package input is missing: $path" }
}
$notices = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'rust\licenses') -Filter *.txt -File) +
    @(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'rust\licenses\windows') -Filter *.txt -File)
if ($notices.Count -eq 0) { throw 'Runtime license notices are missing.' }

$short = $revision.Substring(0, 12)
if (-not $Output) { $Output = Join-Path $repoRoot "dist\omuse-$short-windows-x86_64.zip" }
$Output = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Force (Split-Path -Parent $Output) | Out-Null
$work = Join-Path ([IO.Path]::GetTempPath()) "omuse-package-$([Guid]::NewGuid().ToString('N'))"
$root = Join-Path $work "omuse-$short-windows-x86_64"
try {
    foreach ($name in 'lib', 'models', 'licenses') { New-Item -ItemType Directory -Force (Join-Path $root $name) | Out-Null }
    $inventory = Join-Path $work 'license-inventory'
    # Inventory the crates this release build compiles, not every optional feature.
    & python (Join-Path $repoRoot 'scripts\rust-license-inventory.py') $inventory --target x86_64-pc-windows-msvc --release-build
    if ($LASTEXITCODE -ne 0) { throw 'Rust dependency license inventory failed.' }

    Copy-Item -LiteralPath $Binary -Destination (Join-Path $root 'omuse.exe')
    Copy-Item -LiteralPath (Join-Path $RuntimeDir 'lib\libraw.dll') -Destination (Join-Path $root 'lib')
    Copy-Item -LiteralPath (Join-Path $RuntimeDir 'lib\onnxruntime.dll') -Destination (Join-Path $root 'lib')
    Copy-Item -LiteralPath (Join-Path $RuntimeDir 'models\u2netp.onnx') -Destination (Join-Path $root 'models')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'LICENSE') -Destination (Join-Path $root 'licenses\Omuse-MIT.txt')
    foreach ($notice in $notices) { Copy-Item -LiteralPath $notice.FullName -Destination (Join-Path $root 'licenses') }
    Copy-Item -LiteralPath (Join-Path $inventory 'inventory.json') -Destination (Join-Path $root 'licenses\rust-dependency-inventory.json')
    Copy-Item -LiteralPath (Join-Path $inventory 'THIRD_PARTY_NOTICES.txt') -Destination (Join-Path $root 'licenses\Rust-THIRD-PARTY-NOTICES.txt')

    $readme = @'
Omuse for Windows (development build)

Run omuse.exe. Keep the lib and models folders beside it: Camera RAW uses
lib\libraw.dll, and local subject selection uses lib\onnxruntime.dll with
models\u2netp.onnx.

Requirements
- Windows 10 or 11, x86_64.
- Local subject selection needs the Microsoft Visual C++ 2015-2022
  Redistributable (x64): https://aka.ms/vs/17/release/vc_redist.x64.exe
- MP4 and GIF export need FFmpeg on PATH, for example: winget install Gyan.FFmpeg

Camera RAW uses LibRaw with libjpeg-turbo and zlib. This software is based in
part on the work of the Independent JPEG Group. Licences are in the licenses folder.

Settings are stored in %APPDATA%\omuse; data and recovery in %LOCALAPPDATA%\omuse.
This build is unsigned and is not a release, so Windows SmartScreen may warn
before the first run. Ask Omuse uses Codex or Claude Code when their official
command-line tools are installed and signed in.
In a terminal, pipe command-line modes so the shell waits for them:
  .\omuse.exe --help | Out-Host
'@
    [IO.File]::WriteAllText((Join-Path $root 'README.txt'), $readme.Replace("`r`n", "`n").Replace("`n", "`r`n"))
    $dirtyLabel = if ($dirty) { 'true' } else { 'false' }
    [IO.File]::WriteAllText((Join-Path $root 'SOURCE-REVISION'), "source_revision=$revision`nsource_tree_dirty=$dirtyLabel`ntarget=windows-x86_64`nbundle_kind=full-feature`n")

    $sums = Get-ChildItem -LiteralPath $root -Recurse -File | Sort-Object { $_.FullName.Substring($root.Length + 1).Replace('\', '/') } | ForEach-Object {
        $relative = $_.FullName.Substring($root.Length + 1).Replace('\', '/')
        "$((Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant())  ./$relative"
    }
    [IO.File]::WriteAllText((Join-Path $root 'SHA256SUMS'), (($sums -join "`n") + "`n"))

    if (Test-Path -LiteralPath $Output) { Remove-Item -LiteralPath $Output -Force }
    # Write standard forward-slash entry names; Windows PowerShell's
    # Compress-Archive writes backslashes, which other unzip tools mishandle.
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::Open($Output, [IO.Compression.ZipArchiveMode]::Create)
    try {
        $top = Split-Path -Leaf $root
        foreach ($file in Get-ChildItem -LiteralPath $root -Recurse -File | Sort-Object FullName) {
            $entry = $top + '/' + $file.FullName.Substring($root.Length + 1).Replace('\', '/')
            [void] [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $file.FullName, $entry, [IO.Compression.CompressionLevel]::Optimal)
        }
    } finally {
        $zip.Dispose()
    }
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Output "Created package: $Output"
Write-Output "SHA-256: $((Get-FileHash -Algorithm SHA256 -LiteralPath $Output).Hash.ToLowerInvariant())"
