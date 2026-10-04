use crate::sample::{ProcRow, ProcessSnip, TopProcess};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

const TOP_N: usize = 3;

pub fn sample(sys: &mut System, mem_total_bytes: u64, num_cpus: usize) -> ProcessSnip {
    let refresh = ProcessRefreshKind::nothing().with_cpu().with_memory();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);

    let cpu_div = num_cpus.max(1) as f64;
    let mem_total = mem_total_bytes.max(1) as f64;

    let mut procs: Vec<ProcRow> = sys
        .processes()
        .iter()
        .filter_map(|(pid, p)| {
            let name = p.name().to_string_lossy().to_string();
            if name.is_empty() {
                return None;
            }
            let cpu_pct = (p.cpu_usage() as f64 / cpu_div).max(0.0);
            let mem_bytes = p.memory();
            let mem_pct = 100.0 * mem_bytes as f64 / mem_total;
            if cpu_pct < 0.5 && mem_pct < 0.5 {
                return None;
            }
            Some(ProcRow {
                pid: pid.as_u32(),
                name,
                cpu_pct,
                mem_bytes,
            })
        })
        .collect();

    // Union of the top-N by CPU and top-N by memory, so large idle processes
    // still show; the final table is sorted by CPU%.
    let mut by_cpu = procs.clone();
    by_cpu.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(Ordering::Equal));
    let mut by_mem = procs.clone();
    by_mem.sort_by(|a, b| b.mem_bytes.cmp(&a.mem_bytes));
    let mut keep: HashSet<u32> = HashSet::new();
    keep.extend(by_cpu.iter().take(TOP_N).map(|r| r.pid));
    keep.extend(by_mem.iter().take(TOP_N).map(|r| r.pid));
    procs.retain(|r| keep.contains(&r.pid));
    procs.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(Ordering::Equal));

    let mut gpu = read_gpu_top();
    gpu.truncate(TOP_N);
    let mut disk = read_disk_io_top();
    disk.truncate(TOP_N);

    ProcessSnip { procs, gpu, disk }
}

/// Processes holding GPU memory, via `nvidia-smi --query-compute-apps`.
/// Per-process VRAM (MiB) is reported on datacenter GPUs and TCC mode, but is
/// `N/A` on consumer GeForce cards in WDDM mode (the common Windows case), so
/// we still list the process names and fall back to a clear "n/a" label.
fn read_gpu_top() -> Vec<TopProcess> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,process_name,used_gpu_memory",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let Ok(out) = out else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    // Aggregate by pid: a process spanning multiple GPUs appears once per GPU.
    let mut by_pid: HashMap<u32, (String, Option<f64>)> = HashMap::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if parts.len() < 2 {
            continue;
        }
        let pid = parts[0].parse::<u32>().unwrap_or(0);
        let name = trim_exe_name(parts[1]);
        if name.is_empty() || name.contains("Insufficient Permissions") {
            continue;
        }
        let mem_mib = parts
            .get(2)
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|v| *v > 0.0);

        by_pid
            .entry(pid)
            .and_modify(|(_, m)| {
                if let Some(v) = mem_mib {
                    *m = Some(m.unwrap_or(0.0) + v);
                }
            })
            .or_insert((name, mem_mib));
    }

    let mut rows: Vec<TopProcess> = by_pid
        .into_iter()
        .map(|(pid, (name, mem_mib))| TopProcess {
            name,
            pid,
            detail: match mem_mib {
                Some(mib) => format!("{:.1} GB VRAM", mib / 1024.0),
                None => "VRAM n/a (GeForce/WDDM)".to_string(),
            },
        })
        .collect();

    rows.sort_by(|a, b| {
        vram_sort_key(&b.detail)
            .partial_cmp(&vram_sort_key(&a.detail))
            .unwrap_or(Ordering::Equal)
    });
    rows
}

fn vram_sort_key(detail: &str) -> f64 {
    detail
        .strip_suffix(" GB VRAM")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn trim_exe_name(name: &str) -> String {
    name.trim_end_matches('\u{0003}')
        .trim()
        .rsplit('\\')
        .next()
        .unwrap_or(name)
        .to_string()
}

#[cfg(windows)]
fn read_disk_io_top() -> Vec<TopProcess> {
    use std::collections::HashMap;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let com = match COMLibrary::new() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let wmi = match WMIConnection::new(com) {
        Ok(w) => w,
        Err(_) => return Vec::new(),
    };

    let rows: Vec<HashMap<String, Variant>> = match wmi.raw_query(
        "SELECT Name, IOReadBytesPersec, IOWriteBytesPersec \
         FROM Win32_PerfFormattedData_PerfProc_Process",
    ) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut agg: HashMap<String, (f64, f64)> = HashMap::new();
    for row in rows {
        let name = row.get("Name").map(variant_string).unwrap_or_default();
        if name.is_empty() || name == "_Total" || name == "Idle" {
            continue;
        }
        let read = row
            .get("IOReadBytesPersec")
            .map(variant_f64)
            .unwrap_or(0.0)
            .max(0.0);
        let write = row
            .get("IOWriteBytesPersec")
            .map(variant_f64)
            .unwrap_or(0.0)
            .max(0.0);
        if read + write <= 0.0 {
            continue;
        }
        let display = process_perf_name(&name);
        agg.entry(display)
            .and_modify(|(r, w)| {
                *r += read;
                *w += write;
            })
            .or_insert((read, write));
    }

    let mut rows: Vec<TopProcess> = agg
        .into_iter()
        .map(|(name, (read, write))| TopProcess {
            pid: 0,
            name: name.clone(),
            detail: format!(
                "read {} / write {}",
                format_rate(read),
                format_rate(write)
            ),
        })
        .collect();

    rows.sort_by(|a, b| {
        io_total_bps(&b.detail)
            .partial_cmp(&io_total_bps(&a.detail))
            .unwrap_or(Ordering::Equal)
    });
    rows
}

#[cfg(not(windows))]
fn read_disk_io_top() -> Vec<TopProcess> {
    Vec::new()
}

#[cfg(windows)]
fn process_perf_name(name: &str) -> String {
    name.split('#').next().unwrap_or(name).to_string()
}

#[cfg(windows)]
fn io_total_bps(detail: &str) -> f64 {
    detail
        .split_whitespace()
        .filter_map(parse_rate_token)
        .sum()
}

#[cfg(windows)]
fn parse_rate_token(s: &str) -> Option<f64> {
    if s.ends_with("MB/s") {
        s.trim_end_matches("MB/s")
            .parse::<f64>()
            .ok()
            .map(|v| v * 1_000_000.0)
    } else if s.ends_with("KB/s") {
        s.trim_end_matches("KB/s")
            .parse::<f64>()
            .ok()
            .map(|v| v * 1_000.0)
    } else if s.ends_with("B/s") {
        s.trim_end_matches("B/s").parse::<f64>().ok()
    } else {
        None
    }
}

#[cfg(windows)]
fn format_rate(bps: f64) -> String {
    if bps >= 1_000_000.0 {
        format!("{:.1} MB/s", bps / 1_000_000.0)
    } else if bps >= 1_000.0 {
        format!("{:.0} KB/s", bps / 1_000.0)
    } else {
        format!("{:.0} B/s", bps)
    }
}

#[cfg(windows)]
fn variant_string(v: &wmi::Variant) -> String {
    match v {
        wmi::Variant::String(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

#[cfg(windows)]
fn variant_f64(v: &wmi::Variant) -> f64 {
    match v {
        wmi::Variant::R8(f) => *f,
        wmi::Variant::R4(f) => *f as f64,
        wmi::Variant::I4(i) => *i as f64,
        wmi::Variant::UI4(i) => *i as f64,
        wmi::Variant::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}
