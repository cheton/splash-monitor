//! CLI contract: validation messages, exit codes, precedence and the gates
//! that decide whether a TTY is required.

mod common;

use common::{TestServer, TmpDir, BASE_STATUS, FIXTURE_SAMPLE};
use splash_monitor::cli;
use splash_monitor::theme::Mode;

fn no_env(_: &str) -> Option<String> {
    None
}

fn parse(args: &[&str]) -> Result<cli::Options, cli::CliError> {
    cli::parse(args.iter().map(|arg| arg.to_string()), &no_env)
}

fn parse_with(args: &[&str], env: &[(&str, &str)]) -> Result<cli::Options, cli::CliError> {
    cli::parse(args.iter().map(|arg| arg.to_string()), &|key| {
        env.iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    })
}

#[test]
fn interval_validation() {
    for (args, message) in [
        (vec!["--interval", "0"], "Interval must be >= 1."),
        (
            vec!["--interval", "-1"],
            "Interval must be a positive integer.",
        ),
        (
            vec!["--interval", "3.5"],
            "Interval must be a positive integer.",
        ),
        (
            vec!["--interval", "abc"],
            "Interval must be a positive integer.",
        ),
        (vec!["--interval"], "--interval requires a value"),
    ] {
        let error = parse(&args).expect_err("expected a CLI error");
        assert_eq!(error.code, 2, "{args:?}");
        assert_eq!(error.message.as_deref(), Some(message), "{args:?}");
    }
    assert_eq!(parse(&["--interval", "1"]).expect("valid").interval, 1);
    assert_eq!(parse(&[]).expect("default").interval, 5);
}

#[test]
fn theme_flag_and_environment_select_the_palette() {
    assert_eq!(parse(&[]).expect("default").theme, Mode::Auto);
    assert_eq!(
        parse(&["--theme", "light"]).expect("light").theme,
        Mode::Light
    );
    assert_eq!(
        parse_with(&[], &[("SPLASH_THEME", "dark")])
            .expect("environment")
            .theme,
        Mode::Dark
    );
    // The flag wins over the environment.
    assert_eq!(
        parse_with(&["--theme", "light"], &[("SPLASH_THEME", "dark")])
            .expect("flag wins")
            .theme,
        Mode::Light
    );
}

#[test]
fn invalid_palette_names_are_rejected() {
    let error = parse(&["--theme", "sepia"]).expect_err("expected a CLI error");
    assert_eq!(error.code, 2);
    assert_eq!(
        error.message.as_deref(),
        Some("Theme must be auto, dark, or light.")
    );

    let error = parse_with(&[], &[("SPLASH_THEME", "sepia")]).expect_err("expected a CLI error");
    assert_eq!(error.code, 2);
    assert_eq!(
        error.message.as_deref(),
        Some("Theme must be auto, dark, or light.")
    );
}

#[test]
fn removed_options_are_unknown_options() {
    for flag in ["--sensor-bin", "--no-history"] {
        let error = parse(&[flag, "/tmp/x"]).expect_err("expected a CLI error");
        assert_eq!(error.code, 2);
        assert!(error.show_usage);
        assert_eq!(
            error.message.as_deref(),
            Some(format!("Unknown option: {flag}").as_str())
        );
    }
}

#[test]
fn missing_option_value_shows_usage() {
    for flag in [
        "--url",
        "--log",
        "--cache-dir",
        "--fixtures",
        "--theme",
        "--api-key",
    ] {
        let error = parse(&[flag]).expect_err("expected a CLI error");
        assert_eq!(error.code, 2);
        assert!(error.show_usage, "{flag}");
        assert_eq!(
            error.message.as_deref(),
            Some(format!("{flag} requires a value").as_str())
        );
    }
}

#[test]
fn environment_validation() {
    let error = parse_with(&[], &[("SPLASH_HOST", "http://example.com")])
        .expect_err("expected a CLI error");
    assert_eq!(error.code, 2);
    assert_eq!(
        error.message.as_deref(),
        Some("SPLASH_HOST must not include a scheme. Use SPLASH_URL for a full URL.")
    );

    for port in ["0", "99999", "18o00", ""] {
        let result = parse_with(&[], &[("SPLASH_PORT", port)]);
        if port.is_empty() {
            // Empty means unset, same as `${SPLASH_PORT:-}`.
            assert_eq!(
                result.expect("empty port is unset").base_url,
                cli::DEFAULT_URL
            );
            continue;
        }
        let error = result.expect_err("expected a CLI error for port {port}");
        assert_eq!(error.code, 2, "port {port}");
        assert_eq!(
            error.message.as_deref(),
            Some("SPLASH_PORT must be an integer between 1 and 65535.")
        );
    }
}

#[test]
fn url_precedence() {
    let options = parse_with(
        &["--url", "http://cli.example:1234"],
        &[("SPLASH_URL", "http://env.example:4321")],
    )
    .expect("parse");
    assert_eq!(options.base_url, "http://cli.example:1234");

    let options = parse_with(
        &[],
        &[
            ("SPLASH_URL", "http://env.example:4321"),
            ("SPLASH_HOST", "host.example"),
            ("SPLASH_PORT", "20000"),
        ],
    )
    .expect("parse");
    assert_eq!(options.base_url, "http://env.example:4321");

    let options = parse_with(&[], &[("SPLASH_HOST", "host.example")]).expect("parse");
    assert_eq!(options.base_url, "http://host.example:18000");

    let options = parse_with(&[], &[("SPLASH_PORT", "20001")]).expect("parse");
    assert_eq!(options.base_url, "http://127.0.0.1:20001");

    assert_eq!(parse(&[]).expect("parse").base_url, cli::DEFAULT_URL);
}

#[test]
fn api_key_flag_overrides_environment() {
    assert!(parse(&[]).expect("parse").api_key.is_none());
    assert!(parse_with(&[], &[("SPLASH_API_KEY", "")])
        .expect("empty environment")
        .api_key
        .is_none());
    assert_eq!(
        parse_with(&[], &[("SPLASH_API_KEY", "env-key")])
            .expect("environment")
            .api_key
            .as_deref(),
        Some("env-key")
    );
    assert_eq!(
        parse_with(&["--api-key", "cli-key"], &[("SPLASH_API_KEY", "env-key")])
            .expect("flag wins")
            .api_key
            .as_deref(),
        Some("cli-key")
    );
}

#[test]
fn url_trailing_slash_is_stripped_once() {
    assert_eq!(
        parse(&["--url", "http://example.test:8080/"])
            .expect("parse")
            .base_url,
        "http://example.test:8080"
    );
    assert_eq!(
        parse(&["--url", "http://example.test:8080//"])
            .expect("parse")
            .base_url,
        "http://example.test:8080/"
    );
    assert_eq!(
        parse(&["--url", "http://example.test/base/"])
            .expect("parse")
            .base_url,
        "http://example.test/base"
    );
}

#[test]
fn flags_set_the_expected_options() {
    let options = parse(&[
        "--no-sensors",
        "--once",
        "--no-color",
        "--debug-sensors",
        "--json",
        "--cache-dir",
        "/tmp/cache",
        "--log",
        "/tmp/log.csv",
        "--fixtures",
        "/tmp/fixture.json",
    ])
    .expect("parse");
    assert!(!options.sensors);
    assert!(options.once);
    assert!(!options.color);
    assert!(options.debug_sensors);
    assert!(options.json);
    assert_eq!(
        options.cache_dir.as_deref(),
        Some(std::path::Path::new("/tmp/cache"))
    );
    assert_eq!(
        options.log_file.as_deref(),
        Some(std::path::Path::new("/tmp/log.csv"))
    );
    assert_eq!(
        options.fixtures.as_deref(),
        Some(std::path::Path::new("/tmp/fixture.json"))
    );

    let options = parse_with(&[], &[("SPLASH_CACHE_DIR", "/tmp/from-env")]).expect("parse");
    assert_eq!(
        options.cache_dir.as_deref(),
        Some(std::path::Path::new("/tmp/from-env"))
    );

    // --no-color also wins over NO_COLOR being absent.
    assert!(
        !parse_with(&["--no-color"], &[("NO_COLOR", "1")])
            .expect("parse")
            .color
    );
    assert!(!parse_with(&[], &[("NO_COLOR", "1")]).expect("parse").color);
    assert!(!parse_with(&[], &[("TERM", "dumb")]).expect("parse").color);
}

#[test]
fn help_exits_zero_without_a_terminal() {
    let output = common::run_binary(&["--help"], &[]);
    let (code, stdout, stderr) = common::codes(&output);
    assert_eq!(code, 0, "stderr {stderr}");
    assert!(stdout.starts_with("Usage: splash-monitor"));
    assert!(stdout.contains("--cache-dir"));
    assert!(!stdout.contains("--sensor-bin"));
    assert!(stderr.is_empty());
}

#[test]
fn unknown_option_exits_two_with_usage() {
    let output = common::run_binary(&["--sensor-bin", "/tmp/x"], &[]);
    let (code, _, stderr) = common::codes(&output);
    assert_eq!(code, 2);
    assert!(stderr.contains("Unknown option: --sensor-bin"));
    assert!(stderr.contains("Usage: splash-monitor"));
}

#[test]
fn bad_environment_exits_two_before_any_request() {
    let server = TestServer::start(BASE_STATUS);
    let output = common::run_binary(
        &["--once", "--url", &server.url],
        &[("SPLASH_PORT", "99999")],
    );
    let (code, _, stderr) = common::codes(&output);
    assert_eq!(code, 2);
    assert!(stderr.contains("SPLASH_PORT"));
    assert!(server.paths().is_empty(), "no request may be sent");
}

#[test]
fn interactive_run_needs_a_terminal() {
    // `output()` gives pipes for stdin/stdout: not a TTY.
    let output = common::run_binary(
        &["--fixtures", "/dev/null", "--url", "http://127.0.0.1:1"],
        &[],
    );
    let (code, _, stderr) = common::codes(&output);
    assert_eq!(code, 1);
    assert!(stderr.contains("interactive terminal"));
}

#[test]
fn once_and_json_work_in_a_pipe() {
    let dir = TmpDir::new("cli-pipe");
    let fixture = common::write_fixture(dir.path(), "sample.json", FIXTURE_SAMPLE);

    let output = common::run_binary(
        &[
            "--once",
            "--url",
            "http://127.0.0.1:1",
            "--fixtures",
            fixture.to_str().expect("path"),
        ],
        &[],
    );
    let (code, stdout, _) = common::codes(&output);
    assert_eq!(code, 0);
    assert!(stdout.contains("OFFLINE"));

    let output = common::run_binary(
        &["--json", "--fixtures", fixture.to_str().expect("path")],
        &[],
    );
    let (code, stdout, stderr) = common::codes(&output);
    assert_eq!(code, 0, "stderr {stderr}");
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json sample");
    for key in [
        "cpu_active_ratio",
        "cpu_core_ratios",
        "cpu_core_temps",
        "cpu_temp_avg",
        "gpu_active_ratio",
        "gpu_temp_avg",
        "gpu_power",
        "swap_used_bytes",
        "swap_in_pages",
        "swap_out_pages",
        "page_size",
        "pressure_level",
        "free_percent",
    ] {
        assert!(value.get(key).is_some(), "missing key {key}");
    }
    assert_eq!(value["cpu_core_temps"].as_array().expect("temps").len(), 14);
    assert_eq!(value["page_size"].as_u64(), Some(16384));
}

#[test]
fn url_flag_beats_splash_url_for_the_actual_request() {
    let chosen = TestServer::start(BASE_STATUS);
    let ignored = TestServer::start(BASE_STATUS);
    let output = common::run_binary(
        &["--once", "--no-sensors", "--url", chosen.url.as_str()],
        &[("SPLASH_URL", ignored.url.as_str())],
    );
    let (code, stdout, _) = common::codes(&output);
    assert_eq!(code, 0);
    assert!(stdout.contains("STANDBY"), "body came from --url");
    assert_eq!(chosen.paths(), vec!["/status".to_string()]);
    assert!(ignored.paths().is_empty(), "SPLASH_URL must lose");
}

#[test]
fn api_key_authenticates_status_requests_and_401_is_offline() {
    let server = TestServer::start_with_auth(BASE_STATUS, Some("cli-key"));
    let args = ["--once", "--no-sensors", "--url", server.url.as_str()];

    let output = common::run_binary(&args, &[]);
    let (code, stdout, _) = common::codes(&output);
    assert_eq!(code, 0);
    assert!(stdout.contains("OFFLINE"), "missing key must be rejected");
    assert_eq!(server.authorization_headers(), vec![None]);

    let output = common::run_binary(&args, &[("SPLASH_API_KEY", "wrong-key")]);
    let (code, stdout, _) = common::codes(&output);
    assert_eq!(code, 0);
    assert!(stdout.contains("OFFLINE"), "401 must render offline");

    let output = common::run_binary(&args, &[("SPLASH_API_KEY", "cli-key")]);
    let (code, stdout, _) = common::codes(&output);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("STANDBY"),
        "environment key must authenticate"
    );

    let output = common::run_binary(
        &[
            "--once",
            "--no-sensors",
            "--url",
            &server.url,
            "--api-key",
            "cli-key",
        ],
        &[("SPLASH_API_KEY", "wrong-key")],
    );
    let (code, stdout, _) = common::codes(&output);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("STANDBY"),
        "CLI key must override environment"
    );
    assert_eq!(server.paths(), vec!["/status"; 4]);
    assert_eq!(
        server.authorization_headers(),
        vec![
            None,
            Some("Bearer wrong-key".to_string()),
            Some("Bearer cli-key".to_string()),
            Some("Bearer cli-key".to_string()),
        ]
    );
}

#[test]
fn json_mode_sends_the_bearer_key_on_both_status_requests() {
    let server = TestServer::start_with_auth(BASE_STATUS, Some("cli-key"));
    let output = common::run_binary(
        &[
            "--json",
            "--no-sensors",
            "--url",
            &server.url,
            "--api-key",
            "cli-key",
        ],
        &[],
    );
    let (code, stdout, stderr) = common::codes(&output);
    assert_eq!(code, 0, "stderr {stderr}");
    serde_json::from_str::<serde_json::Value>(&stdout).expect("JSON output");
    assert_eq!(server.paths(), vec!["/status"; 2]);
    assert_eq!(
        server.authorization_headers(),
        vec![Some("Bearer cli-key".to_string()); 2]
    );
}

#[test]
fn splash_url_and_host_port_reach_the_server() {
    let server = TestServer::start(BASE_STATUS);
    let output = common::run_binary(
        &["--once", "--no-sensors"],
        &[("SPLASH_URL", server.url.as_str())],
    );
    let (code, stdout, _) = common::codes(&output);
    assert_eq!(code, 0);
    assert!(stdout.contains("STANDBY"));
    assert_eq!(server.paths(), vec!["/status".to_string()]);

    let server = TestServer::start(BASE_STATUS);
    let port = server.url.rsplit(':').next().expect("port").to_string();
    let output = common::run_binary(
        &["--once", "--no-sensors"],
        &[("SPLASH_HOST", "127.0.0.1"), ("SPLASH_PORT", &port)],
    );
    let (code, stdout, _) = common::codes(&output);
    assert_eq!(code, 0);
    assert!(stdout.contains("STANDBY"));
    assert_eq!(server.paths(), vec!["/status".to_string()]);
}

#[test]
fn offline_endpoint_renders_offline_and_exits_zero() {
    let output = common::run_binary(
        &["--once", "--no-sensors", "--url", "http://127.0.0.1:1"],
        &[],
    );
    let (code, stdout, _) = common::codes(&output);
    assert_eq!(code, 0);
    assert!(stdout.contains("OFFLINE"));
    assert!(stdout.contains("--%"));
    assert!(!stdout.contains("0°C"));
}

#[test]
fn once_keeps_splash_and_local_readings_separate() {
    // Locate a panel by its title and horizontal bounds, rather than by
    // physical row: grid_rows omits rows that are entirely blank.
    fn panel(rows: &[String], x: usize, width: usize, title: &str) -> String {
        let mut collecting = false;
        let mut contents = Vec::new();
        for row in rows {
            let part: String = row.chars().skip(x - 1).take(width).collect();
            if !collecting {
                if !part.contains(title) {
                    continue;
                }
                collecting = true;
            }
            let bottom = part.starts_with('└');
            contents.push(part);
            if bottom {
                return contents.join("\n");
            }
        }
        panic!("panel {title:?} not found or incomplete:\n{}", rows.join("\n"));
    }

    let server = TestServer::start(BASE_STATUS);
    let dir = TmpDir::new("cli-source-separation");
    let mut sample: serde_json::Value =
        serde_json::from_str(FIXTURE_SAMPLE).expect("sample fixture JSON");
    sample["pressure_level"] = 2.into();
    let fixture = common::write_fixture(dir.path(), "sample.json", &sample.to_string());
    let fixture = fixture.to_str().expect("fixture path");

    for online in [true, false] {
        let url = if online {
            server.url.as_str()
        } else {
            "http://127.0.0.1:1"
        };
        let metal = if online { "41.02/42.00" } else { "--/--" };
        let thermal = if online { "nominal" } else { "--" };

        for (cols, rows) in [(100usize, 30usize), (72, 20), (36, 10)] {
            let columns = cols.to_string();
            let lines = rows.to_string();
            let output = common::run_binary(
                &["--once", "--no-color", "--fixtures", fixture, "--url", url],
                &[("COLUMNS", &columns), ("LINES", &lines)],
            );
            let (code, stdout, stderr) = common::codes(&output);
            assert_eq!(code, 0, "{cols}x{rows}, online={online}: {stderr}");
            assert!(
                !stdout.contains("\x1b[38;2;") && !stdout.contains("\x1b[0m"),
                "--no-color emitted color escapes"
            );
            let visible = common::grid_rows(&stdout, cols, rows);
            let screen = visible.join("\n");

            if cols == 100 {
                let width = cols - 2;
                let left_width = (width - 2) / 2;
                let right_x = 2 + left_width + 2;
                let right_width = width - left_width - 2;
                let header = panel(&visible, 2, width, "SPLASH MONITOR");
                let inference = panel(&visible, 2, left_width, "[SPLASH] INFERENCE / REQUESTS");
                let hardware = panel(&visible, right_x, right_width, "[LOCAL] CPU / GPU");
                let cache = panel(&visible, 2, left_width, "[SPLASH] CACHE / KV SPILL");
                let pressure = panel(&visible, right_x, right_width, "[LOCAL] PRESSURE / SWAP");
                let footer = panel(&visible, 2, width, "[SPLASH] MEMORY / CONFIG");

                assert!(header.contains(if online { "STANDBY" } else { "OFFLINE" }), "{header}");
                assert!(inference.contains(&format!("Thermal {thermal}")), "{inference}");
                assert!(cache.contains(if online { "KV Spill 278" } else { "KV Spill --" }), "{cache}");
                if online {
                    assert!(cache.contains("Hits 342"), "{cache}");
                }
                assert!(footer.contains(&format!("Metal {metal} GiB")), "{footer}");
                assert!(hardware.contains("62.0%") && hardware.contains("14.8%"), "{hardware}");
                assert!(pressure.contains("WARNING") && pressure.contains("Free 30%"), "{pressure}");
                assert!(pressure.contains("Swap 6.78 GiB"), "{pressure}");
                assert!(pressure.contains("Device disk3 (all processes)"), "{pressure}");

                for splash in [&inference, &cache, &footer] {
                    for local in ["62.0%", "14.8%", "6.78 GiB", "WARNING", "disk3"] {
                        assert!(!splash.contains(local), "local {local:?} in Splash panel:\n{splash}");
                    }
                }
                for local in [&hardware, &pressure] {
                    for splash in ["Thermal", "nominal", "Metal", "41.02/42.00", "1048576", "KV Spill", "Hits 342"] {
                        assert!(!local.contains(splash), "Splash {splash:?} in local panel:\n{local}");
                    }
                }
            } else if cols == 72 {
                let width = cols - 2;
                let left_width = (width - 2) / 2;
                let right_x = 2 + left_width + 2;
                let right_width = width - left_width - 2;
                let status = panel(&visible, 2, left_width, "[SPLASH] STATUS");
                let hardware = panel(&visible, right_x, right_width, "[LOCAL] CPU / GPU");
                let activity = panel(&visible, 2, width, "[SPLASH] ACTIVITY");
                let pressure = panel(&visible, 2, width, "[LOCAL] PRESSURE / SWAP");
                let spill = panel(&visible, 2, width, "[SPLASH] KV SPILL");

                assert!(status.contains(if online { "ONLINE" } else { "OFFLINE" }), "{status}");
                assert!(status.contains(&format!("Thermal {thermal}")), "{status}");
                assert!(activity.contains(&format!("{metal} GiB")), "{activity}");
                assert!(spill.contains(if online { "Pages 1048576" } else { "Pages --" }), "{spill}");
                assert!(hardware.contains("GPU 62.0%") && hardware.contains("CPU 14.8%"), "{hardware}");
                assert!(pressure.contains("WARNING") && pressure.contains("Swap 6.78 GiB"), "{pressure}");

                for splash in [&status, &activity, &spill] {
                    for local in ["62.0%", "14.8%", "6.78 GiB", "WARNING", "disk3"] {
                        assert!(!splash.contains(local), "local {local:?} in Splash panel:\n{splash}");
                    }
                }
                for local in [&hardware, &pressure] {
                    for splash in ["Thermal", "nominal", "Metal", "41.02/42.00", "1048576", "KV "] {
                        assert!(!local.contains(splash), "Splash {splash:?} in local panel:\n{local}");
                    }
                }
            } else {
                assert!(
                    visible.iter().any(|row| row == &format!("[SPLASH] Metal {metal} GiB")),
                    "{screen}"
                );
                assert!(visible.iter().any(|row| row == "[LOCAL] Swap 6.78 GiB"), "{screen}");
                assert!(
                    visible.iter().any(|row| row.starts_with("[LOCAL] WARNING | OUT ")
                        && row.ends_with("MiB/s")),
                    "{screen}"
                );
                assert!(
                    visible.iter().any(|row| row == if online { "[SPLASH] ONLINE" } else { "[SPLASH] OFFLINE" }),
                    "{screen}"
                );
            }
        }
    }

    assert_eq!(server.paths(), vec!["/status"; 3]);
}
