//! Layout suites: every supported terminal size, panel titles, the core table
//! and the rules about what must never appear.

mod common;

use common::{sample_fixture, BASE_STATUS, FIXTURE_SAMPLE};
use splash_monitor::render;
use splash_monitor::system::SystemSample;
use splash_monitor::theme::Theme;

/// Full layout sizes and the compact sizes the previous suite covered.
const SIZES: [(usize, usize); 11] = [
    (140, 42),
    (120, 36),
    (101, 31),
    (86, 27),
    (80, 14),
    (72, 20),
    (70, 21),
    (64, 15),
    (50, 12),
    (40, 12),
    (36, 10),
];

fn fixture_harness() -> common::Harness {
    common::Harness::from_fixture(Some(BASE_STATUS), FIXTURE_SAMPLE)
}

#[test]
fn full_layout_shows_every_panel_and_value() {
    for (cols, rows) in SIZES
        .iter()
        .filter(|(cols, rows)| *cols >= 100 && *rows >= 30)
    {
        let harness = fixture_harness();
        let text = harness.visible_text(*cols, *rows);
        for expected in [
            "SPLASH MONITOR",
            "INFERENCE / THROUGHPUT",
            "HARDWARE / THERMAL",
            "SPLASH CACHE / I/O",
            "REQUESTS / PRESSURE",
            "MEMORY / CAPACITY / CONFIG",
            "STANDBY",
            "IQ3_XXS",
            "262144",
            "139257",
            "42.00 GiB",
            "6.78 GiB",
            "NORMAL",
            "Thermal nominal (Splash API)",
        ] {
            assert!(
                text.contains(expected),
                "{expected} missing at {cols}x{rows}"
            );
        }
    }
}

#[test]
fn compact_layout_uses_compact_titles() {
    for (cols, rows) in SIZES
        .iter()
        .filter(|(cols, rows)| *cols < 100 || *rows < 30)
    {
        let harness = fixture_harness();
        let text = harness.visible_text(*cols, *rows);
        if *rows >= 12 {
            assert!(
                text.contains("SPLASH MONITOR"),
                "compact header missing at {cols}x{rows}"
            );
            if *cols >= 66 {
                assert!(
                    text.contains("STATUS / INFERENCE"),
                    "compact panel titles missing at {cols}x{rows}"
                );
            }
            assert!(
                !text.contains("INFERENCE / THROUGHPUT")
                    && !text.contains("MEMORY / CAPACITY / CONFIG"),
                "full-layout panels leaked into compact at {cols}x{rows}"
            );
        } else {
            // Below 12 rows the layout falls back to a plain summary line.
            assert!(
                text.contains("SPLASH MONITOR"),
                "fallback header at {cols}x{rows}"
            );
        }
    }
}

#[test]
fn narrow_terminals_use_the_single_resource_panel() {
    for (cols, rows) in [(64, 15), (50, 12)] {
        let harness = fixture_harness();
        let text = harness.visible_text(cols, rows);
        assert!(text.contains("RESOURCE STATUS"), "{cols}x{rows}: {text}");
        assert!(
            !text.contains("STATUS / INFERENCE"),
            "two-panel layout used at {cols}x{rows}"
        );
    }
}

#[test]
fn compact_panels_appear_when_the_terminal_is_wide_enough() {
    for (cols, rows) in [(86, 27), (72, 20), (70, 21)] {
        let harness = fixture_harness();
        let text = harness.visible_text(cols, rows);
        // At 66+ columns the compact layout splits into STATUS + HARDWARE and
        // adds the KV spill panel from 20 rows up.
        for expected in [
            "STATUS / INFERENCE",
            "HARDWARE / THERMAL",
            "INFERENCE / CACHE ACTIVITY",
            "macOS PRESSURE / SWAP",
            "KV SPILL / DISK I/O",
        ] {
            assert!(
                text.contains(expected),
                "{expected} missing at {cols}x{rows}"
            );
        }
    }
}

#[test]
fn no_row_exceeds_the_terminal_width() {
    for (cols, rows) in SIZES {
        let harness = fixture_harness();
        for line in harness.visible_rows(cols, rows) {
            assert!(
                line.chars().count() <= cols,
                "row of {} chars at {cols}x{rows}: {line}",
                line.chars().count()
            );
        }
    }
}

#[test]
fn frames_draw_boxes_and_exactly_one_quit_hint() {
    for (cols, rows) in SIZES
        .iter()
        .filter(|(cols, rows)| *cols >= 64 && *rows >= 12)
    {
        let harness = fixture_harness();
        let text = harness.visible_text(*cols, *rows);
        assert!(text.contains('┌'), "no box corner at {cols}x{rows}");
        assert!(text.contains('┘'), "no box corner at {cols}x{rows}");
        assert_eq!(
            text.matches("q: quit").count(),
            1,
            "quit hint count at {cols}x{rows}"
        );
    }
}

#[test]
fn tiny_terminals_render_without_overflow() {
    for (cols, rows) in [(50, 12), (40, 12), (36, 10)] {
        let harness = fixture_harness();
        let rows_out = harness.visible_rows(cols, rows);
        assert!(!rows_out.is_empty(), "nothing drawn at {cols}x{rows}");
        for line in &rows_out {
            assert!(line.chars().count() <= cols);
        }
    }
}

#[test]
fn core_table_covers_every_core_at_the_large_size() {
    let harness = fixture_harness();
    let text = harness.visible_text(140, 42);
    for core in 0..18 {
        assert!(
            text.contains(&format!("C{core} ")),
            "core C{core} missing from the table"
        );
    }
    assert!(
        text.contains("Sensor-to-core mapping is proportional"),
        "caveat line missing"
    );
    // Core rows carry a utilization percentage.
    assert!(text.contains('%'));
}

#[test]
fn core_table_uses_a_plain_table_when_graphs_do_not_fit() {
    // 120x36 has room for label, utilization and temperature, not for graphs.
    let harness = fixture_harness();
    let text = harness.visible_text(120, 36);
    assert!(
        text.contains("C17 "),
        "core table missing at 120x36: {text}"
    );
    assert!(
        text.contains("Sensor-to-core mapping is proportional"),
        "caveat line missing at 120x36"
    );
}

#[test]
fn core_table_falls_back_to_a_summary_row_when_it_cannot_fit() {
    let harness = fixture_harness();
    let text = harness.visible_text(100, 30);
    assert!(!text.contains("C17 "), "table drawn where it cannot fit");
    assert!(
        text.contains("Cores") && text.contains("sensors)"),
        "summary row missing: {text}"
    );
}

#[test]
fn empty_probes_never_render_a_zero_temperature() {
    for (cols, rows) in SIZES {
        let harness = common::Harness::offline();
        let text = harness.visible_text(cols, rows);
        assert!(!text.contains("0°C"), "0°C drawn at {cols}x{rows}");
        assert!(text.contains("--"), "missing telemetry at {cols}x{rows}");
    }
}

#[test]
fn sensors_disabled_is_labelled_and_percentages_are_missing() {
    // `--no-sensors` leaves the sample empty: nothing may read as a value.
    let empty = common::Harness::with(Some(BASE_STATUS), SystemSample::default());
    // The hint line belongs to the full layout, as in the previous dashboard.
    for (cols, rows) in [(140, 42), (120, 36)] {
        let raw = empty.render_with_sensors(cols, rows, false);
        let text = common::grid_rows(&raw, cols, rows).join("\n");
        assert!(
            text.contains("Sensors disabled (--no-sensors)"),
            "hint missing at {cols}x{rows}"
        );
        assert!(
            text.contains("--%"),
            "missing hardware percent at {cols}x{rows}"
        );
    }
    // Compact frames have no line left for the hint but must still show `--`.
    for (cols, rows) in [(86, 27), (80, 14), (64, 15)] {
        let raw = empty.render_with_sensors(cols, rows, false);
        let text = common::grid_rows(&raw, cols, rows).join("\n");
        assert!(
            text.contains("--%"),
            "missing hardware percent at {cols}x{rows}"
        );
    }
}

#[test]
fn gpu_gap_is_described_by_the_gpu_hint() {
    let mut sample = sample_fixture();
    sample.gpu_active_ratio = None;
    let harness = common::Harness::with(Some(BASE_STATUS), sample);
    let text = harness.visible_text(140, 42);
    assert!(text.contains("GPU utilization unavailable on this macOS"));
    assert!(text.contains("--%"));
}

#[test]
fn unresolved_device_probe_is_labelled() {
    let mut sample = sample_fixture();
    sample.volume = None;
    sample.device_unresolved = Some("/var/folders/tmp".to_string());
    let harness = common::Harness::with(Some(BASE_STATUS), sample);
    let text = harness.visible_text(140, 42);
    assert!(text.contains("Device I/O unresolved for /var/folders/tmp"));
}

#[test]
fn resolved_device_is_labelled_device_wide() {
    let harness = fixture_harness();
    let text = harness.visible_text(140, 42);
    assert!(text.contains("disk3"), "device row missing");
    assert!(text.contains("all processes"), "device-wide label missing");
}

#[test]
fn the_narrowest_full_layout_never_clips_a_label_mid_word() {
    let harness = fixture_harness();
    let rows = harness.visible_rows(100, 30);
    let prefill = rows
        .iter()
        .find(|row| row.contains("Prefill"))
        .expect("prefill label");
    assert!(
        prefill.contains("tok/s"),
        "clipped prefill label: {prefill}"
    );
    let transferred = rows
        .iter()
        .find(|row| row.contains("/ W "))
        .expect("transfer totals");
    // Panel borders share the row with the label; they are not part of it.
    let body = transferred.replace(['│', '┌', '┐', '└', '┘', '─'], " ");
    assert!(
        body.trim_end().ends_with("GiB") || body.trim_end().ends_with(char::is_numeric),
        "clipped transfer totals: {transferred}"
    );
}

#[test]
fn dark_palette_supplies_the_default_true_colors() {
    let harness = fixture_harness();
    let out = harness.render(140, 42, true);
    // Panel titles and body text come from the palette, never from a literal.
    assert!(
        out.contains("\x1b[38;2;117;214;230m"),
        "title color missing"
    );
    assert!(
        out.contains("\x1b[38;2;223;230;236m"),
        "body text color missing"
    );
}

#[test]
fn light_palette_replaces_every_dark_token() {
    let harness = fixture_harness();
    let out = harness.render_with_theme(140, 42, true, &Theme::light());
    assert!(
        out.contains("\x1b[38;2;26;39;49m"),
        "light body text missing"
    );
    assert!(
        !out.contains("\x1b[38;2;223;230;236m"),
        "a dark-palette token survived the switch"
    );
    assert!(
        !out.contains("\x1b[38;2;117;214;230m"),
        "a dark-palette accent survived the switch"
    );
}

#[test]
fn no_color_output_has_no_color_escapes() {
    let harness = fixture_harness();
    let plain = render::render(&harness.frame(true), false, &Theme::dark(), 140, 42);
    // `--no-color` still moves the cursor; it must not emit a color SGR.
    assert!(
        !plain.contains("\x1b[38;2;"),
        "24-bit color in --no-color output"
    );
    let colored = harness.render(140, 42, true);
    assert!(
        colored.contains("\x1b[38;2;"),
        "color output lost its palette"
    );
}

#[test]
fn frame_protocol_starts_home_clear_and_ends_on_the_last_row() {
    let harness = fixture_harness();
    let out = harness.render(140, 42, true);
    assert!(out.starts_with("\x1b[H\x1b[J"), "frame must home and clear");
    assert!(out.ends_with("\x1b[42;1H"), "frame must park the cursor");
}

#[test]
fn obsolete_helper_messages_are_gone_everywhere() {
    for (cols, rows) in SIZES {
        for sensors in [true, false] {
            let harness = fixture_harness();
            let text = harness.render_with_sensors(cols, rows, sensors);
            for forbidden in [
                "Sensor helper missing",
                "splash-sensors",
                "bash sensors",
                "jq",
                "SPLASH_HOST must not",
            ] {
                assert!(
                    !text.contains(forbidden),
                    "{forbidden} appeared at {cols}x{rows}"
                );
            }
        }
    }
}

#[test]
fn offline_frame_reports_offline_not_zero() {
    let harness = common::Harness::offline();
    let text = harness.visible_text(140, 42);
    assert!(text.contains("OFFLINE"));
    assert!(text.contains("--%"));
    assert!(
        !text.contains("0.00 GiB"),
        "offline frame showed zero memory"
    );
}

#[test]
fn offline_empty_sample_and_fixture_agree_on_missing_shape() {
    let empty = common::Harness::with(Some(BASE_STATUS), SystemSample::default());
    let filled = fixture_harness();
    let empty_text = empty.visible_text(140, 42);
    let filled_text = filled.visible_text(140, 42);
    // The filled fixture shows a GPU percentage; the empty sample shows `--`.
    assert!(filled_text.contains("62.0%"));
    assert!(empty_text.contains("GPU utilization unavailable on this macOS"));
}

#[test]
fn gateway_frame_labels_the_source_not_offline() {
    let harness = common::Harness::gateway(Some(common::GATEWAY_STATS), sample_fixture());
    let text = harness.visible_text(140, 42);
    assert!(
        text.contains("GATEWAY"),
        "missing source label in full layout"
    );
    assert!(!text.contains("OFFLINE"), "gateway frame looked offline");
    assert!(text.contains("111.4"), "engine avg missing in full layout");

    let harness = common::Harness::gateway(Some(common::GATEWAY_STATS), sample_fixture());
    let text = harness.visible_text(80, 24);
    assert!(
        text.contains("GATEWAY"),
        "missing source label in compact layout"
    );
    assert!(
        !text.contains("OFFLINE"),
        "compact gateway frame looked offline"
    );
}
