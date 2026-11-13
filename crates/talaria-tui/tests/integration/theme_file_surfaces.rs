//! Catalogue overrides reach every current inline surface and terminal bytes.
use crate::color;
use crate::support;

use ratatui::style::{Color, Style};
use std::collections::BTreeMap;
use talaria_app::theme_file::{
    self, Base, BorderPreset, Chrome, ColorValue, StyleFields, ThemeFile,
};
use talaria_core::session::{Cell, CellId, ReasoningCell, ToolCell, ToolStatus};
use talaria_tui::composer::Composer;
use talaria_tui::history::CellLines;
use talaria_tui::keys::KeyMap;
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::markdown::{MarkdownOptions, MarkdownTheme, render_markdown};
use talaria_tui::status::{ExternalSegment, StatusInputs, StatusSection, Tone};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};
use talaria_tui::theme::{self, *};
use talaria_tui::tool_cell::{Detail, ToolRenderOptions, ToolTheme, tool_lines};

fn input() -> ThemeInputs<'static> {
    ThemeInputs {
        skin: None,
        skipped: &[],
        follow_hermes_skin: true,
        polarity: (Polarity::Dark, PolaritySource::Config),
        depth: (ColorDepth::TrueColor, DepthSource::Config),
        motion: (Motion::Full, MotionSource::Default),
        terminal: TerminalColors::default(),
    }
}
fn empty() -> ThemeFile {
    ThemeFile {
        name: None,
        extends: Base::Dark,
        follow_hermes_skin: false,
        palette: BTreeMap::new(),
        roles: BTreeMap::new(),
        elements: BTreeMap::new(),
        chrome: Chrome::default(),
    }
}

/// Destructuring deliberately rejects newly added fields until they receive a binding.
fn concrete_styles(theme: &Theme) -> BTreeMap<&'static str, Style> {
    let MarkdownTheme {
        heading1,
        heading2,
        heading3,
        strong,
        emphasis,
        strikethrough,
        inline_code,
        link_text,
        link_destination,
        image_alt,
        quote_bar,
        rule,
        code_label,
        table_border,
        table_header,
    } = theme.markdown.clone();
    let ComposerStyle { border, marker } = theme.composer;
    let ToolTheme {
        glyph_running,
        glyph_ok,
        glyph_failed,
        glyph_muted,
        title,
        subject,
        meta,
        output,
        error,
        diff_added,
        diff_removed,
        diff_context,
        diff_hunk,
        diff_file,
        diff_note,
        line_number,
    } = theme.tool.clone();
    [
        ("markdown.heading1", heading1),
        ("markdown.heading2", heading2),
        ("markdown.heading3", heading3),
        ("markdown.strong", strong),
        ("markdown.emphasis", emphasis),
        ("markdown.strikethrough", strikethrough),
        ("markdown.inline_code", inline_code),
        ("markdown.link_text", link_text),
        ("markdown.link_destination", link_destination),
        ("markdown.image_alt", image_alt),
        ("markdown.quote_bar", quote_bar),
        ("markdown.rule", rule),
        ("markdown.code_label", code_label),
        ("markdown.table_border", table_border),
        ("markdown.table_header", table_header),
        ("composer.border", border),
        ("composer.marker", marker),
        ("reasoning", theme.reasoning),
        ("status.text", theme.status.style(Tone::Text)),
        ("status.muted", theme.status.style(Tone::Muted)),
        ("status.separator", theme.status.style(Tone::Separator)),
        ("status.success", theme.status.style(Tone::Success)),
        ("status.warning", theme.status.style(Tone::Warning)),
        ("status.error", theme.status.style(Tone::Error)),
        ("status.attention", theme.status.style(Tone::Attention)),
        ("status.accent", theme.status.style(Tone::Accent)),
        ("tool.glyph_running", glyph_running),
        ("tool.glyph_ok", glyph_ok),
        ("tool.glyph_failed", glyph_failed),
        ("tool.glyph_muted", glyph_muted),
        ("tool.title", title),
        ("tool.subject", subject),
        ("tool.meta", meta),
        ("tool.output", output),
        ("tool.error", error),
        ("tool.diff_added", diff_added),
        ("tool.diff_removed", diff_removed),
        ("tool.diff_context", diff_context),
        ("tool.diff_hunk", diff_hunk),
        ("tool.diff_file", diff_file),
        ("tool.diff_note", diff_note),
        ("tool.line_number", line_number),
    ]
    .into()
}

#[test]
fn all_42_concrete_fields_have_one_binding_and_obey_their_token() {
    assert_eq!(theme::surfaces::SURFACE_BINDINGS.len(), 42);
    let mut file = empty();
    let mut expected = BTreeMap::new();
    for (index, &(field, token)) in theme::surfaces::SURFACE_BINDINGS.iter().enumerate() {
        let channel = u8::try_from(index).unwrap() + 1;
        assert!(
            expected
                .insert(field, Color::Rgb(channel, 101, 202))
                .is_none()
        );
        assert!(
            file.elements
                .insert(
                    token.into(),
                    StyleFields {
                        fg: Some(ColorValue::Rgb(channel, 101, 202)),
                        bg: Some(ColorValue::Rgb(9, 8, 7)),
                        ..StyleFields::default()
                    }
                )
                .is_none(),
            "duplicate token {token}"
        );
    }
    let result = theme::resolve_with_theme_file(&input(), Some(&file));
    let actual = concrete_styles(&result.theme);
    assert_eq!(actual.len(), 42);
    let inspected: BTreeMap<_, _> = theme::surfaces::bound_styles(&result.theme)
        .into_iter()
        .collect();
    assert_eq!(inspected, actual);
    for (field, style) in actual {
        assert_eq!(style.fg, Some(expected[field]), "{field}");
        assert_eq!(
            style.bg, None,
            "inline field {field} must not paint a background"
        );
    }
    assert!(
        theme::surfaces::SURFACE_BINDINGS.contains(&("composer.border", "composer.border.focused"))
    );
}

fn foreground(parser: &vt100::Parser, needle: &str) -> vt100::Color {
    for (row, text) in parser.screen().rows(0, 100).enumerate() {
        if let Some(column) = text.find(needle) {
            let column = unicode_width::UnicodeWidthStr::width(&text[..column]);
            return parser
                .screen()
                .cell(u16::try_from(row).unwrap(), u16::try_from(column).unwrap())
                .unwrap()
                .fgcolor();
        }
    }
    panic!("missing {needle:?}: {}", parser.screen().contents());
}

fn draw(file: &ThemeFile) -> vt100::Parser {
    let result = theme::resolve_with_theme_file(&input(), Some(file));
    let theme = result.theme;
    let sink = support::ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (100, 24),
        4,
        support::probe(true, false, Some((0, 23))),
        RestoreLink::detached(),
    )
    .unwrap();
    let mut lines = render_markdown(
        "# Heading",
        &MarkdownOptions {
            width: 100,
            theme: theme.markdown.clone(),
            color: true,
            base_style: Style::default(),
        },
    );
    lines.extend(theme::cells::ThemedCells { theme: &theme }.lines(
        &Cell::Reasoning(ReasoningCell {
            id: CellId::new(1),
            text: "Reasoning prose".into(),
            sealed: true,
        }),
        100,
    ));
    lines.extend(tool_lines(
        &ToolCell {
            id: CellId::new(2),
            tool_id: "fixture".into(),
            name: "terminal".into(),
            context: None,
            args: None,
            labels: vec![],
            status: ToolStatus::Succeeded,
            progress: None,
            summary: None,
            result: None,
            diff: None,
            duration_s: None,
        },
        &ToolRenderOptions {
            width: 100,
            theme: theme.tool.clone(),
            color: true,
            detail: Detail::Compact,
            expand_hint: None,
            elapsed: None,
        },
    ));
    let diff = talaria_core::diff::parse_diff(
        "--- a/fixture\n+++ b/fixture\n@@ -1 +1 @@\n-old\n+added fixture\n",
    );
    lines.extend(
        talaria_tui::diff_view::diff_lines(
            &diff,
            &talaria_tui::diff_view::DiffRenderOptions {
                width: 100,
                indent: 0,
                max_rows: None,
                theme: theme.tool.clone(),
                color: true,
            },
        )
        .0,
    );
    term.commit_lines(lines).unwrap();
    let mut composer = Composer::with_history(KeyMap::defaults(), vec![]);
    composer.set_style(theme.composer);
    let section = composer.section(100, 5).with_border_preset(result.border);
    let status_input = StatusInputs::default();
    let external = ExternalSegment {
        text: "Status fixture".into(),
        tone: Tone::Text,
    };
    let status = StatusSection::new(&status_input, Some(&external), &theme.status);
    term.draw_live(&LiveView {
        sections: vec![&section as &dyn LiveSection, &status as &dyn LiveSection],
    })
    .unwrap();
    let mut parser = vt100::Parser::new(24, 100, 0);
    parser.process(&sink.take());
    for row in 0..24 {
        for column in 0..100 {
            assert_eq!(
                parser.screen().cell(row, column).unwrap().bgcolor(),
                vt100::Color::Default,
                "background byte at {row},{column}"
            );
        }
    }
    parser
}

#[test]
fn study_minimal_and_full_examples_reach_real_draw_paths() {
    if color::run_with_terminal_colors(
        "theme_file_surfaces::study_minimal_and_full_examples_reach_real_draw_paths",
    ) {
        return;
    }
    let minimal = theme_file::parse(include_str!(
        "../../../talaria-app/tests/data/themes/minimal.toml"
    ))
    .unwrap();
    let parser = draw(&minimal);
    assert_eq!(foreground(&parser, "╭"), vt100::Color::Rgb(255, 184, 107));
    assert_eq!(foreground(&parser, ">"), vt100::Color::Rgb(255, 184, 107));
    let full = theme_file::parse(include_str!(
        "../../../talaria-app/tests/data/themes/ember.toml"
    ))
    .unwrap();
    let parser = draw(&full);
    for (needle, expected) in [
        ("Heading", vt100::Color::Rgb(255, 192, 120)),
        ("Status fixture", vt100::Color::Rgb(245, 234, 219)),
        ("╭", vt100::Color::Rgb(255, 184, 107)),
        (">", vt100::Color::Rgb(255, 184, 107)),
        ("Reasoning prose", vt100::Color::Rgb(196, 181, 162)),
        ("✓", vt100::Color::Rgb(168, 208, 141)),
        ("added fixture", vt100::Color::Rgb(168, 208, 141)),
    ] {
        assert_eq!(foreground(&parser, needle), expected, "{needle}");
    }
}

#[test]
fn all_border_presets_change_corners_sides_and_vim_segment_without_resizing() {
    use ratatui::{buffer::Buffer, layout::Rect};
    for vim in [false, true] {
        for (preset, corners, horizontal, vertical) in [
            (BorderPreset::Rounded, ['╭', '╮', '╰', '╯'], '─', '│'),
            (BorderPreset::Single, ['┌', '┐', '└', '┘'], '─', '│'),
            (BorderPreset::Double, ['╔', '╗', '╚', '╝'], '═', '║'),
            (BorderPreset::Ascii, ['+', '+', '+', '+'], '-', '|'),
            (BorderPreset::None, [' ', ' ', ' ', ' '], ' ', ' '),
        ] {
            let mut composer = Composer::from_startup(KeyMap::defaults(), vec![], vim);
            let section = composer.section(80, 5).with_border_preset(preset);
            let height = section.height(80);
            assert_eq!(height, 3);
            let mut buffer = Buffer::empty(Rect::new(0, 0, 80, height));
            section.render(buffer.area, &mut buffer);
            for ((x, y), expected) in [
                ((0, 0), corners[0]),
                ((79, 0), corners[1]),
                ((0, 2), corners[2]),
                ((79, 2), corners[3]),
            ] {
                assert_eq!(
                    buffer[(x, y)].symbol(),
                    expected.to_string(),
                    "{preset:?} vim={vim}"
                );
            }
            assert_eq!(buffer[(70, 0)].symbol(), horizontal.to_string());
            assert_eq!(buffer[(70, 2)].symbol(), horizontal.to_string());
            assert_eq!(buffer[(0, 1)].symbol(), vertical.to_string());
            assert_eq!(buffer[(79, 1)].symbol(), vertical.to_string());
            assert_eq!(buffer[(2, 1)].symbol(), ">");
        }
    }
}
