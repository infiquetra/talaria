//! The replay files the harness generates from the recorded templates: determinism, shapes,
//! numbering, pace, size, and the failure when a template frame is missing.

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use hermes_client::replay_source::ReplaySource;
use serde_json::Value;
use talaria_bench::{BenchError, Budgets, generate};

const FILE_NAMES: [&str; 3] = [
    "empty.jsonl",
    "reference-stream.jsonl",
    "long-session.jsonl",
];
const EVENT_TYPES: [&str; 5] = [
    "message.start",
    "message.delta",
    "message.complete",
    "tool.start",
    "tool.complete",
];
const BATCH_MS: u64 = 33;
/// The repeating delta length cycle, in characters.
const CUT_CYCLE: [usize; 5] = [3, 5, 7, 4, 6];
/// Envelope fields stamped on every generated event, copied from the first template event.
const STAMPED_KEYS: [&str; 4] = [
    "session_id",
    "replay_epoch",
    "authority_epoch",
    "admission_id",
];

struct Log {
    header: String,
    records: Vec<Value>,
}

fn read_log(path: &Path) -> Log {
    let name = path.file_name().expect("file name").to_string_lossy();
    let text = fs::read_to_string(path).unwrap_or_else(|error| panic!("read {name}: {error}"));
    let mut lines = text.lines();
    let header = lines.next().unwrap_or_else(|| panic!("{name} is empty"));
    let records = lines
        .map(|line| {
            serde_json::from_str(line).unwrap_or_else(|error| panic!("{name}: bad record: {error}"))
        })
        .collect();
    Log {
        header: header.to_string(),
        records,
    }
}

fn template_log(file: &str) -> Log {
    read_log(&support::templates_dir().join(file))
}

fn shipped_budgets() -> Budgets {
    Budgets::load(&support::shipped_budgets_path()).expect("shipped budgets load")
}

fn generate_into(budgets: &Budgets) -> tempfile::TempDir {
    let out = tempfile::tempdir().expect("temp dir");
    generate(&support::templates_dir(), out.path(), budgets)
        .expect("generate writes the three files");
    out
}

fn keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("expected a JSON object, got {value}"))
        .keys()
        .cloned()
        .collect()
}

fn event_type(record: &Value) -> Option<&str> {
    if record["frame"]["method"] == "event" {
        record["frame"]["params"]["type"].as_str()
    } else {
        None
    }
}

fn params(record: &Value) -> &Value {
    &record["frame"]["params"]
}

fn payload_text(record: &Value) -> &str {
    record["frame"]["params"]["payload"]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("event has no payload text: {record}"))
}

fn events(log: &Log) -> Vec<&Value> {
    log.records
        .iter()
        .filter(|record| event_type(record).is_some())
        .collect()
}

/// Splits events into turns; each turn starts at a `message.start`.
fn turns<'a>(events: &[&'a Value]) -> Vec<Vec<&'a Value>> {
    let mut turns: Vec<Vec<&Value>> = Vec::new();
    for event in events {
        if event_type(event) == Some("message.start") {
            turns.push(Vec::new());
        }
        turns
            .last_mut()
            .expect("the first event of a file is a message.start")
            .push(event);
    }
    turns
}

/// The first template event of each type, with the log it came from.
fn template_events() -> BTreeMap<&'static str, Value> {
    let plain = template_log("plain-answer.jsonl");
    let tool = template_log("tool-diff.jsonl");
    let mut found = BTreeMap::new();
    for (log, wanted) in [
        (
            &plain,
            ["message.start", "message.delta", "message.complete"].as_slice(),
        ),
        (&tool, ["tool.start", "tool.complete"].as_slice()),
    ] {
        for kind in wanted {
            let record = log
                .records
                .iter()
                .find(|record| event_type(record) == Some(kind))
                .unwrap_or_else(|| panic!("template has no {kind} event"));
            found.insert(*kind, record.clone());
        }
    }
    found
}

#[test]
fn files_are_deterministic_and_open_with_replay_source() {
    let budgets = support::test_budgets();
    let first = generate_into(&budgets);
    let second = generate_into(&budgets);
    let template_header = template_log("plain-answer.jsonl").header;

    for name in FILE_NAMES {
        let one = fs::read(first.path().join(name)).unwrap_or_else(|_| panic!("{name} missing"));
        let two = fs::read(second.path().join(name)).unwrap_or_else(|_| panic!("{name} missing"));
        assert!(!one.is_empty(), "{name} is empty");
        assert_eq!(one, two, "{name} differs between two runs");

        let log = read_log(&first.path().join(name));
        assert_eq!(
            log.header, template_header,
            "{name}: the header is not the template's first line"
        );

        let source = ReplaySource::open(&first.path().join(name))
            .unwrap_or_else(|error| panic!("ReplaySource rejects {name}: {error}"));
        assert_eq!(source.header().version, 3, "{name}: header version");
        assert!(!source.truncated(), "{name} ends without a newline");
        let inbound = source.inbound().count();
        if name == "empty.jsonl" {
            assert_eq!(
                log.records.len(),
                0,
                "empty.jsonl holds more than the header"
            );
            assert_eq!(inbound, 0, "empty.jsonl yields frames");
            assert_eq!(source.outbound().count(), 0, "empty.jsonl yields frames");
        } else {
            assert!(inbound > 0, "{name} yields no inbound frames");
        }
    }
}

#[test]
fn shapes_match_templates_and_seq_is_renumbered() {
    let out = generate_into(&support::test_budgets());
    let templates = template_events();
    let first_template = template_log("plain-answer.jsonl");
    let first_event = events(&first_template)[0];
    let template_stamps = params(first_event).clone();
    let template_generation = template_stamps["execution_generation"]
        .as_u64()
        .expect("template execution_generation");

    for name in FILE_NAMES {
        let log = read_log(&out.path().join(name));

        // The record's own sequence number is gapless from 1.
        for (index, record) in log.records.iter().enumerate() {
            assert_eq!(
                record["seq"].as_u64(),
                Some(index as u64 + 1),
                "{name}: record {index} has the wrong seq"
            );
        }

        let all = events(&log);
        if name == "empty.jsonl" {
            assert!(all.is_empty(), "empty.jsonl holds events");
            continue;
        }
        assert!(!all.is_empty(), "{name} holds no events");

        for (index, event) in all.iter().enumerate() {
            let kind = event_type(event).expect("event type");
            assert!(
                EVENT_TYPES.contains(&kind),
                "{name}: unexpected event {kind}"
            );
            let template = &templates[kind];

            // Key sets equal the template's at every level.
            assert_eq!(keys(event), keys(template), "{name}: record keys of {kind}");
            assert_eq!(
                keys(&event["frame"]),
                keys(&template["frame"]),
                "{name}: frame keys of {kind}"
            );
            assert_eq!(
                keys(params(event)),
                keys(params(template)),
                "{name}: params keys of {kind}"
            );
            assert_eq!(
                keys(&params(event)["payload"]),
                keys(&params(template)["payload"]),
                "{name}: payload keys of {kind}"
            );
            assert_eq!(event["dir"], template["dir"], "{name}: direction of {kind}");

            // The gateway's per-session counter runs from 1 across the whole file.
            assert_eq!(
                params(event)["seq"].as_u64(),
                Some(index as u64 + 1),
                "{name}: params.seq of event {index}"
            );

            // One session id, replay epoch, authority epoch and admission id per file.
            for key in STAMPED_KEYS {
                if template_stamps.get(key).is_some() {
                    assert_eq!(
                        params(event)[key],
                        template_stamps[key],
                        "{name}: {key} of event {index}"
                    );
                }
            }
        }

        // The execution generation rises by one per turn.
        for (turn_index, turn) in turns(&all).iter().enumerate() {
            let expected = template_generation + turn_index as u64;
            for event in turn {
                assert_eq!(
                    params(event)["execution_generation"].as_u64(),
                    Some(expected),
                    "{name}: execution_generation in turn {turn_index}"
                );
            }
        }

        // Tool ids are unique, and a completion carries the id of its start.
        let mut seen = BTreeSet::new();
        let mut open: Option<String> = None;
        for event in &all {
            let id = params(event)["payload"]["tool_id"]
                .as_str()
                .map(str::to_owned);
            match event_type(event) {
                Some("tool.start") => {
                    let id = id.expect("tool.start has a tool_id");
                    assert!(seen.insert(id.clone()), "{name}: tool id {id} repeats");
                    open = Some(id);
                }
                Some("tool.complete") => {
                    assert_eq!(
                        id,
                        open.take(),
                        "{name}: tool.complete id differs from its start"
                    );
                }
                _ => {}
            }
        }
        if name == "long-session.jsonl" {
            assert_eq!(
                seen.len() as u64,
                support::test_budgets().scenario.long_session_turns,
                "one fresh tool id per turn"
            );
        }
    }
}

/// One turn of the reference stream, taken apart.
struct ReferenceTurn {
    text: String,
    batches: Vec<(u64, usize)>,
    delta_lengths: Vec<usize>,
}

fn reference_turns(log: &Log) -> Vec<ReferenceTurn> {
    let all = events(log);
    let mut last_t = 0;
    for record in &log.records {
        let t = record["tMs"].as_u64().expect("tMs");
        assert!(t >= last_t, "tMs goes backwards in the reference stream");
        last_t = t;
    }
    turns(&all)
        .iter()
        .map(|turn| {
            assert_eq!(event_type(turn[0]), Some("message.start"));
            let last = turn.last().expect("turn has events");
            assert_eq!(
                event_type(last),
                Some("message.complete"),
                "turn ends with complete"
            );
            assert_eq!(
                last["frame"]["params"]["payload"]["status"], "complete",
                "message.complete status"
            );
            let deltas = &turn[1..turn.len() - 1];
            assert!(!deltas.is_empty(), "turn has no deltas");
            let mut batches: Vec<(u64, usize)> = Vec::new();
            let mut text = String::new();
            let mut delta_lengths = Vec::new();
            for delta in deltas {
                assert_eq!(
                    event_type(delta),
                    Some("message.delta"),
                    "only deltas inside a turn"
                );
                let t = delta["tMs"].as_u64().expect("tMs");
                match batches.last_mut() {
                    Some((last_t, count)) if *last_t == t => *count += 1,
                    _ => batches.push((t, 1)),
                }
                let piece = payload_text(delta);
                delta_lengths.push(piece.chars().count());
                text.push_str(piece);
            }
            assert_eq!(
                payload_text(last),
                text,
                "message.complete text differs from the joined deltas"
            );
            ReferenceTurn {
                text,
                batches,
                delta_lengths,
            }
        })
        .collect()
}

/// Pace, cutting and fence rules that hold at any `turn_seconds`.
fn check_pace_and_cutting(turn: &ReferenceTurn, turn_seconds: u64, label: &str) {
    let batches = &turn.batches;
    for (index, (_, count)) in batches.iter().enumerate() {
        if index + 1 < batches.len() {
            let expected = if index % 2 == 0 { 1 } else { 2 };
            assert_eq!(*count, expected, "{label}: frames in batch {index}");
        } else {
            assert!((1..=2).contains(count), "{label}: frames in the last batch");
        }
    }
    for pair in batches.windows(2) {
        assert_eq!(
            pair[1].0 - pair[0].0,
            BATCH_MS,
            "{label}: batches are not 33 ms apart"
        );
    }
    let duration_ms = batches.len() as u64 * BATCH_MS;
    let target_ms = turn_seconds * 1000;
    assert!(
        duration_ms * 100 >= target_ms * 95 && duration_ms * 100 <= target_ms * 105,
        "{label}: the turn lasts {duration_ms} ms, not {target_ms} ms within 5%"
    );

    let (last, rest) = turn.delta_lengths.split_last().expect("deltas");
    for (index, length) in rest.iter().enumerate() {
        assert_eq!(
            *length,
            CUT_CYCLE[index % CUT_CYCLE.len()],
            "{label}: delta {index} length"
        );
    }
    assert!(*last >= 1, "{label}: the last delta is empty");
}

/// Returns the number of lines inside each fenced block, and the number of fence lines.
fn fences(text: &str) -> (Vec<usize>, usize) {
    let mut inside: Option<usize> = None;
    let mut blocks = Vec::new();
    let mut markers = 0;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            markers += 1;
            match inside.take() {
                Some(count) => blocks.push(count),
                None => inside = Some(0),
            }
        } else if let Some(count) = inside.as_mut() {
            *count += 1;
        }
    }
    assert!(inside.is_none(), "a fenced block is cut off");
    (blocks, markers)
}

fn fence_lines_for(turn_seconds: u64) -> usize {
    (1.1 * turn_seconds as f64).round() as usize
}

#[test]
fn reference_stream_pace_and_long_session_size() {
    let budgets = shipped_budgets();
    assert_eq!(budgets.scenario.turn_seconds, 30, "shipped turn_seconds");
    assert_eq!(
        budgets.scenario.long_session_turns, 1000,
        "shipped long_session_turns"
    );
    let out = generate_into(&budgets);

    // Reference stream: two turns at the recorded pace.
    let reference = read_log(&out.path().join("reference-stream.jsonl"));
    let ref_turns = reference_turns(&reference);
    assert_eq!(
        ref_turns.len(),
        2,
        "the reference stream has two assistant turns"
    );
    for (index, turn) in ref_turns.iter().enumerate() {
        let label = format!("reference turn {}", index + 1);
        check_pace_and_cutting(turn, budgets.scenario.turn_seconds, &label);
        assert!(
            (5_000..=9_000).contains(&turn.text.len()),
            "{label}: text is {} bytes",
            turn.text.len()
        );
        let first = turn.text.lines().next().unwrap_or_default();
        assert_eq!(
            first,
            format!("## Reference turn {}", index + 1),
            "{label}: first line"
        );
        let last = turn
            .text
            .trim_end_matches('\n')
            .lines()
            .last()
            .unwrap_or_default();
        assert_eq!(
            last,
            format!("End of reference turn {}.", index + 1),
            "{label}: last line"
        );
    }

    // Turn 1 is line heavy. Blank lines are counted or not; either reading must fall in range.
    let all_lines = ref_turns[0].text.trim_end_matches('\n').lines().count();
    let filled_lines = ref_turns[0]
        .text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    assert!(
        (100..=140).contains(&all_lines) || (100..=140).contains(&filled_lines),
        "turn 1 has {all_lines} lines ({filled_lines} not blank), expected 100 to 140"
    );

    // Turn 2 holds exactly three whole fenced blocks.
    let (blocks, markers) = fences(&ref_turns[1].text);
    assert_eq!(blocks.len(), 3, "turn 2 fenced blocks");
    assert_eq!(markers, 6, "turn 2 fence lines");
    for (index, lines) in blocks.iter().enumerate() {
        assert_eq!(
            *lines,
            fence_lines_for(budgets.scenario.turn_seconds),
            "turn 2: lines in fenced block {index}"
        );
    }

    // Long session: one message and one tool call per turn, all at one instant.
    let long = read_log(&out.path().join("long-session.jsonl"));
    let all = events(&long);
    let long_turns = turns(&all);
    assert_eq!(long_turns.len(), 1000, "message.start count");
    let counts = |kind: &str| all.iter().filter(|e| event_type(e) == Some(kind)).count();
    assert_eq!(counts("message.start"), 1000);
    assert_eq!(counts("tool.start"), 1000);
    assert_eq!(counts("message.delta"), 3000);
    assert_eq!(
        counts("message.complete") + counts("tool.start"),
        2000,
        "transcript cells"
    );
    assert_eq!(all.len(), 7000, "events in the long session");
    let template_diff =
        template_events()["tool.complete"]["frame"]["params"]["payload"]["inline_diff"].clone();
    for (index, turn) in long_turns.iter().enumerate() {
        let kinds: Vec<&str> = turn.iter().map(|e| event_type(e).expect("type")).collect();
        assert_eq!(
            kinds,
            [
                "message.start",
                "message.delta",
                "message.delta",
                "message.delta",
                "message.complete",
                "tool.start",
                "tool.complete"
            ],
            "long session turn {index}: event order"
        );
        let mut joined = String::new();
        for delta in &turn[1..4] {
            let piece = payload_text(delta);
            let length = piece.chars().count();
            assert!(
                (60..=100).contains(&length),
                "turn {index}: delta of {length} characters"
            );
            joined.push_str(piece);
        }
        assert_eq!(
            payload_text(turn[4]),
            joined,
            "turn {index}: complete text vs deltas"
        );
        assert_eq!(
            turn[6]["frame"]["params"]["payload"]["inline_diff"], template_diff,
            "turn {index}: the recorded inline_diff"
        );
    }
    for record in long.records.iter().skip(1) {
        assert_eq!(
            record["tMs"].as_u64(),
            Some(0),
            "long session is an instant file"
        );
    }
}

#[test]
fn short_turn_keeps_three_fences_and_pace() {
    let budgets = support::test_budgets();
    assert_eq!(budgets.scenario.turn_seconds, 6, "test turn_seconds");
    let out = generate_into(&budgets);
    let reference = read_log(&out.path().join("reference-stream.jsonl"));
    let ref_turns = reference_turns(&reference);
    assert_eq!(
        ref_turns.len(),
        2,
        "the reference stream has two assistant turns"
    );
    for (index, turn) in ref_turns.iter().enumerate() {
        check_pace_and_cutting(
            turn,
            budgets.scenario.turn_seconds,
            &format!("short turn {index}"),
        );
    }
    let (blocks, markers) = fences(&ref_turns[1].text);
    assert_eq!(markers, 6, "short turn 2 fence lines");
    assert_eq!(
        blocks,
        vec![fence_lines_for(budgets.scenario.turn_seconds); 3],
        "short turn 2 fenced block sizes"
    );
}

/// Copies a template without the frames of one event type, renumbering the records.
fn write_template_without(source: &Path, target: &Path, dropped: &str) {
    let log = read_log(source);
    let mut text = format!("{}\n", log.header);
    let kept = log
        .records
        .iter()
        .filter(|r| event_type(r) != Some(dropped));
    for (index, record) in kept.enumerate() {
        let mut record = record.clone();
        record["seq"] = Value::from(index as u64 + 1);
        text.push_str(&record.to_string());
        text.push('\n');
    }
    fs::write(target, text).expect("write trimmed template");
}

#[test]
fn missing_template_exits_2_naming_it() {
    let budgets = support::test_budgets();
    let home_of = |kind: &str| {
        if kind.starts_with("message.") {
            "plain-answer.jsonl"
        } else {
            "tool-diff.jsonl"
        }
    };

    // An empty fixtures directory: the error names a template file and a frame type in it.
    let empty = tempfile::tempdir().expect("temp dir");
    let out = tempfile::tempdir().expect("temp dir");
    let error = generate(empty.path(), out.path(), &budgets).expect_err("no templates");
    let BenchError::TemplateMissing { file, frame_type } = &error else {
        panic!("expected TemplateMissing, got {error:?}");
    };
    assert!(
        EVENT_TYPES.contains(&frame_type.as_str()),
        "frame type named: {frame_type}"
    );
    assert!(
        file.contains(home_of(frame_type)),
        "file {file} does not hold {frame_type}"
    );
    let line = error.to_string();
    assert!(
        !line.trim().contains('\n'),
        "the message is more than one line: {line}"
    );
    assert!(
        line.contains(file.as_str()) && line.contains(frame_type.as_str()),
        "{line}"
    );

    // One frame type removed from an otherwise complete set: that exact frame is named.
    let partial = tempfile::tempdir().expect("temp dir");
    write_template_without(
        &support::templates_dir().join("plain-answer.jsonl"),
        &partial.path().join("plain-answer.jsonl"),
        "message.delta",
    );
    fs::copy(
        support::templates_dir().join("tool-diff.jsonl"),
        partial.path().join("tool-diff.jsonl"),
    )
    .expect("copy tool template");
    let error = generate(partial.path(), out.path(), &budgets).expect_err("no message.delta");
    let BenchError::TemplateMissing { file, frame_type } = &error else {
        panic!("expected TemplateMissing, got {error:?}");
    };
    assert_eq!(frame_type, "message.delta");
    assert!(file.contains("plain-answer.jsonl"), "file named: {file}");

    // The command: exit status 2 and one plain line naming the template.
    let output = Command::new(support::bench_bin_path())
        .current_dir(support::workspace_root())
        .arg("generate")
        .arg("--fixtures")
        .arg(empty.path())
        .arg("--out")
        .arg(out.path())
        .output()
        .expect("run talaria-bench");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "generate with no templates; stdout: {stdout}; stderr: {stderr}"
    );
    let text = format!("{stdout}{stderr}");
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected one line, got: {text}");
    let line = lines[0];
    assert!(
        line.contains("template frame") && line.contains("not found in"),
        "{line}"
    );
    assert!(
        EVENT_TYPES
            .iter()
            .any(|kind| line.contains(kind) && line.contains(home_of(kind))),
        "the line names no template file and frame type: {line}"
    );
}
