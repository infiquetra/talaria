//! A selected file reaches real draw paths, not only the catalogue's style map.
use crate::view_colours_support as support;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Paragraph, Widget},
};
use std::collections::BTreeSet;
use talaria_app::theme_file::{ColorValue, StyleFields, catalogue};
use talaria_tui::markdown::MarkdownOptions;

#[test]
fn every_view_follows_the_file() {
    support::warm_syntax();
    let resolved = support::resolve(Some(&support::full_file()));
    let colours: BTreeSet<_> = resolved
        .elements
        .values()
        .flat_map(|style| [style.fg, style.bg, style.underline_color])
        .flatten()
        .map(|colour| format!("{colour:?}"))
        .collect();
    let mut failures = Vec::new();
    for fixture in support::fixtures(&resolved) {
        let bad = fixture.buffer.content.iter().enumerate().find(|(_, cell)| {
            (!cell.symbol().trim().is_empty() && cell.fg == Color::Reset)
                || [cell.fg, cell.bg, cell.underline_color]
                    .into_iter()
                    .any(|colour| {
                        colour != Color::Reset && !colours.contains(&format!("{colour:?}"))
                    })
        });
        if let Some((index, cell)) = bad {
            failures.push(format!(
                "{} ({}, {}): {cell:?}",
                fixture.name,
                index % 80,
                index / 80
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "views still drawing original/default colours:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_token_exists() {
    let resolved = support::resolve(Some(&support::full_file()));
    let tokens: BTreeSet<_> = catalogue().iter().map(|entry| entry.token).collect();
    assert_eq!(tokens.len(), catalogue().len(), "duplicate catalogue token");
    assert_eq!(resolved.elements.len(), tokens.len());
    for (_, token) in talaria_tui::theme::surfaces::SURFACE_BINDINGS {
        assert!(
            tokens.contains(token),
            "inline binding reads absent token {token}"
        );
    }
    for &(view, _, token) in support::PROBES {
        assert!(tokens.contains(token), "{view} reads absent token {token}");
        assert!(resolved.elements.contains_key(token));
    }
}

fn syntax(resolved: &talaria_tui::theme::ThemeFileResolution) -> Buffer {
    support::warm_syntax();
    let lines = resolved.render_markdown(
        "```rust\nfn main() {\n    let answer: u32 = 42;\n    println!(\"hello\"); // note\n}\n```",
        &MarkdownOptions {
            width: 80,
            theme: resolved.theme.markdown.clone(),
            color: resolved.theme.color,
            base_style: Style::default(),
        },
    );
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
    Paragraph::new(lines).render(buffer.area, &mut buffer);
    buffer
}

#[test]
fn syntax_follows_the_file() {
    let mut file = support::full_file();
    let probes = [
        ("fn", "code.syntax.keyword"),
        ("42", "code.syntax.number"),
        ("\"hello\"", "code.syntax.string"),
        ("// note", "code.syntax.comment"),
        ("main", "code.syntax.function"),
        ("u32", "code.syntax.type"),
    ];
    for (index, &(_, token)) in probes.iter().enumerate() {
        file.elements.insert(
            token.into(),
            StyleFields {
                fg: Some(ColorValue::Rgb(u8::try_from(index + 11).unwrap(), 137, 219)),
                underline_color: Some(ColorValue::Rgb(211, 97, 33)),
                underline: Some(true),
                ..StyleFields::default()
            },
        );
    }
    let resolved = support::resolve(Some(&file));
    let buffer = syntax(&resolved);
    for (needle, token) in probes {
        let cell = support::at_text(&buffer, needle);
        let expected = resolved.elements[token];
        assert_eq!(cell.fg, expected.fg.unwrap(), "{token} foreground");
        assert_eq!(
            cell.underline_color,
            expected.underline_color.unwrap(),
            "{token} underline colour"
        );
        assert!(
            cell.modifier.contains(ratatui::style::Modifier::UNDERLINED),
            "{token} underline attribute"
        );
    }
}

#[test]
fn backgrounds_only_on_owned_screens() {
    support::warm_syntax();
    let file = support::full_file();
    let resolved = support::resolve(Some(&file));
    let mut failures = Vec::new();
    for fixture in support::fixtures(&resolved) {
        if fixture.owned_screen {
            if let Some((index, cell)) = fixture
                .buffer
                .content
                .iter()
                .enumerate()
                .find(|(_, cell)| cell.bg == Color::Reset)
            {
                failures.push(format!(
                    "{} left its owned cell ({}, {}) unpainted: {cell:?}",
                    fixture.name,
                    index % 80,
                    index / 80
                ));
            }
        } else if !matches!(fixture.name, "menu" | "menu-dropdown")
            && let Some((index, cell)) = fixture
                .buffer
                .content
                .iter()
                .enumerate()
                .find(|(_, cell)| cell.bg != Color::Reset)
        {
            failures.push(format!(
                "{} painted an inline background at ({}, {}): {cell:?}",
                fixture.name,
                index % 80,
                index / 80
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn workspace_panel_and_menu_follow_the_file() {
    support::warm_syntax();
    let file = support::full_file();
    let resolved = support::resolve(Some(&file));
    let fixtures = support::fixtures(&resolved);
    for &(view, needle, token) in support::PROBES {
        let fixture = fixtures
            .iter()
            .find(|fixture| fixture.name == view)
            .unwrap_or_else(|| panic!("missing fixture {view}"));
        let cell = support::at_text(&fixture.buffer, needle);
        assert_eq!(
            Some(cell.fg),
            resolved.elements[token].fg,
            "{view}: {token}"
        );
    }
    assert!(
        support::PROBES
            .iter()
            .any(|(view, _, _)| *view == "workspace")
    );
    assert!(
        support::PROBES
            .iter()
            .any(|(view, _, _)| *view == "work-panel")
    );
    assert!(support::PROBES.iter().any(|(view, _, _)| *view == "menu"));
    let menu = fixtures.iter().find(|view| view.name == "menu").unwrap();
    let surface = Color::Rgb(0x25, 0x20, 0x1b);
    for x in 0..80 {
        assert_eq!(
            menu.buffer[(x, 0)].bg,
            surface,
            "menu owns its entire first row"
        );
    }
    for y in 1..24 {
        for x in 0..80 {
            assert_eq!(
                menu.buffer[(x, y)].bg,
                Color::Reset,
                "menu does not own the rows below it"
            );
        }
    }
}

#[test]
fn element_overrides_reach_visible_cells_independently_of_role_defaults() {
    support::warm_syntax();
    for &(view, needle, token) in support::PROBES {
        let mut file = support::full_file();
        file.elements.insert(
            token.into(),
            StyleFields {
                fg: Some(ColorValue::Rgb(17, 91, 203)),
                bold: Some(true),
                italic: Some(true),
                ..StyleFields::default()
            },
        );
        let resolved = support::resolve(Some(&file));
        let fixtures = support::fixtures(&resolved);
        let fixture = fixtures
            .iter()
            .find(|fixture| fixture.name == view)
            .unwrap();
        let cell = support::at_text(&fixture.buffer, needle);
        assert_eq!(cell.fg, Color::Rgb(17, 91, 203), "{view}: {token}");
        assert!(
            cell.modifier
                .contains(ratatui::style::Modifier::BOLD | ratatui::style::Modifier::ITALIC),
            "{view}: {token} attributes"
        );
    }
}
