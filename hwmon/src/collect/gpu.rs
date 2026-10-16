use crate::sample::GpuSnapshot;
use anyhow::Context;
use std::process::Command;

pub fn read_gpus() -> anyhow::Result<Vec<GpuSnapshot>> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,temperature.gpu,utilization.gpu,memory.used,memory.total,power.draw,clocks.current.graphics,name",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .context("failed to run nvidia-smi (is the NVIDIA driver installed?)")?;

    if !out.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut gpus = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(gpu) = parse_gpu_line(line) {
            gpus.push(gpu);
        }
    }
    Ok(gpus)
}

/// Parse one line of the query above. `name` is last and split with `splitn`,
/// so commas inside the GPU name cannot shift the numeric fields.
fn parse_gpu_line(line: &str) -> Option<GpuSnapshot> {
    let p: Vec<&str> = line.splitn(8, ',').map(|s| s.trim()).collect();
    if p.len() < 8 {
        return None;
    }
    // "[N/A]", "[Not Supported]", etc. fail to parse and become None.
    let num = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite());
    let mib = |s: &str| s.parse::<u64>().unwrap_or(0);

    Some(GpuSnapshot {
        name: format!("GPU{} {}", p[0], p[7]),
        temp_c: num(p[1]),
        compute_util: num(p[2]),
        vram_used_mib: mib(p[3]),
        vram_total_mib: mib(p[4]),
        power_w: num(p[5]),
        core_mhz: num(p[6]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_normal_line() {
        let g = parse_gpu_line("0, 52, 7, 512, 6144, 18.34, 1395, NVIDIA GeForce GTX 1660 Ti").unwrap();
        assert_eq!(g.name, "GPU0 NVIDIA GeForce GTX 1660 Ti");
        assert_eq!(g.temp_c, Some(52.0));
        assert_eq!(g.compute_util, Some(7.0));
        assert_eq!((g.vram_used_mib, g.vram_total_mib), (512, 6144));
        assert_eq!(g.power_w, Some(18.34));
        assert_eq!(g.core_mhz, Some(1395.0));
    }

    #[test]
    fn na_fields_are_none() {
        let g = parse_gpu_line("0, 48, [N/A], 5, 6144, [N/A], [N/A], GeForce GTX 1660 Ti").unwrap();
        assert_eq!(g.temp_c, Some(48.0));
        assert_eq!(g.compute_util, None);
        assert_eq!(g.power_w, None);
        assert_eq!(g.core_mhz, None);
    }

    #[test]
    fn comma_in_name_does_not_shift_fields() {
        let g = parse_gpu_line("1, 60, 99, 100, 200, 75.5, 1800, Acme, Inc. GPU, rev 2").unwrap();
        assert_eq!(g.name, "GPU1 Acme, Inc. GPU, rev 2");
        assert_eq!(g.power_w, Some(75.5));
        assert_eq!(g.core_mhz, Some(1800.0));
    }

    #[test]
    fn short_line_is_rejected() {
        assert!(parse_gpu_line("0, 52, 7").is_none());
    }
}
