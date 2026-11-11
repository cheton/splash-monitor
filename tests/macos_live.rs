//! Real sensor probes. Ignored by default because they read live hardware;
//! CI runs them on `macos-latest` with `cargo test -- --include-ignored`.

mod common;

use splash_monitor::macos::MacProbe;
use splash_monitor::system::{SystemProbe, SystemSample};

fn assert_plausible(sample: &SystemSample) {
    assert!(
        sample.page_size == 4096 || sample.page_size == 16384,
        "page size {}",
        sample.page_size
    );
    assert!(sample.cpu_cores.len() >= 4, "core count");
    if let Some(ratio) = sample.cpu_active_ratio {
        assert!((0.0..=1.0).contains(&ratio), "cpu ratio {ratio}");
    }
    for (label, value) in &sample.cpu_sensors {
        assert!(!label.is_empty(), "sensor label");
        assert!(*value > 0.0 && *value < 120.0, "sensor {label} = {value}");
    }
    if let Some(temp) = sample.cpu_package {
        assert!(temp > 0.0 && temp < 120.0, "cpu package {temp}");
    }
    if let Some(ratio) = sample.gpu_active_ratio {
        assert!((0.0..=1.0).contains(&ratio), "gpu ratio {ratio}");
    }
    if let Some(temp) = sample.gpu_temp {
        assert!(temp > 0.0 && temp < 120.0, "gpu temp {temp}");
    }
    if let Some(watts) = sample.gpu_power_w {
        assert!(watts > 0.0 && watts < 300.0, "gpu watts {watts}");
    }
    if let Some(bytes) = sample.swap_used_bytes {
        assert!(bytes > 0, "swap used {bytes}");
    }
    if let Some(percent) = sample.free_percent {
        assert!(percent <= 100, "free percent {percent}");
    }
    assert!(sample.pressure_level.is_some(), "pressure level");
}

#[test]
#[ignore = "reads live macOS sensors"]
fn live_probes_report_plausible_values_or_none() {
    let cache_dir = std::env::temp_dir();
    let mut probe = MacProbe::new(true, Some(&cache_dir));
    let first = probe.sample();
    std::thread::sleep(std::time::Duration::from_millis(600));
    let second = probe.sample();

    assert_plausible(&first);
    assert_plausible(&second);

    // Temperatures are never reported as zero, even when a sensor is absent.
    assert!(!second.cpu_sensors.iter().any(|(_, value)| *value == 0.0));
    assert!(
        !second.telemetry_unavailable,
        "probe produced no usable reading"
    );
    assert!(
        second.cpu_active_ratio.is_some(),
        "two Mach samples must produce a CPU ratio"
    );
    assert_eq!(
        second.cpu_cores.len(),
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0),
        "one ratio per logical core"
    );
    assert!(
        second
            .cpu_sensors
            .iter()
            .any(|(label, _)| label.contains("tdie")),
        "expected PMU tdie sensors"
    );
    if let Some(volume) = &second.volume {
        assert!(
            volume.device.starts_with("disk"),
            "device {}",
            volume.device
        );
        assert!(volume.read_bytes.is_some() && volume.write_bytes.is_some());
    }
}

#[test]
#[ignore = "reads live macOS sensors"]
fn json_sample_matches_the_documented_shape() {
    let output = common::run_binary(&["--json"], &[]);
    let (code, stdout, stderr) = common::codes(&output);
    assert_eq!(code, 0, "stderr {stderr}");
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("--json output");

    let page_size = value["page_size"].as_u64().expect("page size");
    assert!(
        page_size == 4096 || page_size == 16384,
        "page size {page_size}"
    );
    assert!(!value["cpu_core_temps"]
        .as_array()
        .expect("temps")
        .is_empty());
    for entry in value["cpu_core_temps"].as_array().expect("temps") {
        let celsius = entry["celsius"].as_f64().expect("celsius");
        assert!(celsius > 0.0 && celsius < 120.0, "core temp {celsius}");
    }
    let ratio = value["cpu_active_ratio"].as_f64().expect("cpu ratio");
    assert!((0.0..=1.0).contains(&ratio));
    assert!(value["swap_used_bytes"].as_u64().expect("swap") > 0);
    assert!(value["free_percent"].as_u64().expect("free") <= 100);
    match &value["gpu_active_ratio"] {
        serde_json::Value::Number(_) => {}
        serde_json::Value::Null => {}
        other => panic!("unexpected gpu_active_ratio {other}"),
    }
}
