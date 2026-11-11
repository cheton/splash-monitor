//! Device-wide read/write counters for the volume that hosts `--cache-dir`.
//! Counters come from `IOBlockStorageDriver` statistics, so they cover every
//! process on the machine and must always be labelled that way.
//!
//! Whole-disk selection mirrors the previous dashboard and btop++
//! `src/osx/btop_collect.cpp` (Apache-2.0, Copyright 2021 Aristocratos).
//! Derived from that code and modified for this project.

use std::ffi::{c_char, c_void, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::macos::ffi;

/// BSD name candidates to try, best match first: whole disk, then the mount's
/// own name.
pub struct DiskTarget {
    pub candidates: Vec<String>,
}

/// Walk up to the nearest existing directory; `/` is the final fallback.
/// `None` when no ancestor can be inspected at all.
fn nearest_existing_dir(path: &Path) -> Option<PathBuf> {
    let mut current: PathBuf = path.to_path_buf();
    loop {
        let c_path = to_c_path(&current)?;
        let mut buf: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(c_path.as_ptr(), &mut buf) } == 0 {
            return Some(current);
        }
        if current == Path::new("/") {
            return None;
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => current = PathBuf::from("/"),
        }
    }
}

fn to_c_path(path: &Path) -> Option<CString> {
    CString::new(path.as_os_str().as_bytes()).ok()
}

/// `disk3s1s1` -> `disk3`: keep `disk` plus the following digits.
fn whole_disk_name(name: &str) -> Option<String> {
    if !name.starts_with("disk") {
        return None;
    }
    let digits: String = name
        .chars()
        .skip(4)
        .take_while(char::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        return None;
    }
    Some(format!("disk{digits}"))
}

/// Device name of the mount that holds `path`: the longest mount point that
/// equals an ancestor of the path. Firmlink mounts such as `/var` carry no
/// device name, so the search continues with their parent mount, which is what
/// `df` reports for the same path.
fn mount_source(path: &Path) -> Option<String> {
    let mut mounts: *mut libc::statfs = std::ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut mounts, libc::MNT_NOWAIT) };
    if count <= 0 {
        return None;
    }
    let table = unsafe { std::slice::from_raw_parts(mounts, count as usize) };

    let mut current = path.to_path_buf();
    loop {
        for mount in table {
            let mount_point = c_array_to_string(&mount.f_mntonname);
            if mount_point.is_empty() || Path::new(&mount_point) != current {
                continue;
            }
            let source = c_array_to_string(&mount.f_mntfromname);
            if !source.is_empty() {
                return Some(source);
            }
        }
        if current == Path::new("/") {
            return None;
        }
        current = current.parent()?.to_path_buf();
    }
}

/// Maps the path to the disk identifiers worth probing. `None` when the path
/// sits on something that is not a local disk.
pub fn resolve(path: &Path) -> Option<DiskTarget> {
    let existing = nearest_existing_dir(path)?;
    let mount_from = match mount_source(&existing) {
        Some(source) => source,
        None => {
            let c_path = to_c_path(&existing)?;
            let mut buf: libc::statfs = unsafe { std::mem::zeroed() };
            if unsafe { libc::statfs(c_path.as_ptr(), &mut buf) } != 0 {
                return None;
            }
            c_array_to_string(&buf.f_mntfromname)
        }
    };
    let bsd_name = mount_from.strip_prefix("/dev/")?;
    let whole = whole_disk_name(bsd_name)?;
    let mut candidates = vec![whole];
    if !candidates.iter().any(|candidate| candidate == bsd_name) {
        candidates.push(bsd_name.to_string());
    }
    Some(DiskTarget { candidates })
}

fn c_array_to_string(array: &[c_char]) -> String {
    let bytes: Vec<u8> = array
        .iter()
        .map(|value| *value as u8)
        .take_while(|value| *value != 0)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// `(device name, bytes read, bytes written)` for the counters that really
/// serve the volume.
///
/// On Apple Silicon the byte counters live only on the whole physical
/// `IOBlockStorageDriver`; APFS container media such as `disk3` publish no
/// statistics of their own. So the search starts at the media node named by
/// the candidates and climbs the IOService plane until a node carries a
/// `Statistics` dictionary. The name returned is the highest media name on
/// that chain, which is the device the counters belong to.
pub fn counters(target: &DiskTarget) -> Option<(String, u64, u64)> {
    // `IOServiceMatching` returns +1; `IOServiceGetMatchingServices` always
    // consumes exactly that reference, on failure as well as success.
    let matching = unsafe { ffi::IOServiceMatching(c"IOMedia".as_ptr()) };
    if matching.is_null() {
        return None;
    }
    let mut iterator: ffi::IoIterator = 0;
    let result = unsafe {
        ffi::IOServiceGetMatchingServices(ffi::KIO_MAIN_PORT_DEFAULT, matching, &mut iterator)
    };
    if result != ffi::KIO_RETURN_SUCCESS {
        return None;
    }
    let iterator = ffi::IoObj::new(iterator)?;

    loop {
        let service = unsafe { ffi::IOIteratorNext(iterator.get()) };
        if service == 0 {
            return None;
        }
        let Some(service) = ffi::IoObj::new(service) else {
            continue;
        };
        let Some(properties) = entry_properties(service.get()) else {
            continue;
        };
        let Some(name) = bsd_name(properties.get()) else {
            continue;
        };
        if !target.candidates.contains(&name) {
            continue;
        }
        if let Some(found) = counters_above(&name, service.get(), properties) {
            return Some(found);
        }
    }
}

fn entry_properties(entry: ffi::IoService) -> Option<ffi::CfRef> {
    let mut properties: *mut c_void = std::ptr::null_mut();
    let result = unsafe {
        ffi::IORegistryEntryCreateCFProperties(entry, &mut properties, std::ptr::null(), 0)
    };
    if result != ffi::KIO_RETURN_SUCCESS {
        return None;
    }
    ffi::CfRef::new(properties)
}

fn bsd_name(properties: *const c_void) -> Option<String> {
    ffi::dict_get(properties, "BSD Name").and_then(ffi::cf_to_string)
}

fn statistics_bytes(properties: *const c_void) -> Option<(u64, u64)> {
    let statistics = ffi::dict_get(properties, "Statistics")?;
    let read = ffi::dict_i64(statistics, "Bytes (Read)")?;
    let written = ffi::dict_i64(statistics, "Bytes (Write)")?;
    if read < 0 || written < 0 {
        return None;
    }
    Some((read as u64, written as u64))
}

/// Climbs the IOService plane from `entry` until a node publishes byte
/// counters. Each media name met on the way replaces the label, so the label
/// ends up naming the device that owns the counters.
fn counters_above(
    seed_name: &str,
    entry_handle: ffi::IoService,
    first_properties: ffi::CfRef,
) -> Option<(String, u64, u64)> {
    let mut label = seed_name.to_string();
    let mut entry = entry_handle;
    let mut properties = first_properties;
    // Ancestors stay retained here: releasing one would invalidate the handle
    // the next `IORegistryEntryGetParentEntry` call still needs.
    let mut chain: Vec<ffi::IoObj> = Vec::new();

    for _ in 0..12 {
        if let Some((read, written)) = statistics_bytes(properties.get()) {
            return Some((label, read, written));
        }
        let mut parent: ffi::IoService = 0;
        let result = unsafe {
            ffi::IORegistryEntryGetParentEntry(entry, c"IOService".as_ptr(), &mut parent)
        };
        if result != ffi::KIO_RETURN_SUCCESS {
            return None;
        }
        let parent = ffi::IoObj::new(parent)?;
        let parent_properties = entry_properties(parent.get())?;
        if let Some(name) = bsd_name(parent_properties.get()) {
            label = name;
        }
        entry = parent.get();
        properties = parent_properties;
        chain.push(parent);
    }
    None
}
