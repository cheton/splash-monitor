//! Dense btop-inspired layout for terminals at least 100 columns by 30 rows.
//! Graphs always show observed samples; unknown values render `--`, never 0.

use crate::render::{bar_graph, dot_graph, draw_box, gradient_meter, plot_history, Frame, Grid};
use crate::theme::{Color, Theme};

/// A core cell is `label | utilization line | percent | temperature area |
/// temperature`, drawn inside a fixed pitch so columns line up and cells never
/// touch. A rich cell carries both graphs (`C17 ⡠⢤⡠⢀ 12% ⣿⣶⣦⣄ 88°C`): the
/// utilization history is a dotted line, the temperature history a filled
/// Braille area, following btop's reading of the two quantities. A medium cell
/// keeps the utilization graph only, a plain cell neither. Graph width grows
/// with the width the panel actually has.
const MIN_GRAPH_WIDTH: usize = 4;
const MAX_GRAPH_WIDTH: usize = 12;
/// Rows the hardware panel spends before the core table: GPU meter, GPU power,
/// CPU meter, thermal state.
const TABLE_FIRST_ROW: usize = 5;

/// The text a cell spends on the label (`C17`), the utilization (`100%`) and
/// the temperature (`88°C`, or `88°` without graphs), plus one separator
/// between every pair of fields. `graphs` is 0, 1 or 2.
fn cell_text_width(graphs: usize) -> usize {
    let temperature = if graphs == 0 { 3 } else { 4 };
    3 + 4 + temperature + (graphs + 2)
}

enum TablePlan {
    /// Columns and rows of core cells, and the graph width each cell gets.
    Cells {
        pitch: usize,
        columns: usize,
        rows: usize,
        graphs: usize,
        graph_width: usize,
    },
    /// Not enough room for a table: one summary row instead.
    Summary,
}

/// Cell geometry shared by every core in one table.
struct CoreLayout {
    graphs: usize,
    graph_width: usize,
    util_cap: f64,
}

fn plan_table(frame: &Frame<'_>, inner_width: usize, top_h: usize) -> TablePlan {
    let core_count = frame.sample.cpu_cores.len();
    // Cell labels are three columns wide (`C17`), so a machine with more than
    // 99 logical cores gets the summary row instead of truncated labels.
    if core_count == 0 || core_count > 99 || frame.sample.cpu_sensors.is_empty() {
        return TablePlan::Summary;
    }
    // Rows the table may use: the panel minus its header rows, the mapping
    // caveat, the history plot label and the plot itself.
    let max_rows = top_h.saturating_sub(TABLE_FIRST_ROW + 3);
    for graphs in [2usize, 1, 0] {
        let fixed = cell_text_width(graphs);
        let columns = inner_width / (fixed + graphs * MIN_GRAPH_WIDTH + 1);
        if columns == 0 {
            continue;
        }
        let rows = core_count.div_ceil(columns);
        if rows > max_rows {
            continue;
        }
        // Spread the columns evenly. Spare width goes into the graphs up to a
        // cap; anything left stays as breathing room between cells.
        let pitch = inner_width / columns;
        let graph_width = ((pitch - 1).saturating_sub(fixed) / graphs.max(1)).min(MAX_GRAPH_WIDTH);
        return TablePlan::Cells {
            pitch,
            columns,
            rows,
            graphs,
            graph_width,
        };
    }
    TablePlan::Summary
}

pub fn render(grid: &mut Grid, frame: &Frame<'_>, theme: &Theme) {
    let cols = grid.cols;
    let rows = grid.rows;
    let x = 2usize;
    let w = cols - 2;
    let gap = 2usize;
    // Balanced panels: the core table needs its columns, the inference and
    // cache graphs take the rest.
    let rw = (w - gap) / 2;
    let lw = w - gap - rw;
    let rx = x + lw + gap;
    let top = 4usize;
    let footer = rows - 4;
    let top_h = (footer - top - 1) / 2;
    let bottom = top + top_h + 1;
    let bh = footer - bottom - 1;
    let graphw = lw - 4;
    let graphw2 = rw - 4;
    let gheight = top_h.saturating_sub(6);

    // Header strip.
    draw_box(grid, theme, x, 1, w, 3, "SPLASH MONITOR");
    let (label, label_color) = if !frame.view.online {
        ("● OFFLINE", theme.critical)
    } else if frame.view.active > 0 {
        ("● RUNNING", theme.good)
    } else if frame.view.released == "true" {
        ("● STANDBY", theme.good)
    } else {
        ("● IDLE", theme.good)
    };
    grid.at(x + 2, 2, Some(label_color), label, 12);
    grid.at(x + 17, 2, Some(theme.text), &frame.display.model, 20);
    grid.at(
        x + 39,
        2,
        Some(theme.muted),
        &format!("{}  {}s refresh", frame.now_text, frame.interval),
        35,
    );
    grid.at(x + w - 12, 2, Some(theme.caution), "q: quit", 9);

    render_inference(grid, frame, theme, x, top, lw, graphw, gheight, top_h);
    render_hardware(grid, frame, theme, rx, top, rw, graphw2, top_h);
    render_cache(grid, frame, theme, x, bottom, lw, graphw, bh);
    render_requests(grid, frame, theme, rx, bottom, rw, graphw2, bh);

    draw_box(grid, theme, x, footer, w, 4, "MEMORY / CAPACITY / CONFIG");
    grid.at(
        x + 2,
        footer + 1,
        Some(theme.good),
        &format!(
            "Splash {}/{} GiB",
            frame.display.metal_gib, frame.display.limit_gib
        ),
        w / 3 - 2,
    );
    grid.at(
        x + w / 3,
        footer + 1,
        Some(theme.caution),
        &format!(
            "Swap {} GiB | OUT {} MiB/s",
            frame.swap_used_gib(),
            Frame::rate_text(frame.rates.swap_out_mib)
        ),
        w / 3 - 2,
    );
    grid.at(
        x + 2 * w / 3,
        footer + 1,
        Some(theme.muted),
        &format!("Weights released {}", frame.view.released),
        w / 3 - 3,
    );
    grid.at(
        x + 2,
        footer + 2,
        Some(theme.muted),
        &format!(
            "Context {} | RAM bound {} | Cache quota {} GiB | KV cap {}",
            frame.display.context_window,
            frame.display.resident,
            frame.display.cache_limit_gib,
            frame.display.kv_capacity
        ),
        w - 4,
    );
}

// Panel geometry is computed once in `render` and shared with the other panels.
#[allow(clippy::too_many_arguments)]
fn render_inference(
    grid: &mut Grid,
    frame: &Frame<'_>,
    theme: &Theme,
    x: usize,
    top: usize,
    lw: usize,
    graphw: usize,
    gheight: usize,
    top_h: usize,
) {
    draw_box(grid, theme, x, top, lw, top_h, "INFERENCE / THROUGHPUT");
    grid.at(
        x + 2,
        top + 1,
        Some(theme.graph),
        &format!("Decode {} tok/s", opt_one_decimal(frame.rates.output_tok)),
        lw / 2 - 3,
    );
    grid.at(
        x + lw / 2,
        top + 1,
        Some(theme.good),
        &longest_fit(
            format!("Prefill avg {} tok/s", frame.display.prefill_tps),
            format!("Prefill {} tok/s", frame.display.prefill_tps),
            lw / 2 - 3,
        ),
        lw / 2 - 3,
    );
    grid.at(
        x + 2,
        top + 2,
        Some(theme.muted),
        &format!(
            "Active {} | Queued {} | Gap p95 {} ms",
            frame.view.active, frame.view.scheduler_queued, frame.display.gap_p95
        ),
        lw - 4,
    );
    grid.at(
        x + 2,
        top + 3,
        Some(theme.muted),
        "Decode tok/s (sampled global counter)",
        lw - 4,
    );
    plot_history(
        grid,
        x + 2,
        top + 4,
        graphw,
        gheight,
        &frame.state.output_history,
        0.0,
        theme.graph_ramp,
    );
    grid.at(
        x + 2,
        top + top_h - 2,
        Some(theme.muted),
        &format!(
            "Engine avg {} tok/s | TTFT p95 {} ms",
            frame.display.decode_rate, frame.display.ttft_p95
        ),
        lw - 4,
    );
}

// Geometry stays positional: every panel is drawn at one box with its own graph column.
#[allow(clippy::too_many_arguments)]
fn render_hardware(
    grid: &mut Grid,
    frame: &Frame<'_>,
    theme: &Theme,
    rx: usize,
    top: usize,
    rw: usize,
    graphw2: usize,
    top_h: usize,
) {
    draw_box(grid, theme, rx, top, rw, top_h, "HARDWARE / THERMAL");

    // Aggregate meters read like btop: label, bar, utilization at the end of
    // the bar, die temperature at the end of the row. Power and load averages
    // keep their own line.
    const PERCENT_WIDTH: usize = 6;
    let meter_width = rw.saturating_sub(4 + 3 + 1 + PERCENT_WIDTH + 8);
    let right_at = |text: &str| rx + rw - 1 - text.chars().count();

    let gpu_temp = format!("{}°C", frame.gpu_temp_text());
    grid.at(rx + 2, top + 1, Some(theme.text), "GPU", 3);
    gradient_meter(grid, rx + 6, top + 1, meter_width, frame.hw_gpu(), theme);
    grid.at(
        rx + 6 + meter_width + 1,
        top + 1,
        Some(load_color(
            theme,
            frame.hw_gpu().map(|value| value.round() as i64),
        )),
        &format!("{}%", frame.gpu_percent_text()),
        PERCENT_WIDTH,
    );
    grid.at(
        right_at(&gpu_temp),
        top + 1,
        Some(temp_color(
            theme,
            frame.sample.gpu_temp.map(|value| value.round() as i64),
        )),
        &gpu_temp,
        PERCENT_WIDTH,
    );
    grid.at(
        rx + 2,
        top + 2,
        Some(theme.muted),
        &format!("GPU power {} W", frame.gpu_power_text()),
        rw - 4,
    );

    let cpu_temp = format!("{}°C", frame.cpu_temp_text());
    grid.at(rx + 2, top + 3, Some(theme.text), "CPU", 3);
    gradient_meter(grid, rx + 6, top + 3, meter_width, frame.hw_cpu(), theme);
    grid.at(
        rx + 6 + meter_width + 1,
        top + 3,
        Some(load_color(
            theme,
            frame.hw_cpu().map(|value| value.round() as i64),
        )),
        &format!("{}%", frame.cpu_percent_text()),
        PERCENT_WIDTH,
    );
    grid.at(
        right_at(&cpu_temp),
        top + 3,
        Some(temp_color(
            theme,
            frame.sample.cpu_package.map(|value| value.round() as i64),
        )),
        &cpu_temp,
        PERCENT_WIDTH,
    );
    grid.at(
        rx + 2,
        top + 4,
        Some(frame.thermal_color(theme)),
        &format!("Thermal {} (Splash API)", frame.view.thermal_state),
        rw - 4,
    );

    let inner_width = rw - 4;
    let mut row = top + TABLE_FIRST_ROW;
    match plan_table(frame, inner_width, top_h) {
        TablePlan::Cells {
            pitch,
            columns,
            rows,
            graphs,
            graph_width,
        } => {
            let core_count = frame.sample.cpu_cores.len();
            // One cap for every core graph keeps the columns comparable. The
            // floor stops a mostly idle machine from amplifying sensor noise.
            let util_cap = frame
                .state
                .core_history
                .iter()
                .map(|history| history.iter().copied().fold(0.0f64, f64::max))
                .fold(0.0f64, f64::max)
                .clamp(20.0, 100.0);
            let layout = CoreLayout {
                graphs,
                graph_width,
                util_cap,
            };
            for index in 0..core_count {
                let column = index / rows;
                let slot = index % rows;
                if column >= columns {
                    break;
                }
                draw_core_cell(
                    grid,
                    frame,
                    theme,
                    rx + 2 + column * pitch,
                    row + slot,
                    index,
                    &layout,
                );
            }
            row += rows;
            // Use the full note when it fits; otherwise the short form, so the
            // line never breaks mid-word.
            const CAVEAT: &str = "Sensor-to-core mapping is proportional (this project's mapping)";
            const CAVEAT_SHORT: &str = "Sensor-to-core mapping is proportional";
            let caveat = if CAVEAT.chars().count() <= inner_width {
                CAVEAT
            } else {
                CAVEAT_SHORT
            };
            grid.at(rx + 2, row, Some(theme.muted), caveat, inner_width);
            row += 1;
        }
        TablePlan::Summary => {
            if !frame.sample.cpu_sensors.is_empty() {
                grid.at(
                    rx + 2,
                    row,
                    Some(theme.muted),
                    &sensor_summary(frame),
                    inner_width,
                );
                row += 1;
            }
        }
    }

    // History plot takes whatever vertical space the table did not use.
    let plot_label_row = row;
    let plot_top = row + 1;
    let last_plot_row = top + top_h - 3;
    let plot_height = last_plot_row.saturating_sub(plot_top) + 1;
    let gpu_available = frame.hw_gpu().is_some();
    let cpu_available = frame.hw_cpu().is_some();
    if plot_height >= 1 && plot_top <= last_plot_row && (gpu_available || cpu_available) {
        if gpu_available {
            grid.at(
                rx + 2,
                plot_label_row,
                Some(theme.muted),
                "System GPU history",
                inner_width,
            );
            plot_history(
                grid,
                rx + 2,
                plot_top,
                graphw2,
                plot_height,
                &frame.state.gpu_history,
                100.0,
                theme.good_ramp,
            );
        } else {
            grid.at(
                rx + 2,
                plot_label_row,
                Some(theme.muted),
                "System CPU history (GPU unavailable)",
                inner_width,
            );
            plot_history(
                grid,
                rx + 2,
                plot_top,
                graphw2,
                plot_height,
                &frame.state.cpu_history,
                100.0,
                theme.accent_ramp,
            );
        }
    }

    // Footer row: sensor scope on the left, load averages on the right, the
    // way btop places them. CPU package power has no macOS API, so it is not
    // shown at all instead of filling the row with `--`.
    let load_text = frame.load_avg_text();
    let scope_width = inner_width.saturating_sub(load_text.chars().count() + 1);
    let footer_text = match frame.sensor_hint() {
        Some(hint) => hint,
        None => longest_fit(
            "System-wide sensors; not Splash-only".to_string(),
            "System-wide sensors".to_string(),
            scope_width,
        ),
    };
    grid.at(
        rx + 2,
        top + top_h - 2,
        Some(theme.muted),
        &footer_text,
        scope_width,
    );
    grid.at(
        right_at(&load_text),
        top + top_h - 2,
        Some(theme.muted),
        &load_text,
        load_text.chars().count(),
    );
}

/// One core cell, laid out inside `pitch` columns. Each field is drawn in its
/// own slice, so a graph can never spill into the neighbouring cell:
/// `C17 ▁▂▄▅▅ 12% ⣿⣶⣦ 88°C`. Temperatures come from the proportional sensor
/// mapping, so the temperature graph is that sensor's own history.
fn draw_core_cell(
    grid: &mut Grid,
    frame: &Frame<'_>,
    theme: &Theme,
    x: usize,
    y: usize,
    core: usize,
    layout: &CoreLayout,
) {
    let CoreLayout {
        graphs,
        graph_width,
        util_cap,
    } = *layout;
    let core_count = frame.sample.cpu_cores.len();
    let util = frame.sample.cpu_cores[core].map(|ratio| (ratio * 100.0).round() as i64);
    let sensor_index = core * frame.sample.cpu_sensors.len() / core_count;
    let temp = frame
        .sample
        .cpu_sensors
        .get(sensor_index)
        .map(|(_, value)| value.round() as i64);

    // The planner only builds a table for core counts a three-column label
    // can name, so the label never truncates and never shifts the graph.
    grid.at(x, y, Some(theme.text), &format!("C{core}"), 3);
    let mut cursor = x + 4;

    if graphs >= 1 {
        match frame.state.core_history.get(core) {
            Some(history) => dot_graph(
                grid,
                cursor,
                y,
                graph_width,
                history,
                util_cap,
                load_color(theme, util),
            ),
            None => grid.repeat_at(cursor, y, Some(theme.muted), '·', graph_width),
        }
        cursor += graph_width + 1;
    }

    let util_text = util
        .map(|value| format!("{value:>3}%"))
        .unwrap_or_else(|| "--%".to_string());
    grid.at(cursor, y, Some(load_color(theme, util)), &util_text, 4);
    cursor += 5;

    if graphs >= 2 {
        match frame.state.sensor_history.get(sensor_index) {
            Some(history) => bar_graph(
                grid,
                cursor,
                y,
                graph_width,
                history,
                0.0,
                temp_color(theme, temp),
            ),
            None => grid.repeat_at(cursor, y, Some(theme.muted), '·', graph_width),
        }
        cursor += graph_width + 1;
    }

    let temp_text = temp
        .map(|value| format!("{value:>2}"))
        .unwrap_or_else(|| "--".to_string());
    let temp_field = if graphs == 0 {
        format!("{temp_text}°")
    } else {
        format!("{temp_text}°C")
    };
    grid.at(
        cursor,
        y,
        Some(temp_color(theme, temp)),
        &temp_field,
        temp_field.chars().count(),
    );
}

/// Utilization colors follow the meter thresholds: 45 %, 70 %, 88 %.
fn load_color(theme: &Theme, util: Option<i64>) -> Color {
    match util {
        Some(util) if util < 45 => theme.good,
        Some(util) if util < 70 => theme.caution,
        Some(util) if util < 88 => theme.hot,
        Some(_) => theme.critical,
        None => theme.muted,
    }
}

/// Die temperature colors: comfortable, warm, hot, at the limit.
fn temp_color(theme: &Theme, temp: Option<i64>) -> Color {
    match temp {
        Some(temp) if temp < 72 => theme.good,
        Some(temp) if temp < 82 => theme.caution,
        Some(temp) if temp < 88 => theme.hot,
        Some(_) => theme.critical,
        None => theme.muted,
    }
}

fn sensor_summary(frame: &Frame<'_>) -> String {
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
    let avg = (values.iter().sum::<f64>() / values.len() as f64).round() as i64;
    format!(
        "Cores {min}–{max}°C  avg {avg}°C ({} sensors)",
        values.len()
    )
}

// Geometry stays positional: every panel is drawn at one box with its own graph column.
#[allow(clippy::too_many_arguments)]
fn render_cache(
    grid: &mut Grid,
    frame: &Frame<'_>,
    theme: &Theme,
    x: usize,
    bottom: usize,
    lw: usize,
    graphw: usize,
    bh: usize,
) {
    draw_box(grid, theme, x, bottom, lw, bh, "SPLASH CACHE / I/O");
    grid.at(
        x + 2,
        bottom + 1,
        Some(theme.data),
        &format!(
            "Read {} MiB/s",
            Frame::rate_text(frame.rates.cache_read_mib)
        ),
        lw / 2 - 3,
    );
    grid.at(
        x + 2 + lw / 2 - 2,
        bottom + 1,
        Some(theme.hot),
        &format!(
            "Write {} MiB/s",
            Frame::rate_text(frame.rates.cache_write_mib)
        ),
        lw / 2 - 3,
    );
    if frame.view.disk_present {
        grid.at(
            x + 2,
            bottom + 2,
            Some(theme.muted),
            &format!(
                "Used {}/{} GiB | persist {} | gate {}",
                frame.display.cache_used_gib,
                frame.display.cache_limit_gib,
                frame.display.cache_persistent,
                frame.view.cache_writes_gated
            ),
            lw - 4,
        );
    } else {
        grid.at(
            x + 2,
            bottom + 2,
            Some(theme.muted),
            "Splash cache metrics unavailable",
            lw - 4,
        );
    }

    grid.at(
        x + 2,
        bottom + 3,
        Some(theme.data),
        "READ  •  Splash cache",
        lw - 4,
    );
    let read_height = ((bh as i64 - 7) / 2).max(1) as usize;
    plot_history(
        grid,
        x + 2,
        bottom + 4,
        graphw,
        read_height,
        &frame.state.cache_read_history,
        0.0,
        theme.data_ramp,
    );
    let write_y = bottom + 4 + read_height;

    // The device row, when a `--cache-dir` volume resolved, costs one row of
    // the write chart so nothing overlaps.
    let device = frame.sample.volume.as_ref().map(|volume| {
        format!(
            "Device {}  R {} W {} MiB/s  (all processes)",
            volume.device,
            Frame::rate_text(frame.rates.volume_read_mib),
            Frame::rate_text(frame.rates.volume_write_mib)
        )
    });
    let reserve = if device.is_some() { 1 } else { 0 };
    let write_height = ((bottom + bh - 2 - write_y - 1 - reserve) as i64).max(1) as usize;
    grid.at(
        x + 2,
        write_y,
        Some(theme.hot),
        "WRITE •  Splash cache",
        lw - 4,
    );
    plot_history(
        grid,
        x + 2,
        write_y + 1,
        graphw,
        write_height,
        &frame.state.cache_write_history,
        0.0,
        theme.hot_ramp,
    );

    if let Some(device) = &device {
        grid.at(x + 2, bottom + bh - 3, Some(theme.muted), device, lw - 4);
    }
    grid.at(
        x + 2,
        bottom + bh - 2,
        Some(theme.muted),
        &longest_fit(
            format!(
                "Transferred R {} / W {} GiB | disk hit tokens {}",
                frame.display.disk_read_gib,
                frame.display.disk_written_gib,
                opt_u64_text(frame.view.disk_hit_tokens)
            ),
            format!(
                "R {} / W {} GiB | hit tokens {}",
                frame.display.disk_read_gib,
                frame.display.disk_written_gib,
                opt_u64_text(frame.view.disk_hit_tokens)
            ),
            lw - 4,
        ),
        lw - 4,
    );
}

// Geometry stays positional: every panel is drawn at one box with its own graph column.
#[allow(clippy::too_many_arguments)]
fn render_requests(
    grid: &mut Grid,
    frame: &Frame<'_>,
    theme: &Theme,
    rx: usize,
    bottom: usize,
    rw: usize,
    graphw2: usize,
    bh: usize,
) {
    draw_box(grid, theme, rx, bottom, rw, bh, "REQUESTS / PRESSURE");
    grid.at(
        rx + 2,
        bottom + 1,
        Some(theme.text),
        &format!(
            "Active {} | Queued {} | Waiting {}",
            frame.view.active, frame.view.scheduler_queued, frame.view.scheduler_waiting
        ),
        rw - 4,
    );
    grid.at(
        rx + 2,
        bottom + 2,
        Some(theme.muted),
        &format!(
            "Prefill {} | Decode {} | Phase {}",
            frame.view.scheduler_prefill, frame.view.scheduler_decode, frame.view.phase
        ),
        rw - 4,
    );
    grid.at(
        rx + 2,
        bottom + 3,
        Some(theme.data),
        &format!(
            "Hits {} | Restores {}",
            opt_u64_text(frame.view.cache_hits),
            opt_u64_text(frame.view.disk_restores)
        ),
        rw - 4,
    );
    grid.at(
        rx + 2,
        bottom + 4,
        Some(theme.hot),
        &format!(
            "KV Spill {} | Pending writes {}",
            frame.display.spill_requests,
            opt_u64_text(frame.view.cache_write_wait)
        ),
        rw - 4,
    );
    grid.at(
        rx + 2,
        bottom + 5,
        Some(frame.pressure_color(theme)),
        &format!(
            "Memory {} | Swap OUT {} MiB/s",
            frame.pressure_text(),
            Frame::rate_text(frame.rates.swap_out_mib)
        ),
        rw - 4,
    );
    grid.at(
        rx + 2,
        bottom + 6,
        Some(theme.muted),
        "Swap OUT history (macOS)",
        rw - 4,
    );
    let swaph = (bh as i64 - 9).max(0) as usize;
    if swaph > 0 {
        plot_history(
            grid,
            rx + 2,
            bottom + 7,
            graphw2,
            swaph,
            &frame.state.swap_out_history,
            0.0,
            theme.caution_ramp,
        );
    }
    grid.at(
        rx + 2,
        bottom + bh - 2,
        Some(theme.muted),
        &format!(
            "Headroom {} GiB | denied {}",
            frame.display.memory_headroom_gib,
            opt_u64_text(frame.view.denied_reservations)
        ),
        rw - 4,
    );
}

fn opt_u64_text(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "--".to_string())
}

fn opt_one_decimal(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "--".to_string())
}

/// The longest wording that fits the column. A clipped tail reads as a broken
/// label, so a narrow panel switches to the short form instead of showing half
/// a phrase.
fn longest_fit(full: String, short: String, width: usize) -> String {
    if full.chars().count() <= width {
        full
    } else {
        short
    }
}

#[cfg(test)]
mod tests {
    use super::longest_fit;

    #[test]
    fn a_label_that_fits_keeps_the_long_form() {
        assert_eq!(
            longest_fit(
                "Prefill avg 874.9 tok/s".to_string(),
                "Prefill 874.9 tok/s".to_string(),
                24,
            ),
            "Prefill avg 874.9 tok/s"
        );
    }

    #[test]
    fn a_label_that_does_not_fit_switches_form() {
        // 23 columns never fit in 21; the short form must be chosen instead.
        assert_eq!(
            longest_fit(
                "Prefill avg 874.9 tok/s".to_string(),
                "Prefill 874.9 tok/s".to_string(),
                21,
            ),
            "Prefill 874.9 tok/s"
        );
    }
}
