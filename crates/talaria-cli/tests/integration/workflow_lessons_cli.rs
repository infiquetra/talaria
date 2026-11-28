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
    assert!(
        date.starts_with("20") && date.as_bytes()[4] == b'-',
        "{stored}"
    );
    assert_eq!(rest, "Name the status words in the prompt.\n");
    assert_eq!(out(&add), format!("recorded: {}", stored));

    let text = talaria(&["workflow", "lessons"], t.path());
    assert!(out(&text).contains("Unreviewed notes from earlier agents, not rules"));
    assert!(out(&text).contains("run 01RUNID: Name the status words in the prompt."));
    let json = talaria(&["workflow", "lessons", "--json"], t.path());
    let body: Value = serde_json::from_str(&out(&json)).unwrap();
    assert_eq!(body["entries"][0]["run"], "01RUNID");
    assert_eq!(
        body["entries"][0]["text"],
        "Name the status words in the prompt."
    );
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
    let empty = talaria(
        &["workflow", "lessons", "add", "--run", "r1", "  "],
        t.path(),
    );
    assert_eq!(empty.status.code(), Some(2), "{}", err(&empty));
    assert!(err(&empty).contains("empty"), "{}", err(&empty));
    let long = "x".repeat(1001);
    let over = talaria(
        &["workflow", "lessons", "add", "--run", "r1", &long],
        t.path(),
    );
    assert_eq!(over.status.code(), Some(2));
    assert!(err(&over).contains("limit is 1000"), "{}", err(&over));
    let no_run = talaria(&["workflow", "lessons", "add", "words"], t.path());
    assert_eq!(no_run.status.code(), Some(2));
    let bad_run = talaria(
        &["workflow", "lessons", "add", "--run", "a b", "words"],
        t.path(),
    );
    assert_eq!(bad_run.status.code(), Some(2));
    assert!(!file.exists());
}

#[cfg(unix)]
#[test]
fn the_notes_file_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    assert!(
        talaria(
            &["workflow", "lessons", "add", "--run", "r1", "x"],
            t.path()
        )
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
    fs::write(
        t.path().join("cfg/lessons.txt"),
        "hand \u{1b}[31mred\u{1b}[0m note\n",
    )
    .unwrap();
    let text = talaria(&["workflow", "lessons"], t.path());
    assert!(!out(&text).contains('\u{1b}'), "{:?}", out(&text));
}

#[test]
fn lesson_text_may_name_flags() {
    let t = tempfile::tempdir().unwrap();
    for args in [
        &["--run", "r1", "pass", "--cwd", "to", "check", "too"][..],
        &["--run", "r1", "--budget-usd", "needs", "a", "yes"][..],
        &["--run", "r1", "the", "--run", "flag", "names", "a", "run"][..],
        &["--run", "r1", "--", "-x", "is", "not", "a", "flag"][..],
    ] {
        let mut full = vec!["workflow", "lessons", "add"];
        full.extend(args);
        let add = talaria(&full, t.path());
        assert!(add.status.success(), "{args:?}: {}", err(&add));
    }
    let stored = fs::read_to_string(t.path().join("cfg/lessons.txt")).unwrap();
    let texts: Vec<_> = stored
        .lines()
        .map(|l| l.split_once(": ").unwrap().1)
        .collect();
    assert_eq!(
        texts,
        [
            "pass --cwd to check too",
            "--budget-usd needs a yes",
            "the --run flag names a run",
            "-x is not a flag"
        ]
    );
}

#[test]
fn a_long_notes_file_shows_the_newest_fifty_and_counts_the_rest() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir_all(t.path().join("cfg")).unwrap();
    let text: String = (1..=53)
        .map(|n| format!("2026-11-23 run r{n}: lesson {n}\n"))
        .collect();
    fs::write(t.path().join("cfg/lessons.txt"), text).unwrap();
    let shown = out(&talaria(&["workflow", "lessons"], t.path()));
    let lines: Vec<_> = shown.lines().collect();
    assert_eq!(lines.len(), 52, "{shown}");
    assert_eq!(
        lines[1],
        "3 older entries are not shown; `workflow lessons --all` prints every entry."
    );
    assert!(lines[2].ends_with("lesson 4"), "{shown}");
    assert!(lines[51].ends_with("lesson 53"), "{shown}");
    let every = out(&talaria(&["workflow", "lessons", "--all"], t.path()));
    assert_eq!(every.lines().count(), 54, "{every}");
    assert!(!every.contains("older"), "{every}");
    let json: Value =
        serde_json::from_str(&out(&talaria(&["workflow", "lessons", "--json"], t.path()))).unwrap();
    assert_eq!(json["entries"].as_array().unwrap().len(), 50);
    assert_eq!(json["older_entries"], 3);
    assert_eq!(json["entries"][0]["run"], "r4");
    let json: Value = serde_json::from_str(&out(&talaria(
        &["workflow", "lessons", "--json", "--all"],
        t.path(),
    )))
    .unwrap();
    assert_eq!(json["entries"].as_array().unwrap().len(), 53);
    assert_eq!(json["older_entries"], 0);
}

#[test]
fn a_listing_of_long_entries_fits_what_hermes_shows_whole() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir_all(t.path().join("cfg")).unwrap();
    // Fifty entries of the longest kind come to more than Hermes shows of one command's output.
    let long = "x".repeat(1000);
    let text: String = (1..=50)
        .map(|n| format!("2026-11-23 run r{n}: {long}\n"))
        .collect();
    fs::write(t.path().join("cfg/lessons.txt"), text).unwrap();
    let shown = out(&talaria(&["workflow", "lessons"], t.path()));
    assert!(shown.len() < 32_000, "{}", shown.len());
    let lines: Vec<_> = shown.lines().collect();
    let printed = lines.len() - 2;
    assert!(
        lines[1].starts_with(&format!("{} older entries are not shown", 50 - printed)),
        "{}",
        lines[1]
    );
    assert!(lines.last().unwrap().starts_with("2026-11-23 run r50: "));
    let json: Value =
        serde_json::from_str(&out(&talaria(&["workflow", "lessons", "--json"], t.path()))).unwrap();
    assert_eq!(json["entries"].as_array().unwrap().len(), printed);
    assert_eq!(json["older_entries"], 50 - printed);
    let every = out(&talaria(&["workflow", "lessons", "--all"], t.path()));
    assert_eq!(every.lines().count(), 51);
}

#[test]
fn a_relative_configuration_folder_is_refused_and_writes_nothing() {
    let t = tempfile::tempdir().unwrap();
    for config in [None, Some("relative-cfg")] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_talaria"));
        command
            .args(["workflow", "lessons", "add", "--run", "r1", "x"])
            .env_clear()
            .env("HERMES_HOME", t.path().join("no-hermes"))
            .current_dir(t.path());
        if let Some(config) = config {
            command.env("TALARIA_CONFIG_DIR", config);
        }
        let add = command.output().unwrap();
        assert_eq!(add.status.code(), Some(1), "{config:?}: {}", err(&add));
        assert!(err(&add).contains("relative path"), "{}", err(&add));
        assert!(err(&add).contains("nothing was written"), "{}", err(&add));
    }
    assert!(!t.path().join(".talaria").exists());
    assert!(!t.path().join("relative-cfg").exists());
}
