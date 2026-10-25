//! Full-screen transcript. Issue 222.
//!
//! The pager owns the view state of the transcript: where the body starts, whether it follows the
//! end of the session, how much of each cell is drawn, and the search. It never calls Hermes and
//! never changes the session it shows.

mod doc;
mod render;
mod search;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use talaria_core::session::Cell;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::history::{CellLines, PlainCellLines};
use crate::markdown::MarkdownOptions;

pub use render::{Details, PagerCellLines, PagerTheme, format_duration_fallback};
pub use search::{Match, Query, SearchIndex, cell_search_text, find_all, fold, highlight_line};

use doc::Document;

/// The renderer the pager uses for its own line counts.
static PLAIN: PlainCellLines = PlainCellLines;

/// The line the screen shows when it is too small for a transcript.
const TOO_SMALL: &str = "Window too small for the transcript. q closes.";

/// The key hints, at 100 columns and more.
const FULL_HINT: &str =
    "/ search · n N next/prev · { } prompts · g G top/bottom · t details · q close";

/// The key hints, at 60 columns and more.
const SHORT_HINT: &str = "/ search · n N · { } · g G · t · q close";

/// The banner shown while Hermes waits for an answer.
const WAITING_BANNER: &str =
    "\u{2691} Hermes is waiting for your answer. Press q to go back and answer it.";

/// The banner shown while the link is being re-established.
const RECONNECTING_BANNER: &str =
    "Connection to Hermes lost; reconnecting. The transcript may be behind.";

/// The banner shown when the link dropped.
const DISCONNECTED_BANNER: &str =
    "Disconnected from Hermes. This is what Talaria received before the connection dropped.";

/// The banner shown when Hermes left older history out of the resume.
const TRUNCATED_BANNER: &str =
    "Older summarized history was left out by Hermes to keep the resume reply small.";

/// The most characters one search query may hold.
const QUERY_LIMIT: usize = 256;

/// What the transcript may show about the gateway link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionBanner {
    /// The link is up. No banner.
    Connected,
    /// The link is dialing again.
    Reconnecting,
    /// The link failed or was closed.
    Disconnected,
}

/// Read-only facts for one draw or one key.
#[derive(Clone, Copy, Debug)]
pub struct PagerContext<'a> {
    /// Final cells, then the live tail.
    pub cells: &'a [Cell],
    /// Bumped when the application replaces the session.
    pub source_epoch: u64,
    /// Session title, or empty when it is unknown.
    pub title: &'a str,
    /// A reply is in progress.
    pub running: bool,
    /// An approval, a question or a masked prompt is waiting.
    pub needs_answer: bool,
    /// Link banner.
    pub connection: ConnectionBanner,
    /// Older summarized history was left out of the resume.
    pub history_truncated: bool,
}

/// A transcript command after the application has mapped the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagerAction {
    /// One line up.
    LineUp,
    /// One line down.
    LineDown,
    /// A page up.
    PageUp,
    /// A page down.
    PageDown,
    /// A half a page up.
    HalfPageUp,
    /// A half a page down.
    HalfPageDown,
    /// First cell.
    Top,
    /// Last line.
    Bottom,
    /// Previous message from the person.
    PrevPrompt,
    /// Next message from the person.
    NextPrompt,
    /// Open the search prompt.
    StartSearch,
    /// Next match.
    NextMatch,
    /// Previous match.
    PrevMatch,
    /// Expanded or compact.
    ToggleDetails,
    /// Escape, after the application has decided it is not Control-C.
    Escape,
    /// Close.
    Close,
}

/// One edit to the search prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchInput {
    /// One typed character.
    Char(char),
    /// The first line of a paste.
    Paste(String),
    /// Delete the last character.
    Backspace,
    /// Confirm the query.
    Enter,
    /// Cancel the prompt.
    Escape,
}

/// What one key did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagerOutcome {
    /// The transcript stays open.
    Stay {
        /// The screen should be drawn again.
        redraw: bool,
    },
    /// The transcript closed. The agent was not interrupted.
    Close,
}

/// The state of the search prompt while it is open.
#[derive(Clone, Debug)]
struct Typing {
    /// The text typed so far.
    query: String,
    /// Where the view was when the prompt opened.
    anchor: (usize, usize),
    /// Whether the view was following when the prompt opened.
    follow: bool,
    /// The confirmed search the prompt suspended.
    confirmed: Option<Query>,
    /// The matches of the suspended search.
    matches: Vec<Match>,
    /// The current match of the suspended search.
    current: Option<usize>,
}

/// View state for one opening of the transcript.
#[derive(Debug)]
pub struct Pager {
    cols: u16,
    rows: u16,
    /// The cell at the top of the body.
    cell_index: usize,
    /// The line of that cell at the top of the body.
    line_in_cell: usize,
    /// Whether the body shows the end of the session.
    follow: bool,
    /// Expanded or compact cells.
    details: Details,
    /// The epoch the view was built from.
    epoch: u64,
    /// Whether new output arrived while the view was not following.
    new_output: bool,
    /// A short note that replaces the key hints in the footer.
    notice: Option<String>,
    /// The search prompt, while it is open.
    typing: Option<Typing>,
    /// The confirmed search.
    confirmed: Option<Query>,
    /// The last confirmed query of this pager.
    last_confirmed: Option<Query>,
    /// The matches of the active search.
    matches: Vec<Match>,
    /// The current match of the active search.
    current: Option<usize>,
    /// The rendered lines of the cells that are final.
    doc: Document,
}

impl Pager {
    /// Open at the bottom, following, expanded. No position is kept from an earlier opening.
    #[must_use]
    pub fn open(ctx: &PagerContext<'_>, cols: u16, rows: u16, _index: &mut SearchIndex) -> Self {
        let mut pager = Self {
            cols,
            rows,
            cell_index: 0,
            line_in_cell: 0,
            follow: true,
            details: Details::Expanded,
            epoch: ctx.source_epoch,
            new_output: false,
            notice: None,
            typing: None,
            confirmed: None,
            last_confirmed: None,
            matches: Vec::new(),
            current: None,
            doc: Document::new(),
        };
        pager.snap_to_bottom(ctx);
        pager
    }

    /// Apply one mapped key.
    pub fn handle_action(
        &mut self,
        action: PagerAction,
        ctx: &PagerContext<'_>,
        _index: &mut SearchIndex,
    ) -> PagerOutcome {
        if self.typing.is_some() {
            return match action {
                PagerAction::Close => PagerOutcome::Close,
                PagerAction::Escape => {
                    self.cancel_typing();
                    PagerOutcome::Stay { redraw: true }
                }
                _ => PagerOutcome::Stay { redraw: false },
            };
        }
        match action {
            PagerAction::Close => PagerOutcome::Close,
            PagerAction::Escape => {
                if self.confirmed.is_some() {
                    self.confirmed = None;
                    self.matches.clear();
                    self.current = None;
                    PagerOutcome::Stay { redraw: true }
                } else {
                    PagerOutcome::Close
                }
            }
            PagerAction::LineUp => self.scroll_up(ctx, 1),
            PagerAction::LineDown => self.scroll_down(ctx, 1),
            PagerAction::PageUp => self.scroll_up(ctx, self.page_size(ctx)),
            PagerAction::PageDown => self.scroll_down(ctx, self.page_size(ctx)),
            PagerAction::HalfPageUp => self.scroll_up(ctx, self.half_page_size(ctx)),
            PagerAction::HalfPageDown => self.scroll_down(ctx, self.half_page_size(ctx)),
            PagerAction::Top => {
                self.cell_index = 0;
                self.line_in_cell = 0;
                self.follow = self.document_fits(ctx);
                PagerOutcome::Stay { redraw: true }
            }
            PagerAction::Bottom => {
                self.snap_to_bottom(ctx);
                self.follow = true;
                self.new_output = false;
                PagerOutcome::Stay { redraw: true }
            }
            PagerAction::PrevPrompt => self.jump_to_prompt(ctx, false),
            PagerAction::NextPrompt => self.jump_to_prompt(ctx, true),
            PagerAction::StartSearch => {
                self.start_typing();
                PagerOutcome::Stay { redraw: true }
            }
            PagerAction::NextMatch => self.step_match(ctx, true),
            PagerAction::PrevMatch => self.step_match(ctx, false),
            PagerAction::ToggleDetails => {
                self.details = match self.details {
                    Details::Expanded => Details::Compact,
                    Details::Compact => Details::Expanded,
                };
                self.doc.clear();
                PagerOutcome::Stay { redraw: true }
            }
        }
    }

    /// Apply one search-prompt edit.
    pub fn handle_search_input(
        &mut self,
        input: SearchInput,
        ctx: &PagerContext<'_>,
        index: &mut SearchIndex,
    ) -> PagerOutcome {
        match input {
            SearchInput::Char(character) => {
                self.start_typing();
                if let Some(typing) = self.typing.as_mut()
                    && typing.query.chars().count() < QUERY_LIMIT
                {
                    typing.query.push(character);
                }
                self.search_from_anchor(ctx, index);
                PagerOutcome::Stay { redraw: true }
            }
            SearchInput::Paste(text) => {
                self.start_typing();
                let first = text.split('\n').next().unwrap_or("").to_string();
                if let Some(typing) = self.typing.as_mut() {
                    for character in first.chars() {
                        if typing.query.chars().count() >= QUERY_LIMIT {
                            break;
                        }
                        typing.query.push(character);
                    }
                }
                self.search_from_anchor(ctx, index);
                PagerOutcome::Stay { redraw: true }
            }
            SearchInput::Backspace => {
                let Some(typing) = self.typing.as_mut() else {
                    return PagerOutcome::Stay { redraw: false };
                };
                if typing.query.is_empty() {
                    self.cancel_typing();
                    return PagerOutcome::Stay { redraw: true };
                }
                typing.query.pop();
                self.search_from_anchor(ctx, index);
                PagerOutcome::Stay { redraw: true }
            }
            SearchInput::Enter => {
                let Some(typing) = self.typing.as_ref() else {
                    return PagerOutcome::Stay { redraw: false };
                };
                if typing.query.is_empty() {
                    match self.last_confirmed.clone() {
                        Some(query) => {
                            if let Some(typing) = self.typing.as_mut() {
                                typing.query = query.text.clone();
                            }
                            self.search_from_anchor(ctx, index);
                            self.confirm_search(ctx, index);
                        }
                        None => {
                            self.cancel_typing();
                        }
                    }
                } else {
                    self.search_from_anchor(ctx, index);
                    self.confirm_search(ctx, index);
                }
                PagerOutcome::Stay { redraw: true }
            }
            SearchInput::Escape => {
                if self.typing.is_some() {
                    self.cancel_typing();
                    PagerOutcome::Stay { redraw: true }
                } else {
                    PagerOutcome::Stay { redraw: false }
                }
            }
        }
    }

    /// A search query is being typed.
    #[must_use]
    pub fn is_typing_search(&self) -> bool {
        self.typing.is_some()
    }

    /// The session's cells changed.
    pub fn content_changed(&mut self, ctx: &PagerContext<'_>, index: &mut SearchIndex) {
        if ctx.source_epoch != self.epoch {
            self.epoch = ctx.source_epoch;
            self.doc.clear();
            self.cell_index = self.cell_index.min(ctx.cells.len().saturating_sub(1));
            self.notice = Some("transcript reloaded from Hermes".to_string());
        } else if !self.follow {
            self.new_output = true;
        }
        if self.follow {
            self.snap_to_bottom(ctx);
        }
        self.rerun_search(ctx, index);
    }

    /// The terminal size changed. The top cell stays.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        self.doc.clear();
    }

    /// Expanded or compact. The application builds the renderer from this on every draw.
    #[must_use]
    pub fn details(&self) -> Details {
        self.details
    }

    /// Draw into the overlay buffer.
    pub fn draw(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        ctx: &PagerContext<'_>,
        render: &dyn CellLines,
    ) {
        let theme = PagerTheme::default();
        // The overlay always draws at the newest size, so the pager takes that size.
        self.cols = area.width;
        self.rows = area.height;
        let cols = area.width;
        let banners = self.banners(ctx);
        let body = area
            .height
            .saturating_sub(2 + u16::try_from(banners.len()).unwrap_or(u16::MAX));
        if cols < 20 || body < 2 {
            let text = fit(TOO_SMALL, usize::from(cols));
            buf.set_stringn(area.x, area.y, text, usize::from(cols), theme.header);
            return;
        }

        // A following view sits at the end of the session, whatever size the screen is now.
        if self.follow {
            self.snap_to_bottom(ctx);
        }
        self.draw_header(area, buf, ctx, &theme);
        let mut row = area.y + 1;
        for banner in &banners {
            let text = fit(banner, usize::from(cols));
            buf.set_stringn(area.x, row, text, usize::from(cols), theme.banner);
            row += 1;
        }
        let body_area = Rect {
            x: area.x,
            y: row,
            width: area.width,
            height: area
                .height
                .saturating_sub(1 + u16::try_from(banners.len()).unwrap_or(0)),
        };
        self.draw_body(body_area, buf, ctx, render, &theme);
        self.draw_footer(area, buf, ctx, &theme);
    }

    /// The header row: the title on the left, the state on the right.
    fn draw_header(
        &self,
        area: Rect,
        buf: &mut Buffer,
        ctx: &PagerContext<'_>,
        theme: &PagerTheme,
    ) {
        let row = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        buf.set_style(row, theme.header);
        let right = if ctx.running { "working" } else { "idle" };
        let right_width = right.width();
        let avail = usize::from(area.width).saturating_sub(right_width + 1);
        let title = if ctx.title.is_empty() {
            "untitled session"
        } else {
            ctx.title
        };
        let count = format!(" · {} cells", ctx.cells.len());
        let mut left = format!(" Transcript · {title}{count}");
        if left.width() > avail {
            let head = " Transcript · ";
            let room = avail.saturating_sub(head.width());
            left = format!("{}{}{}", head, fit(title, room), count);
            if left.width() > avail {
                left = format!("{}{}", head, fit(title, avail.saturating_sub(head.width())));
            }
        }
        buf.set_stringn(area.x, area.y, fit(&left, avail), avail, theme.header);
        let right_x = area.x
            + area
                .width
                .saturating_sub(u16::try_from(right_width).unwrap_or(0));
        buf.set_stringn(right_x, area.y, right, right_width, theme.header);
    }

    /// The body: the cells from the anchor down.
    fn draw_body(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        ctx: &PagerContext<'_>,
        render: &dyn CellLines,
        theme: &PagerTheme,
    ) {
        let query = self.active_query();
        let current = self
            .current
            .and_then(|index| self.matches.get(index))
            .copied();
        let details = self.details;
        if ctx.cells.is_empty() {
            let text = Line::styled("No messages yet.".to_string(), theme.dim);
            buf.set_line(area.x, area.y, &text, area.width);
            return;
        }
        let mut cell = self.cell_index;
        let mut line = self.line_in_cell;
        let mut row = area.y;
        // One cell's rendered hits and its match count. The count is the width of that cell's
        // slice in the ordered match list, so a long transcript is not scanned once per row.
        let mut cached: Option<CachedCellHits> = None;
        while row < area.y + area.height {
            let Some(current_cell) = ctx.cells.get(cell) else {
                break;
            };
            let own = self.internal_render(render);
            let lines = self
                .doc
                .cell_lines(current_cell, cell, area.width, details, render, &own);
            if line >= lines.len() {
                cell += 1;
                line = 0;
                continue;
            }
            let refresh = match cached {
                Some(ref paint) => paint.cell != cell,
                None => true,
            };
            if refresh {
                let mut occurrence = None;
                let mut gutter = false;
                if let Some(found) = current
                    && found.cell_index == cell
                {
                    let hits = rendered_hits(&lines, query.as_ref());
                    let in_cell = self.matches.partition_point(|item| item.cell_index <= cell)
                        - self.matches.partition_point(|item| item.cell_index < cell);
                    if hits.total == in_cell && found.ordinal_in_cell < hits.total {
                        let mut left = found.ordinal_in_cell;
                        for occurrences in &hits.per_line {
                            if left < occurrences.len() {
                                occurrence = Some(occurrences[left]);
                                break;
                            }
                            left -= occurrences.len();
                        }
                    } else {
                        gutter = true;
                    }
                }
                cached = Some(CachedCellHits {
                    cell,
                    occurrence,
                    gutter,
                });
            }
            let paint = cached.as_ref().expect("the cell was just cached");
            let current_offset = paint.occurrence.map(|pair| pair.0);
            let marked = paint.gutter && line == 0;
            let drawn = &lines[line];
            let painted = match &query {
                Some(query) => highlight_line(drawn, query, current_offset, theme),
                None => drawn.clone(),
            };
            if marked {
                buf.set_stringn(area.x, row, "\u{25b8}", 1, theme.gutter_mark);
                buf.set_line(area.x + 1, row, &painted, area.width.saturating_sub(1));
            } else {
                buf.set_line(area.x, row, &painted, area.width);
            }
            line += 1;
            if line >= lines.len() {
                cell += 1;
                line = 0;
            }
            row += 1;
        }
    }

    /// The footer row: the key hints on the left, the position on the right.
    fn draw_footer(
        &self,
        area: Rect,
        buf: &mut Buffer,
        ctx: &PagerContext<'_>,
        theme: &PagerTheme,
    ) {
        let row = area.y + area.height.saturating_sub(1);
        let footer = Rect {
            x: area.x,
            y: row,
            width: area.width,
            height: 1,
        };
        buf.set_style(footer, theme.footer);
        let right = self.right_part(ctx);
        let right_width = right.width();
        let avail = usize::from(area.width).saturating_sub(right_width + 1);
        let left = self.left_part(ctx);
        let left = if avail < 2 {
            String::new()
        } else {
            fit(&left, avail)
        };
        buf.set_stringn(area.x, row, left, avail, theme.footer);
        if right_width <= usize::from(area.width) {
            let x = area.x
                + area
                    .width
                    .saturating_sub(u16::try_from(right_width).unwrap_or(0));
            buf.set_stringn(x, row, right, right_width, theme.footer);
        } else {
            buf.set_stringn(
                area.x,
                row,
                fit(&right, usize::from(area.width)),
                usize::from(area.width),
                theme.footer,
            );
        }
    }

    /// The banners, in the order they are shown.
    fn banners(&self, ctx: &PagerContext<'_>) -> Vec<&'static str> {
        let mut out = Vec::new();
        if ctx.needs_answer {
            out.push(WAITING_BANNER);
        }
        match ctx.connection {
            ConnectionBanner::Connected => {}
            ConnectionBanner::Reconnecting => out.push(RECONNECTING_BANNER),
            ConnectionBanner::Disconnected => out.push(DISCONNECTED_BANNER),
        }
        if ctx.history_truncated {
            out.push(TRUNCATED_BANNER);
        }
        out
    }

    /// The left part of the footer.
    fn left_part(&self, _ctx: &PagerContext<'_>) -> String {
        if let Some(notice) = &self.notice {
            return notice.clone();
        }
        if let Some(typing) = &self.typing {
            return format!("/{}\u{258f}", typing.query);
        }
        if let Some(query) = &self.confirmed {
            if self.matches.is_empty() {
                return format!("no match for \"{}\" · Esc clears", query.text);
            }
            return format!(
                "\"{}\" {} of {} · n N · Esc clears",
                query.text,
                self.current.map_or(0, |index| index + 1),
                self.matches.len()
            );
        }
        let cols = usize::from(self.cols);
        if cols >= 100 {
            FULL_HINT.to_string()
        } else if cols >= 60 {
            SHORT_HINT.to_string()
        } else {
            String::new()
        }
    }

    /// The right part of the footer.
    fn right_part(&self, ctx: &PagerContext<'_>) -> String {
        if self.typing.is_some() {
            if self.active_query().is_none() {
                return String::new();
            }
            if self.matches.is_empty() {
                return "no match".to_string();
            }
            return format!(
                "{} of {}",
                self.current.map_or(0, |index| index + 1),
                self.matches.len()
            );
        }
        if ctx.cells.is_empty() {
            return "cell 0/0".to_string();
        }
        let mut text = format!("cell {}/{}", self.cell_index + 1, ctx.cells.len());
        if self.follow {
            text.push_str(" · following");
        } else if self.new_output {
            text.push_str(" · ↓ new output · End");
        }
        text
    }

    /// The query the screen shows: the one being typed, or the confirmed one.
    fn active_query(&self) -> Option<Query> {
        if let Some(typing) = &self.typing {
            if typing.query.is_empty() {
                return None;
            }
            return Some(Query::new(&typing.query));
        }
        self.confirmed.clone()
    }

    /// Open the search prompt, remembering where the view was.
    fn start_typing(&mut self) {
        if self.typing.is_some() {
            return;
        }
        self.typing = Some(Typing {
            query: String::new(),
            anchor: (self.cell_index, self.line_in_cell),
            follow: self.follow,
            confirmed: self.confirmed.clone(),
            matches: self.matches.clone(),
            current: self.current,
        });
        self.notice = None;
    }

    /// Close the search prompt and put the view and the search back as they were.
    fn cancel_typing(&mut self) {
        let Some(typing) = self.typing.take() else {
            return;
        };
        self.cell_index = typing.anchor.0;
        self.line_in_cell = typing.anchor.1;
        self.follow = typing.follow;
        self.confirmed = typing.confirmed;
        self.matches = typing.matches;
        self.current = typing.current;
    }

    /// Confirm the query being typed.
    fn confirm_search(&mut self, ctx: &PagerContext<'_>, index: &mut SearchIndex) {
        let Some(typing) = self.typing.as_mut() else {
            return;
        };
        if typing.query.is_empty() {
            return;
        }
        let query = Query::new(&typing.query);
        self.typing = None;
        self.confirmed = Some(query.clone());
        self.last_confirmed = Some(query);
        self.notice = None;
        self.rerun_search(ctx, index);
    }

    /// Run the search again, keeping the current match when it is still there.
    fn rerun_search(&mut self, ctx: &PagerContext<'_>, index: &mut SearchIndex) {
        let keep = self
            .current
            .and_then(|position| self.matches.get(position))
            .map(|found| (found.cell_index, found.ordinal_in_cell));
        let Some(query) = self.active_query() else {
            self.matches.clear();
            self.current = None;
            return;
        };
        self.matches = find_all(ctx.cells, ctx.source_epoch, index, &query, &PLAIN);
        self.current = None;
        if self.matches.is_empty() {
            return;
        }
        if let Some((cell, ordinal)) = keep {
            let same = self
                .matches
                .iter()
                .position(|item| (item.cell_index, item.ordinal_in_cell) == (cell, ordinal));
            let next = self
                .matches
                .iter()
                .position(|item| (item.cell_index, item.ordinal_in_cell) > (cell, ordinal));
            self.current = same.or(next).or(Some(self.matches.len() - 1));
        } else {
            self.current = Some(0);
        }
    }

    /// Run the search from the cell the prompt remembered, and move to the first match there.
    fn search_from_anchor(&mut self, ctx: &PagerContext<'_>, index: &mut SearchIndex) {
        let from = self
            .typing
            .as_ref()
            .map_or(self.cell_index, |typing| typing.anchor.0);
        self.rerun_search(ctx, index);
        if self.matches.is_empty() {
            self.current = None;
            return;
        }
        let position = self
            .matches
            .iter()
            .position(|item| item.cell_index >= from)
            .unwrap_or(0);
        self.current = Some(position);
        self.go_to_match(position, ctx);
    }

    /// Move the view to a match.
    fn go_to_match(&mut self, position: usize, ctx: &PagerContext<'_>) {
        let Some(found) = self.matches.get(position).copied() else {
            return;
        };
        self.cell_index = found.cell_index;
        self.line_in_cell = 0;
        self.follow = false;
        let _ = ctx;
    }

    /// Move to the next or the previous match.
    fn step_match(&mut self, ctx: &PagerContext<'_>, forward: bool) -> PagerOutcome {
        let Some(query) = self.confirmed.clone() else {
            return PagerOutcome::Stay { redraw: false };
        };
        if self.matches.is_empty() {
            self.notice = Some(format!("no match for \"{}\"", query.text));
            return PagerOutcome::Stay { redraw: true };
        }
        let current = self.current.unwrap_or(0);
        let next = if forward {
            if current + 1 >= self.matches.len() {
                self.notice = Some("search wrapped to the top".to_string());
                0
            } else {
                current + 1
            }
        } else if current == 0 {
            self.notice = Some("search wrapped to the bottom".to_string());
            self.matches.len() - 1
        } else {
            current - 1
        };
        self.current = Some(next);
        self.go_to_match(next, ctx);
        PagerOutcome::Stay { redraw: true }
    }

    /// Move to the previous or the next message the person sent.
    fn jump_to_prompt(&mut self, ctx: &PagerContext<'_>, forward: bool) -> PagerOutcome {
        let target = if forward {
            ctx.cells
                .iter()
                .enumerate()
                .skip(self.cell_index + 1)
                .find(|(_, cell)| matches!(cell, Cell::User(_)))
                .map(|(index, _)| index)
        } else if matches!(ctx.cells.get(self.cell_index), Some(Cell::User(_)))
            && self.line_in_cell > 0
        {
            Some(self.cell_index)
        } else {
            ctx.cells
                .iter()
                .enumerate()
                .take(self.cell_index)
                .rev()
                .find(|(_, cell)| matches!(cell, Cell::User(_)))
                .map(|(index, _)| index)
        };
        match target {
            Some(index) => {
                self.cell_index = index;
                self.line_in_cell = 0;
                self.follow = false;
                PagerOutcome::Stay { redraw: true }
            }
            None => {
                self.notice = Some(if forward {
                    "no later prompt".to_string()
                } else {
                    "no earlier prompt".to_string()
                });
                PagerOutcome::Stay { redraw: true }
            }
        }
    }

    /// Move the view up by a number of lines.
    fn scroll_up(&mut self, ctx: &PagerContext<'_>, lines: usize) -> PagerOutcome {
        let (cell, line) = self.anchor_up(ctx, lines);
        let moved = (cell, line) != (self.cell_index, self.line_in_cell);
        let was_following = self.follow;
        self.cell_index = cell;
        self.line_in_cell = line;
        self.follow = false;
        PagerOutcome::Stay {
            redraw: moved || was_following,
        }
    }

    /// Move the view down by a number of lines.
    fn scroll_down(&mut self, ctx: &PagerContext<'_>, lines: usize) -> PagerOutcome {
        let (cell, line) = self.anchor_down(ctx, lines);
        let moved = (cell, line) != (self.cell_index, self.line_in_cell);
        self.cell_index = cell;
        self.line_in_cell = line;
        if self.follow {
            return PagerOutcome::Stay { redraw: moved };
        }
        let bottom = self.bottom_snap(ctx);
        if (cell, line) == bottom {
            self.follow = true;
            self.new_output = false;
        }
        PagerOutcome::Stay { redraw: moved }
    }

    /// The anchor a number of lines up.
    fn anchor_up(&mut self, ctx: &PagerContext<'_>, lines: usize) -> (usize, usize) {
        let mut cell = self.cell_index;
        let mut line = self.line_in_cell;
        let mut left = lines;
        while left > 0 {
            if line == 0 {
                if cell == 0 {
                    break;
                }
                cell -= 1;
                line = self.cell_len(ctx, cell);
                if line == 0 {
                    continue;
                }
            }
            line -= 1;
            left -= 1;
        }
        (cell, line)
    }

    /// The anchor a number of lines down.
    fn anchor_down(&mut self, ctx: &PagerContext<'_>, lines: usize) -> (usize, usize) {
        let mut cell = self.cell_index;
        let mut line = self.line_in_cell;
        let mut left = lines;
        while left > 0 {
            let length = self.cell_len(ctx, cell);
            if line + 1 < length {
                line += 1;
            } else if cell + 1 < ctx.cells.len() {
                cell += 1;
                line = 0;
            } else {
                break;
            }
            left -= 1;
        }
        (cell, line)
    }

    /// How many lines the cell at an index has.
    fn cell_len(&mut self, ctx: &PagerContext<'_>, cell: usize) -> usize {
        let Some(current) = ctx.cells.get(cell) else {
            return 0;
        };
        let render = self.internal_render(&PLAIN);
        self.doc
            .count_lines(current, cell, self.cols, self.details, &render)
            .len()
    }

    /// The anchor that shows the end of the session.
    fn bottom_snap(&mut self, ctx: &PagerContext<'_>) -> (usize, usize) {
        let height = self.body_height(ctx);
        let render = self.internal_render(&PLAIN);
        self.doc
            .bottom_snap(ctx.cells, height, self.cols, self.details, &render)
    }

    /// Move the view to the end of the session.
    fn snap_to_bottom(&mut self, ctx: &PagerContext<'_>) {
        let (cell, line) = self.bottom_snap(ctx);
        self.cell_index = cell;
        self.line_in_cell = line;
    }

    /// Whether the whole session fits in the body.
    fn document_fits(&mut self, ctx: &PagerContext<'_>) -> bool {
        let height = self.body_height(ctx);
        let render = self.internal_render(&PLAIN);
        self.doc
            .bottom_snap(ctx.cells, height, self.cols, self.details, &render)
            == (0, 0)
    }

    /// How many rows the body has.
    fn body_height(&self, ctx: &PagerContext<'_>) -> u16 {
        let banners = u16::try_from(self.banners(ctx).len()).unwrap_or(u16::MAX);
        self.rows.saturating_sub(2 + banners)
    }

    /// How many lines a page holds.
    fn page_size(&self, ctx: &PagerContext<'_>) -> usize {
        usize::from(self.body_height(ctx).saturating_sub(1)).max(1)
    }

    /// How many lines half a page holds.
    fn half_page_size(&self, ctx: &PagerContext<'_>) -> usize {
        usize::from(self.body_height(ctx) / 2).max(1)
    }

    /// The renderer the pager uses for the cells it draws itself.
    ///
    /// The cell kinds this screen does not know are drawn by the renderer the application
    /// passes in, which is the scrollback renderer.
    fn internal_render<'a>(&self, other: &'a dyn CellLines) -> PagerCellLines<'a> {
        PagerCellLines {
            details: self.details,
            markdown: MarkdownOptions {
                width: self.cols.saturating_sub(2).max(8),
                ..MarkdownOptions::default()
            },
            theme: PagerTheme::default(),
            other,
        }
    }
}

/// Highlight data for one cell. The body recomputes it only when the cell changes.
struct CachedCellHits {
    cell: usize,
    occurrence: Option<(usize, usize)>,
    gutter: bool,
}

/// The occurrences of the query in the rendered lines of one cell.
struct RenderedHits {
    /// How many occurrences the cell holds.
    total: usize,
    /// The occurrences of each line, as character offsets.
    per_line: Vec<Vec<(usize, usize)>>,
}

/// The occurrences of the query in the rendered lines of one cell.
fn rendered_hits(lines: &[Line<'static>], query: Option<&Query>) -> RenderedHits {
    let Some(query) = query else {
        return RenderedHits {
            total: 0,
            per_line: vec![Vec::new(); lines.len()],
        };
    };
    let mut per_line = Vec::with_capacity(lines.len());
    let mut total = 0;
    for line in lines {
        let found = search::find_in_line(line, query);
        total += found.len();
        per_line.push(found);
    }
    RenderedHits { total, per_line }
}

/// The text cut to a width, with an ellipsis when it does not fit.
fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for character in text.chars() {
        let columns = character.width().unwrap_or(0);
        if used + columns > width.saturating_sub(1) {
            break;
        }
        out.push(character);
        used += columns;
    }
    out.push('\u{2026}');
    out
}
