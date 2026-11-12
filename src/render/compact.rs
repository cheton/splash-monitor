//! Compact framed layout for smaller terminals. Panel heights come from fixed
//! profiles so panels never overlap and nothing wraps off the bottom edge.

use crate::render::{draw_box, sparkline, Frame, Grid};
use crate::theme::Theme;

pub fn render(grid: &mut Grid, frame: &Frame<'_>, theme: &Theme) {
    let cols = grid.cols;
    let rows = grid.rows;
    let x = 2usize;
    let w = cols - 2;

    if w < 24 || rows < 12 {
        grid.at(1, 1, Some(theme.data), "SPLASH MONITOR  |  q: quit", cols);
        grid.at(
            1,
            2,
            Some(theme.text),
            &format!(
                "Metal {}/{} GiB | Swap {} GiB",
                frame.display.metal_gib,
                frame.display.limit_gib,
                frame.swap_used_gib()
            ),
            cols,
        );
        grid.at(
            1,
            3,
            Some(theme.muted),
            &format!(
                "{} | OUT {} MiB/s",
                frame.pressure_text(),
                Frame::rate_text(frame.rates.swap_out_mib)
            ),
            cols,
        );
        return;
    }

    let header_h = 3usize;
    // At 20+ rows all five dashboard sections survive.
    let (mut th, mut ch, mut sh, mut kh, show_spill) = if cols >= 66 && rows >= 20 {
        (5, 4, 5, 3, true)
    } else if cols >= 66 && rows >= 16 {
        (4, 4, 5, 0, false)
    } else if cols >= 66 && rows >= 15 {
        (4, 4, 4, 0, false)
    } else if cols >= 66 && rows >= 14 {
        (4, 4, 3, 0, false)
    } else {
        (3, 3, 3, 0, false)
    };
    if cols >= 66 && rows >= 24 {
        th = 6;
        ch = 5;
        sh = 6;
        kh = 4;
    }

    draw_box(grid, theme, x, 1, w, header_h, "SPLASH MONITOR");
    let (label, label_color) = if !frame.view.online {
        ("● OFFLINE", theme.critical)
    } else if frame.view.gateway {
        ("● GATEWAY", theme.data)
    } else {
        ("● LIVE", theme.good)
    };
    grid.at(x + 2, 2, Some(label_color), label, 999);
    if cols >= 86 {
        grid.at(
            x + 16,
            2,
            Some(theme.muted),
            &format!(
                "{}  |  {}  |  refresh {}s",
                frame.display.model, frame.now_text, frame.interval
            ),
            w - 31,
        );
    } else if cols >= 66 {
        grid.at(
            x + 13,
            2,
            Some(theme.muted),
            &format!("{}  |  refresh {}s", frame.display.model, frame.interval),
            w - 28,
        );
    } else {
        grid.at(
            x + 13,
            2,
            Some(theme.muted),
            &format!("refresh {}s", frame.interval),
            w - 28,
        );
    }
    grid.at(x + w - 12, 2, Some(theme.caution), "q: quit", 9);

    let start = header_h + 1;
    if cols >= 66 {
        let leftw = (w - 2) / 2;
        let rightx = x + leftw + 2;
        let rightw = w - leftw - 2;
        draw_box(grid, theme, x, start, leftw, th, "STATUS / INFERENCE");
        draw_box(grid, theme, rightx, start, rightw, th, "HARDWARE / THERMAL");

        let (label, label_color) = if !frame.view.online {
            ("● OFFLINE", theme.critical)
        } else if frame.view.gateway {
            ("● GATEWAY", theme.data)
        } else {
            ("● ONLINE", theme.good)
        };
        grid.at(x + 2, start + 1, Some(label_color), label, 999);
        grid.at(
            x + 12,
            start + 1,
            Some(theme.text),
            &frame.display.model,
            leftw - 14,
        );
        if th >= 4 {
            grid.at(
                x + 2,
                start + 2,
                Some(theme.muted),
                &format!(
                    "Weights {} | KV {}",
                    weights_label(frame),
                    frame.display.kv_capacity
                ),
                leftw - 4,
            );
        }
        if th >= 5 {
            grid.at(
                x + 2,
                start + 3,
                Some(theme.muted),
                &format!(
                    "Active {}   |   idle {}s",
                    frame.view.active, frame.view.idle_secs
                ),
                leftw - 4,
            );
        }
        if th >= 6 {
            grid.at(
                x + 2,
                start + 4,
                Some(theme.muted),
                "Metal memory is not process RSS",
                leftw - 4,
            );
        }

        let right_inner = rightw - 4;
        grid.at(
            rightx + 2,
            start + 1,
            Some(theme.good),
            &format!(
                "GPU {}%  {}°C",
                frame.gpu_percent_text(),
                frame.gpu_temp_text()
            ),
            right_inner,
        );
        if th >= 4 {
            grid.at(
                rightx + 2,
                start + 2,
                Some(theme.data),
                &cpu_line(frame),
                right_inner,
            );
        }
        if th >= 5 {
            grid.at(
                rightx + 2,
                start + 3,
                Some(theme.muted),
                &format!(
                    "Thermal {} | {}/{} GiB",
                    frame.view.thermal_state, frame.display.metal_gib, frame.display.limit_gib
                ),
                right_inner,
            );
        }
    } else {
        draw_box(grid, theme, x, start, w, th, "RESOURCE STATUS");
        let (label, label_color) = if !frame.view.online {
            ("OFFLINE", theme.critical)
        } else if frame.view.gateway {
            ("GATEWAY", theme.data)
        } else {
            ("ONLINE", theme.good)
        };
        grid.at(x + 2, start + 1, Some(label_color), label, 999);
        grid.at(
            x + 11,
            start + 1,
            Some(theme.text),
            &format!(
                "GPU {}% / {}°C",
                frame.gpu_percent_text(),
                frame.gpu_temp_text()
            ),
            w - 14,
        );
        if th >= 4 {
            grid.at(
                x + 2,
                start + 2,
                Some(theme.muted),
                &format!(
                    "CPU {}% | Splash {}/{} GiB",
                    frame.cpu_percent_text(),
                    frame.display.metal_gib,
                    frame.display.limit_gib
                ),
                w - 4,
            );
        }
    }

    let cy = start + th;
    draw_box(grid, theme, x, cy, w, ch, "INFERENCE / CACHE ACTIVITY");
    let section = x + 2;
    if w >= 67 {
        let section2 = x + w / 3;
        let section3 = x + 2 * w / 3;
        grid.at(
            section,
            cy + 1,
            Some(theme.data),
            &format!("Decode {} tok/s", one_decimal(frame.rates.output_tok)),
            section2 - section - 1,
        );
        grid.at(
            section2,
            cy + 1,
            Some(theme.good),
            &format!(
                "Read {} MiB/s",
                Frame::rate_text(frame.rates.cache_read_mib)
            ),
            section3 - section2 - 1,
        );
        grid.at(
            section3,
            cy + 1,
            Some(theme.hot),
            &format!(
                "Write {} MiB/s",
                Frame::rate_text(frame.rates.cache_write_mib)
            ),
            x + w - section3 - 2,
        );
    } else {
        grid.at(
            section,
            cy + 1,
            Some(theme.data),
            &format!(
                "Context {} | Decode {} tok/s",
                frame.display.context_window,
                one_decimal(frame.rates.output_tok)
            ),
            w - 4,
        );
    }
    if ch >= 4 {
        grid.at(
            section,
            cy + 2,
            Some(theme.muted),
            &format!(
                "Splash {}/{} GiB | Context {} | Disk {}/{} GiB",
                frame.display.metal_gib,
                frame.display.limit_gib,
                frame.display.context_window,
                frame.display.cache_used_gib,
                frame.display.cache_limit_gib
            ),
            w - 4,
        );
    }
    if ch >= 5 {
        grid.at(
            section,
            cy + 3,
            Some(theme.muted),
            &format!(
                "Gap p95 {}ms | Spill {} pages | RAM bound {}",
                frame.display.gap_p95, frame.display.spilled_pages, frame.display.resident
            ),
            w - 4,
        );
    }

    let sy = cy + ch;
    draw_box(grid, theme, x, sy, w, sh, "macOS PRESSURE / SWAP");
    let pressure_color = frame.pressure_color(theme);
    if cols < 66 {
        grid.at(
            section,
            sy + 1,
            Some(pressure_color),
            frame.pressure_text(),
            10,
        );
        grid.at(
            section + 11,
            sy + 1,
            Some(theme.data),
            &format!("Swap {} GiB", frame.swap_used_gib()),
            16,
        );
        if w >= 52 {
            grid.at(
                section + 28,
                sy + 1,
                Some(theme.caution),
                &format!("OUT {}", Frame::rate_text(frame.rates.swap_out_mib)),
                w - 32,
            );
        }
        if sh >= 4 {
            grid.at(
                section,
                sy + 2,
                Some(theme.muted),
                &format!(
                    "Free {}% | IN {} | KV {}pg",
                    frame.free_percent_text(),
                    Frame::rate_text(frame.rates.swap_in_mib),
                    frame.display.spilled_pages
                ),
                w - 4,
            );
        }
    } else {
        grid.at(section, sy + 1, Some(theme.muted), "Pressure", 9);
        grid.at(
            section + 9,
            sy + 1,
            Some(pressure_color),
            frame.pressure_text(),
            10,
        );
        grid.at(
            x + w / 3,
            sy + 1,
            Some(theme.data),
            &format!("Swap {} GiB", frame.swap_used_gib()),
            19,
        );
        grid.at(
            x + 2 * w / 3,
            sy + 1,
            Some(theme.caution),
            &format!("OUT {} MiB/s", Frame::rate_text(frame.rates.swap_out_mib)),
            w / 3 - 2,
        );
        if sh >= 4 {
            grid.at(
                section,
                sy + 2,
                Some(theme.muted),
                &format!("Free {}%", frame.free_percent_text()),
                12,
            );
            grid.at(
                x + w / 3,
                sy + 2,
                Some(theme.good),
                &format!("IN {} MiB/s", Frame::rate_text(frame.rates.swap_in_mib)),
                19,
            );
            grid.at(
                x + 2 * w / 3,
                sy + 2,
                Some(theme.muted),
                &format!("KV {} pages", frame.display.spilled_pages),
                w / 3 - 2,
            );
        }
    }
    if sh >= 5 {
        grid.at(section, sy + 3, Some(theme.muted), "Swap history", 14);
        let histw = (w - 20).min(62);
        sparkline(
            grid,
            x + 18,
            sy + 3,
            histw,
            &frame.state.swap_history,
            theme.data,
        );
    }
    if sh >= 6 {
        grid.at(
            section,
            sy + 4,
            Some(theme.muted),
            "Swap amount is historical; rates measure current I/O.",
            w - 4,
        );
    }

    if show_spill {
        let ky = sy + sh;
        draw_box(grid, theme, x, ky, w, kh, "KV SPILL / DISK I/O");
        grid.at(
            section,
            ky + 1,
            Some(theme.muted),
            &format!(
                "Requests {}   |   Resident {}   |   Pages {}",
                frame.display.spill_requests,
                frame.display.resident_requests,
                frame.display.spilled_pages
            ),
            w - 4,
        );
        if kh >= 4 {
            grid.at(
                section,
                ky + 2,
                Some(theme.text),
                &format!(
                    "Spilled {} GiB  |  Writes {} GiB  |  Reads {} GiB",
                    frame.display.spill_gib, frame.display.write_gib, frame.display.read_gib
                ),
                w - 4,
            );
        }
    }

    let used = header_h + th + ch + sh + kh;
    if rows > used {
        grid.at(
            x + 1,
            used + 1,
            Some(theme.muted),
            "Pressure + swap OUT trend matter more than total swap used.",
            w - 2,
        );
    }
}

/// `released` renders as a word, exactly as the previous dashboard did.
fn weights_label(frame: &Frame<'_>) -> &'static str {
    match frame.view.released.as_str() {
        "true" => "released",
        "false" => "loaded",
        _ => "--",
    }
}

/// The compact CPU line gains the core temperature range when a sensor table
/// would not fit vertically.
fn cpu_line(frame: &Frame<'_>) -> String {
    let base = format!(
        "CPU {}%  {}°C",
        frame.cpu_percent_text(),
        frame.cpu_temp_text()
    );
    if frame.sample.cpu_cores.is_empty() || frame.sample.cpu_sensors.is_empty() {
        return base;
    }
    let values: Vec<f64> = frame
        .sample
        .cpu_sensors
        .iter()
        .map(|(_, value)| *value)
        .collect();
    let min = values.iter().copied().fold(f64::INFINITY, f64::min).round() as i64;
    let max = values
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max)
        .round() as i64;
    format!("{base}  cores {min}-{max}°")
}

fn one_decimal(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "--".to_string())
}
