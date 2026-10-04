use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub refresh: Duration,
    pub log_every: Duration,
    pub log_path: PathBuf,
    pub once: bool,
    pub fahrenheit: bool,
    /// Rated PSU capacity for headroom warnings.
    pub psu_watts: f64,
    /// Fixed estimate for motherboard, RAM, drives, fans (not measured directly).
    pub psu_overhead: f64,
    /// Base URL of a llama.cpp server to scrape, or None to disable.
    pub llama_url: Option<String>,
    /// File holding the llama.cpp API key (for servers started with --api-key).
    pub llama_api_key_file: Option<PathBuf>,
}

impl Config {
    pub fn from_args() -> anyhow::Result<Self> {
        let args = Cli::parse();
        let refresh = Duration::from_secs_f64(args.interval.max(0.25));
        let log_every = Duration::from_secs(if args.log_every > 0 {
            args.log_every
        } else {
            args.interval.max(10.0) as u64
        });
        Ok(Self {
            refresh,
            log_every,
            log_path: PathBuf::from(args.log_path),
            once: args.once,
            fahrenheit: !args.celsius,
            psu_watts: args.psu_watts,
            psu_overhead: args.psu_overhead,
            llama_url: if args.no_llama {
                None
            } else {
                Some(args.llama_url.trim_end_matches('/').to_string())
            },
            llama_api_key_file: {
                let f = args.llama_api_key_file.trim();
                if f.is_empty() { None } else { Some(PathBuf::from(f)) }
            },
        })
    }
}

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "hwmon", about = "Windows hardware monitor TUI")]
struct Cli {
    /// Dashboard refresh interval in seconds
    #[arg(short, long, default_value_t = 1.0)]
    interval: f64,

    /// CSV log interval in seconds (display can refresh faster)
    #[arg(long, default_value_t = 10)]
    log_every: u64,

    /// CSV log file path
    #[arg(long, default_value = r"C:\Logs\hardware-temps.csv")]
    log_path: String,

    /// Sample once and exit (no TUI loop)
    #[arg(long)]
    once: bool,

    /// Report temperatures in Celsius instead of Fahrenheit
    #[arg(long)]
    celsius: bool,

    /// PSU wattage rating (for estimated draw vs capacity)
    #[arg(long, default_value_t = 850.0)]
    psu_watts: f64,

    /// Estimated watts for mobo, RAM, drives, and fans added to GPU/CPU draw
    #[arg(long, default_value_t = 100.0)]
    psu_overhead: f64,

    /// llama.cpp server base URL to scrape (needs `llama-server --metrics`)
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    llama_url: String,

    /// Disable the llama.cpp stats panel
    #[arg(long)]
    no_llama: bool,

    /// File holding the llama.cpp API key (empty = no Authorization header)
    #[arg(long, default_value = "")]
    llama_api_key_file: String,
}

pub fn c_to_display(c: f64, fahrenheit: bool) -> f64 {
    if fahrenheit {
        (c * 9.0 / 5.0) + 32.0
    } else {
        c
    }
}

pub fn temp_unit(fahrenheit: bool) -> &'static str {
    if fahrenheit {
        "F"
    } else {
        "C"
    }
}
