//! macOS memory, swap and pressure readings: `host_statistics64`,
//! `sysctl vm.swapusage` and `kern.memorystatus_vm_pressure_level`.

use crate::macos::ffi;

#[derive(Clone, Copy, Default)]
pub struct MemorySnapshot {
    pub swapins: u64,
    pub swapouts: u64,
    pub free_pages: u64,
    pub active_pages: u64,
    pub inactive_pages: u64,
    /// Reference only: `speculative_count` pages are already counted inside
    /// `free_count` (see `mach/vm_statistics.h`), so they are never added again.
    pub speculative_pages: u64,
    pub wire_pages: u64,
    pub compressor_pages: u64,
    pub external_pages: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct SwapUsage {
    pub total_bytes: u64,
    pub used_bytes: u64,
}

/// Reads `HOST_VM_INFO64`. A kernel older than the declared struct is served
/// one of the documented prefixes instead of failing outright.
pub fn snapshot(host: u32) -> Option<MemorySnapshot> {
    let mut stats = ffi::VmStatistics64::default();
    let counts = [
        ffi::VM_STAT_COUNT_LATEST,
        ffi::VM_STAT_COUNT_REV5,
        ffi::VM_STAT_COUNT_REV4,
        ffi::VM_STAT_COUNT_REV3,
    ];
    let mut used = 0u32;
    for count in counts {
        let mut count_in_out = count;
        let result = unsafe {
            ffi::host_statistics64(
                host,
                ffi::HOST_VM_INFO64,
                &mut stats as *mut ffi::VmStatistics64,
                &mut count_in_out,
            )
        };
        if result == ffi::KERN_SUCCESS {
            used = count;
            break;
        }
    }
    if used == 0 {
        return None;
    }
    Some(MemorySnapshot {
        swapins: stats.swapins,
        swapouts: stats.swapouts,
        free_pages: u64::from(stats.free_count),
        active_pages: u64::from(stats.active_count),
        inactive_pages: u64::from(stats.inactive_count),
        speculative_pages: u64::from(stats.speculative_count),
        wire_pages: u64::from(stats.wire_count),
        compressor_pages: u64::from(stats.compressor_page_count),
        external_pages: u64::from(stats.external_page_count),
    })
}

pub fn page_size(host: u32) -> u64 {
    let mut size: u64 = 0;
    let result = unsafe { ffi::host_page_size(host, &mut size) };
    if result == ffi::KERN_SUCCESS && size > 0 {
        return size;
    }
    ffi::sysctl_scalar::<u64>("hw.pagesize").unwrap_or(16384)
}

pub fn swap_usage() -> Option<SwapUsage> {
    let usage = ffi::sysctl_swap_usage()?;
    Some(SwapUsage {
        total_bytes: usage.xsu_total,
        used_bytes: usage.xsu_used,
    })
}

/// `1` NORMAL, `2` WARNING, `4` CRITICAL; anything else is unknown.
pub fn pressure_level() -> Option<i32> {
    ffi::sysctl_scalar::<i32>("kern.memorystatus_vm_pressure_level")
}

/// 1, 5 and 15 minute load averages from `getloadavg`.
pub fn load_average() -> Option<[f64; 3]> {
    let mut values = [0f64; 3];
    if unsafe { libc::getloadavg(values.as_mut_ptr(), 3) } == 3 {
        Some(values)
    } else {
        None
    }
}

/// Free percentage of physical memory, matching what `memory_pressure -Q`
/// reports: usable pages over usable plus compressor and wired pages.
pub fn free_percent(snapshot: &MemorySnapshot) -> Option<u32> {
    let available = snapshot.free_pages + snapshot.inactive_pages + snapshot.external_pages;
    let total = available + snapshot.compressor_pages + snapshot.wire_pages;
    if total == 0 {
        return None;
    }
    Some(((available as f64 / total as f64) * 100.0).round() as u32)
}
