//! `talaria workflow lessons`: a notes file where agents record what a workflow run taught them.
//!
//! Agents do not edit the workflow skill; they record lessons with this command instead. The file
//! sits in Talaria's configuration folder and honours the `TALARIA_CONFIG_DIR` override. Entries
//! are unreviewed notes: whoever reads them treats them as hints, not rules. The developer may edit
//! or prune the file by hand.
//!
//! One entry is one line: `YYYY-MM-DD run <run-id>: <text>`. A line is appended with a single
//! write to a file opened for appending, so two writers add whole lines and never mix their text.
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use clap::{Args, Subcommand};
use serde_json::json;
use talaria_workflow::runner;

use crate::workflow::safe;

/// The longest entry, in characters, after white space has been tidied. Longer text is refused
/// rather than cut, so a lesson is never silently changed.
pub const MAX_ENTRY_CHARS: usize = 1000;
/// How many of the newest entries a plain listing prints. Hermes keeps only the head and tail of
/// a long command output, so an unbounded listing would silently lose its middle entries.
pub const SHOWN_ENTRIES: usize = 50;
/// The longest run id accepted.
const MAX_RUN_ID_CHARS: usize = 64;
const FILE_NAME: &str = "lessons.txt";

/// Show the lessons earlier agents recorded, or record one.
#[derive(Clone, Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct LessonsArgs {
    #[command(subcommand)]
    pub command: Option<LessonsCommand>,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
    /// Print every entry, not only the newest 50.
    #[arg(long)]
    pub all: bool,
}
#[derive(Clone, Debug, Subcommand)]
pub enum LessonsCommand {
    /// Append one dated entry that names the run it was learned from.
    Add(AddArgs),
}
#[derive(Clone, Debug, Args)]
pub struct AddArgs {
    /// The run the lesson was verified from, as `workflow list` shows it.
    #[arg(long, value_name = "RUN")]
    pub run: String,
    /// The lesson, in plain words; at most 1000 characters. Several words may be given unquoted,
    /// and words that start with a hyphen (a flag the lesson is about) are part of the text.
    #[arg(
        value_name = "TEXT",
        required = true,
        num_args = 1..,
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    pub text: Vec<String>,
}

/// Why an entry was refused or could not be written.
#[derive(Debug, PartialEq, Eq)]
pub enum LessonError {
    EmptyText,
    TooLong { chars: usize },
    BadRunId,
    RelativeFolder(PathBuf),
    Io(String),
}
impl std::fmt::Display for LessonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyText => f.write_str("the lesson text is empty"),
            Self::TooLong { chars } => write!(
                f,
                "the lesson is {chars} characters; the limit is {MAX_ENTRY_CHARS}. Shorten it to the one fact you verified"
            ),
            Self::BadRunId => write!(
                f,
                "the run id must be 1 to {MAX_RUN_ID_CHARS} letters, digits, dots, dashes or underscores, as `workflow list` shows it"
            ),
            Self::RelativeFolder(path) => write!(
                f,
                "Talaria's configuration folder resolves to the relative path {}, because neither TALARIA_CONFIG_DIR nor HOME gives an absolute folder; nothing was written. Set TALARIA_CONFIG_DIR or HOME to an absolute folder",
                path.display()
            ),
            Self::Io(message) => f.write_str(message),
        }
    }
}

/// The notes file, in Talaria's configuration folder.
pub fn lessons_path() -> PathBuf {
    runner::runs::configuration_dir().join(FILE_NAME)
}

/// The text on one line: control characters and runs of white space become single spaces.
fn tidy(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The line one entry is stored as, without its final newline.
pub fn entry_line(date: &str, run: &str, text: &str) -> Result<String, LessonError> {
    let run = run.trim();
    if run.is_empty()
        || run.chars().count() > MAX_RUN_ID_CHARS
        || !run
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(LessonError::BadRunId);
    }
    let text = tidy(text);
    if text.is_empty() {
        return Err(LessonError::EmptyText);
    }
    let chars = text.chars().count();
    if chars > MAX_ENTRY_CHARS {
        return Err(LessonError::TooLong { chars });
    }
    Ok(format!("{date} run {run}: {text}"))
}

/// Append one entry to `path`, creating the file with owner-only permissions when it is new.
///
/// A relative `path` is refused: it would put the notes file inside whatever folder the agent
/// runs in, usually a project checkout where it could be committed.
pub fn append_entry(path: &Path, line: &str) -> Result<(), LessonError> {
    if !path.is_absolute() {
        return Err(LessonError::RelativeFolder(
            path.parent().unwrap_or(path).to_owned(),
        ));
    }
    let io_error = |e: io::Error| LessonError::Io(format!("{}: {e}", path.display()));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let mut options = OpenOptions::new();
    options.read(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(io_error)?;
    // A hand edit may have left the last line without its newline; start a fresh line then.
    let length = file.metadata().map_err(io_error)?.len();
    let mut text = String::new();
    if length > 0 {
        let mut last = [0_u8; 1];
        file.seek(SeekFrom::Start(length - 1)).map_err(io_error)?;
        file.read_exact(&mut last).map_err(io_error)?;
        if last[0] != b'\n' {
            text.push('\n');
        }
    }
    text.push_str(line);
    text.push('\n');
    // One write to a file opened for appending: the system adds the whole entry at the end.
    file.write_all(text.as_bytes()).map_err(io_error)?;
    file.flush().map_err(io_error)
}

/// One stored line, split into its parts when it has the usual shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub date: Option<String>,
    pub run: Option<String>,
    pub text: String,
}
fn parse_line(line: &str) -> Entry {
    let parsed = line.split_once(' ').and_then(|(date, rest)| {
        let rest = rest.strip_prefix("run ")?;
        let (run, text) = rest.split_once(": ")?;
        Some(Entry {
            date: Some(date.to_owned()),
            run: Some(run.to_owned()),
            text: text.to_owned(),
        })
    });
    parsed.unwrap_or_else(|| Entry {
        date: None,
        run: None,
        text: line.to_owned(),
    })
}

/// Every non-blank line of the notes file; a missing file has none.
pub fn read_lines(path: &Path) -> Result<Vec<String>, LessonError> {
    match fs::read(path) {
        Ok(bytes) => Ok(String::from_utf8_lossy(&bytes)
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(LessonError::Io(format!("{}: {e}", path.display()))),
    }
}

fn today() -> String {
    let date = time::OffsetDateTime::now_utc().date();
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
}

fn fail(error: impl std::fmt::Display, code: u8) -> u8 {
    eprintln!("{}", safe(&error.to_string()));
    code
}

fn print(text: &str) -> u8 {
    match io::stdout().write_all(text.as_bytes()) {
        Ok(()) => 0,
        Err(e) => fail(e, 1),
    }
}

pub fn execute(args: LessonsArgs) -> u8 {
    let path = lessons_path();
    match args.command {
        Some(LessonsCommand::Add(add)) => add_entry(&path, &add),
        None => show(&path, args.json, args.all),
    }
}

fn add_entry(path: &Path, args: &AddArgs) -> u8 {
    let line = match entry_line(&today(), &args.run, &args.text.join(" ")) {
        Ok(line) => line,
        Err(e) => return fail(e, 2),
    };
    match append_entry(path, &line) {
        Ok(()) => print(&format!("recorded: {}\n", safe(&line))),
        Err(e) => fail(e, 1),
    }
}

/// The entries a listing prints, newest last, and how many older ones it leaves out.
pub fn shown(lines: &[String], all: bool) -> (&[String], usize) {
    let older = if all {
        0
    } else {
        lines.len().saturating_sub(SHOWN_ENTRIES)
    };
    (&lines[older..], older)
}

/// The line that says how many older entries a listing left out.
fn older_note(older: usize) -> String {
    format!(
        "{older} older entr{} not shown; `workflow lessons --all` prints every entry.",
        if older == 1 { "y is" } else { "ies are" }
    )
}

fn show(path: &Path, json_mode: bool, all: bool) -> u8 {
    let lines = match read_lines(path) {
        Ok(lines) => lines,
        Err(e) => return fail(e, 1),
    };
    let (lines, older) = shown(&lines, all);
    if json_mode {
        let entries: Vec<_> = lines
            .iter()
            .map(|line| {
                let entry = parse_line(line);
                json!({"date": entry.date, "run": entry.run, "text": entry.text})
            })
            .collect();
        let body = json!({
            "version": 1,
            "path": path.display().to_string(),
            "note": "Unreviewed notes from earlier agents, not rules.",
            "entries": entries,
            "older_entries": older,
        });
        return print(&format!("{body}\n"));
    }
    if lines.is_empty() {
        return print("No lessons recorded.\n");
    }
    let mut text = String::from("Unreviewed notes from earlier agents, not rules:\n");
    if older > 0 {
        text.push_str(&older_note(older));
        text.push('\n');
    }
    for line in lines {
        text.push_str(&safe(line));
        text.push('\n');
    }
    print(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_is_one_tidy_line_naming_its_run() {
        assert_eq!(
            entry_line(
                "2026-11-23",
                "01ABC",
                "  Name the\nstatus\twords   in the prompt "
            )
            .unwrap(),
            "2026-11-23 run 01ABC: Name the status words in the prompt"
        );
    }

    #[test]
    fn text_cannot_forge_a_second_entry() {
        let line = entry_line("2026-11-23", "r1", "one\n2026-01-01 run r2: forged").unwrap();
        assert!(!line.contains('\n'));
    }

    #[test]
    fn empty_long_and_odd_input_is_refused() {
        assert_eq!(entry_line("d", "r1", " \n\t "), Err(LessonError::EmptyText));
        assert_eq!(entry_line("d", " ", "x"), Err(LessonError::BadRunId));
        assert_eq!(entry_line("d", "a b", "x"), Err(LessonError::BadRunId));
        assert_eq!(entry_line("d", "a:b", "x"), Err(LessonError::BadRunId));
        assert_eq!(
            entry_line("d", &"r".repeat(65), "x"),
            Err(LessonError::BadRunId)
        );
        assert!(entry_line("d", "r", &"x".repeat(MAX_ENTRY_CHARS)).is_ok());
        assert_eq!(
            entry_line("d", "r", &"x".repeat(MAX_ENTRY_CHARS + 1)),
            Err(LessonError::TooLong {
                chars: MAX_ENTRY_CHARS + 1
            })
        );
    }

    #[test]
    fn a_line_parses_back_into_its_parts() {
        assert_eq!(
            parse_line("2026-11-23 run r1: a: b"),
            Entry {
                date: Some("2026-11-23".into()),
                run: Some("r1".into()),
                text: "a: b".into()
            }
        );
        assert_eq!(
            parse_line("hand written").text,
            "hand written",
            "a line of another shape is kept whole"
        );
    }

    #[test]
    fn a_missing_last_newline_does_not_join_two_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("n").join("lessons.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "hand written note").unwrap();
        append_entry(&path, "2026-11-23 run r1: next").unwrap();
        assert_eq!(
            read_lines(&path).unwrap(),
            ["hand written note", "2026-11-23 run r1: next"]
        );
    }

    #[test]
    fn two_writers_never_corrupt_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lessons.txt");
        let handles: Vec<_> = (0..8)
            .map(|writer| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for n in 0..25 {
                        let line = entry_line(
                            "2026-11-23",
                            &format!("w{writer}"),
                            &format!("entry {n} {}", "x".repeat(400)),
                        )
                        .unwrap();
                        append_entry(&path, &line).unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let lines = read_lines(&path).unwrap();
        assert_eq!(lines.len(), 200);
        for line in &lines {
            let entry = parse_line(line);
            assert!(entry.run.is_some_and(|run| run.starts_with('w')), "{line}");
            assert!(entry.text.ends_with(&"x".repeat(400)), "{line}");
        }
    }

    #[test]
    fn a_relative_notes_path_is_refused_and_creates_nothing() {
        let error = append_entry(Path::new(".talaria/lessons.txt"), "2026-11-23 run r1: x");
        assert_eq!(
            error,
            Err(LessonError::RelativeFolder(PathBuf::from(".talaria")))
        );
        assert!(
            error
                .unwrap_err()
                .to_string()
                .contains("nothing was written")
        );
    }

    #[test]
    fn a_listing_shows_the_newest_entries_and_counts_the_rest() {
        let lines: Vec<String> = (0..SHOWN_ENTRIES + 3).map(|n| format!("e{n}")).collect();
        let (kept, older) = shown(&lines, false);
        assert_eq!(older, 3);
        assert_eq!(kept.len(), SHOWN_ENTRIES);
        assert_eq!(kept[0], "e3");
        assert_eq!(kept.last().unwrap(), &format!("e{}", SHOWN_ENTRIES + 2));
        let (every, none) = shown(&lines, true);
        assert_eq!((every.len(), none), (SHOWN_ENTRIES + 3, 0));
        assert_eq!(shown(&lines[..2], false), (&lines[..2], 0));
        assert_eq!(
            older_note(1),
            "1 older entry is not shown; `workflow lessons --all` prints every entry."
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_new_file_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lessons.txt");
        append_entry(&path, "2026-11-23 run r1: x").unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
