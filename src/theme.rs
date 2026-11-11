//! Color tokens and palettes. Render code names an intent (`theme.good`,
//! `theme.muted`), never an RGB value, so a theme is changed in this file only.

/// One 24-bit color.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b }
    }

    /// Relative luminance in `0.0..=1.0`, used to check that a palette stays
    /// legible on the background it is meant for.
    pub fn luminance(&self) -> f64 {
        (0.2126 * f64::from(self.r) + 0.7152 * f64::from(self.g) + 0.0722 * f64::from(self.b))
            / 255.0
    }
}

/// A resolved palette: semantic tokens plus the vertical ramps used by area
/// graphs, ordered bright at the top of the plot to dim at the baseline.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    /// Panel titles.
    pub title: Color,
    /// Panel borders.
    pub border: Color,
    /// Primary readings and labels.
    pub text: Color,
    /// Secondary text, notes and placeholders.
    pub muted: Color,
    /// Keys and commands, such as the quit hint.
    pub hint: Color,
    /// Healthy state: nominal thermal class, normal pressure, low utilization.
    pub good: Color,
    /// Elevated state: warning pressure, mid utilization, swap activity.
    pub caution: Color,
    /// Hot state: high utilization, high die temperature.
    pub hot: Color,
    /// Broken state: offline endpoint, critical pressure, saturation.
    pub critical: Color,
    /// Inference throughput series.
    pub graph: Color,
    /// CPU-side readings.
    pub accent: Color,
    /// Cache and swap readings.
    pub data: Color,
    pub good_ramp: [Color; 3],
    pub caution_ramp: [Color; 3],
    pub hot_ramp: [Color; 3],
    pub graph_ramp: [Color; 3],
    pub accent_ramp: [Color; 3],
    pub data_ramp: [Color; 3],
}

/// Theme selection. `auto` reads the terminal's `COLORFGBG` hint and falls back
/// to the dark palette when the hint is absent or unclear.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Auto,
    Dark,
    Light,
}

impl Mode {
    /// Parse a `--theme` value or a `SPLASH_THEME` value.
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Some(Mode::Auto),
            "dark" => Some(Mode::Dark),
            "light" => Some(Mode::Light),
            _ => None,
        }
    }
}

impl Theme {
    /// The default palette for dark terminals: the colors of the previous
    /// Bash dashboard.
    pub const fn dark() -> Self {
        Theme {
            title: Color::rgb(117, 214, 230),
            border: Color::rgb(83, 104, 117),
            text: Color::rgb(223, 230, 236),
            muted: Color::rgb(139, 158, 169),
            hint: Color::rgb(245, 201, 116),
            good: Color::rgb(127, 220, 168),
            caution: Color::rgb(245, 201, 116),
            hot: Color::rgb(240, 160, 118),
            critical: Color::rgb(253, 110, 112),
            graph: Color::rgb(197, 157, 239),
            accent: Color::rgb(117, 214, 230),
            data: Color::rgb(117, 214, 230),
            good_ramp: [
                Color::rgb(134, 244, 165),
                Color::rgb(80, 210, 170),
                Color::rgb(39, 159, 146),
            ],
            caution_ramp: [
                Color::rgb(251, 222, 143),
                Color::rgb(223, 165, 106),
                Color::rgb(162, 113, 112),
            ],
            hot_ramp: [
                Color::rgb(255, 181, 126),
                Color::rgb(224, 132, 105),
                Color::rgb(172, 88, 120),
            ],
            graph_ramp: [
                Color::rgb(209, 175, 246),
                Color::rgb(156, 130, 222),
                Color::rgb(100, 95, 172),
            ],
            accent_ramp: [
                Color::rgb(150, 228, 240),
                Color::rgb(96, 186, 214),
                Color::rgb(46, 140, 172),
            ],
            data_ramp: [
                Color::rgb(150, 228, 240),
                Color::rgb(96, 186, 214),
                Color::rgb(46, 140, 172),
            ],
        }
    }

    /// Palette for light terminals: the same roles, darkened so every token
    /// reads against a white background instead of dissolving into it.
    pub const fn light() -> Self {
        Theme {
            title: Color::rgb(20, 104, 120),
            border: Color::rgb(158, 172, 183),
            text: Color::rgb(26, 39, 49),
            muted: Color::rgb(92, 108, 120),
            hint: Color::rgb(140, 95, 20),
            good: Color::rgb(15, 112, 73),
            caution: Color::rgb(146, 99, 14),
            hot: Color::rgb(176, 82, 44),
            critical: Color::rgb(166, 31, 40),
            graph: Color::rgb(92, 66, 150),
            accent: Color::rgb(18, 94, 140),
            data: Color::rgb(150, 60, 95),
            good_ramp: [
                Color::rgb(10, 92, 60),
                Color::rgb(38, 120, 90),
                Color::rgb(88, 152, 122),
            ],
            caution_ramp: [
                Color::rgb(138, 92, 10),
                Color::rgb(160, 118, 40),
                Color::rgb(186, 150, 90),
            ],
            hot_ramp: [
                Color::rgb(168, 72, 36),
                Color::rgb(190, 104, 66),
                Color::rgb(212, 140, 110),
            ],
            graph_ramp: [
                Color::rgb(78, 52, 136),
                Color::rgb(110, 86, 158),
                Color::rgb(148, 126, 182),
            ],
            accent_ramp: [
                Color::rgb(14, 80, 124),
                Color::rgb(48, 110, 150),
                Color::rgb(96, 140, 172),
            ],
            data_ramp: [
                Color::rgb(150, 60, 95),
                Color::rgb(175, 95, 125),
                Color::rgb(200, 135, 160),
            ],
        }
    }

    pub fn resolve(mode: Mode, colorfgbg: Option<&str>) -> Self {
        match mode {
            Mode::Dark => Theme::dark(),
            Mode::Light => Theme::light(),
            Mode::Auto => {
                if light_background(colorfgbg) {
                    Theme::light()
                } else {
                    Theme::dark()
                }
            }
        }
    }
}

/// `COLORFGBG` lists `foreground;background`. A background of `7` or `15` is a
/// light terminal; anything else, including an unset or unparseable value,
/// stays on the dark palette.
fn light_background(colorfgbg: Option<&str>) -> bool {
    let Some(value) = colorfgbg else {
        return false;
    };
    let Some(background) = value.rsplit(';').next() else {
        return false;
    };
    matches!(background.trim().parse::<u8>().ok(), Some(7) | Some(15))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_names_are_case_insensitive() {
        assert_eq!(Mode::parse("LIGHT"), Some(Mode::Light));
        assert_eq!(Mode::parse("Dark"), Some(Mode::Dark));
        assert_eq!(Mode::parse("sepia"), None);
    }

    #[test]
    fn auto_follows_the_reported_background() {
        assert_eq!(
            Theme::resolve(Mode::Auto, Some("15;0")).text,
            Theme::dark().text
        );
        // `COLORFGBG` uses palette indices: 7 is light grey, 15 is white.
        assert_eq!(
            Theme::resolve(Mode::Auto, Some("0;7")).text,
            Theme::light().text
        );
        assert_eq!(
            Theme::resolve(Mode::Auto, Some("15;15")).text,
            Theme::light().text
        );
        // No hint, or an unparseable hint, keeps the dark palette.
        assert_eq!(Theme::resolve(Mode::Auto, None).text, Theme::dark().text);
        assert_eq!(
            Theme::resolve(Mode::Auto, Some("future")).text,
            Theme::dark().text
        );
    }

    #[test]
    fn an_explicit_mode_ignores_the_terminal_hint() {
        assert_eq!(
            Theme::resolve(Mode::Light, Some("15;0")).text,
            Theme::light().text
        );
        assert_eq!(
            Theme::resolve(Mode::Dark, Some("234;255")).text,
            Theme::dark().text
        );
    }

    #[test]
    fn the_light_palette_stays_legible_on_a_light_background() {
        let light = Theme::light();
        let background = 1.0;
        // Body text, muted text and every series color must be clearly darker
        // than a white terminal background.
        for color in [
            light.text,
            light.muted,
            light.title,
            light.good,
            light.caution,
            light.hot,
            light.critical,
            light.graph,
            light.accent,
            light.data,
        ] {
            assert!(
                color.luminance() < background * 0.55,
                "{color:?} is too light"
            );
        }
        // The dark palette is the mirror image: text must be bright.
        assert!(Theme::dark().text.luminance() > 0.7);
    }

    #[test]
    fn ramps_run_from_strong_to_soft() {
        let dark = Theme::dark();
        for (name, ramp) in [
            ("good", dark.good_ramp),
            ("caution", dark.caution_ramp),
            ("hot", dark.hot_ramp),
            ("graph", dark.graph_ramp),
            ("accent", dark.accent_ramp),
            ("data", dark.data_ramp),
        ] {
            assert_eq!(ramp.len(), 3);
            assert_ne!(ramp[0], ramp[2], "{name} ramp is flat");
        }
    }
}
