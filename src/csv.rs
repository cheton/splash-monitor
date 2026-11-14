//! CSV logging. The schema is unchanged from the previous dashboard so old
//! extracts stay comparable: 13 columns, header written only once.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::render::Frame;

pub const HEADER: &str = "timestamp,metal_current_GiB,metal_peak_GiB,metal_limit_GiB,pressure,free_percent,\
swap_used_GiB,swap_in_MiB_s,swap_out_MiB_s,active_requests,kv_spilled_pages,kv_spilled_GiB,weights_released";

/// Local wall-clock timestamp, `%Y-%m-%d %H:%M:%S`, formatted with libc only.
pub fn timestamp_local(now: i64) -> String {
    unsafe {
        let when = libc::time_t::from(now);
        let mut buffer: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&when, &mut buffer).is_null() {
            return "--".to_string();
        }
        let mut text = [0u8; 32];
        let written = libc::strftime(
            text.as_mut_ptr() as *mut libc::c_char,
            text.len(),
            c"%Y-%m-%d %H:%M:%S".as_ptr(),
            &buffer,
        );
        if written == 0 {
            return "--".to_string();
        }
        let bytes = &text[..written as usize];
        String::from_utf8_lossy(bytes).into_owned()
    }
}

pub struct CsvLog {
    path: std::path::PathBuf,
}

impl CsvLog {
    /// Creates the parent directory and the header when the file is missing
    /// or empty. A failure to write is an error: logging was requested.
    pub fn open(path: &Path) -> Result<Self, std::io::Error> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let fresh = !path.exists()
            || std::fs::metadata(path)
                .map(|meta| meta.len() == 0)
                .unwrap_or(false);
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        if fresh {
            writeln!(file, "{HEADER}")?;
        }
        file.flush()?;
        Ok(CsvLog {
            path: path.to_path_buf(),
        })
    }

    /// One row per rendered frame. Unavailable API values stay `--` instead of
    /// being written as zeros.
    pub fn write_frame(&self, frame: &Frame<'_>, now: i64) -> std::io::Result<()> {
        let active = if frame.view.online {
            frame.view.active.to_string()
        } else {
            "--".to_string()
        };
        let fields = [
            timestamp_local(now),
            frame.display.metal_gib.clone(),
            frame.display.peak_gib.clone(),
            frame.display.limit_gib.clone(),
            frame.pressure_text().to_string(),
            frame.free_percent_text(),
            frame.swap_used_gib(),
            Frame::rate_text(frame.rates.swap_in_mib),
            Frame::rate_text(frame.rates.swap_out_mib),
            active,
            frame.display.spilled_pages.clone(),
            frame.display.spill_gib.clone(),
            frame.view.released.clone(),
        ];
        let mut file: File = OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{}", fields.join(","))?;
        Ok(())
    }
}
