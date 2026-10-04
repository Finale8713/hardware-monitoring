#Requires -Version 5.1
# Outputs CPU temperature sensors as JSON for hwmon (Rust). Run from elevated PowerShell for best results.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\LhmReader.ps1')

if (-not (Initialize-LhmReader)) {
    Write-Error 'LibreHardwareMonitorLib not found or failed to load'
    exit 1
}

$out = [System.Collections.Generic.List[object]]::new()
foreach ($t in [HwMonitorLhmReader]::GetTemperaturesC()) {
    if (Test-CpuTempSensorName $t.Item1) {
        $out.Add([ordered]@{ name = $t.Item1; celsius = [double]$t.Item2; source = 'LHM-lib' })
    }
    if ($out.Count -ge 3) { break }
}

$out | ConvertTo-Json -Compress
