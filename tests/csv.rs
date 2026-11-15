//! CSV contract: 13 columns, one header, appended rows, missing values as `--`.

mod common;

use common::{Harness, TmpDir, BASE_STATUS, FIXTURE_SAMPLE, GATEWAY_STATS, NOW};
use splash_monitor::csv::{timestamp_local, CsvLog, HEADER};

#[test]
fn header_is_written_once_and_rows_have_thirteen_columns() {
    let dir = TmpDir::new("csv");
    let path = dir.path().join("nested/sub/run.csv");
    let harness = Harness::from_fixture(Some(BASE_STATUS), FIXTURE_SAMPLE);

    let log = CsvLog::open(&path).expect("open log");
    log.write_frame(&harness.frame(true), NOW).expect("row 1");
    log.write_frame(&harness.frame(true), NOW + 5)
        .expect("row 2");
    drop(log);

    let text = std::fs::read_to_string(&path).expect("read log");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], HEADER);
    assert_eq!(lines.len(), 3);

    for line in &lines[1..] {
        let fields: Vec<&str> = line.split(',').collect();
        assert_eq!(fields.len(), 13, "row {line}");
        assert_eq!(fields[1], "41.02");
        assert_eq!(fields[3], "42.00");
        assert_eq!(fields[4], "NORMAL");
        assert_eq!(fields[5], "30");
        assert_eq!(fields[6], "6.78");
        assert_eq!(fields[12], "true");
        assert!(fields[7] == "--" || fields[7].parse::<f64>().is_ok());
    }

    // A second run appends without repeating the header.
    let log = CsvLog::open(&path).expect("reopen log");
    log.write_frame(&harness.frame(true), NOW + 10)
        .expect("row 3");
    drop(log);
    let appended = std::fs::read_to_string(&path).expect("read log");
    let lines: Vec<&str> = appended.lines().collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0], HEADER);
    assert!(!lines[1..].contains(&HEADER));
}

#[test]
fn offline_rows_record_missing_values_not_zeros() {
    let dir = TmpDir::new("csv-offline");
    let path = dir.path().join("offline.csv");
    let harness = Harness::offline();
    let log = CsvLog::open(&path).expect("open log");
    log.write_frame(&harness.frame(false), NOW).expect("row");
    drop(log);

    let lines: Vec<String> = std::fs::read_to_string(&path)
        .expect("read log")
        .lines()
        .map(str::to_string)
        .collect();
    let fields: Vec<&str> = lines[1].split(',').collect();
    assert_eq!(fields.len(), 13);
    for index in [1, 2, 3, 6, 7, 8, 9, 10, 12] {
        assert_eq!(fields[index], "--", "column {index} of {}", lines[1]);
    }
}

#[test]
fn gateway_rows_preserve_missing_spill_values_and_engine_rows_preserve_counts() {
    let dir = TmpDir::new("csv-gateway");
    let path = dir.path().join("spill.csv");
    let log = CsvLog::open(&path).expect("open log");
    let gateway = Harness::gateway(Some(GATEWAY_STATS), Default::default());
    let engine = Harness::with(Some(BASE_STATUS), Default::default());
    let zero_spill = Harness::with(
        Some(r#"{"kv_spill":{"pages":{"spilled":0,"spilled_bytes":0}}}"#),
        Default::default(),
    );
    for harness in [&gateway, &engine, &zero_spill] {
        log.write_frame(&harness.frame(false), NOW).expect("row");
    }

    let text = std::fs::read_to_string(&path).expect("read log");
    let mut rows = text.lines().skip(1);
    for (pages, gib) in [("--", "--"), ("1048576", "4.005"), ("0", "0.000")] {
        let row = rows.next().expect("spill row");
        let fields: Vec<&str> = row.split(',').collect();
        assert_eq!(fields[10], pages, "kv_spilled_pages in {row}");
        assert_eq!(fields[11], gib, "kv_spilled_GiB in {row}");
    }
}

#[test]
fn timestamps_are_local_wall_clock() {
    let text = timestamp_local(NOW);
    let chars: Vec<char> = text.chars().collect();
    assert_eq!(chars.len(), 19, "timestamp {text}");
    assert_eq!(chars[4], '-');
    assert_eq!(chars[7], '-');
    assert_eq!(chars[10], ' ');
    assert_eq!(chars[13], ':');
    assert_eq!(chars[16], ':');
    assert!(chars.iter().enumerate().all(|(index, ch)| match index {
        4 | 7 => *ch == '-',
        10 => *ch == ' ',
        13 | 16 => *ch == ':',
        _ => ch.is_ascii_digit(),
    }));
}

#[test]
fn unopenable_log_path_is_reported_by_the_binary() {
    let dir = TmpDir::new("csv-blocked");
    // A directory in place of the log file: opening it must fail with exit 1.
    std::fs::create_dir_all(dir.path().join("log.csv")).expect("mkdir");
    let output = common::run_binary(
        &[
            "--once",
            "--no-sensors",
            "--url",
            "http://127.0.0.1:1",
            "--log",
            dir.path().join("log.csv").to_str().expect("path"),
        ],
        &[],
    );
    let (code, _, stderr) = common::codes(&output);
    assert_eq!(code, 1, "stderr {stderr}");
    assert!(stderr.contains("Cannot write log file"));
}
