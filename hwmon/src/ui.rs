use crate::config::{c_to_display, temp_unit, Config};
use crate::sample::Snapshot;
use crate::thresholds::{self, Level};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, snap: &Snapshot, cfg: &Config) {
    let unit = temp_unit(cfg.fahrenheit);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let header = Paragraph::new(vec![
        Line::from(Span::styled(
            " hwmon ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        power_header_line(&snap.power),
        Line::from(format!(
            " refresh {}s  |  log every {}s  |  q quit  |  yellow=80% PSU  red=90% PSU",
            cfg.refresh.as_secs_f64(),
            cfg.log_every.as_secs()
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Hardware Monitor "));
    frame.render_widget(header, chunks[0]);

    let cpu_rows = snap.cpu_temps.len().saturating_add(2);
    let cpu_gpu_rows = cpu_rows.max(snap.gpus.len().saturating_mul(6));
    let top_height = (cpu_gpu_rows.max(6) as u16).saturating_add(2);

    // Each disk renders 2 lines (capacity + read/write); +2 for the border.
    let disk_count = snap.disks.len().max(1);
    let storage_height = (disk_count * 2 + 2).min(12) as u16;

    let show_llama = snap.llama.is_some();
    let mut constraints = vec![Constraint::Min(top_height), Constraint::Length(3)];
    if show_llama {
        constraints.push(Constraint::Length(8));
    }
    constraints.push(Constraint::Min(7));
    constraints.push(Constraint::Length(storage_height));
    constraints.push(Constraint::Min(6));

    let mid = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(chunks[1]);

    let mut idx = 0;
    let top_area = mid[idx];
    idx += 1;
    let mem_area = mid[idx];
    idx += 1;
    let llama_area = if show_llama {
        let a = mid[idx];
        idx += 1;
        Some(a)
    } else {
        None
    };
    let proc_area = mid[idx];
    idx += 1;
    let storage_area = mid[idx];
    idx += 1;
    let net_area = mid[idx];

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(top_area);

    frame.render_widget(cpu_block(snap, unit, cfg.fahrenheit), top[0]);
    frame.render_widget(gpu_block(snap, unit, cfg.fahrenheit), top[1]);
    frame.render_widget(memory_block(snap), mem_area);
    if let (Some(area), Some(llama)) = (llama_area, snap.llama.as_ref()) {
        frame.render_widget(llama_block(llama), area);
    }
    frame.render_widget(process_snip_list(snap), proc_area);
    frame.render_widget(storage_list(snap), storage_area);
    frame.render_widget(network_list(snap), net_area);

    let footer = Paragraph::new(format!(
        " updated {}  |  CSV: {}",
        snap.at.format("%H:%M:%S"),
        cfg.log_path.display()
    ))
    .style(Style::default().fg(Color::DarkGray))
    .block(Block::default().borders(Borders::ALL).title(" Status "));
    frame.render_widget(footer, chunks[2]);
}

fn power_header_line(p: &crate::sample::PowerEstimate) -> Line<'static> {
    let level = thresholds::psu_pct(p.pct_of_psu);
    let cpu = p
        .cpu_w
        .map(|w| format!("{w:.0}"))
        .unwrap_or_else(|| "?".into());
    Line::from(vec![
        Span::raw(" Est. draw "),
        Span::styled(format!("{:.0} W", p.total_w), level.style()),
        Span::raw(format!(
            " / {:.0} W PSU ({:.0}%)  |  GPUs {:.0} + CPU {cpu} + rest {:.0}",
            p.psu_watts, p.pct_of_psu, p.gpu_total_w, p.overhead_w
        )),
    ])
}

fn cpu_block(snap: &Snapshot, unit: &str, fahrenheit: bool) -> Paragraph<'static> {
    let util_level = thresholds::pct_util(snap.cpu_util);
    let mut lines = vec![Line::from(vec![
        Span::raw(" utilization  "),
        Span::styled(
            format!("{:.1}%", snap.cpu_util),
            util_level.style(),
        ),
    ])];

    match snap.power.cpu_w {
        Some(w) => lines.push(Line::from(format!(" power        {w:.1} W"))),
        None => lines.push(Line::from(Span::styled(
            " power        needs LHM web server",
            Style::default().fg(Color::Yellow),
        ))),
    }

    if snap.cpu_temps.is_empty() {
        let msg = snap
            .cpu_note
            .clone()
            .unwrap_or_else(|| "no CPU temperature data".into());
        lines.push(Line::from(Span::styled(msg, Style::default().fg(Color::Yellow))));
    } else {
        for t in &snap.cpu_temps {
            let display = c_to_display(t.celsius, fahrenheit);
            let level = thresholds::cpu_temp_c(t.celsius);
            lines.push(Line::from(vec![
                Span::raw(format!(" {:<22} ", t.name)),
                Span::styled(format!("{display:>5.1} {unit}"), level.style()),
                Span::raw(format!("  ({})", t.source)),
            ]));
        }
    }
    Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" CPU "))
}

fn gpu_block(snap: &Snapshot, unit: &str, fahrenheit: bool) -> Paragraph<'static> {
    let mut lines = Vec::new();
    if snap.gpus.is_empty() {
        lines.push(Line::from(Span::styled(
            " no GPU data (nvidia-smi)",
            Style::default().fg(Color::Yellow),
        )));
    } else {
        for (i, g) in snap.gpus.iter().enumerate() {
            if i > 0 {
                lines.push(Line::from(""));
            }
            lines.extend(gpu_lines(g, unit, fahrenheit));
        }
    }
    Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" GPU "))
}

fn gpu_lines(g: &crate::sample::GpuSnapshot, unit: &str, fahrenheit: bool) -> Vec<Line<'static>> {
    let temp = c_to_display(g.temp_c, fahrenheit);
    let vram_pct = if g.vram_total_mib > 0 {
        100.0 * g.vram_used_mib as f64 / g.vram_total_mib as f64
    } else {
        0.0
    };
    let vram_used_gb = g.vram_used_mib as f64 / 1024.0;
    let vram_total_gb = g.vram_total_mib as f64 / 1024.0;
    // Show MiB below 1 GiB so an idle GPU reads as "4 MiB", not "0.0 GB".
    let vram_used_txt = if g.vram_used_mib < 1024 {
        format!("{} MiB", g.vram_used_mib)
    } else {
        format!("{vram_used_gb:.1} GB")
    };
    let vram_total_txt = if g.vram_total_mib < 1024 {
        format!("{} MiB", g.vram_total_mib)
    } else {
        format!("{vram_total_gb:.1} GB")
    };
    vec![
        Line::from(format!(" {:<24}", g.name)),
        metric_line("temp", &format!("{temp:>5.1} {unit}"), thresholds::gpu_temp_c(g.temp_c)),
        metric_line(
            "compute",
            &format!("{:.0}%", g.compute_util),
            thresholds::pct_util(g.compute_util),
        ),
        metric_line(
            "vram",
            &format!("{vram_pct:.1}%  ({vram_used_txt} / {vram_total_txt})"),
            thresholds::pct_used(vram_pct),
        ),
        Line::from(format!(" core clock   {} MHz", g.core_mhz)),
        Line::from(format!(" power        {:.1} W", g.power_w)),
    ]
}

fn memory_block(snap: &Snapshot) -> Paragraph<'static> {
    let level = thresholds::pct_used(snap.mem_used_pct);
    let mut lines = vec![Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!(
                "{:.1} / {:.1} GB used ({:.1}%)",
                snap.mem_used_gb, snap.mem_total_gb, snap.mem_used_pct
            ),
            level.style(),
        ),
        Span::raw(format!("   available {:.1} GB", snap.mem_available_gb)),
    ])];
    if snap.swap_total_gb > 0.0 {
        let swap_pct = 100.0 * snap.swap_used_gb / snap.swap_total_gb;
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::raw("swap "),
            Span::styled(
                format!(
                    "{:.1} / {:.1} GB ({swap_pct:.1}%)",
                    snap.swap_used_gb, snap.swap_total_gb
                ),
                thresholds::pct_used(swap_pct).style(),
            ),
        ]));
    }
    Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Memory "))
}

fn llama_block(l: &crate::sample::LlamaSnapshot) -> Paragraph<'static> {
    let model = l.model.clone().unwrap_or_else(|| "(model)".into());
    let ctx = l
        .n_ctx
        .map(|n| format!("ctx {n}"))
        .unwrap_or_else(|| "ctx ?".into());
    let slots = l
        .total_slots
        .map(|n| format!("slots {n}"))
        .unwrap_or_default();

    let kv_pct = l.kv_cache_ratio * 100.0;
    let req_level = if l.requests_deferred > 0 {
        Level::Warn
    } else {
        Level::Normal
    };

    let lines = vec![
        Line::from(format!(" {}   {}   {}", truncate_name(&model, 28), ctx, slots)),
        Line::from(vec![
            Span::raw(" generation  "),
            Span::styled(format!("{:.1} tok/s", l.predicted_tps), tps_style(l.predicted_tps)),
        ]),
        Line::from(vec![
            Span::raw(" prompt      "),
            Span::styled(format!("{:.1} tok/s", l.prompt_tps), tps_style(l.prompt_tps)),
        ]),
        Line::from(vec![
            Span::raw(" kv cache    "),
            Span::styled(
                format!("{kv_pct:.1}%  ({} tok)", l.kv_cache_tokens),
                thresholds::pct_used(kv_pct).style(),
            ),
        ]),
        Line::from(vec![
            Span::raw(" requests    "),
            Span::styled(
                format!(
                    "{} active  {} queued",
                    l.requests_processing, l.requests_deferred
                ),
                req_level.style(),
            ),
        ]),
        Line::from(Span::styled(
            format!(
                " totals      {} gen / {} prompt tokens",
                l.tokens_predicted_total, l.prompt_tokens_total
            ),
            Style::default().fg(Color::DarkGray),
        )),
    ];

    Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" llama.cpp "))
}

/// Active token throughput shows green; idle is dimmed.
fn tps_style(tps: f64) -> Style {
    if tps > 0.0 {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn process_snip_list(snap: &Snapshot) -> Table<'static> {
    let header = Row::new(vec![
        Span::raw("  PID"),
        Span::raw("PROCESS"),
        Span::raw("CPU%"),
        Span::raw("MEM"),
    ])
    .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    let mut rows = Vec::new();
    for p in &snap.processes.procs {
        rows.push(Row::new(vec![
            Cell::from(p.pid.to_string()),
            Cell::from(truncate_name(&p.name, 24)),
            Cell::from(format!("{:.1}", p.cpu_pct))
                .style(thresholds::pct_util(p.cpu_pct).style()),
            Cell::from(format_mem(p.mem_bytes)),
        ]));
    }
    // Sparse, separate sources are appended with a tag to keep one compact block.
    for g in &snap.processes.gpu {
        rows.push(Row::new(vec![
            Cell::from(g.pid.to_string()),
            Cell::from(format!("[GPU] {}", truncate_name(&g.name, 20))),
            Cell::from(""),
            Cell::from(g.detail.clone()),
        ]));
    }
    for d in &snap.processes.disk {
        rows.push(Row::new(vec![
            Cell::from("-"),
            Cell::from(format!("[DISK] {}", truncate_name(&d.name, 20))),
            Cell::from(""),
            Cell::from(d.detail.clone()),
        ]));
    }
    if rows.is_empty() {
        rows.push(Row::new(vec![Cell::from("(no active processes)")]));
    }

    Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Min(22),
            Constraint::Length(6),
            Constraint::Length(16),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Top processes "))
}

fn format_mem(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.0} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn truncate_name(name: &str, max: usize) -> String {
    if name.len() <= max {
        name.to_string()
    } else {
        format!("{}…", &name[..max.saturating_sub(1)])
    }
}

fn storage_list(snap: &Snapshot) -> List<'static> {
    let mut items = Vec::new();
    if snap.disks.is_empty() {
        items.push(ListItem::new("  (no disks)"));
    } else {
        for d in &snap.disks {
            let level = thresholds::pct_used(d.used_pct);
            items.push(ListItem::new(Line::from(vec![
                Span::raw(format!("  {}  ", d.drive)),
                Span::styled(
                    format!(
                        "{:.1}/{:.1} GB ({:.1}%)",
                        d.used_gb, d.total_gb, d.used_pct
                    ),
                    level.style(),
                ),
            ])));
            if let (Some(read), Some(write)) = (d.read_bps, d.write_bps) {
                items.push(ListItem::new(format!(
                    "       read {}  |  write {}",
                    format_rate(read),
                    format_rate(write)
                )));
            }
        }
    }
    List::new(items).block(Block::default().borders(Borders::ALL).title(" Storage "))
}

fn network_list(snap: &Snapshot) -> List<'static> {
    let mut items = Vec::new();
    let s = &snap.net_summary;
    if s.adapter_count > 0 {
        items.push(ListItem::new(format!(
            "  TOTAL ({})   ↓ {}  ↑ {}",
            s.adapter_count,
            format_rate(s.total_down_bps),
            format_rate(s.total_up_bps)
        )));
        items.push(ListItem::new(""));
    }
    if snap.networks.is_empty() {
        items.push(ListItem::new("  (rates after next refresh)"));
    } else {
        for n in &snap.networks {
            let ip = n
                .ipv4
                .as_deref()
                .map(|ip| format!("  |  {ip}"))
                .unwrap_or_default();
            items.push(ListItem::new(format!(
                "  {}{}",
                truncate_name(&n.name, 22),
                ip
            )));
            items.push(ListItem::new(format!(
                "       ↓ {}  ↑ {}  |  {} pkt/s down  {} pkt/s up",
                format_rate(n.down_bps),
                format_rate(n.up_bps),
                format_pps(n.down_pps),
                format_pps(n.up_pps),
            )));
            items.push(ListItem::new(format!(
                "       total {:.2} GB down / {:.2} GB up since boot",
                n.total_rx_gb, n.total_tx_gb
            )));
            if n.rx_errors > 0 || n.tx_errors > 0 {
                items.push(ListItem::new(format!(
                    "       errors  rx {}  tx {}",
                    n.rx_errors, n.tx_errors
                )));
            }
            items.push(ListItem::new(""));
        }
    }
    List::new(items).block(Block::default().borders(Borders::ALL).title(" Network "))
}

fn format_pps(pps: f64) -> String {
    if pps >= 1_000.0 {
        format!("{:.1}k", pps / 1_000.0)
    } else {
        format!("{:.0}", pps)
    }
}

fn metric_line(label: &str, value: &str, level: Level) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!(" {label:<12}")),
        Span::styled(value.to_string(), level.style()),
    ])
}

fn format_rate(bps: f64) -> String {
    if bps >= 1_000_000.0 {
        format!("{:.1} MB/s", bps / 1_000_000.0)
    } else if bps >= 1_000.0 {
        format!("{:.0} KB/s", bps / 1_000.0)
    } else {
        format!("{:.0} B/s", bps)
    }
}
