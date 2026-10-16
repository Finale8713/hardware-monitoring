mod collect;
mod config;
mod log;
mod sample;
mod thresholds;
mod ui;

use anyhow::Result;
use collect::Collector;
use config::Config;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use log::CsvLogger;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::stdout;
use std::thread;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    let cfg = Config::from_args()?;
    let mut collector = Collector::new(
        cfg.psu_watts,
        cfg.psu_overhead,
        cfg.llama_url.clone(),
        cfg.llama_api_key_file.clone(),
    );
    let mut logger = CsvLogger::new(&cfg)?;

    // sysinfo needs a short delay between CPU refreshes for usage %
    thread::sleep(Duration::from_millis(250));
    let mut snap = collector.sample();

    if cfg.once {
        print_snapshot(&snap, &cfg);
        return logger.maybe_log(&snap, cfg.fahrenheit);
    }

    run_tui(&cfg, &mut collector, &mut logger, &mut snap)
}

fn print_snapshot(snap: &sample::Snapshot, cfg: &Config) {
    let unit = config::temp_unit(cfg.fahrenheit);
    println!("CPU  util {:.1}%", snap.cpu_util);
    for t in &snap.cpu_temps {
        println!(
            "     {} {:.1} {} ({})",
            t.name,
            config::c_to_display(t.celsius, cfg.fahrenheit),
            unit,
            t.source
        );
    }
    if let Some(note) = &snap.cpu_note {
        println!("     {note}");
    }
    if let Some(w) = snap.power.cpu_w {
        println!("     CPU power {w:.1} W");
    }
    for g in &snap.gpus {
        println!(
            "GPU  {} {:.1} {} compute {:.0}%  {:.1} W",
            g.name,
            config::c_to_display(g.temp_c, cfg.fahrenheit),
            unit,
            g.compute_util,
            g.power_w
        );
    }
    let p = &snap.power;
    let cpu = p
        .cpu_w
        .map(|w| format!("{w:.0}"))
        .unwrap_or_else(|| "?".into());
    println!(
        "PSU  est. {:.0} W / {:.0} W ({:.0}%)  [GPUs {:.0} + CPU {cpu} + rest {:.0}]",
        p.total_w, p.psu_watts, p.pct_of_psu, p.gpu_total_w, p.overhead_w
    );
    println!(
        "RAM  {:.1}/{:.1} GB used ({:.1}%)  available {:.1} GB",
        snap.mem_used_gb, snap.mem_total_gb, snap.mem_used_pct, snap.mem_available_gb
    );
    if snap.swap_total_gb > 0.0 {
        println!(
            "SWAP {:.1}/{:.1} GB used",
            snap.swap_used_gb, snap.swap_total_gb
        );
    }
}

/// Restores the terminal when dropped, so every exit path (error or panic) cleans up.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
        let _ = stdout().execute(crossterm::cursor::Show);
    }
}

fn run_tui(
    cfg: &Config,
    collector: &mut Collector,
    logger: &mut CsvLogger,
    snap: &mut sample::Snapshot,
) -> Result<()> {
    enable_raw_mode()?;
    let _guard = TerminalGuard;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut last_tick = Instant::now();
    let tick_rate = cfg.refresh;

    // A failed CSV write is shown in the status bar; it must not stop the monitor.
    let mut log_error = logger.maybe_log(snap, cfg.fahrenheit).err().map(|e| format!("{e:#}"));

    loop {
        terminal.draw(|f| ui::draw(f, snap, cfg, log_error.as_deref()))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press
                    && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                {
                    return Ok(());
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            *snap = collector.sample();
            log_error = logger.maybe_log(snap, cfg.fahrenheit).err().map(|e| format!("{e:#}"));
            last_tick = Instant::now();
        }
    }
}
