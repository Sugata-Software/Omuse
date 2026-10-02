# Optional, pinned Windows x86_64 runtime assets for local subject detection and RAW.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\prepare-rust-assets.ps1
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$assetDir = if ($env:OMUSE_RUNTIME_DIR) { $env:OMUSE_RUNTIME_DIR } else { Join-Path $repoRoot 'rust\runtime' }
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    Write-Error 'The ONNX Runtime archive is for Windows x86_64.'
    exit 1
}
foreach ($name in 'lib', 'models', 'sources') {
    New-Item -ItemType Directory -Force (Join-Path $assetDir $name) | Out-Null
}

function Get-Sha256([string] $Path) {
    (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-Verified([string] $File, [string] $Url, [string] $Expected) {
    if ((Test-Path -LiteralPath $File) -and (Get-Sha256 $File) -eq $Expected) { return }
    & curl.exe --fail --location --retry 2 --proto '=https' --tlsv1.2 --silent --show-error $Url --output "$File.part"
    if ($LASTEXITCODE -ne 0) { throw "Download failed: $Url" }
    if ((Get-Sha256 "$File.part") -ne $Expected) {
        Remove-Item -LiteralPath "$File.part" -Force
        throw "Checksum mismatch: $File"
    }
    Move-Item -LiteralPath "$File.part" -Destination $File -Force
}

function Write-AssetSums {
    $lines = foreach ($relative in 'lib/libraw.dll', 'lib/onnxruntime.dll', 'models/u2netp.onnx') {
        "$(Get-Sha256 (Join-Path $assetDir $relative))  $relative"
    }
    [IO.File]::WriteAllText((Join-Path $assetDir 'ASSET-SHA256SUMS'), (($lines -join "`n") + "`n"))
}

function Test-AssetSums {
    $sums = Join-Path $assetDir 'ASSET-SHA256SUMS'
    if (-not (Test-Path -LiteralPath $sums)) { return $false }
    foreach ($line in Get-Content -LiteralPath $sums) {
        $hash, $relative = $line -split '  ', 2
        $path = Join-Path $assetDir $relative
        if (-not (Test-Path -LiteralPath $path) -or (Get-Sha256 $path) -ne $hash) { return $false }
    }
    return $true
}

# Reuse complete verified assets; updates should not rebuild LibRaw needlessly.
$recipe = Get-Sha256 $PSCommandPath
$recipeFile = Join-Path $assetDir 'ASSET-RECIPE'
if ((Test-Path -LiteralPath $recipeFile) -and (Get-Content -Raw -LiteralPath $recipeFile).Trim() -eq $recipe -and (Test-AssetSums)) {
    Write-Output "Using verified runtime assets: $assetDir"
    exit 0
}

$sources = Join-Path $assetDir 'sources'
Get-Verified (Join-Path $assetDir 'models\u2netp.onnx') 'https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx' '309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8'
Get-Verified (Join-Path $sources 'LibRaw-0.22.2.tar.gz') 'https://www.libraw.org/data/LibRaw-0.22.2.tar.gz' 'de86b035655accff8d4010f1a221fdf50d353cb7b1422ba26f14a0db92612cfa'
# Archive digest as published by the onnxruntime v1.23.2 GitHub release.
Get-Verified (Join-Path $sources 'onnxruntime-win-x64-1.23.2.zip') 'https://github.com/microsoft/onnxruntime/releases/download/v1.23.2/onnxruntime-win-x64-1.23.2.zip' '0b38df9af21834e41e73d602d90db5cb06dbd1ca618948b8f1d66d607ac9f3cd'

$scratch = Join-Path ([IO.Path]::GetTempPath()) "omuse-assets-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory $scratch | Out-Null
try {
    Expand-Archive -LiteralPath (Join-Path $sources 'onnxruntime-win-x64-1.23.2.zip') -DestinationPath $scratch
    $runtime = Join-Path $scratch 'onnxruntime-win-x64-1.23.2\lib\onnxruntime.dll'
    if ((Get-Sha256 $runtime) -ne 'dec964ab1ee36cc9b0ae247d13b376627992fc57dec0454354017ab8fd84f1ea') {
        throw 'Unexpected onnxruntime.dll in the verified archive'
    }
    Copy-Item -LiteralPath $runtime -Destination (Join-Path $assetDir 'lib\onnxruntime.dll') -Force

    # Build LibRaw from the pinned source with MSVC and a static C runtime, so
    # libraw.dll needs no Visual C++ Redistributable.
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $vs = if (Test-Path -LiteralPath $vswhere) {
        & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    }
    if (-not $vs) { throw 'Building LibRaw needs Visual Studio Build Tools with "Desktop development with C++".' }
    $vcvars = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'
    & tar.exe -xzf (Join-Path $sources 'LibRaw-0.22.2.tar.gz') -C $scratch
    if ($LASTEXITCODE -ne 0) { throw 'Could not extract the LibRaw source' }
    $libraw = Join-Path $scratch 'LibRaw-0.22.2'
    & cmd.exe /d /c "call `"$vcvars`" >nul && cd /d `"$libraw`" && nmake /nologo -f Makefile.msvc COPT=`"/EHsc /MP /MT /I. /DWIN32 /O2 /W0 /nologo`" bin\libraw.dll"
    if ($LASTEXITCODE -ne 0) { throw 'LibRaw build failed' }
    Copy-Item -LiteralPath (Join-Path $libraw 'bin\libraw.dll') -Destination (Join-Path $assetDir 'lib\libraw.dll') -Force

    Write-AssetSums
    [IO.File]::WriteAllText($recipeFile, "$recipe`n")
} finally {
    Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Output "Runtime assets ready: $assetDir"
