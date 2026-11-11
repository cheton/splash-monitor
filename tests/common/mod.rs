//! Shared fixtures for the integration suites: the baseline `/status` body,
//! a `SystemSample` fixture, a render harness and a minimal HTTP server.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use splash_monitor::render::{self, Frame};
use splash_monitor::state::{DashboardState, FrameRates};
use splash_monitor::status::SplashView;
use splash_monitor::system::SystemSample;
use splash_monitor::theme::Theme;

/// One timestamp shared by every derived rate in the fixtures.
pub const NOW: i64 = 1_731_225_600;

/// Baseline `/status` body, extended from the previous suite's fixture with the
/// field groups the renderer actually displays. `configured_memory_limit_bytes`
/// is exactly 42.00 GiB and `weights.released` is `true` with no active
/// requests, so the full layout must show `STANDBY`.
pub const BASE_STATUS: &str = r#"{
  "schema_version": 8,
  "ready": true,
  "thermal_state": "nominal",
  "capabilities": {"max_context_tokens": 262144},
  "memory_actual": {"current_bytes": 44044889620, "peak_bytes": 43154735104},
  "memory_plan": {
    "model": {"model_name": "ISTA-DASLab/Qwen3.8-Flash-Next-GSQ-RCO-GGUF:IQ3_XXS"},
    "budget": {
      "configured_memory_limit_bytes": 45097156608,
      "context_bounds": {"window": 262144, "resident": 139257, "spilled": 262144},
      "kv_capacity_tokens": 139264
    }
  },
  "memory_governor": {"headroom_bytes": 10737418, "denied_reservations": 3751},
  "kv_spill": {
    "requests": {"spill": 278, "resident": 1},
    "pages": {"spilled": 1048576, "spilled_bytes": 4300000000},
    "write_through": {"bytes": 52428800},
    "block_cache": {"read_bytes": 104857600}
  },
  "active_requests": [],
  "weights": {"released": true, "idle_release_seconds": 600, "restores": 6},
  "disk": {
    "used_bytes": 27917287424,
    "capacity_bytes": 999000000000,
    "limit_bytes": 34359738368,
    "volume_available_bytes": 512000000000,
    "writes_gated": false,
    "persistent": false,
    "read_bytes": 1288490188,
    "written_bytes": 536870912,
    "kv_restores": 3924,
    "write_behind": {"waiting": 2},
    "write_budget_bytes": 8388608
  },
  "cache": {"hits": 342, "kv_disk_hit_tokens": 125568},
  "scheduler": {
    "queued": 0,
    "waiting_resources": 0,
    "waiting_prefix": 0,
    "waiting_mask": 0,
    "prefilling": 0,
    "decoding": 1
  },
  "metrics": {
    "decode_gap_ms": {"p95": 131.1},
    "ttft_ms": {"p95": 11244.8},
    "prefill_tokens_per_second": 867.1,
    "decode_output_tokens": 900000
  }
}"#;

/// A `SystemSample` fixture for `--fixtures` and the probe seam: 18 logical
/// cores, 14 `PMU tdie` sensors, swap used exactly 6.78 GiB.
pub const FIXTURE_SAMPLE: &str = r#"{
  "cpu_active_ratio": 0.148,
  "cpu_cores": [0.21, 0.12, 0.06, 0.04, 0.03, 0.02, 0.01, 0.0, 0.0, 0.0, 0.01, 0.05,
                0.21, 0.2, 0.2, 0.19, 0.2, 0.21],
  "cpu_sensors": [
    {"label": "PMU tdie 1", "celsius": 84.9},
    {"label": "PMU tdie 2", "celsius": 81.8},
    {"label": "PMU tdie 3", "celsius": 78.0},
    {"label": "PMU tdie 4", "celsius": 78.7},
    {"label": "PMU tdie 5", "celsius": 82.5},
    {"label": "PMU tdie 6", "celsius": 77.8},
    {"label": "PMU tdie 7", "celsius": 77.9},
    {"label": "PMU tdie 8", "celsius": 82.3},
    {"label": "PMU tdie 9", "celsius": 77.7},
    {"label": "PMU tdie 10", "celsius": 76.9},
    {"label": "PMU tdie 11", "celsius": 76.6},
    {"label": "PMU tdie 12", "celsius": 76.9},
    {"label": "PMU tdie 13", "celsius": 86.5},
    {"label": "PMU tdie 14", "celsius": 51.8}
  ],
  "cpu_package": 79.0,
  "gpu_active_ratio": 0.62,
  "gpu_temp": 58.4,
  "gpu_power_w": 12.7,
  "swap_used_bytes": 7279969567,
  "swap_in_pages": 100,
  "swap_out_pages": 200,
  "page_size": 16384,
  "pressure_level": 1,
  "free_percent": 30,
  "volume": {"device": "disk3", "read_bytes": 104857600, "write_bytes": 20971520}
}"#;

/// The fixture `SystemSample`, read through `FixtureProbe`.
pub fn sample_fixture() -> SystemSample {
    Harness::from_fixture(None, FIXTURE_SAMPLE).sample
}

pub fn write_fixture(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).expect("write fixture");
    path
}

/// Temporary directory per test, removed on drop.
pub struct TmpDir(PathBuf);

impl TmpDir {
    pub fn new(label: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            + std::process::id() as u128;
        let base = std::env::temp_dir().join(format!("splash-monitor-test-{label}-{unique:x}"));
        std::fs::create_dir_all(&base).expect("temp dir");
        TmpDir(base)
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `view` + `sample` + `state` + `rates`, the same pairing `src/main.rs` builds.
pub struct Harness {
    pub view: SplashView,
    pub sample: SystemSample,
    pub state: DashboardState,
    pub rates: FrameRates,
}

impl Harness {
    pub fn offline() -> Self {
        Self::build(None, SystemSample::default())
    }

    pub fn with(body: Option<&str>, sample: SystemSample) -> Self {
        Self::build(body, sample)
    }

    /// Builds the sample through the public `FixtureProbe` seam, so the suites
    /// exercise the same JSON contract as `--fixtures`.
    pub fn from_fixture(body: Option<&str>, sample_json: &str) -> Self {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let name = format!(
            "splash-sample-{}-{}.json",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = write_fixture(&std::env::temp_dir(), &name, sample_json);
        use splash_monitor::system::SystemProbe;
        let sample = splash_monitor::system::FixtureProbe::new(&path).sample();
        let _ = std::fs::remove_file(&path);
        Self::build(body, sample)
    }

    fn build(body: Option<&str>, sample: SystemSample) -> Self {
        let view = splash_monitor::status::parse_status(body);
        let mut state = DashboardState::default();
        let rates = state.update_frame(&view, &sample, NOW);
        Harness {
            view,
            sample,
            state,
            rates,
        }
    }

    pub fn frame(&self, sensors_enabled: bool) -> Frame<'_> {
        Frame {
            interval: 5,
            now_text: "2026-11-10 12:00:00".to_string(),
            view: &self.view,
            display: self.view.display(),
            sample: &self.sample,
            rates: &self.rates,
            state: &self.state,
            sensors_enabled,
        }
    }

    pub fn render(&self, cols: usize, rows: usize, color: bool) -> String {
        self.render_with_theme(cols, rows, color, &Theme::dark())
    }

    pub fn render_with_theme(
        &self,
        cols: usize,
        rows: usize,
        color: bool,
        theme: &Theme,
    ) -> String {
        render::render(&self.frame(true), color, theme, cols, rows)
    }

    pub fn render_with_sensors(&self, cols: usize, rows: usize, sensors: bool) -> String {
        render::render(&self.frame(sensors), false, &Theme::dark(), cols, rows)
    }

    /// The frame as visible rows: cursor addressing resolved into a grid.
    pub fn visible_rows(&self, cols: usize, rows: usize) -> Vec<String> {
        grid_rows(&self.render(cols, rows, true), cols, rows)
    }

    pub fn visible_text(&self, cols: usize, rows: usize) -> String {
        self.visible_rows(cols, rows).join("\n")
    }
}

/// Resolve `\x1b[H\x1b[J` / `\x1b[{row};1H` addressing plus SGR sequences into
/// rows of text, so assertions can talk about what a terminal would show.
pub fn grid_rows(text: &str, cols: usize, rows: usize) -> Vec<String> {
    let mut cells = vec![vec![' '; cols]; rows];
    let mut row = 1usize;
    let mut col = 1usize;
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\x1b' {
            if chars.get(index + 1) == Some(&'[') {
                let mut cursor = index + 2;
                let mut params = String::new();
                while cursor < chars.len()
                    && (chars[cursor].is_ascii_digit() || chars[cursor] == ';')
                {
                    params.push(chars[cursor]);
                    cursor += 1;
                }
                if cursor < chars.len() {
                    if chars[cursor] == 'H' {
                        let parts: Vec<&str> =
                            params.split(';').filter(|part| !part.is_empty()).collect();
                        row = parts
                            .first()
                            .and_then(|part| part.parse::<usize>().ok())
                            .unwrap_or(1)
                            .max(1);
                        col = parts
                            .get(1)
                            .and_then(|part| part.parse::<usize>().ok())
                            .unwrap_or(1)
                            .max(1);
                    }
                    index = cursor + 1;
                    continue;
                }
            }
            index += 1;
            continue;
        }
        if ch != '\r' && ch != '\n' && row <= rows && col <= cols {
            cells[row - 1][col - 1] = ch;
        }
        col += 1;
        index += 1;
    }
    cells
        .into_iter()
        .map(|line| line.into_iter().collect::<String>().trim_end().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Minimal `GET` server: serves one configurable body and records request paths.
pub struct TestServer {
    pub url: String,
    port: u16,
    body: Arc<Mutex<String>>,
    paths: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TestServer {
    pub fn start(body: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let port = listener.local_addr().expect("local addr").port();
        let body = Arc::new(Mutex::new(body.to_string()));
        let paths = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        // A blocking accept answers a request as soon as the kernel queued it.
        // A polled nonblocking accept adds delay, and under load that delay can
        // push a `--once` run past its 3 s request timeout and make a test
        // look offline. `Drop` connects once to release the blocked accept.
        let thread_body = Arc::clone(&body);
        let thread_paths = Arc::clone(&paths);
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve(stream, &thread_body, &thread_paths),
                    Err(_) => break,
                }
            }
        });

        TestServer {
            url: format!("http://127.0.0.1:{port}"),
            port,
            body,
            paths,
            stop,
            thread: Some(thread),
        }
    }

    pub fn set_body(&self, body: &str) {
        *self.body.lock().expect("body lock") = body.to_string();
    }

    pub fn paths(&self) -> Vec<String> {
        self.paths.lock().expect("paths lock").clone()
    }

    pub fn wait_for_requests(&self, count: usize) -> bool {
        for _ in 0..200 {
            if self.paths.lock().expect("paths lock").len() >= count {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }
}

fn serve(mut stream: TcpStream, body: &Arc<Mutex<String>>, paths: &Arc<Mutex<Vec<String>>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        // The shutdown connection sends nothing, so it is not a request.
        return;
    }
    let path = line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    paths.lock().expect("paths lock").push(path);
    let payload = body.lock().expect("body lock").clone();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        payload.len(),
        payload
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Unblock the accept loop, which is otherwise parked in `accept`.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Run the built binary with a scrubbed environment.
pub fn run_binary(args: &[&str], env: &[(&str, &str)]) -> std::process::Output {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_splash-monitor"));
    command.args(args);
    for key in [
        "SPLASH_URL",
        "SPLASH_HOST",
        "SPLASH_PORT",
        "SPLASH_CACHE_DIR",
        "NO_COLOR",
        "COLUMNS",
        "LINES",
        "TERM",
    ] {
        command.env_remove(key);
    }
    // Without a size the binary would query `/dev/tty` and follow whatever
    // window the test happened to run in; pin it so the layout is fixed.
    command.env("COLUMNS", "140").env("LINES", "42");
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("run splash-monitor")
}

pub fn codes(output: &std::process::Output) -> (i32, String, String) {
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}
