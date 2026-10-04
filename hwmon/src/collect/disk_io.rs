#[cfg(windows)]
use std::collections::HashMap;

/// Instant read/write bytes per second per drive letter (e.g. `C:`).
#[cfg(windows)]
pub fn read_disk_io_bps(drives: &[String]) -> HashMap<String, (f64, f64)> {
    use wmi::{COMLibrary, Variant, WMIConnection};

    let mut out = HashMap::new();
    if drives.is_empty() {
        return out;
    }

    let com = match COMLibrary::new() {
        Ok(c) => c,
        Err(_) => return out,
    };
    let wmi = match WMIConnection::new(com) {
        Ok(w) => w,
        Err(_) => return out,
    };

    let rows: Vec<HashMap<String, Variant>> = match wmi.raw_query(
        "SELECT Name, DiskReadBytesPersec, DiskWriteBytesPersec \
         FROM Win32_PerfFormattedData_PerfDisk_LogicalDisk",
    ) {
        Ok(r) => r,
        Err(_) => return out,
    };

    let wanted: HashMap<String, ()> = drives
        .iter()
        .map(|d| (normalize_drive_key(d), ()))
        .collect();

    for row in rows {
        let name = row.get("Name").map(variant_string).unwrap_or_default();
        if name.is_empty() || name == "_Total" {
            continue;
        }
        let key = normalize_drive_key(&name);
        if !wanted.contains_key(&key) {
            continue;
        }
        let read = row
            .get("DiskReadBytesPersec")
            .map(variant_f64)
            .unwrap_or(0.0)
            .max(0.0);
        let write = row
            .get("DiskWriteBytesPersec")
            .map(variant_f64)
            .unwrap_or(0.0)
            .max(0.0);
        out.insert(key, (read, write));
    }

    out
}

#[cfg(not(windows))]
use std::collections::HashMap;
#[cfg(not(windows))]
use parking_lot::Mutex;
#[cfg(not(windows))]
use std::time::Instant;

/// Cumulative sector counters per block device, used to derive bytes/sec.
#[cfg(not(windows))]
struct DiskBaseline {
    at: Instant,
    /// device name -> (sectors read, sectors written)
    counters: HashMap<String, (u64, u64)>,
}

#[cfg(not(windows))]
static DISK_BASELINE: Mutex<Option<DiskBaseline>> = Mutex::new(None);

/// Instant read/write bytes per second per mount point on Linux.
///
/// `/proc/diskstats` reports cumulative 512-byte sectors per block device, so we
/// keep a baseline and diff across calls (the dashboard polls once per refresh
/// tick). Mount points are resolved through `/proc/self/mounts`. The first call
/// after start establishes the baseline and yields no rates.
#[cfg(not(windows))]
pub fn read_disk_io_bps(drives: &[String]) -> HashMap<String, (f64, f64)> {
    let mut out = HashMap::new();
    if drives.is_empty() {
        return out;
    }

    // mount point -> block device name (e.g. "/" -> "nvme0n1p2")
    let mount_to_dev = mount_points_to_devices(drives);
    if mount_to_dev.is_empty() {
        return out;
    }

    // device name -> (sectors read, sectors written)
    let mut cur: HashMap<String, (u64, u64)> = HashMap::new();
    let text = match std::fs::read_to_string("/proc/diskstats") {
        Ok(t) => t,
        Err(_) => return out,
    };
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // major minor name reads ... sectors_read(5) ... writes ... sectors_written(9) ...
        if fields.len() < 14 {
            continue;
        }
        let dev = fields[2];
        if mount_to_dev.values().all(|d| d != dev) {
            continue;
        }
        let sectors_read: u64 = fields[5].parse().unwrap_or(0);
        let sectors_written: u64 = fields[9].parse().unwrap_or(0);
        cur.insert(dev.to_string(), (sectors_read, sectors_written));
    }
    if cur.is_empty() {
        return out;
    }

    // 512 bytes per sector is the fixed unit the kernel reports in /proc/diskstats.
    let now = Instant::now();
    let mut base = DISK_BASELINE.lock();
    if let Some(prev) = base.as_ref() {
        let secs = now.duration_since(prev.at).as_secs_f64();
        if secs > 0.0 {
            for (mount, dev) in &mount_to_dev {
                let (Some(&(cr, cw)), Some(&(pr, pw))) =
                    (cur.get(dev), prev.counters.get(dev))
                else {
                    continue;
                };
                if cr >= pr && cw >= pw {
                    out.insert(
                        mount.clone(),
                        (
                            (cr - pr) as f64 * 512.0 / secs,
                            (cw - pw) as f64 * 512.0 / secs,
                        ),
                    );
                }
            }
        }
    }
    *base = Some(DiskBaseline {
        at: now,
        counters: cur,
    });
    out
}

/// Map requested mount points to backing block device names via `/proc/self/mounts`.
/// Non-block mounts (tmpfs, overlay, …) are skipped since they never name a device.
#[cfg(not(windows))]
fn mount_points_to_devices(drives: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(text) = std::fs::read_to_string("/proc/self/mounts") else {
        return map;
    };
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let (dev, mount) = (parts[0], parts[1]);
        if !dev.starts_with("/dev/") {
            continue;
        }
        if drives.iter().any(|d| d == mount) {
            let name = dev.rsplit('/').next().unwrap_or(dev).to_string();
            map.insert(mount.to_string(), name);
        }
    }
    map
}

#[cfg(windows)]
fn normalize_drive_key(drive: &str) -> String {
    let trimmed = drive.trim().trim_end_matches('\\');
    if trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':' {
        format!("{}:", trimmed.as_bytes()[0] as char)
    } else {
        trimmed.to_uppercase()
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
        wmi::Variant::I8(i) => *i as f64,
        wmi::Variant::UI8(i) => *i as f64,
        wmi::Variant::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}
