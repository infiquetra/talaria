//! Telling a background process that runs a workflow apart from an ordinary one, and finding
//! out which run it carries. Pure text work on the command and the output.
//!
//! The link between a process and a run comes from two places only: the run target a
//! `talaria workflow resume` command names, and the `run_started` line a `talaria workflow run`
//! prints first (a JSON object with `--json`, `run <id> in <folder>` without). Nothing is
//! guessed from names or times.

use serde_json::Value;

use super::{ProcessOutput, ProcessRow};
use crate::side::clean_output;

/// How many leading output lines are searched for the line that names the run.
const START_LINES: usize = 64;
/// Longest label or detail text in a summary, in characters.
const DETAIL_CHARS: usize = 40;
/// Options of `talaria workflow run` and `resume` that take a value in the next word.
const VALUE_FLAGS: &[&str] = &[
    "--args",
    "--args-file",
    "--profile",
    "--session-model",
    "--session-provider",
    "--staffing",
    "--cwd",
    "--budget-tokens",
    "--budget-usd",
    "--on-unknown",
    "--record",
    "--runs-dir",
    "--workflows-dir",
    "--run-dir",
    "--script",
];

/// Which workflow command a process runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowVerb {
    /// `talaria workflow run`.
    Run,
    /// `talaria workflow resume`.
    Resume,
}

/// A `talaria workflow run` or `resume` command found in a shell line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowCommand {
    /// Run or resume.
    pub verb: WorkflowVerb,
    /// The `talaria workflow …` part, with the script or run shortened to its last path part.
    pub label: String,
    /// For `resume`, the run id or run folder the command names.
    pub resume_target: Option<String>,
}

/// The run a workflow process is known to carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunLink {
    /// The process names its run: a run id or a run folder path.
    Named(String),
    /// The process printed which run it started.
    Started {
        /// The run id.
        run_id: String,
        /// The run folder, when the line gave one.
        run_dir: Option<String>,
    },
    /// The process runs a workflow, but nothing it printed or was given names the run.
    Unknown,
}

/// A background process that runs a workflow, and what is known about its run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowProcess {
    /// The command.
    pub command: WorkflowCommand,
    /// The run it carries.
    pub link: RunLink,
}

/// The run a workflow dock is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunIdentity<'a> {
    /// The run id, which is also the name of the run folder.
    pub run_id: &'a str,
    /// The run folder.
    pub run_dir: &'a str,
}

impl RunLink {
    /// Whether this link points at `run`. An unknown link points at nothing.
    #[must_use]
    pub fn is_run(&self, run: RunIdentity<'_>) -> bool {
        match self {
            Self::Unknown => false,
            Self::Started { run_id, run_dir } => {
                run_id == run.run_id
                    || run_dir
                        .as_deref()
                        .is_some_and(|dir| same_folder(dir, run.run_dir))
            }
            Self::Named(target) => {
                target == run.run_id
                    || same_folder(target, run.run_dir)
                    || (target.contains('/') && last_part(target) == run.run_id)
            }
        }
    }
}

impl WorkflowProcess {
    /// Whether the process carries `run`.
    #[must_use]
    pub fn carries(&self, run: RunIdentity<'_>) -> bool {
        self.link.is_run(run)
    }
}

/// What a row runs, when it runs a workflow. `output` is the row's kept output, if any.
#[must_use]
pub fn workflow_process(
    row: &ProcessRow,
    output: Option<&ProcessOutput>,
) -> Option<WorkflowProcess> {
    let command = workflow_command(&row.command)?;
    let link = if let Some(target) = &command.resume_target {
        RunLink::Named(target.clone())
    } else {
        output
            .and_then(|output| started_run(output.raw()))
            .unwrap_or(RunLink::Unknown)
    };
    Some(WorkflowProcess { command, link })
}

/// Find `talaria workflow run` or `talaria workflow resume` in a shell line.
#[must_use]
pub fn workflow_command(shell_line: &str) -> Option<WorkflowCommand> {
    let words = split_words(shell_line);
    let at = words.windows(3).position(|window| {
        last_part(&window[0]) == "talaria"
            && window[1] == "workflow"
            && matches!(window[2].as_str(), "run" | "resume")
    })?;
    let verb = if words[at + 2] == "run" {
        WorkflowVerb::Run
    } else {
        WorkflowVerb::Resume
    };
    let positional = first_positional(&words[at + 3..]);
    let name = if verb == WorkflowVerb::Run {
        "run"
    } else {
        "resume"
    };
    let mut label = format!("talaria workflow {name}");
    if let Some(word) = &positional {
        label.push(' ');
        label.push_str(last_part(word));
    }
    Some(WorkflowCommand {
        verb,
        label,
        resume_target: positional.filter(|_| verb == WorkflowVerb::Resume),
    })
}

/// The run a `talaria workflow run` printed first, found among the leading lines of `output`.
#[must_use]
pub fn started_run(output: &str) -> Option<RunLink> {
    output.lines().take(START_LINES).find_map(|line| {
        let line = clean_output(line);
        let line = line.trim();
        if let Some(Value::Object(object)) = line.starts_with('{').then(|| parse(line)).flatten() {
            if object.get("event").and_then(Value::as_str) != Some("run_started") {
                return None;
            }
            let run_id = object.get("run_id")?.as_str()?.to_owned();
            let run_dir = object
                .get("run_dir")
                .and_then(Value::as_str)
                .map(str::to_owned);
            return Some(RunLink::Started { run_id, run_dir });
        }
        let rest = line.strip_prefix("run ")?;
        let (run_id, run_dir) = rest.split_once(" in ")?;
        (!run_id.is_empty() && !run_id.contains(char::is_whitespace)).then(|| RunLink::Started {
            run_id: run_id.to_owned(),
            run_dir: Some(run_dir.to_owned()),
        })
    })
}

/// A short readable form of a JSON event line, or nothing when the line is not one.
///
/// The event name comes first, then a label, title or message when the event has one. A
/// scheduler event reads as `scheduler queued`. A line cut short before its end still gives its
/// event name when the name came first.
#[must_use]
pub fn summarize_event_line(line: &str) -> Option<String> {
    let line = clean_output(line);
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    let Some(Value::Object(object)) = parse(line) else {
        return cut_event_name(line);
    };
    let mut name = object.get("event")?.as_str()?.to_owned();
    let mut detail = detail_of(&object);
    if let Some(Value::Object(inner)) = object.get("value") {
        if let Some(inner_name) = inner.get("event").and_then(Value::as_str) {
            name = format!("{name} {inner_name}");
        }
        detail = detail.or_else(|| detail_of(inner));
    }
    Some(match detail {
        Some(detail) => format!("{name} · {detail}"),
        None => name,
    })
}

/// The last non-empty line of `raw`, whole.
#[must_use]
pub fn last_whole_line(raw: &str) -> Option<String> {
    raw.rsplit('\n').find_map(|line| {
        let clean = clean_output(line);
        (!clean.trim().is_empty()).then_some(clean)
    })
}

fn parse(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok()
}

/// `{"event":"name"` at the start of a line that ends before its closing brace.
fn cut_event_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("{\"event\":\"")?;
    let (name, _) = rest.split_once('"')?;
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .then(|| name.to_owned())
}

fn detail_of(object: &serde_json::Map<String, Value>) -> Option<String> {
    ["label", "title", "message"].iter().find_map(|key| {
        let text = object.get(*key)?.as_str()?;
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        (!text.is_empty()).then(|| cut(&text, DETAIL_CHARS))
    })
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(max.saturating_sub(1)).collect();
    short.push('…');
    short
}

fn last_part(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

fn same_folder(left: &str, right: &str) -> bool {
    left.trim_end_matches('/') == right.trim_end_matches('/')
}

/// The first word after the verb that is not an option or an option's value.
fn first_positional(words: &[String]) -> Option<String> {
    let mut words = words.iter();
    while let Some(word) = words.next() {
        if is_operator(word) {
            return None;
        }
        if let Some(flag) = word.strip_prefix("--") {
            if !flag.contains('=') && VALUE_FLAGS.contains(&word.as_str()) {
                let _ = words.next();
            }
            continue;
        }
        if word.starts_with('-') && word.len() > 1 {
            continue;
        }
        return Some(word.clone());
    }
    None
}

fn is_operator(word: &str) -> bool {
    matches!(
        word,
        ";" | "|" | "||" | "&&" | "&" | ">" | ">>" | "<" | "\n"
    )
}

/// Split a shell line into words, keeping quoted text together without its quotes. Operators
/// (`;`, `|`, `&`, `<`, `>` and a newline) are words of their own. Escapes are not interpreted.
fn split_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(open) = quote {
            if ch == open {
                quote = None;
            } else {
                word.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                started = true;
            }
            c if c.is_whitespace() && c != '\n' => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            ';' | '|' | '&' | '<' | '>' | '\n' => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
                let mut operator = ch.to_string();
                if matches!(ch, '|' | '&' | '>') && chars.peek() == Some(&ch) {
                    operator.push(ch);
                    let _ = chars.next();
                }
                words.push(operator);
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}
