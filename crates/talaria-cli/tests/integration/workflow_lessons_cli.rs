use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn talaria(args: &[&str], home: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("TALARIA_CONFIG_DIR", home.join("cfg"))
        .env("HERMES_HOME", home.join("no-hermes"))
        .output()
        .unwrap()
}
fn out(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}
fn err(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[test]
fn no_notes_file_prints_an_empty_list() {
    let t = tempfile::tempdir().unwrap();
    let text = talaria(&["workflow", "lessons"], t.path());
    assert!(text.status.success());
    assert_eq!(out(&text), "No lessons recorded.\n");
    let json = talaria(&["workflow", "lessons", "--json"], t.path());
    let body: Value = serde_json::from_str(&out(&json)).unwrap();
    assert_eq!(body["entries"].as_array().unwrap().len(), 0);
    assert!(!t.path().join("cfg").exists(), "reading creates nothing");
}

#[test]
fn an_added_lesson_is_dated_names_its_run_and_reads_back() {
    let t = tempfile::tempdir().unwrap();
    let add = talaria(
        &[
            "workflow",
            "lessons",
            "add",
            "--run",
            "01RUNID",
            "Name the status words in the prompt.",
        ],
        t.path(),
    );
    assert!(add.status.success(), "{}", err(&add));
    assert!(
        out(&add).starts_with("recorded: "),
        "the agent is told exactly what was recorded: {}",
        out(&add)
    );
    let file = t.path().join("cfg").join("lessons.txt");
    let stored = fs::read_to_string(&file).unwrap();
    let (date, rest) = stored.split_once(" run 01RUNID: ").unwrap();
    assert_eq!(date.len(), 10, "{stored}");
    assert!(date.starts_with("20") && date.as_bytes()[4] == b'-', "{stored}");
    assert_eq!(rest, "Name the status words in the prompt.\n");
    assert_eq!(out(&add), format!("recorded: {}", stored));

    let text = talaria(&["workflow", "lessons"], t.path());
    assert!(out(&text).contains("Unreviewed notes from earlier agents, not rules"));
    assert!(out(&text).contains("run 01RUNID: Name the status words in the prompt."));
    let json = talaria(&["workflow", "lessons", "--json"], t.path());
    let body: Value = serde_json::from_str(&out(&json)).unwrap();
    assert_eq!(body["entries"][0]["run"], "01RUNID");
    assert_eq!(body["entries"][0]["text"], "Name the status words in the prompt.");
    assert_eq!(body["entries"][0]["date"], date);
}

#[test]
fn several_unquoted_words_make_one_entry_and_a_second_add_appends() {
    let t = tempfile::tempdir().unwrap();
    for words in [["first", "lesson"], ["second", "lesson"]] {
        let mut args = vec!["workflow", "lessons", "add", "--run", "r1"];
        args.extend(words);
        assert!(talaria(&args, t.path()).status.success());
    }
    let stored = fs::read_to_string(t.path().join("cfg/lessons.txt")).unwrap();
    let lines: Vec<_> = stored.lines().collect();
    assert_eq!(lines.len(), 2, "{stored}");
    assert!(lines[0].ends_with("run r1: first lesson"), "{stored}");
    assert!(lines[1].ends_with("run r1: second lesson"), "{stored}");
}

#[test]
fn empty_overlong_and_unnamed_lessons_are_refused_and_write_nothing() {
    let t = tempfile::tempdir().unwrap();
    let file = t.path().join("cfg/lessons.txt");
    let empty = talaria(&["workflow", "lessons", "add", "--run", "r1", "  "], t.path());
    assert_eq!(empty.status.code(), Some(2), "{}", err(&empty));
    assert!(err(&empty).contains("empty"), "{}", err(&empty));
    let long = "x".repeat(1001);
    let over = talaria(&["workflow", "lessons", "add", "--run", "r1", &long], t.path());
    assert_eq!(over.status.code(), Some(2));
    assert!(err(&over).contains("limit is 1000"), "{}", err(&over));
    let no_run = talaria(&["workflow", "lessons", "add", "words"], t.path());
    assert_eq!(no_run.status.code(), Some(2));
    let bad_run = talaria(&["workflow", "lessons", "add", "--run", "a b", "words"], t.path());
    assert_eq!(bad_run.status.code(), Some(2));
    assert!(!file.exists());
}

#[cfg(unix)]
#[test]
fn the_notes_file_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    assert!(
        talaria(&["workflow", "lessons", "add", "--run", "r1", "x"], t.path())
            .status
            .success()
    );
    let mode = fs::metadata(t.path().join("cfg/lessons.txt"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn escape_sequences_in_a_stored_line_do_not_reach_the_terminal() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir_all(t.path().join("cfg")).unwrap();
    fs::write(t.path().join("cfg/lessons.txt"), "hand \u{1b}[31mred\u{1b}[0m note\n").unwrap();
    let text = talaria(&["workflow", "lessons"], t.path());
    assert!(!out(&text).contains('\u{1b}'), "{:?}", out(&text));
}
