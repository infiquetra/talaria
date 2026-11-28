//! Literal board presentation; terminal modes remain owned by the terminal module.
use crate::history::{grapheme_width, sanitize, word_wrapped};
use crate::theme::surfaces::{ViewStyles, tokens as t};
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Widget, Wrap},
};
use talaria_core::kanban::{
    BoardCard, BoardKey, BoardMode, DetailMotion, DetailViewport, FrameColumn, KanbanBoardState,
    KanbanFrame,
};
use unicode_segmentation::UnicodeSegmentation;
pub struct KanbanOverlay {
    open: bool,
    inline: bool,
}
impl KanbanOverlay {
    pub fn open() -> Self {
        Self {
            open: true,
            inline: false,
        }
    }
    pub fn close(&mut self) {
        self.open = false;
        self.inline = true;
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn showing_inline(&self) -> bool {
        self.inline
    }
}
fn line(text: &str, area: Rect, style: Style, buf: &mut Buffer) {
    Paragraph::new(sanitize(text).into_owned())
        .style(style)
        .render(area, buf);
}

fn fitted(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let text = sanitize(text);
    if Line::from(text.as_ref()).width() <= width {
        return text.into_owned();
    }
    let mut used = 0;
    for (offset, grapheme) in text.grapheme_indices(true) {
        used += usize::from(grapheme_width(grapheme));
        if used >= width {
            let mut clipped = text[..offset].to_owned();
            clipped.push('…');
            return clipped;
        }
    }
    text.into_owned()
}

/// The frame's optional card is the current selection; the state determines whether detail is open.
pub fn draw_kanban(frame: &KanbanFrame, area: Rect, buf: &mut Buffer) {
    draw_kanban_with_theme(frame, area, buf, &ViewStyles::default());
}

/// Draw a board snapshot with the active list styles.
pub fn draw_kanban_with_theme(
    frame: &KanbanFrame,
    area: Rect,
    buf: &mut Buffer,
    views: &ViewStyles,
) {
    let area = area.intersection(buf.area);
    if area.is_empty() {
        return;
    }
    draw_columns(
        frame,
        area,
        buf,
        BoardChrome {
            column: 0,
            escape: "Esc close",
            hints: if area.height >= 15 {
                &BOARD_HINTS[..3]
            } else {
                &[]
            },
            last_event_id: None,
            preview_card: None,
            show_overview: true,
        },
        views,
    );
    views.finish(area, buf, t::LIST_BACKGROUND, true);
}

fn card_columns(area: Rect, count: usize) -> (u16, usize) {
    let visible = count.clamp(1, usize::from((area.width / 22).max(1)));
    (area.width / visible as u16, visible)
}

struct BoardChrome<'a> {
    column: usize,
    escape: &'static str,
    hints: &'static [Hint],
    last_event_id: Option<i64>,
    preview_card: Option<&'a BoardCard>,
    show_overview: bool,
}

impl<'a> BoardChrome<'a> {
    fn for_state(state: &'a KanbanBoardState, height: u16) -> Self {
        let browse = state.browsing_grid();
        Self {
            column: state.column,
            escape: match state.mode {
                BoardMode::Browse if state.detail_id.is_some() => "Esc back to board",
                BoardMode::Browse if !state.filter.is_empty() => "Esc clear filter",
                BoardMode::Browse => "Esc close",
                _ => "Esc cancel",
            },
            hints: match state.mode {
                BoardMode::Confirm { .. } => &CONFIRM_HINTS,
                BoardMode::Browse if browse && height >= 15 => &BOARD_HINTS,
                _ => &[],
            },
            last_event_id: state.live.then_some(state.cursor),
            preview_card: browse.then(|| state.selected()).flatten(),
            show_overview: browse,
        }
    }
}

#[derive(Clone, Copy)]
enum HintKey {
    Char(char),
    Enter,
    Esc,
    Scroll(DetailMotion),
}

struct Hint {
    label: &'static str,
    key: HintKey,
    alternate: Option<HintKey>,
}

impl Hint {
    const fn single(label: &'static str, key: HintKey) -> Self {
        Self {
            label,
            key,
            alternate: None,
        }
    }

    const fn pair(label: &'static str, first: HintKey, second: HintKey) -> Self {
        Self {
            label,
            key: first,
            alternate: Some(second),
        }
    }

    fn key_at(&self, offset: u16) -> Option<HintKey> {
        let Some((alternate, (first, second))) = self.alternate.zip(self.label.split_once('/'))
        else {
            return Some(self.key);
        };
        let offset = usize::from(offset);
        let first_width = Line::raw(first).width();
        let second_width = Line::raw(second.split_once(' ').map_or(second, |(key, _)| key)).width();
        if offset < first_width {
            Some(self.key)
        } else if offset > first_width && offset <= first_width + second_width {
            Some(alternate)
        } else {
            None
        }
    }
}

use HintKey::{Char, Enter, Esc, Scroll};

const BOARD_HINTS: [Hint; 5] = [
    Hint::pair("h/l columns", Char('h'), Char('l')),
    Hint::pair("j/k cards", Char('j'), Char('k')),
    Hint::single("Enter details", Enter),
    Hint::single("/ filter", Char('/')),
    Hint::single("b boards", Char('b')),
];
const CONFIRM_HINTS: [Hint; 1] = [Hint::single("Enter confirm", Enter)];
const DETAIL_HINTS: [Hint; 3] = [
    Hint::pair(
        "↑/↓ scroll",
        Scroll(DetailMotion::Up),
        Scroll(DetailMotion::Down),
    ),
    Hint::pair(
        "PgUp/PgDn page",
        Scroll(DetailMotion::PageUp),
        Scroll(DetailMotion::PageDown),
    ),
    Hint::pair(
        "Home/End",
        Scroll(DetailMotion::Home),
        Scroll(DetailMotion::End),
    ),
];

fn fitted_hints(hints: &[Hint], escape: &str, width: u16) -> String {
    let text = hints
        .iter()
        .map(|hint| hint.label)
        .chain(std::iter::once(escape))
        .collect::<Vec<_>>()
        .join(" · ");
    crate::footer::with_recovery(&text, width)
}

fn hit_hint(text: &str, hints: &[Hint], escape: &'static str, x: u16) -> Option<HintKey> {
    if text.starts_with("Esc") && text.ends_with('…') && x < 3 {
        return Some(Esc);
    }
    let recovery = Hint::single(escape, Esc);
    hints
        .iter()
        .chain(std::iter::once(&recovery))
        .find_map(|hint| {
            let start = text.find(hint.label)?;
            let start = Line::raw(&text[..start]).width() as u16;
            let offset = x.checked_sub(start)?;
            (usize::from(offset) < Line::raw(hint.label).width())
                .then(|| hint.key_at(offset))
                .flatten()
        })
}

fn board_footer(
    frame: &KanbanFrame,
    area: Rect,
    chrome: &BoardChrome<'_>,
) -> (String, Paragraph<'static>) {
    let status = sanitize(&frame.status_line);
    let hints = fitted_hints(chrome.hints, chrome.escape, area.width);
    let footer = if status.is_empty() {
        hints.clone()
    } else {
        format!("{hints}\n{status}")
    };
    (hints, Paragraph::new(footer).wrap(Wrap { trim: false }))
}

struct BoardLayout {
    grid: Rect,
    footer: Rect,
    overview: Vec<String>,
    preview: Vec<String>,
    start: usize,
    columns: usize,
    width: u16,
}

impl BoardLayout {
    fn new(
        frame: &KanbanFrame,
        area: Rect,
        footer: &Paragraph<'_>,
        chrome: &BoardChrome<'_>,
    ) -> Self {
        let footer_height = if area.height > 1 {
            footer
                .line_count(area.width)
                .max(1)
                .min(usize::from(area.height.saturating_sub(3).max(1))) as u16
        } else {
            0
        };
        let footer = Rect::new(
            area.x,
            area.bottom() - footer_height,
            area.width,
            footer_height,
        );
        let body_height = area.height.saturating_sub(1 + footer_height);
        let (width, visible) = card_columns(area, frame.columns.len());
        let start = chrome.column.saturating_sub(visible.saturating_sub(1));
        let columns = frame.columns.len().saturating_sub(start).min(visible);
        let overview = if chrome.show_overview
            && area.width >= 32
            && body_height >= 10
            && columns < frame.columns.len()
        {
            offscreen_overview(&frame.columns, start, columns, area.width)
        } else {
            Vec::new()
        };
        let overview_height = overview.len() as u16;
        let tallest = frame
            .columns
            .iter()
            .skip(start)
            .take(columns)
            .map(|col| col.cards.len())
            .max()
            .unwrap_or(0);
        let preview = if area.width >= 40
            && body_height >= 14
            && tallest.saturating_mul(3).saturating_add(6)
                <= usize::from(body_height - overview_height)
        {
            chrome
                .preview_card
                .map_or_else(Vec::new, |card| card_preview(card, area.width))
        } else {
            Vec::new()
        };
        let grid = Rect::new(
            area.x,
            area.y + 1 + overview_height,
            area.width,
            body_height - overview_height - preview.len() as u16,
        );
        Self {
            grid,
            footer,
            overview,
            preview,
            start,
            columns,
            width,
        }
    }

    fn column(&self, offset: usize, column: &FrameColumn) -> ColumnViewport {
        let rect = Rect::new(
            self.grid.x + offset as u16 * self.width,
            self.grid.y,
            self.width - u16::from(offset + 1 < self.columns),
            self.grid.height,
        );
        let room = usize::from(rect.height.saturating_sub(1));
        // Sparse columns have room to give each card a title and metadata row.
        let card_height =
            if rect.width >= 12 && room >= 5 && column.cards.len().saturating_mul(3) <= room {
                3
            } else {
                1
            };
        let capacity = room / card_height;
        let first = column
            .cards
            .iter()
            .position(|card| card.selected)
            .unwrap_or(0)
            .saturating_sub(capacity.saturating_sub(1));
        ColumnViewport {
            rect,
            card_height,
            capacity,
            first,
        }
    }
}

struct ColumnViewport {
    rect: Rect,
    card_height: usize,
    capacity: usize,
    first: usize,
}

fn offscreen_overview(
    columns: &[FrameColumn],
    start: usize,
    visible: usize,
    width: u16,
) -> Vec<String> {
    let width = usize::from(width);
    let limit = if width < 50 { 3 } else { 2 };
    let mut rows = vec!["Off-screen:".to_owned()];
    let other = columns
        .iter()
        .take(start)
        .chain(columns.iter().skip(start + visible));
    let total = columns.len() - visible;
    for (index, col) in other.enumerate() {
        let label = fitted(&col.label.replace('_', " "), 12);
        let chip = format!("{label} {}", col.cards.len());
        let row = rows.last_mut().expect("overview has a heading");
        let gap = if row.ends_with(':') { " " } else { " · " };
        if Line::raw(row.as_str()).width() + Line::raw(gap).width() + Line::raw(&chip).width()
            <= width
        {
            row.push_str(gap);
            row.push_str(&chip);
        } else if rows.len() < limit {
            rows.push(chip);
        } else {
            let row = rows.last_mut().expect("overview has a heading");
            let suffix = format!(" · +{}", total - index);
            *row = format!(
                "{}{}",
                fitted(row, width.saturating_sub(Line::raw(&suffix).width())),
                suffix
            );
            break;
        }
    }
    rows
}

fn card_preview(card: &BoardCard, width: u16) -> Vec<String> {
    let (label, text) = card
        .latest_summary
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .map_or(("Description", card.body.trim()), |summary| {
            ("Latest result", summary.trim())
        });
    if text.is_empty() {
        return Vec::new();
    }
    let text = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let mut rows = vec![fitted(
        &format!("Selected card · {}", card.title),
        usize::from(width),
    )];
    let excerpt = fitted(&format!("{label}  {text}"), usize::from(width) * 3);
    let mut content = word_wrapped(&excerpt, width);
    if content.len() > 2 {
        content.truncate(2);
        content[1] = fitted(&format!("{}…", content[1]), usize::from(width));
    }
    rows.extend(content);
    rows
}

fn draw_columns(
    frame: &KanbanFrame,
    area: Rect,
    buf: &mut Buffer,
    chrome: BoardChrome<'_>,
    views: &ViewStyles,
) -> Rect {
    Clear.render(area, buf);
    let cards = if frame.shown == frame.total {
        if frame.total == 1 {
            "1 card".into()
        } else {
            format!("{} cards", frame.total)
        }
    } else {
        format!("{} of {} cards", frame.shown, frame.total)
    };
    let sync = match frame.live.as_str() {
        "parked" => "Live updates paused",
        "connecting" => "Connecting to live updates…",
        "off" => "Live updates off",
        _ => "Live",
    };
    let mut heading = format!("Kanban · {} · {cards} · {sync}", frame.board);
    if let Some(id) = chrome.last_event_id {
        let label = format!(" · Last event ID: {id}");
        if Line::raw(sanitize(&heading).as_ref()).width() + Line::raw(&label).width()
            <= usize::from(area.width)
        {
            heading.push_str(&label);
        }
    }
    line(
        &heading,
        Rect::new(area.x, area.y, area.width, 1),
        views.style(t::LIST_HEADER, Style::default()),
        buf,
    );
    let muted = views.style(t::LIST_MUTED, Style::default().dim());
    let (_, footer) = board_footer(frame, area, &chrome);
    let layout = BoardLayout::new(frame, area, &footer, &chrome);
    footer.style(muted).render(layout.footer, buf);
    for (y, text) in (area.y + 1..layout.grid.y).zip(&layout.overview) {
        line(text, Rect::new(area.x, y, area.width, 1), muted, buf);
    }
    for (index, text) in layout.preview.iter().enumerate() {
        let style = if index == 0 {
            views.style(t::LIST_HEADER, Style::default().bold())
        } else {
            views.style(t::LIST_ROW, Style::default())
        };
        line(
            text,
            Rect::new(area.x, layout.grid.bottom() + index as u16, area.width, 1),
            style,
            buf,
        );
    }
    let body = layout.grid;
    if body.is_empty() {
        return body;
    }
    let start = layout.start;
    let columns = layout.columns;
    let row_style = views.style(t::LIST_ROW, Style::default());
    let selected = views.style(
        t::LIST_SELECTED,
        Style::default().add_modifier(Modifier::REVERSED),
    );
    let selected = if views.color() {
        selected
    } else {
        selected.reversed()
    };
    let warning_style = views.style(t::LIST_WARNING, Style::default());
    let divider = views.style(t::LIST_DIVIDER, Style::default().dim());
    for (i, col) in frame.columns.iter().skip(start).take(columns).enumerate() {
        let viewport = layout.column(i, col);
        let x = viewport.rect.x;
        let cell_width = viewport.rect.width;
        if i + 1 < columns {
            for y in body.y..body.bottom() {
                buf.set_stringn(x + cell_width, y, views.divider_symbol("│"), 1, divider);
            }
        }
        let active = chrome.show_overview && chrome.column == start + i;
        let marker = if active { "› " } else { "" };
        let count = format!(" ({})", col.cards.len());
        let label = fitted(
            &col.label.replace('_', " "),
            usize::from(cell_width).saturating_sub(count.len() + Line::raw(marker).width()),
        );
        let header_style = views.style(t::LIST_HEADER, Style::default());
        line(
            &format!("{marker}{label}{count}"),
            Rect::new(x, body.y, cell_width, 1),
            if active {
                header_style.bold()
            } else {
                header_style
            },
            buf,
        );
        let room = usize::from(body.height.saturating_sub(1));
        if frame.shown > 0 && col.cards.is_empty() && room > 0 {
            line(
                "  No cards",
                Rect::new(x, body.y + 1, cell_width, 1),
                views.style(t::LIST_EMPTY, Style::default()),
                buf,
            );
        }
        let card_height = viewport.card_height;
        for (row, card) in col
            .cards
            .iter()
            .skip(viewport.first)
            .take(viewport.capacity)
            .enumerate()
        {
            let marker = if card.selected { "> " } else { "  " };
            let flagged = matches!(card.status.as_str(), "blocked" | "review");
            let warning = if flagged { "! " } else { "" };
            let width = usize::from(cell_width);
            let style = match (card.selected, flagged) {
                (true, _) => selected,
                (false, true) => warning_style,
                _ => row_style,
            };
            let y = body.y + 1 + (row * card_height) as u16;
            if card_height > 1 {
                let indent = marker.len() + warning.len();
                let available = cell_width.saturating_sub(indent as u16).max(1);
                let title = word_wrapped(&sanitize(&card.title), available);
                let first_line = title.first().map(String::as_str).unwrap_or("");
                let first_row = Rect::new(x, y, cell_width, 1);
                if card.selected {
                    buf.set_style(Rect::new(x, y, cell_width, card_height as u16), style);
                }
                line(
                    &format!("{marker}{warning}{first_line}"),
                    first_row,
                    style,
                    buf,
                );
                if let Some(second) = title.get(1) {
                    let continuation = if title.len() > 2 {
                        fitted(&format!("{second} …"), usize::from(available))
                    } else {
                        second.clone()
                    };
                    line(
                        &continuation,
                        Rect::new(x + indent as u16, y + 1, available, 1),
                        style,
                        buf,
                    );
                }
                let metadata = if card.assignee.is_empty() {
                    card.age.clone()
                } else {
                    format!("{} · {}", card.assignee, card.age)
                };
                let metadata_row = y + if title.len() > 1 { 2 } else { 1 };
                line(
                    &fitted(&metadata, width.saturating_sub(2)),
                    Rect::new(x + 2, metadata_row, cell_width.saturating_sub(2), 1),
                    if card.selected { selected } else { muted },
                    buf,
                );
                continue;
            }
            let assignee = if width >= 32 {
                fitted(&card.assignee, width / 5)
            } else {
                String::new()
            };
            let age = fitted(&card.age, 5);
            let suffix = if assignee.is_empty() {
                format!("  {age:>5}")
            } else {
                format!("  {assignee}  {age:>5}")
            };
            let available =
                width.saturating_sub(marker.len() + warning.len() + Line::raw(&suffix).width());
            let title = fitted(&card.title, available);
            let blank = " ".repeat(available.saturating_sub(Line::raw(&title).width()));
            let metadata = if card.selected { selected } else { muted };
            let row = Rect::new(x, y, cell_width, 1);
            let text = Line::from(vec![
                Span::styled(format!("{marker}{warning}{title}{blank}"), style),
                Span::styled(suffix, metadata),
            ]);
            buf.set_style(row, style);
            buf.set_line(row.x, row.y, &text, row.width);
        }
    }
    if frame.shown == 0 && body.height > 1 {
        let empty = Rect::new(body.x, body.y + 1, body.width, 1);
        Clear.render(empty, buf);
        let message = if frame.total == 0 {
            "No cards on this board · b choose board"
        } else {
            "No cards match the filter · Esc clear"
        };
        line(
            message,
            empty,
            views.style(t::LIST_EMPTY, Style::default()),
            buf,
        );
    }
    if columns > 0 && (start > 0 || columns < frame.columns.len()) {
        let text = format!(
            "Columns {}–{} of {}",
            start + 1,
            start + columns,
            frame.columns.len()
        );
        let width = Line::raw(&text).width();
        if Line::raw(sanitize(&heading).as_ref()).width() + 2 + width <= usize::from(area.width) {
            let width = width as u16;
            line(
                &text,
                Rect::new(area.right() - width, area.y, width, 1),
                muted,
                buf,
            );
        }
    }
    body
}

pub struct KanbanView;
impl KanbanView {
    /// Select a card or column header, or activate a visible shortcut.
    pub fn hit_test(
        state: &KanbanBoardState,
        area: Rect,
        column: u16,
        row: u16,
    ) -> Option<BoardKey> {
        if area.is_empty() {
            return None;
        }
        let pos = Position::new(column, row);
        if detail_is_visible(state) {
            let layout = DetailLayout::new(area, state.notice.as_deref());
            if row != layout.hint_row || !area.contains(pos) {
                return None;
            }
            let key = hit_hint(
                layout.footer.last()?,
                &DETAIL_HINTS,
                DETAIL_ESCAPE,
                column - area.x,
            )?;
            return match key {
                Esc => Some(BoardKey::Esc),
                Scroll(motion) => {
                    let rows = detail_rows(state, layout.body.width, &ViewStyles::default());
                    Some(BoardKey::ScrollDetail {
                        motion,
                        viewport: layout.viewport(rows.len()),
                    })
                }
                _ => None,
            };
        }
        let frame = state.frame();
        let chrome = BoardChrome::for_state(state, area.height);
        let (hints, footer) = board_footer(&frame, area, &chrome);
        let layout = BoardLayout::new(&frame, area, &footer, &chrome);
        if row == layout.footer.y && layout.footer.contains(pos) {
            return match hit_hint(&hints, chrome.hints, chrome.escape, column - area.x)? {
                Esc => Some(BoardKey::Esc),
                Enter => Some(BoardKey::Enter),
                Char(c) => Some(BoardKey::Char(c)),
                Scroll(_) => None,
            };
        }
        if !state.browsing_grid() {
            return None;
        }
        for (offset, col) in frame
            .columns
            .iter()
            .skip(layout.start)
            .take(layout.columns)
            .enumerate()
        {
            let viewport = layout.column(offset, col);
            if viewport.rect.contains(pos) {
                if row == viewport.rect.y {
                    return Some(BoardKey::SelectColumn(layout.start + offset));
                }
                let visible = usize::from(row - viewport.rect.y - 1) / viewport.card_height;
                if visible < viewport.capacity {
                    return col
                        .cards
                        .get(viewport.first + visible)
                        .map(|card| BoardKey::SelectCard(card.id.clone()));
                }
                return None;
            }
        }
        None
    }

    pub fn draw(state: &KanbanBoardState, area: Rect, buf: &mut Buffer) {
        Self::draw_with_theme(state, area, buf, &ViewStyles::default());
    }

    pub fn draw_with_theme(
        state: &KanbanBoardState,
        area: Rect,
        buf: &mut Buffer,
        views: &ViewStyles,
    ) {
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }
        if detail_is_visible(state) {
            draw_detail(state, area, buf, views);
            views.finish(area, buf, t::LIST_BACKGROUND, true);
            return;
        }
        let frame = state.frame();
        let body = draw_columns(
            &frame,
            area,
            buf,
            BoardChrome::for_state(state, area.height),
            views,
        );
        if body.is_empty() {
            views.finish(area, buf, t::LIST_BACKGROUND, true);
            return;
        }
        if state.loading || state.error.is_some() {
            Clear.render(body, buf);
            let status = Rect::new(body.x, body.y, body.width, 1);
            if state.loading {
                line(
                    &format!("loading board {}...", state.board),
                    status,
                    views.style(t::LIST_EMPTY, Style::default()),
                    buf,
                );
            } else if let Some(error) = &state.error {
                line(
                    error,
                    status,
                    views.style(t::LIST_ERROR, Style::default()),
                    buf,
                );
                if body.height > 1 {
                    line(
                        "press r to retry, Esc to close",
                        Rect::new(body.x, body.y + 1, body.width, 1),
                        views.style(t::LIST_MUTED, Style::default()),
                        buf,
                    );
                }
            }
        } else if state.detail_id.is_some() {
            Clear.render(body, buf);
            let rows = detail_rows(state, body.width, views);
            let offset = state
                .detail_scroll()
                .min(rows.len().saturating_sub(usize::from(body.height)));
            draw_detail_rows(&rows, offset, body, buf);
        }
        let menu = state.menu_lines();
        if !menu.is_empty() {
            let height = (area.height / 2).min(body.height);
            let inner = Rect::new(body.x, body.bottom() - height, body.width, height);
            Clear.render(inner, buf);
            let selected = match &state.mode {
                BoardMode::Moves { selected, .. } | BoardMode::Boards { selected, .. } => {
                    Some(*selected)
                }
                _ => None,
            };
            let first = selected
                .unwrap_or(0)
                .saturating_sub(usize::from(height - 1));
            for (i, text) in menu
                .iter()
                .skip(first)
                .take(usize::from(height))
                .enumerate()
            {
                let token = if selected == Some(first + i) {
                    t::LIST_SELECTED
                } else if matches!(state.mode, BoardMode::Confirm { .. }) && i == 0 {
                    t::DIALOG_WARNING
                } else {
                    t::DIALOG_TEXT
                };
                line(
                    text,
                    Rect::new(inner.x, inner.y + i as u16, inner.width, 1),
                    views.style(token, Style::default()),
                    buf,
                );
            }
            views.finish(inner, buf, t::DIALOG_BACKGROUND, true);
        }
        views.finish(area, buf, t::LIST_BACKGROUND, true);
    }
}

struct DetailLayout {
    body: Rect,
    footer: Vec<String>,
    hint_row: u16,
}

const DETAIL_ESCAPE: &str = "Esc back to board";

impl DetailLayout {
    fn new(area: Rect, notice: Option<&str>) -> Self {
        let capacity = usize::from(area.height.saturating_sub(2));
        let notice_rows = capacity.saturating_sub(1).min(2);
        let mut footer: Vec<_> = notice
            .filter(|notice| !notice.is_empty())
            .into_iter()
            .flat_map(|notice| word_wrapped(&sanitize(notice), area.width))
            .take(notice_rows)
            .collect();
        if capacity > 0 {
            footer.push(fitted_hints(&DETAIL_HINTS, DETAIL_ESCAPE, area.width));
        }
        let body = Rect::new(
            area.x,
            area.y.saturating_add(1),
            area.width,
            area.height.saturating_sub(1 + footer.len() as u16),
        );
        Self {
            body,
            footer,
            hint_row: area.bottom() - 1,
        }
    }

    fn viewport(&self, total_rows: usize) -> DetailViewport {
        let page_rows = usize::from(self.body.height);
        let max_offset = if page_rows == 0 {
            0
        } else {
            total_rows.saturating_sub(page_rows)
        };
        DetailViewport {
            page_rows,
            max_offset,
        }
    }
}

/// Whether the visible board is showing a card and can accept detail navigation.
pub fn detail_is_visible(state: &KanbanBoardState) -> bool {
    state.detail_id.is_some()
        && state.mode == BoardMode::Browse
        && !state.loading
        && state.error.is_none()
}

/// The row count and scroll bound for the same wrapped detail viewport used by the renderer.
pub fn detail_viewport(state: &KanbanBoardState, area: Rect) -> Option<DetailViewport> {
    if !detail_is_visible(state) || area.is_empty() {
        return None;
    }
    let layout = DetailLayout::new(area, state.notice.as_deref());
    let rows = detail_rows(state, layout.body.width, &ViewStyles::default());
    Some(layout.viewport(rows.len()))
}

fn draw_detail(state: &KanbanBoardState, area: Rect, buf: &mut Buffer, views: &ViewStyles) {
    Clear.render(area, buf);
    let layout = DetailLayout::new(area, state.notice.as_deref());
    let rows = detail_rows(state, layout.body.width, views);
    let viewport = layout.viewport(rows.len());
    let offset = state.detail_scroll().min(viewport.max_offset);
    let header = if viewport.max_offset > 0 {
        let end = (offset + viewport.page_rows).min(rows.len());
        format!(
            "Card details · Lines {}–{end} of {}",
            offset + 1,
            rows.len()
        )
    } else {
        "Card details".into()
    };
    line(
        &header,
        Rect::new(area.x, area.y, area.width, 1),
        views.style(t::LIST_HEADER, Style::default().bold()),
        buf,
    );
    draw_detail_rows(&rows, offset, layout.body, buf);
    let muted = views.style(t::LIST_MUTED, Style::default().dim());
    for (y, text) in (layout.body.bottom()..area.bottom()).zip(layout.footer) {
        line(&text, Rect::new(area.x, y, area.width, 1), muted, buf);
    }
}

fn detail_rows(state: &KanbanBoardState, width: u16, views: &ViewStyles) -> Vec<Line<'static>> {
    if state.detail_fields.is_empty() {
        return vec![Line::styled(
            "Loading card…",
            views.style(t::LIST_EMPTY, Style::default()),
        )];
    }
    let base = views.style(t::LIST_ROW, Style::default());
    let lines = state.selected().map_or_else(
        || {
            state
                .detail_fields
                .iter()
                .map(|line| Line::raw(sanitize(line).into_owned()))
                .collect()
        },
        |card| card_details(card, &state.detail_fields, views),
    );
    lines
        .into_iter()
        .flat_map(|line| {
            let style = base.patch(line.style);
            word_wrapped(&line.to_string(), width)
                .into_iter()
                .map(move |part| Line::styled(part, style))
        })
        .collect()
}

fn draw_detail_rows(rows: &[Line<'_>], offset: usize, area: Rect, buf: &mut Buffer) {
    for (y, row) in (area.y..area.bottom()).zip(rows.iter().skip(offset)) {
        buf.set_line(area.x, y, row, area.width);
    }
}

fn card_details(card: &BoardCard, fields: &[String], views: &ViewStyles) -> Vec<Line<'static>> {
    let title = views.style(t::LIST_HEADER, Style::default().bold());
    let muted = views.style(t::LIST_MUTED, Style::default().dim());
    let mut rows = vec![
        Line::styled(sanitize(&card.title).into_owned(), title),
        Line::styled(
            format!("{} · {}", sanitize(&card.status), sanitize(&card.assignee)),
            muted,
        ),
    ];
    for (label, value) in [
        (
            "Latest result",
            card.latest_summary.as_deref().unwrap_or_default(),
        ),
        ("Description", card.body.as_str()),
    ] {
        if !value.trim().is_empty() {
            rows.push(Line::raw(""));
            rows.push(Line::styled(label, title));
            rows.extend(
                value
                    .lines()
                    .map(|line| Line::raw(sanitize(line).into_owned())),
            );
        }
    }
    rows.push(Line::raw(""));
    rows.extend(fields.iter().filter_map(|field| {
        if [
            "Title: ",
            "Status: ",
            "Assignee: ",
            "Latest result: ",
            "Body: ",
        ]
        .iter()
        .any(|prefix| field.starts_with(prefix))
        {
            return None;
        }
        Some(Line::raw(sanitize(field).into_owned()))
    }));
    rows
}
