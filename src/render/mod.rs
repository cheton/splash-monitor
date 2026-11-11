//! Coordinate-accurate renderer. Text is placed into a fixed grid and then
//! emitted as ANSI, so colored segments can never break visual alignment.
//!
//! The Braille history plot, the gradient meter and the panel geometry are
//! ports of the previous Bash dashboard.

pub mod compact;
pub mod full;

use std::collections::VecDeque;

use crate::state::{DashboardState, FrameRates};
use crate::status::{format_metric, num_gib, SplashDisplay, SplashView};
use crate::system::SystemSample;
use crate::theme::{Color, Theme};

pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    cells: Vec<(char, Option<Color>)>,
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self {
        Grid {
            cols,
            rows,
            cells: vec![(' ', None); cols.max(1) * rows.max(1)],
        }
    }

    fn index(&self, x: usize, y: usize) -> usize {
        (y - 1) * self.cols + (x - 1)
    }

    /// `at(x, y, color, text, maxlen)`: 1-based coordinates, clipped to the
    /// grid and truncated by characters, not bytes.
    pub fn at(&mut self, x: usize, y: usize, color: Option<Color>, text: &str, maxlen: usize) {
        if maxlen == 0 || y == 0 || x == 0 || x > self.cols || y > self.rows {
            return;
        }
        let limit = maxlen.min(self.cols - x + 1);
        for (offset, ch) in text.chars().take(limit).enumerate() {
            let cell = self.index(x + offset, y);
            self.cells[cell] = (ch, color);
        }
    }

    pub fn repeat_at(&mut self, x: usize, y: usize, color: Option<Color>, ch: char, count: usize) {
        for offset in 0..count {
            if x + offset > self.cols {
                break;
            }
            if y == 0 || y > self.rows {
                break;
            }
            let cell = self.index(x + offset, y);
            self.cells[cell] = (ch, color);
        }
    }

    /// Emits the frame: clear screen, then one positioned run per row.
    pub fn emit(&self, color_enabled: bool) -> String {
        let mut out = String::from("\x1b[H\x1b[J");
        for row in 1..=self.rows {
            let mut last = 0usize;
            for column in 1..=self.cols {
                if self.cells[self.index(column, row)].0 != ' ' {
                    last = column;
                }
            }
            if last == 0 {
                continue;
            }
            out.push_str(&format!("\x1b[{row};1H"));
            let mut current: Option<Color> = None;
            for column in 1..=last {
                let (ch, color) = self.cells[self.index(column, row)];
                if color_enabled && color != current {
                    match color {
                        Some(color) => {
                            out.push_str(&format!("\x1b[38;2;{};{};{}m", color.r, color.g, color.b))
                        }
                        None => out.push_str("\x1b[0m"),
                    }
                    current = color;
                }
                out.push(ch);
            }
            if color_enabled && current.is_some() {
                out.push_str("\x1b[0m");
            }
        }
        out.push_str(&format!("\x1b[{};1H", self.rows));
        out
    }
}

/// Box drawing. `box(x, y, w, h, title)`: title at `x + 3`, `maxlen = w - 6`.
pub fn draw_box(
    grid: &mut Grid,
    theme: &Theme,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    title: &str,
) {
    if w < 4 || h < 3 {
        return;
    }
    let horizontal = "─".repeat(w - 2);
    grid.at(x, y, Some(theme.border), &format!("┌{horizontal}┐"), 999);
    for row in 1..h - 1 {
        grid.at(x, y + row, Some(theme.border), "│", 1);
        grid.at(x + w - 1, y + row, Some(theme.border), "│", 1);
    }
    grid.at(
        x,
        y + h - 1,
        Some(theme.border),
        &format!("└{horizontal}┘"),
        999,
    );
    grid.at(x + 3, y, Some(theme.title), &format!(" {title} "), w - 6);
}

/// A Braille dot cell code maps straight onto its U+2800-block code point.
fn braille_glyph(code: u16) -> char {
    char::from_u32(0x2800 + u32::from(code)).unwrap_or(' ')
}

const SPARKLINE_GLYPHS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// In-session sparkline: scaled only to the samples actually collected.
pub fn sparkline(
    grid: &mut Grid,
    x: usize,
    y: usize,
    width: usize,
    values: &VecDeque<f64>,
    color: Color,
) {
    if width == 0 {
        return;
    }
    if values.len() < 2 {
        grid.at(x, y, Some(color), "Collecting history…", width);
        return;
    }
    let start = values.len().saturating_sub(width);
    let window: Vec<f64> = values.iter().skip(start).copied().collect();
    let min = window.iter().copied().fold(f64::INFINITY, f64::min);
    let max = window.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let result: String = window
        .iter()
        .map(|value| {
            let level = if max == min {
                1
            } else {
                ((value - min) * 7.0 / (max - min) + 0.5).floor() as i32
            };
            let level = level.clamp(0, 7) as usize;
            SPARKLINE_GLYPHS[level]
        })
        .collect();
    grid.at(x, y, Some(color), &result, width);
}

/// One-row Braille dot graph for a graph inside a table cell: two samples per
/// character, each drawn as a dot with one connector dot pointing at the next
/// sample, so the series reads as a light dotted line rather than a filled
/// block. `cap > 0` fixes the scale to `0..cap` so cells in one table are
/// comparable; otherwise the window's own range scales the dots, which spreads
/// out readings that sit in a narrow band, such as temperatures. It never draws
/// text, so a graph cannot overflow its cell. A cell with too few samples shows
/// placeholder dots instead of an empty gap.
pub fn dot_graph(
    grid: &mut Grid,
    x: usize,
    y: usize,
    width: usize,
    values: &VecDeque<f64>,
    cap: f64,
    color: Color,
) {
    if width == 0 {
        return;
    }
    if values.len() < 2 {
        grid.repeat_at(x, y, Some(color), '·', width);
        return;
    }

    let count = width * 2;
    let window: Vec<f64> = graph_window(values, count);
    let (min, max) = graph_scale(&window, cap);

    // Rows run top to bottom inside one character. Even samples use the left
    // dots and odd samples the right ones, so the line reads through the cell.
    let left_bits = [1u16, 2, 4, 64];
    let right_bits = [8u16, 16, 32, 128];
    let row_of = |value: f64| -> usize {
        let ratio = ((value - min) / (max - min)).clamp(0.0, 1.0);
        ((1.0 - ratio) * 3.0 + 0.5).floor().clamp(0.0, 3.0) as usize
    };

    let mut dots = vec![0u16; width];
    for (slot, value) in window.iter().enumerate() {
        let px = count - window.len() + slot;
        let column = px / 2;
        let bits = if px % 2 == 0 { &left_bits } else { &right_bits };
        let row = row_of(*value);
        dots[column] |= bits[row];
        // One connector dot toward the next sample keeps a steep change
        // readable as a line instead of as scattered dots.
        if let Some(next) = window.get(slot + 1) {
            let next_row = row_of(*next);
            if next_row != row {
                let step: i64 = if next_row > row { 1 } else { -1 };
                dots[column] |= bits[(row as i64 + step).clamp(0, 3) as usize];
            }
        }
    }

    let line: String = (0..width)
        .map(|column| braille_glyph(dots[column]))
        .collect();
    grid.at(x, y, Some(color), &line, width);
}

/// One-row Braille bar for a graph inside a table cell: two samples per
/// character, filled from the baseline up. Used where a value is better read
/// as an area, such as a temperature history; utilization uses `dot_graph`.
/// `cap > 0` fixes the scale to `0..cap`; otherwise the window's own range
/// scales the bar. A cell with too few samples shows placeholder dots.
pub fn bar_graph(
    grid: &mut Grid,
    x: usize,
    y: usize,
    width: usize,
    values: &VecDeque<f64>,
    cap: f64,
    color: Color,
) {
    if width == 0 {
        return;
    }
    if values.len() < 2 {
        grid.repeat_at(x, y, Some(color), '·', width);
        return;
    }

    let count = width * 2;
    let window: Vec<f64> = graph_window(values, count);
    let (min, max) = graph_scale(&window, cap);

    let left_bits = [1u16, 2, 4, 64];
    let right_bits = [8u16, 16, 32, 128];
    let mut dots = vec![0u16; width];
    for (slot, value) in window.iter().enumerate() {
        if *value <= min {
            continue;
        }
        let px = count - window.len() + slot;
        let column = px / 2;
        let ratio = (value - min) / (max - min);
        let top_row = ((1.0 - ratio) * 3.0 + 0.5).floor().clamp(0.0, 3.0) as usize;
        let bits = if px % 2 == 0 { &left_bits } else { &right_bits };
        for bit in &bits[top_row..4] {
            dots[column] |= bit;
        }
    }

    let line: String = (0..width)
        .map(|column| {
            if dots[column] == 0 {
                ' '
            } else {
                braille_glyph(dots[column])
            }
        })
        .collect();
    grid.at(x, y, Some(color), &line, width);
}

/// The most recent `count` samples of a history.
fn graph_window(values: &VecDeque<f64>, count: usize) -> Vec<f64> {
    let start = values.len().saturating_sub(count);
    values.iter().skip(start).copied().collect()
}

/// Vertical scale of a cell graph. `cap > 0` fixes it to `0..cap`. Otherwise
/// the window's own range is used, padded so a series is not crushed against
/// its edges; a window flatter than one unit is centered instead, so a steady
/// reading looks steady instead of amplifying sensor noise.
fn graph_scale(window: &[f64], cap: f64) -> (f64, f64) {
    if cap > 0.0 {
        return (0.0, cap);
    }
    let min = window.iter().copied().fold(f64::INFINITY, f64::min);
    let max = window.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if max - min < 1.0 {
        (min - 3.0, max + 3.0)
    } else {
        let pad = (max - min) * 0.25;
        (min - pad, max + pad)
    }
}

/// Braille timeline: two horizontal samples and four vertical pixels per cell,
/// filled from the sample down to the baseline. `cap > 0` fixes the scale to
#[allow(clippy::too_many_arguments)]
pub fn plot_history(
    grid: &mut Grid,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    values: &VecDeque<f64>,
    cap: f64,
    ramp: [Color; 3],
) {
    if width == 0 || height == 0 {
        return;
    }
    if values.is_empty() {
        grid.at(x, y, Some(ramp[2]), "Waiting for samples...", width);
        return;
    }
    if values.iter().all(|value| *value <= 0.0) {
        grid.at(
            x,
            y,
            Some(ramp[2]),
            "Idle - no activity in observed samples",
            width,
        );
        grid.repeat_at(x, y + height - 1, Some(ramp[2]), '·', width);
        return;
    }

    let samples: Vec<f64> = values.iter().copied().collect();
    let count = width * 2;
    let first = samples.len().saturating_sub(count);
    let window = &samples[first..];

    let mut min = window.iter().copied().fold(f64::INFINITY, f64::min);
    let mut max = window.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if cap > 0.0 {
        min = 0.0;
        max = cap;
    } else if max > min {
        let pad = (max - min) * 0.12;
        min -= pad;
        max += pad;
    } else {
        let pad = if max == 0.0 { 1.0 } else { max.abs() * 0.08 };
        min -= pad;
        max += pad;
    }
    if max <= min {
        max = min + 1.0;
    }

    let rows_pixels = height * 4;
    let mut dots = vec![0u16; height * width];
    let left_bits = [1u16, 2, 4, 64];
    let right_bits = [8u16, 16, 32, 128];
    for (slot, value) in window.iter().enumerate() {
        let px = count - window.len() + slot;
        let mut py = ((max - value) / (max - min) * (rows_pixels - 1) as f64 + 0.5).floor() as i64;
        if py < 0 {
            py = 0;
        }
        if py >= rows_pixels as i64 {
            py = rows_pixels as i64 - 1;
        }
        if *value <= 0.0 {
            continue;
        }
        let mut pixel = py;
        while pixel < rows_pixels as i64 {
            let row_index = pixel as usize;
            let cell = (row_index / 4) * width + px / 2;
            let bit = if px % 2 == 0 {
                left_bits[row_index % 4]
            } else {
                right_bits[row_index % 4]
            };
            if dots[cell] & bit == 0 {
                dots[cell] |= bit;
            }
            pixel += 1;
        }
    }

    let palette = ramp;
    for row in 0..height {
        let row_color = if row * 3 < height {
            palette[0]
        } else if row * 3 < height * 2 {
            palette[1]
        } else {
            palette[2]
        };
        let line: String = (0..width)
            .map(|column| {
                let code = dots[row * width + column];
                if code == 0 {
                    ' '
                } else {
                    braille_glyph(code)
                }
            })
            .collect();
        grid.at(x, y + row, Some(row_color), &line, width);
    }
}

/// Compact heat meter. An unknown value never renders as 0%.
pub fn gradient_meter(
    grid: &mut Grid,
    x: usize,
    y: usize,
    width: usize,
    value: Option<f64>,
    theme: &Theme,
) {
    if width == 0 {
        return;
    }
    let value = match value {
        Some(value) if value.is_finite() => value,
        _ => {
            grid.repeat_at(x, y, Some(theme.muted), '·', width);
            return;
        }
    };
    let value = value.clamp(0.0, 100.0);
    let used = (value * width as f64 / 100.0 + 0.5).floor() as usize;
    for column in 0..width {
        let (color, ch) = if column >= used {
            // A thin baseline reads as the track. A full light-shade block
            // makes an idle meter louder than the meter itself.
            (theme.muted, '▁')
        } else {
            let color = if column * 100 < width * 45 {
                theme.good
            } else if column * 100 < width * 70 {
                theme.caution
            } else if column * 100 < width * 88 {
                theme.hot
            } else {
                theme.critical
            };
            (color, '▰')
        };
        grid.at(x + column, y, Some(color), &ch.to_string(), 1);
    }
}

/// One rendered frame: `/status` values, sampled telemetry, derived rates and
/// the histories collected so far.
pub struct Frame<'a> {
    pub interval: u64,
    pub now_text: String,
    pub view: &'a SplashView,
    pub display: SplashDisplay,
    pub sample: &'a SystemSample,
    pub rates: &'a FrameRates,
    pub state: &'a DashboardState,
    pub sensors_enabled: bool,
}

impl Frame<'_> {
    pub fn hw_cpu(&self) -> Option<f64> {
        self.sample.cpu_active_ratio.map(|ratio| ratio * 100.0)
    }

    pub fn hw_gpu(&self) -> Option<f64> {
        self.sample.gpu_active_ratio.map(|ratio| ratio * 100.0)
    }

    pub fn cpu_percent_text(&self) -> String {
        format_metric(self.hw_cpu())
    }

    pub fn gpu_percent_text(&self) -> String {
        format_metric(self.hw_gpu())
    }

    pub fn cpu_temp_text(&self) -> String {
        format_metric(self.sample.cpu_package)
    }

    pub fn gpu_temp_text(&self) -> String {
        format_metric(self.sample.gpu_temp)
    }

    pub fn gpu_power_text(&self) -> String {
        format_metric(self.sample.gpu_power_w)
    }

    pub fn load_avg_text(&self) -> String {
        match self.sample.load_avg {
            Some([one, five, fifteen]) => {
                format!("Load avg {one:.2} {five:.2} {fifteen:.2}")
            }
            None => "Load avg --".to_string(),
        }
    }

    pub fn swap_used_gib(&self) -> String {
        self.sample
            .swap_used_bytes
            .map(num_gib)
            .unwrap_or_else(|| "--".to_string())
    }

    pub fn pressure_text(&self) -> &'static str {
        match self.sample.pressure_level {
            Some(1) => "NORMAL",
            Some(2) => "WARNING",
            Some(4) => "CRITICAL",
            _ => "UNKNOWN",
        }
    }

    pub fn pressure_color(&self, theme: &Theme) -> Color {
        match self.pressure_text() {
            "NORMAL" => theme.good,
            "CRITICAL" => theme.critical,
            _ => theme.caution,
        }
    }

    pub fn free_percent_text(&self) -> String {
        self.sample
            .free_percent
            .map(|value| value.to_string())
            .unwrap_or_else(|| "--".to_string())
    }

    pub fn thermal_color(&self, theme: &Theme) -> Color {
        match self.view.thermal_state.as_str() {
            "fair" => theme.caution,
            "serious" => theme.hot,
            "critical" => theme.critical,
            _ => theme.good,
        }
    }

    pub fn rate_text(value: Option<f64>) -> String {
        value
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "--".to_string())
    }

    /// The single hardware caveat line for this frame, if any.
    pub fn sensor_hint(&self) -> Option<String> {
        if !self.sensors_enabled {
            return Some("Sensors disabled (--no-sensors)".to_string());
        }
        if self.sample.telemetry_unavailable {
            return Some("Telemetry unavailable on this macOS".to_string());
        }
        if self.sample.gpu_active_ratio.is_none() {
            return Some("GPU utilization unavailable on this macOS".to_string());
        }
        if let Some(path) = &self.sample.device_unresolved {
            return Some(format!("Device I/O unresolved for {path}"));
        }
        None
    }
}

/// Full layout needs a genuinely large terminal; everything else is compact.
pub fn render(
    frame: &Frame<'_>,
    color_enabled: bool,
    theme: &Theme,
    cols: usize,
    rows: usize,
) -> String {
    let mut grid = Grid::new(cols, rows);
    if cols >= 100 && rows >= 30 {
        full::render(&mut grid, frame, theme);
    } else {
        compact::render(&mut grid, frame, theme);
    }
    grid.emit(color_enabled)
}
