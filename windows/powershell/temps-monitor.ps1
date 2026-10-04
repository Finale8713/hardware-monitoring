#Requires -Version 5.1
<#
.SYNOPSIS
  Persistent CPU/GPU hardware monitor with CSV logging.

.DESCRIPTION
  - GPUs: nvidia-smi (temp, util, VRAM, clocks, power)
  - CPU:  LibreHardwareMonitor WMI when LHM is running (recommended)
          Falls back to ACPI thermal zones (often wrong - logged as Source=ACPI)
  - System: RAM, disks (capacity + read/write IOPS), network throughput, battery

  Disk I/O and network throughput use Windows performance counters (instant rates).

  Temperatures are shown and logged in Fahrenheit. Sensors are read in Celsius
  from hardware APIs and converted automatically.

.PARAMETER IntervalSeconds
  Sample interval (default 10).

.PARAMETER LogPath
  CSV log file (default: C:\Logs\hardware-temps.csv).

.PARAMETER ShowConsole
  Live console dashboard. Omit for log-only background use.

.PARAMETER Live
  Fast refresh with full-screen redraw each tick (default with -ShowConsole). Use -IntervalSeconds 1 for ~1s updates.

.PARAMETER LogIntervalSeconds
  CSV log interval when -Live (default 10). Display still uses -IntervalSeconds.

.PARAMETER Once
  Sample once, log, optionally show dashboard, then exit.

.PARAMETER InstallLhm
  Install LibreHardwareMonitor via winget if missing (see also install-lhm.ps1).

.EXAMPLE
  .\start-monitor.ps1 -ShowConsole

.EXAMPLE
  .\temps-monitor.ps1 -ShowConsole -Live -IntervalSeconds 1

.EXAMPLE
  .\temps-monitor.ps1 -ShowConsole -Once
#>
[CmdletBinding()]
param(
    [int] $IntervalSeconds = 10,
    [int] $LogIntervalSeconds = 0,
    [string] $LogPath = 'C:\Logs\hardware-temps.csv',
    [switch] $ShowConsole,
    [switch] $Live,
    [switch] $Once,
    [switch] $InstallLhm,
    [double] $GpuWarnF = 181,
    [double] $GpuCritF = 194,
    [double] $CpuWarnF = 176,
    [double] $CpuCritF = 194
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'

function Convert-ToFahrenheit {
    param([double]$Celsius)
    [math]::Round(($Celsius * 9 / 5) + 32, 1)
}

$lhmReaderPath = Join-Path $PSScriptRoot 'lib\LhmReader.ps1'
if (-not (Test-Path $lhmReaderPath)) {
    Write-Error "Missing required file: $lhmReaderPath"
    exit 1
}
. $lhmReaderPath

function Ensure-LogDirectory {
    $dir = Split-Path -Parent $LogPath
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
    if (-not (Test-Path $LogPath)) {
        'Timestamp,Source,Device,SensorF,UtilPct,PowerW,MemUtilPct,Alert' | Out-File -FilePath $LogPath -Encoding utf8
    }
}

function Get-LhmSensors {
    Get-CimInstance -Namespace 'root/LibreHardwareMonitor' -ClassName Sensor -ErrorAction SilentlyContinue
}

function Get-CpuTempsFromLhm {
    $sensors = Get-LhmSensors
    if (-not $sensors) { return @() }

    $sensors |
        Where-Object { $_.SensorType -eq 3 -and $_.Value -gt 0 } |
        Where-Object {
            $_.Name -match 'CPU Package|Core \(Tctl/Tdie\)|Core Average|Tctl|Package'
        } |
        Select-Object -First 3 |
        ForEach-Object {
            $f = Convert-ToFahrenheit ([double]$_.Value)
            [pscustomobject]@{
                Source     = 'LHM'
                Device     = $_.Name
                SensorF    = $f
                UtilPct    = $null
                PowerW     = $null
                MemUtilPct = $null
            }
        }
}

function Get-CpuTempsFromAcpi {
    Get-CimInstance MSAcpi_ThermalZoneTemperature -Namespace root/wmi -ErrorAction SilentlyContinue |
        ForEach-Object {
            $c = ($_.CurrentTemperature / 10) - 273.15
            if ($c -lt -40 -or $c -gt 125) { return }
            [pscustomobject]@{
                Source     = 'ACPI'
                Device     = ($_.InstanceName -replace '.*\\', '')
                SensorF    = Convert-ToFahrenheit $c
                UtilPct    = $null
                PowerW     = $null
                MemUtilPct = $null
            }
        }
}

function Get-CpuTemps {
    $lib = @(Get-CpuTempsFromLhmLib)
    if ($lib.Count -gt 0) { return $lib }
    $wmi = @(Get-CpuTempsFromLhm)
    if ($wmi.Count -gt 0) { return $wmi }
    if (Get-LhmInstallPath) { return @() }
    return @(Get-CpuTempsFromAcpi)
}

function Get-CpuClockMhz {
    $mhz = Get-CpuClockMhzFromLhmLib
    if ($mhz) { return $mhz }

    $sensors = Get-LhmSensors
    if (-not $sensors) { return $null }

    $clock = $sensors |
        Where-Object { $_.SensorType -eq 1 -and $_.Value -gt 0 } |
        Where-Object { $_.Name -match 'CPU Core|Core #0|Core \(Tctl/Tdie\)|Bus Speed' } |
        Select-Object -First 1

    if ($clock) { return [math]::Round([double]$clock.Value, 0) }
    return $null
}

function Get-CpuUtilPercent {
    try {
        $sample = Get-Counter '\Processor(_Total)\% Processor Utility' -ErrorAction Stop
        return [math]::Round($sample.CounterSamples[0].CookedValue, 1)
    } catch {
        $sample = Get-Counter '\Processor(_Total)\% Processor Time' -ErrorAction SilentlyContinue
        if ($sample) { return [math]::Round($sample.CounterSamples[0].CookedValue, 1) }
        return $null
    }
}

function Get-CpuUtilReading {
    $pct = Get-CpuUtilPercent
    if ($null -eq $pct) { return $null }
    [pscustomobject]@{
        Source     = 'perf-counter'
        Device     = 'CPU Total'
        SensorF    = $null
        UtilPct    = $pct
        PowerW     = $null
        MemUtilPct = $null
    }
}

function Get-GpuReadings {
    # Note: utilization.memory often reads 100% on laptop GPUs (memory bus activity, not VRAM fill).
    $raw = & nvidia-smi --query-gpu=index,name,temperature.gpu,utilization.gpu,memory.used,memory.total,power.draw,clocks.current.graphics,clocks.current.memory `
        --format=csv,noheader,nounits 2>$null
    if (-not $raw) { return @() }

    foreach ($line in $raw) {
        $p = ($line -split ',').ForEach({ $_.Trim().Trim('"') })
        if ($p.Count -lt 5) { continue }
        $vramUsed = if ($p.Count -ge 6 -and $p[4] -match '^\d') { [double]$p[4] } else { $null }
        $vramTotal = if ($p.Count -ge 7 -and $p[5] -match '^\d') { [double]$p[5] } else { $null }
        $vramPct = if ($vramUsed -and $vramTotal -gt 0) { [math]::Round(100 * $vramUsed / $vramTotal, 1) } else { $null }
        [pscustomobject]@{
            Source       = 'nvidia-smi'
            Device       = "GPU$($p[0]) $($p[1])"
            SensorF      = Convert-ToFahrenheit ([double]$p[2])
            UtilPct      = [double]$p[3]
            MemUtilPct   = $vramPct
            VramUsedMiB  = $vramUsed
            VramTotalMiB = $vramTotal
            PowerW       = if ($p.Count -ge 8 -and $p[6] -match '^\d') { [double]$p[6] } else { $null }
            GfxClockMhz  = if ($p.Count -ge 9 -and $p[7] -match '^\d') { [double]$p[7] } else { $null }
            MemClockMhz  = if ($p.Count -ge 10 -and $p[8] -match '^\d') { [double]$p[8] } else { $null }
        }
    }
}

function Get-MemoryStats {
    $os = Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue
    if (-not $os) { return $null }

    $totalBytes = [double]$os.TotalVisibleMemorySize * 1KB
    $freeBytes = [double]$os.FreePhysicalMemory * 1KB
    $usedBytes = $totalBytes - $freeBytes

    [pscustomobject]@{
        UsedGB  = [math]::Round($usedBytes / 1GB, 1)
        TotalGB = [math]::Round($totalBytes / 1GB, 1)
        FreeGB  = [math]::Round($freeBytes / 1GB, 1)
        UsedPct = if ($totalBytes -gt 0) { [math]::Round(100 * $usedBytes / $totalBytes, 1) } else { 0 }
    }
}

function Format-BytesPerSec {
    param([double]$BytesPerSec)
    if ($BytesPerSec -ge 1MB) { return '{0:N1} MB/s' -f ($BytesPerSec / 1MB) }
    if ($BytesPerSec -ge 1KB) { return '{0:N0} KB/s' -f ($BytesPerSec / 1KB) }
    return '{0:N0} B/s' -f [math]::Round($BytesPerSec, 0)
}

function Get-DiskCapacityStats {
    Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' -ErrorAction SilentlyContinue |
        ForEach-Object {
            if (-not $_.Size) { return }
            $used = $_.Size - $_.FreeSpace
            [pscustomobject]@{
                Drive   = $_.DeviceID
                UsedGB  = [math]::Round($used / 1GB, 1)
                TotalGB = [math]::Round($_.Size / 1GB, 1)
                FreeGB  = [math]::Round($_.FreeSpace / 1GB, 1)
                UsedPct = [math]::Round(100 * $used / $_.Size, 1)
            }
        }
}

function Get-DiskIoStats {
    param([string[]]$Drives)
    if (-not $Drives -or $Drives.Count -eq 0) { return @{} }

    $paths = [System.Collections.Generic.List[string]]::new()
    foreach ($drive in $Drives) {
        $paths.Add("\LogicalDisk($drive)\Disk Reads/sec")
        $paths.Add("\LogicalDisk($drive)\Disk Writes/sec")
        $paths.Add("\LogicalDisk($drive)\Disk Read Bytes/sec")
        $paths.Add("\LogicalDisk($drive)\Disk Write Bytes/sec")
    }

    $ioMap = @{}
    foreach ($drive in $Drives) {
        $ioMap[$drive] = [pscustomobject]@{
            ReadOpsSec    = 0
            WriteOpsSec   = 0
            ReadBytesSec  = 0
            WriteBytesSec = 0
        }
    }

    $sample = Get-Counter -Counter $paths.ToArray() -ErrorAction SilentlyContinue
    if (-not $sample) { return $ioMap }

    foreach ($s in $sample.CounterSamples) {
        if ($s.Path -match 'LogicalDisk\(([A-Z]:)\)\\Disk (Reads|Writes)/sec') {
            $drive = $Matches[1]
            if (-not $ioMap.ContainsKey($drive)) { continue }
            if ($Matches[2] -eq 'Reads') { $ioMap[$drive].ReadOpsSec = [math]::Round($s.CookedValue, 1) }
            else { $ioMap[$drive].WriteOpsSec = [math]::Round($s.CookedValue, 1) }
        }
        elseif ($s.Path -match 'LogicalDisk\(([A-Z]:)\)\\Disk (Read|Write) Bytes/sec') {
            $drive = $Matches[1]
            if (-not $ioMap.ContainsKey($drive)) { continue }
            if ($Matches[2] -eq 'Read') { $ioMap[$drive].ReadBytesSec = $s.CookedValue }
            else { $ioMap[$drive].WriteBytesSec = $s.CookedValue }
        }
    }
    $ioMap
}

function Get-DiskStats {
    $disks = @(Get-DiskCapacityStats)
    $driveLetters = @($disks | ForEach-Object { $_.Drive })
    $ioMap = Get-DiskIoStats $driveLetters

    foreach ($d in $disks) {
        if ($ioMap.ContainsKey($d.Drive)) {
            $i = $ioMap[$d.Drive]
            $d | Add-Member -NotePropertyName ReadOpsSec -NotePropertyValue $i.ReadOpsSec -Force
            $d | Add-Member -NotePropertyName WriteOpsSec -NotePropertyValue $i.WriteOpsSec -Force
            $d | Add-Member -NotePropertyName ReadBytesSec -NotePropertyValue $i.ReadBytesSec -Force
            $d | Add-Member -NotePropertyName WriteBytesSec -NotePropertyValue $i.WriteBytesSec -Force
        } else {
            $d | Add-Member -NotePropertyName ReadOpsSec -NotePropertyValue $null -Force
            $d | Add-Member -NotePropertyName WriteOpsSec -NotePropertyValue $null -Force
            $d | Add-Member -NotePropertyName ReadBytesSec -NotePropertyValue $null -Force
            $d | Add-Member -NotePropertyName WriteBytesSec -NotePropertyValue $null -Force
        }
    }
    $disks
}

function Test-NetworkCounterInstance {
    param([string]$Name)
    if ($Name -eq '_Total') { return $false }
    $skip = '(?i)(loopback|isatap|teredo|qos|filter|bluetooth|kernel|virtualbox|vmware|hyper-v|veth|npcap|tailscale|miniport|wan miniport|ras async|ppoe)'
    $Name -notmatch $skip
}

function Get-NetworkStats {
    $adapters = @(Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' -and -not $_.Virtual })
    $sample = Get-Counter '\Network Interface(*)\Bytes Received/sec', '\Network Interface(*)\Bytes Sent/sec' -ErrorAction SilentlyContinue
    if (-not $sample) { return @() }

    $rx = @{}
    $tx = @{}
    foreach ($s in $sample.CounterSamples) {
        if ($s.Path -notmatch 'Network Interface\((.+)\)\\Bytes (Received|Sent)/sec') { continue }
        $instance = $Matches[1]
        if (-not (Test-NetworkCounterInstance $instance)) { continue }
        if ($Matches[2] -eq 'Received') { $rx[$instance] = $s.CookedValue }
        else { $tx[$instance] = $s.CookedValue }
    }

    foreach ($adapter in ($adapters | Sort-Object Name)) {
        $instance = $adapter.InterfaceDescription
        if (-not $rx.ContainsKey($instance)) { continue }

        $ip = Get-NetIPAddress -InterfaceIndex $adapter.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue |
            Where-Object { $_.IPAddress -notlike '169.254*' } |
            Select-Object -First 1 -ExpandProperty IPAddress

        [pscustomobject]@{
            Name         = $adapter.Name
            Instance     = $instance
            IPv4         = $ip
            DownBytesSec = $rx[$instance]
            UpBytesSec   = $tx[$instance]
        }
    }
}

function Get-BatteryStats {
    $bat = Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $bat) { return $null }

    $status = switch ($bat.BatteryStatus) {
        1 { 'discharging' }
        2 { 'AC power' }
        3 { 'fully charged' }
        4 { 'low' }
        5 { 'critical' }
        6 { 'charging' }
        7 { 'charging (high)' }
        8 { 'charging (low)' }
        9 { 'charging (critical)' }
        default { 'unknown' }
    }

    [pscustomobject]@{
        ChargePct = $bat.EstimatedChargeRemaining
        Status    = $status
        Minutes   = if ($bat.EstimatedRunTime -and $bat.EstimatedRunTime -lt 65535) { $bat.EstimatedRunTime } else { $null }
    }
}

function Get-SystemStats {
    [pscustomobject]@{
        Memory      = Get-MemoryStats
        Disks       = @(Get-DiskStats)
        Network     = @(Get-NetworkStats)
        Battery     = Get-BatteryStats
        CpuClockMhz = Get-CpuClockMhz
    }
}

function Get-AlertLevel {
    param([double]$TempF, [bool]$IsGpu)
    if ($null -eq $TempF) { return '' }
    $warn = if ($IsGpu) { $GpuWarnF } else { $CpuWarnF }
    $crit = if ($IsGpu) { $GpuCritF } else { $CpuCritF }
    if ($TempF -ge $crit) { 'CRIT' }
    elseif ($TempF -ge $warn) { 'WARN' }
    else { '' }
}

function Write-LogRow {
    param($Reading, [string]$Alert)
    $ts = (Get-Date).ToString('o')
    $memUtil = ''
    if ($Reading.PSObject.Properties.Name -contains 'MemUtilPct' -and $null -ne $Reading.MemUtilPct) {
        $memUtil = $Reading.MemUtilPct
    }
    $line = "{0},{1},{2},{3},{4},{5},{6},{7}" -f (
        $ts,
        $Reading.Source,
        ($Reading.Device -replace ',', ';'),
        $(if ($null -ne $Reading.SensorF) { $Reading.SensorF } else { '' }),
        $(if ($null -ne $Reading.UtilPct) { $Reading.UtilPct } else { '' }),
        $(if ($null -ne $Reading.PowerW) { $Reading.PowerW } else { '' }),
        $memUtil,
        $Alert
    )
    Add-Content -Path $LogPath -Value $line -Encoding utf8
}

function Write-SystemLogRows {
    param($System)
    if ($System.Memory) {
        Write-LogRow ([pscustomobject]@{
            Source = 'system'; Device = 'RAM'; SensorF = $null
            UtilPct = $System.Memory.UsedPct; PowerW = $System.Memory.UsedGB; MemUtilPct = $System.Memory.TotalGB
        }) ''
    }
    foreach ($d in $System.Disks) {
        Write-LogRow ([pscustomobject]@{
            Source = 'system'; Device = "Disk $($d.Drive)"; SensorF = $null
            UtilPct = $d.UsedPct; PowerW = $d.UsedGB; MemUtilPct = $d.TotalGB
        }) ''
        if ($null -ne $d.ReadOpsSec) {
            Write-LogRow ([pscustomobject]@{
                Source = 'system'; Device = "Disk $($d.Drive) I/O"; SensorF = $null
                UtilPct = $d.ReadOpsSec; PowerW = $d.WriteOpsSec; MemUtilPct = $null
            }) ''
        }
    }
    foreach ($n in $System.Network) {
        Write-LogRow ([pscustomobject]@{
            Source = 'system'; Device = "Net $($n.Name)"; SensorF = $null
            UtilPct = [math]::Round($n.DownBytesSec / 1KB, 1); PowerW = [math]::Round($n.UpBytesSec / 1KB, 1); MemUtilPct = $null
        }) ''
    }
    if ($System.Battery) {
        Write-LogRow ([pscustomobject]@{
            Source = 'system'; Device = 'Battery'; SensorF = $null
            UtilPct = $System.Battery.ChargePct; PowerW = $null; MemUtilPct = $null
        }) ''
    }
}

function Get-DashboardColor {
    param([string]$Name)
    switch ($Name) {
        'yellow' { [ConsoleColor]::Yellow }
        'cyan' { [ConsoleColor]::Cyan }
        'green' { [ConsoleColor]::Green }
        'red' { [ConsoleColor]::Red }
        'darkYellow' { [ConsoleColor]::Yellow }
        'darkGray' { [ConsoleColor]::DarkGray }
        'darkCyan' { [ConsoleColor]::DarkCyan }
        default { [ConsoleColor]::Gray }
    }
}

function Add-DashLine {
    param($List, [string]$Text, [string]$ColorName = 'gray')
    $null = $List.Add([pscustomobject]@{
        Text  = $Text
        Color = (Get-DashboardColor $ColorName)
    })
}

function Build-DashboardLines {
    param($Readings, [datetime]$Started, $CpuUtilReading, $System)
    $lines = [System.Collections.Generic.List[object]]::new()
    $cpuTemps = $Readings | Where-Object { $_.Source -ne 'nvidia-smi' -and $null -ne $_.SensorF }
    $gpu = $Readings | Where-Object { $_.Source -eq 'nvidia-smi' }

    $intervalNote = if ($Once) { 'single sample' }
        elseif ($script:LiveMode) { "refresh ${IntervalSeconds}s  |  log every ${script:LogEverySeconds}s  |  Ctrl+C to stop" }
        else { "interval ${IntervalSeconds}s  |  Ctrl+C to stop" }

    Add-DashLine $lines "Hardware monitor  |  started $($Started.ToString('yyyy-MM-dd HH:mm:ss'))  |  log: $LogPath" 'cyan'
    Add-DashLine $lines $intervalNote 'darkGray'
    Add-DashLine $lines '' 'gray'

    Add-DashLine $lines 'CPU' 'yellow'
    if ($CpuUtilReading) {
        Add-DashLine $lines ("  utilization {0,5}%" -f $CpuUtilReading.UtilPct) 'cyan'
    }
    if ($System.CpuClockMhz) {
        Add-DashLine $lines ("  clock       {0,5} MHz" -f $System.CpuClockMhz) 'cyan'
    }
    if ($cpuTemps) {
        foreach ($t in $cpuTemps) {
            $a = Get-AlertLevel $t.SensorF $false
            $c = if ($a -eq 'CRIT') { 'red' } elseif ($a -eq 'WARN') { 'yellow' } else { 'green' }
            Add-DashLine $lines ("  {0,-28} {1,6} F  [{2}]  ({3})" -f $t.Device, $t.SensorF, $a, $t.Source) $c
        }
    } else {
        $msg = if (-not (Get-LhmInstallPath)) { '  (no CPU temp - run .\install-lhm.ps1)' }
            elseif (-not (Test-MonitorAdmin)) { '  (no CPU temp - run .\start-monitor.ps1 as Admin)' }
            else { '  (no CPU temp - LHM installed but no sensor data)' }
        Add-DashLine $lines $msg 'darkYellow'
    }

    Add-DashLine $lines '' 'gray'
    Add-DashLine $lines 'GPU' 'yellow'
    if ($gpu) {
        foreach ($g in $gpu) {
            $a = Get-AlertLevel $g.SensorF $true
            $c = if ($a -eq 'CRIT') { 'red' } elseif ($a -eq 'WARN') { 'yellow' } else { 'green' }
            Add-DashLine $lines ("  {0}  {1} F  [{2}]" -f $g.Device, $g.SensorF, $a) $c
            $parts = @()
            if ($null -ne $g.UtilPct) { $parts += "compute $($g.UtilPct)%" }
            if ($null -ne $g.MemUtilPct -and $null -ne $g.VramUsedMiB -and $null -ne $g.VramTotalMiB) {
                $parts += "vram $($g.MemUtilPct)% ($([math]::Round($g.VramUsedMiB))/$([math]::Round($g.VramTotalMiB)) MiB)"
            }
            if ($null -ne $g.GfxClockMhz) { $parts += "core $([math]::Round($g.GfxClockMhz)) MHz" }
            if ($null -ne $g.MemClockMhz) { $parts += "memclk $([math]::Round($g.MemClockMhz)) MHz" }
            if ($null -ne $g.PowerW) { $parts += "power $($g.PowerW) W" }
            if ($parts.Count) { Add-DashLine $lines ("       " + ($parts -join '  ')) 'darkCyan' }
        }
    } else {
        Add-DashLine $lines '  (no GPU data - check nvidia-smi / drivers)' 'darkYellow'
    }

    Add-DashLine $lines '' 'gray'
    Add-DashLine $lines 'MEMORY' 'yellow'
    if ($System.Memory) {
        $m = $System.Memory
        Add-DashLine $lines ("  {0}/{1} GB used ({2}%)  |  {3} GB free" -f $m.UsedGB, $m.TotalGB, $m.UsedPct, $m.FreeGB) 'cyan'
    } else {
        Add-DashLine $lines '  (unavailable)' 'darkYellow'
    }

    Add-DashLine $lines '' 'gray'
    Add-DashLine $lines 'STORAGE' 'yellow'
    if ($System.Disks.Count) {
        foreach ($d in $System.Disks) {
            Add-DashLine $lines ("  {0}  {1}/{2} GB used ({3}%)  |  {4} GB free" -f $d.Drive, $d.UsedGB, $d.TotalGB, $d.UsedPct, $d.FreeGB) 'cyan'
            if ($null -ne $d.ReadOpsSec) {
                $io = "read {0} ops/s ({1})  |  write {2} ops/s ({3})" -f (
                    $d.ReadOpsSec, (Format-BytesPerSec $d.ReadBytesSec), $d.WriteOpsSec, (Format-BytesPerSec $d.WriteBytesSec))
                Add-DashLine $lines "       $io" 'darkCyan'
            }
        }
    } else {
        Add-DashLine $lines '  (no fixed disks found)' 'darkYellow'
    }

    Add-DashLine $lines '' 'gray'
    Add-DashLine $lines 'NETWORK' 'yellow'
    if ($System.Network.Count) {
        foreach ($n in $System.Network) {
            $ip = if ($n.IPv4) { "  |  $($n.IPv4)" } else { '' }
            Add-DashLine $lines ("  {0,-20} down {1}  |  up {2}{3}" -f $n.Name, (Format-BytesPerSec $n.DownBytesSec), (Format-BytesPerSec $n.UpBytesSec), $ip) 'cyan'
        }
    } else {
        Add-DashLine $lines '  (no active adapters found)' 'darkYellow'
    }

    if ($System.Battery) {
        Add-DashLine $lines '' 'gray'
        Add-DashLine $lines 'POWER' 'yellow'
        $b = $System.Battery
        $mins = if ($null -ne $b.Minutes) { "  |  ~$($b.Minutes) min remaining" } else { '' }
        Add-DashLine $lines ("  battery {0}%  |  {1}{2}" -f $b.ChargePct, $b.Status, $mins) 'cyan'
    }

    Add-DashLine $lines '' 'gray'
    Add-DashLine $lines "updated $(Get-Date -Format 'HH:mm:ss')" 'darkGray'
    return $lines.ToArray()
}

function Write-Dashboard {
    param($Lines, [switch]$ClearFirst)
    if ($ClearFirst) {
        try { [Console]::Clear() } catch { Clear-Host }
    }
    foreach ($line in $Lines) {
        if ($line.Text -eq '') { Write-Host '' } else { Write-Host $line.Text -ForegroundColor $line.Color }
    }
}

function Show-Dashboard {
    param($Readings, [datetime]$Started, $CpuUtilReading, $System)
    if (-not $ShowConsole) { return }

    $lines = Build-DashboardLines $Readings $Started $CpuUtilReading $System
    Write-Dashboard $lines -ClearFirst:($script:LiveMode -or $script:DashboardSampleCount -gt 0)
    $script:DashboardSampleCount++
}

function Invoke-Sample {
    param([datetime]$Started)
    try {
        $system = Get-SystemStats
        $readings = @()
        $cpuUtil = Get-CpuUtilReading
        if ($cpuUtil) { $readings += $cpuUtil }
        $readings += Get-CpuTemps
        $readings += Get-GpuReadings

        $now = Get-Date
        $shouldLog = $Once -or -not $script:LiveMode -or (-not $script:LastLogTime) -or
            (($now - $script:LastLogTime).TotalSeconds -ge $script:LogEverySeconds)
        if ($shouldLog) {
            foreach ($r in $readings) {
                $isGpu = $r.Source -eq 'nvidia-smi'
                $alert = if ($null -ne $r.SensorF) { Get-AlertLevel $r.SensorF $isGpu } else { '' }
                Write-LogRow $r $alert
                if ($alert -eq 'CRIT') {
                    Write-Warning "[$(Get-Date -Format 'HH:mm:ss')] $($r.Device) $($r.SensorF) F"
                }
            }
            Write-SystemLogRows $system
            $script:LastLogTime = $now
        }

        Show-Dashboard $readings $Started $cpuUtil $system
    } catch {
        Write-Host "Sample error: $($_.Exception.Message)" -ForegroundColor Red
        if ($ShowConsole) {
            Write-Host $_.ScriptStackTrace -ForegroundColor DarkGray
        }
    }
}

# --- main ---
if ($InstallLhm) { Install-LhmIfMissing | Out-Null }
if (-not (Get-LhmInstallPath)) {
    Write-Verbose 'LibreHardwareMonitor not found. CPU temps unavailable until install-lhm.ps1 is run.'
} elseif (-not (Test-MonitorAdmin)) {
    Write-Verbose 'Not running as Administrator; CPU temps via LibreHardwareMonitor require elevation.'
}

if ($ShowConsole -and -not $Once -and -not $PSBoundParameters.ContainsKey('Live')) {
    $Live = $true
}
if ($Live -and -not $PSBoundParameters.ContainsKey('IntervalSeconds')) {
    $IntervalSeconds = 1
}
if ($LogIntervalSeconds -le 0) {
    $LogIntervalSeconds = if ($Live) { [Math]::Max($IntervalSeconds, 10) } else { $IntervalSeconds }
}

$script:LiveMode = [bool]$Live
$script:LogEverySeconds = $LogIntervalSeconds
$script:LastLogTime = $null

Ensure-LogDirectory
$started = Get-Date
$script:DashboardSampleCount = 0
Write-Verbose "Logging to $LogPath every ${LogIntervalSeconds}s, refresh ${IntervalSeconds}s"

if (-not $ShowConsole) {
    Write-Host "Hardware monitor logging to $LogPath every ${IntervalSeconds}s (no dashboard)." -ForegroundColor Cyan
    Write-Host 'Add -ShowConsole to see the live dashboard. Press Ctrl+C to stop.' -ForegroundColor DarkGray
}

do {
    Invoke-Sample $started
    if ($Once) { break }
    Start-Sleep -Seconds $IntervalSeconds
} while ($true)

exit 0
