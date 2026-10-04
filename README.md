# Hardware Monitor

Live hardware stats: CPU/GPU temperatures (°F), utilization, memory, storage, network, top processes, and optional llama.cpp inference stats.

This repo contains **three related things**. They are easy to mix up:

| What | What it is | When to use it |
|------|------------|----------------|
| **[LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) (LHM)** | Separate app that reads sensors (CPU temps, fans, etc.) | You want a **GUI** and detailed sensor list - use LHM by itself |
| **hwmon** (`hwmon/`) | Terminal dashboard (`hwmon.exe` / `hwmon`) | You want a **compact TUI** + optional CSV logging |
| **PowerShell monitor** (`windows/powershell/`) | Console dashboard in PowerShell | You want a **rich console view** (disk IOPS, battery) without building Rust |

**Important:** LHM is **not** a replacement for hwmon. hwmon **uses** LHM in the background for CPU temperatures. If you only install LHM and open its window, you do not need hwmon at all.

### Platform support

- **`hwmon/`** — cross-platform Rust TUI: runs on **Windows and Linux**. See [hwmon/README.md](hwmon/README.md).
- **`windows/`** — **Windows-only**: the PowerShell monitor, LibreHardwareMonitor install/start helpers, and hwmon's Windows packaging/deploy scripts.

---

## Quick pick

**"I just want to see hardware sensors in a GUI."**
→ Install and run LibreHardwareMonitor only (see [LibreHardwareMonitor only](#librehardwaremonitor-only) below).

**"I want the terminal dashboard."**
→ Package once with `windows\scripts\package.ps1`, then run from `hwmon\dist\hwmon` (see [Run hwmon](#run-hwmon-daily-use)). Keep LHM running for CPU temps. On Linux, just `cargo build --release` under `hwmon/` and run the binary (see [hwmon/README.md](hwmon/README.md)).

**"I want the older PowerShell console monitor."**
→ See [windows/powershell/README.md](windows/powershell/README.md).

---

## Run hwmon (daily use, Windows)

### 1. Package (first time / after code changes)

```powershell
cd windows\scripts
.\package.ps1
```

Or rebuild and launch in one step:

```powershell
cd windows\scripts
.\Build-And-Run-Hwmon.cmd
```

Requires [Rust](https://rustup.rs) (`cargo` on PATH) only to **build**. The packaged folder at `hwmon\dist\hwmon\` does **not** need Rust on machines where you only run `hwmon.exe`.

### 2. Start the monitor

```powershell
cd hwmon\dist\hwmon
.\Run-Hwmon.cmd
```

Or from PowerShell in that folder:

```powershell
.\Run-Hwmon.ps1
```

Press **q** or **Esc** to quit.

**Tip:** In PowerShell you must use `.\Run-Hwmon.cmd`, not `Run-Hwmon.cmd` alone.

### 3. CPU temperatures (one-time setup)

hwmon gets GPU stats from `nvidia-smi`. CPU temps come from **LibreHardwareMonitor’s web server** - not from hwmon itself.

From the packaged folder:

```powershell
cd hwmon\dist\hwmon\scripts
.\install-lhm.ps1                      # once - installs LHM via winget
.\Start-LibreHardwareMonitor.ps1       # starts LHM (use this, not the Start menu shortcut)
```

In the LHM window: **Options → Remote Web Server → Run** (port **8085**).

Check it works: open http://127.0.0.1:8085/data.json in a browser.

After that, `Run-Hwmon.cmd` will try to start LHM automatically if the web server is not up.

### 4. Optional - install to your user Programs folder

```powershell
cd windows\scripts
.\package.ps1 -Install
```

Then open a **new** terminal and run:

```powershell
Run-Hwmon
```

---

## LibreHardwareMonitor only

If you decide you prefer LHM’s GUI and do **not** need the hwmon terminal:

```powershell
cd windows\powershell
.\install-lhm.ps1
.\Start-LibreHardwareMonitor.ps1
```

Use the LHM window for all sensors. You can ignore `hwmon\dist\hwmon` entirely.

The scripts under `hwmon\dist\hwmon\scripts\` are the same LHM helpers - use whichever path is convenient.

**Do not** launch LHM from the Start menu / winget shortcut alone; it can miss DLLs. Always use `Start-LibreHardwareMonitor.ps1`.

---

## What hwmon needs on each PC

| Metric | Source |
|--------|--------|
| CPU utilization, RAM, disks, network | Built into `hwmon.exe` |
| CPU temperature | LibreHardwareMonitor web server (port 8085) |
| CPU power (for PSU estimate) | LibreHardwareMonitor web server (CPU Package sensor) |
| GPU (NVIDIA) | `nvidia-smi` on PATH |
| llama.cpp inference stats (optional) | `llama-server --metrics` HTTP endpoint (port 8080) |

### PSU headroom (850 W default)

The top of the hwmon screen shows **estimated system draw** vs your PSU rating:

**GPUs** (from `nvidia-smi`) **+ CPU** (from LHM) **+ rest** (default 100 W for mobo, RAM, drives, fans)

This is an **estimate**, not a wall-meter reading. Yellow at **80%** of PSU rating, red at **90%**.

```powershell
hwmon.exe --psu-watts 850 --psu-overhead 100
```

Run LHM with the web server enabled so CPU power is included; otherwise CPU shows as `?` and the total is slightly low.

CSV logs default to `C:\Logs\hardware-temps.csv`.

More detail: [hwmon/README.md](hwmon/README.md) (threshold colors, CLI flags, development, Linux).

---

## llama.cpp inference stats (optional)

If you run a [llama.cpp](https://github.com/ggml-org/llama.cpp) server with metrics enabled, hwmon shows a **llama.cpp** panel (model, generation/prompt tok/s, KV-cache pressure, and request queue) right alongside your GPU and VRAM stats.

Start the server with metrics on:

```bash
llama-server -m your-model.gguf --metrics --slots
```

hwmon polls `http://127.0.0.1:8080` by default. If no server is running, the panel simply hides.

```powershell
hwmon.exe --llama-url http://127.0.0.1:8080
hwmon.exe --no-llama
# If the server was started with --api-key, point at a local key file:
hwmon.exe --llama-api-key-file path\to\api-key.txt
```

---

## Colors

Healthy values render **green**. Metrics turn **yellow** as they approach a limit and **red** when critical (thresholds in [hwmon/README.md](hwmon/README.md)). The cyan title and bold section headings are always shown.

---

## PowerShell monitor (alternative dashboard)

Full-featured console monitor - disk IOPS, battery, live dashboard - without building Rust. **Windows-only.**

```powershell
cd windows\powershell
.\install-lhm.ps1
.\Start-LibreHardwareMonitor.ps1
.\start-monitor.ps1 -ShowConsole
```

See [windows/powershell/README.md](windows/powershell/README.md).

---

## Repo layout

```
hardware-monitoring/
├── hwmon/           hwmon (Rust TUI) — cross-platform: Windows + Linux (src/, Cargo.toml)
├── windows/         Windows-only: PowerShell monitor + LHM helpers + packaging scripts
│   ├── powershell/  Original PowerShell monitor + LHM install/start scripts
│   └── scripts/     hwmon Windows packaging (package.ps1 → hwmon/dist/hwmon/)
└── README.md        This file
```