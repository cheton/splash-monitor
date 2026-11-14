use serde_json::Value;

pub type OptNum = Option<f64>;
pub type OptU64 = Option<u64>;

pub const MISSING: &str = "--";

#[derive(Debug, Clone)]
pub struct SplashView {
    pub online: bool,
    /// True when this view came from the oMLX gateway fallback rather than a
    /// direct Splash `/status` answer. The header labels it "GATEWAY".
    pub gateway: bool,
    pub metal_current: u64,
    pub metal_peak: u64,
    pub metal_limit: u64,
    pub context_window: u64,
    pub resident: u64,
    pub spilled_bound: u64,
    pub kv_capacity: u64,
    pub active: u64,
    pub spill_requests: u64,
    pub resident_requests: u64,
    pub spilled_pages: u64,
    pub spilled_bytes: u64,
    pub kv_write_bytes: u64,
    pub kv_read_bytes: u64,
    pub idle_secs: String,
    pub released: String,
    pub model: String,
    pub decode_rate: OptNum,
    pub gap_p95: OptNum,
    pub prefill_ms: OptNum,
    pub phase: String,
    pub prompt_tokens: u64,
    pub prompt_processed: u64,
    pub generated_tokens: u64,
    pub max_new_tokens: u64,
    pub request_id: String,
    pub disk_present: bool,
    pub cache_used: u64,
    pub cache_capacity: u64,
    pub cache_limit: u64,
    pub cache_volume_free: u64,
    pub cache_writes_gated: String,
    pub cache_persistent: String,
    pub disk_read_total: OptU64,
    pub disk_written_total: OptU64,
    pub decode_tokens_total: OptU64,
    pub scheduler_queued: u64,
    pub scheduler_waiting: u64,
    pub scheduler_prefill: u64,
    pub scheduler_decode: u64,
    pub thermal_state: String,
    pub cache_hits: OptU64,
    pub disk_hit_tokens: OptU64,
    pub disk_restores: OptU64,
    pub memory_headroom: OptU64,
    pub denied_reservations: OptU64,
    pub cache_write_wait: OptU64,
    pub cache_write_budget: OptU64,
    pub ttft_p95: OptNum,
    pub prefill_tps: OptNum,
}

/// Walk a JSON path. Any missing key, non-object step or `null` yields `None`.
fn path<'a>(root: &'a Value, steps: &[&str]) -> Option<&'a Value> {
    let mut node = root;
    for step in steps {
        node = node.get(*step)?;
    }
    Some(node)
}

/// Fallback rule of the previous dashboard: `null` and `false` fall through.
fn truthy(value: Option<&Value>) -> Option<&Value> {
    match value {
        None | Some(Value::Null) | Some(Value::Bool(false)) => None,
        Some(found) => Some(found),
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(n)) => n.as_f64(),
        _ => None,
    }
}

fn integer(value: Option<&Value>) -> Option<u64> {
    match value {
        Some(Value::Number(n)) => n.as_u64().or_else(|| n.as_f64().map(|v| v as u64)),
        _ => None,
    }
}

/// `value // fallback` for byte/token counters.
fn u64_or(root: &Value, steps: &[&str], fallback: u64) -> u64 {
    integer(truthy(path(root, steps))).unwrap_or(fallback)
}

/// `value // "--"` kept as a number when present.
fn opt_u64(root: &Value, steps: &[&str]) -> OptU64 {
    integer(truthy(path(root, steps)))
}

/// `value // "--"` kept as a float when present.
fn opt_num(root: &Value, steps: &[&str]) -> OptNum {
    number(truthy(path(root, steps)))
}

/// `value // "--"` then stringified.
fn str_or_dash(root: &Value, steps: &[&str]) -> String {
    match truthy(path(root, steps)) {
        Some(value) => to_display_text(Some(value)),
        None => MISSING.to_string(),
    }
}

/// Text conversion used by the previous dashboard: `null` becomes the literal
/// text `null`.
fn to_display_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "null".to_string(),
        Some(Value::Bool(true)) => "true".to_string(),
        Some(Value::Bool(false)) => "false".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => other.to_string(),
    }
}

/// Field-by-field port of the previous dashboard's `/status` mapping.
/// `body` is the raw `GET /status` text; `None` means the request failed.
pub fn parse_status(body: Option<&str>) -> SplashView {
    let parsed: Option<Value> = body.and_then(|text| serde_json::from_str(text).ok());
    let root: Value = match parsed {
        Some(value @ Value::Object(_)) => value,
        _ => return SplashView::offline(),
    };

    let first_request = path(&root, &["active_requests", "0"]);
    let weights_object = matches!(path(&root, &["weights"]), Some(Value::Object(_)));
    let disk_present = matches!(path(&root, &["disk"]), Some(Value::Object(_)));

    SplashView {
        online: true,
        gateway: false,
        metal_current: u64_or(&root, &["memory_actual", "current_bytes"], 0),
        metal_peak: u64_or(&root, &["memory_actual", "peak_bytes"], 0),
        metal_limit: u64_or(
            &root,
            &["memory_plan", "budget", "configured_memory_limit_bytes"],
            0,
        ),
        context_window: integer(truthy(path(
            &root,
            &["memory_plan", "budget", "context_bounds", "window"],
        )))
        .or_else(|| integer(truthy(path(&root, &["capabilities", "max_context_tokens"]))))
        .unwrap_or(0),
        resident: u64_or(
            &root,
            &["memory_plan", "budget", "context_bounds", "resident"],
            0,
        ),
        spilled_bound: u64_or(
            &root,
            &["memory_plan", "budget", "context_bounds", "spilled"],
            0,
        ),
        kv_capacity: u64_or(&root, &["memory_plan", "budget", "kv_capacity_tokens"], 0),
        active: match path(&root, &["active_requests"]) {
            Some(Value::Array(items)) => items.len() as u64,
            _ => 0,
        },
        spill_requests: u64_or(&root, &["kv_spill", "requests", "spill"], 0),
        resident_requests: u64_or(&root, &["kv_spill", "requests", "resident"], 0),
        spilled_pages: u64_or(&root, &["kv_spill", "pages", "spilled"], 0),
        spilled_bytes: u64_or(&root, &["kv_spill", "pages", "spilled_bytes"], 0),
        kv_write_bytes: u64_or(&root, &["kv_spill", "write_through", "bytes"], 0),
        kv_read_bytes: u64_or(&root, &["kv_spill", "block_cache", "read_bytes"], 0),
        idle_secs: if weights_object {
            str_or_dash(&root, &["weights", "idle_release_seconds"])
        } else {
            MISSING.to_string()
        },
        released: if weights_object {
            to_display_text(path(&root, &["weights", "released"]))
        } else {
            MISSING.to_string()
        },
        model: str_or_dash(&root, &["memory_plan", "model", "model_name"]),
        decode_rate: opt_num(&root, &["metrics", "decode_tokens_per_second"]),
        gap_p95: opt_num(&root, &["metrics", "decode_gap_ms", "p95"]),
        prefill_ms: opt_num(&root, &["model_timing", "prefill", "last_gpu_ms"]),
        phase: match first_request.and_then(|item| truthy(item.get("phase"))) {
            Some(value) => to_display_text(Some(value)),
            None => "idle".to_string(),
        },
        prompt_tokens: first_request
            .and_then(|item| integer(truthy(item.get("prompt_tokens"))))
            .unwrap_or(0),
        prompt_processed: first_request
            .and_then(|item| integer(truthy(item.get("prompt_processed"))))
            .unwrap_or(0),
        generated_tokens: first_request
            .and_then(|item| integer(truthy(item.get("generated_tokens"))))
            .unwrap_or(0),
        max_new_tokens: first_request
            .and_then(|item| integer(truthy(item.get("max_new_tokens"))))
            .unwrap_or(0),
        request_id: match first_request.and_then(|item| truthy(item.get("id"))) {
            Some(value) => to_display_text(Some(value)),
            None => MISSING.to_string(),
        },
        disk_present,
        cache_used: u64_or(&root, &["disk", "used_bytes"], 0),
        cache_capacity: u64_or(&root, &["disk", "capacity_bytes"], 0),
        cache_limit: u64_or(&root, &["disk", "limit_bytes"], 0),
        cache_volume_free: u64_or(&root, &["disk", "volume_available_bytes"], 0),
        cache_writes_gated: str_or_dash(&root, &["disk", "writes_gated"]),
        cache_persistent: str_or_dash(&root, &["disk", "persistent"]),
        disk_read_total: opt_u64(&root, &["disk", "read_bytes"]),
        disk_written_total: opt_u64(&root, &["disk", "written_bytes"]),
        decode_tokens_total: opt_u64(&root, &["metrics", "decode_output_tokens"]),
        scheduler_queued: u64_or(&root, &["scheduler", "queued"], 0),
        scheduler_waiting: u64_or(&root, &["scheduler", "waiting_resources"], 0)
            + u64_or(&root, &["scheduler", "waiting_prefix"], 0)
            + u64_or(&root, &["scheduler", "waiting_mask"], 0),
        scheduler_prefill: u64_or(&root, &["scheduler", "prefilling"], 0),
        scheduler_decode: u64_or(&root, &["scheduler", "decoding"], 0),
        thermal_state: str_or_dash(&root, &["thermal_state"]),
        cache_hits: opt_u64(&root, &["cache", "hits"]),
        disk_hit_tokens: opt_u64(&root, &["cache", "kv_disk_hit_tokens"]),
        disk_restores: opt_u64(&root, &["disk", "kv_restores"]),
        memory_headroom: opt_u64(&root, &["memory_governor", "headroom_bytes"]),
        denied_reservations: opt_u64(&root, &["memory_governor", "denied_reservations"]),
        cache_write_wait: opt_u64(&root, &["disk", "write_behind", "waiting"]),
        cache_write_budget: opt_u64(&root, &["disk", "write_budget_bytes"]),
        ttft_p95: opt_num(&root, &["metrics", "ttft_ms", "p95"]),
        prefill_tps: opt_num(&root, &["metrics", "prefill_tokens_per_second"]),
    }
}

/// Fallback mapping for the oMLX gateway: `GET {gateway}/admin/api/stats`.
/// That is the payload oMLX's own web console renders, so the counters exist.
/// Engine-only fields stay missing (`--` or 0): transferred cache bytes, KV
/// spill, TTFT p95, decode gap, cache persistence and gate, context bounds.
/// A failed request parses to the offline view, exactly like `/status`.
pub fn parse_gateway_stats(body: Option<&str>) -> SplashView {
    let parsed: Option<Value> = body.and_then(|text| serde_json::from_str(text).ok());
    let root: Value = match parsed {
        Some(value @ Value::Object(_)) => value,
        _ => return SplashView::offline(),
    };

    // The cache entry for a Splash-backed model carries the hit and SSD
    // counters; when several models are loaded, prefer the Splash one.
    let cache_model = path(&root, &["runtime_cache", "models"])
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("cache_tier").and_then(Value::as_str) == Some("splash"))
                .or_else(|| items.first())
        });
    // `active_models.models[0]` carries request counts and memory totals.
    let active_model = path(&root, &["active_models", "models"])
        .and_then(Value::as_array)
        .and_then(|items| items.first());

    let model = cache_model.or(active_model).cloned().unwrap_or(Value::Null);

    let cache = model.get("splash_cache").cloned().unwrap_or(Value::Null);
    let pressure = path(&root, &["active_models", "memory_pressure"]);

    SplashView {
        online: true,
        gateway: true,
        metal_current: opt_u64(&root, &["active_models", "model_memory_used"]).unwrap_or(0),
        metal_peak: 0,
        metal_limit: opt_u64(&root, &["active_models", "model_memory_max"]).unwrap_or(0),
        context_window: 0,
        resident: 0,
        spilled_bound: 0,
        kv_capacity: 0,
        active: opt_u64(&root, &["active_models", "total_active_requests"]).unwrap_or(0),
        spill_requests: 0,
        resident_requests: 0,
        spilled_pages: 0,
        spilled_bytes: 0,
        kv_write_bytes: 0,
        kv_read_bytes: 0,
        idle_secs: MISSING.to_string(),
        released: MISSING.to_string(),
        model: model
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| MISSING.to_string()),
        decode_rate: opt_num(&root, &["avg_generation_tps"]),
        gap_p95: None,
        prefill_ms: None,
        phase: {
            let count = |key: &str| {
                active_model
                    .and_then(|m| m.get(key))
                    .and_then(Value::as_array)
                    .is_some_and(|items| !items.is_empty())
            };
            if count("generating") {
                "decoding".to_string()
            } else if count("prefilling") {
                "prefilling".to_string()
            } else {
                "idle".to_string()
            }
        },
        prompt_tokens: 0,
        prompt_processed: 0,
        generated_tokens: 0,
        max_new_tokens: 0,
        request_id: MISSING.to_string(),
        disk_present: cache_model.is_some(),
        cache_used: integer(model.get("total_size_bytes")).unwrap_or(0),
        cache_capacity: integer(model.get("max_size_bytes")).unwrap_or(0),
        cache_limit: integer(model.get("max_size_bytes")).unwrap_or(0),
        cache_volume_free: 0,
        cache_writes_gated: MISSING.to_string(),
        cache_persistent: MISSING.to_string(),
        disk_read_total: None,
        disk_written_total: None,
        decode_tokens_total: opt_u64(&root, &["total_completion_tokens"]),
        scheduler_queued: opt_u64(&root, &["active_models", "total_waiting_requests"]).unwrap_or(0),
        scheduler_waiting: 0,
        scheduler_prefill: active_model
            .and_then(|m| m.get("prefilling"))
            .and_then(Value::as_array)
            .map_or(0, |items| items.len() as u64),
        scheduler_decode: active_model
            .and_then(|m| m.get("generating"))
            .and_then(Value::as_array)
            .map_or(0, |items| items.len() as u64),
        thermal_state: MISSING.to_string(),
        cache_hits: integer(model.get("hits")),
        disk_hit_tokens: integer(cache.get("kv_disk_hit_tokens")),
        disk_restores: integer(model.get("loads")),
        // Headroom means the distance to the gateway's hard memory line; with
        // no pressure block the distance is unknown, not the whole budget.
        memory_headroom: pressure.and_then(|p| {
            let hard = integer(p.get("hard_bytes"))?;
            let current = integer(p.get("current_bytes"))?;
            Some(hard.saturating_sub(current))
        }),
        denied_reservations: None,
        cache_write_wait: None,
        cache_write_budget: None,
        ttft_p95: None,
        prefill_tps: opt_num(&root, &["avg_prefill_tps"]),
    }
}

impl SplashView {
    /// Everything defaults to zero/offline values; the renderer turns those
    /// into `--` for the fields the offline frame must not show.
    pub fn offline() -> Self {
        SplashView {
            online: false,
            gateway: false,
            metal_current: 0,
            metal_peak: 0,
            metal_limit: 0,
            context_window: 0,
            resident: 0,
            spilled_bound: 0,
            kv_capacity: 0,
            active: 0,
            spill_requests: 0,
            resident_requests: 0,
            spilled_pages: 0,
            spilled_bytes: 0,
            kv_write_bytes: 0,
            kv_read_bytes: 0,
            idle_secs: MISSING.to_string(),
            released: MISSING.to_string(),
            model: MISSING.to_string(),
            decode_rate: None,
            gap_p95: None,
            prefill_ms: None,
            phase: "idle".to_string(),
            prompt_tokens: 0,
            prompt_processed: 0,
            generated_tokens: 0,
            max_new_tokens: 0,
            request_id: MISSING.to_string(),
            disk_present: false,
            cache_used: 0,
            cache_capacity: 0,
            cache_limit: 0,
            cache_volume_free: 0,
            cache_writes_gated: MISSING.to_string(),
            cache_persistent: MISSING.to_string(),
            disk_read_total: None,
            disk_written_total: None,
            decode_tokens_total: None,
            scheduler_queued: 0,
            scheduler_waiting: 0,
            scheduler_prefill: 0,
            scheduler_decode: 0,
            thermal_state: MISSING.to_string(),
            cache_hits: None,
            disk_hit_tokens: None,
            disk_restores: None,
            memory_headroom: None,
            denied_reservations: None,
            cache_write_wait: None,
            cache_write_budget: None,
            ttft_p95: None,
            prefill_tps: None,
        }
    }
}

pub fn num_gib(bytes: u64) -> String {
    format!("{:.2}", bytes as f64 / 1073741824.0)
}

pub fn num_gib3(bytes: u64) -> String {
    format!("{:.3}", bytes as f64 / 1073741824.0)
}

pub fn percent(part: u64, whole: u64) -> String {
    if whole > 0 {
        format!("{:.1}", (part as f64 / whole as f64) * 100.0)
    } else {
        "0.0".to_string()
    }
}

pub fn format_metric(value: OptNum) -> String {
    match value {
        Some(n) => format!("{:.1}", n),
        None => MISSING.to_string(),
    }
}

/// `${model##*:}`: text after the last colon, whole string when none.
pub fn model_suffix(model: &str) -> String {
    match model.rfind(':') {
        Some(index) => model[index + 1..].to_string(),
        None => model.to_string(),
    }
}

impl SplashView {
    /// Display-ready strings for the offline overrides documented in the plan.
    pub fn display(&self) -> SplashDisplay {
        let mut display = SplashDisplay {
            metal_gib: num_gib(self.metal_current),
            peak_gib: num_gib(self.metal_peak),
            limit_gib: num_gib(self.metal_limit),
            context_window: self.context_window.to_string(),
            resident: self.resident.to_string(),
            spilled_bound: self.spilled_bound.to_string(),
            kv_capacity: self.kv_capacity.to_string(),
            spill_requests: self.spill_requests.to_string(),
            resident_requests: self.resident_requests.to_string(),
            spilled_pages: self.spilled_pages.to_string(),
            spill_gib: num_gib3(self.spilled_bytes),
            write_gib: num_gib3(self.kv_write_bytes),
            read_gib: num_gib3(self.kv_read_bytes),
            cache_used_gib: num_gib(self.cache_used),
            cache_limit_gib: num_gib(self.cache_limit),
            cache_persistent: self.cache_persistent.clone(),
            model: model_suffix(&self.model),
            ..SplashDisplay::dashes()
        };
        display.decode_rate = format_metric(self.decode_rate);
        display.gap_p95 = format_metric(self.gap_p95);
        display.prefill_ms = format_metric(self.prefill_ms);
        display.ttft_p95 = format_metric(self.ttft_p95);
        display.prefill_tps = format_metric(self.prefill_tps);
        display.disk_read_gib = self
            .disk_read_total
            .map(num_gib)
            .unwrap_or_else(|| MISSING.to_string());
        display.disk_written_gib = self
            .disk_written_total
            .map(num_gib)
            .unwrap_or_else(|| MISSING.to_string());
        display.memory_headroom_gib = self
            .memory_headroom
            .map(num_gib)
            .unwrap_or_else(|| MISSING.to_string());

        if !self.disk_present {
            display.cache_used_gib = MISSING.to_string();
            display.cache_limit_gib = MISSING.to_string();
            display.cache_persistent = MISSING.to_string();
        }
        // The gateway never reports these engine fields; a zero would claim a
        // measurement that never happened.
        if self.gateway {
            for value in [
                &mut display.spill_requests,
                &mut display.resident_requests,
                &mut display.spilled_pages,
                &mut display.spill_gib,
                &mut display.write_gib,
                &mut display.read_gib,
                &mut display.context_window,
                &mut display.resident,
                &mut display.spilled_bound,
                &mut display.kv_capacity,
            ] {
                *value = MISSING.to_string();
            }
        }
        if !self.online {
            for value in [
                &mut display.metal_gib,
                &mut display.peak_gib,
                &mut display.limit_gib,
                &mut display.context_window,
                &mut display.resident,
                &mut display.spilled_bound,
                &mut display.kv_capacity,
                &mut display.spill_requests,
                &mut display.resident_requests,
                &mut display.spilled_pages,
                &mut display.spill_gib,
                &mut display.write_gib,
                &mut display.read_gib,
            ] {
                *value = MISSING.to_string();
            }
        }
        display
    }
}

/// Rendered text for the fields that come from `/status`.
#[derive(Debug, Clone)]
pub struct SplashDisplay {
    pub metal_gib: String,
    pub peak_gib: String,
    pub limit_gib: String,
    pub context_window: String,
    pub resident: String,
    pub spilled_bound: String,
    pub kv_capacity: String,
    pub spill_requests: String,
    pub resident_requests: String,
    pub spilled_pages: String,
    pub spill_gib: String,
    pub write_gib: String,
    pub read_gib: String,
    pub cache_used_gib: String,
    pub cache_limit_gib: String,
    pub cache_persistent: String,
    pub model: String,
    pub decode_rate: String,
    pub gap_p95: String,
    pub prefill_ms: String,
    pub ttft_p95: String,
    pub prefill_tps: String,
    pub disk_read_gib: String,
    pub disk_written_gib: String,
    pub memory_headroom_gib: String,
}

impl SplashDisplay {
    fn dashes() -> Self {
        let dash = MISSING.to_string();
        SplashDisplay {
            metal_gib: dash.clone(),
            peak_gib: dash.clone(),
            limit_gib: dash.clone(),
            context_window: dash.clone(),
            resident: dash.clone(),
            spilled_bound: dash.clone(),
            kv_capacity: dash.clone(),
            spill_requests: dash.clone(),
            resident_requests: dash.clone(),
            spilled_pages: dash.clone(),
            spill_gib: dash.clone(),
            write_gib: dash.clone(),
            read_gib: dash.clone(),
            cache_used_gib: dash.clone(),
            cache_limit_gib: dash.clone(),
            cache_persistent: dash.clone(),
            model: dash.clone(),
            decode_rate: dash.clone(),
            gap_p95: dash.clone(),
            prefill_ms: dash.clone(),
            ttft_p95: dash.clone(),
            prefill_tps: dash.clone(),
            disk_read_gib: dash.clone(),
            disk_written_gib: dash.clone(),
            memory_headroom_gib: dash,
        }
    }
}
