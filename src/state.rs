//! In-session history and rate derivation. Rules are a direct port of the
//! previous dashboard: a rate appears only when a real delta exists, a
//! decreasing counter yields `--` instead of a negative or invented value, and
//! a disconnected frame clears the inference baselines.

use std::collections::VecDeque;

use crate::status::SplashView;
use crate::system::SystemSample;

pub const HISTORY_CAP: usize = 120;
pub const SWAP_HISTORY_CAP: usize = 80;

const MIB: f64 = 1048576.0;

#[derive(Default)]
pub struct DashboardState {
    /// Rendered in-session history. Values are observed samples only.
    pub gpu_history: VecDeque<f64>,
    pub cpu_history: VecDeque<f64>,
    pub swap_history: VecDeque<f64>,
    pub swap_out_history: VecDeque<f64>,
    pub cache_read_history: VecDeque<f64>,
    pub cache_write_history: VecDeque<f64>,
    pub output_history: VecDeque<f64>,
    /// One history per logical core, drawn inside that core's table cell.
    pub core_history: Vec<VecDeque<f64>>,
    /// One history per CPU temperature sensor. A core cell draws the history of
    /// the sensor its proportional mapping selects.
    pub sensor_history: Vec<VecDeque<f64>>,

    prev_swap_in: Option<u64>,
    prev_swap_out: Option<u64>,
    prev_swap_at: Option<i64>,

    prev_cache_read: Option<u64>,
    prev_cache_write: Option<u64>,
    prev_cache_at: Option<i64>,

    prev_volume_read: Option<u64>,
    prev_volume_write: Option<u64>,
    prev_volume_at: Option<i64>,

    prev_metric_at: Option<i64>,
    prev_request_id: Option<String>,
    prev_generated: Option<u64>,
    prev_decode_total: Option<u64>,
}

#[derive(Debug, Default)]
pub struct FrameRates {
    pub swap_in_mib: Option<f64>,
    pub swap_out_mib: Option<f64>,
    pub cache_read_mib: Option<f64>,
    pub cache_write_mib: Option<f64>,
    pub volume_read_mib: Option<f64>,
    pub volume_write_mib: Option<f64>,
    pub output_tok: Option<f64>,
}

impl DashboardState {
    /// Swap page counters are lifetime totals; rates need two samples and a
    /// positive elapsed time.
    pub fn update_swap(&mut self, sample: &SystemSample, now: i64) -> (Option<f64>, Option<f64>) {
        let elapsed = self.prev_swap_at.map(|before| now - before).unwrap_or(0);
        let page_size = sample.page_size as f64;
        let rates = match (
            self.prev_swap_in,
            self.prev_swap_out,
            sample.swap_in_pages,
            sample.swap_out_pages,
        ) {
            (Some(prev_in), Some(prev_out), Some(current_in), Some(current_out))
                if elapsed > 0 && current_in >= prev_in && current_out >= prev_out =>
            {
                let seconds = elapsed as f64;
                (
                    Some((current_in - prev_in) as f64 * page_size / MIB / seconds),
                    Some((current_out - prev_out) as f64 * page_size / MIB / seconds),
                )
            }
            _ => (None, None),
        };
        self.prev_swap_in = sample.swap_in_pages;
        self.prev_swap_out = sample.swap_out_pages;
        self.prev_swap_at = Some(now);
        rates
    }

    /// Splash cache transfer rates straight from the `/status` disk counters.
    pub fn update_cache_io(&mut self, view: &SplashView, now: i64) -> (Option<f64>, Option<f64>) {
        if !view.online
            || !view.disk_present
            || view.disk_read_total.is_none()
            || view.disk_written_total.is_none()
        {
            self.prev_cache_read = None;
            self.prev_cache_write = None;
            self.prev_cache_at = None;
            return (None, None);
        }
        let read = view.disk_read_total.expect("checked");
        let written = view.disk_written_total.expect("checked");
        let elapsed = self.prev_cache_at.map(|before| now - before).unwrap_or(0);
        let rates = match (self.prev_cache_read, self.prev_cache_write) {
            (Some(prev_read), Some(prev_write))
                if elapsed > 0 && read >= prev_read && written >= prev_write =>
            {
                (
                    Some((read - prev_read) as f64 / MIB / elapsed as f64),
                    Some((written - prev_write) as f64 / MIB / elapsed as f64),
                )
            }
            _ => (None, None),
        };
        self.prev_cache_read = Some(read);
        self.prev_cache_write = Some(written);
        self.prev_cache_at = Some(now);
        rates
    }

    /// Device-wide counters for the volume behind `--cache-dir`.
    pub fn update_volume_io(
        &mut self,
        sample: &SystemSample,
        now: i64,
    ) -> (Option<f64>, Option<f64>) {
        let volume = match &sample.volume {
            Some(volume) => volume,
            None => {
                self.prev_volume_read = None;
                self.prev_volume_write = None;
                self.prev_volume_at = None;
                return (None, None);
            }
        };
        let (read, written) = match (volume.read_bytes, volume.write_bytes) {
            (Some(read), Some(written)) => (read, written),
            _ => {
                self.prev_volume_read = None;
                self.prev_volume_write = None;
                self.prev_volume_at = None;
                return (None, None);
            }
        };
        let elapsed = self.prev_volume_at.map(|before| now - before).unwrap_or(0);
        let rates = match (self.prev_volume_read, self.prev_volume_write) {
            (Some(prev_read), Some(prev_write))
                if elapsed > 0 && read >= prev_read && written >= prev_write =>
            {
                (
                    Some((read - prev_read) as f64 / MIB / elapsed as f64),
                    Some((written - prev_write) as f64 / MIB / elapsed as f64),
                )
            }
            _ => (None, None),
        };
        self.prev_volume_read = Some(read);
        self.prev_volume_write = Some(written);
        self.prev_volume_at = Some(now);
        rates
    }

    /// Instantaneous decode token rate. A cumulative server average is never
    /// presented as a live rate; that value is labelled "Engine avg" instead.
    pub fn update_inference(&mut self, view: &SplashView, now: i64) -> Option<f64> {
        let elapsed = self.prev_metric_at.map(|before| now - before).unwrap_or(0);
        let mut rate = None;
        if view.online && elapsed > 0 {
            if let (Some(total), Some(prev_total)) =
                (view.decode_tokens_total, self.prev_decode_total)
            {
                if total >= prev_total {
                    rate = Some((total - prev_total) as f64 / elapsed as f64);
                }
            } else if view.decode_tokens_total.is_none()
                && view.request_id != "--"
                && self.prev_request_id.as_deref() == Some(view.request_id.as_str())
            {
                if let Some(prev_generated) = self.prev_generated {
                    if view.generated_tokens >= prev_generated {
                        rate =
                            Some((view.generated_tokens - prev_generated) as f64 / elapsed as f64);
                    }
                }
            } else if view.active == 0 {
                rate = Some(0.0);
            }
        }

        if view.online {
            self.prev_metric_at = Some(now);
            self.prev_request_id = Some(view.request_id.clone());
            self.prev_generated = Some(view.generated_tokens);
            self.prev_decode_total = view.decode_tokens_total;
        } else {
            self.prev_metric_at = None;
            self.prev_request_id = None;
            self.prev_generated = None;
            self.prev_decode_total = None;
        }
        rate
    }

    /// One pass over a frame: compute every rate, then append only the values
    /// that were actually observed.
    pub fn update_frame(
        &mut self,
        view: &SplashView,
        sample: &SystemSample,
        now: i64,
    ) -> FrameRates {
        let (swap_in, swap_out) = self.update_swap(sample, now);
        let (cache_read, cache_write) = self.update_cache_io(view, now);
        let (volume_read, volume_write) = self.update_volume_io(sample, now);
        let output = self.update_inference(view, now);

        if let Some(percent) = sample.gpu_active_ratio {
            push(&mut self.gpu_history, percent * 100.0, HISTORY_CAP);
        }
        if let Some(percent) = sample.cpu_active_ratio {
            push(&mut self.cpu_history, percent * 100.0, HISTORY_CAP);
        }
        if let Some(bytes) = sample.swap_used_bytes {
            push(
                &mut self.swap_history,
                bytes as f64 / 1073741824.0,
                SWAP_HISTORY_CAP,
            );
        }
        if let Some(rate) = swap_out {
            push(&mut self.swap_out_history, rate, HISTORY_CAP);
        }
        if let Some(rate) = cache_read {
            push(&mut self.cache_read_history, rate, HISTORY_CAP);
        }
        if let Some(rate) = cache_write {
            push(&mut self.cache_write_history, rate, HISTORY_CAP);
        }
        if let Some(rate) = output {
            push(&mut self.output_history, rate, HISTORY_CAP);
        }
        if self.core_history.len() != sample.cpu_cores.len() {
            self.core_history
                .resize_with(sample.cpu_cores.len(), VecDeque::new);
        }
        for (index, ratio) in sample.cpu_cores.iter().enumerate() {
            if let Some(ratio) = ratio {
                push(&mut self.core_history[index], ratio * 100.0, HISTORY_CAP);
            }
        }
        if self.sensor_history.len() != sample.cpu_sensors.len() {
            self.sensor_history
                .resize_with(sample.cpu_sensors.len(), VecDeque::new);
        }
        for (index, (_, celsius)) in sample.cpu_sensors.iter().enumerate() {
            push(&mut self.sensor_history[index], *celsius, HISTORY_CAP);
        }

        FrameRates {
            swap_in_mib: swap_in,
            swap_out_mib: swap_out,
            cache_read_mib: cache_read,
            cache_write_mib: cache_write,
            volume_read_mib: volume_read,
            volume_write_mib: volume_write,
            output_tok: output,
        }
    }
}

fn push(history: &mut VecDeque<f64>, value: f64, cap: usize) {
    history.push_back(value);
    while history.len() > cap {
        history.pop_front();
    }
}
