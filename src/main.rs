//! `splash-monitor` binary: CLI, sampling loop, terminal lifecycle and CSV.

use std::env;
use std::io::{IsTerminal, Write};
use std::time::Duration;

use splash_monitor::{
    cli, csv, http, macos, render, state, status,
    system::{self, SystemProbe},
    term,
    theme::Theme,
};

/// One agent for the whole run, matching `curl --connect-timeout 1 --max-time 3`.
struct Dashboard {
    agent: ureq::Agent,
    state: state::DashboardState,
}

fn main() {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    let wants_help = raw_args.iter().any(|arg| arg == "-h" || arg == "--help");

    let options = match cli::parse(raw_args.into_iter(), &|key| env::var(key).ok()) {
        Ok(options) => options,
        Err(error) => {
            if let Some(message) = &error.message {
                eprintln!("{message}");
            }
            if error.show_usage {
                eprint!("{}", cli::usage());
            }
            std::process::exit(error.code);
        }
    };
    if wants_help {
        print!("{}", cli::usage());
        return;
    }

    let needs_tty = !options.once && !options.json && !options.debug_sensors;
    if needs_tty && (!std::io::stdin().is_terminal() || !std::io::stdout().is_terminal()) {
        eprintln!("Run this monitor in an interactive terminal (or use --once).");
        std::process::exit(1);
    }

    if options.debug_sensors {
        run_debug_sensors(&options);
        return;
    }
    if options.json {
        run_json(&options);
        return;
    }

    let log = match &options.log_file {
        Some(path) => match csv::CsvLog::open(path) {
            Ok(log) => Some(log),
            Err(error) => {
                eprintln!("Cannot write log file {}: {error}", path.display());
                std::process::exit(1);
            }
        },
        None => None,
    };

    let mut probe = build_probe(&options);
    let mut dashboard = Dashboard {
        agent: http::build_agent(),
        state: state::DashboardState::default(),
    };

    // One palette per run. `--no-color`/`NO_COLOR` turns color off; otherwise
    // `--theme` decides, and `auto` asks the terminal through COLORFGBG.
    let theme = Theme::resolve(options.theme, env::var("COLORFGBG").ok().as_deref());

    if options.once {
        let (cols, rows) = terminal_size(true);
        let (view, sample, rates, now) = collect_frame(&mut probe, &mut dashboard, &options);
        let frame = build_frame(&options, &dashboard.state, &view, &sample, &rates, now);
        let layout = render::render(&frame, options.color, &theme, cols, rows);
        let mut out = std::io::stdout();
        let _ = out.write_all(layout.as_bytes());
        let _ = out.write_all(b"\n");
        let _ = out.flush();
        if let Some(log) = &log {
            if log.write_frame(&frame, now).is_err() {
                std::process::exit(1);
            }
        }
        return;
    }

    term::install_panic_hook();
    term::install_signal_handlers();
    let _terminal = term::InteractiveTerminal::enter();

    loop {
        if term::SHUTDOWN_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let (cols, rows) = terminal_size(false);
        let (view, sample, rates, now) = collect_frame(&mut probe, &mut dashboard, &options);
        let frame = build_frame(&options, &dashboard.state, &view, &sample, &rates, now);
        let layout = render::render(&frame, options.color, &theme, cols, rows);
        let mut out = std::io::stdout();
        let _ = out.write_all(layout.as_bytes());
        let _ = out.flush();
        if let Some(log) = &log {
            if log.write_frame(&frame, now).is_err() {
                eprintln!("Cannot append to log file; stopping.");
                std::process::exit(1);
            }
        }
        if term::wait_for_quit(Duration::from_secs(options.interval)) {
            break;
        }
    }
}

fn build_probe(options: &cli::Options) -> Box<dyn system::SystemProbe> {
    match &options.fixtures {
        Some(path) => Box::new(system::FixtureProbe::new(path)),
        None => Box::new(macos::MacProbe::new(
            options.sensors,
            options.cache_dir.as_deref(),
        )),
    }
}

fn now_epoch() -> i64 {
    unsafe { libc::time(std::ptr::null_mut()) as i64 }
}

/// One sampled frame: `/status`, telemetry, derived rates. `now` is read once
/// so CSV rows and every rate delta share the same clock.
fn collect_frame(
    probe: &mut Box<dyn system::SystemProbe>,
    dashboard: &mut Dashboard,
    options: &cli::Options,
) -> (
    status::SplashView,
    system::SystemSample,
    state::FrameRates,
    i64,
) {
    let now = now_epoch();
    let body = http::fetch_status(&dashboard.agent, &options.base_url);
    let view = status::parse_status(body.as_deref());
    let sample = probe.sample();
    let rates = dashboard.state.update_frame(&view, &sample, now);
    (view, sample, rates, now)
}

fn build_frame<'a>(
    options: &'a cli::Options,
    dashboard: &'a state::DashboardState,
    view: &'a status::SplashView,
    sample: &'a system::SystemSample,
    rates: &'a state::FrameRates,
    now: i64,
) -> render::Frame<'a> {
    render::Frame {
        interval: options.interval,
        now_text: csv::timestamp_local(now),
        view,
        display: view.display(),
        sample,
        rates,
        state: dashboard,
        sensors_enabled: options.sensors,
    }
}

fn terminal_size(once: bool) -> (usize, usize) {
    let mut cols = 100usize;
    let mut rows = 32usize;
    if let Ok((measured_cols, measured_rows)) = crossterm::terminal::size() {
        if measured_cols > 0 {
            cols = usize::from(measured_cols);
        }
        if measured_rows > 0 {
            rows = usize::from(measured_rows);
        }
    }
    if once {
        if let Some(value) = numeric_env("COLUMNS") {
            cols = value;
        }
        if let Some(value) = numeric_env("LINES") {
            rows = value;
        }
    }
    (cols, rows)
}

fn numeric_env(key: &str) -> Option<usize> {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
}

fn run_debug_sensors(options: &cli::Options) {
    match &options.fixtures {
        Some(path) => {
            let mut probe = system::FixtureProbe::new(path);
            let sample = probe.sample();
            eprintln!("fixture {}", path.display());
            eprintln!(
                "cpu_active_ratio = {}",
                opt_percent(sample.cpu_active_ratio)
            );
            eprintln!("cpu_package = {}", opt_celsius(sample.cpu_package));
            for (label, value) in &sample.cpu_sensors {
                eprintln!("sensor {label} = {value:.2}");
            }
            eprintln!(
                "free_percent = {}",
                sample
                    .free_percent
                    .map_or_else(|| "unavailable".to_string(), |value| value.to_string())
            );
        }
        None => {
            let mut probe = macos::MacProbe::new(true, options.cache_dir.as_deref());
            for line in probe.debug_lines() {
                eprintln!("{line}");
            }
        }
    }
    std::process::exit(0);
}

fn run_json(options: &cli::Options) {
    let mut probe = build_probe(options);
    let agent = http::build_agent();
    let mut dashboard = state::DashboardState::default();

    let now = now_epoch();
    let view = status::parse_status(http::fetch_status(&agent, &options.base_url).as_deref());
    let first = probe.sample();
    dashboard.update_frame(&view, &first, now);

    // CPU ratios and GPU watts are differences between two samples.
    std::thread::sleep(Duration::from_secs(1));
    let now = now_epoch();
    let view = status::parse_status(http::fetch_status(&agent, &options.base_url).as_deref());
    let sample = probe.sample();
    dashboard.update_frame(&view, &sample, now);

    let value = serde_json::json!({
        "cpu_active_ratio": sample.cpu_active_ratio.map(round3),
        "cpu_core_ratios": sample.cpu_cores.iter().map(|ratio| ratio.map(round3)).collect::<Vec<_>>(),
        "cpu_core_temps": sample.cpu_sensors.iter().map(|(label, celsius)| serde_json::json!({
            "label": label,
            "celsius": round3(*celsius),
        })).collect::<Vec<_>>(),
        "cpu_temp_avg": sample.cpu_package.map(round3),
        "gpu_active_ratio": sample.gpu_active_ratio.map(round3),
        "gpu_temp_avg": sample.gpu_temp.map(round3),
        "gpu_power": sample.gpu_power_w.map(round3),
        "swap_used_bytes": sample.swap_used_bytes,
        "swap_in_pages": sample.swap_in_pages,
        "swap_out_pages": sample.swap_out_pages,
        "page_size": sample.page_size,
        "pressure_level": sample.pressure_level,
        "free_percent": sample.free_percent,
    });
    println!("{value}");
    std::process::exit(0);
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn opt_percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.3}", value * 100.0))
        .unwrap_or_else(|| "unavailable".to_string())
}

fn opt_celsius(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "unavailable".to_string())
}
