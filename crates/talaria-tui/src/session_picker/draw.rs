//! Drawing the session picker overlay (#231).
//!
//! The layout follows the unit document: a header, up to two banners, the body
//! (group headers, rows and failed profiles), the notice line and the footer.
//! Every line starts with a one column margin, and every text is cut to the
//! width with the ellipsis rule of the unit document.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
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

use crate::history::{grapheme_width, sanitize};

/// The narrowest overlay that draws the list.
const MIN_WIDTH: u16 = 24;
/// The shortest overlay that draws the list.
const MIN_HEIGHT: u16 = 6;
/// The width of the age column.
const AGE_WIDTH: usize = 5;
/// The width of the message count column.
const MESSAGE_WIDTH: usize = 9;
/// The widest the profile column grows.
const PROFILE_CAP: usize = 12;
/// The widest the model column grows.
const MODEL_CAP: usize = 20;
/// The widest the source column grows.
const SOURCE_CAP: usize = 10;
/// Below this width the profile column is left out.
const PROFILE_MIN_WIDTH: u16 = 40;
/// Below this width the model column is left out.
const MODEL_MIN_WIDTH: u16 = 60;
/// Below this width the message count column is left out.
const MESSAGE_MIN_WIDTH: u16 = 80;
/// Below this width the source column is left out.
const SOURCE_MIN_WIDTH: u16 = 100;
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
/// The footer of a wide overlay.
const FOOTER_WIDE: &str = "\u{2191}\u{2193} move \u{b7} Enter open \u{b7} r rename \u{b7} b branch \
\u{b7} e export \u{b7} / search \u{b7} Tab profile \u{b7} Ctrl+R refresh \u{b7} Esc close";
/// The footer of an overlay that is not wide enough for the keys.
const FOOTER_NARROW: &str = "\u{2191}\u{2193} Enter r b e / Tab Esc";
/// The footer of the search line.
const FOOTER_SEARCH: &str = "type to filter \u{b7} Enter keep \u{b7} Esc clear";
/// The footer of the rename editor.
const FOOTER_RENAME: &str = "Enter rename \u{b7} Esc cancel";
/// The width the wide footer needs.
const FOOTER_WIDE_MIN_WIDTH: u16 = 110;
/// The width the narrow footer needs.
const FOOTER_NARROW_MIN_WIDTH: u16 = 60;

/// The picker's styles. A later unit fills them.
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
    /// Draw the picker into the area. Nothing outside the area is touched.
    pub fn draw(state: &PickerState, area: Rect, buf: &mut Buffer, now: f64, theme: &PickerTheme) {
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
        let banner_count = u16::try_from(banners.len().min(2)).unwrap_or(0);
        let rows = state.visible_rows();
        let columns = RowColumns::new(&rows, area.width);
        let dialog = match state.mode() {
            PickerMode::Export(dialog) => Some(dialog.clone()),
            _ => None,
        };
        // The export dialog takes the notice row and the footer row, and one row more.
        let dialog_rows = u16::from(dialog.is_some());
        let body_height = usize::from(area.height)
            .saturating_sub(3)
            .saturating_sub(usize::from(banner_count))
            .saturating_sub(usize::from(dialog_rows));

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

        let (body, cursor) = body_lines(state, &rows);
        // The window starts at the smallest offset that keeps the cursor row in sight.
        let offset = match cursor {
            Some(index) => index.saturating_add(1).saturating_sub(body_height),
            None => 0,
        };
        let selected = state.selected().map(|row| row.key.clone());
        let foreground = foreground_key(state);
        let body_top = area.y + 1 + banner_count;
        for (index, line) in body.iter().skip(offset).take(body_height).enumerate() {
            let y = body_top + u16::try_from(index).unwrap_or(0);
            let line = draw_body_line(
                line,
                &columns,
                selected.as_ref(),
                foreground.as_ref(),
                now,
                theme,
            );
            buf.set_line(area.x, y, &line, area.width);
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
    /// The source column, when it is shown.
    source: Option<usize>,
}

impl RowColumns {
    /// The columns the width allows, and the width the title gets.
    fn new(rows: &[&DirectoryRow], width: u16) -> Self {
        let profile = if width >= PROFILE_MIN_WIDTH && profile_count(rows) > 1 {
            let widest = rows
                .iter()
                .map(|row| columns(&row.key.profile))
                .max()
                .unwrap_or(0);
            Some(widest.min(PROFILE_CAP))
        } else {
            None
        };
        let model = if width >= MODEL_MIN_WIDTH {
            let widest = rows
                .iter()
                .map(|row| columns(short_model_text(row.model.as_deref())))
                .max()
                .unwrap_or(0);
            (widest > 0).then_some(widest.min(MODEL_CAP))
        } else {
            None
        };
        let messages = (width >= MESSAGE_MIN_WIDTH).then_some(MESSAGE_WIDTH);
        let source = if width >= SOURCE_MIN_WIDTH {
            let widest = rows
                .iter()
                .filter_map(|row| source_tag(row.source.as_deref()))
                .map(columns)
                .max()
                .unwrap_or(0);
            (widest > 0).then_some(widest.min(SOURCE_CAP))
        } else {
            None
        };
        let others = profile.unwrap_or(0)
            + model.unwrap_or(0)
            + AGE_WIDTH
            + messages.unwrap_or(0)
            + source.unwrap_or(0);
        let shown = 2
            + usize::from(profile.is_some())
            + usize::from(model.is_some())
            + usize::from(messages.is_some())
            + usize::from(source.is_some());
        let title = usize::from(width)
            .saturating_sub(1 + 4)
            .saturating_sub(others)
            .saturating_sub(2 * (shown - 1));
        Self {
            profile,
            title,
            model,
            messages,
            source,
        }
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
    for (origin, label) in [(RowOrigin::Live, "LIVE"), (RowOrigin::Stored, "EARLIER")] {
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
        lines.push(BodyLine::Group(" UNAVAILABLE".to_owned()));
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
        if is_foreground || row.running {
            theme.marker
        } else {
            Style::new()
        },
    ));

    let mut cells: Vec<Span<'static>> = Vec::new();
    if let Some(width) = columns.profile {
        cells.push(Span::raw(cut(&sanitize(&row.key.profile), width)));
    }
    if row.title.is_empty() {
        cells.push(Span::styled(cut(UNTITLED, columns.title), theme.dim));
    } else {
        cells.push(Span::raw(cut(&sanitize(&row.title), columns.title)));
    }
    if let Some(width) = columns.model {
        cells.push(Span::raw(cut(
            &sanitize(short_model_text(row.model.as_deref())),
            width,
        )));
    }
    cells.push(Span::raw(pad_left(
        &format_age(row.started_at, now),
        AGE_WIDTH,
    )));
    if let Some(width) = columns.messages {
        cells.push(Span::raw(pad_left(&message_text(row.message_count), width)));
    }
    if let Some(width) = columns.source {
        let tag = source_tag(row.source.as_deref()).unwrap_or_default();
        cells.push(Span::styled(cut(&sanitize(tag), width), theme.dim));
    }

    for (index, cell) in cells.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(cell);
    }
    Line::from(spans)
}

/// The header line: the count on the left, the state of the list on the right.
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
        let x = area.x + area.width - u16::try_from(right_width).unwrap_or(0);
        buf.set_stringn(x, area.y, right, right_width, theme.header);
    }
}

/// The header's right hand text, if any.
fn right_text(state: &PickerState, rows: &[&DirectoryRow]) -> Option<String> {
    if !matches!(state.mode(), PickerMode::Search) {
        let query = query_text(state);
        if !query.is_empty() {
            return Some(format!("Search: \"{query}\""));
        }
    }
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
        PickerMode::Search => Some(format!("Search: {}{}", query_text(state), CARET)),
        PickerMode::Waiting(waiting) => Some(waiting_line(waiting)),
        _ => state.notice().map(|notice| sanitize(notice).into_owned()),
    }
}

/// The rename editor: the old title, then the text with the caret in it.
fn rename_line(editor: &LineEditor) -> String {
    let text = sanitize(&editor.text);
    let caret = editor.caret.min(text.chars().count());
    let at = text
        .char_indices()
        .nth(caret)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    format!(
        "New title for \"{}\": {}{}{}",
        sanitize(&editor.original_title),
        &text[..at],
        CARET,
        &text[at..]
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
        _ => {
            if width >= FOOTER_WIDE_MIN_WIDTH {
                Some(FOOTER_WIDE)
            } else if width >= FOOTER_NARROW_MIN_WIDTH {
                Some(FOOTER_NARROW)
            } else {
                None
            }
        }
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

/// The tag a row's source shows, if it shows one.
fn source_tag(source: Option<&str>) -> Option<&str> {
    match source {
        None => None,
        Some("tui") | Some("cli") => None,
        Some("gui") | Some("desktop") => Some("desktop"),
        Some(other) => Some(other),
    }
}

/// The message count of a row.
fn message_text(count: u64) -> String {
    if count == 1 {
        "1 msg".to_owned()
    } else {
        format!("{count} msgs")
    }
}

/// The columns a text takes.
fn columns(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// The text cut to `width` columns: the longest prefix of whole characters that
/// fits in `width - 1` columns, then the ellipsis. Text that fits is padded.
fn cut(text: &str, width: usize) -> String {
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
        return text.to_owned();
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

/// The search text.
fn query_text(state: &PickerState) -> String {
    state.query().to_owned()
}

/// The profile the filter holds, as the header shows it.
fn filter_text(state: &PickerState) -> String {
    match state.filter() {
        ProfileFilter::All => "all profiles".to_owned(),
        ProfileFilter::One(profile) => format!("profile {}", sanitize(profile)),
    }
}
