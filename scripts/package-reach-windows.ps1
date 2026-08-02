# Build a Windows Reach player zip for local distribution.
#
# Usage (from repo root, in PowerShell):
#   pwsh ./scripts/package-reach-windows.ps1
#   pwsh ./scripts/package-reach-windows.ps1 -SkipBuild
#
# Output:
#   dist/Reach-windows/reach.exe
#   dist/Reach-windows/levels/
#   dist/Reach-windows/assets/
#   dist/Reach-windows.zip
#
# Layout matches Reach's flat release root: exe + levels/ + assets/ side by side
# (see games/reach/src/main.rs `root_dir`).
param(
    [switch]$SkipBuild,
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help) {
    Write-Host "Usage: ./scripts/package-reach-windows.ps1 [-SkipBuild]"
    exit 0
}

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

$Dist = Join-Path $Root "dist"
$OutDir = Join-Path $Dist "Reach-windows"
$Zip = Join-Path $Dist "Reach-windows.zip"

if ($IsWindows -eq $false -and $env:OS -ne "Windows_NT") {
    Write-Error "This script packages a Windows Reach zip. On other OSes use ./scripts/package-reach.sh (macOS) or cross-compile with a Windows target."
}

if (-not $SkipBuild) {
    Write-Host "==> cargo build -p reach --release"
    cargo build -p reach --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$metaJson = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
$TargetDir = $metaJson.target_directory
$BinSrc = Join-Path $TargetDir "release/reach.exe"
if (-not (Test-Path $BinSrc)) {
    # Some hosts produce `reach` without .exe when cross-checking; prefer .exe on Windows.
    $alt = Join-Path $TargetDir "release/reach"
    if (Test-Path $alt) { $BinSrc = $alt }
}

if (-not (Test-Path $BinSrc)) {
    Write-Error "Missing release binary at $BinSrc — run without -SkipBuild"
}

Write-Host "==> assembling $OutDir"
if (Test-Path $OutDir) { Remove-Item -Recurse -Force $OutDir }
New-Item -ItemType Directory -Path $OutDir | Out-Null

Copy-Item $BinSrc (Join-Path $OutDir "reach.exe")
Copy-Item -Recurse (Join-Path $Root "games/reach/levels") (Join-Path $OutDir "levels")
Copy-Item -Recurse (Join-Path $Root "games/reach/assets") (Join-Path $OutDir "assets")

$Readme = @"
Reach (Kerabit)
===============

1. Unzip this folder anywhere.
2. Double-click reach.exe (keep levels/ and assets/ next to it).
3. Controls: Space start/next · WASD move · R retry · Escape quit.

Built from the Kerabit engine — https://kerabitengine.vercel.app
"@
Set-Content -Path (Join-Path $OutDir "README.txt") -Value $Readme -Encoding UTF8

Write-Host "==> zipping $Zip"
if (Test-Path $Zip) { Remove-Item -Force $Zip }
Compress-Archive -Path $OutDir -DestinationPath $Zip -Force

Write-Host ""
Write-Host "Done."
Write-Host "  Folder: $OutDir"
Write-Host "  Zip:    $Zip"
Write-Host "Unzip Reach-windows.zip and run reach.exe."
