use crate::sample::GpuSnapshot;
use anyhow::Context;
use std::process::Command;

pub fn read_gpus() -> anyhow::Result<Vec<GpuSnapshot>> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,temperature.gpu,utilization.gpu,memory.used,memory.total,power.draw,clocks.current.graphics",
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

fn parse_gpu_line(line: &str) -> Option<GpuSnapshot> {
    let p: Vec<&str> = line.split(',').map(|s| s.trim().trim_matches('"')).collect();
    if p.len() < 7 {
        return None;
    }

    let parse_f = |s: &str| s.parse::<f64>().unwrap_or(0.0);
    let parse_u = |s: &str| s.parse::<u64>().unwrap_or(0);

    Some(GpuSnapshot {
        name: format!("GPU{} {}", p[0], p[1]),
        temp_c: parse_f(p[2]),
        compute_util: parse_f(p[3]),
        vram_used_mib: parse_u(p[4]),
        vram_total_mib: parse_u(p[5]),
        power_w: parse_f(p[6]),
        core_mhz: p.get(7).map(|s| parse_f(s) as u64).unwrap_or(0),
    })
}
