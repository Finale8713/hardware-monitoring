#Requires -Version 5.1
<#
.SYNOPSIS
  Launch the hardware monitor (elevated when needed for CPU temps via LibreHardwareMonitor).

.EXAMPLE
  .\start-monitor.ps1 -ShowConsole
#>
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [object[]] $Remaining
)

Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib\LhmReader.ps1')

$monitorScript = Join-Path $PSScriptRoot 'temps-monitor.ps1'
$forward = [System.Collections.Generic.List[string]]::new()
$forward.Add('-NoProfile')
$forward.Add('-ExecutionPolicy')
$forward.Add('Bypass')
$forward.Add('-File')
$forward.Add($monitorScript)
foreach ($arg in $Remaining) { $forward.Add([string]$arg) }

if (-not (Test-MonitorAdmin)) {
    Write-Host 'Requesting Administrator (required for CPU temperature sensors)...' -ForegroundColor Yellow
    Write-Host 'The dashboard opens in a separate elevated window.' -ForegroundColor DarkGray
    Start-Process -FilePath 'powershell.exe' -Verb RunAs -ArgumentList $forward.ToArray() | Out-Null
    exit 0
}

& $monitorScript @Remaining
exit 0
