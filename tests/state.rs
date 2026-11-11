//! Derived rates and in-session history: first-sample blanks, page math,
//! counter rollback and history caps.

mod common;

use common::{BASE_STATUS, NOW};
use splash_monitor::state::{DashboardState, HISTORY_CAP, SWAP_HISTORY_CAP};
use splash_monitor::status::parse_status;
use splash_monitor::system::SystemSample;

fn swap_sample(in_pages: u64, out_pages: u64) -> SystemSample {
    SystemSample {
        swap_in_pages: Some(in_pages),
        swap_out_pages: Some(out_pages),
        swap_used_bytes: Some(1 << 30),
        page_size: 16384,
        ..SystemSample::default()
    }
}

#[test]
fn first_frame_reports_no_rate() {
    let view = parse_status(Some(BASE_STATUS));
    let mut state = DashboardState::default();
    let rates = state.update_frame(&view, &swap_sample(100, 200), NOW);
    assert_eq!(rates.swap_in_mib, None);
    assert_eq!(rates.swap_out_mib, None);
    assert_eq!(rates.cache_read_mib, None);
    assert_eq!(rates.cache_write_mib, None);
    assert_eq!(rates.volume_read_mib, None);
    assert_eq!(rates.volume_write_mib, None);
    assert_eq!(rates.output_tok, None);
}

#[test]
fn swap_pages_convert_to_mebibytes_per_second() {
    let mut state = DashboardState::default();
    state.update_swap(&swap_sample(0, 0), NOW);

    // 1000 pages x 16384 B in one second is 15.625 MiB/s.
    let (swap_in, swap_out) = state.update_swap(&swap_sample(1000, 2000), NOW + 1);
    let swap_in = swap_in.expect("rate");
    let swap_out = swap_out.expect("rate");
    assert!((swap_in - 15.625).abs() < 0.02, "swap in {swap_in}");
    assert!((swap_out - 31.25).abs() < 0.02, "swap out {swap_out}");

    // The same 1000 pages over four seconds is a quarter of the rate.
    let (swap_in, _) = state.update_swap(&swap_sample(2000, 2000), NOW + 5);
    let swap_in = swap_in.expect("rate");
    assert!((swap_in - 3.90625).abs() < 0.02, "swap in {swap_in}");
}

#[test]
fn counter_rollback_yields_missing_not_zero() {
    let mut state = DashboardState::default();
    state.update_swap(&swap_sample(1000, 1000), NOW);
    let (swap_in, swap_out) = state.update_swap(&swap_sample(500, 900), NOW + 1);
    assert_eq!(swap_in, None);
    assert_eq!(swap_out, None);
}

#[test]
fn zero_elapsed_never_divides() {
    let mut state = DashboardState::default();
    state.update_swap(&swap_sample(0, 0), NOW);
    let (swap_in, swap_out) = state.update_swap(&swap_sample(1000, 1000), NOW);
    assert_eq!(swap_in, None);
    assert_eq!(swap_out, None);
}

#[test]
fn cache_rates_come_from_server_counters() {
    let first = parse_status(Some(
        r#"{"disk":{"read_bytes":10485760,"written_bytes":5242880}}"#,
    ));
    let second = parse_status(Some(
        r#"{"disk":{"read_bytes":20971520,"written_bytes":10485760}}"#,
    ));
    let mut state = DashboardState::default();
    let (read, write) = state.update_cache_io(&first, NOW);
    assert_eq!(read, None);
    assert_eq!(write, None);
    let (read, write) = state.update_cache_io(&second, NOW + 2);
    assert!((read.expect("read") - 5.0).abs() < 0.02);
    assert!((write.expect("write") - 2.5).abs() < 0.02);

    // An offline frame clears the baseline instead of reusing it.
    let offline = parse_status(None);
    let (read, write) = state.update_cache_io(&offline, NOW + 4);
    assert_eq!(read, None);
    assert_eq!(write, None);
    let (read, write) = state.update_cache_io(&second, NOW + 6);
    assert_eq!(read, None);
    assert_eq!(write, None);
}

#[test]
fn decode_rate_uses_counter_delta_not_the_server_average() {
    let first = parse_status(Some(
        r#"{"metrics":{"decode_output_tokens":1000},"active_requests":[{"id":"r1","generated_tokens":10}]}"#,
    ));
    let second = parse_status(Some(
        r#"{"metrics":{"decode_output_tokens":1500},"active_requests":[{"id":"r1","generated_tokens":15}]}"#,
    ));
    let mut state = DashboardState::default();
    assert_eq!(state.update_inference(&first, NOW), None);
    let rate = state.update_inference(&second, NOW + 5).expect("rate");
    assert!((rate - 100.0).abs() < 0.02, "rate {rate}");

    // A counter that went backwards (server restart) yields no rate.
    let rollback = parse_status(Some(
        r#"{"metrics":{"decode_output_tokens":10},"active_requests":[{"id":"r1","generated_tokens":1}]}"#,
    ));
    assert_eq!(state.update_inference(&rollback, NOW + 10), None);
}

#[test]
fn histories_stop_at_their_caps() {
    let view = parse_status(Some(BASE_STATUS));
    let sample = common::sample_fixture();
    let mut state = DashboardState::default();
    for step in 0..200 {
        state.update_frame(&view, &sample, NOW + step);
    }
    assert_eq!(state.output_history.len(), HISTORY_CAP);
    assert_eq!(state.gpu_history.len(), HISTORY_CAP);
    assert_eq!(state.cpu_history.len(), HISTORY_CAP);
    assert_eq!(state.cache_read_history.len(), HISTORY_CAP);
    assert_eq!(state.cache_write_history.len(), HISTORY_CAP);
    assert_eq!(state.swap_history.len(), SWAP_HISTORY_CAP);
    assert_eq!(state.core_history.len(), sample.cpu_cores.len());
    for core in &state.core_history {
        assert_eq!(core.len(), HISTORY_CAP);
    }
}

#[test]
fn missing_values_are_not_recorded_as_zero_history() {
    let view = parse_status(None);
    let mut state = DashboardState::default();
    for step in 0..10 {
        state.update_frame(&view, &SystemSample::default(), NOW + step);
    }
    assert!(state.gpu_history.is_empty());
    assert!(state.cpu_history.is_empty());
    assert!(state.swap_history.is_empty());
    assert!(state.output_history.is_empty());
    assert!(state.core_history.is_empty());
}
