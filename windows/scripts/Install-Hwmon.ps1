#Requires -Version 5.1
<#
.SYNOPSIS
  Copy the portable hwmon folder to %LOCALAPPDATA%\Programs\hwmon and add to user PATH.
#>
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Source = if ($PSScriptRoot -match '\\dist\\hwmon$') {
    $PSScriptRoot
} else {
    Join-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) 'hwmon\dist\hwmon'
}

if (-not (Test-Path (Join-Path $Source 'hwmon.exe'))) {
    Write-Host "Run package.ps1 first. Expected: $Source\hwmon.exe" -ForegroundColor Red
    exit 1
}

$Target = Join-Path $env:LOCALAPPDATA 'Programs\hwmon'
if (Test-Path $Target) {
    Remove-Item -LiteralPath $Target -Recurse -Force
}
New-Item -ItemType Directory -Path $Target -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $Source '*') -Destination $Target -Recurse -Force

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike "*$Target*") {
    $newPath = if ([string]::IsNullOrWhiteSpace($userPath)) { $Target } else { "$userPath;$Target" }
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    $env:Path = "$env:Path;$Target"
    Write-Host 'Added to user PATH (open a new terminal for hwmon / Run-Hwmon).' -ForegroundColor Green
}

Write-Host "Installed to: $Target" -ForegroundColor Green
Write-Host 'Run:  Run-Hwmon   or   hwmon.exe' -ForegroundColor Cyan
