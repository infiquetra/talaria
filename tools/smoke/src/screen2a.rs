//! Pure helpers over phase 2a screen text, one per check the steps make.
//!
//! Every helper takes raw screen text and reads it the way the steps do: lines with
//! box-drawing characters removed and each line trimmed, except [`has_debris`], which looks
//! at the raw lines for the debris itself.

use std::sync::OnceLock;

use regex::Regex;

use crate::Token;
use crate::wording::{
    AGENT_WORDS, DEBRIS, DIFF_ROWS, EDIT_HEADERS, EDIT_STAT, MENTION_ROW, PAGER_FOLLOW,
    PAGER_HEADER, PAGER_NOMATCH, PALETTE_FOOTER, SHELL_CELL, SHELL_EXIT_MARK, SHELL_HEADER,
    SIDE_EMPTY, SIDE_FAILED, SIDE_FOOTER, SIDE_HEADER, SYSTEM_GUTTER, TASK_ACTIVE, TASK_COLLAPSED,
    TASK_EXPAND_HINT, TASK_EXPANDED, THEME_LINES,
};

/// How many lines at the end of the screen the palette footer is looked for in.
const FOOTER_LINES: usize = 25;

/// The mark of a finished tool header (issue #221, shell output rows scenario).
const SUCCESS_MARK: &str = "✓";

/// The separator the footers and headers put between their parts.
const SEPARATOR: &str = "·";

/// The start of a user row in the transcript (issue #222, transcript layout scenario).
const USER_ROW: &str = "›";

/// The mark before the selected row of the `@` completion list (recorded from the live
/// binary).
const SELECTED_ROW: &str = "›";

/// The mark of a tool header: finished well or finished badly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// The `✓` mark.
    Success,
    /// The `✗` mark.
    Failure,
}

/// A parsed tool header row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolHeader {
    /// The row that holds the header.
    pub line_index: usize,
    /// The header's mark.
    pub mark: Mark,
    /// The exit code after the failure mark, if any.
    pub exit: Option<i32>,
}

/// A parsed edit header row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditHeader {
    /// The row that holds the header.
    pub line_index: usize,
    /// Whether the stat fragment is present.
    pub stat_ok: bool,
}

/// One palette row: the command name and its tag, if shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteRow {
    /// The command name.
    pub name: String,
    /// The right-aligned tag, if shown.
    pub tag: Option<String>,
}

/// The parsed palette: whether the footer is present and the rows in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Whether the footer row is present.
    pub footer: bool,
    /// The selectable rows in order.
    pub rows: Vec<PaletteRow>,
}

/// One mention row: the file and its folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionRow {
    /// The file's base name.
    pub file: String,
    /// The parent folder.
    pub folder: String,
}

/// A parsed collapsed-paste label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteLabel {
    /// The paste number.
    pub number: u32,
    /// The line count after the `+`.
    pub lines: u32,
}

/// A parsed shell cell: the header row and the output rows it owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellCell {
    /// The row that holds the header.
    pub header_index: usize,
    /// The output rows under the header.
    pub output_has: Vec<String>,
}

/// A parsed side-answer cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideCell {
    /// The row that holds the header.
    pub header_index: usize,
    /// The row that holds the footer, or the header row when the cell failed and carries
    /// no footer.
    pub footer_index: usize,
    /// The answer text between the header and the footer.
    pub answer: String,
    /// Whether the answer failed.
    pub failed: bool,
    /// Whether the answer is empty.
    pub empty: bool,
}

/// The parsed pager frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pager {
    /// Whether the pager is open.
    pub open: bool,
    /// Whether it follows new output.
    pub following: bool,
    /// The confirmed search: match index, match count, query.
    pub search: Option<(u32, u32, String)>,
    /// Whether the frame reports no match.
    pub no_match: bool,
}

/// A parsed theme report: which of the five lines is present, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeReport {
    /// One flag per line of [`crate::wording::THEME_LINES`], in order.
    pub present: [bool; 5],
}

/// The task panel: expanded, collapsed or absent, with the in-progress row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskView {
    /// The expanded panel and the in-progress row, if any.
    Expanded { active_row: Option<String> },
    /// The collapsed panel; the header line names the step in progress.
    Collapsed { active_row: Option<String> },
    /// No panel.
    Absent,
}

/// The agent state of a pane-state report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentState {
    /// Idle.
    Idle,
    /// Working.
    Working,
    /// Blocked.
    Blocked,
}

/// The text's lines without box-drawing characters (U+2500 to U+257F) and without the `· `
/// gutter Talaria draws before a system line, each trimmed.
pub(crate) fn normalised(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| {
            let plain: String = line
                .chars()
                .filter(|character| !('\u{2500}'..='\u{257F}').contains(character))
                .collect();
            let plain = plain.trim();
            match plain.strip_prefix(SYSTEM_GUTTER) {
                Some(rest) if rest.is_empty() || rest.starts_with(char::is_whitespace) => {
                    rest.trim().to_owned()
                }
                _ => plain.to_owned(),
            }
        })
        .collect()
}

/// Whether the line starts with `prefix` and the prefix ends the line or a word.
fn starts_with_word(line: &str, prefix: &str) -> bool {
    line.strip_prefix(prefix).is_some_and(|rest| {
        rest.chars()
            .next()
            .is_none_or(|next| !next.is_alphanumeric())
    })
}

/// The line split at runs of two or more spaces, which is how rows lay out their columns.
fn columns(line: &str) -> Vec<&str> {
    line.split("  ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// The tool header for `command`: the first line starting `⏺ Shell(<command>)` that holds
/// the success mark or the failure mark with an exit code.
pub fn tool_header(text: &str, command: &str) -> Option<ToolHeader> {
    let start = format!("{SHELL_HEADER}{command})");
    normalised(text)
        .iter()
        .enumerate()
        .find_map(|(line_index, line)| {
            let rest = line.strip_prefix(&start)?;
            if let Some((_, after)) = rest.split_once(SHELL_EXIT_MARK) {
                let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
                let exit = digits.parse::<i32>().ok()?;
                return Some(ToolHeader {
                    line_index,
                    mark: Mark::Failure,
                    exit: Some(exit),
                });
            }
            rest.contains(SUCCESS_MARK).then_some(ToolHeader {
                line_index,
                mark: Mark::Success,
                exit: None,
            })
        })
}

/// Whether any of the `within` lines after line `from` holds `needle`.
pub fn rows_after(text: &str, from: usize, within: usize, needle: &str) -> bool {
    normalised(text)
        .iter()
        .skip(from.saturating_add(1))
        .take(within)
        .any(|line| line.contains(needle))
}

/// The edit header for `file`: the first line starting `⏺ Edit` or `⏺ Write` that holds
/// the file name. `stat_ok` tells whether it also holds the stat fragment.
pub fn edit_header(text: &str, file: &str) -> Option<EditHeader> {
    normalised(text)
        .iter()
        .enumerate()
        .find_map(|(line_index, line)| {
            let is_edit = EDIT_HEADERS.iter().any(|start| line.starts_with(start));
            (is_edit && line.contains(file)).then(|| EditHeader {
                line_index,
                stat_ok: line.contains(EDIT_STAT),
            })
        })
}

/// Whether the numbered diff rows sit at or after line `from`.
pub fn has_diff_rows(text: &str, from: usize) -> bool {
    let lines = normalised(text);
    let after = lines.get(from..).unwrap_or_default();
    DIFF_ROWS.iter().all(|row| {
        after.iter().any(|line| {
            line.strip_prefix(row)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
        })
    })
}

/// Whether any raw line holds diff-header or escape debris.
pub fn has_debris(text: &str) -> bool {
    text.lines()
        .any(|line| DEBRIS.iter().any(|piece| line.contains(piece)))
}

/// The parsed palette: footer presence and rows in order.
pub fn palette_state(visible: &str) -> Palette {
    let lines = normalised(visible);
    let tail = lines.len().saturating_sub(FOOTER_LINES);
    let footer = lines[tail..]
        .iter()
        .any(|line| line.contains(PALETTE_FOOTER) && line.contains(SEPARATOR));
    let rows = lines.iter().filter_map(|line| palette_row(line)).collect();
    Palette { footer, rows }
}

/// A palette row: a command name, then the tag at the right edge if one is shown.
fn palette_row(line: &str) -> Option<PaletteRow> {
    let is_name = |name: &str| {
        name.len() > 1
            && name.starts_with('/')
            && name[1..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    let wide = columns(line);
    if wide.len() > 1 {
        return is_name(wide[0]).then(|| PaletteRow {
            name: wide[0].to_owned(),
            tag: wide.last().map(|tag| (*tag).to_owned()),
        });
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    match words.as_slice() {
        [name] if is_name(name) => Some(PaletteRow {
            name: (*name).to_owned(),
            tag: None,
        }),
        [name, tag] if is_name(name) => Some(PaletteRow {
            name: (*name).to_owned(),
            tag: Some((*tag).to_owned()),
        }),
        _ => None,
    }
}

/// The mention row for the scenario's file, with its folder, if the list shows it. The
/// selected row starts with the `›` mark.
pub fn mention_list(visible: &str) -> Option<MentionRow> {
    normalised(visible).iter().find_map(|line| {
        let line = line
            .strip_prefix(SELECTED_ROW)
            .map_or(line.as_str(), str::trim_start);
        let folder = line.strip_prefix(MENTION_ROW)?;
        if !folder.is_empty() && !folder.starts_with(char::is_whitespace) {
            return None;
        }
        Some(MentionRow {
            file: MENTION_ROW.to_owned(),
            folder: folder.trim().to_owned(),
        })
    })
}

/// Every collapsed-paste label in the text, in order.
pub fn paste_labels(text: &str) -> Vec<PasteLabel> {
    static LABEL: OnceLock<Option<Regex>> = OnceLock::new();
    let Some(label) =
        LABEL.get_or_init(|| Regex::new(r"\[Pasted text #(\d+) \+(\d+) lines\]").ok())
    else {
        return Vec::new();
    };
    let joined = normalised(text).join(" ");
    label
        .captures_iter(&joined)
        .filter_map(|found| {
            Some(PasteLabel {
                number: found[1].parse().ok()?,
                lines: found[2].parse().ok()?,
            })
        })
        .collect()
}

/// Whether any line holds the dropped-file label for `name`.
pub fn file_label(text: &str, name: &str) -> bool {
    let Ok(label) = Regex::new(&format!(r"\[File #\d+:\s*{}\]", regex::escape(name))) else {
        return false;
    };
    text.lines().any(|line| label.is_match(line)) || label.is_match(&normalised(text).join(""))
}

/// Whether the line starts something that ends a cell's output.
fn ends_cell(line: &str) -> bool {
    line.starts_with(SHELL_CELL)
        || line.starts_with(SIDE_HEADER)
        || line.starts_with(USER_ROW)
        || line.starts_with("⏺")
        || line.starts_with("> ")
}

/// The parsed shell cell: the header row and the output rows it owns.
pub fn shell_cell(text: &str, command: &str) -> Option<ShellCell> {
    let header = format!("{SHELL_CELL}{command}");
    let lines = normalised(text);
    let header_index = lines.iter().position(|line| {
        line.strip_prefix(&header)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    })?;
    let output_has = lines[header_index + 1..]
        .iter()
        .take_while(|line| !ends_cell(line))
        .filter(|line| !line.is_empty())
        .cloned()
        .collect();
    Some(ShellCell {
        header_index,
        output_has,
    })
}

/// The parsed side-answer cell for `question`.
pub fn side_cell(text: &str, question: &str) -> Option<SideCell> {
    let header = format!("{SIDE_HEADER}{question}");
    let lines = normalised(text);
    let header_index = lines.iter().position(|line| line.starts_with(&header))?;
    let mut answer: Vec<&str> = Vec::new();
    for (offset, line) in lines[header_index + 1..].iter().enumerate() {
        let index = header_index + 1 + offset;
        if let Some(rest) = line.strip_prefix(SIDE_FAILED) {
            return Some(SideCell {
                header_index,
                footer_index: header_index,
                answer: rest.trim().to_owned(),
                failed: true,
                empty: false,
            });
        }
        if line.starts_with(SIDE_FOOTER) {
            let answer = answer.join("\n");
            return Some(SideCell {
                header_index,
                footer_index: index,
                empty: answer.is_empty() || answer == SIDE_EMPTY,
                answer,
                failed: false,
            });
        }
        if line.starts_with(SIDE_HEADER) || line.starts_with(USER_ROW) {
            return None;
        }
        if !line.is_empty() {
            answer.push(line);
        }
    }
    None
}

/// The parsed pager frame.
pub fn pager_state(visible: &str) -> Pager {
    static SEARCH: OnceLock<Option<Regex>> = OnceLock::new();
    let lines = normalised(visible);
    let open = lines.iter().any(|line| line.starts_with(PAGER_HEADER));
    if !open {
        return Pager {
            open,
            following: false,
            search: None,
            no_match: false,
        };
    }
    let following = lines.iter().any(|line| {
        line.split(SEPARATOR)
            .skip(1)
            .any(|part| starts_with_word(part.trim(), PAGER_FOLLOW))
    });
    let search = SEARCH
        .get_or_init(|| Regex::new(r#"^"(.+)" (\d+) of (\d+)\b"#).ok())
        .as_ref()
        .and_then(|pattern| {
            lines.iter().rev().find_map(|line| {
                let found = pattern.captures(line)?;
                Some((
                    found[2].parse().ok()?,
                    found[3].parse().ok()?,
                    found[1].to_owned(),
                ))
            })
        });
    let no_match = lines.iter().any(|line| line.starts_with(PAGER_NOMATCH));
    Pager {
        open,
        following,
        search,
        no_match,
    }
}

/// The parsed theme report: which lines are present.
pub fn theme_report(text: &str) -> ThemeReport {
    let lines = normalised(text);
    let mut present = [false; 5];
    for (flag, key) in present.iter_mut().zip(THEME_LINES) {
        *flag = lines.iter().any(|line| {
            if key.ends_with(':') {
                line.starts_with(key)
            } else {
                starts_with_word(line, key)
            }
        });
    }
    ThemeReport { present }
}

/// The task panel: expanded, collapsed or absent, with the in-progress row.
pub fn task_view(visible: &str) -> TaskView {
    let lines = normalised(visible);
    let Some(at) = lines
        .iter()
        .rposition(|line| line.starts_with(TASK_EXPANDED) || line.starts_with(TASK_COLLAPSED))
    else {
        return TaskView::Absent;
    };
    let header = &lines[at];
    if header.starts_with(TASK_COLLAPSED) {
        // The collapsed header names the step in progress between its count and its hint.
        let names_a_step = header
            .split(SEPARATOR)
            .skip(1)
            .map(str::trim)
            .any(|part| !part.is_empty() && !part.contains(TASK_EXPAND_HINT));
        return TaskView::Collapsed {
            active_row: names_a_step.then(|| header.clone()),
        };
    }
    let active_row = lines[at + 1..]
        .iter()
        .take_while(|line| line.starts_with('['))
        .find(|line| line.starts_with(TASK_ACTIVE))
        .cloned();
    TaskView::Expanded { active_row }
}

/// Every task record in the text, in order, as `(completed, total)`.
pub fn task_records(text: &str) -> Vec<(u32, u32)> {
    static RECORD: OnceLock<Option<Regex>> = OnceLock::new();
    let Some(record) = RECORD.get_or_init(|| Regex::new(r"^Tasks (\d+)/(\d+) · ").ok()) else {
        return Vec::new();
    };
    normalised(text)
        .iter()
        .filter_map(|line| {
            let found = record.captures(line)?;
            Some((found[1].parse().ok()?, found[2].parse().ok()?))
        })
        .collect()
}

/// The agent state of a pane-state report, or `None` when the `talaria` label is absent
/// or no state word is shown. Matching is case-insensitive. Herdr reports a finished turn
/// nobody has looked at yet as `done`, which is the same ready state as `idle`.
pub fn agent_state(pane_get_text: &str) -> Option<AgentState> {
    let lower = pane_get_text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| c.is_whitespace() || "\"'`,:;=()[]{}<>|".contains(c))
        .filter(|word| !word.is_empty())
        .collect();
    let label = AGENT_WORDS.first()?;
    let at = words.iter().position(|word| word == label)?;
    let state_of = |word: &&str| match *word {
        "idle" | "done" => Some(AgentState::Idle),
        "working" => Some(AgentState::Working),
        "blocked" => Some(AgentState::Blocked),
        _ => None,
    };
    // The state word follows the label; a report that lists it first still counts.
    words[at..]
        .iter()
        .find_map(state_of)
        .or_else(|| words[..at].iter().find_map(state_of))
}

/// The prompt the phase 2a tool steps submit: `<action>, then join these two words with a
/// hyphen and reply with only the result: <word> and <number>`.
pub fn prompt_after(token: &Token, action: &str) -> String {
    format!(
        "{action}, then join these two words with a hyphen and reply with only the result: \
         {} and {}",
        token.word, token.number
    )
}
