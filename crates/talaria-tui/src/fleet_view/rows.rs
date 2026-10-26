//! The list part of the fleet overlay: header text, columns, group headers and rows.

use ratatui::buffer::Buffer;
use std::collections::BTreeMap;

use ratatui::style::Style;
use talaria_core::directory::rows::{RowKey, format_age};
use talaria_core::fleet::{FleetRow, ProfileStatus, RowState};
use talaria_core::fleet_view::detail::short_model;
use talaria_core::fleet_view::{
    FleetViewState, ProfileFilter, VisibleGroup, glyph, title_or_untitled,
};
use talaria_core::review::PrLabel;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::FleetTheme;
use crate::history::{grapheme_width, sanitize};

/// Width of the cursor mark and of the glyph.
const MARK_WIDTH: usize = 1;
/// Width of the title column.
const TITLE_WIDTH: usize = 28;
/// Width of the profile column.
const PROFILE_WIDTH: usize = 12;
/// Width of the model column.
const MODEL_WIDTH: usize = 18;
/// Width of the pull-request column.
const PR_WIDTH: usize = 14;
/// Width of the age column.
const AGE_WIDTH: usize = 4;
/// Width of the cost column.
const COST_WIDTH: usize = 7;
/// The profile column needs this many columns.
const PROFILE_MIN_WIDTH: u16 = 80;
/// The model column needs this many columns.
const MODEL_MIN_WIDTH: u16 = 60;
/// The pull-request column needs this many columns.
const PR_MIN_WIDTH: u16 = 110;
/// The cost column needs this many columns.
const COST_MIN_WIDTH: u16 = 100;
/// Between two columns.
const GAP: &str = "  ";
/// The fewest columns a shrunk title keeps.
const TITLE_FLOOR: usize = 8;
/// The most lines of profiles that are not listed before they are summed up.
const MAX_UNLISTED: usize = 2;

pub(super) fn columns_of(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// The text cut to `width` columns: the longest prefix of whole graphemes that fits in
/// `width - 1` columns, then the ellipsis. Hermes text is made printable first. With `pad`,
/// text that fits is padded with spaces to `width`.
pub(super) fn cut(text: &str, width: usize, pad: bool) -> String {
    let clean = sanitize(text);
    let used = columns_of(&clean);
    if used <= width {
        let mut out = clean.into_owned();
        if pad {
            out.push_str(&" ".repeat(width - used));
        }
        return out;
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut taken = 0;
    for grapheme in clean.graphemes(true) {
        let step = usize::from(grapheme_width(grapheme));
        if taken + step > width - 1 {
            break;
        }
        out.push_str(grapheme);
        taken += step;
    }
    out.push('\u{2026}');
    if pad {
        out.push_str(&" ".repeat(width.saturating_sub(taken + 1)));
    }
    out
}

/// The end of the text that fits in `width` columns, with an ellipsis in front when the start
/// was cut, and the columns it takes.
pub(super) fn cut_tail(text: &str, width: usize) -> (String, usize) {
    let used = columns_of(text);
    if used <= width {
        return (text.to_owned(), used);
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut taken = 1;
    for grapheme in text.graphemes(true).rev() {
        let step = usize::from(grapheme_width(grapheme));
        if taken + step > width {
            break;
        }
        kept.push(grapheme);
        taken += step;
    }
    kept.reverse();
    (format!("\u{2026}{}", kept.concat()), taken)
}

/// The text broken at spaces into lines of at most `width` columns.
pub(super) fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if columns_of(line) + 1 + columns_of(word) <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

/// Write `text` at `x`, `y`, no wider than `width` columns.
pub(super) fn draw_text(buf: &mut Buffer, x: u16, y: u16, text: &str, width: usize, style: Style) {
    buf.set_stringn(x, y, text, width, style);
}

/// The header line, and the link note with its column offset when there is room for it.
pub(super) struct Header {
    pub(super) left: String,
    pub(super) note: Option<(u16, String)>,
}

pub(super) fn header_text(state: &FleetViewState, width: usize) -> Header {
    let mut left = format!("Fleet \u{b7} {} sessions", state.row_count());
    if let ProfileFilter::One(profile) = state.profile_filter() {
        left.push_str(&format!(" \u{b7} profile {}", sanitize(profile)));
    }
    if !state.filter_text().is_empty() {
        left.push_str(&format!(
            " \u{b7} filter \"{}\"",
            sanitize(state.filter_text())
        ));
    }
    let note = state.link_note().map(|note| sanitize(note).into_owned());
    match note {
        Some(note) if columns_of(&left) + columns_of(&note) < width => {
            let at = u16::try_from(width - columns_of(&note)).unwrap_or(0);
            Header {
                left,
                note: Some((at, note)),
            }
        }
        _ => Header {
            left: cut(&left, width, false),
            note: None,
        },
    }
}

/// One line for each profile that is not listed, at most two, then a summary line.
pub(super) fn not_listed_lines(profiles: &[(String, ProfileStatus)]) -> Vec<String> {
    let mut lines: Vec<String> = profiles
        .iter()
        .filter_map(|(name, status)| {
            let reason = match status {
                ProfileStatus::Listed { .. } | ProfileStatus::Loading => return None,
                ProfileStatus::Failed { reason } | ProfileStatus::Parked { reason } => {
                    sanitize(reason).into_owned()
                }
                ProfileStatus::OutOfScope => "out of scope".to_owned(),
            };
            Some(format!("not listed: {} ({reason})", sanitize(name)))
        })
        .collect();
    if lines.len() > MAX_UNLISTED {
        let extra = lines.len() - MAX_UNLISTED;
        lines.truncate(MAX_UNLISTED);
        lines.push(format!("\u{2026} and {extra} more profiles not listed"));
    }
    lines
}

/// The columns a width allows.
pub(super) struct Columns {
    profile: bool,
    model: bool,
    pr: bool,
    cost: bool,
    title: usize,
    detail: usize,
}

impl Columns {
    pub(super) fn new(
        groups: &[VisibleGroup<'_>],
        labels: &BTreeMap<RowKey, PrLabel>,
        width: u16,
    ) -> Self {
        let mut profiles: Vec<&str> = groups
            .iter()
            .flat_map(|group| group.rows.iter().map(|(row, _)| row.key.profile.as_str()))
            .collect();
        profiles.sort_unstable();
        profiles.dedup();
        let profile = width >= PROFILE_MIN_WIDTH && profiles.len() > 1;
        let model = width >= MODEL_MIN_WIDTH;
        let pr = width >= PR_MIN_WIDTH
            && groups
                .iter()
                .flat_map(|group| group.rows.iter())
                .any(|(row, _)| labels.contains_key(&row.key));
        let cost = width >= COST_MIN_WIDTH;
        let count =
            5 + usize::from(profile) + usize::from(model) + usize::from(pr) + usize::from(cost);
        let others = 2 * MARK_WIDTH
            + AGE_WIDTH
            + if profile { PROFILE_WIDTH } else { 0 }
            + if model { MODEL_WIDTH } else { 0 }
            + if pr { PR_WIDTH } else { 0 }
            + if cost { COST_WIDTH } else { 0 }
            + (count - 1) * GAP.len();
        let room = usize::from(width).saturating_sub(others);
        let title = TITLE_WIDTH.min(room.max(TITLE_FLOOR));
        let detail = room.saturating_sub(title);
        Self {
            profile,
            model,
            pr,
            cost,
            title,
            detail,
        }
    }
}

/// One line of the list before it is drawn.
pub(super) enum BodyLine<'a> {
    Group(RowState, usize),
    Row(&'a FleetRow, &'a str),
    Empty(String),
}

fn group_name(state: RowState) -> &'static str {
    match state {
        RowState::NeedsYou => "NEEDS YOU",
        RowState::Working => "WORKING",
        RowState::Failed => "FAILED",
        RowState::Done => "DONE",
        RowState::Idle => "IDLE",
    }
}

/// The list, and the place of the cursor row in it.
pub(super) fn body_lines<'a>(
    state: &FleetViewState,
    groups: &'a [VisibleGroup<'a>],
) -> (Vec<BodyLine<'a>>, Option<usize>) {
    let mut lines = Vec::new();
    let mut cursor = None;
    for group in groups {
        lines.push(BodyLine::Group(group.state, group.rows.len()));
        for (row, detail) in &group.rows {
            if state.cursor() == Some(&row.key) {
                cursor = Some(lines.len());
            }
            lines.push(BodyLine::Row(row, detail));
        }
    }
    if groups.is_empty() {
        lines.push(BodyLine::Empty(empty_text(state)));
    }
    (lines, cursor)
}

fn empty_text(state: &FleetViewState) -> String {
    if !state.filter_text().is_empty() {
        return format!("No sessions match \"{}\".", sanitize(state.filter_text()));
    }
    match state.profile_filter() {
        ProfileFilter::One(profile) => {
            format!("No live sessions in profile {}.", sanitize(profile))
        }
        ProfileFilter::All => "No live sessions in the served profiles.".to_owned(),
    }
}

/// Draw one list line at `x`, `y`.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_body_line(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    line: &BodyLine<'_>,
    columns: &Columns,
    cursor: Option<&RowKey>,
    state: &FleetViewState,
    labels: &BTreeMap<RowKey, PrLabel>,
    theme: &FleetTheme,
) {
    let width = usize::from(buf.area.right().saturating_sub(x));
    match line {
        BodyLine::Group(group, count) => {
            let text = format!("{} ({count})", group_name(*group));
            draw_text(buf, x, y, &text, width, theme.group);
        }
        BodyLine::Empty(text) => {
            draw_text(buf, x, y, &cut(text, width, false), width, theme.notice)
        }
        BodyLine::Row(row, detail) => {
            let on_cursor = cursor == Some(&row.key);
            let mut style = theme.row_style(row.state, row.stale);
            if on_cursor {
                style = style.patch(theme.cursor);
            }
            let label = labels.get(&row.key);
            let (text, pr_at) = row_text(row, detail, columns, on_cursor, state, label);
            draw_text(buf, x, y, &text, width, style);
            // The label is drawn over its blank cell so that its tone can differ from the row's.
            if let (Some(at), Some(label)) = (pr_at, label) {
                let at = u16::try_from(at).unwrap_or(u16::MAX);
                let cell = style.patch(theme.label_style(label.tone));
                let text = cut(&label.text, PR_WIDTH, true);
                draw_text(buf, x.saturating_add(at), y, &text, PR_WIDTH, cell);
            }
        }
    }
}

/// The row's text, and the column its pull-request cell starts at when the column is shown.
fn row_text(
    row: &FleetRow,
    detail: &str,
    columns: &Columns,
    on_cursor: bool,
    state: &FleetViewState,
    label: Option<&PrLabel>,
) -> (String, Option<usize>) {
    let mut parts = vec![
        if on_cursor { "\u{203a}" } else { " " }.to_owned(),
        glyph(row.state).to_string(),
        title_cell(row, columns.title, state),
    ];
    if columns.profile {
        parts.push(cut(&row.key.profile, PROFILE_WIDTH, true));
    }
    if columns.model {
        let model = row
            .model
            .as_deref()
            .filter(|model| !model.is_empty())
            .map_or("-", short_model);
        parts.push(cut(model, MODEL_WIDTH, true));
    }
    let mut pr_at = None;
    if columns.pr {
        pr_at = Some(columns_of(&parts.join(GAP)) + GAP.len());
        parts.push(cut(
            label.map_or("", |label| label.text.as_str()),
            PR_WIDTH,
            true,
        ));
    }
    parts.push(cut(detail, columns.detail, true));
    parts.push(cut(
        &format_age(Some(row.state_since), state.now()),
        AGE_WIDTH,
        true,
    ));
    if columns.cost {
        let cost = row
            .cost_usd
            .map(|value| format!("${value:.2}"))
            .unwrap_or_default();
        parts.push(cut(&cost, COST_WIDTH, true));
    }
    (parts.join(GAP), pr_at)
}

/// The title with its marker, cut so that the marker stays.
fn title_cell(row: &FleetRow, width: usize, state: &FleetViewState) -> String {
    let suffix = if row.foreground || state.foreground() == Some(&row.key) {
        " (here)"
    } else if row.gone {
        " (ended)"
    } else if row.unlisted {
        " (hidden)"
    } else {
        ""
    };
    let title = sanitize(&title_or_untitled(row)).into_owned();
    if columns_of(&title) + suffix.len() <= width {
        return cut(&format!("{title}{suffix}"), width, true);
    }
    if suffix.len() < width {
        let kept = cut(&title, width - suffix.len(), false);
        return cut(&format!("{kept}{suffix}"), width, true);
    }
    cut(&title, width, true)
}
