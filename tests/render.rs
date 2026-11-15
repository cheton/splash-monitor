//! Layout suites: every supported terminal size, panel titles, the core table
//! and the rules about what must never appear.

mod common;

use common::{sample_fixture, BASE_STATUS, FIXTURE_SAMPLE};
use splash_monitor::render;
use splash_monitor::system::SystemSample;
use splash_monitor::theme::Theme;

/// Full layout sizes, compact profiles, and their exact transition boundaries.
const SIZES: [(usize, usize); 20] = [
    (140, 42),
    (120, 36),
    (101, 31),
    (100, 30),
    (100, 29),
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
const LOCAL_MEMORY: &str = "[LOCAL] MEMORY";
const DISK_IO: &str = "[LOCAL] DISK I/O";
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

/// Keep blank physical rows when decoding cursor-addressed output. Column 1 is
/// outside the dashboard boxes, so a marker there does not change their cells.
fn physical_rows(raw: &str, cols: usize, height: usize) -> Vec<String> {
    let mut marked = raw.to_string();
    for row in 1..=height {
        marked.push_str(&format!("\x1b[{row};1H~"));
    }
    let rows = common::grid_rows(&marked, cols, height);
    assert_eq!(rows.len(), height);
    rows
}

fn assert_rectangle(
    rows: &[String],
    x: usize,
    top: usize,
    width: usize,
    height: usize,
    title: &str,
) {
    let slice = |y: usize| -> String {
        rows[y - 1].chars().skip(x - 1).take(width).collect()
    };
    let upper = slice(top);
    assert_eq!(upper.chars().count(), width, "short top border: {upper}");
    assert!(
        upper.starts_with('┌') && upper.ends_with('┐') && upper.contains(title),
        "broken top border for {title} at row {top}: {upper}"
    );
    for y in top + 1..top + height - 1 {
        let side = slice(y);
        assert_eq!(side.chars().count(), width, "short row {y}: {side}");
        assert!(
            side.starts_with('│') && side.ends_with('│'),
            "broken side border for {title} at row {y}: {side}"
        );
    }
    let bottom_row = top + height - 1;
    let lower = slice(bottom_row);
    assert_eq!(lower.chars().count(), width, "short bottom border: {lower}");
    assert!(
        lower.starts_with('└')
            && lower.ends_with('┘')
            && lower.chars().skip(1).take(width - 2).all(|ch| ch == '─'),
        "broken bottom border for {title} at row {bottom_row}: {lower}"
    );
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
        splash.push((MEMORY, panel(rows, x, left_width, MEMORY)));
        splash.push((CACHE, panel(rows, x, left_width, CACHE)));
        local.push((HARDWARE, panel(rows, right_x, right_width, HARDWARE)));
        local.push((LOCAL_MEMORY, panel(rows, right_x, right_width, LOCAL_MEMORY)));
        local.push((DISK_IO, panel(rows, right_x, right_width, DISK_IO)));
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
        local.push((LOCAL_MEMORY, panel(rows, x, width, LOCAL_MEMORY)));
    } else {
        splash.push((ACTIVITY, panel(rows, x, width, ACTIVITY)));
        local.push((HARDWARE, panel(rows, x, width, HARDWARE)));
        local.push((LOCAL_MEMORY, panel(rows, x, width, LOCAL_MEMORY)));
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
            "Compressed ",
            "Wired ",
            "Free index",
            "Device",
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
        if *title != LOCAL_MEMORY {
            for value in ["Compressed ", "Wired ", "Free index", "Swap used"] {
                assert!(
                    !text.contains(value),
                    "{value} leaked into {title} at {cols}x{rows}:\n{text}"
                );
            }
        }
        if *title != DISK_IO {
            for value in ["Device", "disk3", "all processes"] {
                assert!(
                    !text.contains(value),
                    "{value} leaked into {title} at {cols}x{rows}:\n{text}"
                );
            }
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
                let text = rows.join("\n");
                assert!(!text.contains(DISK_IO) && !text.contains("Compressed"));
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
            let memory = panels.local(LOCAL_MEMORY);
            assert!(memory.contains("WARNING"), "{cols}x{height}:\n{memory}");
            assert!(memory.contains("6.78 GiB"), "{cols}x{height}:\n{memory}");
            let gauges_visible = (cols >= 100 && height >= 30) || (cols >= 66 && height >= 24);
            for gauge in ["Compressed 3.00 GiB", "Wired 5.00 GiB"] {
                assert_eq!(
                    memory.contains(gauge),
                    gauges_visible,
                    "{gauge} visibility at {cols}x{height}:\n{memory}"
                );
            }

            if cols >= 100 && height >= 30 {
                assert!(panels.splash(INFERENCE).contains("Thermal critical"));
                assert!(panels.splash(CACHE).contains("KV Spill 278"));
                assert!(panels.splash(MEMORY).contains("KV cap 139264"));
                let disk = panels.local(DISK_IO);
                assert!(disk.contains("disk3") && disk.contains("all processes"), "{disk}");
            } else {
                assert!(!rows.join("\n").contains(DISK_IO), "{cols}x{height}");
                if cols >= 66 {
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
            "26.00/32.00 GiB",
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
        let memory = panels.local(LOCAL_MEMORY);
        for expected in [
            "Pressure WARNING | Free index 30%",
            "Compressed 3.00 GiB | Wired 5.00 GiB",
            "Swap used 6.78 GiB",
            "IN -- MiB/s",
            "OUT -- MiB/s",
            "Compressed history (GiB, auto)",
        ] {
            assert!(memory.contains(expected), "{expected} at {cols}x{height}:\n{memory}");
        }
        let disk = panels.local(DISK_IO);
        for expected in ["Device disk3 (all processes)", "Read -- MiB/s", "Write -- MiB/s"] {
            assert!(disk.contains(expected), "{expected} at {cols}x{height}:\n{disk}");
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
fn swap_and_device_rates_keep_their_units_and_sources() {
    let mut harness = fixture_harness();
    harness.rates.swap_in_mib = Some(12_345.67);
    harness.rates.swap_out_mib = Some(98.76);
    harness.rates.volume_read_mib = Some(12.34);
    harness.rates.volume_write_mib = Some(56.78);
    for (cols, height) in [(100, 30), (140, 42)] {
        let rows = harness.visible_rows(cols, height);
        let panels = source_panels(&rows, cols, height);
        let memory = panels.local(LOCAL_MEMORY);
        let rates = memory.lines().nth(4).unwrap();
        assert!(rates.contains("IN 12345.67 MiB/s") && rates.contains("OUT 98.76 MiB/s"), "{memory}");
        assert!(!memory.contains("12.34") && !memory.contains("56.78"), "{memory}");
        let disk = panels.local(DISK_IO);
        assert!(disk.contains("Read 12.34 MiB/s") && disk.contains("Write 56.78 MiB/s"), "{disk}");
        assert!(!disk.contains("12345.67") && !disk.contains("98.76"), "{disk}");
    }
}

#[test]
fn disk_panel_shows_rate_histories_and_observed_session_totals() {
    let mut harness = fixture_harness();
    harness.rates.volume_read_mib = Some(2.0);
    harness.rates.volume_write_mib = Some(3.0);
    harness.state.volume_read_history = [0.0, 2.0].into_iter().collect();
    harness.state.volume_write_history = [3.0, 0.0].into_iter().collect();
    harness.state.volume_session_read_bytes = Some(1_342_177_280);
    harness.state.volume_session_write_bytes = Some(536_870_912);
    for (cols, height) in [(100, 30), (140, 42)] {
        let rows = harness.visible_rows(cols, height);
        let panels = source_panels(&rows, cols, height);
        let disk = panels.local(DISK_IO);
        assert!(disk.contains("Read 2.00 MiB/s"), "{disk}");
        assert!(disk.contains("Write 3.00 MiB/s"), "{disk}");
        assert!(disk.contains("▁█") && disk.contains("█▁"), "{disk}");
        assert!(disk.contains("Session R 1.25 / W 0.50 GiB"), "{disk}");
    }
    harness.rates.volume_read_mib = None;
    harness.rates.volume_write_mib = None;
    let rows = harness.visible_rows(100, 30);
    let panels = source_panels(&rows, 100, 30);
    let disk = panels.local(DISK_IO);
    assert!(!disk.contains("▁█") && !disk.contains("█▁"), "{disk}");
}

#[test]
fn memory_panels_keep_each_metric_on_its_own_row() {
    let harness = distinct_harness();
    for (cols, height) in [(100, 30), (140, 42)] {
        let rows = harness.visible_rows(cols, height);
        let panels = source_panels(&rows, cols, height);
        let server = panels.splash(MEMORY);
        let server_rows: Vec<&str> = server.lines().collect();
        assert_eq!(server_rows.len(), 6, "{server}");
        for (index, expected) in [
            (1, "Metal 41.02/42.00 GiB"),
            (2, "Weights released true"),
            (3, "Context 262144 | RAM bound 139257"),
            (4, "Cache quota 32.00 GiB | KV cap 139264"),
        ] {
            assert!(server_rows[index].contains(expected), "{expected} at {cols}x{height}:\n{server}");
        }
        let memory = panels.local(LOCAL_MEMORY);
        let memory_rows: Vec<&str> = memory.lines().collect();
        assert_eq!(memory_rows.len(), (height - 9) / 2, "{memory}");
        for (index, expected) in [
            (1, "Pressure WARNING | Free index 30%"),
            (2, "Compressed 3.00 GiB | Wired 5.00 GiB"),
            (3, "Swap used 6.78 GiB"),
            (5, "Compressed history (GiB, auto)"),
        ] {
            assert!(memory_rows[index].contains(expected), "{expected} at {cols}x{height}:\n{memory}");
        }
        assert!(memory_rows[4].contains("IN -- MiB/s") && memory_rows[4].contains("OUT -- MiB/s"), "{memory}");
    }
}

#[test]
fn full_layout_has_independent_source_aligned_rectangles() {
    let harness = fixture_harness();
    for (cols, height) in [(100, 30), (101, 31), (120, 36), (140, 42)] {
        let rows = physical_rows(&harness.render(cols, height, false), cols, height);
        let x = 2;
        let width = cols - 2;
        let right_width = (width - 2) / 2;
        let left_width = width - 2 - right_width;
        let right_x = x + left_width + 2;
        let extra = height - 30;
        let inference_h = 11 + extra / 2;
        let cache_h = 10 + extra - extra / 2;
        let hardware_h = (height - 9).div_ceil(2);
        let cache_y = 4 + inference_h;
        let server_memory_y = cache_y + cache_h;
        let local_memory_y = 4 + hardware_h;
        let local_memory_h = height - 9 - hardware_h;
        let disk_y = local_memory_y + local_memory_h;

        assert_rectangle(&rows, x, 1, width, 3, "SPLASH MONITOR");
        for (panel_x, top, panel_width, panel_height, title) in [
            (x, 4, left_width, inference_h, INFERENCE),
            (x, server_memory_y, left_width, 6, MEMORY),
            (x, cache_y, left_width, cache_h, CACHE),
            (right_x, 4, right_width, hardware_h, HARDWARE),
            (right_x, local_memory_y, right_width, local_memory_h, LOCAL_MEMORY),
            (right_x, disk_y, right_width, 6, DISK_IO),
        ] {
            assert_rectangle(&rows, panel_x, top, panel_width, panel_height, title);
        }
        assert_eq!(server_memory_y + 5, height);
        assert_eq!(disk_y, server_memory_y);
        assert_eq!(disk_y + 5, height);
        assert!(hardware_h.abs_diff(local_memory_h) <= 1);
        for row in &rows[3..] {
            let gap: String = row.chars().skip(x + left_width - 1).take(2).collect();
            assert_eq!(gap, "  ", "content crossed the column gap at {cols}x{height}: {row}");
        }
    }
}

#[test]
fn minimum_full_cache_retains_separate_read_and_write_charts() {
    let mut harness = fixture_harness();
    harness.state.cache_read_history = [1.0, 2.0].into_iter().collect();
    harness.state.cache_write_history = [3.0, 4.0].into_iter().collect();
    let rows = harness.visible_rows(100, 30);
    let panels = source_panels(&rows, 100, 30);
    let cache = panels.splash(CACHE);
    let lines: Vec<&str> = cache.lines().collect();
    assert_eq!(lines.len(), 10, "{cache}");
    for (line, label) in [(lines[5], "R"), (lines[6], "W")] {
        assert!(
            line.trim_start_matches('│').trim_start().starts_with(label)
                && line.chars().any(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch)),
            "missing {label} chart: {cache}"
        );
    }
    assert!(
        lines[8].contains("R ") && lines[8].contains("/ W ") && lines[8].contains("hit tokens"),
        "missing transfer totals: {cache}"
    );
}

#[test]
fn either_full_mode_dimension_below_threshold_selects_compact() {
    let harness = fixture_harness();
    for (cols, height) in [(100, 29), (99, 30)] {
        let rows = harness.visible_rows(cols, height);
        let text = rows.join("\n");
        let panels = source_panels(&rows, cols, height);
        assert!(panels.splash(STATUS).contains("ONLINE"), "{cols}x{height}");
        assert!(panels.local(LOCAL_MEMORY).contains("Swap history"), "{cols}x{height}");
        for title in [INFERENCE, CACHE, MEMORY, DISK_IO] {
            assert!(!text.contains(title), "{title} appeared at {cols}x{height}");
        }
    }
}

#[test]
fn compact_height_profiles_prioritize_thermal_and_retain_narrow_values() {
    let harness = distinct_harness();
    for (cols, height) in [(66, 14), (66, 15), (66, 16), (66, 20), (66, 24), (86, 27)] {
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
        let memory = panels.local(LOCAL_MEMORY);
        assert_eq!(memory.contains("Swap history"), height >= 16, "{cols}x{height}");
        assert!(memory.contains("WARNING") && memory.contains("Swap 6.78 GiB"), "{memory}");
        assert!(memory.contains("OUT -- MiB/s"), "{memory}");
        if height >= 15 {
            let free = memory.lines().find(|line| line.contains("Free index")).unwrap();
            assert!(
                free.contains("Free index 30%") && free.contains("IN -- MiB/s"),
                "free index and IN rate overlapped at {cols}x{height}: {free}"
            );
        }
        for gauge in ["Compressed 3.00 GiB", "Wired 5.00 GiB"] {
            assert_eq!(memory.contains(gauge), height >= 24, "{cols}x{height}:\n{memory}");
        }
    }
    for height in 12..=15 {
        let rows = harness.visible_rows(64, height);
        let panels = source_panels(&rows, 64, height);
        let hardware = panels.local(HARDWARE);
        assert!(hardware.contains("GPU 62.0%"), "64x{height}:\n{hardware}");
        assert_eq!(hardware.contains("CPU 14.8%"), height >= 14, "64x{height}");
        let memory = panels.local(LOCAL_MEMORY);
        assert!(memory.contains("WARNING") && memory.contains("Swap 6.78 GiB"), "{memory}");
        assert!(memory.contains("OUT -- MiB/s"), "{memory}");
        assert!(!memory.contains("Compressed") && !memory.contains("Wired"), "{memory}");
        if height >= 15 {
            let free = memory.lines().find(|line| line.contains("Free index")).unwrap();
            assert!(free.contains("Free index 30%") && free.contains("IN -- MiB/s"), "{free}");
        }
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
    let memory = panels.local(LOCAL_MEMORY);
    assert!(memory.contains("Swap history"), "{memory}");
    assert!(memory.contains("Compressed 3.00 GiB | Wired 5.00 GiB"), "{memory}");
    assert!(!memory.contains("Swap amount is historical"), "{memory}");
    let text = rows.join("\n");
    assert!(text.contains("Swap used can reflect earlier activity."), "{text}");
    assert!(!text.contains("Pressure + swap OUT trend matter more"), "{text}");
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
            assert!(panels.local(LOCAL_MEMORY).contains("NORMAL"));
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
    let mut sample = sample_fixture();
    sample.cpu_cores = vec![Some(0.1); 99];
    let harness = common::Harness::with(Some(BASE_STATUS), sample);
    let rows = harness.visible_rows(100, 30);
    let panels = source_panels(&rows, 100, 30);
    let hardware = panels.local(HARDWARE);
    for core in 0..99 {
        assert!(!hardware.contains(&format!("C{core} ")), "truncated table: {hardware}");
    }
    assert!(
        hardware.contains("Cores 52–87°C  avg 78°C (14 sensors)"),
        "complete summary row missing: {hardware}"
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
    // Exercise an empty sample with sensors disabled.
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
    sample.gpu_active_ratio = None;
    sample.device_unresolved = Some("/var/folders/tmp".to_string());
    let harness = common::Harness::with(Some(BASE_STATUS), sample);
    let rows = harness.visible_rows(140, 42);
    let panels = source_panels(&rows, 140, 42);
    let hardware = panels.local(HARDWARE);
    let memory = panels.local(LOCAL_MEMORY);
    let disk = panels.local(DISK_IO);
    assert!(hardware.contains("GPU utilization unavailable on this macOS"), "{hardware}");
    assert!(disk.contains("Device I/O unresolved"), "{disk}");
    assert!(disk.contains("Path /var/folders/tmp"), "{disk}");
    assert!(!disk.contains("all processes") && !disk.contains("disk3"), "{disk}");
    for text in [hardware, memory, panels.splash(CACHE)] {
        assert!(!text.contains("Device I/O unresolved") && !text.contains("/var/folders/tmp"), "{text}");
    }
    for (cols, height) in [(86, 27), (66, 20), (64, 15), (36, 10)] {
        let text = harness.visible_text(cols, height);
        assert!(!text.contains("Device I/O unresolved") && !text.contains("/var/folders/tmp"), "{text}");
        assert!(!text.contains(DISK_IO), "{text}");
    }

    let mut sample = sample_fixture();
    sample.volume = None;
    let long_path = "/var/folders/abcdefghijklmnopqrstuvwxyz0123456789/another-directory";
    sample.device_unresolved = Some(long_path.to_string());
    let harness = common::Harness::with(Some(BASE_STATUS), sample);
    let rows = harness.visible_rows(100, 30);
    let panels = source_panels(&rows, 100, 30);
    let disk = panels.local(DISK_IO);
    let path_row = disk.lines().find(|line| line.contains("Path ")).unwrap();
    let clipped = path_row.trim_matches('│').trim();
    assert!(clipped.starts_with("Path /var/folders/"), "{disk}");
    assert!(!clipped.contains(long_path), "long path should be clipped: {disk}");
    assert!(!disk.contains("all processes") && !disk.contains("disk3"), "{disk}");
}

#[test]
fn resolved_device_is_labelled_device_wide() {
    let harness = fixture_harness();
    for (cols, height) in [(100, 30), (140, 42)] {
        let rows = harness.visible_rows(cols, height);
        let panels = source_panels(&rows, cols, height);
        let disk = panels.local(DISK_IO);
        assert!(disk.contains("Device disk3 (all processes)"), "{disk}");
        assert!(disk.contains("Read -- MiB/s") && disk.contains("Write -- MiB/s"), "{disk}");
        for text in [panels.local(LOCAL_MEMORY), panels.local(HARDWARE), panels.splash(CACHE)] {
            assert!(!text.contains("disk3") && !text.contains("all processes"), "{text}");
        }
    }

    let mut sample = sample_fixture();
    let device = "disk123456789012345678901";
    sample.volume.as_mut().unwrap().device = device.to_string();
    let harness = common::Harness::with(Some(BASE_STATUS), sample);
    let rows = harness.visible_rows(100, 30);
    let panels = source_panels(&rows, 100, 30);
    let disk = panels.local(DISK_IO);
    assert!(disk.contains(&format!("{device} | all processes")), "{disk}");
    assert!(!panels.local(LOCAL_MEMORY).contains(device));
}

#[test]
fn unavailable_device_keeps_its_panel_and_missing_rates() {
    let mut sample = sample_fixture();
    sample.volume = None;
    sample.device_unresolved = None;
    let harness = common::Harness::with(Some(BASE_STATUS), sample);
    for (cols, height) in [(100, 30), (140, 42)] {
        let raw = harness.render(cols, height, false);
        let rows = physical_rows(&raw, cols, height);
        let width = cols - 2;
        let right_width = (width - 2) / 2;
        let left_width = width - 2 - right_width;
        let right_x = 2 + left_width + 2;
        let memory_h = (height - 9) / 2;
        assert_rectangle(&rows, right_x, height - 5 - memory_h, right_width, memory_h, LOCAL_MEMORY);
        assert_rectangle(&rows, right_x, height - 5, right_width, 6, DISK_IO);
        let panels = source_panels(&rows, cols, height);
        let disk = panels.local(DISK_IO);
        assert!(disk.contains("Device unavailable"), "{disk}");
        assert!(disk.contains("R -- / W -- MiB/s"), "{disk}");
        assert!(!disk.contains("all processes") && !disk.contains("disk3"), "{disk}");
    }
}

#[test]
fn compressed_history_expands_and_respects_current_availability() {
    for (cols, height) in [(100, 30), (140, 42)] {
        for (values, current, active) in [
            (vec![1.4, 2.2, 0.6, 3.0], 3 * 1_073_741_824u64, true),
            (vec![0.0, 0.0, 0.0, 0.0], 0, false),
        ] {
            let mut harness = fixture_harness();
            harness.sample.compressed_bytes = Some(current);
            harness.state.compressed_history = values.into_iter().collect();
            let rows = harness.visible_rows(cols, height);
            let panels = source_panels(&rows, cols, height);
            let memory = panels.local(LOCAL_MEMORY);
            let lines: Vec<&str> = memory.lines().collect();
            assert_eq!(lines.len(), (height - 9) / 2, "{memory}");
            assert!(lines[5].contains("Compressed history (GiB, auto)"), "{memory}");
            let graph = lines[6];
            if active {
                assert!(
                    lines[6..lines.len() - 1].iter().any(|line| line
                        .chars()
                        .any(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch))),
                    "no compressed history graph at {cols}x{height}: {memory}"
                );
            } else {
                assert!(memory.contains("Compressed 0.00 GiB"), "{memory}");
                assert!(graph.contains("0.00 GiB in observed samples"), "{memory}");
                assert!(!graph.contains("Idle - no activity"), "{memory}");
            }
            assert!(!graph.contains('·'), "dotted baseline in memory graph: {memory}");
            assert!(!memory.contains("Device") && !memory.contains("disk3"), "{memory}");
            let disk = panels.local(DISK_IO);
            assert_eq!(disk.lines().count(), 6, "{disk}");
            assert!(disk.contains("Device disk3 (all processes)"), "{disk}");
        }

        let mut harness = fixture_harness();
        harness.state.compressed_history = [1.0, 2.0, 3.0].into_iter().collect();
        harness.sample.compressed_bytes = None;
        let rows = harness.visible_rows(cols, height);
        let panels = source_panels(&rows, cols, height);
        let memory = panels.local(LOCAL_MEMORY);
        let graph = memory.lines().nth(6).unwrap();
        assert!(memory.contains("Compressed -- GiB"), "{memory}");
        assert!(graph.contains("Unavailable"), "{memory}");
        assert!(!graph.chars().any(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch)), "{memory}");

        harness.sample.compressed_bytes = Some(3 * 1_073_741_824);
        harness.state.compressed_history.clear();
        let rows = harness.visible_rows(cols, height);
        let panels = source_panels(&rows, cols, height);
        let memory = panels.local(LOCAL_MEMORY);
        assert!(memory.contains("Compressed 3.00 GiB"), "{memory}");
        assert!(memory.lines().nth(6).unwrap().contains("Waiting for samples..."), "{memory}");
    }
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
    for (cols, height) in [(100, 30), (140, 42), (86, 27), (66, 24), (72, 20), (64, 15), (36, 10)] {
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
            let memory = panels.local(LOCAL_MEMORY);
            assert!(hardware.contains("62.0%") && hardware.contains("14.8%"), "{hardware}");
            assert!(memory.contains("WARNING") && memory.contains("6.78 GiB"), "{memory}");
            if (cols >= 100 && height >= 30) || (cols >= 66 && height >= 24) {
                assert!(memory.contains("Compressed 3.00 GiB") && memory.contains("Wired 5.00 GiB"), "{memory}");
            }
            if cols >= 100 && height >= 30 {
                assert!(panels.splash(MEMORY).contains("--/-- GiB"));
                let disk = panels.local(DISK_IO);
                assert!(disk.contains("disk3") && disk.contains("all processes"), "{disk}");
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
    for (cols, height) in [(100, 30), (140, 42), (86, 27), (72, 20), (64, 15), (36, 10)] {
        let rows = harness.visible_rows(cols, height);
        if height < 12 {
            assert!(prefixed_row(&rows, "[SPLASH]", "Metal").contains("41.02/42.00 GiB"));
            assert!(prefixed_row(&rows, "[LOCAL]", "Swap").contains("-- GiB"));
            assert!(prefixed_row(&rows, "[LOCAL]", "UNKNOWN").contains("OUT -- MiB/s"));
            continue;
        }
        let panels = source_panels(&rows, cols, height);
        let hardware = panels.local(HARDWARE);
        let memory = panels.local(LOCAL_MEMORY);
        assert!(hardware.contains("--%"), "{cols}x{height}:\n{hardware}");
        assert!(memory.contains("UNKNOWN"), "{memory}");
        if cols >= 100 && height >= 30 {
            assert!(memory.contains("Swap used -- GiB"), "{memory}");
            assert!(memory.contains("Compressed -- GiB") && memory.contains("Wired -- GiB"), "{memory}");
            assert!(memory.contains("Unavailable"), "{memory}");
            assert!(panels.local(DISK_IO).contains("Device unavailable"));
            assert!(panels.splash(INFERENCE).contains("Thermal critical"));
            assert!(panels.splash(CACHE).contains("KV Spill 278"));
            assert!(panels.splash(MEMORY).contains("41.02/42.00 GiB"));
        } else {
            assert!(memory.contains("Swap -- GiB"), "{memory}");
            if cols >= 66 && height >= 24 {
                assert!(memory.contains("Compressed -- GiB") && memory.contains("Wired -- GiB"), "{memory}");
            }
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
        (
            100,
            30,
            vec![
                "[GATEWAY] INFERENCE / REQUESTS",
                "[GATEWAY] CACHE / KV SPILL",
                "[GATEWAY] MEMORY / CONFIG",
            ],
        ),
        (
            140,
            42,
            vec![
                "[GATEWAY] INFERENCE / REQUESTS",
                "[GATEWAY] CACHE / KV SPILL",
                "[GATEWAY] MEMORY / CONFIG",
            ],
        ),
        (
            80,
            24,
            vec![
                "[GATEWAY] STATUS",
                "[GATEWAY] ACTIVITY",
                "[GATEWAY] KV SPILL",
            ],
        ),
        (64, 15, vec!["[GATEWAY] ACTIVITY"]),
        (36, 10, vec!["[GATEWAY] Metal", "[GATEWAY] ONLINE"]),
    ] {
        let rows = harness.visible_rows(cols, height);
        let text = rows.join("\n");
        for label in expected {
            assert!(
                text.contains(label),
                "{label} missing at {cols}x{height}: {text}"
            );
            if cols >= 100 && height >= 30 {
                let width = cols - 2;
                let right_width = (width - 2) / 2;
                let left_width = width - 2 - right_width;
                panel(&rows, 2, left_width, label);
            }
        }
        assert!(!text.contains("[SPLASH]"), "{cols}x{height}: {text}");
        assert!(text.contains("[LOCAL]"), "{cols}x{height}: {text}");
        if height >= 12 {
            let width = cols - 2;
            let full = cols >= 100 && height >= 30;
            let (hardware_x, hardware_width) = if cols >= 66 {
                let right_width = if full {
                    (width - 2) / 2
                } else {
                    width - (width - 2) / 2 - 2
                };
                (2 + width - right_width, right_width)
            } else {
                (2, width)
            };
            let hardware = panel(&rows, hardware_x, hardware_width, HARDWARE);
            let (memory_x, memory_width) = if full {
                (hardware_x, hardware_width)
            } else {
                (2, width)
            };
            let memory = panel(&rows, memory_x, memory_width, LOCAL_MEMORY);
            assert!(!hardware.contains("[GATEWAY]") && !memory.contains("[GATEWAY]"), "{text}");
            if full {
                let disk = panel(&rows, hardware_x, hardware_width, DISK_IO);
                assert!(!disk.contains("[GATEWAY]"), "{disk}");
                assert!(memory.contains("Compressed 3.00 GiB") && memory.contains("Wired 5.00 GiB"), "{memory}");
            }
        }
    }
}
