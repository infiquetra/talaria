//! The replay entry point: argument parsing, and the paths that end before a terminal is
//! touched. Anything that needs a pseudo-terminal is tested from the benchmark crate.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use hermes_client::replay_source::Speed;

// The binary crate cannot be imported from here, so the module is compiled into this test.
#[allow(dead_code)]
#[path = "../src/bench_replay.rs"]
mod bench_replay;

use bench_replay::{ArgError, BenchReplayArgs, parse_args};

const MISSING_FIXTURE: &str = "no-such-fixture.jsonl";

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| item.to_string()).collect()
}

fn parsed(list: &[&str]) -> BenchReplayArgs {
    match parse_args(&strings(list)) {
        Ok(Some(args)) => args,
        Ok(None) => panic!("{list:?} should be a replay request, got none"),
        Err(error) => panic!("{list:?} should parse, got {error:?}"),
    }
}

fn speed_of(value: &str) -> Speed {
    parsed(&["--bench-replay", "a.jsonl", "--bench-speed", value]).speed
}

fn assert_bad_speed(value: &str) {
    match parse_args(&strings(&[
        "--bench-replay",
        "a.jsonl",
        "--bench-speed",
        value,
    ])) {
        Err(ArgError::BadSpeed(_)) => {}
        Err(other) => panic!("speed {value:?} should be a bad speed, got {other:?}"),
        Ok(_) => panic!("speed {value:?} should be rejected"),
    }
}

fn assert_missing_value(list: &[&str]) {
    match parse_args(&strings(list)) {
        Err(ArgError::MissingValue(_)) => {}
        Err(other) => panic!("{list:?} should lack a value, got {other:?}"),
        Ok(_) => panic!("{list:?} should be rejected"),
    }
}

#[test]
fn parse_args_absent_speeds_and_errors() {
    // Without the flag the normal path runs.
    for list in [
        &[][..],
        &["--version"][..],
        &["--help"][..],
        &["--bench-speed", "10"][..],
    ] {
        assert!(
            matches!(parse_args(&strings(list)), Ok(None)),
            "{list:?} has no replay flag and should fall through"
        );
    }

    // Defaults: recorded speed, no report path.
    let defaults = parsed(&["--bench-replay", "a.jsonl"]);
    assert_eq!(defaults.fixture, PathBuf::from("a.jsonl"));
    assert_eq!(defaults.speed, Speed::Recorded);
    assert_eq!(defaults.report, None);

    // The options in either order, with the report path.
    let full = parsed(&[
        "--bench-speed",
        "10",
        "--bench-report",
        "out.json",
        "--bench-replay",
        "b.jsonl",
    ]);
    assert_eq!(full.fixture, PathBuf::from("b.jsonl"));
    assert_eq!(full.speed, Speed::Factor(10.0));
    assert_eq!(full.report, Some(PathBuf::from("out.json")));

    assert_eq!(speed_of("instant"), Speed::Instant);
    assert_eq!(speed_of("recorded"), Speed::Recorded);
    assert_eq!(speed_of("10"), Speed::Factor(10.0));
    assert_eq!(speed_of("0.5"), Speed::Factor(0.5));

    for value in ["0", "0.0", "-1", "abc", "inf", "NaN", "1e400"] {
        assert_bad_speed(value);
    }

    assert_missing_value(&["--bench-replay", "a.jsonl", "--bench-speed"]);
    assert_missing_value(&["--bench-replay", "a.jsonl", "--bench-report"]);
    assert!(
        parse_args(&strings(&["--bench-replay"])).is_err(),
        "a replay flag without a fixture should be rejected"
    );
}

struct Outcome {
    code: Option<i32>,
    bytes: Vec<u8>,
}

/// Runs the binary with piped output and no input, from the package directory.
fn run_talaria(args: &[&str]) -> Outcome {
    let output = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::null())
        .output()
        .expect("run the talaria binary");
    let mut bytes = output.stdout;
    bytes.extend_from_slice(&output.stderr);
    Outcome {
        code: output.status.code(),
        bytes,
    }
}

/// The single non-empty line the run printed, after checking there is exactly one and no
/// terminal control byte.
fn only_line(outcome: &Outcome) -> String {
    assert!(
        !outcome.bytes.contains(&0x1b),
        "a piped run must not write escape sequences"
    );
    let text = String::from_utf8_lossy(&outcome.bytes).into_owned();
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected one plain line, got {text:?}");
    lines[0].to_string()
}

#[test]
fn missing_fixture_and_piped_output_exit_2() {
    // A fixture that does not exist is reported by the name it was given, and the
    // terminal check never comes first.
    let missing = run_talaria(&["--bench-replay", MISSING_FIXTURE]);
    assert_eq!(missing.code, Some(2), "a missing fixture exits 2");
    let line = only_line(&missing);
    assert!(
        line.contains(MISSING_FIXTURE),
        "the line should name the path as given: {line:?}"
    );
    assert!(
        !line.contains("needs a terminal"),
        "the fixture is opened before the terminal is checked: {line:?}"
    );

    // A fixture that exists still needs a terminal when the output is piped.
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/plain-answer.jsonl");
    let fixture = fixture.to_str().expect("fixture path is text");
    let piped = run_talaria(&["--bench-replay", fixture]);
    assert_eq!(piped.code, Some(2), "piped output exits 2");
    let line = only_line(&piped);
    assert!(
        line.contains("needs a terminal"),
        "the line should say a terminal is needed: {line:?}"
    );
}

#[test]
fn benchmark_rejects_a_recording_with_dropped_frames_before_opening_terminal() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("loss.jsonl");
    std::fs::write(
        &path,
        concat!(
            "{\"kind\":\"header\",\"version\":3}\n",
            "{\"kind\":\"dropped\",\"seq\":1,\"tMs\":0,\"count\":100}\n",
        ),
    )
    .expect("recording");
    let result = run_talaria(&["--bench-replay", path.to_str().expect("path")]);
    assert_eq!(result.code, Some(2));
    let line = only_line(&result);
    assert!(line.contains("100") && line.contains("dropped"), "{line}");
}

#[test]
fn bad_speed_exits_2_with_one_plain_line() {
    // The speed is checked before the fixture, so the fixture need not exist.
    for value in ["0", "-1", "abc", "inf", "NaN"] {
        let outcome = run_talaria(&["--bench-replay", MISSING_FIXTURE, "--bench-speed", value]);
        assert_eq!(outcome.code, Some(2), "speed {value:?} exits 2");
        let line = only_line(&outcome);
        assert!(
            line.contains("--bench-speed must be instant, recorded or a positive number"),
            "speed {value:?} gave {line:?}"
        );
    }
}
