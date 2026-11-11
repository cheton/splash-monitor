//! Splash Monitor library: `/status` model, macOS telemetry probes, state
//! history and the terminal renderer. The `splash-monitor` binary is a thin
//! driver over this API, which is also what the integration tests drive.

pub mod cli;
pub mod csv;
pub mod http;
pub mod macos;
pub mod render;
pub mod state;
pub mod status;
pub mod system;
pub mod term;
pub mod theme;
