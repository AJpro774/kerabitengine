# Build every Windows player/editor zip from this checkout.
#
# Usage (repo root, PowerShell):
#   pwsh ./scripts/package-windows.ps1
#   pwsh ./scripts/package-windows.ps1 -SkipBuild
#   pwsh ./scripts/package-windows.ps1 -Only reach,editor
#
# Output under dist/:
#   Reach-windows.zip, Surge-windows.zip, Spark-windows.zip,
#   Strike-windows.zip, Showcase-windows.zip, Kerabit-editor-windows.zip
#
# Each game zip is exe + data side by side (see kerabit::packaged_data_root).
# The editor zip includes games/, mods/, examples/scenes/, community/.

param(
    [switch]$SkipBuild,
    [string[]]$Only = @(),
    [switch]$Help
)

$ErrorActionPreference = "Stop"

if ($Help) {
    Write-Host "Usage: ./scripts/package-windows.ps1 [-SkipBuild] [-Only reach,surge,spark,strike,showcase,editor]"
    exit 0
}

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

if ($IsWindows -eq $false -and $env:OS -ne "Windows_NT") {
    Write-Error "This script packages native Windows binaries. Run it on Windows (or in CI windows-latest)."
}

function Want($name) {
    if ($Only.Count -eq 0) { return $true }
    return $Only -contains $name
}

$crates = @()
if (Want "reach") { $crates += "reach" }
if (Want "surge") { $crates += "surge" }
if (Want "spark") { $crates += "spark" }
if (Want "strike") { $crates += "strike" }
if (Want "showcase") { $crates += "showcase" }
if (Want "editor") { $crates += "kerabit-editor" }

if (-not $SkipBuild) {
    foreach ($pkg in $crates) {
        Write-Host "==> cargo build -p $pkg --release"
        cargo build -p $pkg --release
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
}

$metaJson = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
$TargetDir = $metaJson.target_directory
$Release = Join-Path $TargetDir "release"
$Dist = Join-Path $Root "dist"
if (-not (Test-Path $Dist)) { New-Item -ItemType Directory -Path $Dist | Out-Null }

function Find-Bin([string]$name) {
    $exe = Join-Path $Release "$name.exe"
    if (Test-Path $exe) { return $exe }
    $plain = Join-Path $Release $name
    if (Test-Path $plain) { return $plain }
    Write-Error "Missing release binary $name.exe under $Release — run without -SkipBuild"
}

function Zip-Folder([string]$outDir, [string]$zip) {
    if (Test-Path $zip) { Remove-Item -Force $zip }
    Compress-Archive -Path $outDir -DestinationPath $zip -Force
}

function Fresh-Dir([string]$path) {
    if (Test-Path $path) { Remove-Item -Recurse -Force $path }
    New-Item -ItemType Directory -Path $path | Out-Null
}

function Write-Readme([string]$path, [string]$body) {
    Set-Content -Path $path -Value $body -Encoding UTF8
}

if (Want "reach") {
    $out = Join-Path $Dist "Reach-windows"
    Fresh-Dir $out
    Copy-Item (Find-Bin "reach") (Join-Path $out "reach.exe")
    Copy-Item -Recurse (Join-Path $Root "games/reach/levels") (Join-Path $out "levels")
    Copy-Item -Recurse (Join-Path $Root "games/reach/assets") (Join-Path $out "assets")
    Write-Readme (Join-Path $out "README.txt") @"
Reach (Kerabit)
===============

1. Unzip this folder anywhere.
2. Double-click reach.exe (keep levels/ and assets/ next to it).
3. Controls: Space start/next · WASD move · R retry · Escape quit.

Progress: %USERPROFILE%\.kerabit\reach_progress.txt
https://kerabitengine.vercel.app
"@
    Zip-Folder $out (Join-Path $Dist "Reach-windows.zip")
    Write-Host "  $out"
}

if (Want "surge") {
    $out = Join-Path $Dist "Surge-windows"
    Fresh-Dir $out
    Copy-Item (Find-Bin "surge") (Join-Path $out "surge.exe")
    Copy-Item -Recurse (Join-Path $Root "games/surge/levels") (Join-Path $out "levels")
    if (Test-Path (Join-Path $Root "games/surge/assets")) {
        Copy-Item -Recurse (Join-Path $Root "games/surge/assets") (Join-Path $out "assets")
    }
    Write-Readme (Join-Path $out "README.txt") @"
Surge (Kerabit)
===============

Keep levels/ (and assets/ if present) next to surge.exe.
Controls: arrows pick arena · 1/2 mode · Space start · WASD · R retry · Esc.

Best scores: %USERPROFILE%\.kerabit\surge_best.txt
"@
    Zip-Folder $out (Join-Path $Dist "Surge-windows.zip")
    Write-Host "  $out"
}

if (Want "spark") {
    $out = Join-Path $Dist "Spark-windows"
    Fresh-Dir $out
    Copy-Item (Find-Bin "spark") (Join-Path $out "spark.exe")
    Copy-Item -Recurse (Join-Path $Root "games/spark/scenes") (Join-Path $out "scenes")
    Write-Readme (Join-Path $out "README.txt") @"
Spark (Kerabit)
===============

Keep scenes/ next to spark.exe. WASD move, reach the cyan pad, Esc quits.
"@
    Zip-Folder $out (Join-Path $Dist "Spark-windows.zip")
    Write-Host "  $out"
}

if (Want "strike") {
    $out = Join-Path $Dist "Strike-windows"
    Fresh-Dir $out
    Copy-Item (Find-Bin "strike") (Join-Path $out "strike.exe")
    Copy-Item -Recurse (Join-Path $Root "games/strike/scenes") (Join-Path $out "scenes")
    Copy-Item -Recurse (Join-Path $Root "games/strike/assets") (Join-Path $out "assets")
    Write-Readme (Join-Path $out "README.txt") @"
Strike (Kerabit)
===============

Keep scenes/ and assets/ next to strike.exe.
WASD, mouse look, left-click fire, Space jump, R retry, Esc quit.
"@
    Zip-Folder $out (Join-Path $Dist "Strike-windows.zip")
    Write-Host "  $out"
}

if (Want "showcase") {
    $out = Join-Path $Dist "Showcase-windows"
    Fresh-Dir $out
    Copy-Item (Find-Bin "showcase") (Join-Path $out "showcase.exe")
    Write-Readme (Join-Path $out "README.txt") @"
Kerabit Showcase
================

Double-click showcase.exe. Esc quits. No extra data folder.
"@
    Zip-Folder $out (Join-Path $Dist "Showcase-windows.zip")
    Write-Host "  $out"
}

if (Want "editor") {
    $out = Join-Path $Dist "Kerabit-editor-windows"
    Fresh-Dir $out
    Copy-Item (Find-Bin "kerabit-editor") (Join-Path $out "kerabit-editor.exe")
    Copy-Item -Recurse (Join-Path $Root "games") (Join-Path $out "games")
    Copy-Item -Recurse (Join-Path $Root "mods") (Join-Path $out "mods")
    $exScenes = Join-Path $Root "examples/scenes"
    if (Test-Path $exScenes) {
        New-Item -ItemType Directory -Path (Join-Path $out "examples") | Out-Null
        Copy-Item -Recurse $exScenes (Join-Path $out "examples/scenes")
    }
    if (Test-Path (Join-Path $Root "community")) {
        Copy-Item -Recurse (Join-Path $Root "community") (Join-Path $out "community")
    }
    Write-Readme (Join-Path $out "README.txt") @"
Kerabit Editor (Windows)
========================

1. Unzip and keep this folder together (games/, mods/, kerabit-editor.exe).
2. Double-click kerabit-editor.exe (or run it from this folder).
3. File → Open a scene under games/*/levels or games/*/scenes.

Settings and mods prefs: %USERPROFILE%\.kerabit\
Copy your Mac ~/.kerabit folder there to keep snap, mods, and Reach times.

For cargo/MCP authoring, clone the repo and: cargo run -p kerabit-editor
"@
    Zip-Folder $out (Join-Path $Dist "Kerabit-editor-windows.zip")
    Write-Host "  $out"
}

Write-Host ""
Write-Host "Done. Zips in $Dist"
