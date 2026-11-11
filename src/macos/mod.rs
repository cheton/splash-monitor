//! The macOS probe: Mach CPU ticks, IOHID/AppleSMC temperatures, IOReport GPU
//! counters, `host_statistics64` memory and `IOBlockStorageDriver` device I/O.

pub mod cpu;
pub mod disk;
pub mod ffi;
pub mod gpu;
pub mod memory;
pub mod thermal;

use std::path::Path;
use std::time::{Duration, Instant};

use crate::system::{SystemProbe, SystemSample, VolumeSample};

/// Counter rates need a measurable window even on the very first frame.
const MIN_SAMPLE_WINDOW: Duration = Duration::from_millis(200);

pub struct MacProbe {
    host: Option<ffi::MachHost>,
    page_size: u64,
    sensors: bool,
    gpu: Option<gpu::GpuProbe>,
    cpu_previous: Option<cpu::CpuSnapshot>,
    counters_at: Instant,
    cache_dir_display: Option<String>,
    disk_target: Option<disk::DiskTarget>,
}

impl MacProbe {
    /// `cache_dir` selects the volume whose device counters are sampled; it is
    /// never presented as a Splash-only measurement.
    pub fn new(sensors: bool, cache_dir: Option<&Path>) -> Self {
        let host = ffi::MachHost::new();
        let page_size = match &host {
            Some(host) => memory::page_size(host.get()),
            None => memory::page_size(0),
        };
        let gpu = if sensors { gpu::GpuProbe::new() } else { None };
        let disk_target = cache_dir.and_then(disk::resolve);
        // Seed the tick baseline at startup so the first frame already covers a
        // window; the same rule as the legacy native helper.
        let host_id = host.as_ref().map(|host| host.get()).unwrap_or(0);
        let cpu_previous = cpu::snapshot(host_id);
        MacProbe {
            host,
            page_size,
            sensors,
            gpu,
            cpu_previous,
            counters_at: Instant::now(),
            cache_dir_display: cache_dir.map(|path| path.display().to_string()),
            disk_target,
        }
    }

    /// Every reading in this frame comes from a system-wide source, so the
    /// frame is described that way rather than attributed to Splash.
    pub fn read_frame(&mut self) -> SystemSample {
        let host = self.host.as_ref().map(|host| host.get()).unwrap_or(0);

        // CPU ticks and GPU counters are differences: keep a measurable window.
        if self.counters_at.elapsed() < MIN_SAMPLE_WINDOW {
            std::thread::sleep(MIN_SAMPLE_WINDOW - self.counters_at.elapsed());
        }
        self.counters_at = Instant::now();

        let mut cpu_active_ratio = None;
        let mut cpu_cores: Vec<Option<f64>> = Vec::new();
        if let Some(current) = cpu::snapshot(host) {
            let (aggregate, per_core) = match &self.cpu_previous {
                Some(previous) => cpu::delta(previous, &current),
                None => (None, vec![None; current.cores.len()]),
            };
            cpu_active_ratio = aggregate;
            cpu_cores = per_core;
            self.cpu_previous = Some(current);
        }

        let thermal = if self.sensors {
            let mut no_log: Option<Vec<String>> = None;
            thermal::read(&mut no_log)
        } else {
            thermal::ThermalSample::default()
        };

        let (gpu_active_ratio, gpu_power_w) = match &mut self.gpu {
            Some(probe) => probe.sample(),
            None => (None, None),
        };

        let memory_snapshot = memory::snapshot(host);
        let swap = memory::swap_usage();
        let pressure_level = memory::pressure_level();
        let free_percent = memory_snapshot.as_ref().and_then(memory::free_percent);

        let mut volume = None;
        let mut device_unresolved = None;
        if let Some(target) = &self.disk_target {
            match disk::counters(target) {
                Some((device, read_bytes, written_bytes)) => {
                    volume = Some(VolumeSample {
                        device,
                        read_bytes: Some(read_bytes),
                        write_bytes: Some(written_bytes),
                    });
                }
                None => {
                    device_unresolved = self.cache_dir_display.clone();
                }
            }
        }

        let telemetry_unavailable = self.sensors
            && cpu_active_ratio.is_none()
            && thermal.cpu_package.is_none()
            && thermal.cpu_sensors.is_empty()
            && gpu_active_ratio.is_none()
            && thermal.gpu.is_none();

        SystemSample {
            cpu_active_ratio,
            cpu_cores,
            cpu_sensors: thermal.cpu_sensors,
            cpu_package: thermal.cpu_package,
            load_avg: memory::load_average(),
            gpu_active_ratio,
            gpu_temp: thermal.gpu,
            gpu_power_w,
            swap_used_bytes: swap.map(|usage| usage.used_bytes),
            swap_in_pages: memory_snapshot.map(|snapshot| snapshot.swapins),
            swap_out_pages: memory_snapshot.map(|snapshot| snapshot.swapouts),
            page_size: self.page_size,
            pressure_level,
            free_percent,
            volume,
            device_unresolved,
            telemetry_unavailable,
        }
    }

    /// Diagnostics for `--debug-sensors`: raw sensor values, IOReport channel
    /// availability, the computed free percentage and the resolved device.
    pub fn debug_lines(&mut self) -> Vec<String> {
        let mut lines = Vec::new();

        let mut sensor_log: Option<Vec<String>> = Some(Vec::new());
        let thermal = thermal::read(&mut sensor_log);
        if let Some(collected) = sensor_log {
            lines.extend(collected);
        }
        lines.push(format!(
            "cpu_package = {}",
            thermal
                .cpu_package
                .map_or_else(|| "unavailable".to_string(), |value| format!("{value:.2}"))
        ));
        lines.push(format!(
            "gpu_temp = {}",
            thermal
                .gpu
                .map_or_else(|| "unavailable".to_string(), |value| format!("{value:.2}"))
        ));

        match &self.gpu {
            Some(probe) => lines.append(&mut probe.debug_lines()),
            None => lines.push("ioreport subscription unavailable".to_string()),
        }
        let (gpu_active_ratio, gpu_power_w) = match &mut self.gpu {
            Some(probe) => probe.sample(),
            None => (None, None),
        };
        lines.push(format!(
            "gpu_active_ratio = {}",
            gpu_active_ratio.map_or_else(
                || "unavailable".to_string(),
                |value| format!("{:.3}", value * 100.0)
            )
        ));
        lines.push(format!(
            "gpu_power = {}",
            gpu_power_w.map_or_else(|| "unavailable".to_string(), |value| format!("{value:.2}"))
        ));

        if let Some(snapshot) =
            memory::snapshot(self.host.as_ref().map(|host| host.get()).unwrap_or(0))
        {
            lines.push(format!(
                "free_percent = {}",
                memory::free_percent(&snapshot)
                    .map_or_else(|| "unavailable".to_string(), |value| value.to_string())
            ));
        } else {
            lines.push("free_percent = unavailable".to_string());
        }

        if let Some(target) = &self.disk_target {
            match disk::counters(target) {
                Some((device, read_bytes, written_bytes)) => lines.push(format!(
                    "device = {device} read={read_bytes} write={written_bytes}"
                )),
                None => lines.push(format!(
                    "device unresolved for {}",
                    self.cache_dir_display.clone().unwrap_or_default()
                )),
            }
        } else if let Some(path) = &self.cache_dir_display {
            lines.push(format!("device unresolved for {path}"));
        }
        lines
    }
}

impl SystemProbe for MacProbe {
    fn sample(&mut self) -> SystemSample {
        self.read_frame()
    }
}
