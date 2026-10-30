//! The peek panel: the latest output of one session, or its open card, under the list.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use talaria_core::directory::rows::format_age;
use talaria_core::fleet::{FleetRow, Outcome, RowState};
use talaria_core::fleet_view::{FleetViewState, glyph, row_detail, title_or_untitled};
use talaria_core::session::{Cell, NoticeKind, PromptState, SessionState, TurnOutcome, TurnState};

use super::rows::cut;
use super::{FleetTheme, Panel, PeekPanel};
use crate::history::{CellLines, sanitize, sanitize_line, wrap_lines};
use crate::live::LiveSection;
use crate::markdown::{MarkdownTheme, cells::MarkdownCellLines};
use crate::prompts::PromptStack;
use crate::tool_cell::{Detail, ToolCellLines, ToolRenderOptions, ToolTheme};

/// The line above the panel.
const SEPARATOR: &str = "\u{2500}\u{2500} peek \u{2500}\u{2500}";
/// Rows the separator and the header take.
const CHROME_ROWS: usize = 2;
/// The longest title a line names before it is cut.
const TITLE_CHARS: usize = 40;
/// Shown when a failed turn left no error text.
const FAILED_FALLBACK: &str = "Hermes could not finish the turn.";

/// How the latest turn ended.
enum Ended {
    Completed,
    Interrupted,
    Failed,
}

/// Draw the panel at `x`, `y` in at most `room` rows. Returns the rows used.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_panel(
    state: &FleetViewState,
    panel: &Panel<'_>,
    x: u16,
    y: u16,
    width: u16,
    room: usize,
    buf: &mut Buffer,
    theme: &FleetTheme,
) -> usize {
    let rows = u16::try_from(room).unwrap_or(u16::MAX);
    if rows == 0 || width == 0 {
        return 0;
    }
    let mut scratch = Buffer::empty(Rect::new(0, 0, width, rows));
    let used = render(state, panel, &mut scratch, theme).min(room);
    for row in 0..u16::try_from(used).unwrap_or(0) {
        for column in 0..width {
            buf[(x + column, y + row)] = scratch[(column, row)].clone();
        }
    }
    used
}

/// The hint line of the first card, read from the card's own rendering.
pub(super) fn card_hint(stack: &PromptStack, width: u16) -> Option<String> {
    let section = stack.section()?;
    let height = section.height(width);
    if height == 0 {
        return None;
    }
    let mut scratch = Buffer::empty(Rect::new(0, 0, width, height));
    section.render(*scratch.area(), &mut scratch);
    let last = height - 1;
    let text: String = (0..width).map(|x| scratch[(x, last)].symbol()).collect();
    Some(text.trim_end().to_owned())
}

fn render(
    state: &FleetViewState,
    panel: &Panel<'_>,
    scratch: &mut Buffer,
    theme: &FleetTheme,
) -> usize {
    let width = usize::from(scratch.area.width);
    let height = usize::from(scratch.area.height);
    let key = state.peek().map(|(key, _)| key).or_else(|| state.cursor());
    let row = key.and_then(|key| state.find_row(key));
    let now = match panel {
        Panel::Session(panel) => panel.now,
        Panel::External(panel) => panel.now,
        _ => state.now(),
    };
    let mut y = 0usize;
    let put = |scratch: &mut Buffer, text: &str, style: Style, y: &mut usize| {
        if *y < height {
            scratch.set_stringn(0, u16::try_from(*y).unwrap_or(0), text, width, style);
            *y += 1;
        }
    };
    put(scratch, SEPARATOR, theme.dim, &mut y);
    let header = match (row, key) {
        (Some(row), _) => header_line(row, now),
        (None, Some(key)) => format!(
            "\u{b7} (ended) \u{b7} {} \u{b7} {}",
            sanitize(&key.profile),
            sanitize(&key.session_id)
        ),
        (None, None) => String::new(),
    };
    let header_style = row.map_or(theme.header, |row| theme.row_style(row.state, false));
    put(scratch, &cut(&header, width, false), header_style, &mut y);

    match panel {
        Panel::Opening => {
            let title = row.map_or_else(|| "(untitled)".to_owned(), title_or_untitled);
            let text = format!("Opening {}\u{2026}", cut(&title, TITLE_CHARS, false));
            put(scratch, &cut(&text, width, false), theme.dim, &mut y);
        }
        Panel::Failed(text) => {
            for line in wrap_lines(
                vec![Line::from(sanitize(text).into_owned())],
                width_u16(width),
            ) {
                put(scratch, &line.to_string(), theme.failed, &mut y);
            }
        }
        Panel::Terminal(text) => {
            let lines = text
                .split('\n')
                .map(|line| Line::from(sanitize(line).into_owned()))
                .collect();
            for line in wrap_lines(lines, width_u16(width)) {
                put(scratch, &line.to_string(), theme.header, &mut y);
            }
        }
        Panel::Session(panel) => {
            session_body(panel, row, scratch, theme, &mut y, height.max(CHROME_ROWS));
        }
        Panel::External(panel) => external_body(panel, scratch, theme, &mut y, height),
    }
    y.min(height)
}

fn external_body(
    panel: &super::ExternalPeekPanel<'_>,
    scratch: &mut Buffer,
    theme: &FleetTheme,
    y: &mut usize,
    height: usize,
) {
    use talaria_core::fleet::acp::AcpCell;
    let width = usize::from(scratch.area.width);
    let mut headers = Vec::new();
    if let Some(usage) = &panel.mapping.usage {
        let mut text = format!("ctx {}/{}", usage.used, usage.size);
        if let (Some(amount), Some(currency)) = (usage.cost_amount, usage.cost_currency.as_deref())
        {
            text.push_str(&format!(" - cost {amount} {currency}"));
        }
        headers.push(text);
    }
    let mut capabilities = Vec::new();
    if let Some(mode) = &panel.mapping.mode {
        capabilities.push(format!("mode: {mode}"));
    }
    if !panel.mapping.commands.is_empty() {
        capabilities.push(format!("commands: {}", panel.mapping.commands.join(", ")));
    }
    if !capabilities.is_empty() {
        headers.push(capabilities.join(" - "));
    }
    let mut lines = Vec::new();
    for cell in &panel.mapping.transcript {
        let text = match cell {
            AcpCell::Text(text)
            | AcpCell::Thought(text)
            | AcpCell::Notice(text)
            | AcpCell::Plan(text) => text.clone(),
            AcpCell::Tool {
                title,
                body,
                status,
                ..
            } => format!("{title}: {body} {}", status.as_deref().unwrap_or_default()),
        };
        lines.extend(
            wrap_lines(
                vec![Line::raw(sanitize(&text).into_owned())],
                width_u16(width),
            )
            .into_iter()
            .map(|line| line.to_string()),
        );
    }
    let room = height.saturating_sub(*y);
    let minimum_permission = panel
        .mapping
        .needs
        .first()
        .map_or(0, |need| 1 + usize::from(!need.options.is_empty()));
    headers.truncate(room.saturating_sub(minimum_permission));
    let body_room = room.saturating_sub(headers.len());
    if let Some(need) = panel.mapping.needs.first() {
        let permission_rows = body_room.min(need.options.len().saturating_add(1));
        let transcript_rows = body_room.saturating_sub(permission_rows);
        let mut visible = lines
            .into_iter()
            .rev()
            .take(transcript_rows)
            .collect::<Vec<_>>();
        visible.reverse();
        if permission_rows > 0 {
            visible.push(format!("Permission: {}", need.tool_title));
        }
        let option_rows = permission_rows.saturating_sub(1);
        let start = panel
            .selected_option
            .saturating_sub(option_rows / 2)
            .min(need.options.len().saturating_sub(option_rows));
        for (index, option) in need
            .options
            .iter()
            .enumerate()
            .skip(start)
            .take(option_rows)
        {
            let marker = if index == panel.selected_option {
                ">"
            } else {
                " "
            };
            visible.push(format!("{marker} {}. {}", index + 1, option.name));
        }
        lines = visible;
    } else {
        lines = lines.into_iter().rev().take(body_room).collect();
        lines.reverse();
    }
    headers.extend(lines);
    for line in &headers {
        scratch.set_stringn(
            0,
            u16::try_from(*y).unwrap_or(0),
            cut(&sanitize(line), width, false),
            width,
            theme.idle,
        );
        *y += 1;
    }
}

fn width_u16(width: usize) -> u16 {
    u16::try_from(width).unwrap_or(u16::MAX)
}

/// `<glyph> <title> · <profile> · <model> · <state word> <age>`.
fn header_line(row: &FleetRow, now: f64) -> String {
    let word = match row.state {
        RowState::NeedsYou => "needs you".to_owned(),
        RowState::Working => "working".to_owned(),
        RowState::Failed | RowState::Done | RowState::Idle => row_detail(row, &[]),
    };
    let model = row
        .model
        .as_deref()
        .filter(|model| !model.is_empty())
        .unwrap_or("-");
    format!(
        "{} {} \u{b7} {} \u{b7} {} \u{b7} {} {}",
        glyph(row.state),
        title_or_untitled(row),
        row.key.profile,
        model,
        word,
        format_age(Some(row.state_since), now),
    )
}

fn session_body(
    panel: &PeekPanel<'_>,
    row: Option<&FleetRow>,
    scratch: &mut Buffer,
    theme: &FleetTheme,
    y: &mut usize,
    height: usize,
) {
    let width = scratch.area.width;
    let session = panel.session;
    let running = matches!(session.turn(), TurnState::Running);
    let ended = if running {
        None
    } else {
        ended_of(session, row)
    };
    let mut footer: Vec<(String, Style)> = Vec::new();
    if running {
        footer.push((activity_line(session, usize::from(width)), theme.working));
    }
    if let Some(ended) = &ended {
        footer.push(outcome_line(ended, session, usize::from(width), theme));
    }
    let room = height.saturating_sub(*y).saturating_sub(footer.len());

    let section = panel.stack.and_then(PromptStack::section);
    if let Some(section) = section {
        let shown = usize::from(section.height(width)).min(room);
        if shown > 0 {
            let area = Rect::new(0, u16::try_from(*y).unwrap_or(0), width, width_u16(shown));
            section.render(area, scratch);
            *y += shown;
        }
    } else {
        let lines = tail_lines(session, width, ended.is_some());
        let skip = lines.len().saturating_sub(room);
        for line in lines.into_iter().skip(skip) {
            scratch.set_line(0, u16::try_from(*y).unwrap_or(0), &line, width);
            *y += 1;
        }
    }
    for (text, style) in footer {
        if *y < height {
            scratch.set_stringn(
                0,
                u16::try_from(*y).unwrap_or(0),
                &text,
                usize::from(width),
                style,
            );
            *y += 1;
        }
    }
}

fn ended_of(session: &SessionState, row: Option<&FleetRow>) -> Option<Ended> {
    if let TurnState::Finished(outcome) = session.turn() {
        return Some(match outcome {
            TurnOutcome::Completed => Ended::Completed,
            TurnOutcome::Interrupted => Ended::Interrupted,
            TurnOutcome::Failed => Ended::Failed,
        });
    }
    let row = row?;
    match (row.state, row.outcome) {
        (RowState::Done | RowState::Failed, Some(Outcome::Completed)) => Some(Ended::Completed),
        (RowState::Done | RowState::Failed, Some(Outcome::Interrupted)) => Some(Ended::Interrupted),
        (RowState::Done | RowState::Failed, Some(Outcome::Failed)) | (RowState::Failed, _) => {
            Some(Ended::Failed)
        }
        _ => None,
    }
}

/// `✓ completed`, `■ interrupted` or `✗ failed: <error>`, with its style.
fn outcome_line(
    ended: &Ended,
    session: &SessionState,
    width: usize,
    theme: &FleetTheme,
) -> (String, Style) {
    let (text, style) = match ended {
        Ended::Completed => ("\u{2713} completed".to_owned(), theme.done),
        Ended::Interrupted => ("\u{25a0} interrupted".to_owned(), theme.idle),
        Ended::Failed => {
            let error = error_text(session).unwrap_or(FAILED_FALLBACK);
            (
                format!("\u{2717} failed: {}", one_line(error)),
                theme.failed,
            )
        }
    };
    (cut(&text, width, false), style)
}

/// The error notice of the latest turn, if it has one.
fn error_text(session: &SessionState) -> Option<&str> {
    session.cells().iter().rev().find_map(|cell| match cell {
        Cell::Notice(notice) if notice.kind == NoticeKind::Error => Some(notice.text.as_str()),
        _ => None,
    })
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `✻ <activity>` or `✻ working`.
fn activity_line(session: &SessionState, width: usize) -> String {
    let activity = session
        .activity()
        .and_then(|text| {
            text.lines()
                .rev()
                .map(str::trim)
                .find(|line| !line.is_empty())
        })
        .unwrap_or("working");
    cut(&format!("\u{273b} {activity}"), width, false)
}

/// The cells of the latest turn as wrapped lines. When the turn has an outcome line, its own
/// interrupted and error notices are left out because that line says the same.
fn tail_lines(session: &SessionState, width: u16, ended: bool) -> Vec<Line<'static>> {
    let cells = session.cells();
    let start = cells
        .iter()
        .rposition(|cell| matches!(cell, Cell::User(_)))
        .unwrap_or(0);
    let last_error = cells
        .iter()
        .rposition(|cell| matches!(cell, Cell::Notice(n) if n.kind == NoticeKind::Error));
    let renderer = ToolCellLines::new(
        Box::new(MarkdownCellLines::new(true, MarkdownTheme::default())),
        ToolRenderOptions {
            width,
            theme: ToolTheme::default(),
            color: true,
            detail: Detail::Compact,
            expand_hint: None,
            elapsed: None,
        },
    );
    let mut out = Vec::new();
    for (index, cell) in cells.iter().enumerate().skip(start) {
        let hide = match cell {
            Cell::Prompt(prompt) => {
                matches!(prompt.state, PromptState::Open | PromptState::Answering(_))
            }
            Cell::Notice(notice) if ended => {
                notice.kind == NoticeKind::Interrupted
                    || (notice.kind == NoticeKind::Error && Some(index) == last_error)
            }
            _ => false,
        };
        if hide {
            continue;
        }
        let lines: Vec<Line<'static>> = renderer
            .lines(cell, width)
            .into_iter()
            .map(sanitize_line)
            .collect();
        out.extend(wrap_lines(lines, width));
    }
    out
}
