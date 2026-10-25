//! Tool-call rows and the drafting line. Issue 221.

use std::borrow::Cow;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use serde_json::{Map, Value};
use talaria_core::session::{Cell, CellId, OutcomeView, ToolCell, ToolStatus, ToolView, tool_view};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::diff_view::{DiffRenderOptions, diff_lines};
use crate::history::{CellLines, sanitize};
use crate::live::LiveSection;

/// Compact scrollback, or the full form the transcript can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    /// The short form written into scrollback.
    Compact,
    /// Arguments, output and the whole diff.
    Full,
}

/// Named colours for a tool cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolTheme {
    /// The mark while a call runs.
    pub glyph_running: Style,
    /// The mark when a call succeeded.
    pub glyph_ok: Style,
    /// The mark when a call failed.
    pub glyph_failed: Style,
    /// The mark when a call did not finish, and an unknown row.
    pub glyph_muted: Style,
    /// The tool title.
    pub title: Style,
    /// What the call acted on.
    pub subject: Style,
    /// Duration, progress, counts and hints.
    pub meta: Style,
    /// Ordinary output.
    pub output: Style,
    /// Failure text.
    pub error: Style,
    /// An added diff row.
    pub diff_added: Style,
    /// A removed diff row.
    pub diff_removed: Style,
    /// An unchanged diff row.
    pub diff_context: Style,
    /// A hunk header after the first.
    pub diff_hunk: Style,
    /// A file row when a diff has several files.
    pub diff_file: Style,
    /// A diff note.
    pub diff_note: Style,
    /// The line number column.
    pub line_number: Style,
}

impl Default for ToolTheme {
    fn default() -> Self {
        let green = Style::default().fg(Color::Green);
        let red = Style::default().fg(Color::Red);
        let dim = Style::default().add_modifier(Modifier::DIM);
        let bold = Style::default().add_modifier(Modifier::BOLD);
        Self {
            glyph_running: dim,
            glyph_ok: green,
            glyph_failed: red,
            glyph_muted: dim,
            title: bold,
            subject: Style::default(),
            meta: dim,
            output: Style::default(),
            error: red,
            diff_added: green,
            diff_removed: red,
            diff_context: Style::default(),
            diff_hunk: Style::default().fg(Color::Cyan),
            diff_file: bold,
            diff_note: dim,
            line_number: dim,
        }
    }
}

/// What one draw of a tool cell is allowed to use.
pub struct ToolRenderOptions {
    /// Columns for every row.
    pub width: u16,
    /// Colours and weight.
    pub theme: ToolTheme,
    /// When false, colour is removed and bold and dim stay.
    pub color: bool,
    /// Compact or full.
    pub detail: Detail,
    /// The whole phrase inside the parentheses of a more-lines row.
    pub expand_hint: Option<String>,
    /// How long a running call has been on screen.
    pub elapsed: Option<Duration>,
}

/// The mark that starts every row.
const GLYPH: &str = "\u{23fa}";
/// Spaces before diff rows in a tool cell.
const DIFF_INDENT: u16 = 4;
/// Most screen rows of diff in the compact form.
const COMPACT_DIFF_ROWS: usize = 40;
/// First row of a body, and the columns every body row's prefix takes.
const BODY_FIRST: &str = "  \u{23bf}  ";
const BODY_REST: &str = "     ";
const BODY_COLUMNS: usize = 5;
/// The gateway cut the result.
const CUT_NOTE: &str = " (cut by Hermes at 32,768 characters)";
/// Result keys that hold a tool's output text.
const OUTPUT_KEYS: [&str; 6] = [
    "output",
    "stdout",
    "output_preview",
    "content",
    "result",
    "message",
];

pub(crate) fn spaces(count: usize) -> String {
    " ".repeat(count)
}

pub(crate) fn text_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Keep whole clusters that fit in `max - 1` columns and end with `…`.
pub(crate) fn cut_force(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let keep = max - 1;
    let mut used = 0;
    let mut end = 0;
    for (start, cluster) in text.grapheme_indices(true) {
        let columns = text_width(cluster);
        if used + columns > keep {
            break;
        }
        used += columns;
        end = start + cluster.len();
    }
    let mut cut = String::with_capacity(end + 3);
    cut.push_str(&text[..end]);
    cut.push('\u{2026}');
    cut
}

/// True when the text is at most `max` columns wide. Stops reading as soon as it is not.
fn fits(text: &str, max: usize) -> bool {
    if text.len() <= max {
        return true;
    }
    let mut used = 0;
    for cluster in text.graphemes(true) {
        used += text_width(cluster);
        if used > max {
            return false;
        }
    }
    true
}

/// The text as it is when it fits, else cut with `…` so it is at most `max` columns.
pub(crate) fn cut_to(text: &str, max: usize) -> String {
    if fits(text, max) {
        text.to_owned()
    } else {
        cut_force(text, max)
    }
}

/// The first clusters of the text that fit in `max` columns, with no mark of the cut.
fn clip(text: &str, max: usize) -> String {
    if fits(text, max) {
        return text.to_owned();
    }
    let mut used = 0;
    let mut end = 0;
    for (start, cluster) in text.grapheme_indices(true) {
        used += text_width(cluster);
        if used > max {
            break;
        }
        end = start + cluster.len();
    }
    text[..end].to_owned()
}

/// Hard wrap at cluster boundaries into rows of at most `avail` columns, one row at a time.
///
/// A cluster wider than `avail` is drawn as `…`. Every row takes at least one cluster, so the
/// walk ends. Empty text is one empty row.
pub(crate) struct WrapRows<'a> {
    rest: &'a str,
    avail: usize,
    started: bool,
}

impl<'a> WrapRows<'a> {
    pub(crate) fn new(text: &'a str, avail: usize) -> Self {
        Self {
            rest: text,
            avail: avail.max(1),
            started: false,
        }
    }

    /// True when text is left for another row.
    pub(crate) fn has_more(&self) -> bool {
        !self.rest.is_empty()
    }
}

impl<'a> Iterator for WrapRows<'a> {
    type Item = Cow<'a, str>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            if self.started {
                return None;
            }
            self.started = true;
            return Some(Cow::Borrowed(""));
        }
        self.started = true;
        let mut used = 0;
        let mut end = 0;
        for (start, cluster) in self.rest.grapheme_indices(true) {
            let columns = text_width(cluster);
            if used + columns > self.avail {
                if end == 0 {
                    self.rest = &self.rest[start + cluster.len()..];
                    return Some(Cow::Borrowed("\u{2026}"));
                }
                break;
            }
            used += columns;
            end = start + cluster.len();
        }
        let (row, rest) = self.rest.split_at(end);
        self.rest = rest;
        Some(Cow::Borrowed(row))
    }
}

/// Remove foreground and background colour from every span. Bold and dim stay.
pub(crate) fn plain_line(line: &mut Line<'static>) {
    plain_style(&mut line.style);
    for span in &mut line.spans {
        plain_style(&mut span.style);
    }
}

fn plain_style(style: &mut Style) {
    style.fg = None;
    style.bg = None;
    style.underline_color = None;
}

/// Drop clusters from the end of the row until it is at most `width` columns.
fn clamp_line(line: &mut Line<'static>, width: usize) {
    loop {
        let joined: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        if text_width(&joined) <= width {
            return;
        }
        let Some(last) = line.spans.last_mut() else {
            return;
        };
        let mut text = std::mem::take(&mut last.content).into_owned();
        let cut = text
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(start, _)| start);
        text.truncate(cut);
        if text.is_empty() {
            line.spans.pop();
        } else {
            last.content = Cow::Owned(text);
        }
    }
}

/// A one-style row that is cut to the width and never wrapped.
fn fixed(text: &str, style: Style, width: usize) -> Line<'static> {
    Line::from(Span::styled(cut_to(text, width), style))
}

/// Everything one render shares.
struct Ctx<'a> {
    theme: &'a ToolTheme,
    width: usize,
    hint: Option<String>,
}

impl Ctx<'_> {
    fn hint_suffix(&self) -> String {
        self.hint
            .as_deref()
            .map(|hint| format!(" ({})", sanitize(hint)))
            .unwrap_or_default()
    }
}

/// The text of a tool, kept to one line and visible.
fn clean(text: &str) -> String {
    sanitize(text).into_owned()
}

/// Pieces of the first row, and how each is cut when the row is too wide.
struct Header<'a> {
    theme: &'a ToolTheme,
    glyph: Style,
    title: String,
    subject: Option<String>,
    /// Edit cells show `title subject` with no parentheses.
    edit: bool,
    mark: Option<(char, Style)>,
    label: Option<(String, Style)>,
    stat: Option<String>,
    /// Duration and progress, each shown after ` · `.
    tail: Vec<String>,
    separator: usize,
    dim: bool,
}

impl Header<'_> {
    fn has_segment(&self) -> bool {
        self.mark.is_some() || self.label.is_some() || self.stat.is_some() || !self.tail.is_empty()
    }

    fn spans(&self) -> Vec<Span<'static>> {
        let theme = self.theme;
        let mut spans = vec![Span::styled(GLYPH, self.glyph)];
        let has_title = !self.title.is_empty();
        if has_title {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(self.title.clone(), theme.title));
            if let Some(subject) = &self.subject {
                if self.edit {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(subject.clone(), theme.subject));
                } else {
                    spans.push(Span::styled("(", theme.subject));
                    spans.push(Span::styled(subject.clone(), theme.subject));
                    spans.push(Span::styled(")", theme.subject));
                }
            }
        }
        if self.has_segment() {
            spans.push(Span::raw(if has_title {
                spaces(self.separator)
            } else {
                " ".to_owned()
            }));
            let mut first = true;
            let mut item = |spans: &mut Vec<Span<'static>>, text: String, style: Style| {
                if !first {
                    spans.push(Span::raw(" "));
                }
                first = false;
                spans.push(Span::styled(text, style));
            };
            if let Some((mark, style)) = self.mark {
                item(&mut spans, mark.to_string(), style);
            }
            if let Some((label, style)) = &self.label {
                item(&mut spans, label.clone(), *style);
            }
            if let Some(stat) = &self.stat {
                item(&mut spans, stat.clone(), theme.meta);
            }
            for piece in &self.tail {
                spans.push(Span::styled(format!(" \u{b7} {piece}"), theme.meta));
            }
        }
        if self.dim {
            for span in &mut spans {
                span.style = span.style.add_modifier(Modifier::DIM);
            }
        }
        spans
    }

    fn width(&self) -> usize {
        self.spans()
            .iter()
            .map(|span| text_width(&span.content))
            .sum()
    }

    /// Cut pieces in the order of the layout rules until the row fits `width`.
    fn fit(mut self, width: usize) -> Vec<Span<'static>> {
        if width < 3 {
            return vec![Span::styled(GLYPH, self.glyph)];
        }
        if self.width() > width {
            self.separator = 1;
        }
        if self.width() > width
            && let Some(columns) = self.subject.as_deref().map(text_width)
            && columns > 8
        {
            let target = columns.saturating_sub(self.width() - width).max(8);
            self.subject = self.subject.take().map(|text| cut_to(&text, target));
        }
        if self.width() > width
            && let Some((label, _)) = self.label.as_mut()
            && text_width(label) > 12
        {
            *label = cut_to(label, 12);
        }
        if self.width() > width {
            self.tail.clear();
        }
        if self.width() > width {
            self.subject = None;
        }
        if self.width() > width {
            self.label = None;
            self.stat = None;
        }
        if self.width() > width {
            let target = text_width(&self.title).saturating_sub(self.width() - width);
            self.title = if target == 0 {
                String::new()
            } else {
                cut_force(&self.title, target)
            };
        }
        self.spans()
    }
}

fn is_shell(cell: &ToolCell) -> bool {
    matches!(cell.name.as_str(), "terminal" | "execute_code")
}

fn stat_text(view: &ToolView) -> Option<String> {
    let stats = view.diff.as_ref()?.stats();
    Some(if stats.files > 1 {
        format!("{} files +{} -{}", stats.files, stats.added, stats.removed)
    } else {
        format!("+{} -{}", stats.added, stats.removed)
    })
}

/// The first row's pieces, with the strings clipped to `clip_to` columns when given.
fn header<'a>(
    view: &ToolView,
    theme: &'a ToolTheme,
    elapsed: Option<Duration>,
    clip_to: Option<usize>,
) -> Header<'a> {
    let limit = |text: &str| match clip_to {
        Some(max) => clip(&clean(text), max),
        None => clean(text),
    };
    let stat = stat_text(view);
    let duration = view.duration.as_deref().map(limit);
    let (glyph, mark, label, stat, tail, dim) = match &view.outcome {
        OutcomeView::Running => {
            let seconds = elapsed.map_or(0, |elapsed| elapsed.as_secs());
            let label = if seconds >= 1 {
                format!("running {seconds}s")
            } else {
                "running".to_owned()
            };
            let tail = view.progress.as_deref().map(limit).into_iter().collect();
            (
                theme.glyph_running,
                Some(('\u{2026}', theme.glyph_running)),
                Some((label, theme.meta)),
                None,
                tail,
                false,
            )
        }
        OutcomeView::Succeeded { headline } => {
            let label = match (&stat, headline) {
                (None, Some(headline)) => Some((limit(headline), theme.meta)),
                _ => None,
            };
            (
                theme.glyph_ok,
                Some(('\u{2713}', theme.glyph_ok)),
                label,
                stat,
                duration.into_iter().collect(),
                false,
            )
        }
        OutcomeView::Failed { headline, .. } => (
            theme.glyph_failed,
            Some(('\u{2717}', theme.glyph_failed)),
            Some((limit(headline), theme.error)),
            None,
            duration.into_iter().collect(),
            false,
        ),
        OutcomeView::Unfinished => (
            theme.glyph_muted,
            Some(('\u{25cc}', theme.glyph_muted)),
            Some(("not finished".to_owned(), theme.meta)),
            stat,
            Vec::new(),
            false,
        ),
        OutcomeView::Unknown => (theme.glyph_muted, None, None, stat, Vec::new(), true),
    };
    Header {
        theme,
        glyph,
        title: limit(&view.title),
        subject: view.subject.as_deref().map(limit),
        edit: view.diff.is_some(),
        mark,
        label,
        stat,
        tail,
        separator: 2,
        dim,
    }
}

/// Wrap styled spans at cluster boundaries. Continuation rows start with `indent` spaces,
/// unless the width leaves no room for text after them.
fn wrap_spans(spans: &[Span<'static>], width: usize, indent: usize) -> Vec<Line<'static>> {
    let lead = if width > indent { indent } else { 0 };
    let mut rows: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut used = 0;
    let mut text_columns = 0;
    for span in spans {
        for cluster in span.content.graphemes(true) {
            let mut columns = text_width(cluster);
            let mut shown = cluster;
            if text_columns > 0 && used + columns > width {
                rows.push(if lead > 0 {
                    vec![Span::raw(spaces(lead))]
                } else {
                    Vec::new()
                });
                used = lead;
                text_columns = 0;
            }
            if used + columns > width {
                shown = "\u{2026}";
                columns = 1;
            }
            used += columns;
            text_columns += columns;
            let row = rows.last_mut().expect("rows always holds a row");
            match row.last_mut() {
                Some(last) if last.style == span.style && text_columns > columns => {
                    last.content.to_mut().push_str(shown);
                }
                _ => row.push(Span::styled(shown.to_owned(), span.style)),
            }
        }
    }
    rows.into_iter().map(Line::from).collect()
}

/// The first row when it must show the whole subject: it wraps instead of being cut.
fn full_header(view: &ToolView, ctx: &Ctx<'_>, elapsed: Option<Duration>) -> Vec<Line<'static>> {
    let spans = header(view, ctx.theme, elapsed, None).spans();
    wrap_spans(&spans, ctx.width, 2)
}

/// The failure text of a failed cell, without the exit code the headline starts with.
fn failure_text(view: &ToolView) -> Option<String> {
    let OutcomeView::Failed { headline, detail } = &view.outcome else {
        return None;
    };
    if let Some(detail) = detail {
        return Some(detail.clone());
    }
    let exit = view
        .output
        .as_ref()
        .and_then(|output| output.exit_code)
        .filter(|code| *code != 0);
    let text = match exit {
        Some(code) => {
            let lead = format!("exit {code}");
            match headline.strip_prefix(&lead) {
                Some("") => return None,
                Some(rest) => rest.strip_prefix(": ").unwrap_or(headline),
                None => headline,
            }
        }
        None if headline == "failed" => return None,
        None => headline,
    };
    Some(text.to_owned())
}

/// A body of at most `max_lines` logical lines and `max_rows` screen rows, then a more-lines
/// row. A line that only partly fits ends with `…`. Wrapping stops at the row limit, so a long
/// line is never wrapped whole.
struct Preview<'a> {
    text: &'a str,
    total_lines: usize,
    style: Style,
    max_lines: usize,
    max_rows: usize,
    cut_by_hermes: bool,
}

fn push_preview(out: &mut Vec<Line<'static>>, preview: &Preview<'_>, ctx: &Ctx<'_>) {
    let prefixed = ctx.width > BODY_COLUMNS;
    let avail = if prefixed {
        ctx.width - BODY_COLUMNS
    } else {
        ctx.width
    };
    let mut rows: Vec<String> = Vec::new();
    let mut shown_lines = 0usize;
    let mut rows_left = preview.max_rows;
    for raw in preview.text.split('\n').take(preview.max_lines) {
        if rows_left == 0 {
            break;
        }
        let line = sanitize(raw);
        let mut wrap = WrapRows::new(&line, avail);
        let mut taken = 0;
        while taken < rows_left {
            let Some(row) = wrap.next() else { break };
            rows.push(row.into_owned());
            taken += 1;
        }
        shown_lines += 1;
        if wrap.has_more() {
            if let Some(last) = rows.last_mut() {
                *last = cut_force(last, avail);
            }
            rows_left = 0;
        } else {
            rows_left -= taken;
        }
    }
    let hidden = preview.total_lines.saturating_sub(shown_lines);
    let count = rows.len();
    for (index, mut row) in rows.into_iter().enumerate() {
        if hidden == 0 && preview.cut_by_hermes && index + 1 == count {
            row.push_str(CUT_NOTE);
            row = cut_to(&row, avail);
        }
        out.push(body_row(index == 0, prefixed, row, preview.style, ctx));
    }
    if hidden > 0 {
        let mut text = format!("{BODY_REST}\u{2026} +{hidden} lines{}", ctx.hint_suffix());
        if preview.cut_by_hermes {
            text.push_str(CUT_NOTE);
        }
        out.push(fixed(&text, ctx.theme.meta, ctx.width));
    }
}

fn body_row(
    first: bool,
    prefixed: bool,
    text: String,
    style: Style,
    ctx: &Ctx<'_>,
) -> Line<'static> {
    if !prefixed {
        return Line::from(Span::styled(text, style));
    }
    let lead = if first {
        Span::styled(BODY_FIRST, ctx.theme.meta)
    } else {
        Span::raw(BODY_REST)
    };
    Line::from(vec![lead, Span::styled(text, style)])
}

fn push_no_output(out: &mut Vec<Line<'static>>, cut: bool, ctx: &Ctx<'_>) {
    let mut text = format!("{BODY_FIRST}(no output)");
    if cut {
        text.push_str(CUT_NOTE);
    }
    out.push(fixed(&text, ctx.theme.meta, ctx.width));
}

fn push_diff(
    out: &mut Vec<Line<'static>>,
    view: &ToolView,
    options: &ToolRenderOptions,
    ctx: &Ctx<'_>,
    compact: bool,
) {
    let Some(model) = &view.diff else { return };
    let (rows, hidden) = diff_lines(
        model,
        &DiffRenderOptions {
            width: options.width,
            indent: DIFF_INDENT,
            max_rows: compact.then_some(COMPACT_DIFF_ROWS),
            theme: options.theme.clone(),
            color: options.color,
        },
    );
    out.extend(rows);
    if hidden > 0 {
        let text = format!(
            "{}\u{2026} +{hidden} more diff lines{}",
            spaces(usize::from(DIFF_INDENT)),
            ctx.hint_suffix()
        );
        out.push(fixed(&text, ctx.theme.meta, ctx.width));
    }
    for note in &model.notes {
        let text = format!("{}{}", spaces(usize::from(DIFF_INDENT)), clean(note));
        out.push(fixed(&text, ctx.theme.diff_note, ctx.width));
    }
}

fn compact_body(
    cell: &ToolCell,
    view: &ToolView,
    options: &ToolRenderOptions,
    ctx: &Ctx<'_>,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    if view.quiet {
        return out;
    }
    let theme = ctx.theme;
    let shell = is_shell(cell);
    match &view.outcome {
        OutcomeView::Running => {}
        OutcomeView::Succeeded { .. } | OutcomeView::Unfinished | OutcomeView::Unknown => {
            if view.diff.is_some() {
                push_diff(&mut out, view, options, ctx, true);
            } else if shell
                && matches!(view.outcome, OutcomeView::Succeeded { .. })
                && let Some(output) = &view.output
            {
                if output.text.is_empty() {
                    push_no_output(&mut out, output.cut_by_hermes, ctx);
                } else {
                    let preview = Preview {
                        text: &output.text,
                        total_lines: output.lines,
                        style: theme.output,
                        max_lines: 3,
                        max_rows: 6,
                        cut_by_hermes: output.cut_by_hermes,
                    };
                    push_preview(&mut out, &preview, ctx);
                }
            }
        }
        OutcomeView::Failed { detail, .. } => {
            let output = view
                .output
                .as_ref()
                .filter(|output| !output.text.is_empty());
            if shell {
                if let Some(output) = output {
                    let preview = Preview {
                        text: &output.text,
                        total_lines: output.lines,
                        style: theme.output,
                        max_lines: 6,
                        max_rows: 10,
                        cut_by_hermes: output.cut_by_hermes,
                    };
                    push_preview(&mut out, &preview, ctx);
                } else if let Some(text) = failure_text(view) {
                    push_failure(&mut out, &text, ctx);
                }
            } else if let Some(text) = detail {
                push_failure(&mut out, text, ctx);
            }
        }
    }
    out
}

fn push_failure(out: &mut Vec<Line<'static>>, text: &str, ctx: &Ctx<'_>) {
    let preview = Preview {
        text,
        total_lines: text.split('\n').count(),
        style: ctx.theme.error,
        max_lines: 6,
        max_rows: 10,
        cut_by_hermes: false,
    };
    push_preview(out, &preview, ctx);
}

/// A dim section title and its text, every row prefixed with four spaces.
fn push_section(
    out: &mut Vec<Line<'static>>,
    title: &str,
    text: &str,
    style: Style,
    ctx: &Ctx<'_>,
) {
    out.push(fixed(&format!("  {title}"), ctx.theme.meta, ctx.width));
    let lead = 4;
    let (prefix, avail) = if ctx.width > lead {
        (spaces(lead), ctx.width - lead)
    } else {
        (String::new(), ctx.width)
    };
    for raw in text.split('\n') {
        let line = sanitize(raw);
        for row in WrapRows::new(&line, avail) {
            let mut spans = Vec::with_capacity(2);
            if !prefix.is_empty() {
                spans.push(Span::raw(prefix.clone()));
            }
            spans.push(Span::styled(row.into_owned(), style));
            out.push(Line::from(spans));
        }
    }
}

fn result_object(cell: &ToolCell) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(cell.result.as_deref()?) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// The pretty-printed result object when it holds no output text. The diff the cell drew from
/// its `diff` key is left out so it is not printed twice.
fn object_text(cell: &ToolCell, view: &ToolView) -> Option<String> {
    let mut map = result_object(cell)?;
    if OUTPUT_KEYS
        .iter()
        .any(|key| map.get(*key).is_some_and(Value::is_string))
    {
        return None;
    }
    if cell.diff.is_none() && view.diff.is_some() {
        map.remove("diff");
    }
    if map.is_empty() {
        return None;
    }
    serde_json::to_string_pretty(&Value::Object(map)).ok()
}

fn full_lines(
    cell: &ToolCell,
    view: &ToolView,
    options: &ToolRenderOptions,
    ctx: &Ctx<'_>,
) -> Vec<Line<'static>> {
    let theme = ctx.theme;
    let mut out = full_header(view, ctx, options.elapsed);
    if let Some(args) = &cell.args
        && !args.as_object().is_some_and(Map::is_empty)
        && let Ok(text) = serde_json::to_string_pretty(args)
    {
        push_section(&mut out, "Arguments", &text, theme.output, ctx);
    }
    let output = view
        .output
        .as_ref()
        .filter(|output| !output.text.is_empty());
    if let Some(output) = output {
        push_section(&mut out, "Output", &output.text, theme.output, ctx);
    } else if let Some(text) = failure_text(view) {
        push_section(&mut out, "Error", &text, theme.error, ctx);
    } else if let Some(text) = object_text(cell, view) {
        push_section(&mut out, "Output", &text, theme.output, ctx);
    }
    if let Some(note) = view
        .output
        .as_ref()
        .and_then(|output| output.tool_note.as_deref())
    {
        let text = format!("{}{}", spaces(usize::from(DIFF_INDENT)), clean(note));
        out.push(fixed(&text, theme.meta, ctx.width));
    }
    push_diff(&mut out, view, options, ctx, false);
    if view
        .output
        .as_ref()
        .is_some_and(|output| output.cut_by_hermes)
    {
        let text = format!(
            "{}{}",
            spaces(usize::from(DIFF_INDENT)),
            CUT_NOTE.trim_start()
        );
        out.push(fixed(&text, theme.meta, ctx.width));
    }
    out
}

/// Rows for one tool cell. Issue 221.
#[must_use]
pub fn tool_lines(cell: &ToolCell, options: &ToolRenderOptions) -> Vec<Line<'static>> {
    let width = usize::from(options.width.max(1));
    let view = tool_view(cell);
    let ctx = Ctx {
        theme: &options.theme,
        width,
        hint: options.expand_hint.clone(),
    };
    let mut lines = match options.detail {
        Detail::Compact => {
            let spans = header(&view, &options.theme, options.elapsed, Some(width)).fit(width);
            let mut lines = vec![Line::from(spans)];
            lines.extend(compact_body(cell, &view, options, &ctx));
            lines
        }
        Detail::Full => full_lines(cell, &view, options, &ctx),
    };
    for line in &mut lines {
        clamp_line(line, width);
        if !options.color {
            plain_line(line);
        }
    }
    lines
}

/// The application's cell renderer, with tool cells drawn by this unit.
pub struct ToolCellLines {
    inner: Box<dyn CellLines>,
    options: ToolRenderOptions,
    elapsed: HashMap<CellId, Duration>,
}

impl ToolCellLines {
    /// Wrap the renderer the application already uses for other cells.
    #[must_use]
    pub fn new(inner: Box<dyn CellLines>, options: ToolRenderOptions) -> Self {
        Self {
            inner,
            options,
            elapsed: HashMap::new(),
        }
    }

    /// Copy elapsed times of running cells.
    pub fn set_clock(&mut self, clock: &ToolClock, now: Instant) {
        self.elapsed = clock
            .first_seen
            .keys()
            .filter_map(|id| clock.elapsed(*id, now).map(|elapsed| (*id, elapsed)))
            .collect();
    }

    /// Replace the more-lines phrase.
    pub fn set_expand_hint(&mut self, hint: Option<String>) {
        self.options.expand_hint = hint;
    }
}

impl CellLines for ToolCellLines {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        let Cell::Tool(tool) = cell else {
            return self.inner.lines(cell, width);
        };
        let options = ToolRenderOptions {
            width,
            theme: self.options.theme.clone(),
            color: self.options.color,
            detail: self.options.detail,
            expand_hint: self.options.expand_hint.clone(),
            elapsed: self.elapsed.get(&tool.id).copied(),
        };
        tool_lines(tool, &options)
    }
}

/// When each running tool cell was first seen.
pub struct ToolClock {
    first_seen: HashMap<CellId, Instant>,
}

impl ToolClock {
    /// An empty clock.
    #[must_use]
    pub fn new() -> Self {
        Self {
            first_seen: HashMap::new(),
        }
    }

    /// Remember running cells and forget the ones that stopped.
    pub fn observe(&mut self, cells: &[Cell], now: Instant) {
        let running: Vec<CellId> = cells
            .iter()
            .filter_map(|cell| match cell {
                Cell::Tool(tool) if tool.status == ToolStatus::Running => Some(tool.id),
                _ => None,
            })
            .collect();
        self.first_seen.retain(|id, _| running.contains(id));
        for id in running {
            self.first_seen.entry(id).or_insert(now);
        }
    }

    /// How long one running cell has been on screen.
    #[must_use]
    pub fn elapsed(&self, id: CellId, now: Instant) -> Option<Duration> {
        self.first_seen
            .get(&id)
            .map(|first| now.saturating_duration_since(*first))
    }

    /// The next whole second of any running cell, or nothing when none run.
    #[must_use]
    pub fn next_tick(&self, now: Instant) -> Option<Instant> {
        self.first_seen
            .values()
            .map(|first| {
                let seconds = now.saturating_duration_since(*first).as_secs();
                *first + Duration::from_secs(seconds + 1)
            })
            .min()
    }
}

impl Default for ToolClock {
    fn default() -> Self {
        Self::new()
    }
}

/// The "preparing" row while a tool call is still being written.
pub struct DraftingLine<'a> {
    /// The tool name, when one is being written.
    pub drafted: Option<&'a str>,
    /// Colours and weight.
    pub theme: &'a ToolTheme,
}

impl LiveSection for DraftingLine<'_> {
    fn height(&self, _width: u16) -> u16 {
        u16::from(self.drafted.is_some())
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let Some(name) = self.drafted else { return };
        if area.width == 0 || area.height == 0 {
            return;
        }
        let cell = ToolCell {
            id: CellId::new(0),
            tool_id: String::new(),
            name: name.to_owned(),
            context: None,
            args: None,
            labels: Vec::new(),
            status: ToolStatus::Running,
            progress: None,
            summary: None,
            result: None,
            diff: None,
            duration_s: None,
        };
        let title = clean(&tool_view(&cell).title);
        let text = cut_to(
            &format!("{GLYPH} Preparing {title}\u{2026}"),
            usize::from(area.width),
        );
        let line = Line::styled(text, self.theme.meta);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}
