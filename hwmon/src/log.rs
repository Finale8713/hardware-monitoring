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

    fn ensure_writer(&mut self) -> anyhow::Result<&mut BufWriter<File>> {
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
                writeln!(
                    w,
                    "Timestamp,Source,Device,SensorF,UtilPct,PowerW,MemUtilPct,Alert"
                )?;
            }
            self.writer = Some(w);
        }
        Ok(self.writer.as_mut().unwrap())
    }

    fn write_snapshot(&mut self, snap: &Snapshot, fahrenheit: bool) -> anyhow::Result<()> {
        let w = self.ensure_writer()?;
        let ts = snap.at.to_rfc3339();

        writeln!(
            w,
            "{ts},perf-counter,CPU Total,,{:.1},,",
            snap.cpu_util
        )?;

        for t in &snap.cpu_temps {
            let temp = c_to_display(t.celsius, fahrenheit);
            writeln!(
                w,
                "{ts},{},\"{}\",{temp:.1},,,",
                t.source,
                t.name.replace(',', ";")
            )?;
        }

        for g in &snap.gpus {
            let temp = c_to_display(g.temp_c, fahrenheit);
            let vram_pct = if g.vram_total_mib > 0 {
                100.0 * g.vram_used_mib as f64 / g.vram_total_mib as f64
            } else {
                0.0
            };
            writeln!(
                w,
                "{ts},nvidia-smi,\"{}\",{temp:.1},{:.0},{:.1},{vram_pct:.1},",
                g.name.replace(',', ";"),
                g.compute_util,
                g.power_w
            )?;
        }

        writeln!(
            w,
            "{ts},system,RAM,,{:.1},{:.1},{:.1},",
            snap.mem_used_pct, snap.mem_used_gb, snap.mem_total_gb
        )?;

        for d in &snap.disks {
            writeln!(
                w,
                "{ts},system,\"Disk {}\",,{:.1},{:.1},{:.1},",
                d.drive, d.used_pct, d.used_gb, d.total_gb
            )?;
        }

        writeln!(
            w,
            "{ts},estimate,\"PSU total\",,{:.1},{:.1},{:.1},",
            snap.power.pct_of_psu,
            snap.power.total_w,
            snap.power.psu_watts
        )?;

        w.flush()?;
        Ok(())
    }
}
