# Hardware Monitor (PowerShell)

Windows CLI for live CPU/GPU **temperatures (Fahrenheit)**, **utilization**, and **system stats**, with optional CSV logging.

## Quick start

```powershell
cd windows\powershell

.\install-lhm.ps1
.\Start-LibreHardwareMonitor.ps1    # open LHM (fixes Aga.Controls.dll error)
.\start-monitor.ps1 -ShowConsole
.\temps-monitor.ps1 -ShowConsole -IntervalSeconds 2
```

**LibreHardwareMonitor:** Do not use the Start menu / `LibreHardwareMonitor` command alone — winget’s link can miss DLLs. Use `.\Start-LibreHardwareMonitor.ps1` instead.

CPU temperatures require **Administrator** (use `start-monitor.ps1` or an elevated shell). See parameter table in the repo root README history or run `Get-Help .\temps-monitor.ps1 -Full`.

## Files

| File | Purpose |
|------|---------|
| `temps-monitor.ps1` | Main monitor loop |
| `start-monitor.ps1` | Launches monitor elevated for CPU sensors |
| `install-lhm.ps1` | Installs LibreHardwareMonitor via winget |
| `lib/LhmReader.ps1` | Loads LibreHardwareMonitorLib for CPU sensors |
