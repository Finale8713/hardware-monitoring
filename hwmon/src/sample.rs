use chrono::{DateTime, Local};

#[derive(Clone, Debug, Default)]
pub struct CpuTemp {
    pub name: String,
    pub celsius: f64,
    pub source: String,
}

#[derive(Clone, Debug, Default)]
pub struct GpuSnapshot {
    pub name: String,
    pub temp_c: f64,
    pub compute_util: f64,
    pub vram_used_mib: u64,
    pub vram_total_mib: u64,
    pub power_w: f64,
    pub core_mhz: u64,
}

#[derive(Clone, Debug)]
pub struct PowerEstimate {
    pub gpu_total_w: f64,
    pub cpu_w: Option<f64>,
    pub overhead_w: f64,
    pub total_w: f64,
    pub psu_watts: f64,
    pub pct_of_psu: f64,
}

impl Default for PowerEstimate {
    fn default() -> Self {
        Self {
            gpu_total_w: 0.0,
            cpu_w: None,
            overhead_w: 100.0,
            total_w: 0.0,
            psu_watts: 850.0,
            pct_of_psu: 0.0,
        }
    }
}

impl PowerEstimate {
    pub fn compute(
        gpus: &[GpuSnapshot],
        cpu_w: Option<f64>,
        psu_watts: f64,
        overhead_w: f64,
    ) -> Self {
        let gpu_total_w: f64 = gpus.iter().map(|g| g.power_w).sum();
        let cpu_used = cpu_w.unwrap_or(0.0);
        let total_w = gpu_total_w + cpu_used + overhead_w;
        let pct_of_psu = if psu_watts > 0.0 {
            100.0 * total_w / psu_watts
        } else {
            0.0
        };
        Self {
            gpu_total_w,
            cpu_w,
            overhead_w,
            total_w,
            psu_watts,
            pct_of_psu,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DiskSnapshot {
    pub drive: String,
    pub used_gb: f64,
    pub total_gb: f64,
    pub used_pct: f64,
    pub read_bps: Option<f64>,
    pub write_bps: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct NetSnapshot {
    pub name: String,
    pub ipv4: Option<String>,
    pub down_bps: f64,
    pub up_bps: f64,
    pub down_pps: f64,
    pub up_pps: f64,
    pub total_rx_gb: f64,
    pub total_tx_gb: f64,
    pub rx_errors: u64,
    pub tx_errors: u64,
}

#[derive(Clone, Debug, Default)]
pub struct NetSummary {
    pub total_down_bps: f64,
    pub total_up_bps: f64,
    pub adapter_count: usize,
}

/// Live stats scraped from a llama.cpp server (`--metrics` / `/props`).
#[derive(Clone, Debug, Default)]
pub struct LlamaSnapshot {
    pub model: Option<String>,
    pub n_ctx: Option<u64>,
    pub total_slots: Option<u64>,
    /// Token generation throughput (tok/s).
    pub predicted_tps: f64,
    /// Prompt (prefill) throughput (tok/s).
    pub prompt_tps: f64,
    /// KV cache occupancy as a fraction (0.0 - 1.0).
    pub kv_cache_ratio: f64,
    pub kv_cache_tokens: u64,
    pub requests_processing: u64,
    pub requests_deferred: u64,
    pub tokens_predicted_total: u64,
    pub prompt_tokens_total: u64,
}

#[derive(Clone, Debug)]
pub struct TopProcess {
    pub name: String,
    pub pid: u32,
    pub detail: String,
}

/// One process in the merged top-processes table (CPU% plus memory columns).
#[derive(Clone, Debug)]
pub struct ProcRow {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f64,
    pub mem_bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct ProcessSnip {
    pub procs: Vec<ProcRow>,
    pub gpu: Vec<TopProcess>,
    pub disk: Vec<TopProcess>,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub at: DateTime<Local>,
    pub cpu_util: f64,
    pub cpu_temps: Vec<CpuTemp>,
    pub cpu_note: Option<String>,
    pub gpus: Vec<GpuSnapshot>,
    pub power: PowerEstimate,
    pub mem_used_gb: f64,
    pub mem_total_gb: f64,
    pub mem_used_pct: f64,
    pub mem_available_gb: f64,
    pub swap_used_gb: f64,
    pub swap_total_gb: f64,
    pub disks: Vec<DiskSnapshot>,
    pub networks: Vec<NetSnapshot>,
    pub net_summary: NetSummary,
    pub processes: ProcessSnip,
    pub llama: Option<LlamaSnapshot>,
}
