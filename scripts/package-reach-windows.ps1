# Thin wrapper: Windows Reach zip only.
# Full kit (all games + editor):  pwsh ./scripts/package-windows.ps1
param(
    [switch]$SkipBuild,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
if ($Help) {
    Write-Host "Usage: ./scripts/package-reach-windows.ps1 [-SkipBuild]"
    exit 0
}

& (Join-Path $PSScriptRoot "package-windows.ps1") -Only reach @PSBoundParameters
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
