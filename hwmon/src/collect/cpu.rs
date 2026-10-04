use crate::sample::CpuTemp;

const LHM_HTTP_URL: &str = "http://127.0.0.1:8085/data.json";

const CPU_TEMP_HELP: &str = "CPU temp needs either:\n\
  A) LibreHardwareMonitor → Options → Remote Web Server → check Run (verify http://127.0.0.1:8085/data.json in a browser)\n\
  B) Run hwmon from an elevated PowerShell (uses LibreHardwareMonitorLib like the PowerShell monitor)";

#[cfg(windows)]
pub fn read_cpu_sensors() -> (Vec<CpuTemp>, Option<f64>, Option<String>) {
    if let Ok(json) = fetch_lhm_json() {
        let mut temps = Vec::new();
        collect_temps_from_json(&json, &mut temps);
        let power = find_cpu_package_power(&json);
        if !temps.is_empty() {
            return (temps, power, None);
        }
    }

    if let Ok(temps) = read_lhm_temps_wmi() {
        if !temps.is_empty() {
            return (temps, None, None);
        }
    }

    if let Ok(temps) = read_lhm_temps_powershell() {
        if !temps.is_empty() {
            return (temps, None, None);
        }
    }

    (vec![], None, Some(CPU_TEMP_HELP.into()))
}

#[cfg(not(windows))]
pub fn read_cpu_sensors() -> (Vec<CpuTemp>, Option<f64>, Option<String>) {
    let temps = read_sysfs_cpu_temps();
    let power_w = read_rapl_cpu_power_w();
    let note = if temps.is_empty() {
        Some("CPU temperature unavailable (no hwmon CPU sensor loaded)".into())
    } else {
        None
    };
    (temps, power_w, note)
}

#[cfg(not(windows))]
use parking_lot::Mutex;
#[cfg(not(windows))]
use std::time::Instant;

/// Cumulative package energy counter (µJ) and when it was read, for delta power.
#[cfg(not(windows))]
struct RaplBaseline {
    at: Instant,
    energy_uj: u64,
}

#[cfg(not(windows))]
static RAPL_BASELINE: Mutex<Option<RaplBaseline>> = Mutex::new(None);

/// Average CPU package power (watts) since the previous sample, computed from the
/// RAPL `energy_uj` counter. Returns `None` when the counter is unreadable (it is
/// root-only `0400` by default) or on the first call, which only sets the baseline.
#[cfg(not(windows))]
fn read_rapl_cpu_power_w() -> Option<f64> {
    let reading = read_rapl_package_energy_uj()?;
    let now = Instant::now();
    let mut base = RAPL_BASELINE.lock();
    let watts = base.as_ref().and_then(|b| {
        let secs = now.duration_since(b.at).as_secs_f64();
        // Skip counter wrap (negative delta) and too-fast resampling.
        if secs > 0.0 && reading >= b.energy_uj {
            Some((reading - b.energy_uj) as f64 / 1e6 / secs)
        } else {
            None
        }
    });
    *base = Some(RaplBaseline {
        at: now,
        energy_uj: reading,
    });
    watts
}

/// Locate the RAPL `package` domain under `/sys/class/powercap` and read its
/// cumulative energy counter (µJ). Ignores the `core`/`dram`/`uncore` sub-domains,
/// which are bogus on AMD.
#[cfg(not(windows))]
fn read_rapl_package_energy_uj() -> Option<u64> {
    let powercap = std::path::Path::new("/sys/class/powercap");
    let entries = std::fs::read_dir(powercap).ok()?;
    for entry in entries.flatten() {
        let base = entry.path();
        let Ok(name) = std::fs::read_to_string(base.join("name")) else {
            continue;
        };
        if !name.trim().starts_with("package") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(base.join("energy_uj")) else {
            continue;
        };
        if let Ok(v) = raw.trim().parse() {
            return Some(v);
        }
    }
    None
}

/// Read CPU temperatures from the Linux hwmon sysfs interface
/// (`/sys/class/hwmon/hwmonN/tempM_input`, millidegrees C). No extra dependencies;
/// lm-sensors/hwmon drivers must expose the sensor (e.g. `k10temp`, `coretemp`).
#[cfg(not(windows))]
fn read_sysfs_cpu_temps() -> Vec<CpuTemp> {
    const MAX_TEMPS: usize = 3;

    let mut temps = Vec::new();
    let root = std::path::Path::new("/sys/class/hwmon");
    let Ok(entries) = std::fs::read_dir(root) else {
        return temps;
    };

    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        if temps.len() >= MAX_TEMPS {
            break;
        }
        let base = entry.path();
        let Ok(name) = std::fs::read_to_string(base.join("name")) else {
            continue;
        };
        let name = name.trim();
        if !is_cpu_hwmon_name(name) {
            continue;
        }
        collect_hwmon_temps(&base, name, &mut temps);
    }
    temps
}

#[cfg(not(windows))]
fn collect_hwmon_temps(base: &std::path::Path, device: &str, temps: &mut Vec<CpuTemp>) {
    const MAX_TEMPS: usize = 3;

    let Ok(dir) = std::fs::read_dir(base) else {
        return;
    };
    for entry in dir.flatten() {
        if temps.len() >= MAX_TEMPS {
            break;
        }
        let file = entry.file_name();
        let file = file.to_string_lossy();
        // Match tempN_input; the suffix excludes tempN_max/crit/offset/alarm.
        let Some(idx) = file.strip_prefix("temp").and_then(|s| s.strip_suffix("_input")) else {
            continue;
        };
        let Some(raw) = std::fs::read_to_string(entry.path()).ok() else {
            continue;
        };
        let Ok(milli) = raw.trim().parse::<f64>() else {
            continue;
        };
        let celsius = milli / 1000.0;
        if celsius <= 0.0 || celsius > 200.0 {
            continue;
        }
        // Prefer the per-sensor label ("Tctl", "Package id 0", …), else device name.
        let label = std::fs::read_to_string(base.join(format!("temp{idx}_label")))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| device.to_string());
        temps.push(CpuTemp {
            name: label,
            celsius,
            source: "Linux-hwmon".into(),
        });
    }
}

#[cfg(not(windows))]
fn is_cpu_hwmon_name(name: &str) -> bool {
    matches!(
        name,
        "k10temp" | "zenpower" | "coretemp" | "cpu" | "cpu_thermal"
    )
}

#[cfg(windows)]
fn fetch_lhm_json() -> anyhow::Result<serde_json::Value> {
    let body = ureq::get(LHM_HTTP_URL)
        .call()
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .into_string()?;
    Ok(serde_json::from_str(&body)?)
}

#[cfg(windows)]
fn read_lhm_temps_wmi() -> anyhow::Result<Vec<CpuTemp>> {
    use std::collections::HashMap;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let com = COMLibrary::new()?;
    let wmi = WMIConnection::with_namespace_path("root\\LibreHardwareMonitor", com)?;

    let rows: Vec<HashMap<String, Variant>> =
        wmi.raw_query("SELECT Name, Value, SensorType FROM Sensor WHERE SensorType = 3")?;

    Ok(parse_cpu_temp_rows(rows.into_iter().filter_map(|row| {
        let name = row.get("Name").map(variant_string)?;
        let value = row.get("Value").map(variant_f64)?;
        Some((name, value))
    })))
}

#[cfg(windows)]
fn read_lhm_temps_powershell() -> anyhow::Result<Vec<CpuTemp>> {
    use serde::Deserialize;
    use std::process::Command;

    #[derive(Deserialize)]
    struct TempJson {
        name: String,
        celsius: f64,
        source: Option<String>,
    }

    let script = powershell_helper_script()?;
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.to_string_lossy(),
        ])
        .output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("PowerShell helper failed: {err}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return Ok(vec![]);
    }

    let parsed: Vec<TempJson> = serde_json::from_str(&stdout)?;
    Ok(parsed
        .into_iter()
        .map(|t| CpuTemp {
            name: t.name,
            celsius: t.celsius,
            source: t.source.unwrap_or_else(|| "LHM-lib".into()),
        })
        .collect())
}

#[cfg(windows)]
fn powershell_helper_script() -> anyhow::Result<std::path::PathBuf> {
    use std::path::PathBuf;

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(home) = std::env::var("HWMON_HOME") {
        let home = PathBuf::from(home);
        candidates.push(home.join("scripts").join("Get-CpuTempsJson.ps1"));
        candidates.push(home.join("Get-CpuTempsJson.ps1"));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("Get-CpuTempsJson.ps1"));
            candidates.push(dir.join("scripts").join("Get-CpuTempsJson.ps1"));
            // Dev build: target/release/hwmon.exe -> repo/windows/powershell/
            if let Some(repo) = dir
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
            {
                candidates.push(
                    repo.join("windows")
                        .join("powershell")
                        .join("Get-CpuTempsJson.ps1"),
                );
            }
        }
    }

    for path in candidates {
        if path.is_file() {
            return Ok(path);
        }
    }

    anyhow::bail!(
        "Get-CpuTempsJson.ps1 not found (set HWMON_HOME or run from packaged dist folder)"
    );
}

#[cfg(windows)]
fn collect_temps_from_json(node: &serde_json::Value, temps: &mut Vec<CpuTemp>) {
    let is_temp_sensor = node
        .get("Type")
        .and_then(|v| v.as_str())
        .is_none_or(|t| t == "Temperature");
    if !is_temp_sensor {
        if let Some(children) = node.get("Children").and_then(|v| v.as_array()) {
            for child in children {
                collect_temps_from_json(child, temps);
            }
        }
        return;
    }

    let text = node.get("Text").and_then(|v| v.as_str());
    let celsius = parse_lhm_temp_celsius(node);
    if let (Some(text), Some(celsius)) = (text, celsius) {
        if is_cpu_temp_name(text) && temps.len() < 3 {
            temps.push(CpuTemp {
                name: text.to_string(),
                celsius,
                source: "LHM-http".into(),
            });
        }
    }
    if let Some(children) = node.get("Children").and_then(|v| v.as_array()) {
        for child in children {
            collect_temps_from_json(child, temps);
        }
    }
}

/// LHM web JSON uses numeric `Value` in some builds; 0.9.x uses strings like `"85.9 °C"`.
#[cfg(windows)]
fn parse_lhm_temp_celsius(node: &serde_json::Value) -> Option<f64> {
    for key in ["Value", "RawValue"] {
        let Some(v) = node.get(key) else { continue };
        if let Some(n) = v.as_f64() {
            if n > 0.0 {
                return Some(n);
            }
        } else if let Some(s) = v.as_str() {
            if let Some(n) = parse_celsius_from_display(s) {
                return Some(n);
            }
        }
    }
    None
}

fn parse_celsius_from_display(s: &str) -> Option<f64> {
    let num: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if num.is_empty() {
        return None;
    }
    num.parse().ok().filter(|&v| v > 0.0)
}

fn parse_cpu_temp_rows<I>(rows: I) -> Vec<CpuTemp>
where
    I: Iterator<Item = (String, f64)>,
{
    let mut temps = Vec::new();
    for (name, value) in rows {
        if value <= 0.0 || !is_cpu_temp_name(&name) {
            continue;
        }
        temps.push(CpuTemp {
            name,
            celsius: value,
            source: "LHM".into(),
        });
        if temps.len() >= 3 {
            break;
        }
    }
    temps
}

fn is_cpu_temp_name(name: &str) -> bool {
    name.contains("CPU Package")
        || name.contains("Core (Tctl/Tdie)")
        || name.contains("Cores (Average)")
        || name.contains("Core Average")
        || name.contains("Tctl")
        || name.contains("Tdie")
}

#[cfg(windows)]
use wmi::Variant;

#[cfg(windows)]
fn variant_string(v: &Variant) -> String {
    match v {
        Variant::String(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

#[cfg(windows)]
fn find_cpu_package_power(node: &serde_json::Value) -> Option<f64> {
    let is_power_sensor = node
        .get("Type")
        .and_then(|v| v.as_str())
        .is_some_and(|t| t == "Power");
    if is_power_sensor {
        let text = node.get("Text").and_then(|v| v.as_str()).unwrap_or("");
        if is_cpu_package_power_name(text) {
            if let Some(watts) = parse_lhm_power_watts(node) {
                return Some(watts);
            }
        }
    }
    node.get("Children")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .find_map(find_cpu_package_power)
}

#[cfg(windows)]
fn is_cpu_package_power_name(name: &str) -> bool {
    name.contains("CPU Package") || name == "Package"
}

#[cfg(windows)]
fn parse_lhm_power_watts(node: &serde_json::Value) -> Option<f64> {
    for key in ["Value", "RawValue"] {
        let Some(v) = node.get(key) else { continue };
        if let Some(n) = v.as_f64() {
            if n > 0.0 {
                return Some(n);
            }
        } else if let Some(s) = v.as_str() {
            if let Some(n) = parse_watts_from_display(s) {
                return Some(n);
            }
        }
    }
    None
}

fn parse_watts_from_display(s: &str) -> Option<f64> {
    let num: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if num.is_empty() {
        return None;
    }
    num.parse().ok().filter(|&v| v > 0.0)
}

#[cfg(windows)]
fn variant_f64(v: &Variant) -> f64 {
    match v {
        Variant::R8(f) => *f,
        Variant::R4(f) => *f as f64,
        Variant::I4(i) => *i as f64,
        Variant::UI4(i) => *i as f64,
        Variant::I2(i) => *i as f64,
        Variant::UI2(i) => *i as f64,
        Variant::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}
