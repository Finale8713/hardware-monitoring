use ratatui::style::{Color, Modifier, Style};

/// Green / yellow / red styling for metrics approaching limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Normal,
    Warn,
    Critical,
}

impl Level {
    pub fn style(self) -> Style {
        match self {
            Level::Normal => Style::default().fg(Color::Green),
            Level::Warn => Style::default().fg(Color::Yellow),
            Level::Critical => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        }
    }
}

/// RAM, VRAM, or disk space used (0–100%).
pub fn pct_used(pct: f64) -> Level {
    level_high(pct, 85.0, 95.0)
}

/// CPU or GPU utilization (0–100%).
pub fn pct_util(pct: f64) -> Level {
    level_high(pct, 85.0, 95.0)
}

/// AMD/Intel CPU die readings (°C).
pub fn cpu_temp_c(c: f64) -> Level {
    level_high(c, 80.0, 90.0)
}

/// NVIDIA/AMD GPU hotspot (°C). Idle laptop GPUs are often 50–65°C.
pub fn gpu_temp_c(c: f64) -> Level {
    level_high(c, 75.0, 83.0)
}

/// Percent of rated PSU capacity (estimated draw).
pub fn psu_pct(pct: f64) -> Level {
    level_high(pct, 80.0, 90.0)
}

fn level_high(value: f64, warn: f64, crit: f64) -> Level {
    if value >= crit {
        Level::Critical
    } else if value >= warn {
        Level::Warn
    } else {
        Level::Normal
    }
}
