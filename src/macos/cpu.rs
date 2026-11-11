//! Per-core and aggregate CPU utilization from Mach processor ticks.
//! Logic and tick handling follow btop++ `src/osx/btop_collect.cpp`
//! (Apache-2.0, Copyright 2021 Aristocratos). Derived from that code and
//! modified for this project.

use crate::macos::ffi;

#[derive(Clone, Default)]
pub struct CpuSnapshot {
    /// One `(busy, total)` tick pair per logical core, in core order.
    pub cores: Vec<(u64, u64)>,
}

pub fn snapshot(host: u32) -> Option<CpuSnapshot> {
    let array = ffi::ProcessorInfoArray::read(host, ffi::PROCESSOR_CPU_LOAD_INFO)?;
    let base = array.ptr as *const ffi::ProcessorCpuLoadInfo;
    let mut cores = Vec::with_capacity(array.count as usize);
    for index in 0..(array.count as usize) {
        let info = unsafe { &*base.add(index) };
        let user = u64::from(info.cpu_ticks[ffi::CPU_STATE_USER]);
        let system = u64::from(info.cpu_ticks[ffi::CPU_STATE_SYSTEM]);
        let idle = u64::from(info.cpu_ticks[ffi::CPU_STATE_IDLE]);
        let nice = u64::from(info.cpu_ticks[ffi::CPU_STATE_NICE]);
        let busy = user + system + nice;
        cores.push((busy, busy + idle));
    }
    Some(CpuSnapshot { cores })
}

/// Aggregate ratio plus per-core ratios between two snapshots. Cores whose
/// total ticks did not increase yield `None` instead of a guessed value.
pub fn delta(prev: &CpuSnapshot, cur: &CpuSnapshot) -> (Option<f64>, Vec<Option<f64>>) {
    let mut per_core = Vec::with_capacity(cur.cores.len());
    let mut busy_delta = 0u64;
    let mut total_delta = 0u64;

    for (index, (cur_busy, cur_total)) in cur.cores.iter().enumerate() {
        let ratio = match prev.cores.get(index) {
            Some((prev_busy, prev_total)) if cur_total > prev_total => {
                let busy = cur_busy.saturating_sub(*prev_busy);
                let total = cur_total - prev_total;
                busy_delta += busy;
                total_delta += total;
                clamp_ratio(busy as f64 / total as f64)
            }
            _ => None,
        };
        per_core.push(ratio);
    }

    let aggregate = if total_delta > 0 {
        clamp_ratio(busy_delta as f64 / total_delta as f64)
    } else {
        None
    };
    (aggregate, per_core)
}

fn clamp_ratio(value: f64) -> Option<f64> {
    if value.is_finite() {
        Some(value.clamp(0.0, 1.0))
    } else {
        None
    }
}
