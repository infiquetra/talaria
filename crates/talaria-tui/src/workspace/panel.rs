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
    if area.is_empty() {
        return;
    }
    Clear.render(area, buffer);
    for row in area.y..area.bottom() {
        buffer.set_stringn(area.x, row, "│", 1, Style::default());
    }
    let content = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(1),
        area.height,
    );
    if content.is_empty() {
        return;
    }
    buffer.set_stringn(
        content.x,
        content.y,
        "Work",
        usize::from(content.width),
        Style::default().add_modifier(Modifier::BOLD),
    );
    if let Some(checklist) = checklist {
        let offset = if content.height > 2 { 2 } else { 1 };
        let height = content.height.saturating_sub(offset).min(PANEL_MAX_ROWS);
        if height > 0 {
            ChecklistSection::new(checklist, expanded, height, hint, style).render(
                Rect::new(content.x, content.y + offset, content.width, height),
                buffer,
            );
        }
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

/// What the work panel is asked to show. The frame ignores this until the rows exist.
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
/// Empty until the sections are drawn.
#[must_use]
pub fn content_lines(work: &WorkContents<'_>, width: u16, height: u16) -> Vec<String> {
    let _ = (work, width, height);
    Vec::new()
}
