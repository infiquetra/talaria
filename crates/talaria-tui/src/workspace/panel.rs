//! The work panel's frame and hosted checklist.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Clear, Widget},
};
use talaria_core::checklist::Checklist;

use crate::{
    checklist::{ChecklistSection, ChecklistStyle, PANEL_MAX_ROWS},
    live::LiveSection,
    theme::surfaces::{ViewStyles, tokens as t},
};

/// Draw an opaque panel over the transcript when necessary. Content starts after
/// its one-column divider; the footer never belongs to this rectangle.
pub fn draw(
    area: Rect,
    buffer: &mut Buffer,
    checklist: Option<&Checklist>,
    expanded: bool,
    style: &ChecklistStyle,
    hint: Option<String>,
) {
    draw_with_theme(
        area,
        buffer,
        checklist,
        expanded,
        style,
        hint,
        &ViewStyles::default(),
    );
}

/// Draw a checklist-hosting panel with an explicit resolved palette.
pub fn draw_with_theme(
    area: Rect,
    buffer: &mut Buffer,
    checklist: Option<&Checklist>,
    expanded: bool,
    style: &ChecklistStyle,
    hint: Option<String>,
    views: &ViewStyles,
) {
    if area.is_empty() {
        return;
    }
    Clear.render(area, buffer);
    for row in area.y..area.bottom() {
        buffer.set_stringn(
            area.x,
            row,
            views.divider_symbol("│"),
            1,
            views.style(t::WORKSPACE_DIVIDER, Style::default()),
        );
    }
    let content = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(1),
        area.height,
    );
    if content.is_empty() {
        views.finish(area, buffer, t::WORK_BACKGROUND, true);
        return;
    }
    buffer.set_stringn(
        content.x,
        content.y,
        "Work",
        usize::from(content.width),
        views.style(t::WORK_TITLE, Style::default().add_modifier(Modifier::BOLD)),
    );
    let style = checklist_style(style, views);
    if let Some(checklist) = checklist {
        let offset = if content.height > 2 { 2 } else { 1 };
        let height = content.height.saturating_sub(offset).min(PANEL_MAX_ROWS);
        if height > 0 {
            ChecklistSection::new(checklist, expanded, height, hint, &style).render(
                Rect::new(content.x, content.y + offset, content.width, height),
                buffer,
            );
        }
    }
    views.finish(area, buffer, t::WORK_BACKGROUND, true);
}

pub(crate) fn checklist_style(style: &ChecklistStyle, views: &ViewStyles) -> ChecklistStyle {
    ChecklistStyle {
        header: views.inline(t::WORK_SECTION, style.header),
        hint: views.inline(t::WORK_HELP, style.hint),
        in_progress: views.inline(t::WORK_TASK_TEXT, style.in_progress),
        pending: views.inline(t::WORK_TASK_TEXT, style.pending),
        completed: views.inline(t::WORK_TASK_TEXT, style.completed),
        cancelled: views.inline(t::WORK_TASK_CANCELLED, style.cancelled),
        glyph_in_progress: views.inline(t::WORK_TASK_RUNNING, style.glyph_in_progress),
        glyph_completed: views.inline(t::WORK_TASK_DONE, style.glyph_completed),
    }
}

/// Content columns inside the divider. The panel allocation is one wider.
pub const CONTENT_COLUMNS: u16 = 32;

/// One fleet request the panel lists before the focused session's children.
/// Foreground prompts are already in the attention count and are not repeated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaitingRequest {
    /// Queue order. The oldest request is first, matching the jump key.
    pub seq: u64,
    /// The request's own label.
    pub label: String,
    /// True when the request belongs to the focused session.
    pub foreground: bool,
}

/// What the work panel is asked to show.
#[derive(Clone)]
pub struct WorkContents<'a> {
    /// The status row's needs-you count. The panel does not recount.
    pub needs_you: u32,
    /// The focused session's goal, when one is still available to the panel.
    pub goal: Option<&'a talaria_core::automation::GoalView>,
    /// The focused session's turn, in the status row's words.
    pub turn: crate::status::TurnState,
    /// The explicit verdict belongs to the turn before the one now running.
    pub verdict_previous_turn: bool,
    /// The hosted checklist. The same list the frame already draws.
    pub checklist: Option<&'a talaria_core::checklist::Checklist>,
    /// Ctrl+T's expanded state for that checklist.
    pub checklist_expanded: bool,
    /// Fleet requests, including any the panel must skip.
    pub waiting: &'a [WaitingRequest],
    /// The focused session's children.
    pub agents: &'a talaria_core::subagents::SubagentTree,
    /// The parent session's model. A child never inherits it.
    pub parent_model: Option<&'a str>,
    /// How many recent completions the agents section may spell out.
    pub recent_limit: usize,
    /// Voice phase. It stays in the status row.
    pub voice_phase: Option<&'a str>,
    /// Read-aloud state. It stays in the status row.
    pub read_aloud: Option<&'a str>,
    /// Voice target. It stays in the status row.
    pub voice_target: Option<&'a str>,
}

/// The panel's content rows at `width` columns, at most `height` of them.
#[must_use]
pub fn content_lines(work: &WorkContents<'_>, width: u16, height: u16) -> Vec<String> {
    use crate::{checklist::cut_to_width, history::sanitize};
    use talaria_core::{checklist::TodoStatus, subagents::RowState};
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let literal = |s: &str| sanitize(&s.replace(['\n', '\r', '\t'], " ")).into_owned();
    let fit = |s: String| cut_to_width(&s, width);
    let mut header = vec![if work.needs_you == 0 {
        "Work".into()
    } else {
        format!(
            "Work / {} need{} you",
            work.needs_you,
            if work.needs_you == 1 { "s" } else { "" }
        )
    }];
    if let Some(goal) = work.goal {
        let state = if goal.wait.is_some() {
            "parked"
        } else {
            goal.status.as_str()
        };
        header.push(format!("Goal: {}", literal(&goal.title)));
        if state == "parked" || state == "paused" {
            let reason = match &goal.wait {
                Some(
                    talaria_core::automation::WaitView::Until { reason, .. }
                    | talaria_core::automation::WaitView::Session { reason, .. }
                    | talaria_core::automation::WaitView::Pid { reason, .. },
                ) => Some(reason.as_str()),
                None => goal.paused_reason.as_deref(),
            }
            .unwrap_or("");
            header.push(format!("{state}: {}", literal(reason)));
        }
    }
    let turn = match work.turn {
        crate::status::TurnState::Idle => "idle",
        crate::status::TurnState::Streaming => "streaming",
        crate::status::TurnState::Waiting => "waiting",
        crate::status::TurnState::Cancelled => "cancelled",
    };
    header.push(format!("Turn: {turn}"));
    let verdict = work.goal.and_then(|goal| goal.panel_verdict());
    let label = verdict.map_or_else(
        || {
            if work
                .goal
                .is_some_and(|g| g.status == "active" || g.status == "paused")
            {
                "pending".to_owned()
            } else {
                "unknown".to_owned()
            }
        },
        &literal,
    );
    header.push(format!("Verdict: {label}"));
    if verdict.is_some() && work.verdict_previous_turn {
        header.push("previous turn".into());
    }
    let mut groups: Vec<(String, Vec<Vec<String>>, usize)> = Vec::new();
    if let Some(checklist) = work.checklist {
        let counts = checklist.counts();
        let title = format!(
            "{} Tasks {}/{}",
            if work.checklist_expanded {
                "▾"
            } else {
                "▸"
            },
            counts.completed,
            counts.total
        );
        let items = if work.checklist_expanded {
            checklist
                .tree()
                .iter()
                .map(|row| {
                    let item = &checklist.items[row.index];
                    let mark = match item.status {
                        TodoStatus::Pending => "[ ]",
                        TodoStatus::InProgress => "[>]",
                        TodoStatus::Completed => "[x]",
                        TodoStatus::Cancelled => "[-]",
                    };
                    vec![format!(
                        "{}{} {}",
                        " ".repeat(2 + 2 * usize::from(row.depth.min(4))),
                        mark,
                        literal(&item.content)
                    )]
                })
                .collect()
        } else {
            Vec::new()
        };
        groups.push((title, items, 0));
    }
    let mut waiting: Vec<_> = work
        .waiting
        .iter()
        .filter(|item| !item.foreground)
        .collect();
    waiting.sort_by_key(|item| item.seq);
    let rows = work.agents.rows_in_order();
    let counts = work.agents.counts();
    let mut agent_items: Vec<Vec<String>> = waiting
        .iter()
        .map(|item| {
            vec![
                format!("! {}", literal(&item.label)),
                "  model: unknown".into(),
                "  effort: unknown".into(),
            ]
        })
        .collect();
    for status in [RowState::Running, RowState::Queued] {
        for row in rows
            .iter()
            .map(|r| work.agents.row(r.index))
            .filter(|r| r.state == status)
        {
            let label = if status == RowState::Running {
                "running"
            } else {
                "queued"
            };
            agent_items.push(vec![
                format!("> {} / {label}", literal(&row.goal)),
                format!(
                    "  model: {}",
                    literal(row.model.as_deref().unwrap_or("unknown"))
                ),
                "  effort: unknown".into(),
            ]);
        }
    }
    let recent = work.agents.recent_completions(work.recent_limit);
    let omitted_recent = work
        .agents
        .recent_completion_count()
        .saturating_sub(recent.len());
    for row in recent {
        agent_items.push(vec![format!("✓ {}", literal(&row.goal))]);
    }
    if !agent_items.is_empty() || omitted_recent > 0 {
        groups.push((
            format!(
                "Agents / {} running, {} waiting",
                counts.live.saturating_sub(counts.queued),
                waiting.len()
            ),
            agent_items,
            omitted_recent,
        ));
    }
    let budget = usize::from(height);
    let minimum_groups: usize = groups
        .iter()
        .map(|(_, items, omitted)| 1 + usize::from(!items.is_empty() || *omitted > 0))
        .sum();
    if header.len() + minimum_groups > budget {
        header.truncate(budget.saturating_sub(minimum_groups).max(1));
    }
    let full = header.len()
        + groups
            .iter()
            .map(|(_, items, omitted)| {
                1 + items.iter().map(Vec::len).sum::<usize>() + usize::from(*omitted > 0)
            })
            .sum::<usize>()
        + groups.len();
    let spaced = full <= budget;
    if spaced && work.goal.is_some() && full + 2 <= budget {
        header.insert(1, String::new());
    }
    let mut out = header;
    for (index, (title, items, omitted)) in groups.iter().enumerate() {
        if spaced {
            out.push(String::new());
        }
        if out.len() >= budget {
            break;
        }
        out.push(title.clone());
        let reserve: usize = groups
            .iter()
            .skip(index + 1)
            .map(|(_, items, omitted)| 1 + usize::from(!items.is_empty() || *omitted > 0))
            .sum();
        let mut accounted = false;
        for (at, item) in items.iter().enumerate() {
            let left = budget.saturating_sub(out.len() + reserve);
            let remaining = items.len() - at + omitted;
            if item.len() + usize::from(remaining > 1) > left {
                if left > 0 {
                    out.push(format!("+{remaining} more"));
                }
                accounted = true;
                break;
            }
            out.extend(item.iter().cloned());
        }
        if !accounted && *omitted > 0 && out.len() + reserve < budget {
            out.push(format!("+{omitted} more"));
        }
    }
    if spaced && !work.agents.rows_in_order().is_empty() && out.len() < budget {
        out.push("Alt+S details / Alt+A next".into());
    }
    out.into_iter().take(budget).map(fit).collect()
}

/// Paint the panel, retaining the divider and limiting every row to its content area.
pub fn draw_work(area: Rect, buffer: &mut Buffer, work: &WorkContents<'_>) {
    draw_work_with_theme(area, buffer, work, &ViewStyles::default());
}

/// Paint each work field through its catalogue element.
pub fn draw_work_with_theme(
    area: Rect,
    buffer: &mut Buffer,
    work: &WorkContents<'_>,
    views: &ViewStyles,
) {
    if area.is_empty() {
        return;
    }
    Clear.render(area, buffer);
    for y in area.y..area.bottom() {
        buffer.set_stringn(
            area.x,
            y,
            views.divider_symbol("│"),
            1,
            views.style(t::WORKSPACE_DIVIDER, Style::default()),
        );
    }
    let width = area.width.saturating_sub(1);
    for (y, line) in content_lines(work, width, area.height).iter().enumerate() {
        buffer.set_line(
            area.x + 1,
            area.y + y as u16,
            &work_line(line, work, views),
            width,
        );
    }
    views.finish(area, buffer, t::WORK_BACKGROUND, true);
}

fn work_line(
    line: &str,
    work: &WorkContents<'_>,
    views: &ViewStyles,
) -> ratatui::text::Line<'static> {
    use ratatui::text::{Line, Span};
    if !views.active() {
        return Line::from(line.to_owned());
    }
    let span =
        |text: &str, token| Span::styled(text.to_owned(), views.style(token, Style::default()));
    if let Some(attention) = line.strip_prefix("Work / ") {
        return Line::from(vec![
            span("Work / ", t::WORK_TITLE),
            span(attention, t::WORK_ATTENTION),
        ]);
    }
    if line == "Work" {
        return Line::from(span(line, t::WORK_TITLE));
    }
    for (label, token) in [("Goal: ", t::WORK_GOAL), ("Turn: ", t::WORK_TURN)] {
        if let Some(value) = line.strip_prefix(label) {
            return Line::from(vec![span(label, t::WORK_LABEL), span(value, token)]);
        }
    }
    if let Some(value) = line.strip_prefix("Verdict: ") {
        let token = match work.goal.and_then(|goal| goal.panel_verdict()) {
            Some("success" | "passed" | "passing") => t::WORK_VERDICT_SUCCESS,
            Some("failure" | "failed" | "failing") => t::WORK_VERDICT_FAILURE,
            _ => t::WORK_VERDICT_PENDING,
        };
        return Line::from(vec![span("Verdict: ", t::WORK_LABEL), span(value, token)]);
    }
    let trimmed = line.trim_start();
    for (mark, token) in [
        ("[ ]", t::WORK_TASK_PENDING),
        ("[>]", t::WORK_TASK_RUNNING),
        ("[x]", t::WORK_TASK_DONE),
        ("[-]", t::WORK_TASK_CANCELLED),
    ] {
        if let Some(text) = trimmed.strip_prefix(mark) {
            let indent = &line[..line.len() - trimmed.len()];
            return Line::from(vec![
                span(indent, t::WORK_TASK_TEXT),
                span(mark, token),
                span(
                    text,
                    if mark == "[-]" {
                        t::WORK_TASK_CANCELLED
                    } else {
                        t::WORK_TASK_TEXT
                    },
                ),
            ]);
        }
    }
    for (prefix, token) in [
        ("! ", t::WORK_AGENT_WAITING),
        ("> ", t::WORK_AGENT_RUNNING),
        ("✓ ", t::WORK_AGENT_DONE),
    ] {
        if let Some(text) = line.strip_prefix(prefix) {
            let token = if text.ends_with(" / queued") {
                t::WORK_AGENT_QUEUED
            } else {
                token
            };
            if let Some((name, state)) = text.rsplit_once(" / ") {
                return Line::from(vec![
                    span(prefix, token),
                    span(name, t::WORK_AGENT_NAME),
                    span(&format!(" / {state}"), token),
                ]);
            }
            return Line::from(vec![span(prefix, token), span(text, t::WORK_AGENT_NAME)]);
        }
    }
    for label in ["  model: ", "  effort: "] {
        if let Some(value) = line.strip_prefix(label) {
            return Line::from(vec![
                span(label, t::WORK_AGENT_METADATA),
                span(
                    value,
                    if value == "unknown" {
                        t::WORK_AGENT_UNKNOWN
                    } else {
                        t::WORK_AGENT_METADATA
                    },
                ),
            ]);
        }
    }
    let token = if line.starts_with(['▾', '▸']) || line.starts_with("Agents /") {
        t::WORK_SECTION
    } else if line.starts_with('+') {
        t::WORK_OVERFLOW
    } else if line.starts_with("Alt+") {
        t::WORK_HELP
    } else if line.starts_with("parked:") || line.starts_with("paused:") {
        t::WORK_GOAL
    } else {
        t::WORK_LABEL
    };
    Line::from(span(line, token))
}
