use std::path::PathBuf;

use crate::theme::Mode;

pub const DEFAULT_URL: &str = "http://127.0.0.1:18000";

#[derive(Debug)]
pub struct Options {
    pub interval: u64, // seconds, >= 1
    pub base_url: String,
    pub log_file: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
    pub sensors: bool, // false for --no-sensors
    pub once: bool,
    pub color: bool,
    pub theme: Mode,
    pub debug_sensors: bool,
    pub json: bool,
    pub fixtures: Option<PathBuf>,
}

#[derive(Debug)]
pub struct CliError {
    pub message: Option<String>,
    pub show_usage: bool,
    pub code: i32,
}

impl CliError {
    fn message_only(message: &str) -> Self {
        Self {
            message: Some(message.to_string()),
            show_usage: false,
            code: 2,
        }
    }

    fn with_usage(message: &str) -> Self {
        Self {
            message: Some(message.to_string()),
            show_usage: true,
            code: 2,
        }
    }
}

pub fn usage() -> String {
    String::from(
        "Usage: splash-monitor [--interval SECONDS] [--url BASE_URL] [--log FILE]\
                      [--once] [--no-color] [--theme auto|dark|light]\n\
\n\
Interactive Splash + macOS dashboard with inference, Splash memory,\n\
context capacity, KV spill, swap rates, pressure and in-session history.\n\
\n\
Options:\n\
  -i, --interval SECONDS   Refresh interval, default 5 (minimum 1)\n\
  -u, --url BASE_URL       Splash URL, default http://127.0.0.1:18000\n\
  -l, --log FILE           Append measurements to CSV\n\
      --cache-dir PATH     Optional device-level I/O diagnostics\n\
      --no-sensors         Do not query optional hardware sensors\n\
      --once               Render one frame, then exit (also works in a pipe)\n\
      --no-color           Disable colors (still uses cursor controls)\n\
      --theme MODE           Palette for colors: auto, dark, light (default auto)\n\
      --debug-sensors      Print macOS sensor diagnostics to stderr, then exit\n\
      --json               Print one telemetry JSON sample to stdout, then exit\n\
      --fixtures PATH      Read system telemetry from a JSON fixture file\n\
  -h, --help               Show this help\n\
\n\
Environment (read once at startup):\n\
  SPLASH_URL                 Full base URL, same form as --url\n\
  SPLASH_HOST                Host only, default 127.0.0.1\n\
  SPLASH_PORT                Port only, default 18000\n\
  SPLASH_CACHE_DIR           Default path for --cache-dir\n\
  SPLASH_THEME                 Default palette: auto, dark, light\n\
Priority: --url > SPLASH_URL > SPLASH_HOST / SPLASH_PORT > default.\n\
\n\
Press q to quit, or Ctrl-C. No sudo required.\n",
    )
}

/// Env values are treated as absent when unset or empty, matching the
/// `${VAR:-}` / `${VAR:-default}` forms used by the previous shell CLI.
fn env_value(env: &impl Fn(&str) -> Option<String>, key: &str) -> Option<String> {
    env(key).filter(|value| !value.is_empty())
}

pub fn parse(
    args: impl Iterator<Item = String>,
    env: &impl Fn(&str) -> Option<String>,
) -> Result<Options, CliError> {
    // Environment validation runs before argument parsing.
    if let Some(host) = env_value(env, "SPLASH_HOST") {
        if host.contains("://") {
            return Err(CliError::message_only(
                "SPLASH_HOST must not include a scheme. Use SPLASH_URL for a full URL.",
            ));
        }
    }
    let port = env_value(env, "SPLASH_PORT");
    if let Some(port) = &port {
        let numeric = port.chars().all(|c| c.is_ascii_digit()) && port.parse::<u32>().is_ok();
        let in_range = numeric && {
            match port.parse::<u32>() {
                Ok(value) => (1..=65535).contains(&value),
                Err(_) => false,
            }
        };
        if !in_range {
            return Err(CliError::message_only(
                "SPLASH_PORT must be an integer between 1 and 65535.",
            ));
        }
    }

    // URL precedence: --url > SPLASH_URL > SPLASH_HOST/SPLASH_PORT > default.
    let mut base_url = env_value(env, "SPLASH_URL").unwrap_or_default();
    if base_url.is_empty() && (env_value(env, "SPLASH_HOST").is_some() || port.is_some()) {
        let host = env_value(env, "SPLASH_HOST").unwrap_or_else(|| "127.0.0.1".to_string());
        let port = port.unwrap_or_else(|| "18000".to_string());
        base_url = format!("http://{host}:{port}");
    }
    if base_url.is_empty() {
        base_url = DEFAULT_URL.to_string();
    }

    let mut interval = String::from("5");
    let mut log_file: Option<PathBuf> = None;
    let mut cache_dir = env_value(env, "SPLASH_CACHE_DIR").map(PathBuf::from);
    let mut sensors = true;
    let mut once = false;
    let mut color =
        env_value(env, "NO_COLOR").is_none() && env_value(env, "TERM").as_deref() != Some("dumb");
    let mut theme = env_value(env, "SPLASH_THEME");
    let mut debug_sensors = false;
    let mut json = false;
    let mut fixtures: Option<PathBuf> = None;
    let mut url_from_cli = None;

    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let mut take_value = |flag: &str| -> Result<String, CliError> {
            args.next()
                .ok_or_else(|| CliError::with_usage(&format!("{flag} requires a value")))
        };
        match arg.as_str() {
            "-i" | "--interval" => interval = take_value("--interval")?,
            "-u" | "--url" => {
                let value = take_value("--url")?;
                // Strip exactly one trailing slash.
                url_from_cli = Some(
                    value
                        .strip_suffix('/')
                        .map_or(value.clone(), |s| s.to_string()),
                );
            }
            "-l" | "--log" => log_file = Some(PathBuf::from(take_value("--log")?)),
            "--cache-dir" => cache_dir = Some(PathBuf::from(take_value("--cache-dir")?)),
            "--no-sensors" => sensors = false,
            "--once" => once = true,
            "--no-color" => color = false,
            "--theme" => theme = Some(take_value("--theme")?),
            "--debug-sensors" => debug_sensors = true,
            "--json" => json = true,
            "--fixtures" => fixtures = Some(PathBuf::from(take_value("--fixtures")?)),
            "-h" | "--help" => {
                return Ok(Options {
                    interval: 5,
                    base_url,
                    log_file: None,
                    cache_dir: None,
                    sensors: true,
                    once: false,
                    color,
                    theme: Mode::Auto,
                    debug_sensors: false,
                    json: false,
                    fixtures: None,
                })
            }
            _ => {
                return Err(CliError::with_usage(&format!("Unknown option: {arg}")));
            }
        }
    }
    if let Some(url) = url_from_cli {
        base_url = url;
    }

    if interval.is_empty() || !interval.chars().all(|c| c.is_ascii_digit()) {
        return Err(CliError::message_only(
            "Interval must be a positive integer.",
        ));
    }
    let interval: u64 = interval
        .parse()
        .map_err(|_| CliError::message_only("Interval must be >= 1."))?;
    if interval < 1 {
        return Err(CliError::message_only("Interval must be >= 1."));
    }

    let theme = match theme.as_deref() {
        Some(value) => Mode::parse(value)
            .ok_or_else(|| CliError::message_only("Theme must be auto, dark, or light."))?,
        None => Mode::Auto,
    };

    Ok(Options {
        interval,
        base_url,
        log_file,
        cache_dir,
        sensors,
        once,
        color,
        theme,
        debug_sensors,
        json,
        fixtures,
    })
}
