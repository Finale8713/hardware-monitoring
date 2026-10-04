#Requires -Version 5.1
<#
.SYNOPSIS
  Start LibreHardwareMonitor if needed, then launch hwmon TUI.
#>
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$HwmonArgs
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Dir = $PSScriptRoot
$Hwmon = Join-Path $Dir 'hwmon.exe'
$LhmScript = Join-Path $Dir 'scripts\Start-LibreHardwareMonitor.ps1'
$LhmUrl = 'http://127.0.0.1:8085/data.json'

if (-not (Test-Path $Hwmon)) {
    Write-Host "Missing $Hwmon - run windows\scripts\package.ps1 first." -ForegroundColor Red
    exit 1
}

$env:HWMON_HOME = $Dir

function Test-LhmWebServer {
    try {
        Invoke-WebRequest -Uri $LhmUrl -UseBasicParsing -TimeoutSec 2 | Out-Null
        return $true
    } catch {
        return $false
    }
}

if (-not (Test-LhmWebServer)) {
    Write-Host 'LHM web server not reachable on port 8085.' -ForegroundColor Yellow
    if (Test-Path $LhmScript) {
        Write-Host 'Starting LibreHardwareMonitor...' -ForegroundColor Cyan
        & $LhmScript
        Write-Host 'Waiting for web server (enable Options -> Remote Web Server -> Run if needed)...' -ForegroundColor DarkGray
        $deadline = (Get-Date).AddSeconds(20)
        while ((Get-Date) -lt $deadline) {
            Start-Sleep -Seconds 1
            if (Test-LhmWebServer) { break }
        }
    } else {
        Write-Host 'CPU temps need LHM with Remote Web Server enabled.' -ForegroundColor Yellow
    }
}

Set-Location $Dir
& $Hwmon @HwmonArgs
