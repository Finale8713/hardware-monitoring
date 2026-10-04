mod cpu;
mod disk_io;
mod gpu;
mod llamacpp;
mod network;
mod processes;

use crate::sample::{DiskSnapshot, PowerEstimate, Snapshot};
use chrono::Local;
use sysinfo::{Disks, Networks, System};

pub struct Collector {
    sys: System,
    disks: Disks,
    networks: Networks,
    psu_watts: f64,
    psu_overhead: f64,
    tick: u64,
    process_snip_cache: crate::sample::ProcessSnip,
    llama_url: Option<String>,
    llama_api_key_file: Option<std::path::PathBuf>,
    llama_props: Option<llamacpp::LlamaProps>,
}

impl Collector {
    pub fn new(
        psu_watts: f64,
        psu_overhead: f64,
        llama_url: Option<String>,
        llama_api_key_file: Option<std::path::PathBuf>,
    ) -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        Self {
            sys,
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            psu_watts,
            psu_overhead,
            tick: 0,
            process_snip_cache: crate::sample::ProcessSnip::default(),
            llama_url,
            llama_api_key_file,
            llama_props: None,
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.disks.refresh(false);
        self.networks.refresh(false);

        let (cpu_temps, cpu_power_w, cpu_note) = cpu::read_cpu_sensors();
        let gpus = gpu::read_gpus().unwrap_or_default();
        let power = PowerEstimate::compute(
            &gpus,
            cpu_power_w,
            self.psu_watts,
            self.psu_overhead,
        );

        let mem_total = self.sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
        let mem_used = self.sys.used_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
        let mem_pct = if mem_total > 0.0 {
            100.0 * mem_used / mem_total
        } else {
            0.0
        };
        let mem_available =
            self.sys.available_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
        let swap_used = self.sys.used_swap() as f64 / (1024.0 * 1024.0 * 1024.0);
        let swap_total = self.sys.total_swap() as f64 / (1024.0 * 1024.0 * 1024.0);
        self.tick = self.tick.wrapping_add(1);
        let process_snip = if self.tick % 2 == 0 {
            let mem_total_bytes = self.sys.total_memory();
            let num_cpus = self.sys.cpus().len().max(1);
            let snip = processes::sample(&mut self.sys, mem_total_bytes, num_cpus);
            self.process_snip_cache = snip.clone();
            snip
        } else {
            self.process_snip_cache.clone()
        };

        let mut disks: Vec<DiskSnapshot> = self
            .disks
            .iter()
            .filter_map(|d| {
                let mount = d.mount_point().to_string_lossy().to_string();
                if mount.is_empty() {
                    return None;
                }
                let total = d.total_space() as f64 / (1024.0 * 1024.0 * 1024.0);
                let free = d.available_space() as f64 / (1024.0 * 1024.0 * 1024.0);
                let used = (total - free).max(0.0);
                let used_pct = if total > 0.0 {
                    100.0 * used / total
                } else {
                    0.0
                };
                Some(DiskSnapshot {
                    drive: mount,
                    used_gb: round1(used),
                    total_gb: round1(total),
                    used_pct: round1(used_pct),
                    read_bps: None,
                    write_bps: None,
                })
            })
            .collect();

        let drive_keys: Vec<String> = disks.iter().map(|d| d.drive.clone()).collect();
        let io_map = disk_io::read_disk_io_bps(&drive_keys);
        for disk in &mut disks {
            if let Some((read, write)) = io_map.get(&disk_io_key(&disk.drive)) {
                disk.read_bps = Some(*read);
                disk.write_bps = Some(*write);
            }
        }

        let (networks, net_summary) = network::sample(&self.networks);

        let llama = self.llama_url.as_ref().and_then(|url| {
            llamacpp::sample(url, self.llama_api_key_file.as_deref(), &mut self.llama_props)
        });

        Snapshot {
            at: Local::now(),
            cpu_util: round1(global_cpu_usage(&self.sys)),
            cpu_temps,
            cpu_note,
            gpus,
            power,
            mem_used_gb: round1(mem_used),
            mem_total_gb: round1(mem_total),
            mem_used_pct: round1(mem_pct),
            mem_available_gb: round1(mem_available),
            swap_used_gb: round1(swap_used),
            swap_total_gb: round1(swap_total),
            disks,
            networks,
            net_summary,
            processes: process_snip,
            llama,
        }
    }
}

fn global_cpu_usage(sys: &System) -> f64 {
    let cpus = sys.cpus();
    if cpus.is_empty() {
        return 0.0;
    }
    cpus.iter().map(|c| c.cpu_usage() as f64).sum::<f64>() / cpus.len() as f64
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn disk_io_key(drive: &str) -> String {
    let trimmed = drive.trim().trim_end_matches('\\');
    if trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':' {
        format!("{}:", trimmed.as_bytes()[0] as char)
    } else {
        trimmed.to_uppercase()
    }
}
