//! Colour constraints reach every palette and preserve content and attributes.
use crate::view_colours_support as support;

use talaria_tui::theme::detect::{self, EnvView};
use talaria_tui::theme::{self, ColorsSetting};

#[test]
fn nonempty_no_color_strips_every_view_colour() {
    assert_off(detect::depth(
        &EnvView::from_pairs(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")]),
        ColorsSetting::TrueColor,
    ));
}

#[test]
fn config_colors_none_strips_every_view_colour() {
    assert_off(detect::depth(&EnvView::default(), ColorsSetting::None));
}

fn assert_off(depth: (theme::ColorDepth, theme::DepthSource)) {
    support::warm_syntax();
    let file = support::full_file();
    let enabled = support::fixtures(&support::resolve(Some(&file)));
    let resolved = theme::resolve_with_theme_file(&support::inputs(depth), Some(&file));
    assert!(!resolved.theme.color);
    let disabled = support::fixtures(&resolved);
    assert_eq!(enabled.len(), disabled.len());
    let mut failures = Vec::new();
    for (on, off) in enabled.iter().zip(&disabled) {
        assert_eq!(on.name, off.name);
        assert_eq!(
            support::text(&on.buffer),
            support::text(&off.buffer),
            "{}: colours-off changed symbols",
            on.name
        );
        for (index, (on, off)) in on
            .buffer
            .content
            .iter()
            .zip(&off.buffer.content)
            .enumerate()
        {
            assert!(
                off.modifier.contains(on.modifier),
                "{}: removed attributes at ({}, {})",
                off.symbol(),
                index % 80,
                index / 80
            );
        }
        if let Some(evidence) = support::no_colour(&off.buffer) {
            failures.push(format!("{}: {evidence}", off.name));
        }
    }
    let rewind = resolved.rewind_theme(true);
    for (name, style) in [
        ("selected", rewind.selected),
        ("metadata", rewind.dim),
        ("warning", rewind.warning),
        ("accent", rewind.accent),
    ] {
        if style.fg.is_some() || style.bg.is_some() || style.underline_color.is_some() {
            failures.push(format!("rewind.{name}: {style:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "colours remain after the final constraint:\n{}",
        failures.join("\n")
    );
}
