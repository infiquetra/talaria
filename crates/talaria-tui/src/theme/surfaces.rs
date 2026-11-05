//! Bind the existing inline style fields to catalogue tokens.

use super::Theme;
use crate::status::{StatusPalette, Tone};
use ratatui::style::{Modifier, Style};
use std::collections::BTreeMap;

/// Concrete style field and its catalogue token.
pub const SURFACE_BINDINGS: &[(&str, &str)] = &[
    ("markdown.heading1", "transcript.heading.1"),
    ("markdown.heading2", "transcript.heading.2"),
    ("markdown.heading3", "transcript.heading.3"),
    ("markdown.strong", "transcript.strong"),
    ("markdown.emphasis", "transcript.emphasis"),
    ("markdown.strikethrough", "transcript.strikethrough"),
    ("markdown.inline_code", "code.inline"),
    ("markdown.link_text", "transcript.link.text"),
    ("markdown.link_destination", "transcript.link.destination"),
    ("markdown.image_alt", "transcript.image.alt"),
    ("markdown.quote_bar", "transcript.quote.bar"),
    ("markdown.rule", "transcript.rule"),
    ("markdown.code_label", "code.label"),
    ("markdown.table_border", "transcript.table.border"),
    ("markdown.table_header", "transcript.table.header"),
    ("status.text", "status.text"),
    ("status.muted", "status.muted"),
    ("status.separator", "status.separator"),
    ("status.success", "status.success"),
    ("status.warning", "status.warning"),
    ("status.error", "status.error"),
    ("status.attention", "status.attention"),
    ("status.accent", "status.accent"),
    ("composer.border", "composer.border.focused"),
    ("composer.marker", "composer.marker"),
    ("reasoning", "transcript.reasoning"),
    ("tool.glyph_running", "tool.running"),
    ("tool.glyph_ok", "tool.success"),
    ("tool.glyph_failed", "tool.failed"),
    ("tool.glyph_muted", "tool.muted"),
    ("tool.title", "tool.title"),
    ("tool.subject", "tool.subject"),
    ("tool.meta", "tool.metadata"),
    ("tool.output", "tool.output"),
    ("tool.error", "tool.error"),
    ("tool.diff_added", "diff.added"),
    ("tool.diff_removed", "diff.removed"),
    ("tool.diff_context", "diff.context"),
    ("tool.diff_hunk", "diff.hunk"),
    ("tool.diff_file", "diff.file"),
    ("tool.diff_note", "diff.note"),
    ("tool.line_number", "diff.line_number"),
];

const TONES: [(Tone, &str); 8] = [
    (Tone::Text, "status.text"),
    (Tone::Muted, "status.muted"),
    (Tone::Separator, "status.separator"),
    (Tone::Success, "status.success"),
    (Tone::Warning, "status.warning"),
    (Tone::Error, "status.error"),
    (Tone::Attention, "status.attention"),
    (Tone::Accent, "status.accent"),
];

macro_rules! style_fields {
    ($theme:ident, $field:ident, $style:ident, $($($path:ident).+),+ $(,)?) => {
        match $field {
            $(stringify!($($path).+) => { $theme.$($path).+ = $style; },)+
            _ => {}
        }
    };
}

macro_rules! read_fields {
    ($theme:ident, $field:ident, $($($path:ident).+),+ $(,)?) => {
        match $field {
            $(stringify!($($path).+) => Some($theme.$($path).+),)+
            _ => None
        }
    };
}

fn read(theme: &Theme, field: &str) -> Option<Style> {
    read_fields!(
        theme,
        field,
        markdown.heading1,
        markdown.heading2,
        markdown.heading3,
        markdown.strong,
        markdown.emphasis,
        markdown.strikethrough,
        markdown.inline_code,
        markdown.link_text,
        markdown.link_destination,
        markdown.image_alt,
        markdown.quote_bar,
        markdown.rule,
        markdown.code_label,
        markdown.table_border,
        markdown.table_header,
        composer.border,
        composer.marker,
        reasoning,
        tool.glyph_running,
        tool.glyph_ok,
        tool.glyph_failed,
        tool.glyph_muted,
        tool.title,
        tool.subject,
        tool.meta,
        tool.output,
        tool.error,
        tool.diff_added,
        tool.diff_removed,
        tool.diff_context,
        tool.diff_hunk,
        tool.diff_file,
        tool.diff_note,
        tool.line_number
    )
    .or_else(|| {
        TONES
            .iter()
            .find(|(_, name)| *name == field)
            .map(|(tone, _)| theme.status.style(*tone))
    })
}

/// Inspect every bound style without relying on the resolver's binding application.
pub fn bound_styles(theme: &Theme) -> Vec<(&'static str, Style)> {
    SURFACE_BINDINGS
        .iter()
        .filter_map(|(field, _)| read(theme, field).map(|style| (*field, style)))
        .collect()
}

pub(super) fn apply(
    theme: &mut Theme,
    elements: &BTreeMap<String, Style>,
    sources: &BTreeMap<String, BTreeMap<String, String>>,
) {
    let mut status = TONES.map(|(tone, _)| theme.status.style(tone));
    for &(field, token) in SURFACE_BINDINGS {
        let Some(&style) = elements.get(token) else {
            continue;
        };
        // Inline cells never own the terminal background, including retained scrollback.
        let mut style = Style { bg: None, ..style };
        if !style.add_modifier.contains(Modifier::UNDERLINED)
            && sources
                .get(token)
                .and_then(|fields| fields.get("underline_color"))
                .is_some_and(|source| source == "skin")
        {
            // An untouched skin foreground need not introduce a dormant underline emission.
            // The complete catalogue keeps its effective-foreground fallback independently.
            style.underline_color = read(theme, field).and_then(|legacy| legacy.underline_color);
        }
        style_fields!(
            theme,
            field,
            style,
            markdown.heading1,
            markdown.heading2,
            markdown.heading3,
            markdown.strong,
            markdown.emphasis,
            markdown.strikethrough,
            markdown.inline_code,
            markdown.link_text,
            markdown.link_destination,
            markdown.image_alt,
            markdown.quote_bar,
            markdown.rule,
            markdown.code_label,
            markdown.table_border,
            markdown.table_header,
            composer.border,
            composer.marker,
            reasoning,
            tool.glyph_running,
            tool.glyph_ok,
            tool.glyph_failed,
            tool.glyph_muted,
            tool.title,
            tool.subject,
            tool.meta,
            tool.output,
            tool.error,
            tool.diff_added,
            tool.diff_removed,
            tool.diff_context,
            tool.diff_hunk,
            tool.diff_file,
            tool.diff_note,
            tool.line_number
        );
        if let Some(index) = TONES.iter().position(|(_, name)| *name == field) {
            status[index] = style;
        }
    }
    theme.status = StatusPalette::from_styles([
        status[0], status[1], status[2], status[3], status[4], status[5], status[6],
    ])
    .with_accent(status[7]);
}
