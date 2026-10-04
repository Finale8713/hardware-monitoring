#Requires -Version 5.1
<#
.SYNOPSIS
  Rebuild hwmon and launch the TUI from dist\hwmon.
.EXAMPLE
  .\Build-And-Run-Hwmon.ps1
#>
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$ScriptsDir = $PSScriptRoot
$RepoRoot = Split-Path (Split-Path $ScriptsDir -Parent) -Parent  # scripts -> windows -> repo
$DistDir = Join-Path $RepoRoot 'hwmon\dist\hwmon'

& (Join-Path $ScriptsDir 'package.ps1')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$launcher = Join-Path $DistDir 'Run-Hwmon.ps1'
if (-not (Test-Path $launcher)) {
    Write-Host "Missing $launcher" -ForegroundColor Red
    exit 1
}

& $launcher @args
