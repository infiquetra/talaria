//! Session directory layout and rendering.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use talaria_core::directory::export::{ExportFormat, fallback_name};
use talaria_core::directory::rows::{
    DirectoryRow, RowKey, RowOrigin, format_age, short_model_text,
};
use talaria_core::directory::state::{
    ExportDialog, LineEditor, PickerMode, PickerState, ProfileFilter, ProfileLoad, WaitingFor,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::history::{grapheme_width, sanitize, word_wrapped};
use crate::theme::surfaces::{ViewStyles, tokens as t};

/// The narrowest overlay that draws the list.
const MIN_WIDTH: u16 = 24;
/// The shortest overlay that draws the list.
const MIN_HEIGHT: u16 = 6;
/// The width of the age column.
const AGE_WIDTH: usize = 10;
/// The width of the message count column.
const MESSAGE_WIDTH: usize = 9;
/// The widest the profile column grows.
const PROFILE_CAP: usize = 12;
/// The widest the model column grows.
const MODEL_CAP: usize = 20;
/// Below this width the profile column is left out.
const PROFILE_MIN_WIDTH: u16 = 40;
/// Below this width the model column is left out.
const MODEL_MIN_WIDTH: u16 = 92;
/// Below this width the message count column is left out.
const MESSAGE_MIN_WIDTH: u16 = 88;
/// The text an untitled row carries.
const UNTITLED: &str = "untitled session";
/// The text the overlay draws when it does not fit.
const TOO_SMALL: &str = " Window too small for the session list. Esc closes.";
/// The caret block the editor and the search line end with.
const CARET: &str = "\u{258f}";
/// The opening of the scope note, which is already a whole sentence.
const SCOPE_NOTE_PREFIX: &str = "Showing only the profile";
/// The opening of the side failure sentence.
const SIDE_FAILURE_PREFIX: &str = "Could not reach Hermes to list sessions";
const FOOTER: &str = "↑↓ move · Enter open · / search · Tab profile · Esc close";
const FOOTER_NARROW: &str = "Enter open · / search · Esc close";
const ACTIONS: &str = "r rename · b branch · e export · Ctrl+R refresh";
/// The footer of the search line.
const FOOTER_SEARCH: &str = "type to filter \u{b7} Enter keep \u{b7} Esc clear";
/// The footer of the rename editor.
const FOOTER_RENAME: &str = "Enter rename \u{b7} Esc cancel";
/// The picker's styles.
#[derive(Clone, Debug)]
pub struct PickerTheme {
    /// The header line.
    pub header: Style,
    /// A group header.
    pub group: Style,
    /// The cursor and the foreground marker.
    pub marker: Style,
    /// Text that steps back: an untitled row, a source tag, a failed profile.
    pub dim: Style,
    /// The notice line and the empty state.
    pub notice: Style,
    /// The footer.
    pub footer: Style,
    /// The chosen format of the export dialog.
    pub chosen: Style,
    row: Style,
    divider: Style,
    running: Style,
}

impl PickerTheme {
    /// The default theme.
    pub fn new() -> Self {
        Self {
            header: Style::new().bold(),
            group: Style::new().bold(),
            marker: Style::new().bold(),
            dim: Style::new().add_modifier(Modifier::DIM),
            notice: Style::new(),
            footer: Style::new().add_modifier(Modifier::DIM),
            chosen: Style::new().add_modifier(Modifier::REVERSED),
            row: Style::new(),
            divider: Style::new().dim(),
            running: Style::new().bold(),
        }
    }

    fn resolve(views: &ViewStyles) -> Self {
        let base = Self::new();
        let chosen = views.style(t::LIST_SELECTED, base.chosen);
        let chosen = if views.color() {
            chosen
        } else {
            chosen.reversed()
        };
        Self {
            header: views.style(t::LIST_HEADER, base.header),
            group: views.style(t::LIST_MUTED, base.group),
            marker: views.style(t::LIST_SELECTED, base.marker),
            dim: views.style(t::LIST_MUTED, base.dim),
            notice: views.style(t::LIST_EMPTY, base.notice),
            footer: views.style(t::LIST_MUTED, base.footer),
            chosen,
            row: views.style(t::LIST_ROW, base.row),
            divider: views.style(t::LIST_DIVIDER, base.divider),
            running: views.style(t::LIST_RUNNING, base.running),
        }
    }
}

impl Default for PickerTheme {
    fn default() -> Self {
        Self::new()
    }
}

/// Draws the picker state inside the overlay closure.
pub struct SessionPickerView;

impl SessionPickerView {
    /// Resolve a terminal coordinate to the session row drawn at that position.
    pub fn hit_test(state: &PickerState, area: Rect, column: u16, row: u16) -> Option<&RowKey> {
        if area.width < MIN_WIDTH
            || area.height < MIN_HEIGHT
            || !matches!(state.mode(), PickerMode::List | PickerMode::Search)
        {
            return None;
        }
        let rows = state.visible_rows();
        let banner_count = state.banners().len().min(2) as u16;
        let layout = DirectoryLayout::new(state, area, banner_count, !rows.is_empty());
        if !layout.rows.contains(Position::new(column, row)) {
            return None;
        }
        let (body, cursor) = body_lines(state, &rows);
        let offset = body_offset(cursor, layout.rows.height) + usize::from(row - layout.rows.y);
        match body.get(offset) {
            Some(BodyLine::Row(row)) => Some(&row.key),
            _ => None,
        }
    }

    /// Draw the picker with the active theme and terminal glyph constraints.
    pub fn draw_with_theme(
        state: &PickerState,
        area: Rect,
        buf: &mut Buffer,
        now: f64,
        views: &ViewStyles,
    ) {
        Self::draw_directory(
            state,
            area,
            buf,
            now,
            &PickerTheme::resolve(views),
            Some(views),
        );
        views.finish(area, buf, t::LIST_BACKGROUND, true);
    }

    /// Draw the picker into the area. Nothing outside the area is touched.
    pub fn draw(state: &PickerState, area: Rect, buf: &mut Buffer, now: f64, theme: &PickerTheme) {
        Self::draw_directory(state, area, buf, now, theme, None);
    }

    fn draw_directory(
        state: &PickerState,
        area: Rect,
        buf: &mut Buffer,
        now: f64,
        theme: &PickerTheme,
        views: Option<&ViewStyles>,
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
            let width = usize::from(area.width);
            buf.set_stringn(area.x, area.y, cut(TOO_SMALL, width), width, theme.notice);
            return;
        }

        let banners = state.banners();
        let banner_count = banners.len().min(2) as u16;
        let rows = state.visible_rows();
        let selected = state.selected();
        let dialog = match state.mode() {
            PickerMode::Export(dialog) => Some(dialog.clone()),
            _ => None,
        };
        let layout = DirectoryLayout::new(state, area, banner_count, !rows.is_empty());
        let list = layout.list;
        let details = layout.details;
        let columns = RowColumns::new(&rows, list.width);

        draw_header(state, &rows, area, buf, theme);
        for (offset, banner) in banners.iter().take(2).enumerate() {
            let y = area.y + 1 + u16::try_from(offset).unwrap_or(0);
            let text = cut(&banner_line(banner), usize::from(area.width) - 1);
            buf.set_stringn(
                area.x,
                y,
                format!(" {text}"),
                usize::from(area.width),
                theme.notice,
            );
        }

        if let Some(search_row) = layout.search_row {
            let query = query_text(state);
            let (label, style) = if matches!(state.mode(), PickerMode::Search) {
                (format!(" Search  {query}{CARET}"), theme.row)
            } else if !query.is_empty() {
                (format!(" Search  {query}"), theme.row)
            } else {
                (" Search  Press / to filter".to_owned(), theme.dim)
            };
            let width = usize::from(area.width);
            buf.set_stringn(area.x, search_row, cut(&label, width), width, style);
        }

        if list.y < layout.rows.y {
            let header = columns.header();
            buf.set_stringn(list.x, list.y, header, usize::from(list.width), theme.group);
        }

        let (body, cursor) = body_lines(state, &rows);
        let body_height = usize::from(layout.rows.height);
        let offset = body_offset(cursor, layout.rows.height);
        let foreground = foreground_key(state);
        for (index, line) in body.iter().skip(offset).take(body_height).enumerate() {
            let y = layout.rows.y + u16::try_from(index).unwrap_or(0);
            let selected_key = selected.map(|row| &row.key);
            if matches!(line, BodyLine::Row(row) if selected_key == Some(&row.key)) {
                buf.set_style(Rect::new(list.x, y, list.width, 1), theme.chosen);
            }
            let line = draw_body_line(
                line,
                &columns,
                selected_key,
                foreground.as_ref(),
                now,
                theme,
            );
            buf.set_line(list.x, y, &line, list.width);
        }

        if let Some(row) = selected.filter(|_| !details.is_empty()) {
            draw_details(row, foreground.as_ref(), now, details, buf, theme, views);
        }

        let notice_row = area.bottom().saturating_sub(2);
        let footer_row = area.bottom().saturating_sub(1);
        match &dialog {
            Some(dialog) => {
                draw_export_dialog(dialog, &rows, state.save_folder(), area, buf, theme);
            }
            None => {
                if let Some(notice) = notice_line(state) {
                    let text = cut(&notice, usize::from(area.width) - 1);
                    buf.set_stringn(
                        area.x,
                        notice_row,
                        format!(" {text}"),
                        usize::from(area.width),
                        theme.notice,
                    );
                } else if area.width >= 48 && state.selected().is_some() {
                    buf.set_stringn(
                        area.x + 1,
                        notice_row,
                        ACTIONS,
                        usize::from(area.width - 1),
                        theme.footer,
                    );
                }
                if let Some(footer) = footer_text(state.mode(), area.width) {
                    let text = cut(footer, usize::from(area.width) - 1);
                    buf.set_stringn(
                        area.x,
                        footer_row,
                        format!(" {text}"),
                        usize::from(area.width),
                        theme.footer,
                    );
                }
            }
        }
    }
}

fn body_offset(cursor: Option<usize>, height: u16) -> usize {
    // Start at the smallest offset that keeps the selected row in view.
    cursor.map_or(0, |index| {
        index.saturating_add(1).saturating_sub(usize::from(height))
    })
}

struct DirectoryLayout {
    list: Rect,
    details: Rect,
    rows: Rect,
    search_row: Option<u16>,
}

impl DirectoryLayout {
    fn new(state: &PickerState, area: Rect, banner_count: u16, has_rows: bool) -> Self {
        // The export dialog takes the notice row and the footer row, and one row more.
        let dialog_rows = u16::from(matches!(state.mode(), PickerMode::Export(_)));
        let search = (area.width >= 48 && area.height >= 12)
            || !state.query().is_empty()
            || matches!(state.mode(), PickerMode::Search);
        let column_header = area.height >= 12 && has_rows;
        let body_top = area.y + 1 + banner_count + u16::from(search);
        let body = Rect::new(
            area.x,
            body_top,
            area.width,
            area.bottom()
                .saturating_sub(2 + dialog_rows)
                .saturating_sub(body_top),
        );
        let (list, details) = match state.selected() {
            Some(_) if body.width >= 112 && body.height >= 12 => {
                let list_width = body.width - (body.width / 3).clamp(39, 64);
                (
                    Rect::new(body.x, body.y, list_width, body.height),
                    Rect::new(
                        body.x + list_width,
                        body.y,
                        body.width - list_width,
                        body.height,
                    ),
                )
            }
            Some(_) if body.width >= 54 && body.height >= 10 => {
                let details_height = if body.height >= 16 { 6 } else { 4 };
                (
                    Rect::new(body.x, body.y, body.width, body.height - details_height),
                    Rect::new(
                        body.x,
                        body.bottom() - details_height,
                        body.width,
                        details_height,
                    ),
                )
            }
            _ => (body, Rect::default()),
        };
        let header_height = u16::from(column_header);
        Self {
            list,
            details,
            rows: Rect::new(
                list.x,
                list.y + header_height,
                list.width,
                list.height.saturating_sub(header_height),
            ),
            search_row: search.then_some(body_top - 1),
        }
    }
}

/// The widths of the row line's columns.
struct RowColumns {
    /// The profile column, when it is shown.
    profile: Option<usize>,
    /// The title column, always shown.
    title: usize,
    /// The model column, when it is shown.
    model: Option<usize>,
    /// The message count column, when it is shown.
    messages: Option<usize>,
}

impl RowColumns {
    /// The columns the width allows, and the width the title gets.
    fn new(rows: &[&DirectoryRow], width: u16) -> Self {
        let profile = if width >= PROFILE_MIN_WIDTH && profile_count(rows) > 1 {
            let widest = rows
                .iter()
                .map(|row| columns(&sanitize(&row.key.profile)))
                .max()
                .unwrap_or(0);
            Some(widest.clamp("Profile".len(), PROFILE_CAP))
        } else {
            None
        };
        let model = if width >= MODEL_MIN_WIDTH {
            let widest = rows
                .iter()
                .map(|row| columns(&sanitize(short_model_text(row.model.as_deref()))))
                .max()
                .unwrap_or(0);
            (widest > 0).then_some(widest.clamp("Model".len(), MODEL_CAP))
        } else {
            None
        };
        let messages = (width >= MESSAGE_MIN_WIDTH).then_some(MESSAGE_WIDTH);
        let others = profile.unwrap_or(0) + model.unwrap_or(0) + AGE_WIDTH + messages.unwrap_or(0);
        let shown = 2
            + usize::from(profile.is_some())
            + usize::from(model.is_some())
            + usize::from(messages.is_some());
        let title = usize::from(width)
            .saturating_sub(6)
            .saturating_sub(others)
            .saturating_sub(2 * (shown - 1));
        Self {
            profile,
            title,
            model,
            messages,
        }
    }

    fn header(&self) -> String {
        let mut cells = vec![cut("Session", self.title)];
        if let Some(width) = self.profile {
            cells.push(cut("Profile", width));
        }
        if let Some(width) = self.model {
            cells.push(cut("Model", width));
        }
        cells.push(pad_left("Started", AGE_WIDTH));
        if let Some(width) = self.messages {
            cells.push(pad_left("Messages", width));
        }
        format!("     {}", cells.join("  "))
    }
}

/// One line of the body, before it is drawn.
enum BodyLine<'a> {
    /// A group header.
    Group(String),
    /// A session row.
    Row(&'a DirectoryRow),
    /// A profile whose list could not be read.
    Unavailable(String, RowOrigin, String),
    /// The empty state.
    Empty,
}

/// The body of the list, and the place of the cursor row in it.
fn body_lines<'a>(
    state: &PickerState,
    rows: &[&'a DirectoryRow],
) -> (Vec<BodyLine<'a>>, Option<usize>) {
    let selected = state.selected().map(|row| row.key.clone());
    let mut lines = Vec::new();
    let mut cursor = None;
    for (origin, label) in [(RowOrigin::Live, "Live"), (RowOrigin::Stored, "Earlier")] {
        let group: Vec<&DirectoryRow> = rows
            .iter()
            .copied()
            .filter(|row| row.origin == origin)
            .collect();
        if group.is_empty() {
            continue;
        }
        lines.push(BodyLine::Group(format!(" {label} ({})", group.len())));
        for row in group {
            if selected.as_ref() == Some(&row.key) {
                cursor = Some(lines.len());
            }
            lines.push(BodyLine::Row(row));
        }
    }
    if rows.is_empty() {
        lines.push(BodyLine::Empty);
    }
    let unavailable = state.unavailable();
    if !unavailable.is_empty() {
        lines.push(BodyLine::Group(" Unavailable".to_owned()));
        lines.extend(
            unavailable
                .into_iter()
                .map(|(profile, origin, reason)| BodyLine::Unavailable(profile, origin, reason)),
        );
    }
    (lines, cursor)
}

/// The drawn text of one body line.
fn draw_body_line(
    line: &BodyLine<'_>,
    columns: &RowColumns,
    selected: Option<&RowKey>,
    foreground: Option<&RowKey>,
    now: f64,
    theme: &PickerTheme,
) -> Line<'static> {
    match line {
        BodyLine::Group(text) => Line::from(text.clone()).style(theme.group),
        BodyLine::Empty => Line::from(" No sessions found.").style(theme.notice),
        BodyLine::Unavailable(profile, origin, reason) => {
            let scope = match origin {
                RowOrigin::Live => "live",
                RowOrigin::Stored => "earlier",
            };
            Line::from(format!(
                "   {}: could not list {scope} sessions ({}).",
                sanitize(profile),
                sanitize(reason)
            ))
            .style(theme.dim)
        }
        BodyLine::Row(row) => {
            let is_selected = selected == Some(&row.key);
            let is_foreground = foreground == Some(&row.key);
            row_line(row, columns, is_selected, is_foreground, now, theme)
        }
    }
}

/// The line of one session row.
fn row_line(
    row: &DirectoryRow,
    columns: &RowColumns,
    is_selected: bool,
    is_foreground: bool,
    now: f64,
    theme: &PickerTheme,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    spans.push(Span::styled(
        if is_selected { "\u{25b8} " } else { "  " }.to_owned(),
        if is_selected {
            theme.marker
        } else {
            Style::new()
        },
    ));
    spans.push(Span::styled(
        if is_foreground {
            "\u{25cf} "
        } else if row.running {
            "\u{25d0} "
        } else {
            "  "
        }
        .to_owned(),
        if !is_selected && (is_foreground || row.running) {
            theme.running
        } else {
            Style::new()
        },
    ));

    let mut cells: Vec<Span<'static>> = Vec::new();
    cells.push(Span::styled(
        cut(session_title(row), columns.title),
        if row.title.is_empty() && !is_selected {
            theme.dim
        } else {
            Style::new()
        },
    ));
    let secondary = if is_selected { Style::new() } else { theme.dim };
    if let Some(width) = columns.profile {
        cells.push(Span::styled(cut(&row.key.profile, width), secondary));
    }
    if let Some(width) = columns.model {
        cells.push(Span::styled(
            cut(short_model_text(row.model.as_deref()), width),
            secondary,
        ));
    }
    cells.push(Span::styled(
        pad_left(&format_age(row.started_at, now), AGE_WIDTH),
        secondary,
    ));
    if let Some(width) = columns.messages {
        cells.push(Span::styled(
            pad_left(&row.message_count.to_string(), width),
            secondary,
        ));
    }

    for (index, cell) in cells.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(cell);
    }
    Line::from(spans).style(if is_selected { theme.chosen } else { theme.row })
}

fn session_title(row: &DirectoryRow) -> &str {
    if row.title.is_empty() {
        UNTITLED
    } else {
        &row.title
    }
}

fn draw_details(
    row: &DirectoryRow,
    foreground: Option<&RowKey>,
    now: f64,
    mut area: Rect,
    buf: &mut Buffer,
    theme: &PickerTheme,
    views: Option<&ViewStyles>,
) {
    let side = area.height >= 12;
    if side {
        let divider = views.map_or("│", |views| views.divider_symbol("│"));
        for y in area.y..area.bottom() {
            buf.set_stringn(area.x, y, divider, 1, theme.divider);
        }
    }
    let inset = if side { 2 } else { 1 };
    area.x += inset;
    area.width -= inset;
    buf.set_stringn(
        area.x,
        area.y,
        "Selected session",
        usize::from(area.width),
        theme.group,
    );

    let status = if foreground == Some(&row.key) {
        "Current session"
    } else if row.running {
        "Running"
    } else {
        match row.origin {
            RowOrigin::Live => "Live",
            RowOrigin::Stored => "Earlier",
        }
    };
    let age = format_age(row.started_at, now);
    let messages = row.message_count.to_string();
    let source = row
        .source
        .as_deref()
        .filter(|source| !source.is_empty())
        .unwrap_or("—");
    let model = row
        .model
        .as_deref()
        .filter(|model| !model.is_empty())
        .unwrap_or("—");
    if side {
        let mut y = 1;
        for part in detail_rows(session_title(row), area.width, 2) {
            draw_detail_line(Line::styled(part, theme.header), area, y, buf);
            y += 1;
        }
        y += 1;
        let fields = [
            ("Status", status),
            ("Model", model),
            ("Profile", row.key.profile.as_str()),
            ("Started", &age),
            ("Messages", &messages),
            ("Session ID", &row.key.session_id),
            ("Source", source),
        ];
        let width = usize::from(area.width);
        let label_width = 11;
        for (index, (label, value)) in fields.into_iter().enumerate() {
            if y >= area.height {
                break;
            }
            let value = sanitize(value);
            let remaining_fields = fields.len() - index - 1;
            let available = usize::from(area.height - y).saturating_sub(remaining_fields);
            if columns(&value) > width - label_width && available >= 2 {
                draw_detail_line(Line::styled(label, theme.dim), area, y, buf);
                y += 1;
                for part in detail_rows(&value, area.width, available - 1) {
                    draw_detail_line(Line::styled(part, theme.row), area, y, buf);
                    y += 1;
                }
            } else {
                let line = Line::from(vec![
                    Span::styled(format!("{label:<label_width$}"), theme.dim),
                    Span::styled(cut(&value, width - label_width), theme.row),
                ]);
                draw_detail_line(line, area, y, buf);
                y += 1;
            }
        }
    } else {
        let width = usize::from(area.width);
        let model_rows = if area.height >= 6 { 2 } else { 1 };
        let mut y = 1;
        for part in detail_rows(model, area.width - 7, model_rows) {
            let line = Line::from(vec![
                Span::styled(if y == 1 { "Model  " } else { "       " }, theme.dim),
                Span::styled(part, theme.row),
            ]);
            draw_detail_line(line, area, y, buf);
            y += 1;
        }
        let profile = format!("{} · {status}", sanitize(&row.key.profile));
        let source = format!("{} · {messages} messages · Started {age}", sanitize(source));
        let fields = [
            ("Profile  ", profile.as_str()),
            ("Source  ", source.as_str()),
            ("Session ID  ", &row.key.session_id),
        ];
        for (offset, (label, value)) in (y..area.height).zip(fields) {
            let line = Line::from(vec![
                Span::styled(label, theme.dim),
                Span::styled(cut(value, width - columns(label)), theme.row),
            ]);
            draw_detail_line(line, area, offset, buf);
        }
    }
}

fn detail_rows(text: &str, width: u16, maximum: usize) -> Vec<String> {
    let mut rows = word_wrapped(&sanitize(text), width);
    if rows.len() > maximum {
        rows.truncate(maximum);
        if let Some(last) = rows.last_mut() {
            *last = cut(&format!("{last}…"), usize::from(width));
        }
    }
    rows
}

fn draw_detail_line(line: Line<'_>, area: Rect, offset: u16, buf: &mut Buffer) {
    buf.set_line(area.x, area.y + offset, &line, area.width);
}

/// The header line: the count on the left, the loading state on the right.
fn draw_header(
    state: &PickerState,
    rows: &[&DirectoryRow],
    area: Rect,
    buf: &mut Buffer,
    theme: &PickerTheme,
) {
    let width = usize::from(area.width);
    let left = format!(
        " Sessions \u{b7} {} \u{b7} {} shown",
        filter_text(state),
        rows.len()
    );
    // The right hand text is left out when the left text, two spaces and it do
    // not fit in the width.
    let right = right_text(state, rows).filter(|text| columns(&left) + 2 + columns(text) <= width);
    let left = cut(&left, width - 1);
    buf.set_stringn(area.x, area.y, left, width - 1, theme.header);
    if let Some(right) = right {
        let right_width = columns(&right);
        let right_cols = u16::try_from(right_width).unwrap_or(area.width);
        let x = area
            .x
            .saturating_add(area.width.saturating_sub(right_cols.min(area.width)));
        let room = usize::from(area.right().saturating_sub(x));
        buf.set_stringn(x, area.y, right, right_width.min(room), theme.header);
    }
}

/// The header's right hand text, if any.
fn right_text(state: &PickerState, rows: &[&DirectoryRow]) -> Option<String> {
    if rows.is_empty() {
        return state
            .pending_profile()
            .map(|profile| format!("Loading {}\u{2026}", sanitize(profile)));
    }
    if shows_cached_rows(state, rows) {
        return Some("Refreshing\u{2026}".to_owned());
    }
    None
}

/// Whether the rows on screen are the cached ones.
///
/// A row whose profile and scope has not answered yet is a row from the
/// previous listing, so the list is being read again under the operator.
fn shows_cached_rows(state: &PickerState, rows: &[&DirectoryRow]) -> bool {
    let loads = state.loads();
    rows.iter().any(|row| {
        matches!(
            loads.get(&(row.key.profile.clone(), row.origin)),
            Some(ProfileLoad::Pending)
        )
    })
}

/// The notice line, if any. The text carries no margin.
fn notice_line(state: &PickerState) -> Option<String> {
    match state.mode() {
        PickerMode::Rename(editor) => Some(rename_line(editor)),
        PickerMode::Search => None,
        PickerMode::Waiting(waiting) => Some(waiting_line(waiting)),
        _ => state.notice().map(|notice| sanitize(notice).into_owned()),
    }
}

/// The rename editor: the old title, then the text with the caret in it.
///
/// The caret is a character index into the raw editor text. Each side is
/// sanitized on its own so a stand-in that is wider than one character cannot
/// leave the caret in the middle of that stand-in.
fn rename_line(editor: &LineEditor) -> String {
    let caret = editor.caret.min(editor.text.chars().count());
    let at = editor
        .text
        .char_indices()
        .nth(caret)
        .map(|(index, _)| index)
        .unwrap_or(editor.text.len());
    let (head, tail) = editor.text.split_at(at);
    format!(
        "New title for \"{}\": {}{}{}",
        sanitize(&editor.original_title),
        sanitize(head),
        CARET,
        sanitize(tail)
    )
}

/// The notice of a call that runs.
fn waiting_line(waiting: &WaitingFor) -> String {
    match waiting {
        WaitingFor::Rename(_) => "Renaming\u{2026}".to_owned(),
        WaitingFor::Branch => "Branching\u{2026}".to_owned(),
        WaitingFor::Export(_) => "Exporting\u{2026}".to_owned(),
        WaitingFor::Open(_) => "Opening\u{2026}".to_owned(),
    }
}

/// The footer of the mode the picker is in.
fn footer_text(mode: &PickerMode, width: u16) -> Option<&'static str> {
    match mode {
        PickerMode::Search => Some(FOOTER_SEARCH),
        PickerMode::Rename(_) => Some(FOOTER_RENAME),
        _ if width >= 58 => Some(FOOTER),
        _ if width >= 35 => Some(FOOTER_NARROW),
        _ => Some("Enter open · Esc close"),
    }
}

/// The three lines of the export dialog: the title, the formats and the file.
fn draw_export_dialog(
    dialog: &ExportDialog,
    rows: &[&DirectoryRow],
    save_folder: &str,
    area: Rect,
    buf: &mut Buffer,
    theme: &PickerTheme,
) {
    let width = usize::from(area.width) - 1;
    let title = rows
        .iter()
        .find(|row| row.key == dialog.key)
        .map(|row| row.title.clone())
        .unwrap_or_default();
    let title = if title.is_empty() {
        UNTITLED.to_owned()
    } else {
        title
    };
    let heading = cut(&format!("Export \"{}\"", sanitize(&title)), width);
    buf.set_stringn(
        area.x,
        area.bottom().saturating_sub(3),
        format!(" {heading}"),
        usize::from(area.width),
        theme.notice,
    );

    let mut spans: Vec<Span<'static>> = vec![Span::raw(" Format: ")];
    for (index, (key, format, label)) in [
        ("m", ExportFormat::Markdown, ExportFormat::Markdown.label()),
        ("j", ExportFormat::Json, ExportFormat::Json.label()),
        ("h", ExportFormat::Html, ExportFormat::Html.label()),
    ]
    .into_iter()
    .enumerate()
    {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::raw(format!("[{key}] ")));
        let style = if dialog.format == format {
            theme.chosen
        } else {
            Style::new()
        };
        spans.push(Span::styled(label.to_owned(), style));
    }
    spans.push(Span::raw("          "));
    let secrets = if dialog.redact {
        "Secrets: redacted (x to include them)"
    } else {
        "Secrets: included (x to redact them)"
    };
    spans.push(Span::raw(secrets.to_owned()));
    let formats = cut_line(&Line::from(spans), width);
    buf.set_line(
        area.x,
        area.bottom().saturating_sub(2),
        &formats,
        area.width,
    );

    // The dialog names the folder the export really goes to.
    let path = format!(
        "{}/{}",
        sanitize(save_folder),
        fallback_name(&dialog.key.session_id, dialog.format)
    );
    let saves = cut(
        &format!("Saves to {path} \u{b7} Enter export \u{b7} Esc cancel"),
        width,
    );
    buf.set_stringn(
        area.x,
        area.bottom().saturating_sub(1),
        format!(" {saves}"),
        usize::from(area.width),
        theme.notice,
    );
}

/// The banner line of one banner.
///
/// The scope note already reads as a sentence. The side failure carries only a
/// reason, so the sentence around it is built here.
fn banner_line(banner: &str) -> String {
    let text = sanitize(banner);
    if text.starts_with(SCOPE_NOTE_PREFIX) || text.starts_with(SIDE_FAILURE_PREFIX) {
        return text.into_owned();
    }
    format!("Could not reach Hermes to list sessions ({text}). Ctrl+R tries again.")
}

/// The number of profiles the visible rows carry.
fn profile_count(rows: &[&DirectoryRow]) -> usize {
    let mut names: Vec<&str> = Vec::new();
    for row in rows {
        if !names.contains(&row.key.profile.as_str()) {
            names.push(row.key.profile.as_str());
        }
    }
    names.len()
}

/// The columns a text takes.
fn columns(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// The text cut to `width` columns: the longest prefix of whole characters that
/// fits in `width - 1` columns, then the ellipsis. Text that fits is padded.
///
/// Sanitized first, so the column count is the stand-in width and no control,
/// line separator, or paragraph separator is copied into the result.
fn cut(text: &str, width: usize) -> String {
    let clean = sanitize(text);
    let text = clean.as_ref();
    let used = columns(text);
    if used <= width {
        let mut out = text.to_owned();
        out.push_str(&" ".repeat(width - used));
        return out;
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut taken = 0;
    for grapheme in text.graphemes(true) {
        let step = usize::from(grapheme_width(grapheme));
        if taken + step > width - 1 {
            break;
        }
        out.push_str(grapheme);
        taken += step;
    }
    out.push('\u{2026}');
    out.push_str(&" ".repeat(width.saturating_sub(taken + 1)));
    out
}

/// The text in `width` columns, right aligned.
fn pad_left(text: &str, width: usize) -> String {
    let used = columns(text);
    if used >= width {
        return cut(text, width);
    }
    format!("{}{text}", " ".repeat(width - used))
}

/// The line cut to `width` columns: the text that fits, then the ellipsis. The
/// styles of the spans are kept.
fn cut_line(line: &Line<'static>, width: usize) -> Line<'static> {
    let used: usize = line
        .spans
        .iter()
        .map(|span| columns(span.content.as_ref()))
        .sum();
    if used <= width {
        return line.clone();
    }
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut taken = 0;
    for span in &line.spans {
        if taken >= width - 1 {
            break;
        }
        let room = width - 1 - taken;
        let (kept, overflow) = split_at_columns(span.content.as_ref(), room);
        taken += columns(&kept);
        if !kept.is_empty() {
            spans.push(Span::styled(kept, span.style));
        }
        if overflow {
            break;
        }
    }
    let style = line.spans.last().map_or(Style::new(), |span| span.style);
    spans.push(Span::styled("\u{2026}".to_owned(), style));
    Line::from(spans)
}

/// The text that fits in `room` columns, and whether the rest did not fit.
fn split_at_columns(text: &str, room: usize) -> (String, bool) {
    let mut kept = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let step = usize::from(grapheme_width(grapheme));
        if used + step > room {
            return (kept, true);
        }
        kept.push_str(grapheme);
        used += step;
    }
    (kept, false)
}

/// The session the foreground holds.
fn foreground_key(state: &PickerState) -> Option<RowKey> {
    state.foreground().cloned()
}

/// The search text, with controls and separators already replaced by stand-ins.
fn query_text(state: &PickerState) -> String {
    sanitize(state.query()).into_owned()
}

/// The profile the filter holds, as the header shows it.
fn filter_text(state: &PickerState) -> String {
    match state.filter() {
        ProfileFilter::All => "all profiles".to_owned(),
        ProfileFilter::One(profile) => format!("profile {}", sanitize(profile)),
    }
}
