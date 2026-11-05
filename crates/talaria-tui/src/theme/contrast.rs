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

fn inline(token: &str) -> bool {
    matches!(
        token.split('.').next(),
        Some("transcript" | "composer" | "status" | "tool" | "diff" | "workspace")
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
        let Some((foreground, background)) = measurable(style.fg).zip(background) else {
            report.unknown_contrast.push(entry.token.into());
            continue;
        };
        let floor = if entry.role == "border" { 3.0 } else { 4.5 };
        let ratio = color::contrast_ratio(foreground, background);
        if ratio < floor {
            report.warnings.push(format!(
                "{} contrast {:.2}:1 is below {:.1}:1",
                entry.token, ratio, floor
            ));
        }
    }
}
