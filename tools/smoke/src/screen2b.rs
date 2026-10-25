//! Pure readers for model, context, queue and session screens.

use crate::screen2a::normalised;
use crate::wording2b::{
    PICKER_EFFORT, PICKER_NOT_SET_UP, PICKER_TITLE, PRINT_OK, REWIND_TITLE, USAGE_TITLE,
};

/// The model picker overlay, as one screen shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerState {
    /// Whether the picker title line is shown.
    pub title: bool,
    /// The provider group headers, in order.
    pub groups: Vec<String>,
    /// Whether a current-model row is shown.
    pub has_current: bool,
    /// The selected row's text, when a status row is selected.
    pub selected_row: Option<String>,
    /// Whether the not-set-up banner is shown.
    pub not_set_up: bool,
    /// Whether the effort footer is shown.
    pub effort_footer: bool,
    /// The no-match line, when the picker shows one.
    pub no_match: Option<String>,
}

/// The `/context` header line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextHeader {
    /// The model word of the header.
    pub model: String,
    /// Whether the header ends with a percent in parentheses.
    pub percent: bool,
    /// The header's line index.
    pub line_index: usize,
}

/// The category rows under a context header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextRows {
    /// Whether the category row is shown.
    pub categories: bool,
    /// Whether the free-space row is shown.
    pub free_space: bool,
    /// Whether the copied-source row is shown.
    pub source: bool,
}

/// The `/usage` block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageBlock {
    /// The title's line index.
    pub title_index: usize,
    /// Whether the tokens row is shown.
    pub tokens_row: bool,
    /// Whether the cost row is shown.
    pub cost_row: bool,
}

/// The rewind overlay's prompt list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewindList {
    /// Whether the overlay is open.
    pub open: bool,
    /// The listed prompt numbers, in screen order.
    pub prompts: Vec<u32>,
    /// The selected prompt number, if one is selected.
    pub selected: Option<u32>,
}

/// The three facts of a headless `--print` run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintCheck {
    /// Whether the printed text is exactly the answer.
    pub text_ok: bool,
    /// Whether the error stream is empty.
    pub err_empty: bool,
    /// Whether the exit marker says zero.
    pub exit_ok: bool,
    /// Whether the run is a no-gateway failure.
    pub not_started: bool,
}

/// Reads the model picker overlay of one screen.
pub fn picker_state(text: &str) -> PickerState {
    let lines = normalised(text);
    let raw: Vec<_> = text.lines().collect();
    let title_index = lines
        .iter()
        .rposition(|line| line == PICKER_TITLE || line.starts_with(&format!("{PICKER_TITLE} · ")));
    let mut state = PickerState {
        title: title_index.is_some(),
        groups: Vec::new(),
        has_current: false,
        selected_row: None,
        not_set_up: false,
        effort_footer: false,
        no_match: None,
    };
    let Some(title_index) = title_index else {
        return state;
    };
    for (index, line) in lines.iter().enumerate().skip(title_index + 1) {
        if line.starts_with(PICKER_EFFORT) {
            state.effort_footer = true;
            break;
        }
        if line.starts_with(PICKER_NOT_SET_UP) {
            state.not_set_up = true;
        } else if line.starts_with("No model matches \"") && line.ends_with("\".") {
            state.no_match = Some(line.clone());
        } else if let Some(row) = line.strip_prefix("› ") {
            state.selected_row = Some(row.to_owned());
        }
        state.has_current |= line.starts_with("● ") || line.starts_with("★● ");
        if !state.not_set_up
            && !line.starts_with(['★', '●', '>'])
            && let Some((group, _)) = line.split_once(" · ")
            && !group.is_empty()
        {
            state.groups.push(group.to_owned());
        } else if !state.not_set_up
            && !line.starts_with(['★', '●', '>'])
            && !line.starts_with("No model matches ")
            && line != "Loading models…"
            && raw[index]
                .strip_prefix(' ')
                .is_some_and(|rest| rest.starts_with(|c: char| !c.is_whitespace()))
        {
            state.groups.push(line.clone());
        }
    }
    state
}

/// Reads the `/context` header line of one screen.
pub fn context_header(text: &str) -> Option<ContextHeader> {
    normalised(text)
        .iter()
        .enumerate()
        .rev()
        .find_map(|(line_index, line)| {
            let rest = line.strip_prefix("Context · ")?;
            let (model, usage) = rest.split_once(" · ")?;
            let (used, capacity) = usage.split_once(" / ")?;
            let (capacity, suffix) = capacity.split_once(" tokens")?;
            if model.is_empty() || !token_count(used) || !token_count(capacity) {
                return None;
            }
            let percent = suffix
                .trim()
                .strip_prefix('(')
                .and_then(|value| value.strip_suffix("%)"))
                .is_some_and(|value| {
                    let value = value.strip_prefix('~').unwrap_or(value);
                    value.parse::<f64>().is_ok_and(|number| number.is_finite())
                });
            Some(ContextHeader {
                model: model.to_owned(),
                percent,
                line_index,
            })
        })
}

/// Reads the category rows under a context header.
pub fn context_rows(text: &str, header_line: usize) -> ContextRows {
    let lines = normalised(text);
    let mut rows = ContextRows {
        categories: false,
        free_space: false,
        source: false,
    };
    if !lines
        .get(header_line)
        .is_some_and(|line| line.starts_with("Context · "))
    {
        return rows;
    }
    for line in lines.iter().skip(header_line.saturating_add(1)) {
        if block_end(line) {
            break;
        }
        rows.categories |= line == "Estimated usage by category";
        rows.free_space |= starts_row(line, "Free space");
        rows.source |= line.starts_with("Source: ");
    }
    rows
}

/// Reads the `/usage` block of one screen.
pub fn usage_block(text: &str) -> Option<UsageBlock> {
    let lines = normalised(text);
    let title_index = lines.iter().rposition(|line| line == USAGE_TITLE)?;
    let mut block = UsageBlock {
        title_index,
        tokens_row: false,
        cost_row: false,
    };
    for line in &lines[title_index + 1..] {
        if block_end(line) {
            break;
        }
        block.tokens_row |= starts_row(line, "Tokens");
        block.cost_row |= starts_row(line, "Cost");
    }
    Some(block)
}

/// Reads the rewind overlay's prompt list of one screen.
pub fn rewind_list(text: &str) -> RewindList {
    let lines = normalised(text);
    let title_index = lines.iter().rposition(|line| line == REWIND_TITLE);
    let mut list = RewindList {
        open: title_index.is_some(),
        prompts: Vec::new(),
        selected: None,
    };
    let Some(title_index) = title_index else {
        return list;
    };
    for line in &lines[title_index + 1..] {
        let (selected, row) = line
            .strip_prefix("› ")
            .map_or((false, line.as_str()), |row| (true, row));
        let Some(row) = row.strip_prefix('#') else {
            if line.is_empty() {
                continue;
            }
            break;
        };
        let Some((prompt, suffix)) = number_prefix(row) else {
            continue;
        };
        if !suffix.is_empty() && !suffix.starts_with(char::is_whitespace) {
            continue;
        }
        list.prompts.push(prompt);
        if selected {
            list.selected = Some(prompt);
        }
    }
    list
}

/// The text after `approvals: ` on the status line, if the segment is shown.
pub fn status_approvals(visible: &str) -> Option<String> {
    status_segments(visible)?
        .into_iter()
        .find_map(|part| part.strip_prefix("approvals: ").map(str::to_owned))
}

/// The line index of the rewound marker for one prompt number.
pub fn rewound_line(text: &str, prompt: u32) -> Option<usize> {
    normalised(text)
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, line)| {
            let rest = line.strip_prefix("Rewound to before prompt #")?;
            let (number, suffix) = number_prefix(rest)?;
            (number == prompt && (suffix.starts_with(' ') || suffix.starts_with('.')))
                .then_some(index)
        })
}

/// The queued count of one screen, if a queued indicator is shown.
pub fn queue_indicator(text: &str) -> Option<u32> {
    normalised(text).iter().rev().find_map(|line| {
        let (preview, rest) = line.rsplit_once("queued (")?;
        let (count, suffix) = number_prefix(rest)?;
        let hint = suffix.strip_prefix(')')?;
        if preview.trim().is_empty() {
            (hint.is_empty() || hint.starts_with(" · ")).then_some(count)
        } else {
            (preview.ends_with(char::is_whitespace) && hint.starts_with(" · ↑ select"))
                .then_some(count)
        }
    })
}

/// The folder segment of the status line, if one is shown.
pub fn folder_segment(visible: &str) -> Option<String> {
    let segments = status_segments(visible)?;
    let approvals = segments
        .iter()
        .position(|part| part.starts_with("approvals: "))?;
    segments
        .get(approvals + 1)
        .filter(|folder| !folder.is_empty() && !folder.starts_with("ctx "))
        .map(|folder| (*folder).to_owned())
}

/// The lines after the last line holding the needle.
pub fn lines_after_last<'a>(text: &'a str, needle: &str) -> Vec<&'a str> {
    let lines: Vec<_> = text.lines().collect();
    let Some(index) = lines.iter().rposition(|line| line.contains(needle)) else {
        return Vec::new();
    };
    lines[index + 1..].to_vec()
}

/// Checks the three facts of a headless `--print` run.
pub fn print_check(out: &str, err: &str, exit: Option<i32>) -> PrintCheck {
    let mut lines = out.lines().filter(|line| !line.trim().is_empty());
    let first = lines.next();
    let not_started = first
        .filter(|_| lines.next().is_none())
        .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .is_some_and(|value| value["status"] == "not_started" && value["exit_code"] == 4);
    PrintCheck {
        text_ok: out.trim() == PRINT_OK,
        err_empty: err.is_empty(),
        exit_ok: exit == Some(0),
        not_started,
    }
}

fn token_count(value: &str) -> bool {
    !value.is_empty()
        && value.chars().any(|character| character.is_ascii_digit())
        && value
            .chars()
            .all(|character| character.is_ascii_digit() || character == ',')
}

fn starts_row(line: &str, label: &str) -> bool {
    line.strip_prefix(label)
        .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

fn block_end(line: &str) -> bool {
    line.starts_with('/')
        || line.starts_with('>')
        || line.starts_with('›')
        || line.starts_with("Context · ")
        || line == USAGE_TITLE
        || line.contains("approvals: ")
}

fn number_prefix(value: &str) -> Option<(u32, &str)> {
    let end = value
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(value.len());
    Some((value[..end].parse().ok()?, &value[end..]))
}

fn status_segments(visible: &str) -> Option<Vec<&str>> {
    visible.lines().rev().find_map(|line| {
        let segments: Vec<_> = line.split('│').map(str::trim).collect();
        (segments.len() >= 3
            && !segments[0].is_empty()
            && !segments[0].starts_with(['>', '›', '$'])
            && segments[1].starts_with("approvals: "))
        .then_some(segments)
    })
}
