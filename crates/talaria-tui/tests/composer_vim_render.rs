//! Mode segment rows, and the same rows with vim left off.

#[path = "composer_support/mod.rs"]
mod composer_support;

use ratatui::style::Modifier;
use talaria_tui::live::LiveSection;

use composer_support::{Rig, rows, style_at};

const PLACEHOLDER: &str = "Type a message. Enter sends, Shift+Enter adds a line.";

#[test]
fn mode_segment_and_off_parity() {
    let mut plain = Rig::new();
    assert_eq!(drawn(&mut plain, 60), empty_boxed());

    plain.composer.load("hello world".to_string());
    assert_eq!(
        drawn(&mut plain, 60),
        vec![
            edge('╭', '╮', ""),
            content_row("> ", "hello world"),
            edge('╰', '╯', ""),
        ]
    );

    plain.composer.load("one\ntwo".to_string());
    assert_eq!(
        drawn(&mut plain, 60),
        vec![
            edge('╭', '╮', ""),
            content_row("> ", "one"),
            content_row("  ", "two"),
            edge('╰', '╯', ""),
        ]
    );

    plain.composer.load("hi".to_string());
    let narrow = plain.composer.section(19, 10);
    let narrow_height = narrow.height(19);
    assert_eq!(rows(&narrow, 19, narrow_height), vec!["> hi".to_string()]);

    let mut scrolled = Rig::new();
    scrolled.composer.load(thirty_lines());
    assert_eq!(drawn(&mut scrolled, 60), window(23, 30, "↑ 22 more", ""));

    let mut armed = Rig::new();
    armed.press("ctrl+c");
    let armed_rows = drawn(&mut armed, 60);
    let armed_hint = edge('╰', '╯', "Press Ctrl+C again to exit");
    assert_eq!(
        armed_rows.last().map(String::as_str),
        Some(armed_hint.as_str())
    );
    assert!(armed_rows.iter().all(|row| !row.contains("-- ")));

    let mut vim = Rig::new();
    vim.composer.set_vim_enabled(true);
    let empty = drawn(&mut vim, 60);
    assert!(
        empty.iter().any(|row| row.contains("-- NORMAL --")),
        "empty vim draft shows -- NORMAL --, got {empty:?}"
    );
    assert!(
        empty.iter().all(|row| !row.contains(PLACEHOLDER)),
        "an empty normal draft has no placeholder: {empty:?}"
    );
    assert_eq!(
        empty,
        vec![
            edge('╭', '╮', ""),
            content_row("> ", ""),
            mode_bottom("-- NORMAL --"),
        ]
    );
    let empty_section = vim.composer.section(60, 10);
    let empty_height = empty_section.height(60);
    assert!(style_at(&empty_section, 60, empty_height, 4, 1).contains(Modifier::REVERSED));
    assert!(style_at(&empty_section, 60, empty_height, 2, 1).contains(Modifier::DIM));

    vim.composer.load("hello world".to_string());
    assert_eq!(bottom(&mut vim), mode_bottom("-- NORMAL --"));
    assert_eq!(top(&mut vim), edge('╭', '╮', ""));
    let normal = vim.composer.section(60, 10);
    let normal_height = normal.height(60);
    assert!(style_at(&normal, 60, normal_height, 14, 1).contains(Modifier::REVERSED));

    vim.press("i");
    assert_eq!(bottom(&mut vim), mode_bottom("-- INSERT --"));
    vim.press("esc");
    assert_eq!(bottom(&mut vim), mode_bottom("-- NORMAL --"));

    vim.press("d2");
    assert_eq!(bottom(&mut vim), mode_bottom("-- NORMAL --  d2"));
    let pending = vim.composer.section(60, 10);
    let pending_height = pending.height(60);
    assert!(style_at(&pending, 60, pending_height, 2, pending_height - 1).contains(Modifier::DIM));
    assert!(
        !style_at(&pending, 60, pending_height, 16, pending_height - 1).contains(Modifier::DIM)
    );
    vim.press("esc");

    vim.composer.load("hello world".to_string());
    vim.composer.set_caret(0);
    vim.press("ve");
    assert_eq!(bottom(&mut vim), mode_bottom("-- VISUAL --"));
    let visual = vim.composer.section(60, 10);
    let visual_height = visual.height(60);
    assert!(style_at(&visual, 60, visual_height, 4, 1).contains(Modifier::REVERSED));
    assert!(style_at(&visual, 60, visual_height, 8, 1).contains(Modifier::REVERSED));
    assert!(!style_at(&visual, 60, visual_height, 9, 1).contains(Modifier::REVERSED));
    vim.press("esc");

    vim.press("V");
    assert_eq!(bottom(&mut vim), mode_bottom("-- VISUAL LINE --"));
    vim.press("esc");

    vim.composer.load(String::new());
    vim.press("i");
    vim.press("ctrl+c");
    let exit_rows = drawn(&mut vim, 60);
    let exit_hint = edge('╰', '╯', "Press Ctrl+C again to exit");
    assert_eq!(
        exit_rows.last().map(String::as_str),
        Some(exit_hint.as_str())
    );
    assert!(exit_rows.iter().all(|row| !row.contains("-- INSERT --")));

    vim.press("esc");
    vim.composer.load(thirty_lines());
    let wide = drawn(&mut vim, 60);
    assert_eq!(wide[0], edge('╭', '╮', "↑ 22 more"));
    let wide_mode = mode_bottom("-- NORMAL --");
    assert_eq!(wide.last().map(String::as_str), Some(wide_mode.as_str()));
    assert!(wide.iter().all(|row| !row.contains('↓')));
    assert!(wide.iter().any(|row| row.contains("L30")));

    let borderless = vim.composer.section(19, 10);
    let borderless_height = borderless.height(19);
    let narrow_rows = rows(&borderless, 19, borderless_height);
    assert_eq!(narrow_rows.last().map(String::as_str), Some("-- NORMAL --"));
    assert!(
        narrow_rows
            .iter()
            .all(|row| !row.contains('↓') && !row.contains('↑'))
    );
    assert!(narrow_rows.iter().any(|row| row.contains("L30")));

    vim.press("99999999999999999999");
    let crowded = vim.composer.section(20, 10);
    let crowded_height = crowded.height(20);
    let crowded_rows = rows(&crowded, 20, crowded_height);
    let crowded_bottom = crowded_rows.last().cloned().unwrap_or_default();
    assert!(crowded_bottom.contains("-- NORMAL --"), "{crowded_bottom}");
    assert!(!crowded_bottom.contains('9'), "{crowded_bottom}");
}

fn drawn(rig: &mut Rig, width: u16) -> Vec<String> {
    let section = rig.composer.section(width, 10);
    let height = section.height(width);
    rows(&section, width, height)
}

fn bottom(rig: &mut Rig) -> String {
    drawn(rig, 60).last().cloned().unwrap_or_default()
}

fn top(rig: &mut Rig) -> String {
    drawn(rig, 60).first().cloned().unwrap_or_default()
}

fn thirty_lines() -> String {
    (1..=30)
        .map(|n| format!("L{n:02}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn content_row(prompt: &str, text: &str) -> String {
    let width = 53;
    let body = format!("{text:<width$}");
    format!("│ {prompt}{body}  │")
}

fn edge(left: char, right: char, hint: &str) -> String {
    let interior = 58usize.saturating_sub(hint.chars().count());
    format!("{left}{}{hint}{right}", "─".repeat(interior))
}

fn mode_bottom(segment: &str) -> String {
    let used = 1 + segment.chars().count();
    let dashes = 58usize.saturating_sub(used);
    format!("╰ {segment}{}╯", "─".repeat(dashes))
}

fn empty_boxed() -> Vec<String> {
    vec![
        edge('╭', '╮', ""),
        content_row("> ", PLACEHOLDER),
        edge('╰', '╯', ""),
    ]
}

fn window(first: u32, last: u32, up: &str, down: &str) -> Vec<String> {
    let mut drawn = vec![edge('╭', '╮', up)];
    for n in first..=last {
        let prompt = if n == first { "> " } else { "  " };
        drawn.push(content_row(prompt, &format!("L{n:02}")));
    }
    drawn.push(edge('╰', '╯', down));
    drawn
}
