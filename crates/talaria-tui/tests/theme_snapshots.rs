//! Resolved themes reach the screen. Behavior contract: issue 227.
//!
//! The expected colours come from specification 7.2 and 7.3, computed by hand
//! from the skin fixtures, not copied from the issue rows.

mod support;

use std::path::PathBuf;

use serde_json::Value;
use support::{ByteSink, probe};
use talaria_core::appearance::{Skin, SkippedValue};
use talaria_core::session::{CellId, ToolCell, ToolStatus};
use talaria_tui::composer::Composer;
use talaria_tui::keys::KeyMap;
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::markdown::{MarkdownOptions, render_markdown};
use talaria_tui::status::{ExternalSegment, StatusInputs, StatusSection, Tone};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};
use talaria_tui::theme::{
    ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, Rgb, TerminalColors,
    Theme, ThemeInputs,
};
use talaria_tui::tool_cell::{Detail, ToolRenderOptions, tool_lines};

fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb(r, g, b)
}

fn skin_of(name: &str) -> (Skin, Vec<SkippedValue>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/skins")
        .join(name);
    let data: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let (skin, skipped) = Skin::from_value(&data);
    (skin.expect("the fixture is a skin"), skipped)
}

fn resolve_for(
    skin: &Skin,
    skipped: &[SkippedValue],
    terminal: TerminalColors,
    depth: (ColorDepth, DepthSource),
) -> Theme {
    let dark = terminal.background == Some(rgb(0x1e, 0x1e, 0x1e));
    let polarity = if dark {
        (
            Polarity::Dark,
            PolaritySource::TerminalBackground(rgb(0x1e, 0x1e, 0x1e)),
        )
    } else {
        (
            Polarity::Light,
            PolaritySource::TerminalBackground(rgb(0xff, 0xff, 0xff)),
        )
    };
    talaria_tui::theme::resolve(&ThemeInputs {
        skin: Some(skin),
        skipped,
        follow_hermes_skin: true,
        polarity,
        depth,
        motion: (Motion::Full, MotionSource::Default),
        terminal,
    })
}

/// Draw the composer and one status line through the real terminal.
fn draw_screen(theme: &Theme, markdown: &[ratatui::text::Line<'static>]) -> vt100::Parser {
    let sink = ByteSink::default();
    // The live region starts at its drawn height (composer and status line), so the committed
    // lines above it stay on the 12-row screen.
    let mut term = InlineTerminal::new(
        sink.clone(),
        (100, 12),
        4,
        probe(true, false, Some((0, 11))),
        RestoreLink::detached(),
    )
    .unwrap();
    if !markdown.is_empty() {
        term.commit_lines(markdown.to_vec()).unwrap();
    }
    let mut composer = Composer::with_history(KeyMap::defaults(), Vec::new());
    composer.set_style(theme.composer);
    let section = composer.section(100, 5);
    let inputs = StatusInputs {
        needs_you: 2,
        ..StatusInputs::default()
    };
    let external = ExternalSegment {
        text: "model sonnet".to_owned(),
        tone: Tone::Text,
    };
    let status = StatusSection::new(&inputs, Some(&external), &theme.status);
    let view = LiveView {
        sections: vec![&section as &dyn LiveSection, &status as &dyn LiveSection],
    };
    term.draw_live(&view).unwrap();
    let mut parser = vt100::Parser::new(12, 100, 0);
    parser.process(&sink.take());
    parser
}

/// The foreground colour of the first cell of `needle` on its row.
fn first_cell_fg(parser: &vt100::Parser, needle: &str) -> vt100::Color {
    use unicode_width::UnicodeWidthStr;
    for row in 0..12 {
        let text: String = parser
            .screen()
            .rows(0, 100)
            .nth(row as usize)
            .unwrap_or_default()
            .trim_end()
            .to_owned();
        if let Some(index) = text.find(needle) {
            let col = UnicodeWidthStr::width(&text[..index]) as u16;
            let cell = parser
                .screen()
                .cell(row, col)
                .expect("a cell for the match");
            return cell.fgcolor();
        }
    }
    panic!("{needle:?} is not on the screen")
}

#[test]
fn composer_and_status_follow_default_skin() {
    let (skin, skipped) = skin_of("default.json");
    let terminal = TerminalColors {
        background: Some(rgb(0x1e, 0x1e, 0x1e)),
        foreground: None,
    };
    let theme = resolve_for(
        &skin,
        &skipped,
        terminal,
        (ColorDepth::TrueColor, DepthSource::Colorterm),
    );
    let parser = draw_screen(&theme, &[]);

    // The composer border's top-left cell: input_rule, dark.
    assert_eq!(
        first_cell_fg(&parser, "╭"),
        vt100::Color::Rgb(0xcd, 0x7f, 0x32),
        "the composer border"
    );
    assert_eq!(
        first_cell_fg(&parser, "model sonnet"),
        vt100::Color::Rgb(0xc0, 0xc0, 0xc0),
        "the text entry"
    );
    assert_eq!(
        first_cell_fg(&parser, "need you"),
        vt100::Color::Rgb(0xff, 0xd7, 0x00),
        "the attention entry"
    );

    // Light: the overlay recolours text and attention, not the border.
    let light = TerminalColors {
        background: Some(rgb(0xff, 0xff, 0xff)),
        foreground: None,
    };
    let theme = resolve_for(
        &skin,
        &skipped,
        light,
        (ColorDepth::TrueColor, DepthSource::Colorterm),
    );
    let parser = draw_screen(&theme, &[]);
    assert_eq!(
        first_cell_fg(&parser, "╭"),
        vt100::Color::Rgb(0xcd, 0x7f, 0x32),
        "the composer border, light"
    );
    assert_eq!(
        first_cell_fg(&parser, "model sonnet"),
        vt100::Color::Rgb(0x6f, 0x6f, 0x6f),
        "the text entry, light"
    );
    assert_eq!(
        first_cell_fg(&parser, "need you"),
        vt100::Color::Rgb(0xc8, 0x96, 0x1e),
        "the attention entry, light"
    );

    // Depth none: every cell keeps the default colours.
    let theme = resolve_for(
        &skin,
        &skipped,
        terminal,
        (ColorDepth::None, DepthSource::NoColorEnv),
    );
    let parser = draw_screen(&theme, &[]);
    for row in 0..12 {
        for col in 0..100 {
            if let Some(cell) = parser.screen().cell(row, col) {
                assert_eq!(
                    cell.fgcolor(),
                    vt100::Color::Default,
                    "no foreground at {row},{col}"
                );
                assert_eq!(
                    cell.bgcolor(),
                    vt100::Color::Default,
                    "no background at {row},{col}"
                );
            }
        }
    }
}

#[test]
fn ansi256_uses_indexed_colours() {
    let (skin, skipped) = skin_of("default.json");
    let terminal = TerminalColors {
        background: Some(rgb(0x1e, 0x1e, 0x1e)),
        foreground: None,
    };
    let theme = resolve_for(
        &skin,
        &skipped,
        terminal,
        (ColorDepth::Ansi256, DepthSource::Term),
    );
    let parser = draw_screen(&theme, &[]);

    assert_eq!(
        first_cell_fg(&parser, "╭"),
        vt100::Color::Idx(173),
        "the composer border"
    );
    assert_eq!(
        first_cell_fg(&parser, "model sonnet"),
        vt100::Color::Idx(250)
    );
    assert_eq!(first_cell_fg(&parser, "need you"), vt100::Color::Idx(220));
    for row in 0..12 {
        for col in 0..100 {
            if let Some(cell) = parser.screen().cell(row, col) {
                assert!(
                    !matches!(cell.fgcolor(), vt100::Color::Rgb(..)),
                    "no RGB foreground at {row},{col}"
                );
                assert!(
                    !matches!(cell.bgcolor(), vt100::Color::Rgb(..)),
                    "no RGB background at {row},{col}"
                );
            }
        }
    }
}

#[test]
fn markdown_cells_use_skin_roles() {
    let (skin, skipped) = skin_of("ares.json");
    let terminal = TerminalColors {
        background: Some(rgb(0x1e, 0x1e, 0x1e)),
        foreground: None,
    };
    let theme = resolve_for(
        &skin,
        &skipped,
        terminal,
        (ColorDepth::TrueColor, DepthSource::Colorterm),
    );
    let options = MarkdownOptions {
        width: 80,
        theme: theme.markdown.clone(),
        color: theme.color,
        base_style: ratatui::style::Style::default(),
    };
    let lines = render_markdown(
        "# Title\n\n[link](http://example.com) and `code`\n",
        &options,
    );
    let parser = draw_screen(&theme, &lines);

    assert_eq!(
        first_cell_fg(&parser, "Title"),
        vt100::Color::Rgb(0xc7, 0xa9, 0x6b),
        "the level-1 heading"
    );
    assert_eq!(
        first_cell_fg(&parser, "link"),
        vt100::Color::Rgb(0xdd, 0x4a, 0x3a),
        "the link"
    );
    assert_eq!(
        first_cell_fg(&parser, "code"),
        vt100::Color::Rgb(0xdd, 0x4a, 0x3a),
        "the inline code"
    );
}

#[test]
fn tool_cells_use_skin_roles() {
    let (skin, skipped) = skin_of("ares.json");
    let dark = TerminalColors {
        background: Some(rgb(0x1e, 0x1e, 0x1e)),
        foreground: None,
    };
    let theme = resolve_for(
        &skin,
        &skipped,
        dark,
        (ColorDepth::TrueColor, DepthSource::Colorterm),
    );
    let finished = ToolCell {
        id: CellId::new(1),
        tool_id: "call-1".to_owned(),
        name: "terminal".to_owned(),
        context: None,
        args: None,
        labels: Vec::new(),
        status: ToolStatus::Succeeded,
        progress: None,
        summary: None,
        result: None,
        diff: None,
        duration_s: None,
    };
    let lines = tool_lines(
        &finished,
        &ToolRenderOptions {
            width: 80,
            theme: theme.tool.clone(),
            color: theme.color,
            detail: Detail::Compact,
            expand_hint: None,
            elapsed: None,
        },
    );
    let mark = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .find(|span| span.content.contains('\u{2713}'))
        .expect("a finished call shows its tick");
    // `ui_ok` of the skin, which already clears the semantic floor on this terminal.
    assert_eq!(
        mark.style.fg,
        Some(ratatui::style::Color::Rgb(0x4c, 0xaf, 0x50))
    );
    // Without `diff_added_word` the added-row colour falls back to `ui_ok`.
    assert_eq!(
        theme.tool.diff_added.fg,
        Some(ratatui::style::Color::Rgb(0x4c, 0xaf, 0x50))
    );
}
