//! Contrast diagnostics for selected theme files.

use super::{Rgb, ThemeInputs, color, surfaces};
use ratatui::style::{Color, Style};
use std::collections::BTreeMap;
use talaria_app::theme_file::catalogue;
use talaria_core::appearance::ThemeFileReport;

fn measurable(color: Option<Color>) -> Option<Rgb> {
    match color? {
        Color::Rgb(r, g, b) => Some(Rgb(r, g, b)),
        Color::Indexed(index @ 16..=255) => Some(color::indexed_rgb(index)),
        // Named ANSI colours and indexes below 16 are controlled by the terminal owner.
        _ => None,
    }
}

/// Inline pickers drop the selection fill to preserve terminal scrollback. Either side of an
/// authored selection can be its readable ink (Homebrew has green ink on a deep green fill).
pub(super) fn selection_ink(style: Style, canvas: Option<Color>) -> Option<Color> {
    if let (Some(canvas), Some(fill), Some(ink)) = (
        canvas.and_then(color::rgb_of),
        style.bg.and_then(color::rgb_of),
        style.fg.and_then(color::rgb_of),
    ) && color::contrast_ratio(fill, canvas) < 4.5
        && color::contrast_ratio(ink, canvas) > color::contrast_ratio(fill, canvas)
    {
        return style.fg;
    }
    style.bg
}

/// Correct a readable RGB pair when converting its ink and fill independently loses contrast.
pub(super) fn ansi16_ink(
    authored: Style,
    converted: Style,
    role: &str,
    canvases: [Color; 2],
) -> Option<Color> {
    let Color::Rgb(r, g, b) = authored.fg? else {
        return None;
    };
    let original = Rgb(r, g, b);
    let original_fill = authored.bg?;
    let fill = color::rgb_of(converted.bg?)?;
    let ink = color::rgb_of(converted.fg?)?;
    let floor = if role == "border" { 3.0 } else { 4.5 };
    if color::contrast_ratio(original, color::rgb_of(original_fill)?) < floor {
        return None;
    }
    // Tool inks are also reused on the list surface in the review panel. Prefer a swatch
    // that preserves contrast on both original surfaces when the authored colour did so.
    let other = canvases
        .iter()
        .position(|canvas| *canvas == original_fill)
        .and_then(|index| color::rgb_of(canvases[1 - index]))
        .filter(|rgb| color::contrast_ratio(original, *rgb) >= floor)
        .map(|rgb| color::indexed_rgb(color::to_ansi16(rgb)));
    let readable = |ink| color::contrast_ratio(ink, fill) >= floor;
    let on_both =
        |ink| readable(ink) && other.is_none_or(|bg| color::contrast_ratio(ink, bg) >= floor);
    if on_both(ink) {
        return None;
    }
    color::nearest_ansi16(original, on_both)
        .or_else(|| color::nearest_ansi16(original, readable))
        .map(color::ansi_color)
}

/// Keep finished text legible on the terminal's native scrollback background.
pub(crate) fn native_history_line(line: &mut ratatui::text::Line<'_>, background: Rgb) {
    for style in std::iter::once(&mut line.style).chain(line.spans.iter_mut().map(|s| &mut s.style))
    {
        if measurable(style.fg).is_some_and(|rgb| color::contrast_ratio(rgb, background) < 4.5) {
            style.fg = Some(Color::Reset);
        }
    }
}

fn inline(token: &str) -> bool {
    matches!(
        token.split('.').next(),
        Some("transcript" | "composer" | "status" | "tool" | "diff" | "workspace" | "effort")
    ) || surfaces::SURFACE_BINDINGS
        .iter()
        .any(|(_, bound)| *bound == token)
}

pub(super) fn diagnose(
    elements: &BTreeMap<String, Style>,
    inputs: &ThemeInputs<'_>,
    report: &mut ThemeFileReport,
) {
    let terminal_background = inputs
        .terminal
        .background
        .filter(|color| *color != Rgb(0, 0, 0));
    for entry in catalogue() {
        let Some(style) = elements.get(entry.token) else {
            continue;
        };
        // Paired selection defaults describe whole owned selection cells. They are not one of
        // the currently bound inline fields; their foreground is meaningful against that pair.
        let paired = matches!(entry.role, "selection" | "focus");
        let background = if inline(entry.token) && !paired {
            terminal_background
        } else {
            measurable(style.bg)
        };
        let floor = if entry.role == "border" { 3.0 } else { 4.5 };
        check(report, entry.token, measurable(style.fg), background, floor);
        let inline_label = match entry.token {
            "list.selected" => Some("list.selected (inline)"),
            "composer.completion.selected" => Some("composer.completion.selected (inline)"),
            "dialog.clarification.selected" => Some("dialog.clarification.selected (inline)"),
            _ => None,
        };
        if let Some(label) = inline_label {
            check(
                report,
                label,
                measurable(selection_ink(
                    *style,
                    elements
                        .get("transcript.background")
                        .and_then(|style| style.bg),
                )),
                terminal_background,
                floor,
            );
        }
    }
}

fn check(
    report: &mut ThemeFileReport,
    label: &str,
    foreground: Option<Rgb>,
    background: Option<Rgb>,
    floor: f64,
) {
    let Some((foreground, background)) = foreground.zip(background) else {
        report.unknown_contrast.push(label.into());
        return;
    };
    let ratio = color::contrast_ratio(foreground, background);
    if ratio < floor {
        report.warnings.push(format!(
            "{label} contrast {ratio:.2}:1 is below {floor:.1}:1"
        ));
    }
}
