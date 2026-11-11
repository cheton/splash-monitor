//! Interactive lifecycle in a real pty: alternate screen, key handling, exit.
//!
//! The pty buffer is small, so the master side must be drained continuously
//! while the child renders: a full buffer blocks its writes and it never gets
//! to read the key.

mod common;

use std::os::unix::io::FromRawFd;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Master side of a pty plus a reader thread that never stops draining it.
struct Pty {
    master: libc::c_int,
    bytes: Arc<Mutex<Vec<u8>>>,
    done: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl Pty {
    /// Opens a `rows` x `cols` pty and returns the master plus the slave copy
    /// that the child should inherit.
    fn open(rows: u16, cols: u16) -> (Self, libc::c_int) {
        let mut master: libc::c_int = 0;
        let mut slave: libc::c_int = 0;
        let mut window = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let opened = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut window,
            )
        };
        assert_eq!(opened, 0, "openpty failed");

        let bytes = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let reader_bytes = Arc::clone(&bytes);
        let reader_done = Arc::clone(&done);
        let reader = std::thread::spawn(move || loop {
            let mut buffer = [0u8; 4096];
            let read = unsafe {
                libc::read(
                    master,
                    buffer.as_mut_ptr() as *mut libc::c_void,
                    buffer.len(),
                )
            };
            if read > 0 {
                reader_bytes
                    .lock()
                    .expect("bytes lock")
                    .extend_from_slice(&buffer[..read as usize]);
                continue;
            }
            if read < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EIO) {
                break; // the slave side closed: the child is gone
            }
            if reader_done.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        });

        (
            Pty {
                master,
                bytes,
                done,
                reader: Some(reader),
            },
            slave,
        )
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes.lock().expect("bytes lock")).to_string()
    }

    fn wait_for(&self, needle: &str, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.text().contains(needle) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    fn send(&self, text: &str) {
        let written = unsafe {
            libc::write(
                self.master,
                text.as_ptr() as *const libc::c_void,
                text.len(),
            )
        };
        assert_eq!(written, text.len() as libc::ssize_t, "write to pty");
    }

    /// Collect everything the child wrote, then close the master.
    fn finish(mut self) -> String {
        // Give the restore sequences a moment to reach the master.
        std::thread::sleep(Duration::from_millis(200));
        self.done.store(true, Ordering::Relaxed);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        unsafe { libc::close(self.master) };
        self.text()
    }
}

fn spawn_on_pty(slave: libc::c_int, fixture: &str, url: &str) -> std::process::Child {
    let slave_in = unsafe { libc::dup(slave) };
    let slave_out = unsafe { libc::dup(slave) };
    unsafe { libc::close(slave) };
    Command::new(env!("CARGO_BIN_EXE_splash-monitor"))
        .args(["--interval", "1", "--fixtures", fixture, "--url", url])
        .env_remove("COLUMNS")
        .env_remove("LINES")
        .env_remove("TERM")
        .env_remove("NO_COLOR")
        .stdin(Stdio::from(unsafe { std::fs::File::from_raw_fd(slave_in) }))
        .stdout(Stdio::from(unsafe {
            std::fs::File::from_raw_fd(slave_out)
        }))
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn splash-monitor")
}

#[test]
fn interactive_run_enters_the_alternate_screen_and_restores_on_quit() {
    let dir = common::TmpDir::new("pty");
    let fixture = common::write_fixture(dir.path(), "sample.json", common::FIXTURE_SAMPLE);
    let server = common::TestServer::start(common::BASE_STATUS);
    let fixture_path = fixture.to_str().expect("fixture path").to_string();

    let (pty, slave) = Pty::open(35, 110);
    let mut child = spawn_on_pty(slave, &fixture_path, &server.url);

    assert!(
        pty.wait_for("SPLASH CACHE / I/O", Duration::from_secs(20)),
        "no full-layout frame drawn"
    );
    pty.send("q");
    let status = child.wait().expect("wait for child");
    let text = pty.finish();

    assert_eq!(status.code(), Some(0), "exit code for stream: {text}");
    assert!(text.contains("\x1b[?1049h"), "alternate screen not entered");
    assert!(
        text.contains("\x1b[?1049l"),
        "alternate screen not restored"
    );
    assert!(text.contains("\x1b[?25h"), "cursor not restored");
    let cursor_restored = text.find("\x1b[?25h").expect("cursor restore");
    let screen_restored = text.rfind("\x1b[?1049l").expect("screen restore");
    assert!(
        cursor_restored < screen_restored,
        "cursor must be restored before leaving the alternate screen"
    );
    assert_eq!(text.matches("q: quit").count(), 1, "frames drawn: {text}");
    assert!(text.contains("STANDBY"));
    assert!(text.contains("HARDWARE / THERMAL"));
    assert!(server.wait_for_requests(1), "the run fetched /status");
}

#[test]
fn stray_keys_keep_running_and_ctrl_c_exits_cleanly() {
    let dir = common::TmpDir::new("pty-signal");
    let fixture = common::write_fixture(dir.path(), "sample.json", common::FIXTURE_SAMPLE);
    let server = common::TestServer::start(common::BASE_STATUS);
    let fixture_path = fixture.to_str().expect("fixture path").to_string();

    let (pty, slave) = Pty::open(35, 110);
    let mut child = spawn_on_pty(slave, &fixture_path, &server.url);

    assert!(
        pty.wait_for("SPLASH CACHE / I/O", Duration::from_secs(20)),
        "no full-layout frame drawn"
    );
    pty.send("\u{1b}[2~"); // Insert: a key that must not end the run.
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        child.try_wait().expect("try wait").is_none(),
        "a stray key ended the run"
    );

    // Ctrl-C on a pty delivers SIGINT; that must restore the terminal too.
    let pid = child.id() as libc::pid_t;
    unsafe { libc::kill(pid, libc::SIGINT) };
    let status = child.wait().expect("wait after SIGINT");
    let text = pty.finish();

    assert!(status.success(), "SIGINT exit: {text}");
    assert!(
        text.contains("\x1b[?1049l"),
        "alternate screen not restored"
    );
    assert!(text.contains("\x1b[?25h"), "cursor not restored");
    assert!(text.matches("q: quit").count() >= 1);
}
