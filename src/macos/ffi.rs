//! Hand-written declarations for the Mach, CoreFoundation, IOKit, IOHID and
//! IOReport calls the dashboard uses. No bindgen, no wrapper crate.
//!
//! Ownership rules applied throughout this crate:
//!   * every `Create`/`Copy` call returns a +1 reference and is released exactly
//!     once through the `CfRef` wrapper below;
//! * `Get`/`GetValue` calls return borrowed references and are never released;
//!   * `IOServiceGetMatchingServices` consumes the matching dictionary;
//!   * the array from `host_processor_info` is freed with `vm_deallocate` at
//!     4 bytes per element (`integer_t`), not 8.

#![allow(non_camel_case_types)]
// The helper functions below take CoreFoundation opaque pointers that came
// from an FFI call. Each one null-checks before dereferencing, so the safe
// signatures are intentional rather than an oversight.
#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use std::ffi::{c_char, c_int, c_void};

// ---------------------------------------------------------------- Mach types

pub type MachPort = u32;
pub type TaskPort = u32;
pub type HostPortId = u32;
pub type Natural = u32;
pub type Integer = i32;
pub type MsgTypeNumber = u32;
pub type KernReturn = c_int;
pub type VmSize = u64;
pub type VmAddress = usize;

pub const KERN_SUCCESS: KernReturn = 0;

pub const PROCESSOR_CPU_LOAD_INFO: c_int = 2;
pub const CPU_STATE_USER: usize = 0;
pub const CPU_STATE_SYSTEM: usize = 1;
pub const CPU_STATE_IDLE: usize = 2;
pub const CPU_STATE_NICE: usize = 3;
pub const CPU_STATE_MAX: usize = 4;

pub const HOST_VM_INFO64: c_int = 4;

#[repr(C)]
pub struct ProcessorCpuLoadInfo {
    pub cpu_ticks: [u32; CPU_STATE_MAX],
}

/// Field-for-field copy of `mach/vm_statistics.h` `struct vm_statistics64`
/// (rev6 header). The kernel count passed to `host_statistics64` is derived
/// from this layout, and an older kernel can be served a truncated prefix.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VmStatistics64 {
    pub free_count: Natural,
    pub active_count: Natural,
    pub inactive_count: Natural,
    pub wire_count: Natural,
    pub zero_fill_count: u64,
    pub reactivations: u64,
    pub pageins: u64,
    pub pageouts: u64,
    pub faults: u64,
    pub cow_faults: u64,
    pub lookups: u64,
    pub hits: u64,
    pub purges: u64,
    pub purgeable_count: Natural,
    /// `speculative_count` pages are already inside `free_count`
    /// (`mach/vm_statistics.h`), so callers must not add them again.
    pub speculative_count: Natural,
    pub decompressions: u64,
    pub compressions: u64,
    pub swapins: u64,
    pub swapouts: u64,
    pub compressor_page_count: Natural,
    pub throttled_count: Natural,
    pub external_page_count: Natural,
    pub internal_page_count: Natural,
    pub total_uncompressed_pages_in_compressor: u64,
    pub swapped_count: u64,
    pub total_tag_storage_pages: u64,
    pub nontag_pageable_tag_storage_pages: u64,
    pub nontag_wired_tag_storage_pages: u64,
    pub free_tag_storage_pages: u64,
    pub tag_storing_tag_storage_pages: u64,
    pub total_tagged_pages: u64,
    pub resident_tagged_pages: u64,
    pub compressed_tagged_pages: u64,
    pub tagged_compressions: u64,
    pub tagged_decompressions: u64,
    pub compressed_tag_storage_bytes: u64,
    pub speculative_pages_created: u64,
    pub speculative_pages_activated: u64,
    pub swap_count: u64,
    pub empty_tag_storing_tag_storage_pages: u64,
    pub executable_count: u64,
    pub shared_region_count: u64,
    pub boot_stolen_count: u64,
    pub secluded_count: u64,
    pub active_internal_count: u64,
    pub inactive_internal_count: u64,
    pub active_external_count: u64,
    pub inactive_external_count: u64,
    pub purgeable_pageable_count: u64,
    pub purgeable_wired_count: u64,
    pub background_internal_count: u64,
    pub background_external_count: u64,
    pub donated_count: u64,
    pub realtime_count: u64,
    pub max_mem_count: u64,
    pub phantom_ghosts_found: u64,
    pub phantom_ghosts_added: u64,
}

/// `HOST_VM_INFO64_COUNT` for the layout above, plus the older kernel
/// prefixes this dashboard can fall back to.
pub const VM_STAT_COUNT_LATEST: MsgTypeNumber =
    (std::mem::size_of::<VmStatistics64>() / std::mem::size_of::<Integer>()) as MsgTypeNumber;
pub const VM_STAT_COUNT_REV5: MsgTypeNumber =
    (std::mem::offset_of!(VmStatistics64, executable_count) / std::mem::size_of::<Integer>())
        as MsgTypeNumber;
pub const VM_STAT_COUNT_REV4: MsgTypeNumber = (std::mem::offset_of!(VmStatistics64, swap_count)
    / std::mem::size_of::<Integer>())
    as MsgTypeNumber;
pub const VM_STAT_COUNT_REV3: MsgTypeNumber = (std::mem::offset_of!(
    VmStatistics64,
    total_tag_storage_pages
) / std::mem::size_of::<Integer>()) as MsgTypeNumber;

#[repr(C)]
pub struct XswUsage {
    pub xsu_total: u64,
    pub xsu_avail: u64,
    pub xsu_used: u64,
    pub xsu_pagesize: u32,
    pub xsu_encrypted: c_int,
}

pub const CTL_VM: c_int = 2;
pub const VM_SWAPUSAGE: c_int = 5;

// ------------------------------------------------------- CoreFoundation types

pub type CfAllocatorRef = *const c_void;
pub type CfStringRef = *const c_void;
pub type CfTypeRef = *const c_void;
pub type CfNumberRef = *const c_void;
pub type CfArrayRef = *const c_void;
pub type CfDictionaryRef = *const c_void;
pub type CfMutableDictionaryRef = *mut c_void;
pub type CfIndex = i64;
pub type CfHashCode = u64;

pub const KCF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
pub const KCF_NUMBER_INT_TYPE: c_int = 9;
pub const KCF_NUMBER_SINT64_TYPE: c_int = 4;

#[repr(C)]
pub struct CFDictionaryKeyCallBacks {
    pub version: CfIndex,
    pub cf_hash: Option<extern "C" fn(CfAllocatorRef, *const c_void) -> CfHashCode>,
    pub cf_retain: Option<extern "C" fn(CfAllocatorRef, *const c_void) -> *const c_void>,
    pub cf_release: Option<extern "C" fn(CfAllocatorRef, *const c_void)>,
    pub cf_equal: Option<extern "C" fn(CfAllocatorRef, *const c_void, *const c_void) -> c_int>,
    pub cf_copy_description: Option<extern "C" fn(CfAllocatorRef, *const c_void) -> CfStringRef>,
}

pub type CFDictionaryValueCallBacks = CFDictionaryKeyCallBacks;

// -------------------------------------------------------------- IOKit types

pub type IoService = MachPort;
pub type IoConnect = MachPort;
pub type IoIterator = MachPort;
pub type IoObject = MachPort;
pub type KernReturnIo = c_int;

pub const KIO_MAIN_PORT_DEFAULT: MachPort = 0;
pub const KIO_RETURN_SUCCESS: KernReturnIo = 0;

pub const IO_NAME_SIZE: usize = 128;
pub type IoName = [c_char; IO_NAME_SIZE];

// ------------------------------------------------------------------ sysctl

pub const CTL_MAXNAME: usize = 12;

// --------------------------------------------------------------- externals

extern "C" {
    pub static mach_task_self_: TaskPort;

    pub fn mach_host_self() -> HostPortId;
    pub fn mach_port_deallocate(task: TaskPort, name: MachPort) -> KernReturn;
    pub fn host_processor_info(
        host: HostPortId,
        flavor: c_int,
        processor_count: *mut Natural,
        processor_info: *mut *mut Integer,
        info_count: *mut MsgTypeNumber,
    ) -> KernReturn;
    pub fn host_statistics64(
        host: HostPortId,
        flavor: c_int,
        host_info: *mut VmStatistics64,
        host_info_count: *mut MsgTypeNumber,
    ) -> KernReturn;
    pub fn host_page_size(host: HostPortId, size: *mut VmSize) -> KernReturn;
    pub fn vm_deallocate(task: TaskPort, address: VmAddress, size: VmSize) -> KernReturn;

    pub fn sysctl(
        name: *const c_int,
        name_len: u32,
        old_value: *mut c_void,
        old_len: *mut usize,
        new_value: *mut c_void,
        new_len: usize,
    ) -> c_int;
    pub fn sysctlbyname(
        name: *const c_char,
        old_value: *mut c_void,
        old_len: *mut usize,
        new_value: *mut c_void,
        new_len: usize,
    ) -> c_int;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
    pub static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;

    pub fn CFRelease(cf: CfTypeRef);
    pub fn CFRetain(cf: CfTypeRef);
    pub fn CFStringCreateWithCString(
        alloc: CfAllocatorRef,
        c_str: *const c_char,
        encoding: u32,
    ) -> CfStringRef;
    pub fn CFStringGetCString(
        the_string: CfStringRef,
        buffer: *mut c_char,
        buffer_size: CfIndex,
        encoding: u32,
    ) -> c_int;
    pub fn CFStringGetLength(the_string: CfStringRef) -> CfIndex;
    pub fn CFNumberCreate(
        alloc: CfAllocatorRef,
        the_type: c_int,
        value_ptr: *const c_void,
    ) -> CfNumberRef;
    pub fn CFNumberGetValue(number: CfNumberRef, the_type: c_int, value_ptr: *mut c_void) -> c_int;
    pub fn CFDictionaryCreate(
        alloc: CfAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: CfIndex,
        key_callbacks: *const CFDictionaryKeyCallBacks,
        value_callbacks: *const CFDictionaryValueCallBacks,
    ) -> CfMutableDictionaryRef;
    pub fn CFDictionaryCreateMutableCopy(
        alloc: CfAllocatorRef,
        capacity: CfIndex,
        dict: CfDictionaryRef,
    ) -> CfMutableDictionaryRef;
    pub fn CFDictionaryGetValue(dict: CfDictionaryRef, key: CfTypeRef) -> CfTypeRef;
    pub fn CFDictionaryGetCount(dict: CfDictionaryRef) -> CfIndex;
    pub fn CFArrayGetCount(array: CfArrayRef) -> CfIndex;
    pub fn CFArrayGetValueAtIndex(array: CfArrayRef, index: CfIndex) -> CfTypeRef;
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    pub fn IOServiceMatching(name: *const c_char) -> CfMutableDictionaryRef;
    pub fn IOServiceGetMatchingServices(
        main_port: MachPort,
        matching: CfMutableDictionaryRef,
        existing: *mut IoIterator,
    ) -> KernReturnIo;
    pub fn IOServiceGetMatchingService(
        main_port: MachPort,
        matching: CfMutableDictionaryRef,
    ) -> IoService;
    pub fn IOServiceOpen(
        service: IoService,
        owning_task: TaskPort,
        connection_type: c_int,
        connect: *mut IoConnect,
    ) -> KernReturnIo;
    pub fn IOServiceClose(connect: IoConnect) -> KernReturnIo;
    pub fn IOObjectRelease(object: IoObject) -> KernReturnIo;
    pub fn IOIteratorNext(iterator: IoIterator) -> IoObject;
    pub fn IORegistryEntryCreateCFProperties(
        entry: IoService,
        properties: *mut CfMutableDictionaryRef,
        allocator: CfAllocatorRef,
        options: u32,
    ) -> KernReturnIo;
    pub fn IORegistryEntryGetParentEntry(
        entry: IoService,
        plane: *const c_char,
        parent: *mut IoService,
    ) -> KernReturnIo;
    pub fn IORegistryEntryGetName(entry: IoService, name: *mut c_char) -> KernReturnIo;
    pub fn IOConnectCallStructMethod(
        connection: IoConnect,
        selector: u32,
        input_struct: *const c_void,
        input_struct_size: usize,
        output_struct: *mut c_void,
        output_struct_size: *mut usize,
    ) -> KernReturnIo;

    // IOHID event system (exported by IOKit; only partly present in headers).
    pub fn IOHIDEventSystemClientCreate(allocator: CfAllocatorRef) -> *mut c_void;
    pub fn IOHIDEventSystemClientSetMatching(
        client: *mut c_void,
        match_dict: CfDictionaryRef,
    ) -> c_int;
    pub fn IOHIDEventSystemClientCopyServices(client: *mut c_void) -> CfArrayRef;
    pub fn IOHIDServiceClientCopyProperty(
        service: *const c_void,
        property: CfStringRef,
    ) -> CfTypeRef;
    pub fn IOHIDServiceClientCopyEvent(
        service: *const c_void,
        event_type: i64,
        options: i32,
        timestamp: i64,
    ) -> *mut c_void;
    pub fn IOHIDEventGetFloatValue(event: *mut c_void, field: i32) -> f64;
}

#[link(name = "IOReport")]
extern "C" {
    pub fn IOReportCopyChannelsInGroup(
        group: CfStringRef,
        sub_group: CfStringRef,
        match_value: u64,
        match_mask: u64,
        options: u64,
    ) -> CfDictionaryRef;
    pub fn IOReportMergeChannels(
        channels: CfDictionaryRef,
        more_channels: CfDictionaryRef,
        null: CfTypeRef,
    );
    pub fn IOReportCreateSubscription(
        user_data: *mut c_void,
        channels_to_subscribe: CfMutableDictionaryRef,
        dummy_out: *mut CfMutableDictionaryRef,
        subscription_options: u64,
        hypervisors: CfTypeRef,
    ) -> *mut c_void;
    pub fn IOReportCreateSamples(
        subscription: *mut c_void,
        channels_to_sample: CfMutableDictionaryRef,
        null: CfTypeRef,
    ) -> CfDictionaryRef;
    pub fn IOReportCreateSamplesDelta(
        prev_sample: CfDictionaryRef,
        new_sample: CfDictionaryRef,
        null: CfTypeRef,
    ) -> CfDictionaryRef;
    pub fn IOReportChannelGetGroup(row: CfDictionaryRef) -> CfStringRef;
    pub fn IOReportChannelGetSubGroup(row: CfDictionaryRef) -> CfStringRef;
    pub fn IOReportChannelGetChannelName(row: CfDictionaryRef) -> CfStringRef;
    pub fn IOReportChannelGetUnitLabel(row: CfDictionaryRef) -> CfStringRef;
    pub fn IOReportStateGetCount(row: CfDictionaryRef) -> i32;
    pub fn IOReportStateGetNameForIndex(row: CfDictionaryRef, index: i32) -> CfStringRef;
    pub fn IOReportStateGetResidency(row: CfDictionaryRef, index: i32) -> i64;
    pub fn IOReportSimpleGetIntegerValue(row: CfDictionaryRef, index: i32) -> i64;
}

// ------------------------------------------------------------- RAII wrappers

/// Owns one +1 CoreFoundation reference and releases it once.
pub struct CfRef {
    ptr: *const c_void,
}

impl CfRef {
    pub fn new(ptr: *const c_void) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(CfRef { ptr })
        }
    }

    pub fn get(&self) -> CfTypeRef {
        self.ptr
    }
}

impl Drop for CfRef {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { CFRelease(self.ptr) };
        }
    }
}

/// Owns one IOKit object (service, iterator) and releases it once.
pub struct IoObj {
    object: IoObject,
}

impl IoObj {
    pub fn new(object: IoObject) -> Option<Self> {
        if object == KIO_MAIN_PORT_DEFAULT {
            None
        } else {
            Some(IoObj { object })
        }
    }

    pub fn get(&self) -> IoObject {
        self.object
    }
}

impl Drop for IoObj {
    fn drop(&mut self) {
        unsafe { IOObjectRelease(self.object) };
    }
}

/// Owns an IOServiceOpen connection.
pub struct IoConn {
    connect: IoConnect,
}

impl IoConn {
    pub fn open(service: IoService, connection_type: c_int) -> Option<Self> {
        let mut connect: IoConnect = 0;
        let result =
            unsafe { IOServiceOpen(service, mach_task_self_, connection_type, &mut connect) };
        if result != KIO_RETURN_SUCCESS || connect == 0 {
            return None;
        }
        Some(IoConn { connect })
    }

    pub fn get(&self) -> IoConnect {
        self.connect
    }
}

impl Drop for IoConn {
    fn drop(&mut self) {
        unsafe { IOServiceClose(self.connect) };
    }
}

/// The host special port returned by `mach_host_self()`.
pub struct MachHost {
    port: HostPortId,
}

impl MachHost {
    pub fn new() -> Option<Self> {
        let port = unsafe { mach_host_self() };
        if port == 0 {
            None
        } else {
            Some(MachHost { port })
        }
    }

    pub fn get(&self) -> HostPortId {
        self.port
    }
}

impl Drop for MachHost {
    fn drop(&mut self) {
        unsafe { mach_port_deallocate(mach_task_self_, self.port) };
    }
}

/// `processor_info_array_t` returned by `host_processor_info`. Freed with
/// `vm_deallocate`, sized in `integer_t` (4-byte) units.
pub struct ProcessorInfoArray {
    pub ptr: *mut Integer,
    info_count: MsgTypeNumber,
    pub count: Natural,
}

impl ProcessorInfoArray {
    pub fn read(host: HostPortId, flavor: c_int) -> Option<Self> {
        let mut count: Natural = 0;
        let mut info: *mut Integer = std::ptr::null_mut();
        let mut info_count: MsgTypeNumber = 0;
        let result =
            unsafe { host_processor_info(host, flavor, &mut count, &mut info, &mut info_count) };
        if result != KERN_SUCCESS || info.is_null() || count == 0 {
            return None;
        }
        Some(ProcessorInfoArray {
            ptr: info,
            info_count,
            count,
        })
    }
}

impl Drop for ProcessorInfoArray {
    fn drop(&mut self) {
        unsafe {
            vm_deallocate(
                mach_task_self_,
                self.ptr as VmAddress,
                self.info_count as VmSize * std::mem::size_of::<Integer>() as VmSize,
            );
        }
    }
}

// ------------------------------------------------------------- CF utilities

pub fn cf_string(text: &str) -> Option<CfRef> {
    let c_text = match std::ffi::CString::new(text) {
        Ok(value) => value,
        Err(_) => return None,
    };
    unsafe {
        CfRef::new(CFStringCreateWithCString(
            std::ptr::null(),
            c_text.as_ptr(),
            KCF_STRING_ENCODING_UTF8,
        ))
    }
}

/// Copies a CFString into a Rust `String`. Borrowed reference: not released.
pub fn cf_to_string(value: CfTypeRef) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let mut buffer = [0i8; 512];
    let ok = unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as CfIndex,
            KCF_STRING_ENCODING_UTF8,
        )
    };
    if ok == 0 {
        return None;
    }
    let bytes: Vec<u8> = buffer
        .iter()
        .map(|byte| *byte as u8)
        .take_while(|byte| *byte != 0)
        .collect();
    String::from_utf8(bytes).ok()
}

pub fn cf_number_i64(value: CfTypeRef) -> Option<i64> {
    if value.is_null() {
        return None;
    }
    let mut number: i64 = 0;
    let ok = unsafe {
        CFNumberGetValue(
            value,
            KCF_NUMBER_SINT64_TYPE,
            &mut number as *mut i64 as *mut c_void,
        )
    };
    if ok == 0 {
        None
    } else {
        Some(number)
    }
}

pub fn cf_number_i32(value: CfTypeRef) -> Option<i32> {
    if value.is_null() {
        return None;
    }
    let mut number: i32 = 0;
    let ok = unsafe {
        CFNumberGetValue(
            value,
            KCF_NUMBER_INT_TYPE,
            &mut number as *mut i32 as *mut c_void,
        )
    };
    if ok == 0 {
        None
    } else {
        Some(number)
    }
}

/// Borrows a dictionary value. The value stays owned by the dictionary.
pub fn dict_get(dict: CfDictionaryRef, key: &str) -> Option<CfTypeRef> {
    if dict.is_null() {
        return None;
    }
    let key_ref = cf_string(key)?;
    let value = unsafe { CFDictionaryGetValue(dict, key_ref.get()) };
    if value.is_null() {
        None
    } else {
        Some(value)
    }
}

pub fn dict_i64(dict: CfDictionaryRef, key: &str) -> Option<i64> {
    cf_number_i64(dict_get(dict, key)?)
}

pub fn array_len(array: CfArrayRef) -> usize {
    if array.is_null() {
        return 0;
    }
    unsafe { CFArrayGetCount(array) as usize }
}

pub fn array_at(array: CfArrayRef, index: usize) -> Option<CfTypeRef> {
    if array.is_null() {
        return None;
    }
    let value = unsafe { CFArrayGetValueAtIndex(array, index as CfIndex) };
    if value.is_null() {
        None
    } else {
        Some(value)
    }
}

/// `sysctlbyname` for a scalar value.
pub fn sysctl_scalar<T: Copy>(name: &str) -> Option<T> {
    let c_name = std::ffi::CString::new(name).ok()?;
    let mut value: T = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<T>();
    let result = unsafe {
        sysctlbyname(
            c_name.as_ptr(),
            &mut value as *mut T as *mut c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || len < std::mem::size_of::<T>() {
        return None;
    }
    Some(value)
}

/// `sysctl({CTL_VM, VM_SWAPUSAGE})` for the swap usage struct.
pub fn sysctl_swap_usage() -> Option<XswUsage> {
    let name = [CTL_VM, VM_SWAPUSAGE];
    let mut usage: XswUsage = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<XswUsage>();
    let result = unsafe {
        sysctl(
            name.as_ptr(),
            name.len() as u32,
            &mut usage as *mut XswUsage as *mut c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || len < std::mem::size_of::<XswUsage>() {
        return None;
    }
    Some(usage)
}
