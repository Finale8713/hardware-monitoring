use crate::sample::{NetSnapshot, NetSummary};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use sysinfo::Networks;

static NET_BASELINE: Mutex<Option<NetBaseline>> = Mutex::new(None);

struct NetBaseline {
    at: Instant,
    totals: HashMap<String, NetCounters>,
}

#[derive(Clone, Copy, Default)]
struct NetCounters {
    rx_bytes: u64,
    tx_bytes: u64,
    rx_packets: u64,
    tx_packets: u64,
}

pub fn sample(networks: &Networks) -> (Vec<NetSnapshot>, NetSummary) {
    let ips = adapter_ipv4_map();
    let now = Instant::now();
    let mut current = HashMap::new();

    for (name, data) in networks.iter() {
        if should_skip_adapter(&name) {
            continue;
        }
        if data.total_received() == 0
            && data.total_transmitted() == 0
            && data.total_packets_received() == 0
            && data.total_packets_transmitted() == 0
        {
            continue;
        }
        current.insert(
            name.clone(),
            NetCounters {
                rx_bytes: data.total_received(),
                tx_bytes: data.total_transmitted(),
                rx_packets: data.total_packets_received(),
                tx_packets: data.total_packets_transmitted(),
            },
        );
    }

    let mut out = Vec::new();
    let mut guard = NET_BASELINE.lock().unwrap();
    if let Some(base) = guard.as_ref() {
        let secs = now.duration_since(base.at).as_secs_f64().max(0.001);
        for (name, cur) in &current {
            let Some(prev) = base.totals.get(name) else {
                continue;
            };
            let down = byte_delta(cur.rx_bytes, prev.rx_bytes, secs);
            let up = byte_delta(cur.tx_bytes, prev.tx_bytes, secs);
            let down_pps = packet_delta(cur.rx_packets, prev.rx_packets, secs);
            let up_pps = packet_delta(cur.tx_packets, prev.tx_packets, secs);
            if down.is_none() && up.is_none() {
                continue;
            }
            let data = networks.get(name);
            let (rx_err, tx_err) = data
                .map(|d| {
                    (
                        d.total_errors_on_received(),
                        d.total_errors_on_transmitted(),
                    )
                })
                .unwrap_or((0, 0));

            out.push(NetSnapshot {
                name: name.clone(),
                ipv4: ips.get(name).cloned(),
                down_bps: down.unwrap_or(0.0),
                up_bps: up.unwrap_or(0.0),
                down_pps: down_pps.unwrap_or(0.0),
                up_pps: up_pps.unwrap_or(0.0),
                total_rx_gb: cur.rx_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                total_tx_gb: cur.tx_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                rx_errors: rx_err,
                tx_errors: tx_err,
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
    }
    *guard = Some(NetBaseline {
        at: now,
        totals: current,
    });

    let summary = NetSummary {
        total_down_bps: out.iter().map(|n| n.down_bps).sum(),
        total_up_bps: out.iter().map(|n| n.up_bps).sum(),
        adapter_count: out.len(),
    };

    (out, summary)
}

fn byte_delta(cur: u64, prev: u64, secs: f64) -> Option<f64> {
    if cur >= prev {
        Some((cur - prev) as f64 / secs)
    } else {
        None
    }
}

fn packet_delta(cur: u64, prev: u64, secs: f64) -> Option<f64> {
    byte_delta(cur, prev, secs)
}

fn should_skip_adapter(name: &str) -> bool {
    if name.contains("Loopback") {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    [
        "isatap",
        "teredo",
        "bluetooth",
        "kernel",
        "virtualbox",
        "vmware",
        "npcap",
        "wan miniport",
        "ras async",
        "ppoe",
        "qos packet scheduler",
    ]
    .iter()
    .any(|s| lower.contains(s))
}

#[cfg(windows)]
fn adapter_ipv4_map() -> HashMap<String, String> {
    use std::collections::HashMap;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let mut map = HashMap::new();
    let com = match COMLibrary::new() {
        Ok(c) => c,
        Err(_) => return map,
    };
    let wmi = match WMIConnection::new(com) {
        Ok(w) => w,
        Err(_) => return map,
    };

    let rows: Vec<HashMap<String, Variant>> = match wmi.raw_query(
        "SELECT NetConnectionID, Description, IPAddress FROM Win32_NetworkAdapterConfiguration WHERE IPEnabled=TRUE",
    ) {
        Ok(r) => r,
        Err(_) => return map,
    };

    for row in rows {
        let desc = row
            .get("Description")
            .map(variant_string)
            .unwrap_or_default();
        let conn = row
            .get("NetConnectionID")
            .map(variant_string)
            .unwrap_or_default();
        let ip = row.get("IPAddress").and_then(first_ipv4);
        let Some(ip) = ip else { continue };
        if !conn.is_empty() {
            map.insert(conn, ip.clone());
        }
        if !desc.is_empty() {
            map.insert(desc, ip);
        }
    }

    map
}

#[cfg(not(windows))]
fn adapter_ipv4_map() -> HashMap<String, String> {
    HashMap::new()
}

#[cfg(windows)]
fn first_ipv4(v: &wmi::Variant) -> Option<String> {
    match v {
        wmi::Variant::String(s) => parse_ipv4(s),
        wmi::Variant::Array(arr) => arr.iter().find_map(|item| {
            if let wmi::Variant::String(s) = item {
                parse_ipv4(s)
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn parse_ipv4(s: &str) -> Option<String> {
    if s.contains('.') && !s.starts_with("169.254") && !s.starts_with("127.") {
        Some(s.to_string())
    } else {
        None
    }
}

#[cfg(windows)]
fn variant_string(v: &wmi::Variant) -> String {
    match v {
        wmi::Variant::String(s) => s.clone(),
        other => format!("{other:?}"),
    }
}
