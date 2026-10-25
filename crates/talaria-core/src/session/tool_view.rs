//! A pure view of one tool cell. Issue 221. The cell is not changed.
//!
//! Hermes's verdict (`is_error`, already in the cell's status) decides success; the view never
//! judges a result again. It only words what the cell holds.

use serde_json::{Map, Value};

use super::text::strip_terminal_escapes;
use super::{ToolCell, ToolProgress, ToolStatus};
use crate::diff::{DiffModel, parse_diff};

/// What the screen shows for one tool cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolView {
    /// Short name, such as `Shell` or a label.
    pub title: String,
    /// What the call acted on. Absent for a label title and for a quiet tool.
    pub subject: Option<String>,
    /// Running, succeeded, failed, unfinished or unknown.
    pub outcome: OutcomeView,
    /// How long the call took, in Hermes's wording.
    pub duration: Option<String>,
    /// Progress wording, when the cell has progress.
    pub progress: Option<String>,
    /// Cleaned output, when the cell has a result.
    pub output: Option<OutputView>,
    /// Parsed diff, when the cell has one.
    pub diff: Option<DiffModel>,
    /// Quiet tools draw no body.
    pub quiet: bool,
}

/// How the header states the outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutcomeView {
    /// The call is still running.
    Running,
    /// Hermes did not mark the call as an error.
    Succeeded {
        /// Optional headline, such as `exit 1` for a tool that is not a shell.
        headline: Option<String>,
    },
    /// Hermes marked the call as an error.
    Failed {
        /// First line of the failure, possibly prefixed with the exit code.
        headline: String,
        /// The whole failure text when it is long or has more than one line.
        detail: Option<String>,
    },
    /// The turn ended while the call was still running.
    Unfinished,
    /// A tool row restored from history, with no outcome.
    Unknown,
}

/// Cleaned output of one call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputView {
    /// Output after escape and carriage-return cleanup.
    pub text: String,
    /// Logical lines in `text`.
    pub lines: usize,
    /// The gateway cut the result at 32,768 characters.
    pub cut_by_hermes: bool,
    /// Integer exit code, when the result had one.
    pub exit_code: Option<i64>,
    /// The terminal tool's own truncation note.
    pub tool_note: Option<String>,
}

/// Result keys that hold a tool's output text, in the order they are tried.
const OUTPUT_KEYS: [&str; 6] = [
    "output",
    "stdout",
    "output_preview",
    "content",
    "result",
    "message",
];
/// Result keys that explain a failure, in the order they are tried.
const FAILURE_KEYS: [&str; 5] = [
    "user_summary",
    "error",
    "exit_code_meaning",
    "message",
    "reason",
];
/// Tools whose cells draw no body.
const QUIET: [&str; 3] = ["clarify", "todo_list", "todo"];
/// A failure text longer than this, or on several lines, is also kept whole as the detail.
const DETAIL_CHARS: usize = 60;

/// Talaria's title for a Hermes tool name.
fn title_of(name: &str) -> String {
    let known = match name {
        "terminal" => "Shell",
        "execute_code" => "Code",
        "read_file" => "Read",
        "write_file" => "Write",
        "patch" => "Edit",
        "search_files" => "Search",
        "web_search" => "Web search",
        "web_extract" => "Fetch",
        "delegate_task" => "Delegate",
        "skill_view" | "skill_manage" => "Skill",
        "memory" => "Memory",
        "clarify" => "Question",
        "todo_list" | "todo" => "Tasks",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_owned();
    }
    let spaced = name.trim().replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Tool".to_owned(),
    }
}

/// The title from the first label with non-blank `text`, plus how many labels follow it.
fn label_title(labels: &[Value]) -> Option<String> {
    let (index, text) = labels.iter().enumerate().find_map(|(index, label)| {
        label
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| (index, text))
    })?;
    let more = labels.len() - index - 1;
    Some(if more > 0 {
        format!("{text} +{more}")
    } else {
        text.to_owned()
    })
}

/// One line: line breaks become spaces, trimmed, empty meaning none.
fn one_line(text: &str) -> Option<String> {
    let line = text
        .replace("\r\n", " ")
        .replace(['\n', '\r'], " ")
        .trim()
        .to_owned();
    (!line.is_empty()).then_some(line)
}

/// The first non-blank line of a string argument.
fn first_line_of(args: Option<&Value>, key: &str) -> Option<String> {
    let text = args?.get(key)?.as_str()?;
    text.lines().find_map(one_line)
}

fn subject_of(cell: &ToolCell) -> Option<String> {
    let context = || cell.context.as_deref().and_then(one_line);
    match cell.name.as_str() {
        "terminal" => first_line_of(cell.args.as_ref(), "command").or_else(context),
        "execute_code" => first_line_of(cell.args.as_ref(), "code").or_else(context),
        _ => context(),
    }
}

/// Escapes removed, `\r\n` as `\n`, each line as a terminal would leave it after its last
/// lone `\r`, and no trailing blank lines.
fn clean_output(text: &str) -> String {
    let stripped = strip_terminal_escapes(text).replace("\r\n", "\n");
    let mut lines: Vec<&str> = stripped
        .split('\n')
        .map(|line| line.rsplit('\r').next().unwrap_or(line))
        .collect();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn count_lines(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.matches('\n').count() + usize::from(!text.ends_with('\n'))
    }
}

fn hex4(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<u32> {
    let mut value = 0;
    for _ in 0..4 {
        value = value * 16 + chars.next()?.to_digit(16)?;
    }
    Some(value)
}

/// The body of a JSON string that may be cut anywhere: decoding stops at the first unescaped
/// quote or at the end, and an escape cut short at the end is dropped.
fn lenient_json_string(body: &str) -> String {
    let mut out = String::new();
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => break,
            '\\' => match chars.next() {
                None => break,
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('b') => out.push('\u{8}'),
                Some('f') => out.push('\u{c}'),
                Some('u') => {
                    let Some(unit) = hex4(&mut chars) else { break };
                    if (0xD800..0xDC00).contains(&unit) {
                        let mut ahead = chars.clone();
                        let low = (ahead.next() == Some('\\') && ahead.next() == Some('u'))
                            .then(|| hex4(&mut ahead))
                            .flatten()
                            .filter(|low| (0xDC00..0xE000).contains(low));
                        match low {
                            Some(low) => {
                                chars = ahead;
                                let joined = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                                out.push(char::from_u32(joined).unwrap_or('\u{fffd}'));
                            }
                            None => out.push('\u{fffd}'),
                        }
                    } else {
                        out.push(char::from_u32(unit).unwrap_or('\u{fffd}'));
                    }
                }
                Some(other) => out.push(other),
            },
            other => out.push(other),
        }
    }
    out
}

/// What the result string holds, read once.
#[derive(Default)]
struct Parsed {
    object: Option<Map<String, Value>>,
    /// Output text before cleanup.
    output: Option<String>,
    exit_code: Option<i64>,
    tool_note: Option<String>,
    cut: bool,
    /// A cut result whose inner text was decoded as output: what explained a failure was cut.
    decoded: bool,
    /// The failure text for a result that is not a JSON object, or a cut kept as it is.
    raw: Option<String>,
}

fn string_field(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(Value::as_str))
        .map(str::to_owned)
}

fn parse_result(result: &str) -> Parsed {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(result) else {
        return Parsed {
            output: Some(result.to_owned()),
            raw: Some(result.to_owned()),
            ..Parsed::default()
        };
    };
    let cut = map.get("truncated") == Some(&Value::Bool(true));
    let inner = string_field(&map, &["output", "error"]);
    if cut && let Some(inner) = inner {
        let decoded = OUTPUT_KEYS.iter().find_map(|key| {
            inner
                .strip_prefix(&format!("{{\"{key}\": \""))
                .map(lenient_json_string)
        });
        return match decoded {
            Some(text) => Parsed {
                object: Some(map),
                output: Some(text),
                cut,
                decoded: true,
                ..Parsed::default()
            },
            None => Parsed {
                object: Some(map),
                output: Some(inner.clone()),
                cut,
                raw: Some(inner),
                ..Parsed::default()
            },
        };
    }
    Parsed {
        output: string_field(&map, &OUTPUT_KEYS),
        exit_code: map.get("exit_code").and_then(|code| {
            if code.is_i64() || code.is_u64() {
                code.as_i64()
            } else {
                None
            }
        }),
        tool_note: map
            .get("truncation_note")
            .and_then(Value::as_str)
            .map(str::to_owned),
        cut,
        object: Some(map),
        ..Parsed::default()
    }
}

fn failed_outcome(parsed: &Parsed) -> OutcomeView {
    let text = if parsed.decoded {
        None
    } else if let Some(raw) = &parsed.raw {
        Some(raw.clone())
    } else {
        parsed.object.as_ref().and_then(|map| {
            FAILURE_KEYS.iter().find_map(|key| {
                map.get(*key)
                    .and_then(Value::as_str)
                    .filter(|text| !text.trim().is_empty())
                    .map(str::to_owned)
            })
        })
    };
    let text = text
        .map(|text| clean_output(&text))
        .filter(|text| !text.trim().is_empty());
    let first = text
        .as_deref()
        .and_then(|text| text.lines().find(|line| !line.trim().is_empty()))
        .map(|line| line.trim().to_owned());
    let exit = parsed.exit_code.filter(|code| *code != 0);
    let headline = match (exit, first) {
        (Some(code), Some(line)) => format!("exit {code}: {line}"),
        (Some(code), None) => format!("exit {code}"),
        (None, Some(line)) => line,
        (None, None) => "failed".to_owned(),
    };
    let detail = text.filter(|text| text.contains('\n') || text.chars().count() > DETAIL_CHARS);
    OutcomeView::Failed { headline, detail }
}

/// The diff an edit shows: the cell's escape-stripped `inline_diff`, else a `patch` result's
/// raw `diff`.
fn diff_of(cell: &ToolCell, parsed: Option<&Parsed>) -> Option<DiffModel> {
    if let Some(model) = cell
        .diff
        .as_deref()
        .map(parse_diff)
        .filter(|model| !model.is_empty())
    {
        return Some(model);
    }
    if cell.name != "patch" {
        return None;
    }
    let text = parsed?.object.as_ref()?.get("diff")?.as_str()?;
    if text.trim().is_empty() {
        return None;
    }
    Some(parse_diff(&strip_terminal_escapes(text))).filter(|model| !model.is_empty())
}

/// Derive the view. Issue 221.
#[must_use]
pub fn tool_view(cell: &ToolCell) -> ToolView {
    let parsed = cell.result.as_deref().map(parse_result);
    let quiet = QUIET.contains(&cell.name.as_str());
    let labelled = label_title(&cell.labels);
    let title = labelled.clone().unwrap_or_else(|| title_of(&cell.name));
    let diff = diff_of(cell, parsed.as_ref());
    let mut subject = if labelled.is_some() || quiet {
        None
    } else {
        subject_of(cell)
    };
    if subject.is_none() && labelled.is_none() && !quiet {
        subject = diff
            .as_ref()
            .and_then(DiffModel::display_path)
            .and_then(one_line);
    }
    let output = parsed.as_ref().map(|parsed| {
        let text = clean_output(parsed.output.as_deref().unwrap_or_default());
        OutputView {
            lines: count_lines(&text),
            text,
            cut_by_hermes: parsed.cut,
            exit_code: parsed.exit_code,
            tool_note: parsed.tool_note.clone(),
        }
    });
    let outcome = match cell.status {
        ToolStatus::Running => OutcomeView::Running,
        ToolStatus::Unfinished => OutcomeView::Unfinished,
        ToolStatus::Unknown => OutcomeView::Unknown,
        ToolStatus::Succeeded => OutcomeView::Succeeded {
            headline: cell.summary.as_deref().and_then(one_line).or_else(|| {
                parsed
                    .as_ref()
                    .and_then(|parsed| parsed.exit_code)
                    .filter(|code| *code != 0)
                    .map(|code| format!("exit {code}"))
            }),
        },
        ToolStatus::Failed => failed_outcome(parsed.as_ref().unwrap_or(&Parsed::default())),
    };
    ToolView {
        title,
        subject,
        outcome,
        duration: cell.duration_s.and_then(format_duration),
        progress: cell.progress.as_ref().and_then(format_progress),
        output,
        diff,
        quiet,
    }
}

/// Hermes's duration words. Negative and non-finite values are absent.
///
/// Values are compared before rounding, and whole seconds round half to even, as Hermes's
/// Python formatting does.
#[must_use]
pub fn format_duration(seconds: f64) -> Option<String> {
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    if seconds < 10.0 {
        return Some(format!("{seconds:.1}s"));
    }
    if seconds < 60.0 {
        return Some(format!("{}s", seconds.round_ties_even()));
    }
    // The value is finite and non-negative here, so the conversion only saturates.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total = seconds.round_ties_even() as u64;
    let (minutes, rest) = (total / 60, total % 60);
    Some(if rest == 0 {
        format!("{minutes}m")
    } else {
        format!("{minutes}m {rest}s")
    })
}

/// A progress number without a trailing `.0` when it is whole, else with one decimal.
fn progress_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// Progress words from a payload: a percent within 0 to 100, else a fraction of a positive
/// total, then the message's first line.
#[must_use]
pub fn format_progress(progress: &ToolProgress) -> Option<String> {
    let number = match (progress.percent, progress.total) {
        (Some(percent), _) if (0..=100).contains(&percent) => Some(format!("{percent}%")),
        (_, Some(total)) if total > 0.0 && total.is_finite() && progress.progress.is_finite() => {
            Some(format!(
                "{}/{}",
                progress_number(progress.progress),
                progress_number(total)
            ))
        }
        _ => None,
    };
    let message = progress
        .message
        .as_deref()
        .and_then(|message| message.lines().find_map(one_line));
    match (number, message) {
        (Some(number), Some(message)) => Some(format!("{number} \u{b7} {message}")),
        (Some(number), None) => Some(number),
        (None, Some(message)) => Some(message),
        (None, None) => None,
    }
}
