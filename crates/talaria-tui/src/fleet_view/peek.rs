//! The peek panel: the latest output of one session, or its open card, under the list.

use crate::theme::surfaces::{ViewStyles, tokens as t};
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
use crate::history::{CellLines, sanitize, sanitize_line, word_wrapped, wrap_lines};
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
    views: Option<&ViewStyles>,
) -> usize {
    let rows = u16::try_from(room).unwrap_or(u16::MAX);
    if rows == 0 || width == 0 {
        return 0;
    }
    let mut scratch = Buffer::empty(Rect::new(0, 0, width, rows));
    let used = render(state, panel, &mut scratch, theme, views).min(room);
    for row in 0..u16::try_from(used).unwrap_or(0) {
        for column in 0..width {
            buf[(x + column, y + row)] = scratch[(column, row)].clone();
        }
    }
    used
}

fn render(
    state: &FleetViewState,
    panel: &Panel<'_>,
    scratch: &mut Buffer,
    theme: &FleetTheme,
    views: Option<&ViewStyles>,
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
    let separator = views.map_or_else(|| SEPARATOR.to_owned(), |v| v.divider_text(SEPARATOR));
    put(scratch, &separator, theme.dim, &mut y);
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
    let header = word_wrapped(&sanitize(&header), scratch.area.width);
    let header_rows = height.saturating_sub(3).clamp(1, 3);
    for (index, text) in header.iter().take(header_rows).enumerate() {
        let text = if index + 1 == header_rows && header.len() > header_rows {
            cut(&format!("{text}…"), width, false)
        } else {
            text.clone()
        };
        put(scratch, &text, header_style, &mut y);
    }
    if let Panel::Session(panel) = &panel
        && let Some(counts) = count_line(panel.subagents, panel.background)
    {
        put(scratch, &cut(&counts, width, false), theme.dim, &mut y);
    }

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
        Panel::Session(panel) => session_body(
            panel,
            row,
            scratch,
            theme,
            &mut y,
            height.max(CHROME_ROWS),
            views,
        ),
        Panel::External(panel) => external_body(panel, scratch, theme, &mut y, height, views),
    }
    y.min(height)
}

fn external_body(
    panel: &super::ExternalPeekPanel<'_>,
    scratch: &mut Buffer,
    theme: &FleetTheme,
    y: &mut usize,
    height: usize,
    views: Option<&ViewStyles>,
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
        headers.push((text, theme.idle));
    }
    let mut capabilities = Vec::new();
    if let Some(mode) = &panel.mapping.mode {
        capabilities.push(format!("mode: {mode}"));
    }
    if !panel.mapping.commands.is_empty() {
        capabilities.push(format!("commands: {}", panel.mapping.commands.join(", ")));
    }
    if !capabilities.is_empty() {
        headers.push((capabilities.join(" - "), theme.idle));
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
        let style = views.map_or(theme.idle, |v| {
            v.style(
                if matches!(cell, AcpCell::Tool { .. }) {
                    t::TOOL_OUTPUT
                } else {
                    t::FLEET_PEEK
                },
                theme.idle,
            )
        });
        lines.extend(
            wrap_lines(
                vec![Line::raw(sanitize(&text).into_owned())],
                width_u16(width),
            )
            .into_iter()
            .map(|line| (line.to_string(), style)),
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
            visible.push((format!("Permission: {}", need.tool_title), theme.idle));
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
            visible.push((
                format!("{marker} {}. {}", index + 1, option.name),
                theme.idle,
            ));
        }
        lines = visible;
    } else {
        lines = lines.into_iter().rev().take(body_room).collect();
        lines.reverse();
    }
    headers.extend(lines);
    for (line, style) in &headers {
        scratch.set_stringn(
            0,
            u16::try_from(*y).unwrap_or(0),
            cut(&sanitize(line), width, false),
            width,
            *style,
        );
        *y += 1;
    }
}

fn width_u16(width: usize) -> u16 {
    u16::try_from(width).unwrap_or(u16::MAX)
}

/// `3 sub-agents · 2 background`; a zero or unknown part is left out, and `None` when both are.
fn count_line(subagents: Option<u32>, background: Option<usize>) -> Option<String> {
    let mut parts = Vec::new();
    match subagents {
        Some(1) => parts.push("1 sub-agent".to_owned()),
        Some(n) if n > 0 => parts.push(format!("{n} sub-agents")),
        _ => {}
    }
    if let Some(m) = background.filter(|m| *m > 0) {
        parts.push(format!("{m} background"));
    }
    (!parts.is_empty()).then(|| parts.join(" \u{b7} "))
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
    views: Option<&ViewStyles>,
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
        let section = section.without_key_hints();
        let section = match views {
            Some(views) => section.with_theme(views),
            None => section,
        };
        let shown = usize::from(section.height(width)).min(room);
        if shown > 0 {
            let area = Rect::new(0, u16::try_from(*y).unwrap_or(0), width, width_u16(shown));
            section.render(area, scratch);
            *y += shown;
        }
    } else {
        let lines = tail_lines(session, width, ended.is_some(), views);
        let skip = lines.len().saturating_sub(room);
        for line in lines.into_iter().skip(skip) {
            let line = if let Some(v) = views {
                line.style(v.style(t::FLEET_PEEK, Style::default()))
            } else {
                line
            };
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

/// The cells of the latest turn as wrapped lines, from the cell that started it: the person's
/// prompt or the notice of a turn Hermes started. When the turn has an outcome line, its own
/// interrupted and error notices are left out because that line says the same.
fn tail_lines(
    session: &SessionState,
    width: u16,
    ended: bool,
    views: Option<&ViewStyles>,
) -> Vec<Line<'static>> {
    let cells = session.cells();
    let start = cells.iter().rposition(Cell::starts_turn).unwrap_or(0);
    let last_error = cells
        .iter()
        .rposition(|cell| matches!(cell, Cell::Notice(n) if n.kind == NoticeKind::Error));
    let mut markdown = MarkdownTheme::default();
    let mut tool = ToolTheme::default();
    if let Some(v) = views {
        macro_rules! apply { ($value:ident; $($field:ident => $token:ident),+ $(,)?) => { $( $value.$field = v.style(t::$token, $value.$field); )+ }; }
        apply!(markdown;
            heading1 => TRANSCRIPT_HEADING_1, heading2 => TRANSCRIPT_HEADING_2,
            heading3 => TRANSCRIPT_HEADING_3, strong => TRANSCRIPT_STRONG,
            emphasis => TRANSCRIPT_EMPHASIS, strikethrough => TRANSCRIPT_STRIKETHROUGH,
            inline_code => CODE_INLINE, link_text => TRANSCRIPT_LINK_TEXT,
            link_destination => TRANSCRIPT_LINK_DESTINATION, image_alt => TRANSCRIPT_IMAGE_ALT,
            quote_bar => TRANSCRIPT_QUOTE_BAR, rule => TRANSCRIPT_RULE,
            code_label => CODE_LABEL, table_border => TRANSCRIPT_TABLE_BORDER,
            table_header => TRANSCRIPT_TABLE_HEADER
        );
        apply!(tool;
            glyph_running => TOOL_RUNNING, glyph_ok => TOOL_SUCCESS,
            glyph_failed => TOOL_FAILED, glyph_muted => TOOL_MUTED,
            title => TOOL_TITLE, subject => TOOL_SUBJECT, meta => TOOL_METADATA,
            output => TOOL_OUTPUT, error => TOOL_ERROR, diff_added => DIFF_ADDED,
            diff_removed => DIFF_REMOVED, diff_context => DIFF_CONTEXT,
            diff_hunk => DIFF_HUNK, diff_file => DIFF_FILE, diff_note => DIFF_NOTE,
            line_number => DIFF_LINE_NUMBER
        );
    }
    let color = views.is_none_or(|v| !v.active() || v.color());
    let renderer = ToolCellLines::new(
        Box::new(MarkdownCellLines::new_with_theme(
            color,
            markdown,
            views.cloned().unwrap_or_default(),
        )),
        ToolRenderOptions {
            width,
            theme: tool,
            color,
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

#[cfg(test)]
mod tests {
    use super::tail_lines;
    use hermes_contract::schemas::CanonicalSnapshot;
    use serde_json::json;
    use talaria_core::session::SessionState;

    fn session(messages: serde_json::Value) -> SessionState {
        let snapshot: CanonicalSnapshot = serde_json::from_value(json!({
            "session_id": "s-peek",
            "stored_session_id": "stored-1",
            "messages": messages,
            "message_count": 4,
            "running": false,
            "authority_epoch": 1,
            "replay_epoch": "epoch-1",
            "last_sequence": 1,
            "subscription_id": "sub-1",
            "revision": 1,
            "execution_generation": 1,
            "pending": [],
            "prompts": [],
            "info": {"desktop_protocol": "1"}
        }))
        .expect("snapshot");
        SessionState::from_snapshot(snapshot)
    }

    fn text(session: &SessionState) -> String {
        tail_lines(session, 80, true, None)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_turn_hermes_started_is_the_latest_turn_without_the_previous_prompt() {
        let session = session(json!([
            {"role": "user", "text": "first question"},
            {"role": "assistant", "text": "first answer"},
            {"role": "user", "display_kind": "async_delegation_complete",
             "text": "synthetic subagent result"},
            {"role": "assistant", "text": "second answer"}
        ]));
        let shown = text(&session);
        assert!(shown.contains("second answer"), "{shown}");
        assert!(shown.contains("Subagent results returned"), "{shown}");
        assert!(!shown.contains("first question"), "{shown}");
        assert!(!shown.contains("first answer"), "{shown}");
    }

    #[test]
    fn a_typed_prompt_still_starts_the_latest_turn() {
        let session = session(json!([
            {"role": "user", "text": "first question"},
            {"role": "assistant", "text": "first answer"},
            {"role": "user", "text": "second question"},
            {"role": "assistant", "text": "second answer"}
        ]));
        let shown = text(&session);
        assert!(shown.contains("second question"), "{shown}");
        assert!(!shown.contains("first answer"), "{shown}");
    }
}
