//! Echo detection and the text the generator must keep out of the reply stream.

mod support;

use std::path::Path;

use talaria_bench::{Emulator, generate, typed_prefix_visible};

/// The text the harness types, one character per key.
const PROBE: &str = "^kbdprobe-0123456789-abcdefghijklmnopqrs";

#[test]
fn typed_prefix_visible_detects_echo() {
    assert_eq!(PROBE.chars().count(), 40);

    // The composer shows the first five characters: the prefix of five is there, six is not.
    let mut emulator = Emulator::new(48, 160);
    emulator.feed(b"> ^kbdp");
    assert!(typed_prefix_visible(&emulator, PROBE, 1));
    assert!(typed_prefix_visible(&emulator, PROBE, 5));
    assert!(!typed_prefix_visible(&emulator, PROBE, 6));

    // The tail of the probe without its leading caret is ordinary text, not an echo.
    let mut emulator = Emulator::new(48, 160);
    emulator.feed(b"> kbdp");
    assert!(!typed_prefix_visible(&emulator, PROBE, 1));
    assert!(!typed_prefix_visible(&emulator, PROBE, 5));

    // A blank screen shows nothing.
    let emulator = Emulator::new(48, 160);
    assert!(!typed_prefix_visible(&emulator, PROBE, 1));
}

#[test]
fn typed_prefix_visible_joins_wrapped_rows() {
    // Ten columns: the echo fills row 1 and continues on row 2.
    let mut emulator = Emulator::new(10, 10);
    emulator.feed(b"> ^kbdprob");
    emulator.feed(b"e-01");
    assert!(typed_prefix_visible(&emulator, PROBE, 12));
    assert!(!typed_prefix_visible(&emulator, PROBE, 13));
}

/// Every `.jsonl` file the generator wrote, as parsed records without the header.
fn generated_records(out: &Path) -> Vec<(String, Vec<serde_json::Value>)> {
    let mut files = Vec::new();
    for name in [
        "empty.jsonl",
        "reference-stream.jsonl",
        "long-session.jsonl",
    ] {
        let text = std::fs::read_to_string(out.join(name)).expect("generated file readable");
        let records = text
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("one json record"))
            .filter(|record| record["kind"] == "frame")
            .collect();
        files.push((name.to_string(), records));
    }
    files
}

#[test]
fn generated_stream_never_contains_probe_text() {
    let out = tempfile::tempdir().expect("tempdir");
    generate(
        &support::templates_dir(),
        out.path(),
        &support::test_budgets(),
    )
    .expect("generate from the template frames");

    let mut deltas_seen = 0usize;
    for (name, records) in generated_records(out.path()) {
        let mut deltas_in_file = 0usize;
        for record in records {
            let params = &record["frame"]["params"];
            if params["type"] != "message.delta" {
                continue;
            }
            deltas_in_file += 1;
            let text = params["payload"]["text"]
                .as_str()
                .expect("a delta carries text");
            assert!(!text.contains("kbdprobe"), "{name}: probe text in {text:?}");
            assert!(!text.contains('^'), "{name}: caret in {text:?}");
            assert!(
                !text.chars().any(|c| c.is_control() && c != '\n'),
                "{name}: control character in {text:?}"
            );
        }
        if name != "empty.jsonl" {
            assert!(deltas_in_file > 0, "{name} holds no deltas to check");
        }
        deltas_seen += deltas_in_file;
    }
    assert!(deltas_seen > 0);
}
