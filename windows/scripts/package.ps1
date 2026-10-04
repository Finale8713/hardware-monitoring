#Requires -Version 5.1
<#
.SYNOPSIS
  Build hwmon.exe and assemble a portable folder you can copy anywhere.

.EXAMPLE
  .\package.ps1
  .\package.ps1 -Install
#>
[CmdletBinding()]
param(
    [switch]$Install
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$WindowsDir = Split-Path $PSScriptRoot -Parent   # windows\
$RepoRoot = Split-Path $WindowsDir -Parent        # repo root
$RustDir = Join-Path $RepoRoot 'hwmon'
$PsDir = Join-Path $WindowsDir 'powershell'
$OutDir = Join-Path $RustDir 'dist\hwmon'

Push-Location $RustDir
try {
    Write-Host 'Building release binary...' -ForegroundColor Cyan
    cargo build --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    Pop-Location
}

$ExeSrc = Join-Path $RustDir 'target\release\hwmon.exe'
if (-not (Test-Path $ExeSrc)) {
    throw "Missing build output: $ExeSrc"
}

if (Test-Path $OutDir) {
    Remove-Item -LiteralPath $OutDir -Recurse -Force
}
$ScriptsOut = Join-Path $OutDir 'scripts'
$LibOut = Join-Path $ScriptsOut 'lib'
New-Item -ItemType Directory -Path $LibOut -Force | Out-Null

Copy-Item -LiteralPath $ExeSrc -Destination (Join-Path $OutDir 'hwmon.exe')

$toCopy = @(
    @{ Src = Join-Path $PsDir 'Get-CpuTempsJson.ps1'; Dst = Join-Path $ScriptsOut 'Get-CpuTempsJson.ps1' }
    @{ Src = Join-Path $PsDir 'Start-LibreHardwareMonitor.ps1'; Dst = Join-Path $ScriptsOut 'Start-LibreHardwareMonitor.ps1' }
    @{ Src = Join-Path $PsDir 'install-lhm.ps1'; Dst = Join-Path $ScriptsOut 'install-lhm.ps1' }
    @{ Src = Join-Path $PsDir 'lib\LhmReader.ps1'; Dst = Join-Path $LibOut 'LhmReader.ps1' }
)
foreach ($item in $toCopy) {
    if (-not (Test-Path $item.Src)) { throw "Missing: $($item.Src)" }
    Copy-Item -LiteralPath $item.Src -Destination $item.Dst
}

@'
# Portable hwmon bundle — copy this entire folder anywhere (USB, Desktop, etc.)

Run:
  Double-click Run-Hwmon.cmd
  or:  .\Run-Hwmon.ps1

First-time CPU temps:
  1. scripts\install-lhm.ps1   (once, installs LibreHardwareMonitor)
  2. scripts\Start-LibreHardwareMonitor.ps1
  3. In LHM: Options -> Remote Web Server -> Run (port 8085)

Requirements on the PC:
  - Windows 10/11
  - NVIDIA: nvidia-smi on PATH (GPU stats)
  - LibreHardwareMonitor with web server for CPU temps (see above)

Logs default to C:\Logs\hardware-temps.csv
'@ | Set-Content -LiteralPath (Join-Path $OutDir 'README.txt') -Encoding UTF8

# Run-Hwmon.ps1 and .cmd are maintained in scripts/ and copied each package run
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Run-Hwmon.ps1') -Destination (Join-Path $OutDir 'Run-Hwmon.ps1')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Run-Hwmon.cmd') -Destination (Join-Path $OutDir 'Run-Hwmon.cmd')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Install-Hwmon.ps1') -Destination (Join-Path $OutDir 'Install-Hwmon.ps1')

Write-Host ''
Write-Host "Packaged: $OutDir" -ForegroundColor Green
Write-Host '  hwmon.exe' -ForegroundColor DarkGray
Write-Host '  Run-Hwmon.cmd / Run-Hwmon.ps1' -ForegroundColor DarkGray
Write-Host '  scripts\  (LHM helpers, optional admin CPU path)' -ForegroundColor DarkGray

if ($Install) {
    & (Join-Path $OutDir 'Install-Hwmon.ps1')
}
