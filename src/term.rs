//! Terminal lifecycle: raw mode, alternate screen, cursor visibility, panic
//! recovery and shutdown on Ctrl-C or a signal.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal;

pub static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
pub static IN_ALTERNATE_SCREEN: AtomicBool = AtomicBool::new(false);

extern "C" fn signal_handler(_signal: i32) {
    // Only a flag is touched here: the render loop performs the real restore.
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn install_signal_handlers() {
    unsafe {
        libc::signal(
            libc::SIGINT,
            signal_handler as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            signal_handler as *const () as libc::sighandler_t,
        );
    }
}

/// Writes the restore sequence the previous dashboard used on exit.
/// Emits the restore once: the flag is taken here, so a panic-hook restore
/// followed by the normal drop cannot send the sequence twice.
pub fn restore_terminal() {
    let mut out = std::io::stdout();
    let sequence = if IN_ALTERNATE_SCREEN.swap(false, Ordering::SeqCst) {
        "\x1b[0m\x1b[?25h\x1b[?1049l"
    } else {
        "\x1b[0m\x1b[?25h"
    };
    let _ = out.write_all(sequence.as_bytes());
    let _ = out.flush();
    let _ = terminal::disable_raw_mode();
}

pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = terminal::disable_raw_mode();
        restore_terminal();
        previous(info);
    }));
}

/// Enters raw mode plus the alternate screen. Drop (or `restore`) puts the
/// terminal back.
pub struct InteractiveTerminal;

impl InteractiveTerminal {
    pub fn enter() -> Self {
        let _ = terminal::enable_raw_mode();
        let mut out = std::io::stdout();
        let entered_alt = out
            .write_all(b"\x1b[?1049h\x1b[?25l")
            .and_then(|_| out.flush())
            .is_ok();
        IN_ALTERNATE_SCREEN.store(entered_alt, Ordering::SeqCst);
        InteractiveTerminal
    }

    pub fn restore(&self) {
        restore_terminal();
    }
}

impl Drop for InteractiveTerminal {
    fn drop(&mut self) {
        self.restore();
    }
}

/// Waits up to `interval` for the next key. `q`, `Q` and Ctrl-C stop the loop.
pub fn wait_for_quit(interval: std::time::Duration) -> bool {
    if !event::poll(interval).unwrap_or(false) {
        return false;
    }
    match event::read() {
        Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
            use crossterm::event::KeyCode;
            let ctrl_c = key.code == KeyCode::Char('c')
                && key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL);
            matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q')) || ctrl_c
        }
        _ => false,
    }
}
