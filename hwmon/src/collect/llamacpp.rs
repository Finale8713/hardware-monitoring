use crate::sample::LlamaSnapshot;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// Model/context info from `/props`. These don't change while the server runs,
/// so we fetch once and cache.
#[derive(Clone, Debug, Default)]
pub struct LlamaProps {
    pub model: Option<String>,
    pub n_ctx: Option<u64>,
    pub total_slots: Option<u64>,
}

/// Scrape one sample from a llama.cpp server. Returns `None` when the server is
/// unreachable (so the panel simply hides).
pub fn sample(
    base_url: &str,
    api_key_file: Option<&Path>,
    props_cache: &mut Option<LlamaProps>,
) -> Option<LlamaSnapshot> {
    // Re-read each tick so key rotation doesn't require an hwmon restart.
    let key = read_api_key(api_key_file);
    let metrics_text = fetch_text(&format!("{base_url}/metrics"), key.as_deref())?;
    let m = parse_metrics(&metrics_text);

    if props_cache.is_none() {
        *props_cache = fetch_props(base_url, key.as_deref());
    }
    let props = props_cache.clone().unwrap_or_default();

    let get = |key: &str| m.get(key).copied().unwrap_or(0.0);

    Some(LlamaSnapshot {
        model: props.model,
        n_ctx: props.n_ctx,
        total_slots: props.total_slots,
        predicted_tps: get("llamacpp:predicted_tokens_seconds"),
        prompt_tps: get("llamacpp:prompt_tokens_seconds"),
        kv_cache_ratio: get("llamacpp:kv_cache_usage_ratio"),
        kv_cache_tokens: get("llamacpp:kv_cache_tokens") as u64,
        requests_processing: get("llamacpp:requests_processing") as u64,
        requests_deferred: get("llamacpp:requests_deferred") as u64,
        tokens_predicted_total: get("llamacpp:tokens_predicted_total") as u64,
        prompt_tokens_total: get("llamacpp:prompt_tokens_total") as u64,
    })
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(400))
        .timeout_read(Duration::from_millis(700))
        .build()
}

fn read_api_key(path: Option<&Path>) -> Option<String> {
    let key = std::fs::read_to_string(path?).ok()?.trim().to_string();
    if key.is_empty() { None } else { Some(key) }
}

fn fetch_text(url: &str, api_key: Option<&str>) -> Option<String> {
    let mut req = agent().get(url);
    if let Some(key) = api_key {
        req = req.set("Authorization", &format!("Bearer {key}"));
    }
    req.call().ok()?.into_string().ok()
}

/// Parse Prometheus exposition lines like `llamacpp:predicted_tokens_seconds 45.6`.
fn parse_metrics(text: &str) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Value is the last whitespace-separated token; the rest is the key.
        let mut parts = line.rsplitn(2, char::is_whitespace);
        let (Some(value), Some(key)) = (parts.next(), parts.next()) else {
            continue;
        };
        // Drop any Prometheus labels: `name{a="b"}` -> `name`.
        let key = key.split('{').next().unwrap_or(key).trim();
        if let Ok(v) = value.trim().parse::<f64>() {
            if v.is_finite() {
                map.insert(key.to_string(), v);
            }
        }
    }
    map
}

fn fetch_props(base_url: &str, api_key: Option<&str>) -> Option<LlamaProps> {
    let text = fetch_text(&format!("{base_url}/props"), api_key)?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let dgs = json.get("default_generation_settings");

    let n_ctx = dgs
        .and_then(|d| d.get("n_ctx"))
        .and_then(|v| v.as_u64())
        .or_else(|| json.get("n_ctx").and_then(|v| v.as_u64()));

    let model = json
        .get("model_path")
        .and_then(|v| v.as_str())
        .or_else(|| dgs.and_then(|d| d.get("model")).and_then(|v| v.as_str()))
        .or_else(|| json.get("model").and_then(|v| v.as_str()))
        .map(basename);

    let total_slots = json.get("total_slots").and_then(|v| v.as_u64());

    Some(LlamaProps {
        model,
        n_ctx,
        total_slots,
    })
}

fn basename(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_string()
}
