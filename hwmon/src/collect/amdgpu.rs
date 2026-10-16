//! AMD GPUs through the amdgpu driver's sysfs files (Linux only).

use crate::sample::{GpuSnapshot, GpuSource};
use std::fs;
use std::path::Path;

const AMD_VENDOR: &str = "0x1002";

/// Read every AMD GPU under `drm_root` (normally `/sys/class/drm`).
/// Missing or unreadable files become `None`; a missing root gives an empty list.
pub fn read_gpus(drm_root: &Path) -> Vec<GpuSnapshot> {
    let Ok(entries) = fs::read_dir(drm_root) else {
        return Vec::new();
    };
    let mut cards: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.strip_prefix("card").is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())))
        .collect();
    cards.sort();
    cards
        .iter()
        .filter_map(|card| read_card(&drm_root.join(card).join("device"), card))
        .collect()
}

fn read_card(device: &Path, card: &str) -> Option<GpuSnapshot> {
    if read_str(&device.join("vendor"))? != AMD_VENDOR {
        return None;
    }
    let hwmon = first_hwmon(device);
    let in_hwmon = |file: &str| hwmon.as_ref().and_then(|h| read_num(&h.join(file)));
    let label = read_str(&device.join("product_name"))
        .or_else(|| hwmon.as_ref().and_then(|h| read_str(&h.join("name"))))
        .unwrap_or_else(|| "AMD GPU".into());
    let mib = |file: &str| read_num(&device.join(file)).map(|b| (b / (1024.0 * 1024.0)) as u64);
    Some(GpuSnapshot {
        source: GpuSource::Amdgpu,
        name: format!("{label} ({card})"),
        temp_c: in_hwmon("temp1_input").map(|m| m / 1000.0),
        compute_util: read_num(&device.join("gpu_busy_percent")),
        vram_used_mib: mib("mem_info_vram_used"),
        vram_total_mib: mib("mem_info_vram_total"),
        power_w: in_hwmon("power1_average").or_else(|| in_hwmon("power1_input")).map(|u| u / 1_000_000.0),
        core_mhz: None,
    })
}

fn first_hwmon(device: &Path) -> Option<std::path::PathBuf> {
    let mut dirs: Vec<_> = fs::read_dir(device.join("hwmon"))
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("hwmon")))
        .collect();
    dirs.sort();
    dirs.into_iter().next()
}

fn read_str(path: &Path) -> Option<String> {
    let s = fs::read_to_string(path).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn read_num(path: &Path) -> Option<f64> {
    read_str(path)?.parse::<f64>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fake `/sys/class/drm` in the system temp dir, removed on drop.
    struct FakeSysfs(PathBuf);

    impl FakeSysfs {
        fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "hwmon-amdgpu-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn file(&self, rel: &str, contents: &str) -> &Self {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, contents).unwrap();
            self
        }
    }

    impl Drop for FakeSysfs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn vega_with_every_file() {
        let fs = FakeSysfs::new();
        fs.file("card0/device/vendor", "0x1002\n")
            .file("card0/device/gpu_busy_percent", "37\n")
            .file("card0/device/mem_info_vram_used", &format!("{}\n", 256 * MIB))
            .file("card0/device/mem_info_vram_total", &format!("{}\n", 2048 * MIB))
            .file("card0/device/hwmon/hwmon3/name", "amdgpu\n")
            .file("card0/device/hwmon/hwmon3/temp1_input", "45000\n")
            .file("card0/device/hwmon/hwmon3/power1_average", "12500000\n");
        assert_eq!(
            read_gpus(&fs.0),
            vec![GpuSnapshot {
                source: GpuSource::Amdgpu,
                name: "amdgpu (card0)".into(),
                temp_c: Some(45.0),
                compute_util: Some(37.0),
                vram_used_mib: Some(256),
                vram_total_mib: Some(2048),
                power_w: Some(12.5),
                core_mhz: None,
            }]
        );
    }

    #[test]
    fn power1_input_used_when_no_average() {
        let fs = FakeSysfs::new();
        fs.file("card0/device/vendor", "0x1002")
            .file("card0/device/hwmon/hwmon0/power1_input", "8000000");
        assert_eq!(read_gpus(&fs.0)[0].power_w, Some(8.0));
    }

    #[test]
    fn missing_power_and_vram_are_none() {
        let fs = FakeSysfs::new();
        fs.file("card0/device/vendor", "0x1002")
            .file("card0/device/gpu_busy_percent", "3")
            .file("card0/device/hwmon/hwmon1/temp1_input", "51500");
        let g = &read_gpus(&fs.0)[0];
        assert_eq!(g.name, "AMD GPU (card0)");
        assert_eq!(g.temp_c, Some(51.5));
        assert_eq!(g.compute_util, Some(3.0));
        assert_eq!((g.power_w, g.vram_used_mib, g.vram_total_mib), (None, None, None));
    }

    #[test]
    fn non_amd_cards_and_connectors_are_ignored() {
        let fs = FakeSysfs::new();
        fs.file("card0/device/vendor", "0x10de")
            .file("card0/device/gpu_busy_percent", "50")
            .file("card1/device/vendor", "0x1002")
            .file("card1-eDP-1/device/vendor", "0x1002")
            .file("renderD128/device/vendor", "0x1002");
        let names: Vec<_> = read_gpus(&fs.0).into_iter().map(|g| g.name).collect();
        assert_eq!(names, ["AMD GPU (card1)"]);
    }

    #[test]
    fn garbage_contents_become_none() {
        let fs = FakeSysfs::new();
        fs.file("card0/device/vendor", "0x1002")
            .file("card0/device/gpu_busy_percent", "busy\n")
            .file("card0/device/mem_info_vram_used", "")
            .file("card0/device/mem_info_vram_total", "NaN")
            .file("card0/device/hwmon/hwmon0/temp1_input", "\u{fffd}xx")
            .file("card0/device/hwmon/hwmon0/power1_average", "-");
        let g = &read_gpus(&fs.0)[0];
        assert_eq!(
            (g.temp_c, g.compute_util, g.power_w, g.vram_used_mib, g.vram_total_mib),
            (None, None, None, None, None)
        );
    }

    #[test]
    fn missing_root_is_empty() {
        assert!(read_gpus(Path::new("/nonexistent/hwmon-test/drm")).is_empty());
    }
}
