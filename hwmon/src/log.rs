use crate::config::{c_to_display, Config};
use crate::sample::Snapshot;
use anyhow::Context;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};

pub struct CsvLogger {
    path: std::path::PathBuf,
    every: Duration,
    last: Option<Instant>,
    writer: Option<BufWriter<File>>,
}

impl CsvLogger {
    pub fn new(cfg: &Config) -> anyhow::Result<Self> {
        Ok(Self {
            path: cfg.log_path.clone(),
            every: cfg.log_every,
            last: None,
            writer: None,
        })
    }

    pub fn maybe_log(&mut self, snap: &Snapshot, fahrenheit: bool) -> anyhow::Result<()> {
        let now = Instant::now();
        if let Some(last) = self.last {
            if now.duration_since(last) < self.every {
                return Ok(());
            }
        }
        self.write_snapshot(snap, fahrenheit)?;
        self.last = Some(now);
        Ok(())
    }

    fn ensure_writer(&mut self, fahrenheit: bool) -> anyhow::Result<&mut BufWriter<File>> {
        if self.writer.is_none() {
            if let Some(parent) = self.path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("create log dir {}", parent.display()))?;
                }
            }
            let new_file = !Path::new(&self.path).exists();
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .with_context(|| format!("open log {}", self.path.display()))?;
            let mut w = BufWriter::new(file);
            if new_file {
                writeln!(w, "{}", header(fahrenheit))?;
            }
            self.writer = Some(w);
        }
        Ok(self.writer.as_mut().unwrap())
    }

    fn write_snapshot(&mut self, snap: &Snapshot, fahrenheit: bool) -> anyhow::Result<()> {
        let w = self.ensure_writer(fahrenheit)?;
        for row in rows(snap, fahrenheit) {
            writeln!(w, "{row}")?;
        }
        w.flush()?;
        Ok(())
    }
}

#[cfg(test)]
const COLUMNS: usize = 8;

fn header(fahrenheit: bool) -> String {
    let temp = if fahrenheit { "TempF" } else { "TempC" };
    format!("Timestamp,Source,Device,{temp},UtilPct,PowerW,UsedGB,TotalGB")
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn opt(v: Option<f64>) -> String {
    v.map(|v| format!("{v:.1}")).unwrap_or_default()
}

/// One CSV line per metric, always `COLUMNS` fields. Empty field = not applicable.
fn rows(snap: &Snapshot, fahrenheit: bool) -> Vec<String> {
    let ts = snap.at.to_rfc3339();
    let row = |source: &str, device: &str, temp: Option<f64>, util: Option<f64>, power: Option<f64>, used: Option<f64>, total: Option<f64>| {
        format!(
            "{ts},{source},{},{},{},{},{},{}",
            quote(device),
            opt(temp.map(|c| c_to_display(c, fahrenheit))),
            opt(util),
            opt(power),
            opt(used),
            opt(total)
        )
    };
    let mut out = vec![row("sysinfo", "CPU Total", None, Some(snap.cpu_util), snap.power.cpu_w, None, None)];
    for t in &snap.cpu_temps {
        out.push(row(&t.source, &t.name, Some(t.celsius), None, None, None, None));
    }
    for g in &snap.gpus {
        out.push(row(
            "nvidia-smi",
            &g.name,
            g.temp_c,
            g.compute_util,
            g.power_w,
            Some(g.vram_used_mib as f64 / 1024.0),
            Some(g.vram_total_mib as f64 / 1024.0),
        ));
    }
    out.push(row("system", "RAM", None, Some(snap.mem_used_pct), None, Some(snap.mem_used_gb), Some(snap.mem_total_gb)));
    for d in &snap.disks {
        out.push(row("system", &format!("Disk {}", d.drive), None, Some(d.used_pct), None, Some(d.used_gb), Some(d.total_gb)));
    }
    out.push(row("estimate", "PSU total", None, Some(snap.power.pct_of_psu), Some(snap.power.total_w), None, Some(snap.power.psu_watts)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::{CpuTemp, DiskSnapshot, GpuSnapshot};

    /// Count CSV fields, respecting double-quoted fields.
    fn field_count(line: &str) -> usize {
        let mut n = 1;
        let mut in_q = false;
        for c in line.chars() {
            match c {
                '"' => in_q = !in_q,
                ',' if !in_q => n += 1,
                _ => {}
            }
        }
        n
    }

    #[test]
    fn every_row_matches_header_field_count() {
        let snap = Snapshot {
            cpu_temps: vec![CpuTemp { name: "Tctl, die".into(), celsius: 60.0, source: "Linux-hwmon".into() }],
            gpus: vec![GpuSnapshot { name: "GPU0 \"Weird, Name\"".into(), ..Default::default() }],
            disks: vec![DiskSnapshot { drive: "/".into(), ..Default::default() }],
            ..Default::default()
        };
        for f in [true, false] {
            assert_eq!(field_count(&header(f)), COLUMNS);
            for r in rows(&snap, f) {
                assert_eq!(field_count(&r), COLUMNS, "{r}");
            }
        }
    }

    #[test]
    fn header_honors_unit() {
        assert!(header(true).contains("TempF"));
        assert!(header(false).contains("TempC"));
    }
}
