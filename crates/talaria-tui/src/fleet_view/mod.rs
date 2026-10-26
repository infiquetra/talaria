//! Draw the fleet overlay: a header, grouped session rows, an optional peek panel, the notice
//! line and the footer. The screen reads the view state by shared reference and writes only
//! into the buffer it is given.

mod peek;
mod rows;

use std::collections::BTreeMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::RowState;
use talaria_core::fleet_view::{FleetViewState, Focus, PeekStatus};
use talaria_core::review::{LabelTone, PrLabel};
use talaria_core::session::SessionState;

use crate::keys::{Action, KeyMap};
use crate::prompts::PromptStack;

use rows::{
    Columns, body_lines, cut, cut_tail, draw_body_line, draw_text, header_text, not_listed_lines,
};

/// The narrowest overlay that draws the list.
const MIN_WIDTH: u16 = 40;
/// The shortest overlay that draws the list.
const MIN_HEIGHT: u16 = 10;
/// From this height the peek panel takes the lower half; below it the panel replaces the list.
const SPLIT_HEIGHT: u16 = 20;
/// The fewest rows the peek panel gets when the screen is split.
const MIN_PANEL_ROWS: usize = 8;
/// Shown when the overlay is too small to draw.
const TOO_SMALL: &str = "Window too small for the fleet view. Esc closes.";
/// Shown before the first snapshot.
const LOOKING: &str = "Looking for sessions\u{2026}";
/// The footer of the reply editor.
const FOOTER_REPLY: &str = "enter send \u{b7} esc keep draft";
/// The footer of the filter line.
const FOOTER_FILTER: &str = "type to filter \u{b7} enter keep \u{b7} esc clear";
/// The footer of the answer focus when no card hint can be read.
const BACK_TO_LIST: &str = "ctrl+c back to list";

/// The peeked session, when the panel has one to draw.
pub struct PeekPanel<'a> {
    pub session: &'a SessionState,
    pub stack: Option<&'a PromptStack>,
    pub now: f64,
}

/// The key names the footer shows. A `None` leaves that hint out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetKeyLabels {
    pub open: Option<String>,
    pub peek: Option<String>,
    pub reply: Option<String>,
    pub stop: Option<String>,
    pub answer: Option<String>,
    pub review: Option<String>,
    pub filter: Option<String>,
}

impl Default for FleetKeyLabels {
    fn default() -> Self {
        let some = |text: &str| Some(text.to_owned());
        Self {
            open: some("enter"),
            peek: some("space"),
            reply: some("r"),
            stop: some("x"),
            answer: some("a"),
            review: some("v"),
            filter: some("/"),
        }
    }
}

impl FleetKeyLabels {
    /// The first key bound to each fleet action, in lower case.
    #[must_use]
    pub fn from_key_map(map: &KeyMap) -> Self {
        let label = |action: Action| map.label(action).map(|text| text.to_lowercase());
        Self {
            open: label(Action::FleetOpen),
            peek: label(Action::FleetPeek),
            reply: label(Action::FleetReply),
            stop: label(Action::FleetStop),
            answer: label(Action::FleetAnswer),
            review: label(Action::FleetReview),
            filter: label(Action::FleetFilter),
        }
    }
}

/// Colours for one fleet frame. Stale rows use the dim modifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetTheme {
    pub needs_you: Style,
    pub working: Style,
    pub failed: Style,
    pub done: Style,
    pub idle: Style,
    pub dim: Style,
    pub header: Style,
    pub footer: Style,
    /// Added to the row under the cursor.
    pub cursor: Style,
    /// A group header such as `WORKING (3)`.
    pub group: Style,
    /// The notice line.
    pub notice: Style,
    /// A pull-request label with passing checks, or a merged pull request in `Success`.
    pub success: Style,
    /// A pull-request label with failing checks.
    pub error: Style,
    /// A pull-request label with pending checks.
    pub warning: Style,
    /// A pull-request label with no checks, a closed pull request, or a stale label.
    pub muted: Style,
    /// A merged pull request.
    pub accent: Style,
    /// The key names the footer shows.
    pub keys: FleetKeyLabels,
}

impl Default for FleetTheme {
    fn default() -> Self {
        Self {
            needs_you: Style::default(),
            working: Style::default(),
            failed: Style::default(),
            done: Style::default(),
            idle: Style::default(),
            dim: Style::default().add_modifier(Modifier::DIM),
            header: Style::default().add_modifier(Modifier::BOLD),
            footer: Style::default().add_modifier(Modifier::DIM),
            cursor: Style::default().add_modifier(Modifier::BOLD),
            group: Style::default().add_modifier(Modifier::BOLD),
            notice: Style::default(),
            success: Style::default(),
            error: Style::default(),
            warning: Style::default(),
            muted: Style::default().add_modifier(Modifier::DIM),
            accent: Style::default(),
            keys: FleetKeyLabels::default(),
        }
    }
}

impl FleetTheme {
    /// The default theme with the footer naming the keys bound in `map`.
    #[must_use]
    pub fn with_key_map(map: &KeyMap) -> Self {
        Self {
            keys: FleetKeyLabels::from_key_map(map),
            ..Self::default()
        }
    }

    /// The style for a row in `state`. Stale rows are dim as well as coloured.
    #[must_use]
    pub fn row_style(&self, state: RowState, stale: bool) -> Style {
        let base = match state {
            RowState::NeedsYou => self.needs_you,
            RowState::Working => self.working,
            RowState::Failed => self.failed,
            RowState::Done => self.done,
            RowState::Idle => self.idle,
        };
        if stale { base.patch(self.dim) } else { base }
    }

    /// The style for a pull-request label in `tone`.
    #[must_use]
    pub fn label_style(&self, tone: LabelTone) -> Style {
        match tone {
            LabelTone::Success => self.success,
            LabelTone::Error => self.error,
            LabelTone::Warning => self.warning,
            LabelTone::Muted => self.muted,
            LabelTone::Accent => self.accent,
        }
    }
}

/// What the peek area shows.
enum Panel<'a> {
    /// The session is being attached, or the panel has nothing yet.
    Opening,
    /// The attach failed.
    Failed(&'a str),
    /// The session's latest output.
    Session(&'a PeekPanel<'a>),
}

/// The fleet overlay.
pub struct FleetView;

impl FleetView {
    /// Draw into `buf`. Nothing outside `area` is touched.
    pub fn draw(
        state: &FleetViewState,
        peek: Option<PeekPanel<'_>>,
        area: Rect,
        buf: &mut Buffer,
        theme: &FleetTheme,
    ) {
        Self::draw_with_labels(state, peek, area, buf, theme, &BTreeMap::new());
    }

    /// Draw like `draw`, with each row's pull-request label taken from `labels`. A row without
    /// a label leaves its cell blank, and the `PR` column shows only when a visible row has one.
    pub fn draw_with_labels(
        state: &FleetViewState,
        peek: Option<PeekPanel<'_>>,
        area: Rect,
        buf: &mut Buffer,
        theme: &FleetTheme,
        labels: &BTreeMap<RowKey, PrLabel>,
    ) {
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                buf[(x, y)].reset();
            }
        }
        if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
            draw_too_small(area, buf, theme);
            return;
        }
        let width = usize::from(area.width);
        let notice_row = area.bottom() - 2;
        let footer_row = area.bottom() - 1;

        let panel = panel_of(state, peek.as_ref());
        let has_card = panel_has_card(state, panel.as_ref());

        if state.seen_snapshot() {
            draw_body(state, panel.as_ref(), area, buf, theme, notice_row, labels);
        } else {
            draw_text(buf, area.x, area.y, LOOKING, width, theme.notice);
        }
        draw_notice(state, area, buf, theme, notice_row);
        let footer = footer_text(state, theme, has_card, panel.as_ref(), area.width);
        draw_text(
            buf,
            area.x,
            footer_row,
            &cut(&footer, width, false),
            width,
            theme.footer,
        );
    }
}

fn draw_too_small(area: Rect, buf: &mut Buffer, theme: &FleetTheme) {
    let width = usize::from(area.width);
    let pieces = rows::wrap_words(TOO_SMALL, width);
    for (y, piece) in (area.y..area.bottom()).zip(&pieces) {
        draw_text(buf, area.x, y, piece, width, theme.notice);
    }
}

/// Which panel to draw: the failure or opening text from the view state first, otherwise the
/// session passed in.
fn panel_of<'a>(state: &'a FleetViewState, peek: Option<&'a PeekPanel<'a>>) -> Option<Panel<'a>> {
    match (state.peek(), peek) {
        (Some((_, PeekStatus::Opening)), _) => Some(Panel::Opening),
        (Some((_, PeekStatus::Failed { text })), _) => Some(Panel::Failed(text)),
        (_, Some(panel)) => Some(Panel::Session(panel)),
        (Some((_, PeekStatus::Open { .. })), None) => Some(Panel::Opening),
        (None, None) => None,
    }
}

/// Whether the peek shows an open card, which adds `a answer` to the footer.
fn panel_has_card(state: &FleetViewState, panel: Option<&Panel<'_>>) -> bool {
    match panel {
        Some(Panel::Session(panel)) => {
            let from_status = matches!(
                state.peek(),
                Some((
                    _,
                    PeekStatus::Open {
                        has_open_prompt: true,
                        ..
                    }
                ))
            );
            from_status || panel.stack.is_some_and(|stack| stack.section().is_some())
        }
        _ => false,
    }
}

/// The header, banners, list, peek panel and the profiles that are not listed.
fn draw_body(
    state: &FleetViewState,
    panel: Option<&Panel<'_>>,
    area: Rect,
    buf: &mut Buffer,
    theme: &FleetTheme,
    notice_row: u16,
    labels: &BTreeMap<RowKey, PrLabel>,
) {
    let width = usize::from(area.width);
    let mut y = area.y;
    let header = header_text(state, width);
    draw_text(buf, area.x, y, &header.left, width, theme.header);
    if let Some((x, note)) = &header.note {
        let room = width.saturating_sub(usize::from(*x));
        draw_text(buf, area.x + *x, y, note, room, theme.dim);
    }
    y += 1;
    if let Some(note) = state.scope_note() {
        draw_text(
            buf,
            area.x,
            y,
            &cut(note, width, false),
            width,
            theme.notice,
        );
        y += 1;
    }
    let unlisted = not_listed_lines(state.profiles());
    let reserved = u16::try_from(unlisted.len()).unwrap_or(0);
    let top = y;
    let avail = usize::from(notice_row.saturating_sub(top).saturating_sub(reserved));

    let visible = state.visible();
    let columns = Columns::new(&visible, labels, area.width);
    let (lines, cursor_index) = body_lines(state, &visible);
    let list_rows = match panel {
        None => avail,
        Some(_) if area.height >= SPLIT_HEIGHT => {
            avail - (avail / 2).max(MIN_PANEL_ROWS).min(avail)
        }
        Some(_) => 0,
    };
    let shown = lines.len().min(list_rows);
    // The window starts at the smallest offset that keeps the cursor row in sight.
    let offset = match cursor_index {
        Some(index) => index.saturating_add(1).saturating_sub(list_rows),
        None => 0,
    };
    let cursor = state.cursor();
    for (index, line) in lines.iter().skip(offset).take(shown).enumerate() {
        let row_y = top + u16::try_from(index).unwrap_or(0);
        draw_body_line(
            buf, area.x, row_y, line, &columns, cursor, state, labels, theme,
        );
    }
    let mut next = top + u16::try_from(shown).unwrap_or(0);

    if let Some(panel) = panel {
        let room = avail - shown;
        let used = peek::draw_panel(state, panel, area.x, next, area.width, room, buf, theme);
        next += u16::try_from(used).unwrap_or(0);
    }
    for line in &unlisted {
        if next >= notice_row {
            break;
        }
        draw_text(
            buf,
            area.x,
            next,
            &cut(line, width, false),
            width,
            theme.dim,
        );
        next += 1;
    }
}

fn draw_notice(state: &FleetViewState, area: Rect, buf: &mut Buffer, theme: &FleetTheme, row: u16) {
    let Some(notice) = state.notice() else {
        return;
    };
    let width = usize::from(area.width);
    let clean = crate::history::sanitize(notice);
    if *state.focus() == Focus::Reply {
        // The end of the draft is where the person is typing, so the start gives way.
        let (text, used) = cut_tail(&clean, width.saturating_sub(1));
        draw_text(buf, area.x, row, &text, width, theme.notice);
        let caret = area.x + u16::try_from(used).unwrap_or(0);
        if caret < area.right() {
            buf[(caret, row)].set_style(Style::default().add_modifier(Modifier::REVERSED));
        }
    } else {
        draw_text(
            buf,
            area.x,
            row,
            &cut(&clean, width, false),
            width,
            theme.notice,
        );
    }
}

fn footer_text(
    state: &FleetViewState,
    theme: &FleetTheme,
    has_card: bool,
    panel: Option<&Panel<'_>>,
    width: u16,
) -> String {
    match state.focus() {
        Focus::Reply => FOOTER_REPLY.to_owned(),
        Focus::Filter => FOOTER_FILTER.to_owned(),
        Focus::Answer => match panel {
            Some(Panel::Session(panel)) => panel
                .stack
                .and_then(|stack| peek::card_hint(stack, width))
                .map_or_else(
                    || BACK_TO_LIST.to_owned(),
                    |hint| match hint.find("Ctrl+C") {
                        Some(at) => format!("{}{BACK_TO_LIST}", &hint[..at]),
                        None => format!("{hint} \u{b7} {BACK_TO_LIST}"),
                    },
                ),
            _ => BACK_TO_LIST.to_owned(),
        },
        Focus::List | Focus::ConfirmStop => list_footer(&theme.keys, has_card),
    }
}

fn list_footer(keys: &FleetKeyLabels, has_card: bool) -> String {
    let mut parts = Vec::new();
    let mut hint = |key: &Option<String>, word: &str| {
        if let Some(key) = key {
            parts.push(format!("{key} {word}"));
        }
    };
    hint(&keys.open, "open");
    hint(&keys.peek, "peek");
    hint(&keys.reply, "reply");
    hint(&keys.stop, "stop");
    if has_card {
        hint(&keys.answer, "answer");
    }
    hint(&keys.review, "review");
    hint(&keys.filter, "filter");
    parts.push("esc close".to_owned());
    parts.join(" \u{b7} ")
}
