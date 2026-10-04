# hwmon

Terminal UI hardware monitor (Windows + Linux), using [ratatui](https://github.com/ratatui-org/ratatui).

## Build & run (development)

```powershell
# from the hwmon/ directory
cargo run --release
```

```powershell
# Options
cargo run --release -- --interval 1 --log-every 10
cargo run --release -- --once
cargo run --release -- --celsius
cargo run --release -- --psu-watts 850 --psu-overhead 100
cargo run --release -- --llama-url http://127.0.0.1:8080   # scrape a llama.cpp server
cargo run --release -- --no-llama                          # hide the llama.cpp panel
cargo run --release -- --llama-api-key-file path\to\key.txt # if server uses --api-key
```

Press **q** or **Esc** to quit.

## Package for deployment (Windows, recommended)

Build a **portable folder** you can copy to Desktop, USB, or another PC - no `cargo`, no repo layout required:

```powershell
cd windows\scripts
.\package.ps1
```

Output: `hwmon\dist\hwmon\`

| File | Purpose |
|------|---------|
| `hwmon.exe` | Standalone monitor (single binary) |
| `Run-Hwmon.cmd` | Double-click launcher (starts LHM if needed, then TUI) |
| `Run-Hwmon.ps1` | Same, from PowerShell |
| `scripts\` | LHM install/start helpers (optional admin CPU path) |
| `Install-Hwmon.ps1` | Copy to `%LOCALAPPDATA%\Programs\hwmon` and add to user PATH |

```powershell
# in windows\scripts
.\package.ps1 -Install    # package + install to Programs\hwmon
```

After install, from any new terminal:

```powershell
Run-Hwmon
# or
hwmon.exe --once
```

**What you still need on each PC (Windows):** `nvidia-smi` (GPU), LibreHardwareMonitor with **Remote Web Server → Run** (CPU temps). The launcher tries to start LHM automatically.

### Color thresholds (TUI)

Healthy values render **green**. They turn **yellow** (warn) or **red** (critical) at the limits defined in `src/thresholds.rs`:

| Metric | Warn | Critical |
|--------|------|----------|
| CPU / GPU utilization | 85% | 95% |
| RAM / VRAM / disk used | 85% | 95% |
| CPU temperature | 80 °C (176 °F) | 90 °C (194 °F) |
| GPU temperature | 75 °C (167 °F) | 83 °C (181 °F) |
| PSU draw (% of rating) | 80% | 90% |

GPU idle temps around **55-65 °C (130-150 °F)** on laptops are normal and show green.

llama.cpp generation/prompt throughput shows green when active and dims when idle; KV-cache % uses the RAM/VRAM thresholds above.

## Data sources

| Metric | Source |
|--------|--------|
| CPU utilization | sysinfo |
| CPU temperature | Windows: LibreHardwareMonitor **Remote Web Server** (`http://127.0.0.1:8085/data.json`) or WMI · Linux: hwmon sysfs |
| CPU power (PSU estimate) | Windows: LibreHardwareMonitor **Remote Web Server** (CPU Package power sensor) · Linux: RAPL (`/sys/class/powercap`) |
| GPU (temperature, utilization, VRAM, power, core clock) | nvidia-smi |
| Memory / disks / swap | sysinfo |
| Top processes (CPU%, memory) | sysinfo |
| GPU processes | `nvidia-smi --query-compute-apps` |
| Per-process disk-IO | Windows: WMI · Linux: not available |
| Disk read/write throughput | Windows: WMI (`Win32_PerfFormattedData_PerfDisk_LogicalDisk`) · Linux: `/proc/diskstats` |
| Network throughput / packets / totals / errors | sysinfo (delta between refreshes) |
| llama.cpp inference stats | `llama-server` `GET /metrics` (Prometheus) + `/props` |

## Requirements

- Rust toolchain (`rustup`)
- NVIDIA: `nvidia-smi` on PATH
- Windows CPU temps: [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) running via `../windows/powershell/Start-LibreHardwareMonitor.ps1`
- In LHM: **Options → Remote Web Server → Run** (port **8085**). Test in a browser: http://127.0.0.1:8085/data.json
- WMI (`0x8004100E`) is normal on many installs - HTTP is used instead
- Optional: a [llama.cpp](https://github.com/ggml-org/llama.cpp) server started with `llama-server --metrics --slots` (defaults to `http://127.0.0.1:8080`; override with `--llama-url`, disable with `--no-llama`; use `--llama-api-key-file` if the server requires an API key)

## Known issues

- **CSV log failure aborts the TUI and can leave the terminal broken.** If the log path is
  not writable (the default is `C:\Logs\hardware-temps.csv`, which requires the directory to
  exist and be writable), `CsvLogger::maybe_log` returns an error inside the TUI loop. By then
  raw mode and the alternate screen are already active, so `restore_terminal` is skipped and
  `hwmon` exits with a stuck terminal (no cursor/echo). Workaround: pass a writable
  `--log-path`. A `Drop`-based terminal restore guard and treating log errors as non-fatal are
  planned.

## Linux

`hwmon` also builds and runs on Linux. The `#[cfg(windows)]`-gated paths (LibreHardwareMonitor
HTTP/WMI, PowerShell helper) are skipped and replaced by native sources:

| Metric | Linux source |
|--------|--------------|
| CPU utilization / RAM / disks / network | `sysinfo` (built in) |
| CPU temperature | hwmon sysfs (`/sys/class/hwmon` — `k10temp`, `coretemp`, `zenpower`) |
| CPU power | RAPL package energy (`/sys/class/powercap`, first sample establishes the baseline) |
| GPU (NVIDIA) | `nvidia-smi` on PATH |
| Disk read/write throughput | `/proc/diskstats` (first sample only establishes the baseline) |
| llama.cpp | same as Windows (`llama-server --metrics`) |

Not available on Linux (Windows/WMI-based): battery status, per-process disk-IO ranking, and
network adapter IPv4 labels.

CPU power reads the RAPL `energy_uj` counter under `/sys/class/powercap`, which is root-only
(`0400`) by default. Make it world-readable once (a udev rule or `chmod 0444 …/energy_uj`) or run
`hwmon` as root; otherwise CPU power shows `?`.

The default `--log-path` is the Windows value (`C:\Logs\hardware-temps.csv`); pass `--log-path`
explicitly on Linux.

## Roadmap

- Battery status
- LibreHardwareMonitor in-process (no separate tray app)
- Rolling-average tok/s for llama.cpp (server reports per-request values)