//! `/status` parsing: online/offline matrix, fallback chains and guards.

mod common;

use common::{BASE_STATUS, GATEWAY_STATS};
use splash_monitor::status::{
    format_metric, model_suffix, num_gib, num_gib3, parse_gateway_stats, parse_status,
};

#[test]
fn failed_or_malformed_body_is_offline() {
    for body in [
        None,
        Some(""),
        Some("not json at all"),
        Some("[]"),
        Some("null"),
    ] {
        let view = parse_status(body);
        assert!(!view.online, "expected offline for {body:?}");
        let display = view.display();
        assert_eq!(display.metal_gib, "--");
        assert_eq!(display.limit_gib, "--");
        assert_eq!(display.context_window, "--");
        assert_eq!(display.resident, "--");
        assert_eq!(display.spilled_pages, "--");
        assert_eq!(display.spill_gib, "--");
        assert_eq!(display.kv_capacity, "--");
        assert_eq!(display.model, "--");
    }
}

#[test]
fn empty_object_is_online_with_zero_defaults() {
    let view = parse_status(Some("{}"));
    assert!(view.online);
    assert_eq!(view.metal_limit, 0);
    assert_eq!(view.context_window, 0);
    assert_eq!(view.thermal_state, "--");
    assert_eq!(view.released, "--");
    assert_eq!(view.idle_secs, "--");
}

#[test]
fn context_window_fallback_chain() {
    // Primary path wins.
    let view = parse_status(Some(
        r#"{"memory_plan":{"budget":{"context_bounds":{"window":131072}}},
            "capabilities":{"max_context_tokens":262144}}"#,
    ));
    assert_eq!(view.context_window, 131072);

    // `null` and `false` fall through to the next source; `0` is kept.
    let cases = [
        (
            r#"{"memory_plan":{"budget":{"context_bounds":{"window":null}}},"capabilities":{"max_context_tokens":262144}}"#,
            262144,
        ),
        (
            r#"{"memory_plan":{"budget":{"context_bounds":{"window":false}}},"capabilities":{"max_context_tokens":262144}}"#,
            262144,
        ),
        (
            r#"{"memory_plan":{"budget":{"context_bounds":{}}},"capabilities":{"max_context_tokens":262144}}"#,
            262144,
        ),
        (
            r#"{"memory_plan":{"budget":{"context_bounds":{"window":0}}},"capabilities":{"max_context_tokens":262144}}"#,
            0,
        ),
        (
            r#"{"memory_plan":null,"capabilities":{"max_context_tokens":131072}}"#,
            131072,
        ),
    ];
    for (body, expected) in cases {
        let view = parse_status(Some(body));
        assert_eq!(view.context_window, expected, "body {body}");
    }

    // Neither source present: zero, which the renderer prints as 0.
    let view = parse_status(Some(r#"{"capabilities":{}}"#));
    assert_eq!(view.context_window, 0);
}

#[test]
fn weights_guard() {
    let view = parse_status(Some(r#"{"active_requests":[]}"#));
    assert_eq!(view.released, "--");
    assert_eq!(view.idle_secs, "--");

    let view = parse_status(Some(
        r#"{"weights":{"released":false,"idle_release_seconds":600}}"#,
    ));
    assert_eq!(view.released, "false");
    assert_eq!(view.idle_secs, "600");

    let view = parse_status(Some(r#"{"weights":{}}"#));
    assert_eq!(view.released, "null");
    assert_eq!(view.idle_secs, "--");
}

#[test]
fn disk_guard() {
    let view = parse_status(Some(BASE_STATUS));
    assert!(view.disk_present);
    let display = view.display();
    assert_ne!(display.cache_used_gib, "--");
    assert_ne!(display.cache_limit_gib, "--");
    // The previous dashboard read this with jq's `//`, which treats `false`
    // as missing: `"persistent":false` displays as `--`, never as `false`.
    assert_eq!(display.cache_persistent, "--");
    let view = parse_status(Some(r#"{"disk":{"persistent":true,"used_bytes":1}}"#));
    assert_eq!(view.display().cache_persistent, "true");

    let view = parse_status(Some(r#"{"memory_actual":{"current_bytes":1}}"#));
    assert!(!view.disk_present);
    let display = view.display();
    assert_eq!(display.cache_used_gib, "--");
    assert_eq!(display.cache_limit_gib, "--");
    assert_eq!(display.cache_persistent, "--");
    // Cache counters never turn into a zero rate.
    assert_eq!(view.disk_read_total, None);
    assert_eq!(view.disk_written_total, None);
}

#[test]
fn scheduler_waiting_is_a_sum() {
    let view = parse_status(Some(
        r#"{"scheduler":{"queued":4,"waiting_resources":1,"waiting_prefix":2,"waiting_mask":3}}"#,
    ));
    assert_eq!(view.scheduler_waiting, 6);
    assert_eq!(view.scheduler_queued, 4);
}

#[test]
fn model_suffix_and_metrics() {
    assert_eq!(model_suffix("a/b/model:IQ3_XXS"), "IQ3_XXS");
    assert_eq!(model_suffix("no-colon-model"), "no-colon-model");
    assert_eq!(model_suffix("--"), "--");
    assert_eq!(format_metric(Some(12.34)), "12.3");
    assert_eq!(format_metric(None), "--");
    assert_eq!(num_gib(45097156608), "42.00");
    assert_eq!(num_gib(0), "0.00");
    assert_eq!(num_gib3(1073741824), "1.000");
}

#[test]
fn baseline_fixture_maps_the_displayed_fields() {
    let view = parse_status(Some(BASE_STATUS));
    assert!(view.online);
    assert_eq!(view.context_window, 262144);
    assert_eq!(view.resident, 139257);
    assert_eq!(view.kv_capacity, 139264);
    assert_eq!(view.metal_limit, 45097156608);
    assert_eq!(view.display().model, "IQ3_XXS");
    assert_eq!(view.active, 0);
    assert_eq!(view.released, "true");
    assert_eq!(view.thermal_state, "nominal");
    assert_eq!(view.spill_requests, 278);
    assert_eq!(view.cache_hits, Some(342));
    assert_eq!(view.disk_restores, Some(3924));
    assert_eq!(view.cache_write_wait, Some(2));
    assert_eq!(view.display().limit_gib, "42.00");
    assert_eq!(view.display().prefill_tps, "867.1");
    assert_eq!(view.display().ttft_p95, "11244.8");
}

#[test]
fn gateway_failed_body_is_offline() {
    for body in [
        None,
        Some(""),
        Some("not json at all"),
        Some("[]"),
        Some("null"),
    ] {
        let view = parse_gateway_stats(body);
        assert!(!view.online, "body {body:?} looked online");
        assert!(!view.gateway, "body {body:?} claimed the fallback");
    }
}

#[test]
fn gateway_stats_map_the_console_counters() {
    let view = parse_gateway_stats(Some(GATEWAY_STATS));
    assert!(view.online);
    assert!(view.gateway);
    assert_eq!(view.decode_tokens_total, Some(113514));
    assert_eq!(view.decode_rate, Some(111.4));
    assert_eq!(view.prefill_tps, Some(1623.6));
    assert_eq!(view.active, 1);
    assert_eq!(view.scheduler_queued, 2);
    assert_eq!(view.scheduler_prefill, 0);
    assert_eq!(view.scheduler_decode, 1);
    assert_eq!(view.phase, "decoding");
    assert_eq!(view.cache_hits, Some(162));
    assert_eq!(view.disk_restores, Some(735));
    assert_eq!(view.disk_hit_tokens, Some(23520));
    assert!(view.disk_present);
    assert_eq!(view.cache_used, 9292677120);
    assert_eq!(view.cache_limit, 63993282560);
    assert_eq!(view.metal_current, 83192376848);
    assert_eq!(view.metal_limit, 115974014416);
    assert_eq!(view.memory_headroom, Some(110175313695 - 83192376848));
    // Engine-only fields stay missing: no transferred bytes, no percentiles.
    assert_eq!(view.disk_read_total, None);
    assert_eq!(view.disk_written_total, None);
    assert_eq!(view.ttft_p95, None);
    assert_eq!(view.gap_p95, None);

    let display = view.display();
    assert_eq!(display.model, "Qwen3.8-Flash-Next-GSQ-RCO-IQ3_S");
    assert_eq!(display.decode_rate, "111.4");
    assert_eq!(display.prefill_tps, "1623.6");
    assert_eq!(display.ttft_p95, "--");
    assert_eq!(display.disk_read_gib, "--");
    assert_eq!(display.cache_used_gib, "8.65");
    // Engine-only fields must not render as a claimed zero.
    assert_eq!(display.spill_requests, "--");
    assert_eq!(display.kv_capacity, "--");
    assert_eq!(display.context_window, "--");
    assert_eq!(display.resident, "--");
}

#[test]
fn gateway_payload_without_cache_model_keeps_cache_lines_empty() {
    let view = parse_gateway_stats(Some(r#"{"total_completion_tokens": 5}"#));
    assert!(view.online);
    assert!(view.gateway);
    assert!(!view.disk_present);
    assert_eq!(view.cache_hits, None);
    assert_eq!(view.model, "--");
    assert_eq!(view.memory_headroom, None);
    assert_eq!(view.decode_tokens_total, Some(5));
}
