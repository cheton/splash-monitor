//! One sampled frame of system telemetry, plus the probe seam used by the
//! real macOS probes, the `--fixtures` fixture probe and the test suite.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct VolumeSample {
    pub device: String,
    pub read_bytes: Option<u64>,
    pub write_bytes: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct SystemSample {
    pub cpu_active_ratio: Option<f64>,
    pub cpu_cores: Vec<Option<f64>>,
    pub cpu_sensors: Vec<(String, f64)>,
    pub cpu_package: Option<f64>,
    /// 1, 5 and 15 minute load averages.
    pub load_avg: Option<[f64; 3]>,
    pub gpu_active_ratio: Option<f64>,
    pub gpu_temp: Option<f64>,
    pub gpu_power_w: Option<f64>,
    pub swap_used_bytes: Option<u64>,
    /// Lifetime page counters, not rates: the dashboard derives rates from
    /// two samples.
    pub swap_in_pages: Option<u64>,
    pub swap_out_pages: Option<u64>,
    pub page_size: u64,
    pub pressure_level: Option<i32>,
    pub free_percent: Option<u32>,
    pub volume: Option<VolumeSample>,
    /// Set when a `--cache-dir` probe was requested but no counters resolved.
    pub device_unresolved: Option<String>,
    /// Set when the probe ran and produced no usable hardware reading.
    pub telemetry_unavailable: bool,
}

pub trait SystemProbe {
    fn sample(&mut self) -> SystemSample;
}

/// Reads a `SystemSample` from JSON. Every field is optional so a fixture can
/// describe a partial machine: absent values stay `None` instead of zero.
pub struct FixtureProbe {
    path: PathBuf,
}

impl FixtureProbe {
    pub fn new(path: impl AsRef<Path>) -> Self {
        FixtureProbe {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl SystemProbe for FixtureProbe {
    fn sample(&mut self) -> SystemSample {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        let value: serde_json::Value =
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);

        let ratio = |key: &str| value.get(key).and_then(serde_json::Value::as_f64);
        let integer = |key: &str| value.get(key).and_then(serde_json::Value::as_u64);

        let mut cpu_cores = Vec::new();
        if let Some(serde_json::Value::Array(items)) = value.get("cpu_cores") {
            for item in items {
                cpu_cores.push(item.as_f64());
            }
        }
        let mut cpu_sensors = Vec::new();
        if let Some(serde_json::Value::Array(items)) = value.get("cpu_sensors") {
            for item in items {
                let label = item
                    .get("label")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let celsius = item.get("celsius").and_then(serde_json::Value::as_f64);
                if let Some(celsius) = celsius {
                    cpu_sensors.push((label.to_string(), celsius));
                }
            }
        }
        let load_avg = value
            .get("load_avg")
            .and_then(serde_json::Value::as_array)
            .and_then(|items| {
                let values: Option<Vec<f64>> =
                    items.iter().map(serde_json::Value::as_f64).collect();
                values
                    .filter(|values| values.len() == 3)
                    .map(|values| [values[0], values[1], values[2]])
            });
        let volume = value.get("volume").map(|volume| VolumeSample {
            device: volume
                .get("device")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            read_bytes: volume.get("read_bytes").and_then(serde_json::Value::as_u64),
            write_bytes: volume
                .get("write_bytes")
                .and_then(serde_json::Value::as_u64),
        });

        let mut sample = SystemSample {
            cpu_active_ratio: ratio("cpu_active_ratio"),
            cpu_cores,
            cpu_sensors,
            cpu_package: ratio("cpu_package"),
            load_avg,
            gpu_active_ratio: ratio("gpu_active_ratio"),
            gpu_temp: ratio("gpu_temp"),
            gpu_power_w: ratio("gpu_power_w"),
            swap_used_bytes: integer("swap_used_bytes"),
            swap_in_pages: integer("swap_in_pages"),
            swap_out_pages: integer("swap_out_pages"),
            page_size: integer("page_size").unwrap_or(16384),
            pressure_level: value
                .get("pressure_level")
                .and_then(serde_json::Value::as_i64)
                .map(|value| value as i32),
            free_percent: integer("free_percent").map(|value| value as u32),
            volume,
            device_unresolved: None,
            telemetry_unavailable: false,
        };
        sample.telemetry_unavailable = sample.cpu_active_ratio.is_none()
            && sample.cpu_cores.iter().all(Option::is_none)
            && sample.cpu_sensors.is_empty()
            && sample.cpu_package.is_none()
            && sample.gpu_active_ratio.is_none()
            && sample.gpu_temp.is_none()
            && sample.gpu_power_w.is_none()
            && sample.swap_used_bytes.is_none()
            && sample.free_percent.is_none();
        sample
    }
}
