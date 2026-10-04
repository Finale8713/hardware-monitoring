# LibreHardwareMonitorLib in-process reader (no WMI). Requires admin for CPU sensors on most systems.

$script:LhmReaderReady = $false
$script:LhmInstallPath = $null

function Get-LhmInstallPath {
    $searchRoots = @(
        (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages'),
        ${env:ProgramFiles},
        ${env:ProgramFiles(x86)}
    )
    foreach ($root in $searchRoots) {
        if (-not (Test-Path $root)) { continue }
        $lib = Get-ChildItem -Path $root -Recurse -Filter 'LibreHardwareMonitorLib.dll' -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($lib) { return $lib.DirectoryName }
    }

    $cmd = Get-Command LibreHardwareMonitor.exe -ErrorAction SilentlyContinue
    if ($cmd) {
        $item = Get-Item -LiteralPath $cmd.Source -ErrorAction SilentlyContinue
        if ($item.Target) {
            return (Split-Path -Parent $item.Target[0])
        }
        $dir = Split-Path -Parent $cmd.Source
        if (Test-Path (Join-Path $dir 'LibreHardwareMonitorLib.dll')) { return $dir }
    }
    return $null
}

function Install-LhmIfMissing {
    if (Get-LhmInstallPath) { return $true }
    Write-Host 'Installing LibreHardwareMonitor via winget...' -ForegroundColor Cyan
    $winget = Get-Command winget -ErrorAction SilentlyContinue
    if (-not $winget) {
        Write-Warning 'winget not found. Install LibreHardwareMonitor manually: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/releases'
        return $false
    }
    & winget install --id LibreHardwareMonitor.LibreHardwareMonitor `
        --accept-package-agreements --accept-source-agreements
    return [bool](Get-LhmInstallPath)
}

function Test-MonitorAdmin {
    $principal = New-Object Security.Principal.WindowsPrincipal ([Security.Principal.WindowsIdentity]::GetCurrent())
    $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Initialize-LhmReader {
    if ($script:LhmReaderReady -eq $true) { return $true }

    $script:LhmInstallPath = Get-LhmInstallPath
    if (-not $script:LhmInstallPath) { return $false }

    $libPath = Join-Path $script:LhmInstallPath 'LibreHardwareMonitorLib.dll'
    if (-not (Test-Path $libPath)) { return $false }

    Get-ChildItem -Path $script:LhmInstallPath -Filter '*.dll' |
        Sort-Object { $_.Name -ne 'LibreHardwareMonitorLib.dll' } |
        ForEach-Object {
            try { [void][System.Reflection.Assembly]::LoadFrom($_.FullName) } catch { }
        }

    if (-not ('HwMonitorLhmReader' -as [type])) {
        $src = @'
using System;
using System.Collections.Generic;
using System.Linq;
using LibreHardwareMonitor.Hardware;

public static class HwMonitorLhmReader {
    private static Computer computer;

    private static void EnsureComputer() {
        if (computer != null) { return; }
        computer = new Computer {
            IsCpuEnabled = true,
            IsGpuEnabled = false,
            IsMemoryEnabled = false,
            IsMotherboardEnabled = false,
            IsControllerEnabled = false,
            IsNetworkEnabled = false,
            IsStorageEnabled = false
        };
        computer.Open();
    }

    private static void UpdateAll() {
        EnsureComputer();
        computer.Accept(new UpdateVisitor());
    }

    public static Tuple<string, double>[] GetTemperaturesC() {
        UpdateAll();
        var list = new List<Tuple<string, double>>();
        foreach (var hardware in computer.Hardware) {
            CollectTemps(hardware, list);
            foreach (var sub in hardware.SubHardware) {
                sub.Update();
                CollectTemps(sub, list);
            }
        }
        return list.ToArray();
    }

    public static double? GetCpuClockMhz() {
        UpdateAll();
        foreach (var hardware in computer.Hardware.Where(h => h.HardwareType == HardwareType.Cpu)) {
            foreach (var sensor in hardware.Sensors) {
                if (sensor.SensorType == SensorType.Clock && sensor.Value.HasValue && sensor.Value > 0)
                    return sensor.Value;
            }
            foreach (var sub in hardware.SubHardware) {
                foreach (var sensor in sub.Sensors) {
                    if (sensor.SensorType == SensorType.Clock && sensor.Value.HasValue && sensor.Value > 0)
                        return sensor.Value;
                }
            }
        }
        return null;
    }

    private static void CollectTemps(IHardware hardware, List<Tuple<string, double>> list) {
        foreach (var sensor in hardware.Sensors) {
            if (sensor.SensorType != SensorType.Temperature) { continue; }
            if (!sensor.Value.HasValue || sensor.Value <= 0) { continue; }
            list.Add(Tuple.Create(sensor.Name, (double)sensor.Value.Value));
        }
    }

    private class UpdateVisitor : IVisitor {
        public void VisitComputer(IComputer c) { c.Traverse(this); }
        public void VisitHardware(IHardware h) {
            h.Update();
            foreach (var sub in h.SubHardware) { sub.Accept(this); }
        }
        public void VisitSensor(ISensor s) { }
        public void VisitParameter(IParameter p) { }
    }
}
'@
        Add-Type -TypeDefinition $src -ReferencedAssemblies $libPath -ErrorAction Stop
    }

    $script:LhmReaderReady = $true
    return $true
}

function Test-CpuTempSensorName {
    param([string]$Name)
    $Name -match 'CPU Package|Core \(Tctl/Tdie\)|Core Average|Tctl|Package|CCD|Core Max|Core #'
}

function Get-CpuTempsFromLhmLib {
    if (-not (Initialize-LhmReader)) { return @() }
    try {
        $pairs = [HwMonitorLhmReader]::GetTemperaturesC()
        @(
            $pairs |
                Where-Object { Test-CpuTempSensorName $_.Item1 } |
                Select-Object -First 3 |
                ForEach-Object {
                    [pscustomobject]@{
                        Source     = 'LHM-lib'
                        Device     = $_.Item1
                        SensorF    = (Convert-ToFahrenheit $_.Item2)
                        UtilPct    = $null
                        PowerW     = $null
                        MemUtilPct = $null
                    }
                }
        )
    } catch {
        Write-Verbose "LHM lib read failed: $_"
        return @()
    }
}

function Get-CpuClockMhzFromLhmLib {
    if (-not (Initialize-LhmReader)) { return $null }
    try {
        $mhz = [HwMonitorLhmReader]::GetCpuClockMhz()
        if ($mhz.HasValue) { return [math]::Round($mhz.Value, 0) }
    } catch {
        Write-Verbose "LHM lib clock read failed: $_"
    }
    return $null
}
