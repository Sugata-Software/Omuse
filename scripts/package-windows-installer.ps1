# Wrap the exact reviewed portable release; never rebuild or replace its payload.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $Version,
    [Parameter(Mandatory = $true)] [string] $Output,
    [string] $Archive
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($Version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') { throw 'Invalid version' }
$repo = Split-Path -Parent $PSScriptRoot
$Output = [IO.Path]::GetFullPath($Output)
$compiler = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
if (-not (Test-Path -LiteralPath $compiler -PathType Leaf)) { throw 'Inno Setup 6 is required on the packaging machine.' }
$compilerVersion = (Get-Item -LiteralPath $compiler).VersionInfo.FileVersion
if ($compilerVersion -notmatch '^6\.') { throw 'Only the reviewed Inno Setup 6 compiler family is supported.' }
$arguments = @((Join-Path $PSScriptRoot 'windows-installer.py'), 'stage', '--version', $Version, '--output', $Output)
if ($Archive) { $arguments += @('--archive', [IO.Path]::GetFullPath($Archive)) }
& python @arguments
if ($LASTEXITCODE -ne 0) { throw 'Portable payload verification failed.' }
& $compiler '/Qp' "/DAppVersion=$Version" "/DPayloadDir=$Output\payload" "/DOutputDir=$Output" (Join-Path $repo 'packaging\windows\omuse.iss')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
$metadata = @{
    version = $compilerVersion
    sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $compiler).Hash.ToLowerInvariant()
}
[IO.File]::WriteAllText((Join-Path $Output 'compiler.json'), ($metadata | ConvertTo-Json))
Write-Output "Created $Output\omuse-$Version-windows-x86_64-setup.exe"
