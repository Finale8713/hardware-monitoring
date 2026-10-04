#Requires -Version 5.1
<#
.SYNOPSIS
  Start LibreHardwareMonitor from its full install folder (fixes missing Aga.Controls.dll).
#>
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib\LhmReader.ps1')

$dir = Get-LhmInstallPath
if (-not $dir) {
    Write-Host 'LibreHardwareMonitor not found. Run .\install-lhm.ps1 first.' -ForegroundColor Red
    exit 1
}

$exe = Join-Path $dir 'LibreHardwareMonitor.exe'
if (-not (Test-Path $exe)) {
    Write-Host "Missing: $exe" -ForegroundColor Red
    exit 1
}

if (-not (Test-Path (Join-Path $dir 'Aga.Controls.dll'))) {
    Write-Host 'Install looks broken (Aga.Controls.dll missing). Re-run: .\install-lhm.ps1' -ForegroundColor Red
    exit 1
}

Write-Host "Starting LibreHardwareMonitor from:`n  $dir" -ForegroundColor Cyan
Start-Process -FilePath $exe -WorkingDirectory $dir
Write-Host ''
Write-Host 'For Rust hwmon CPU temps, in LHM enable:' -ForegroundColor Yellow
Write-Host '  Options -> Remote Web Server -> Run (port 8085)' -ForegroundColor White
Write-Host '  Then open http://127.0.0.1:8085/data.json in a browser to verify' -ForegroundColor DarkGray
