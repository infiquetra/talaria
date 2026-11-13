//! The prompt history store: file format, loading, and compaction.
//!
//! Behavior contract: issue 228.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde_json::Value;
use talaria_app::comforts::prompt_history::{PromptHistory, compact_failed_notice};

fn require_non_root() {
    if rustix::process::geteuid().is_root() {
        panic!("run as a non-root user: this test forces a write failure with folder permissions");
    }
}

fn history_file(dir: &Path) -> std::path::PathBuf {
    dir.join("history.jsonl")
}

fn line(folder: &str, text: &str) -> String {
    serde_json::json!({"v": 1, "ts": 1792659315, "folder": folder, "text": text}).to_string()
}

#[test]
fn append_writes_one_private_json_line() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("talaria-home");
    let (mut store, notice) = PromptHistory::open(&dir, true, "workspace-a");
    assert_eq!(notice, None);

    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let notice = store.append("fix the failing test");
    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert_eq!(notice, None);

    let file = history_file(&dir);
    let text = std::fs::read_to_string(&file).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1);
    let record: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(record["v"], Value::from(1));
    assert_eq!(record["folder"], Value::from("workspace-a"));
    assert_eq!(record["text"], Value::from("fix the failing test"));
    let ts = record["ts"].as_i64().expect("a unix timestamp");
    assert!((before..=after).contains(&ts), "ts {ts} is current");

    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn leading_space_and_oversize_are_not_saved() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("talaria-home");
    let (mut store, _) = PromptHistory::open(&dir, true, "workspace-a");
    store.append("first saved prompt");
    let file = history_file(&dir);
    let before = std::fs::read(&file).unwrap();

    let big = "x".repeat(100_001);
    assert_eq!(store.append(" secret thing"), None);
    assert_eq!(store.append(&big), None);

    let after = std::fs::read(&file).unwrap();
    assert_eq!(before, after, "the file is byte-for-byte unchanged");

    let texts: Vec<String> = store
        .snapshot()
        .iter()
        .map(|entry| entry.text.clone())
        .collect();
    assert!(texts.contains(&" secret thing".to_owned()));
    assert!(texts.contains(&big));
}

#[test]
fn load_reads_last_mebibyte_and_skips_bad_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("talaria-home");
    std::fs::create_dir_all(&dir).unwrap();
    // About 3 MiB of short lines, a broken line in the last MiB, and content that
    // straddles the 1 MiB boundary so the first partial line must be dropped.
    let mut body = String::new();
    body.push_str(&line("workspace-a", "first-line-marker"));
    body.push('\n');
    for index in 0..50_000 {
        body.push_str(&line("workspace-a", &format!("prompt-{index:05}")));
        body.push('\n');
        if index == 39_000 {
            body.push_str("this line is not json {\n");
        }
    }
    assert!(body.len() > 3 * 1024 * 1024);
    std::fs::write(history_file(&dir), &body).unwrap();

    let (store, _) = PromptHistory::open(&dir, true, "workspace-a");
    let texts: Vec<String> = store
        .snapshot()
        .iter()
        .map(|entry| entry.text.clone())
        .collect();

    assert!(!texts.contains(&"first-line-marker".to_owned()));
    assert!(!texts.iter().any(|text| text.contains("not json")));
    // Every whole valid line of the last MiB, oldest first.
    let tail = &body.as_bytes()[body.len() - 1024 * 1024..];
    let start = tail.iter().position(|byte| *byte == b'\n').unwrap() + 1;
    let expected: Vec<String> = std::str::from_utf8(&tail[start..])
        .unwrap()
        .lines()
        .filter_map(|text| serde_json::from_str::<Value>(text).ok())
        .filter_map(|record| {
            (record["v"] == 1)
                .then(|| record["text"].as_str().map(str::to_owned))
                .flatten()
        })
        .collect();
    assert!(!expected.is_empty());
    assert_eq!(texts, expected);
}

fn padded_line(index: usize, folder: &str, width: usize) -> String {
    let stem = format!("entry-{index:06}");
    let probe =
        serde_json::json!({"v": 1, "ts": 1792659315, "folder": folder, "text": ""}).to_string();
    let pad = width - 1 - probe.len() - stem.len();
    let text = format!("{stem}{}", "p".repeat(pad));
    let line =
        serde_json::json!({"v": 1, "ts": 1792659315, "folder": folder, "text": text}).to_string();
    assert_eq!(line.len() + 1, width);
    line
}

#[test]
fn compaction_by_size_keeps_newest() {
    require_non_root();
    // Case A: 30,000 lines of 150 bytes keep exactly the newest 5,000.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("talaria-home");
    std::fs::create_dir_all(&dir).unwrap();
    let mut body = String::new();
    for index in 0..30_000 {
        body.push_str(&padded_line(index, "workspace-a", 150));
        body.push('\n');
    }
    std::fs::write(history_file(&dir), &body).unwrap();
    let (store, _) = PromptHistory::open(&dir, true, "workspace-a");
    assert!(store.compact().unwrap());
    let text = std::fs::read_to_string(history_file(&dir)).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 5_000);
    assert!(lines[0].contains("entry-025000"));
    assert!(lines[4_999].contains("entry-029999"));
    let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "no temporary file is left behind");

    // Case B: 10,001 lines of 500 bytes keep the newest lines within 1 MiB.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("talaria-home");
    std::fs::create_dir_all(&dir).unwrap();
    let mut body = String::new();
    for index in 0..10_001 {
        body.push_str(&padded_line(index, "workspace-a", 500));
        body.push('\n');
    }
    std::fs::write(history_file(&dir), &body).unwrap();
    let (store, _) = PromptHistory::open(&dir, true, "workspace-a");
    assert!(store.compact().unwrap());
    let text = std::fs::read_to_string(history_file(&dir)).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2_097);
    assert!(lines[0].contains("entry-007904"));
    assert!(lines[2_096].contains("entry-010000"));

    // Case C: a read-only folder fails the compaction and keeps the original.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("talaria-home");
    std::fs::create_dir_all(&dir).unwrap();
    let mut body = String::new();
    for index in 0..30_000 {
        body.push_str(&padded_line(index, "workspace-a", 150));
        body.push('\n');
    }
    std::fs::write(history_file(&dir), &body).unwrap();
    let mut permissions = std::fs::metadata(&dir).unwrap().permissions();
    permissions.set_mode(0o500);
    std::fs::set_permissions(&dir, permissions).unwrap();
    let (store, _) = PromptHistory::open(&dir, true, "workspace-a");
    assert!(store.compact().is_err(), "the caller turns this into H2");
    assert_eq!(
        compact_failed_notice("permission denied"),
        "Prompt history could not be compacted (permission denied); it will be tried again next start."
    );
    let kept = std::fs::read(history_file(&dir)).unwrap();
    assert_eq!(kept, body.as_bytes());
    let mut permissions = std::fs::metadata(&dir).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&dir, permissions).unwrap();
}

#[test]
fn folder_view_seeds_newest_1000() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("talaria-home");
    std::fs::create_dir_all(&dir).unwrap();
    let mut body = String::new();
    for index in 0..1_500 {
        body.push_str(&line("workspace-this", &format!("this-{index:04}")));
        body.push('\n');
    }
    for index in 0..200 {
        body.push_str(&line("workspace-other", &format!("other-{index:04}")));
        body.push('\n');
    }
    std::fs::write(history_file(&dir), &body).unwrap();

    let (store, _) = PromptHistory::open(&dir, true, "workspace-this");
    let seed = store.composer_seed();
    assert_eq!(seed.len(), 1_000);
    assert_eq!(seed[0], "this-0500");
    assert_eq!(seed[999], "this-1499");
}

#[test]
fn save_false_and_write_failure() {
    require_non_root();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("talaria-home");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        history_file(&dir),
        format!("{}\n", line("workspace-a", "kept")),
    )
    .unwrap();
    let before = std::fs::read(history_file(&dir)).unwrap();

    let (mut store, _) = PromptHistory::open(&dir, false, "workspace-a");
    assert!(store.snapshot().is_empty(), "nothing is read");
    assert_eq!(store.append("unsaved"), None);
    let after = std::fs::read(history_file(&dir)).unwrap();
    assert_eq!(before, after, "nothing is written");

    let dir = tmp.path().join("locked-home");
    std::fs::create_dir_all(&dir).unwrap();
    let mut permissions = std::fs::metadata(&dir).unwrap().permissions();
    permissions.set_mode(0o500);
    std::fs::set_permissions(&dir, permissions).unwrap();
    let (mut store, _) = PromptHistory::open(&dir, true, "workspace-a");
    assert_eq!(
        store.append("cannot land"),
        Some(
            "Prompt history could not be saved (permission denied); history works for this run only."
                .to_owned()
        )
    );
    assert_eq!(store.append("cannot land either"), None, "H1 shows once");
    let mut permissions = std::fs::metadata(&dir).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&dir, permissions).unwrap();
}
