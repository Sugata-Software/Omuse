# Build the native Omuse editor for Windows.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\build-rust.ps1 [extra cargo arguments]
[CmdletBinding()]
param([Parameter(ValueFromRemainingArguments = $true)] [string[]] $CargoArgs = @())
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot

$jobs = if ($env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS } else { '2' }
if ($jobs -notmatch '^[1-9][0-9]*$') {
    Write-Error 'CARGO_BUILD_JOBS must be a positive integer.'
    exit 2
}
foreach ($tool in 'cargo', 'rustc') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        Write-Error "Missing build tool: $tool. Install Rust with rustup (https://rustup.rs)."
        exit 1
    }
}
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$msvc = if (Test-Path $vswhere) {
    & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not $msvc) {
    Write-Error 'Missing the MSVC C++ toolchain. Install Visual Studio Build Tools with "Desktop development with C++" (MSVC and a Windows SDK).'
    exit 1
}

Set-Location $repoRoot
& cargo build --manifest-path rust/Cargo.toml --release --locked --jobs $jobs @CargoArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Output 'Omuse build complete. Run: rust\target\release\omuse.exe [optional-project.omuse]'
