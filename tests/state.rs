//! Derived rates and in-session history: first-sample blanks, page math,
//! counter rollback and history caps.

mod common;

use common::{BASE_STATUS, NOW};
use splash_monitor::state::{DashboardState, HISTORY_CAP, SWAP_HISTORY_CAP};
use splash_monitor::status::parse_status;
use splash_monitor::system::{SystemSample, VolumeSample};

fn swap_sample(in_pages: u64, out_pages: u64) -> SystemSample {
    SystemSample {
        swap_in_pages: Some(in_pages),
        swap_out_pages: Some(out_pages),
        swap_used_bytes: Some(1 << 30),
        page_size: 16384,
        ..SystemSample::default()
    }
}

fn volume_sample(device: &str, read: Option<u64>, write: Option<u64>) -> SystemSample {
    SystemSample {
        volume: Some(VolumeSample {
            device: device.to_string(),
            read_bytes: read,
            write_bytes: write,
        }),
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
fn volume_rates_and_session_totals_track_observed_deltas() {
    const MIB: u64 = 1_048_576;
    let view = parse_status(None);
    let mut state = DashboardState::default();
    assert_eq!(state.volume_session_read_bytes, None);
    assert_eq!(state.volume_session_write_bytes, None);

    let first = volume_sample("disk0", Some(10 * MIB), Some(20 * MIB));
    let rates = state.update_frame(&view, &first, NOW);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    assert_eq!(state.volume_session_read_bytes, Some(0));
    assert_eq!(state.volume_session_write_bytes, Some(0));
    assert!(state.volume_read_history.is_empty());
    assert!(state.volume_write_history.is_empty());

    let next = volume_sample("disk0", Some(13 * MIB), Some(25 * MIB));
    let rates = state.update_frame(&view, &next, NOW + 2);
    assert_eq!(
        (rates.volume_read_mib, rates.volume_write_mib),
        (Some(1.5), Some(2.5))
    );
    assert_eq!(state.volume_session_read_bytes, Some(3 * MIB));
    assert_eq!(state.volume_session_write_bytes, Some(5 * MIB));

    let rates = state.update_frame(&view, &next, NOW + 3);
    assert_eq!(
        (rates.volume_read_mib, rates.volume_write_mib),
        (Some(0.0), Some(0.0))
    );
    assert_eq!(
        state.volume_read_history.iter().copied().collect::<Vec<_>>(),
        vec![1.5, 0.0]
    );
    assert_eq!(
        state.volume_write_history.iter().copied().collect::<Vec<_>>(),
        vec![2.5, 0.0]
    );

    // A monotonic counter delta is still observed when no rate can be timed.
    let same_time = volume_sample("disk0", Some(14 * MIB), Some(26 * MIB));
    let rates = state.update_frame(&view, &same_time, NOW + 3);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    assert_eq!(state.volume_session_read_bytes, Some(4 * MIB));
    assert_eq!(state.volume_session_write_bytes, Some(6 * MIB));
    assert_eq!(state.volume_read_history.len(), 2);
    assert_eq!(state.volume_write_history.len(), 2);

    let after = volume_sample("disk0", Some(15 * MIB), Some(28 * MIB));
    let rates = state.update_frame(&view, &after, NOW + 4);
    assert_eq!(
        (rates.volume_read_mib, rates.volume_write_mib),
        (Some(1.0), Some(2.0))
    );
    assert_eq!(state.volume_session_read_bytes, Some(5 * MIB));
    assert_eq!(state.volume_session_write_bytes, Some(8 * MIB));
}

#[test]
fn volume_gaps_clear_baselines_without_erasing_totals_or_history() {
    let view = parse_status(None);
    let mut state = DashboardState::default();
    state.update_frame(&view, &volume_sample("disk0", Some(10), Some(20)), NOW);
    state.update_frame(&view, &volume_sample("disk0", Some(12), Some(23)), NOW + 1);

    let rates = state.update_frame(&view, &SystemSample::default(), NOW + 2);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    assert_eq!(state.volume_session_read_bytes, Some(2));
    assert_eq!(state.volume_session_write_bytes, Some(3));
    let rates = state.update_frame(&view, &volume_sample("disk0", Some(100), Some(200)), NOW + 3);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    state.update_frame(&view, &volume_sample("disk0", Some(101), Some(202)), NOW + 4);
    assert_eq!(state.volume_session_read_bytes, Some(3));
    assert_eq!(state.volume_session_write_bytes, Some(5));

    let rates = state.update_frame(&view, &volume_sample("disk0", None, Some(300)), NOW + 5);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    let rates = state.update_frame(&view, &volume_sample("disk0", Some(500), Some(600)), NOW + 6);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    let rates = state.update_frame(&view, &volume_sample("disk0", Some(501), None), NOW + 7);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    let rates = state.update_frame(
        &view,
        &volume_sample("disk0", Some(1000), Some(1000)),
        NOW + 8,
    );
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    assert_eq!(state.volume_session_read_bytes, Some(3));
    assert_eq!(state.volume_session_write_bytes, Some(5));
    assert_eq!(state.volume_read_history.len(), 2);
    assert_eq!(state.volume_write_history.len(), 2);

    state.update_frame(
        &view,
        &volume_sample("disk0", Some(1001), Some(1002)),
        NOW + 9,
    );
    assert_eq!(state.volume_session_read_bytes, Some(4));
    assert_eq!(state.volume_session_write_bytes, Some(7));
    assert_eq!(state.volume_read_history.len(), 3);
    assert_eq!(state.volume_write_history.len(), 3);
}

#[test]
fn volume_counter_rollback_rebaselines_both_counters() {
    let view = parse_status(None);
    let mut state = DashboardState::default();
    state.update_frame(&view, &volume_sample("disk0", Some(10), Some(20)), NOW);
    state.update_frame(&view, &volume_sample("disk0", Some(15), Some(25)), NOW + 1);

    let rates = state.update_frame(&view, &volume_sample("disk0", Some(14), Some(28)), NOW + 2);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    assert_eq!(state.volume_session_read_bytes, Some(5));
    assert_eq!(state.volume_session_write_bytes, Some(5));
    assert_eq!(state.volume_read_history.len(), 1);
    assert_eq!(state.volume_write_history.len(), 1);

    state.update_frame(&view, &volume_sample("disk0", Some(16), Some(30)), NOW + 3);
    assert_eq!(state.volume_session_read_bytes, Some(7));
    assert_eq!(state.volume_session_write_bytes, Some(7));
    let rates = state.update_frame(&view, &volume_sample("disk0", Some(20), Some(29)), NOW + 4);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    assert_eq!(state.volume_session_read_bytes, Some(7));
    assert_eq!(state.volume_session_write_bytes, Some(7));

    state.update_frame(&view, &volume_sample("disk0", Some(21), Some(31)), NOW + 5);
    assert_eq!(state.volume_session_read_bytes, Some(8));
    assert_eq!(state.volume_session_write_bytes, Some(9));
    assert_eq!(state.volume_read_history.len(), 3);
    assert_eq!(state.volume_write_history.len(), 3);
}

#[test]
fn volume_device_change_resets_session_even_after_a_gap_or_with_missing_counters() {
    let view = parse_status(None);
    let mut state = DashboardState::default();
    state.update_frame(&view, &volume_sample("disk0", Some(10), Some(20)), NOW);
    state.update_frame(&view, &volume_sample("disk0", Some(15), Some(25)), NOW + 1);

    let rates = state.update_frame(&view, &volume_sample("disk1", None, Some(30)), NOW + 2);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    assert_eq!(state.volume_session_read_bytes, None);
    assert_eq!(state.volume_session_write_bytes, None);
    assert!(state.volume_read_history.is_empty());
    assert!(state.volume_write_history.is_empty());

    let rates = state.update_frame(&view, &volume_sample("disk1", Some(40), Some(50)), NOW + 3);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    assert_eq!(state.volume_session_read_bytes, Some(0));
    assert_eq!(state.volume_session_write_bytes, Some(0));
    state.update_frame(&view, &volume_sample("disk1", Some(42), Some(53)), NOW + 4);
    state.update_frame(&view, &SystemSample::default(), NOW + 5);
    assert_eq!(state.volume_session_read_bytes, Some(2));
    assert_eq!(state.volume_session_write_bytes, Some(3));
    assert_eq!(state.volume_read_history.len(), 1);
    assert_eq!(state.volume_write_history.len(), 1);

    let rates = state.update_frame(&view, &volume_sample("disk0", Some(60), Some(70)), NOW + 6);
    assert_eq!((rates.volume_read_mib, rates.volume_write_mib), (None, None));
    assert_eq!(state.volume_session_read_bytes, Some(0));
    assert_eq!(state.volume_session_write_bytes, Some(0));
    assert!(state.volume_read_history.is_empty());
    assert!(state.volume_write_history.is_empty());
}

#[test]
fn volume_rate_histories_are_bounded() {
    const MIB: u64 = 1_048_576;
    let view = parse_status(None);
    let mut state = DashboardState::default();
    let mut read = 0;
    let mut write = 0;
    for step in 0..(HISTORY_CAP + 4) {
        read += step as u64 * MIB;
        write += step as u64 * 2 * MIB;
        state.update_frame(
            &view,
            &volume_sample("disk0", Some(read), Some(write)),
            NOW + step as i64,
        );
    }
    assert_eq!(state.volume_read_history.len(), HISTORY_CAP);
    assert_eq!(state.volume_write_history.len(), HISTORY_CAP);
    assert_eq!(state.volume_read_history.front(), Some(&4.0));
    assert_eq!(state.volume_write_history.front(), Some(&8.0));
    assert_eq!(
        state.volume_read_history.back(),
        Some(&((HISTORY_CAP + 3) as f64))
    );
    assert_eq!(
        state.volume_write_history.back(),
        Some(&((HISTORY_CAP + 3) as f64 * 2.0))
    );
    assert_eq!(state.volume_session_read_bytes, Some(read));
    assert_eq!(state.volume_session_write_bytes, Some(write));
}

#[test]
fn volume_session_totals_saturate() {
    let mut state = DashboardState::default();
    state.update_volume_io(&volume_sample("disk0", Some(0), Some(0)), NOW);
    state.update_volume_io(
        &volume_sample("disk0", Some(u64::MAX), Some(u64::MAX)),
        NOW + 1,
    );
    state.update_volume_io(&volume_sample("disk0", Some(0), Some(0)), NOW + 2);
    state.update_volume_io(&volume_sample("disk0", Some(1), Some(2)), NOW + 3);
    assert_eq!(state.volume_session_read_bytes, Some(u64::MAX));
    assert_eq!(state.volume_session_write_bytes, Some(u64::MAX));
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
    let mut sample = common::sample_fixture();
    let mut state = DashboardState::default();
    for step in 0..200 {
        sample.compressed_bytes = Some(step as u64 * 536_870_912);
        state.update_frame(&view, &sample, NOW + step);
    }
    assert_eq!(state.output_history.len(), HISTORY_CAP);
    assert_eq!(state.gpu_history.len(), HISTORY_CAP);
    assert_eq!(state.cpu_history.len(), HISTORY_CAP);
    assert_eq!(state.cache_read_history.len(), HISTORY_CAP);
    assert_eq!(state.cache_write_history.len(), HISTORY_CAP);
    assert_eq!(state.compressed_history.len(), HISTORY_CAP);
    assert_eq!(
        state.compressed_history.front(),
        Some(&((200 - HISTORY_CAP) as f64 / 2.0))
    );
    assert_eq!(state.compressed_history.back(), Some(&99.5));
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
    assert!(state.compressed_history.is_empty());
    assert!(state.swap_history.is_empty());
    assert!(state.output_history.is_empty());
    assert!(state.core_history.is_empty());
}

#[test]
fn compressed_history_records_zero_and_skips_missing_samples() {
    let view = parse_status(None);
    let mut state = DashboardState::default();
    let mut sample = SystemSample {
        compressed_bytes: Some(0),
        ..SystemSample::default()
    };
    state.update_frame(&view, &sample, NOW);
    sample.compressed_bytes = Some(3_489_660_928);
    state.update_frame(&view, &sample, NOW + 1);
    sample.compressed_bytes = None;
    state.update_frame(&view, &sample, NOW + 2);
    assert_eq!(
        state.compressed_history.iter().copied().collect::<Vec<_>>(),
        vec![0.0, 3.25]
    );
}

#[test]
fn fixture_memory_gauges_are_unsigned_bytes_and_count_as_telemetry() {
    for (json, compressed, wired, unavailable) in [
        (r#"{"compressed_bytes":0}"#, Some(0), None, false),
        (r#"{"wired_bytes":0}"#, None, Some(0), false),
        (
            r#"{"compressed_bytes":3221225472,"wired_bytes":5368709120,"page_size":4096}"#,
            Some(3_221_225_472),
            Some(5_368_709_120),
            false,
        ),
        (r#"{}"#, None, None, true),
        (
            r#"{"compressed_bytes":null,"wired_bytes":null}"#,
            None,
            None,
            true,
        ),
        (
            r#"{"compressed_bytes":-1,"wired_bytes":-1}"#,
            None,
            None,
            true,
        ),
        (
            r#"{"compressed_bytes":1.5,"wired_bytes":1.5}"#,
            None,
            None,
            true,
        ),
        (
            r#"{"compressed_bytes":"1","wired_bytes":"1"}"#,
            None,
            None,
            true,
        ),
    ] {
        let sample = common::Harness::from_fixture(None, json).sample;
        assert_eq!(sample.compressed_bytes, compressed, "{json}");
        assert_eq!(sample.wired_bytes, wired, "{json}");
        assert_eq!(sample.telemetry_unavailable, unavailable, "{json}");
    }
}
