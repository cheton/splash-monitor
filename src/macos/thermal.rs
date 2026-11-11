//! CPU die / package / GPU temperatures.
//!
//! IOHID sensor matching and name classification follow btop++
//! `src/osx/sensors.cpp` (Apache-2.0, Copyright 2021 Aristocratos). Derived
//! from that code and modified for this project.
//!
//! The AppleSMC `Tg*` GPU key list is not from btop++: that project reads no
//! `Tg*` keys. It came from the native helper that shipped with the earlier
//! version of this dashboard.

use std::collections::BTreeMap;
use std::ffi::c_void;

use crate::macos::ffi;

const TEMP_EVENT_TYPE: i64 = 15;
const TEMP_EVENT_FIELD: i32 = 15 << 16;
const PRIMARY_USAGE_PAGE: i32 = 0xff00;
const PRIMARY_USAGE: i32 = 5;

#[derive(Clone, Default)]
pub struct ThermalSample {
    /// Per-sensor readings, ordered by sensor index (`PMU tdie 1` first) or,
    /// when the SoC exposes no indexed sensors, by sensor name.
    pub cpu_sensors: Vec<(String, f64)>,
    pub cpu_package: Option<f64>,
    pub gpu: Option<f64>,
}

fn average(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

/// `PMU tdie 10` -> 10. `None` when the prefix is absent or the remainder does
/// not start with decimal digits.
fn sensor_index(name: &str, prefix: &str) -> Option<i32> {
    let rest = name.strip_prefix(prefix)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<i32>().ok()
}

fn cf_literal(text: &str) -> Option<ffi::CfRef> {
    ffi::cf_string(text)
}

// ------------------------------------------------------------ AppleSMC path

fn fourcc(key: &str) -> u32 {
    let bytes = key.as_bytes();
    (u32::from(bytes[0]) << 24)
        | (u32::from(bytes[1]) << 16)
        | (u32::from(bytes[2]) << 8)
        | u32::from(bytes[3])
}

#[repr(C)]
struct SmcVersion {
    major: i8,
    minor: i8,
    build: i8,
    reserved: i8,
    release: u16,
}

#[repr(C)]
struct SmcLimits {
    version: u16,
    length: u16,
    cpu: u32,
    gpu: u32,
    mem: u32,
}

#[repr(C)]
struct SmcKeyInfo {
    size: u32,
    key_type: u32,
    attributes: i8,
}

#[repr(C)]
struct SmcData {
    key: u32,
    version: SmcVersion,
    limits: SmcLimits,
    key_info: SmcKeyInfo,
    result: i8,
    status: i8,
    command: i8,
    data32: u32,
    bytes: [i8; 32],
}

const _: () = assert!(std::mem::size_of::<SmcData>() == 80);

const SMC_SELECTOR: u32 = 2;
const SMC_COMMAND_KEY_INFO: i8 = 9;
const SMC_COMMAND_READ: i8 = 5;

/// GPU-specific AppleSMC keys seen on M1/M2/M4/M5 machines. Missing keys are
/// normal: they simply do not contribute to the average.
const SMC_GPU_KEYS: [&str; 20] = [
    "Tg0U", "Tg0X", "Tg0d", "Tg0g", "Tg0j", "Tg1Y", "Tg1c", "Tg1g", "Tg0G", "Tg0H", "Tg1U", "Tg1k",
    "Tg0K", "Tg0L", "Tg0e", "Tg0k", "Tg0f", "Tg05", "Tg0D", "Tg0T",
];

impl SmcData {
    fn zeroed() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

fn smc_call(connect: u32, request: &SmcData, response: &mut SmcData) -> bool {
    let mut len = std::mem::size_of::<SmcData>();
    let result = unsafe {
        ffi::IOConnectCallStructMethod(
            connect,
            SMC_SELECTOR,
            request as *const SmcData as *const c_void,
            std::mem::size_of::<SmcData>(),
            response as *mut SmcData as *mut c_void,
            &mut len,
        )
    };
    result == ffi::KIO_RETURN_SUCCESS
}

fn smc_read_key(connect: u32, key: &str) -> Option<f64> {
    let mut request = SmcData::zeroed();
    let mut response = SmcData::zeroed();
    request.key = fourcc(key);
    request.command = SMC_COMMAND_KEY_INFO;
    if !smc_call(connect, &request, &mut response) {
        return None;
    }
    let key_size = response.key_info.size;
    let key_type = response.key_info.key_type;
    if key_size == 0 || key_size > 32 {
        return None;
    }

    request.key_info.size = key_size;
    request.command = SMC_COMMAND_READ;
    response = SmcData::zeroed();
    if !smc_call(connect, &request, &mut response) {
        return None;
    }

    let byte_at = |index: usize| response.bytes[index] as u8;
    let value = if key_type == fourcc("flt ") && key_size >= 4 {
        let bytes: [u8; 4] = [byte_at(0), byte_at(1), byte_at(2), byte_at(3)];
        f32::from_le_bytes(bytes) as f64
    } else if key_type == fourcc("sp78") && key_size >= 2 {
        response.bytes[0] as f64 + f64::from(byte_at(1)) / 256.0
    } else if key_type == fourcc("ui16") && key_size >= 2 {
        f64::from(u16::from_be_bytes([byte_at(0), byte_at(1)]))
    } else {
        return None;
    };

    if value.is_finite() && value > 0.0 && value < 150.0 {
        Some(value)
    } else {
        None
    }
}

fn smc_gpu_temperature(log: &mut Option<Vec<String>>) -> Option<f64> {
    // `IOServiceMatching` returns +1; `IOServiceGetMatchingService` always
    // consumes exactly that reference.
    let matching = unsafe { ffi::IOServiceMatching(c"AppleSMC".as_ptr()) };
    if matching.is_null() {
        return None;
    }
    let service = unsafe { ffi::IOServiceGetMatchingService(ffi::KIO_MAIN_PORT_DEFAULT, matching) };
    let service = ffi::IoObj::new(service)?;
    let connect = ffi::IoConn::open(service.get(), 0)?;

    let mut values = Vec::new();
    for key in SMC_GPU_KEYS {
        let value = smc_read_key(connect.get(), key);
        if let Some(lines) = log.as_mut() {
            match value {
                Some(temp) => lines.push(format!("smc {key} = {temp:.2}")),
                None => lines.push(format!("smc {key} = unavailable")),
            }
        }
        if let Some(temp) = value {
            values.push(temp);
        }
    }
    average(&values)
}

// ------------------------------------------------------------- IOHID sensors

/// Reads every matching IOHID temperature service, classifies it, then lets
/// the AppleSMC GPU keys override the IOHID GPU average.
///
/// When `log` is present it collects one line per probed key, which
/// `--debug-sensors` prints to stderr.
pub fn read(log: &mut Option<Vec<String>>) -> ThermalSample {
    let page_number = ffi::CfRef::new(unsafe {
        ffi::CFNumberCreate(
            std::ptr::null(),
            ffi::KCF_NUMBER_INT_TYPE,
            &PRIMARY_USAGE_PAGE as *const i32 as *const c_void,
        )
    });
    let usage_number = ffi::CfRef::new(unsafe {
        ffi::CFNumberCreate(
            std::ptr::null(),
            ffi::KCF_NUMBER_INT_TYPE,
            &PRIMARY_USAGE as *const i32 as *const c_void,
        )
    });
    let page_key = cf_literal("PrimaryUsagePage");
    let usage_key = cf_literal("PrimaryUsage");
    let product_key = cf_literal("Product");
    let (Some(page_number), Some(usage_number), Some(page_key), Some(usage_key), Some(product_key)) =
        (page_number, usage_number, page_key, usage_key, product_key)
    else {
        return ThermalSample::default();
    };
    let keys: [*const c_void; 2] = [page_key.get(), usage_key.get()];
    let values: [*const c_void; 2] = [page_number.get(), usage_number.get()];
    let matching = match ffi::CfRef::new(unsafe {
        ffi::CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            2,
            &ffi::kCFTypeDictionaryKeyCallBacks,
            &ffi::kCFTypeDictionaryValueCallBacks,
        )
    }) {
        Some(value) => value,
        None => return ThermalSample::default(),
    };

    let client = unsafe { ffi::IOHIDEventSystemClientCreate(std::ptr::null()) };
    if client.is_null() {
        return ThermalSample::default();
    }
    unsafe { ffi::IOHIDEventSystemClientSetMatching(client, matching.get()) };
    let services_ref = unsafe { ffi::IOHIDEventSystemClientCopyServices(client) };

    let mut accel: Vec<f64> = Vec::new();
    let mut acc_named: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut tdie: Vec<f64> = Vec::new();
    let mut tdie_indexed: BTreeMap<i32, Vec<f64>> = BTreeMap::new();
    let mut soc: Vec<f64> = Vec::new();
    let mut gpu: Vec<f64> = Vec::new();
    let mut logged: BTreeMap<String, Vec<Option<f64>>> = BTreeMap::new();

    if let Some(services) = ffi::CfRef::new(services_ref) {
        for index in 0..ffi::array_len(services.get()) {
            let Some(service) = ffi::array_at(services.get(), index) else {
                continue;
            };
            let property =
                unsafe { ffi::IOHIDServiceClientCopyProperty(service, product_key.get()) };
            let name =
                match ffi::CfRef::new(property).and_then(|owned| ffi::cf_to_string(owned.get())) {
                    Some(text) if !text.is_empty() => text,
                    _ => continue,
                };
            let event = unsafe { ffi::IOHIDServiceClientCopyEvent(service, TEMP_EVENT_TYPE, 0, 0) };
            if event.is_null() {
                continue;
            }
            let temperature = unsafe { ffi::IOHIDEventGetFloatValue(event, TEMP_EVENT_FIELD) };
            unsafe { ffi::CFRelease(event as *const c_void) };

            let valid = temperature.is_finite() && temperature > 0.0 && temperature < 150.0;
            if log.is_some() {
                // IOHID exposes several service instances per sensor name. The
                // debug list keeps one aggregated line per sensor.
                logged.entry(name.clone()).or_default().push(if valid {
                    Some(temperature)
                } else {
                    None
                });
            }
            if !valid {
                continue;
            }

            if name.starts_with("eACC") || name.starts_with("pACC") {
                accel.push(temperature);
                acc_named.entry(name.clone()).or_default().push(temperature);
            } else if name.starts_with("PMU tdie") {
                tdie.push(temperature);
                if let Some(index) = sensor_index(&name, "PMU tdie") {
                    tdie_indexed.entry(index).or_default().push(temperature);
                }
            } else if name.starts_with("SOC MTR Temp Sensor") {
                soc.push(temperature);
            }

            if name.contains("GPU") || (name.starts_with("PMU TP") && name.ends_with('g')) {
                gpu.push(temperature);
            }
        }
    }
    unsafe { ffi::CFRelease(client as *const c_void) };

    if let Some(lines) = log.as_mut() {
        for (name, readings) in &logged {
            let valid: Vec<f64> = readings.iter().flatten().copied().collect();
            match average(&valid) {
                Some(value) => lines.push(format!(
                    "sensor {name} = {value:.2} ({} readings)",
                    valid.len()
                )),
                None => lines.push(format!("sensor {name} = unavailable")),
            }
        }
    }

    let mut sample = ThermalSample::default();
    if !tdie_indexed.is_empty() {
        for (index, readings) in &tdie_indexed {
            if let Some(value) = average(readings) {
                sample
                    .cpu_sensors
                    .push((format!("PMU tdie {index}"), value));
            }
        }
    } else {
        for (name, readings) in &acc_named {
            if let Some(value) = average(readings) {
                sample.cpu_sensors.push((name.clone(), value));
            }
        }
    }
    sample.cpu_package = average(&accel)
        .or_else(|| average(&tdie))
        .or_else(|| average(&soc));
    sample.gpu = average(&gpu);
    if let Some(smc) = smc_gpu_temperature(&mut *log) {
        sample.gpu = Some(smc);
    }
    if let Some(lines) = log.as_mut() {
        let source = if tdie_indexed.is_empty() {
            "ACC"
        } else {
            "PMU tdie"
        };
        lines.push(format!(
            "cpu core sensors used: {} ({source})",
            sample.cpu_sensors.len()
        ));
    }
    sample
}
