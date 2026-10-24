//! Drawn rows of the composer, compared as cell symbols.

#[path = "composer_support/mod.rs"]
mod composer_support;

use std::collections::BTreeMap;

use ratatui::style::Modifier;
use talaria_tui::composer::Composer;
use talaria_tui::keys::KeyMap;
use talaria_tui::live::LiveSection;

use composer_support::{Rig, rows, style_at, symbol_at};

const PLACEHOLDER: &str = "Type a message. Enter sends, Shift+Enter adds a line.";
const PLACEHOLDER_ALT: &str = "Type a message. Enter sends, Alt+Enter adds a line.";

#[test]
fn drafts_at_60_columns() {
    let mut rig = Rig::new();
    let section = rig.composer.section(60, 10);
    let height = section.height(60);
    assert_eq!(height, 3);
    let drawn = rows(&section, 60, height);
    assert_eq!(
        drawn,
        vec![
            edge('╭', '╮', ""),
            content_row("> ", PLACEHOLDER),
            edge('╰', '╯', ""),
        ]
    );
    assert!(style_at(&section, 60, height, 4, 1).contains(Modifier::REVERSED));
    assert!(style_at(&section, 60, height, 2, 1).contains(Modifier::BOLD));
    assert!(style_at(&section, 60, height, 5, 1).contains(Modifier::DIM));

    rig.composer.set_keyboard_enhancement(false);
    let section = rig.composer.section(60, 10);
    let height = section.height(60);
    let drawn = rows(&section, 60, height);
    assert_eq!(drawn[1], content_row("> ", PLACEHOLDER_ALT));

    rig.composer.load("one\ntwo".to_string());
    let section = rig.composer.section(60, 10);
    let height = section.height(60);
    assert_eq!(height, 4);
    let drawn = rows(&section, 60, height);
    assert_eq!(drawn[1], content_row("> ", "one"));
    assert_eq!(drawn[2], content_row("  ", "two"));
    assert!(drawn[1].starts_with("│ > "));
    assert!(drawn[2].starts_with("│   "));
    assert!(style_at(&section, 60, height, 7, 2).contains(Modifier::REVERSED));
}

#[test]
fn scrolling_hints_and_exit_hint() {
    let mut rig = Rig::new();
    let draft = (1..=30)
        .map(|n| format!("L{n:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    rig.composer.load(draft);
    let section = rig.composer.section(60, 10);
    let height = section.height(60);
    assert_eq!(height, 10);
    assert_eq!(rows(&section, 60, height), window(23, 30, "↑ 22 more", ""));

    for _ in 0..15 {
        rig.press("up");
    }
    let section = rig.composer.section(60, 10);
    let height = section.height(60);
    assert_eq!(
        rows(&section, 60, height),
        window(15, 22, "↑ 14 more", "↓ 8 more")
    );
    assert!(style_at(&section, 60, height, 7, 1).contains(Modifier::REVERSED));

    rig.press("ctrl+c");
    assert_eq!(rig.composer.text(), "");
    assert_eq!(rig.composer.history().len(), 1);
    let section = rig.composer.section(60, 10);
    let height = section.height(60);
    let drawn = rows(&section, 60, height);
    assert_eq!(drawn[1], content_row("> ", PLACEHOLDER));
    assert!(!drawn.iter().any(|row| row.contains("Press Ctrl")));

    rig.press("ctrl+c");
    let section = rig.composer.section(60, 10);
    let height = section.height(60);
    let drawn = rows(&section, 60, height);
    assert_eq!(
        *drawn.last().unwrap(),
        edge('╰', '╯', "Press Ctrl+C again to exit")
    );

    rig.press("z");
    let section = rig.composer.section(60, 10);
    let drawn = rows(&section, 60, section.height(60));
    assert_eq!(drawn[1], content_row("> ", "z"));
    assert!(!drawn.iter().any(|row| row.contains("Press Ctrl")));

    let mut entries = BTreeMap::new();
    entries.insert("cancel".to_string(), vec!["ctrl+d".to_string()]);
    let (map, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    let mut rebound = Rig::from_composer(Composer::new(map));
    rebound.press("ctrl+d");
    let section = rebound.composer.section(60, 10);
    let drawn = rows(&section, 60, section.height(60));
    assert_eq!(
        *drawn.last().unwrap(),
        edge('╰', '╯', "Press Ctrl+D again to exit")
    );
}

#[test]
fn narrow_wide_and_hostile_text() {
    let mut rig = Rig::new();
    rig.composer.load("hi".to_string());
    let section = rig.composer.section(19, 10);
    let height = section.height(19);
    assert_eq!(height, 1);
    let drawn = rows(&section, 19, height);
    assert_eq!(drawn, vec!["> hi".to_string()]);
    assert!(style_at(&section, 19, height, 4, 0).contains(Modifier::REVERSED));

    let section = rig.composer.section(20, 10);
    let height = section.height(20);
    assert_eq!(height, 3);
    assert_eq!(
        rows(&section, 20, height),
        vec![
            "╭──────────────────╮".to_string(),
            "│ > hi             │".to_string(),
            "╰──────────────────╯".to_string(),
        ]
    );

    let section = rig.composer.section(20, 2);
    let height = section.height(20);
    assert_eq!(height, 1);
    assert_eq!(rows(&section, 20, height), vec!["> hi".to_string()]);

    rig.composer.load("a\t你e\u{0301}".to_string());
    let section = rig.composer.section(60, 10);
    let height = section.height(60);
    let drawn = rows(&section, 60, height);
    assert!(drawn[1].contains("a    你"));
    assert!(!drawn[1].contains('\t'));
    assert!(drawn[1].ends_with('│'));
    assert_eq!(symbol_at(&section, 60, height, 0, 1), "│");
    assert_eq!(symbol_at(&section, 60, height, 59, 1), "│");
    assert_eq!(symbol_at(&section, 60, height, 4, 1), "a");
    assert_eq!(symbol_at(&section, 60, height, 9, 1), "你");
    assert_eq!(symbol_at(&section, 60, height, 11, 1), "e\u{0301}");

    let mut hostile = String::new();
    hostile.push('\u{202E}');
    hostile.push('x');
    hostile.push('\u{200B}');
    rig.composer.handle_paste(&hostile);
    assert_eq!(rig.composer.text(), "a\t你e\u{0301}x");
    assert!(!rig.composer.text().contains('\u{202E}'));
    assert!(!rig.composer.text().contains('\u{200B}'));

    rig.composer.set_title(Some("Hi".to_string()));
    let section = rig.composer.section(20, 10);
    let drawn = rows(&section, 20, section.height(20));
    assert_eq!(drawn[0], "╭ Hi───────────────╮");
}

#[test]
fn huge_draft_draws_only_visible_rows() {
    let text = (1..=50_000)
        .map(|n| format!("L{n:05}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut rig = Rig::new();
    rig.composer.load(text);
    let section = rig.composer.section(60, 10);
    let height = section.height(60);
    assert!(height <= 10, "height {height}");
    let drawn = rows(&section, 60, height);
    let blob = drawn.join("\n");
    for n in 1..=50_000 {
        let label = format!("L{n:05}");
        let want = (49_993..=50_000).contains(&n);
        assert_eq!(blob.contains(&label), want, "{label} in {blob}");
    }
}

/// A boxed content row: border, space, two-cell prompt, 53 text cells, two spaces, border.
fn content_row(prompt: &str, text: &str) -> String {
    let width = 53;
    let body = format!("{text:<width$}");
    format!("│ {prompt}{body}  │")
}

/// Top or bottom border. `hint` is right-aligned in the 58-cell interior.
fn edge(left: char, right: char, hint: &str) -> String {
    let interior = 58usize.saturating_sub(hint.chars().count());
    format!("{left}{}{hint}{right}", "─".repeat(interior))
}

/// Eight visible labels `L{first}` through `L{last}`, with the prompt on the first of them.
fn window(first: u32, last: u32, up: &str, down: &str) -> Vec<String> {
    let mut drawn = vec![edge('╭', '╮', up)];
    for n in first..=last {
        let prompt = if n == first { "> " } else { "  " };
        drawn.push(content_row(prompt, &format!("L{n:02}")));
    }
    drawn.push(edge('╰', '╯', down));
    drawn
}
