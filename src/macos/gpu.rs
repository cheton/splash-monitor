//! GPU utilization and power from IOReport: the `GPU Stats` /
//! `GPU Performance States` residency channel and the `Energy Model`
//! `GPU Energy` channel. One subscription lives for the whole run.
//!
//! Channel selection and the busy-state rule follow btop++
//! `src/osx/btop_collect.cpp` (Apache-2.0, Copyright 2021 Aristocratos).
//! Derived from that code and modified for this project.

use std::ffi::c_void;
use std::time::Instant;

use crate::macos::ffi;

const IDLE_STATES: [&str; 3] = ["IDLE", "OFF", "DOWN"];

pub struct GpuProbe {
    subscription: *mut c_void,
    channels: ffi::CfRef,
    previous: Option<ffi::CfRef>,
    last_wall: Option<Instant>,
    channel_count: usize,
    saw_performance_states: bool,
    saw_energy: bool,
}

impl GpuProbe {
    /// Builds the long-lived subscription. `None` means IOReport is
    /// unavailable and both readings stay `None` (never zero).
    pub fn new() -> Option<Self> {
        let group = ffi::cf_string("GPU Stats")?;
        let sub_group = ffi::cf_string("GPU Performance States")?;
        let energy_group = ffi::cf_string("Energy Model")?;

        let stats = ffi::CfRef::new(unsafe {
            ffi::IOReportCopyChannelsInGroup(group.get(), sub_group.get(), 0, 0, 0)
        });
        let energy = ffi::CfRef::new(unsafe {
            ffi::IOReportCopyChannelsInGroup(energy_group.get(), std::ptr::null(), 0, 0, 0)
        });
        if stats.is_none() && energy.is_none() {
            return None;
        }
        if let (Some(stats), Some(energy)) = (&stats, &energy) {
            unsafe { ffi::IOReportMergeChannels(stats.get(), energy.get(), std::ptr::null()) };
        }
        let base = if let Some(stats) = &stats {
            stats.get()
        } else {
            energy.as_ref().expect("energy").get()
        };
        let channels = ffi::CfRef::new(unsafe {
            ffi::CFDictionaryCreateMutableCopy(
                std::ptr::null(),
                ffi::CFDictionaryGetCount(base),
                base,
            )
        })?;
        let channel_count = unsafe { ffi::CFDictionaryGetCount(channels.get()) } as usize;

        let mut dummy: *mut c_void = std::ptr::null_mut();
        let subscription = unsafe {
            ffi::IOReportCreateSubscription(
                std::ptr::null_mut(),
                channels.get() as *mut c_void,
                &mut dummy,
                0,
                std::ptr::null(),
            )
        };
        if !dummy.is_null() {
            unsafe { ffi::CFRelease(dummy) };
        }
        if subscription.is_null() {
            return None;
        }

        let mut probe = GpuProbe {
            subscription,
            channels,
            previous: None,
            last_wall: None,
            channel_count,
            saw_performance_states: false,
            saw_energy: false,
        };
        // Seed the counter baseline so the first sampled frame already covers a
        // window instead of reporting `--`.
        probe.previous = ffi::CfRef::new(unsafe {
            ffi::IOReportCreateSamples(
                subscription,
                probe.channels.get() as *mut c_void,
                std::ptr::null(),
            )
        });
        probe.last_wall = Some(Instant::now());
        Some(probe)
    }

    /// `(utilization ratio, watts)` for the window since the previous call.
    /// A failed or empty sample returns `None` rather than inventing a rate.
    pub fn sample(&mut self) -> (Option<f64>, Option<f64>) {
        let current = match ffi::CfRef::new(unsafe {
            ffi::IOReportCreateSamples(
                self.subscription,
                self.channels.get() as *mut c_void,
                std::ptr::null(),
            )
        }) {
            Some(value) => value,
            None => return (None, None),
        };
        let now = Instant::now();
        let elapsed = self
            .last_wall
            .map(|before| now.duration_since(before).as_secs_f64());
        self.last_wall = Some(now);

        let previous = match &self.previous {
            Some(value) => value,
            None => {
                self.previous = Some(current);
                return (None, None);
            }
        };
        let delta = match ffi::CfRef::new(unsafe {
            ffi::IOReportCreateSamplesDelta(previous.get(), current.get(), std::ptr::null())
        }) {
            Some(value) => value,
            None => {
                self.previous = Some(current);
                return (None, None);
            }
        };

        let rows_ref = ffi::dict_get(delta.get(), "IOReportChannels");
        let mut busy: i64 = 0;
        let mut total: i64 = 0;
        let mut energy_raw: Option<i64> = None;
        let mut energy_unit: Option<String> = None;

        if let Some(rows) = rows_ref {
            for index in 0..ffi::array_len(rows) {
                let Some(row) = ffi::array_at(rows, index) else {
                    continue;
                };
                let group = ffi::cf_to_string(unsafe { ffi::IOReportChannelGetGroup(row) })
                    .unwrap_or_default();
                if group == "GPU Stats" {
                    let sub = ffi::cf_to_string(unsafe { ffi::IOReportChannelGetSubGroup(row) })
                        .unwrap_or_default();
                    let channel =
                        ffi::cf_to_string(unsafe { ffi::IOReportChannelGetChannelName(row) })
                            .unwrap_or_default();
                    if sub == "GPU Performance States" && channel == "GPUPH" {
                        self.saw_performance_states = true;
                        let states = unsafe { ffi::IOReportStateGetCount(row) };
                        for state in 0..states {
                            let residency =
                                unsafe { ffi::IOReportStateGetResidency(row, state) }.max(0);
                            let name = ffi::cf_to_string(unsafe {
                                ffi::IOReportStateGetNameForIndex(row, state)
                            })
                            .unwrap_or_default();
                            total += residency;
                            if !IDLE_STATES.contains(&name.as_str()) {
                                busy += residency;
                            }
                        }
                    }
                } else if group == "Energy Model" {
                    let channel =
                        ffi::cf_to_string(unsafe { ffi::IOReportChannelGetChannelName(row) })
                            .unwrap_or_default();
                    if channel == "GPU Energy" {
                        self.saw_energy = true;
                        energy_raw = Some(unsafe { ffi::IOReportSimpleGetIntegerValue(row, 0) });
                        energy_unit =
                            ffi::cf_to_string(unsafe { ffi::IOReportChannelGetUnitLabel(row) });
                    }
                }
            }
        }

        self.previous = Some(current);

        let utilization = if total > 0 {
            Some(((busy as f64) / (total as f64)).clamp(0.0, 1.0))
        } else {
            None
        };
        let watts = match (energy_raw, energy_unit, elapsed) {
            (Some(raw), Some(unit), Some(seconds)) if seconds > 0.0 => match unit_divisor(&unit) {
                Some(divisor) => {
                    let watts = (raw as f64) / divisor / seconds;
                    if (0.0..300.0).contains(&watts) {
                        Some(watts)
                    } else {
                        None
                    }
                }
                None => None,
            },
            _ => None,
        };
        (utilization, watts)
    }

    /// Diagnostics for `--debug-sensors`.
    pub fn debug_lines(&self) -> Vec<String> {
        vec![
            format!(
                "ioreport subscription active ({} channels)",
                self.channel_count
            ),
            format!(
                "GPU Stats / GPU Performance States / GPUPH: {}",
                if self.saw_performance_states {
                    "available"
                } else {
                    "not seen"
                }
            ),
            format!(
                "Energy Model / GPU Energy: {}",
                if self.saw_energy {
                    "available"
                } else {
                    "not seen"
                }
            ),
        ]
    }
}

fn unit_divisor(unit: &str) -> Option<f64> {
    if unit.contains("nJ") {
        Some(1e9)
    } else if unit.contains("uJ") || unit.contains("\u{b5}J") {
        Some(1e6)
    } else if unit.contains("mJ") {
        Some(1e3)
    } else {
        None
    }
}

impl Drop for GpuProbe {
    fn drop(&mut self) {
        if !self.subscription.is_null() {
            // IOReport subscription objects are CoreFoundation objects.
            unsafe { ffi::CFRelease(self.subscription as *const c_void) };
        }
    }
}
