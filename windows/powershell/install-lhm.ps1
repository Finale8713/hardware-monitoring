#Requires -Version 5.1
<#
.SYNOPSIS
  Install LibreHardwareMonitor (winget) for CPU temperature support.
#>
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib\LhmReader.ps1')

if (Install-LhmIfMissing) {
    $dir = Get-LhmInstallPath
    Write-Host "LibreHardwareMonitor installed at: $dir" -ForegroundColor Green
    Write-Host ''
    Write-Host 'Start LibreHardwareMonitor (use this — not the bare exe name):' -ForegroundColor Cyan
    Write-Host '  .\Start-LibreHardwareMonitor.ps1' -ForegroundColor White
    Write-Host ''
    Write-Host 'The winget shortcut can fail with missing Aga.Controls.dll.' -ForegroundColor DarkGray
    Write-Host 'Run the monitor as Administrator for CPU temps:' -ForegroundColor Cyan
    Write-Host '  .\start-monitor.ps1 -ShowConsole' -ForegroundColor White
} else {
    exit 1
}
