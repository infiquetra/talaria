//! Context, usage and compaction blocks (#234).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use talaria_core::context::breakdown::{Breakdown, CategoryRow, GridCell, category_index};
use talaria_core::context::compact::CompactResult;
use talaria_core::context::text::ReadError;
use talaria_core::context::tracker::{CompactingLine, ContextNotice};
use talaria_core::context::usage::{CostLine, Plan, UsageReport, UsageSource};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::history::sanitize;
use crate::live::LiveSection;

/// Below this width the grid is left out: it is thirty-nine columns wide.
const GRID_WIDTH: u16 = 39;

/// Styles for the context blocks. `categories` follows Hermes's display order.
#[derive(Clone, Debug)]
pub struct ContextStyle {
    pub header: Style,
    pub dim: Style,
    pub error: Style,
    pub tick: Style,
    pub categories: [Style; 9],
    pub free: Style,
}

impl ContextStyle {
    /// The full-colour style.
    pub fn standard() -> Self {
        ContextStyle {
            header: Style::default().add_modifier(Modifier::BOLD),
            dim: Style::default().add_modifier(Modifier::DIM),
            error: Style::default().fg(Color::Red),
            tick: Style::default().fg(Color::Green),
            categories: [
                Style::default().fg(Color::Blue),
                Style::default().fg(Color::Magenta),
                Style::default().fg(Color::Yellow),
                Style::default().fg(Color::Cyan),
                Style::default().fg(Color::LightMagenta),
                Style::default().fg(Color::LightBlue),
                Style::default().fg(Color::Green),
                Style::default().fg(Color::LightCyan),
                Style::default().fg(Color::White),
            ],
            free: Style::default().add_modifier(Modifier::DIM),
        }
    }

    /// No colours; bold and dim kept.
    pub fn plain() -> Self {
        ContextStyle {
            header: Style::default().add_modifier(Modifier::BOLD),
            dim: Style::default().add_modifier(Modifier::DIM),
            error: Style::default(),
            tick: Style::default(),
            categories: [
                Style::default(),
                Style::default(),
                Style::default(),
                Style::default(),
                Style::default(),
                Style::default(),
                Style::default(),
                Style::default(),
                Style::default(),
            ],
            free: Style::default().add_modifier(Modifier::DIM),
        }
    }
}

/// The columns a text takes.
fn width_of(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// A number with a comma every three digits.
fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(character);
    }
    out
}

/// A share of a percent with one decimal.
fn percent(tenths: u32) -> String {
    format!("{}.{}%", tenths / 10, tenths % 10)
}

/// The longest prefix of whole graphemes that fits, ending `…`.
fn cut_text(text: &str, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let limit = usize::from(width);
    if width_of(text) <= limit {
        return text.to_owned();
    }
    let mut kept = String::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true) {
        let columns = width_of(grapheme);
        if used + columns > limit - 1 {
            break;
        }
        kept.push_str(grapheme);
        used += columns;
    }
    kept.push('…');
    kept
}

/// Cut a styled row to the width, keeping the styles of the parts that fit.
fn cut_row(spans: Vec<Span<'static>>, width: u16) -> Vec<Line<'static>> {
    let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
    if width_of(&text) <= usize::from(width) {
        return vec![Line::from(spans)];
    }
    let cut = cut_text(&text, width);
    let keep = cut.graphemes(true).count();
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut seen = 0usize;
    for span in spans {
        if seen >= keep {
            break;
        }
        let len = span.content.graphemes(true).count();
        let take = (keep - seen).min(len);
        if take > 0 {
            let piece: String = span.content.graphemes(true).take(take).collect();
            out.push(Span::styled(piece, span.style));
        }
        seen += len;
    }
    vec![Line::from(out)]
}

/// One prose row: made literal, then wrapped, never cut.
fn prose(text: String, style: Style, width: u16) -> Vec<Line<'static>> {
    let text = sanitize(&text).into_owned();
    wrap_prose(&text, width)
        .into_iter()
        .map(|row| Line::from(Span::styled(row, style)))
        .collect()
}

/// Wrap prose at word boundaries: a word that does not fit moves whole to the next
/// row, and a word wider than the row is split by grapheme. Nothing is lost.
fn wrap_prose(text: &str, width: u16) -> Vec<String> {
    let limit = usize::from(width);
    if limit == 0 {
        return Vec::new();
    }
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    let mut row_width = 0usize;
    let mut break_at: Option<usize> = None;
    for grapheme in text.graphemes(true) {
        let columns = width_of(grapheme);
        if row_width + columns > limit && row_width > 0 {
            let head = match break_at {
                Some(at) => row[..at].to_owned(),
                None => std::mem::take(&mut row),
            };
            let tail = match break_at {
                Some(at) => row[at + 1..].to_owned(),
                None => String::new(),
            };
            rows.push(head);
            row = tail;
            row_width = width_of(&row);
            break_at = None;
        }
        if grapheme == " " && row_width > 0 && !row.trim().is_empty() {
            break_at = Some(row.len());
        }
        row.push_str(grapheme);
        row_width += columns;
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

/// The `/context` block for a breakdown.
pub fn context_block(
    typed: &str,
    b: &Breakdown,
    width: u16,
    s: &ContextStyle,
) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut rows: Vec<Line<'static>> = Vec::new();
    rows.extend(prose(typed.to_owned(), s.dim, width));

    let mut header = String::from("Context");
    if !b.model.is_empty() {
        header.push_str(&format!(" · {}", b.model));
    }
    if b.max > 0 {
        let used = if b.estimated {
            format!("~{}", grouped(b.used))
        } else {
            grouped(b.used)
        };
        let share = if b.estimated {
            format!("(~{:.0}%)", b.percent)
        } else {
            format!("({}%)", b.percent)
        };
        header.push_str(&format!(" · {used} / {} tokens {share}", grouped(b.max)));
    } else {
        header.push_str(&format!(" · {} tokens", grouped(b.used)));
    }
    rows.extend(prose(header, s.header, width));

    // The table is as wide as the grid: a terminal that cannot show the grid cannot
    // show the table either.
    if width >= GRID_WIDTH {
        if b.max > 0 {
            for line in grid_rows(b, s) {
                rows.extend(cut_row(line, width));
            }
        }
        rows.extend(prose(
            "Estimated usage by category".to_owned(),
            s.header,
            width,
        ));
        for row in b.rows() {
            rows.extend(cut_row(category_spans(&row, b, s), width));
        }
    }
    rows.extend(prose(source_line(b), s.dim, width));
    if !b.files.is_empty() {
        rows.extend(prose("Context files".to_owned(), s.header, width));
        for file in &b.files {
            rows.extend(cut_row(file_spans(file, s), width));
        }
    }
    rows
}

/// The five grid rows, twenty cells each.
fn grid_rows(b: &Breakdown, s: &ContextStyle) -> Vec<Vec<Span<'static>>> {
    let grid = b.grid();
    let mut rows: Vec<Vec<Span<'static>>> = Vec::new();
    for chunk in grid.chunks(20) {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for (index, cell) in chunk.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(" ", Style::default()));
            }
            let (glyph, style) = match cell {
                GridCell::Category(category) => {
                    let id = b
                        .categories
                        .get(*category)
                        .map(|category| category.id.as_str())
                        .unwrap_or_default();
                    (
                        talaria_core::context::breakdown::glyph_for(id),
                        s.categories[category_index(id)],
                    )
                }
                GridCell::Free => (talaria_core::context::breakdown::FREE_GLYPH, s.free),
            };
            spans.push(Span::styled(glyph.to_string(), style));
        }
        rows.push(spans);
    }
    rows
}

/// One row of the category table.
fn category_spans(row: &CategoryRow, b: &Breakdown, s: &ContextStyle) -> Vec<Span<'static>> {
    let table = b.rows();
    let label_width = table
        .iter()
        .map(|row| row.label.chars().count())
        .max()
        .unwrap_or(0);
    let token_width = table
        .iter()
        .map(|row| format!("~{}", grouped(row.tokens)).chars().count())
        .max()
        .unwrap_or(0);
    let percent_width = table
        .iter()
        .map(|row| percent(row.tenths_of_percent).chars().count())
        .max()
        .unwrap_or(0);
    let style = match row.category {
        Some(index) => {
            let id = b
                .categories
                .get(index)
                .map(|category| category.id.as_str())
                .unwrap_or_default();
            s.categories[category_index(id)]
        }
        None => s.free,
    };
    let label = format!("{:<width$}", sanitize(&row.label), width = label_width);
    let tokens = format!(
        "{:>width$}",
        format!("~{}", grouped(row.tokens)),
        width = token_width
    );
    let share = format!(
        "{:>width$}",
        percent(row.tenths_of_percent),
        width = percent_width
    );
    vec![
        Span::styled(row.glyph.to_string(), style),
        Span::styled(format!(" {label}  {tokens} tokens  {share}"), style),
    ]
}

/// One context file row.
fn file_spans(
    file: &talaria_core::context::breakdown::ContextFile,
    s: &ContextStyle,
) -> Vec<Span<'static>> {
    let (glyph, note) = talaria_core::context::text::file_status(&file.status);
    let label = sanitize(&file.label).into_owned();
    let mut spans = vec![
        Span::styled("  ", Style::default()),
        Span::styled(glyph.to_owned(), Style::default()),
        Span::styled(format!(" {label}"), Style::default()),
        Span::styled(
            format!("  ~{} tokens", grouped(file.est_tokens)),
            Style::default(),
        ),
    ];
    if let Some(note) = note {
        spans.push(Span::styled(format!(" · {note}"), s.dim));
    }
    spans
}

/// The source line of a breakdown.
fn source_line(b: &Breakdown) -> String {
    use talaria_core::context::breakdown::ContextSource;
    let source = match &b.source {
        ContextSource::ProviderUsage => "provider usage",
        ContextSource::LocalEstimate => "local estimate",
        ContextSource::ProviderUsagePlusEstimate => "provider usage + estimated new messages",
        ContextSource::Other(raw) => raw.as_str(),
    };
    let estimated = matches!(
        b.source,
        ContextSource::ProviderUsage | ContextSource::ProviderUsagePlusEstimate
    );
    if estimated {
        format!("Source: {source}; category counts are local estimates.")
    } else {
        format!("Source: {source}")
    }
}

/// The `/context` block before the agent exists, with an optional last reading.
pub fn prebuild_block(
    typed: &str,
    last: Option<&talaria_core::context::usage::ContextFigure>,
    width: u16,
    s: &ContextStyle,
) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut rows: Vec<Line<'static>> = Vec::new();
    rows.extend(prose(typed.to_owned(), s.dim, width));
    rows.extend(prose(
        "Context · not measured yet".to_owned(),
        s.header,
        width,
    ));
    rows.extend(prose(
        "Hermes builds this session's agent on the next prompt; until then there is nothing to \
         break down."
            .to_owned(),
        s.dim,
        width,
    ));
    if let Some(figure) = last {
        rows.extend(prose(
            format!(
                "Last reading in this view: {} / {} tokens ({}%)",
                grouped(figure.used),
                grouped(figure.max),
                figure.percent
            ),
            s.dim,
            width,
        ));
    }
    rows
}

/// A row of the usage table: two spaces, the label padded to eleven columns, two spaces.
fn usage_row(label: &str, value: String, width: u16) -> Vec<Line<'static>> {
    prose(format!("  {label:<11}  {value}"), Style::default(), width)
}

/// The `/usage` block for a report.
pub fn usage_block(
    typed: &str,
    r: &UsageReport,
    width: u16,
    s: &ContextStyle,
) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut rows: Vec<Line<'static>> = Vec::new();
    rows.extend(prose(typed.to_owned(), s.dim, width));
    rows.extend(prose("Usage · this session".to_owned(), s.header, width));

    if let Some(error) = &r.stats_error {
        if r.source == UsageSource::LastReading {
            rows.extend(prose(
                format!(
                    "  (Hermes did not answer session.stats: {}; these are the last figures this \
                     view received.)",
                    error.short()
                ),
                s.dim,
                width,
            ));
        } else {
            rows.extend(prose(
                error.row("/usage", "session.stats", ""),
                s.error,
                width,
            ));
        }
    } else if r.source == UsageSource::LastReading {
        rows.extend(prose(
            "  (Hermes has not rebuilt this session's agent since its last compaction or restart; \
             these are the last figures this view received.)"
                .to_owned(),
            s.dim,
            width,
        ));
    } else if r.source == UsageSource::Nothing {
        rows.extend(prose(
            "  No API calls yet in this session.".to_owned(),
            s.dim,
            width,
        ));
    }

    if r.source != UsageSource::Nothing {
        if let Some(row) = model_row(r) {
            rows.extend(usage_row("Model", row, width));
        }
        let tokens = token_parts(r);
        if !tokens.is_empty() {
            rows.extend(usage_row("Tokens", tokens.join(" · "), width));
        }
        if let Some(row) = cache_row(r) {
            rows.extend(usage_row("Cache", row, width));
        }
        let mut calls = r.calls.to_string();
        if let Some(latency) = r.avg_latency_s {
            calls.push_str(&format!(" · average latency {latency:.1} s"));
        }
        if let Some(speed) = r.tokens_per_s {
            calls.push_str(&format!(" · {speed:.1} tokens/s"));
        }
        rows.extend(usage_row("API calls", calls, width));
        if let Some(figure) = &r.context {
            let used = if figure.estimated {
                format!("~{}", grouped(figure.used))
            } else {
                grouped(figure.used)
            };
            let share = if figure.estimated {
                format!("(~{}%)", figure.percent)
            } else {
                format!("({}%)", figure.percent)
            };
            rows.extend(usage_row(
                "Context",
                format!("{used} / {} tokens {share}", grouped(figure.max)),
                width,
            ));
        }
        if let Some(compressions) = r.compressions {
            rows.extend(usage_row("Compactions", compressions.to_string(), width));
        }
        if let Some(subagents) = r.active_subagents.filter(|count| *count > 0) {
            rows.extend(usage_row(
                "Subagents",
                format!("{subagents} running in this Hermes (all sessions)"),
                width,
            ));
        }
        if !r.rate_limits.is_empty() {
            let mut parts: Vec<String> = r
                .rate_limits
                .iter()
                .map(|row| {
                    format!(
                        "{} {} of {} left",
                        row.name,
                        grouped(row.remaining),
                        grouped(row.limit)
                    )
                })
                .collect();
            if let Some(last) = parts.last_mut() {
                last.push_str(" (as of the last API call)");
            }
            rows.extend(usage_row("Rate limits", parts.join(" · "), width));
        }
    }
    rows.extend(usage_row("Cost", cost_text(&r.cost), width));
    if let Some(plan) = &r.plan {
        for row in plan_rows(plan, width) {
            rows.extend(row);
        }
    }
    rows
}

/// The model row, with its provider and fallback.
fn model_row(r: &UsageReport) -> Option<String> {
    let mut text = match (&r.model, &r.provider) {
        (Some(model), Some(provider)) => format!("{model} ({provider})"),
        (Some(model), None) => model.clone(),
        (None, _) => "unknown".to_owned(),
    };
    if let Some(fallback) = &r.fallback_model {
        text.push_str(&format!(" · fallback {fallback}"));
    }
    Some(text)
}

/// The token parts, for the figures that are known.
fn token_parts(r: &UsageReport) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(input) = r.input {
        parts.push(format!("input {}", grouped(input)));
    }
    if let Some(output) = r.output {
        parts.push(format!("output {}", grouped(output)));
    }
    if let Some(reasoning) = r.reasoning {
        parts.push(format!("reasoning {}", grouped(reasoning)));
    }
    if let Some(total) = r.total {
        parts.push(format!("total {}", grouped(total)));
    }
    parts
}

/// The cache row, when the session read or wrote tokens.
fn cache_row(r: &UsageReport) -> Option<String> {
    let shown =
        r.cache_read.is_some_and(|value| value > 0) || r.cache_write.is_some_and(|value| value > 0);
    if !shown {
        return None;
    }
    let mut parts = Vec::new();
    if let Some(read) = r.cache_read {
        parts.push(format!("read {}", grouped(read)));
    }
    if let Some(write) = r.cache_write {
        parts.push(format!("write {}", grouped(write)));
    }
    if let Some(hit) = r.cache_hit_pct {
        parts.push(format!("hit {hit}%"));
    }
    Some(parts.join(" · "))
}

/// The cost, as it reads.
fn cost_text(cost: &CostLine) -> String {
    match cost {
        CostLine::Amount { usd, status } => {
            let decimals = if *usd == 0.0 || *usd >= 0.01 { 2 } else { 4 };
            let text = format!("{usd:.decimals$}");
            let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
            let mut amount = format!("${}", grouped_text(whole));
            if !fraction.is_empty() {
                amount.push('.');
                amount.push_str(fraction);
            }
            match status {
                Some(status) => format!("{amount} ({status})"),
                None => amount,
            }
        }
        CostLine::Included => "included in your plan".to_owned(),
        CostLine::NotReported => "not reported by Hermes for this session".to_owned(),
    }
}

/// A number with a comma every three digits, from text.
fn grouped_text(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(character);
    }
    out
}

/// The plan rows: the plan itself and one row per bar.
fn plan_rows(plan: &Plan, width: u16) -> Vec<Vec<Line<'static>>> {
    let mut rows: Vec<Vec<Line<'static>>> = Vec::new();
    let mut parts: Vec<String> = Vec::new();
    parts.push(
        plan.name
            .clone()
            .unwrap_or_else(|| "Nous Portal".to_owned()),
    );
    if let Some(status) = &plan.status {
        parts.push(status.clone());
    }
    if let Some(spendable) = &plan.spendable {
        parts.push(format!("{spendable} spendable"));
    }
    if let Some(renews) = &plan.renews {
        parts.push(format!("renews {renews}"));
    }
    rows.push(usage_row("Plan", parts.join(" · "), width));
    for bar in [&plan.plan_bar, &plan.topup_bar].into_iter().flatten() {
        let filled = (bar.fill * 20.0).round() as usize;
        let mut text = String::new();
        for cell in 0..20 {
            text.push(if cell < filled { '■' } else { '·' });
        }
        let mut value = format!(
            "{} [{text}] {} of {} left",
            bar.kind, bar.remaining, bar.total
        );
        if let Some(used) = bar.pct_used {
            value.push_str(&format!(" · {used}% used"));
        }
        rows.push(prose(
            format!("               {value}"),
            Style::default(),
            width,
        ));
    }
    rows
}

/// The `/compact` block for a result.
pub fn compact_block(
    typed: &str,
    r: &CompactResult,
    width: u16,
    s: &ContextStyle,
) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut rows: Vec<Line<'static>> = Vec::new();
    rows.extend(prose(typed.to_owned(), s.dim, width));
    match r {
        CompactResult::Preview { lines } => {
            if lines.is_empty() {
                rows.extend(prose(
                    "  Hermes sent an empty preview.".to_owned(),
                    s.dim,
                    width,
                ));
            } else {
                for line in lines {
                    rows.extend(prose(format!("  {line}"), s.dim, width));
                }
            }
        }
        CompactResult::NoChange { summary } => match summary {
            Some(summary) => {
                rows.extend(prose(format!("  {}", summary.headline), s.dim, width));
                if let Some(line) = &summary.token_line {
                    rows.extend(prose(format!("    {line}"), s.dim, width));
                }
                if let Some(note) = &summary.note {
                    rows.extend(prose(format!("    {note}"), s.dim, width));
                }
            }
            None => rows.extend(prose("  Hermes made no change.".to_owned(), s.dim, width)),
        },
        CompactResult::Compacted {
            summary,
            message_count,
            ..
        } => {
            let headline = match summary {
                Some(summary) => format!("  ✓ {}", summary.headline),
                None => match message_count {
                    Some(count) => {
                        format!("  ✓ Compacted: the conversation now has {count} messages.")
                    }
                    None => "  ✓ Compacted.".to_owned(),
                },
            };
            rows.extend(prose(headline, s.tick, width));
            if let Some(summary) = summary {
                if let Some(line) = &summary.token_line {
                    rows.extend(prose(format!("    {line}"), s.dim, width));
                }
                if let Some(note) = &summary.note {
                    rows.extend(prose(format!("    {note}"), s.dim, width));
                }
            }
            rows.extend(prose(
                "  The model now sees a summary of the earlier conversation; your scrollback above \
                 is unchanged."
                    .to_owned(),
                s.dim,
                width,
            ));
        }
        CompactResult::Refused { text } => rows.extend(prose(format!("  {text}"), s.error, width)),
        CompactResult::Unknown => rows.extend(prose(
            format!("  {}", talaria_core::context::text::T_C13),
            s.error,
            width,
        )),
        CompactResult::NotSent { text } => rows.extend(prose(format!("  {text}"), s.error, width)),
        CompactResult::UnknownStatus { status } => rows.extend(prose(
            format!(
                "  Hermes answered with status \"{status}\", which this Talaria does not know. \
                 Nothing is assumed to have changed."
            ),
            s.error,
            width,
        )),
        CompactResult::Unreadable => rows.extend(prose(
            "  Hermes's answer could not be read. Run /usage to see whether a compaction was \
             counted."
                .to_owned(),
            s.error,
            width,
        )),
    }
    rows
}

/// A one-row notice block, with no typed line.
pub fn notice_block(n: &ContextNotice, width: u16, s: &ContextStyle) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let text = match n {
        ContextNotice::AutoCompacted => {
            "Context was compacted automatically during this turn.".to_owned()
        }
        ContextNotice::CompactedElsewhere { headline, .. } => match headline {
            Some(headline) => {
                format!("Conversation compacted by another Hermes client: {headline}")
            }
            None => "Conversation compacted by another Hermes client.".to_owned(),
        },
    };
    prose(text, s.dim, width)
}

/// The error block for a failed `/context` or `/usage`.
pub fn error_block(
    typed: &str,
    command: &str,
    method: &str,
    sid: &str,
    e: &ReadError,
    width: u16,
    s: &ContextStyle,
) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut rows: Vec<Line<'static>> = Vec::new();
    rows.extend(prose(typed.to_owned(), s.dim, width));
    rows.extend(prose(e.row(command, method, sid), s.error, width));
    rows
}

/// The live compacting line section.
pub struct CompactingSection {
    pub line: Option<CompactingLine>,
    pub now_ms: u64,
}

impl CompactingSection {
    /// The row the section shows, if it shows one.
    pub fn row(&self) -> Option<String> {
        let line = self.line?;
        let (words, since_ms) = match line {
            CompactingLine::Manual { since_ms } => ("Compacting conversation", since_ms),
            CompactingLine::Auto { since_ms } => ("Compacting context", since_ms),
        };
        let seconds = self.now_ms.saturating_sub(since_ms) / 1000;
        Some(format!("✻ {words} · {seconds}s"))
    }
}

impl LiveSection for CompactingSection {
    fn height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }
        u16::from(self.row().is_some())
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let Some(row) = self.row() else {
            return;
        };
        buf.set_line(
            area.x,
            area.y,
            &Line::from(Span::styled(row, Style::default())),
            area.width,
        );
    }
}
