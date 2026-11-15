//! Layout suites: every supported terminal size, panel titles, the core table
//! and the rules about what must never appear.

mod common;

use common::{sample_fixture, BASE_STATUS, FIXTURE_SAMPLE};
use splash_monitor::render;
use splash_monitor::system::SystemSample;
use splash_monitor::theme::Theme;

/// Full layout sizes, compact profiles, and their exact transition boundaries.
const SIZES: [(usize, usize); 19] = [
    (140, 42),
    (120, 36),
    (101, 31),
    (100, 30),
    (99, 30),
    (86, 27),
    (80, 14),
    (72, 20),
    (70, 21),
    (66, 24),
    (66, 20),
    (65, 20),
    (64, 15),
    (64, 14),
    (64, 13),
    (64, 12),
    (50, 12),
    (40, 12),
    (36, 10),
];

const INFERENCE: &str = "[SPLASH] INFERENCE / REQUESTS";
const HARDWARE: &str = "[LOCAL] CPU / GPU";
const CACHE: &str = "[SPLASH] CACHE / KV SPILL";
const PRESSURE: &str = "[LOCAL] PRESSURE / SWAP";
const MEMORY: &str = "[SPLASH] MEMORY / CONFIG";
const STATUS: &str = "[SPLASH] STATUS";
const ACTIVITY: &str = "[SPLASH] ACTIVITY";
const SPILL: &str = "[SPLASH] KV SPILL";

fn fixture_harness() -> common::Harness {
    common::Harness::from_fixture(Some(BASE_STATUS), FIXTURE_SAMPLE)
}

fn distinct_harness() -> common::Harness {
    let body = BASE_STATUS.replace(
        "\"thermal_state\": \"nominal\"",
        "\"thermal_state\": \"critical\"",
    );
    let mut sample = sample_fixture();
    sample.pressure_level = Some(2);
    common::Harness::with(Some(&body), sample)
}

/// `grid_rows` omits blank screen rows, so locate borders by content rather
/// than treating indices as terminal y coordinates. Slice columns first so a
/// neighboring panel can never satisfy an assertion.
fn panel(rows: &[String], x: usize, width: usize, title: &str) -> String {
    let slices: Vec<String> = rows
        .iter()
        .map(|row| row.chars().skip(x - 1).take(width).collect())
        .collect();
    let start = slices
        .iter()
        .position(|row| row.contains(title))
        .unwrap_or_else(|| panic!("panel {title:?} missing in:\n{}", rows.join("\n")));
    assert!(
        slices[start].starts_with('┌') && slices[start].ends_with('┐'),
        "broken top border for {title}: {}",
        slices[start]
    );
    let end = slices
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, row)| row.starts_with('└'))
        .map(|(index, _)| index)
        .unwrap_or_else(|| panic!("bottom border missing for {title}"));
    assert!(
        slices[end].ends_with('┘'),
        "broken bottom border for {title}: {}",
        slices[end]
    );
    for row in &slices[start + 1..end] {
        assert!(
            row.starts_with('│') && row.ends_with('│'),
            "broken side border for {title}: {row}"
        );
    }
    slices[start..=end].join("\n")
}

struct Panels {
    splash: Vec<(&'static str, String)>,
    local: Vec<(&'static str, String)>,
}

impl Panels {
    fn splash(&self, title: &str) -> &str {
        self.splash
            .iter()
            .find(|(name, _)| *name == title)
            .unwrap_or_else(|| panic!("no Splash panel {title}"))
            .1
            .as_str()
    }

    fn local(&self, title: &str) -> &str {
        self.local
            .iter()
            .find(|(name, _)| *name == title)
            .unwrap_or_else(|| panic!("no local panel {title}"))
            .1
            .as_str()
    }
}

fn source_panels(rows: &[String], cols: usize, height: usize) -> Panels {
    let x = 2;
    let width = cols - 2;
    let mut splash = Vec::new();
    let mut local = Vec::new();
    if cols >= 100 && height >= 30 {
        let right_width = (width - 2) / 2;
        let left_width = width - 2 - right_width;
        let right_x = x + left_width + 2;
        splash.push((INFERENCE, panel(rows, x, left_width, INFERENCE)));
        splash.push((CACHE, panel(rows, x, left_width, CACHE)));
        splash.push((MEMORY, panel(rows, x, width, MEMORY)));
        local.push((HARDWARE, panel(rows, right_x, right_width, HARDWARE)));
        local.push((PRESSURE, panel(rows, right_x, right_width, PRESSURE)));
    } else if cols >= 66 {
        let left_width = (width - 2) / 2;
        let right_width = width - left_width - 2;
        let right_x = x + left_width + 2;
        splash.push((STATUS, panel(rows, x, left_width, STATUS)));
        splash.push((ACTIVITY, panel(rows, x, width, ACTIVITY)));
        if height >= 20 {
            splash.push((SPILL, panel(rows, x, width, SPILL)));
        }
        local.push((HARDWARE, panel(rows, right_x, right_width, HARDWARE)));
        local.push((PRESSURE, panel(rows, x, width, PRESSURE)));
    } else {
        splash.push((ACTIVITY, panel(rows, x, width, ACTIVITY)));
        local.push((HARDWARE, panel(rows, x, width, HARDWARE)));
        local.push((PRESSURE, panel(rows, x, width, PRESSURE)));
    }
    Panels { splash, local }
}

fn assert_no_source_leaks(panels: &Panels, cols: usize, rows: usize) {
    for (title, text) in &panels.splash {
        for local_value in [
            "62.0%",
            "14.8%",
            "58.4°C",
            "79.0°C",
            "12.7 W",
            "6.78 GiB",
            "WARNING",
            "disk3",
            "all processes",
            "Swap",
        ] {
            assert!(
                !text.contains(local_value),
                "{local_value} leaked into {title} at {cols}x{rows}:\n{text}"
            );
        }
    }
    for (title, text) in &panels.local {
        for splash_value in [
            "Thermal",
            "critical",
            "1048576",
            "KV ",
            "Metal",
            "41.02",
            "42.00 GiB",
            "IQ3_XXS",
            "ONLINE",
            "OFFLINE",
            "Headroom",
        ] {
            assert!(
                !text.contains(splash_value),
                "{splash_value} leaked into {title} at {cols}x{rows}:\n{text}"
            );
        }
    }
}

fn prefixed_row<'a>(rows: &'a [String], source: &str, label: &str) -> &'a str {
    rows.iter()
        .map(|row| row.trim_start())
        .find(|row| row.starts_with(source) && row.contains(label))
        .unwrap_or_else(|| panic!("missing {source} {label} row:\n{}", rows.join("\n")))
}

#[test]
fn source_ownership_survives_size_boundaries_and_no_color() {
    let harness = distinct_harness();
    for (cols, height) in SIZES {
        for color in [true, false] {
            let raw = harness.render(cols, height, color);
            let rows = common::grid_rows(&raw, cols, height);
            if height < 12 {
                let metal = prefixed_row(&rows, "[SPLASH]", "Metal");
                let swap = prefixed_row(&rows, "[LOCAL]", "Swap");
                let pressure = prefixed_row(&rows, "[LOCAL]", "WARNING");
                assert_eq!(metal, "[SPLASH] Metal 41.02/42.00 GiB");
                assert_eq!(swap, "[LOCAL] Swap 6.78 GiB");
                assert_eq!(pressure, "[LOCAL] WARNING | OUT -- MiB/s");
                assert_eq!(prefixed_row(&rows, "[SPLASH]", "ONLINE"), "[SPLASH] ONLINE");
                continue;
            }

            let panels = source_panels(&rows, cols, height);
            assert_no_source_leaks(&panels, cols, height);
            assert!(panels.local(HARDWARE).contains("62.0%"), "{cols}x{height}");
            assert!(panels.local(HARDWARE).contains("58.4°C"), "{cols}x{height}");
            if height >= 14 {
                assert!(panels.local(HARDWARE).contains("14.8%"), "{cols}x{height}");
                assert!(panels.local(HARDWARE).contains("79.0°C"), "{cols}x{height}");
            }
            let pressure = panels.local(PRESSURE);
            assert!(pressure.contains("WARNING"), "{cols}x{height}:\n{pressure}");
            assert!(pressure.contains("6.78 GiB"), "{cols}x{height}:\n{pressure}");

            if cols >= 100 && height >= 30 {
                assert!(panels.splash(INFERENCE).contains("Thermal critical"));
                assert!(panels.splash(CACHE).contains("KV Spill 278"));
                assert!(panels.splash(MEMORY).contains("KV cap 139264"));
                assert!(pressure.contains("disk3") && pressure.contains("all processes"));
            } else if cols >= 66 {
                if height >= 14 {
                    assert!(panels.splash(STATUS).contains("Thermal critical"));
                }
                if height >= 20 {
                    assert!(panels.splash(SPILL).contains("1048576"));
                }
            }
        }
    }
}

#[test]
fn full_layout_keeps_each_metric_group_in_its_source_panel() {
    let harness = distinct_harness();
    for (cols, height) in SIZES
        .iter()
        .copied()
        .filter(|(cols, height)| *cols >= 100 && *height >= 30)
    {
        let rows = harness.visible_rows(cols, height);
        let panels = source_panels(&rows, cols, height);
        let header = panel(&rows, 2, cols - 2, "SPLASH MONITOR");
        assert!(header.contains("STANDBY") && header.contains("IQ3_XXS"), "{header}");
        for expected in [
            "Decode",
            "Prefill",
            "Active 0",
            "Queued 0",
            "Waiting 0",
            "Phase",
            "Gap p95 131.1 ms",
            "Thermal critical",
            "Headroom",
            "denied 3751",
            "TTFT p95",
        ] {
            assert!(panels.splash(INFERENCE).contains(expected), "{expected} at {cols}x{height}:\n{}", panels.splash(INFERENCE));
        }
        for expected in [
            "Read",
            "Write",
            "Used 26.00/32.00 GiB",
            "Hits 342",
            "Restores 3924",
            "KV Spill 278",
            "Pending writes 2",
            "hit tokens 125568",
        ] {
            assert!(panels.splash(CACHE).contains(expected), "{expected} at {cols}x{height}:\n{}", panels.splash(CACHE));
        }
        let hardware = panels.local(HARDWARE);
        for expected in ["GPU", "CPU", "62.0%", "14.8%", "58.4°C", "79.0°C", "GPU power 12.7 W"] {
            assert!(hardware.contains(expected), "{expected} at {cols}x{height}:\n{hardware}");
        }
        for expected in [
            "Pressure WARNING",
            "Free 30%",
            "Swap 6.78 GiB",
            "IN -- MiB/s",
            "OUT -- MiB/s",
            "Swap OUT history",
            "disk3",
            "all processes",
            "R -- / W -- MiB/s",
        ] {
            assert!(panels.local(PRESSURE).contains(expected), "{expected} at {cols}x{height}:\n{}", panels.local(PRESSURE));
        }
        for expected in [
            "Metal 41.02/42.00 GiB",
            "Weights released true",
            "Context 262144",
            "RAM bound 139257",
            "Cache quota 32.00 GiB",
            "KV cap 139264",
        ] {
            assert!(panels.splash(MEMORY).contains(expected), "{expected} at {cols}x{height}:\n{}", panels.splash(MEMORY));
        }
    }
}

#[test]
fn compact_height_profiles_prioritize_thermal_and_retain_narrow_values() {
    let harness = distinct_harness();
    for (cols, height) in [(66, 14), (66, 20), (66, 24)] {
        let rows = harness.visible_rows(cols, height);
        let panels = source_panels(&rows, cols, height);
        let status = panels.splash(STATUS);
        assert!(status.contains("ONLINE") && status.contains("IQ3_XXS"), "{status}");
        assert!(status.contains("Thermal critical"), "{cols}x{height}:\n{status}");
        assert_eq!(status.contains("Weights"), height >= 20, "{cols}x{height}");
        assert_eq!(status.contains("idle 600s"), height >= 24, "{cols}x{height}");
        let hardware = panels.local(HARDWARE);
        assert_eq!(
            hardware.contains("System-wide sensors"),
            height >= 20,
            "{cols}x{height}:\n{hardware}"
        );
        assert_eq!(
            hardware.contains("Load avg"),
            height >= 24,
            "{cols}x{height}:\n{hardware}"
        );
        assert_eq!(
            panels.local(PRESSURE).contains("Swap history"),
            height >= 20,
            "{cols}x{height}"
        );
    }
    for height in 12..=15 {
        let rows = harness.visible_rows(64, height);
        let panels = source_panels(&rows, 64, height);
        let hardware = panels.local(HARDWARE);
        assert!(hardware.contains("GPU 62.0%"), "64x{height}:\n{hardware}");
        assert_eq!(hardware.contains("CPU 14.8%"), height >= 14, "64x{height}");
        assert_eq!(
            panels.splash(ACTIVITY).contains("41.02/42.00 GiB"),
            height >= 13,
            "64x{height}:\n{}",
            panels.splash(ACTIVITY)
        );
    }
}

#[test]
fn expanded_compact_splash_details_remain_visible() {
    let harness = distinct_harness();
    let rows = harness.visible_rows(86, 27);
    let panels = source_panels(&rows, 86, 27);
    let activity = panels.splash(ACTIVITY);
    for expected in [
        "41.02/42.00 GiB",
        "Context 262144",
        "Gap p95 131.1ms",
        "Spill 1048576 pages",
        "RAM bound 139257",
    ] {
        assert!(activity.contains(expected), "{expected} missing:\n{activity}");
    }
    let spill = panels.splash(SPILL);
    for expected in [
        "Requests 278",
        "Resident 1",
        "Pages 1048576",
        "Spilled ",
        "Writes ",
        "Reads ",
    ] {
        assert!(spill.contains(expected), "{expected} missing:\n{spill}");
    }
    let pressure = panels.local(PRESSURE);
    assert!(pressure.contains("Swap history"), "{pressure}");
    assert!(pressure.contains("Swap amount is historical"), "{pressure}");
}

#[test]
fn unavailable_cache_does_not_hide_independent_splash_kv_metrics() {
    let mut body: serde_json::Value = serde_json::from_str(BASE_STATUS).unwrap();
    body.as_object_mut().unwrap().remove("disk");
    let body = body.to_string();
    let harness = common::Harness::with(Some(&body), sample_fixture());
    let rows = harness.visible_rows(100, 30);
    let panels = source_panels(&rows, 100, 30);
    let cache = panels.splash(CACHE);
    assert!(cache.contains("Splash cache metrics unavailable"), "{cache}");
    assert!(cache.contains("Hits 342"), "{cache}");
    assert!(cache.contains("KV Spill 278"), "{cache}");
    assert!(cache.contains("Pending writes --"), "{cache}");
}

#[test]
fn normal_fixture_pressure_remains_local() {
    let harness = fixture_harness();
    for (cols, height) in [(100, 30), (72, 20), (64, 15), (36, 10)] {
        let rows = harness.visible_rows(cols, height);
        if height < 12 {
            assert!(prefixed_row(&rows, "[LOCAL]", "NORMAL").contains("OUT -- MiB/s"));
        } else {
            let panels = source_panels(&rows, cols, height);
            assert!(panels.local(PRESSURE).contains("NORMAL"));
            for (title, text) in &panels.splash {
                assert!(!text.contains("NORMAL"), "local pressure leaked into {title}");
            }
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
    let harness = fixture_harness();
    for (cols, height) in SIZES {
        let rows = harness.visible_rows(cols, height);
        let text = rows.join("\n");
        if height >= 12 {
            panel(&rows, 2, cols - 2, "SPLASH MONITOR");
        }
        assert_eq!(
            text.matches("q: quit").count(),
            1,
            "quit hint count at {cols}x{height}"
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
fn core_table_covers_every_core_when_it_fits() {
    let harness = fixture_harness();
    for (cols, height) in [(120, 36), (140, 42)] {
        let rows = harness.visible_rows(cols, height);
        let panels = source_panels(&rows, cols, height);
        let hardware = panels.local(HARDWARE);
        for core in 0..18 {
            assert!(
                hardware.contains(&format!("C{core} ")),
                "core C{core} missing at {cols}x{height}:\n{hardware}"
            );
        }
        assert!(
            hardware.contains("Sensor-to-core mapping is proportional"),
            "caveat line missing at {cols}x{height}:\n{hardware}"
        );
    }
}

#[test]
fn core_table_falls_back_to_a_summary_row_when_it_cannot_fit() {
    let harness = fixture_harness();
    let rows = harness.visible_rows(100, 30);
    let panels = source_panels(&rows, 100, 30);
    let hardware = panels.local(HARDWARE);
    assert!(!hardware.contains("C17 "), "truncated table: {hardware}");
    assert!(
        hardware.contains("Cores") && hardware.contains("(14 sensors)"),
        "summary row missing: {hardware}"
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
        let decoded = common::grid_rows(&raw, cols, rows);
        let panels = source_panels(&decoded, cols, rows);
        let hardware = panels.local(HARDWARE);
        assert!(
            hardware.contains("Sensors disabled (--no-sensors)"),
            "hint missing at {cols}x{rows}:\n{hardware}"
        );
        assert!(
            hardware.contains("--%"),
            "missing hardware percent at {cols}x{rows}:\n{hardware}"
        );
    }
    // Compact frames have no line left for the hint but must still show `--`.
    for (cols, rows) in [(86, 27), (80, 14), (64, 15)] {
        let raw = empty.render_with_sensors(cols, rows, false);
        let decoded = common::grid_rows(&raw, cols, rows);
        let panels = source_panels(&decoded, cols, rows);
        assert!(
            panels.local(HARDWARE).contains("--%"),
            "missing hardware percent at {cols}x{rows}"
        );
    }
}

#[test]
fn gpu_gap_is_described_by_the_gpu_hint() {
    let mut sample = sample_fixture();
    sample.gpu_active_ratio = None;
    let harness = common::Harness::with(Some(BASE_STATUS), sample);
    let rows = harness.visible_rows(140, 42);
    let panels = source_panels(&rows, 140, 42);
    let hardware = panels.local(HARDWARE);
    assert!(hardware.contains("GPU utilization unavailable on this macOS"));
    assert!(hardware.contains("--%"));
}

#[test]
fn unresolved_device_probe_is_labelled() {
    let mut sample = sample_fixture();
    sample.volume = None;
    sample.device_unresolved = Some("/var/folders/tmp".to_string());
    let harness = common::Harness::with(Some(BASE_STATUS), sample);
    let rows = harness.visible_rows(140, 42);
    let panels = source_panels(&rows, 140, 42);
    assert!(
        panels.local(HARDWARE).contains("Device I/O unresolved for /var/folders/tmp")
    );
    assert!(!panels.local(PRESSURE).contains("all processes"));
}

#[test]
fn resolved_device_is_labelled_device_wide() {
    let harness = fixture_harness();
    let rows = harness.visible_rows(140, 42);
    let panels = source_panels(&rows, 140, 42);
    let pressure = panels.local(PRESSURE);
    assert!(pressure.contains("disk3"), "device row missing: {pressure}");
    assert!(pressure.contains("all processes"), "device-wide label missing: {pressure}");
    assert!(!panels.splash(CACHE).contains("disk3"));
}

#[test]
fn the_narrowest_full_layout_never_clips_a_label_mid_word() {
    let harness = fixture_harness();
    let rows = harness.visible_rows(100, 30);
    let panels = source_panels(&rows, 100, 30);
    let prefill = panels
        .splash(INFERENCE)
        .lines()
        .find(|row| row.contains("Prefill"))
        .expect("prefill label");
    assert!(
        prefill.contains("tok/s"),
        "clipped prefill label: {prefill}"
    );
    let transferred = panels
        .splash(CACHE)
        .lines()
        .find(|row| row.contains("/ W "))
        .expect("transfer totals");
    let body = transferred.replace('│', " ");
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
fn offline_server_does_not_erase_live_local_telemetry() {
    let mut sample = sample_fixture();
    sample.pressure_level = Some(2);
    let harness = common::Harness::with(None, sample);
    for (cols, height) in [(100, 30), (72, 20), (64, 15), (36, 10)] {
        for color in [true, false] {
            let rows = common::grid_rows(&harness.render(cols, height, color), cols, height);
            if height < 12 {
                assert!(prefixed_row(&rows, "[SPLASH]", "OFFLINE").contains("OFFLINE"));
                assert!(prefixed_row(&rows, "[SPLASH]", "Metal").contains("--/-- GiB"));
                assert!(prefixed_row(&rows, "[LOCAL]", "Swap").contains("6.78 GiB"));
                assert!(prefixed_row(&rows, "[LOCAL]", "WARNING").contains("OUT -- MiB/s"));
                continue;
            }
            let header = panel(&rows, 2, cols - 2, "SPLASH MONITOR");
            assert!(header.contains("OFFLINE"), "{cols}x{height}:\n{header}");
            let panels = source_panels(&rows, cols, height);
            assert_no_source_leaks(&panels, cols, height);
            let hardware = panels.local(HARDWARE);
            let pressure = panels.local(PRESSURE);
            assert!(hardware.contains("62.0%") && hardware.contains("14.8%"), "{hardware}");
            assert!(pressure.contains("WARNING") && pressure.contains("6.78 GiB"), "{pressure}");
            if cols >= 100 {
                assert!(panels.splash(MEMORY).contains("--/-- GiB"));
                assert!(pressure.contains("disk3") && pressure.contains("all processes"));
            } else {
                assert!(panels.splash(ACTIVITY).contains("--/-- GiB"));
                if cols >= 66 {
                    assert!(panels.splash(STATUS).contains("OFFLINE"));
                }
            }
        }
    }
}

#[test]
fn missing_local_probes_do_not_erase_splash_data() {
    let body = BASE_STATUS.replace(
        "\"thermal_state\": \"nominal\"",
        "\"thermal_state\": \"critical\"",
    );
    let harness = common::Harness::with(Some(&body), SystemSample::default());
    for (cols, height) in [(100, 30), (72, 20), (64, 15), (36, 10)] {
        let rows = harness.visible_rows(cols, height);
        if height < 12 {
            assert!(prefixed_row(&rows, "[SPLASH]", "Metal").contains("41.02/42.00 GiB"));
            assert!(prefixed_row(&rows, "[LOCAL]", "Swap").contains("-- GiB"));
            assert!(prefixed_row(&rows, "[LOCAL]", "UNKNOWN").contains("OUT -- MiB/s"));
            continue;
        }
        let panels = source_panels(&rows, cols, height);
        let hardware = panels.local(HARDWARE);
        let pressure = panels.local(PRESSURE);
        assert!(hardware.contains("--%"), "{cols}x{height}:\n{hardware}");
        assert!(pressure.contains("UNKNOWN") && pressure.contains("Swap -- GiB"), "{pressure}");
        if cols >= 100 {
            assert!(panels.splash(INFERENCE).contains("Thermal critical"));
            assert!(panels.splash(CACHE).contains("KV Spill 278"));
            assert!(panels.splash(MEMORY).contains("41.02/42.00 GiB"));
        } else {
            assert!(panels.splash(ACTIVITY).contains("41.02/42.00 GiB"));
            if cols >= 66 {
                assert!(panels.splash(STATUS).contains("Thermal critical"));
                assert!(panels.splash(SPILL).contains("1048576"));
            }
        }
    }
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

#[test]
fn gateway_readings_keep_their_source_labels_at_all_sizes() {
    let harness = common::Harness::gateway(Some(common::GATEWAY_STATS), sample_fixture());
    for (cols, height, expected) in [
        (140, 42, vec!["[GATEWAY] INFERENCE / REQUESTS", "[GATEWAY] CACHE / KV SPILL", "[GATEWAY] MEMORY / CONFIG"]),
        (80, 24, vec!["[GATEWAY] STATUS", "[GATEWAY] ACTIVITY", "[GATEWAY] KV SPILL"]),
        (64, 15, vec!["[GATEWAY] ACTIVITY"]),
        (36, 10, vec!["[GATEWAY] Metal", "[GATEWAY] ONLINE"]),
    ] {
        let text = harness.visible_text(cols, height);
        for label in expected {
            assert!(text.contains(label), "{label} missing at {cols}x{height}: {text}");
        }
        assert!(!text.contains("[SPLASH]"), "{cols}x{height}: {text}");
        assert!(text.contains("[LOCAL]"), "{cols}x{height}: {text}");
    }
}
