mod markdown_support;

use markdown_support::{options, read_fixture, render, to_ansi};
use talaria_tui::markdown::render_markdown;

fn control_char(ch: char) -> bool {
    let code = u32::from(ch);
    code <= 0x1F || code == 0x7F || (0x80..=0x9F).contains(&code)
}

#[test]
fn wide_text_fits_80_columns() {
    let text = read_fixture("wide.md");
    let lines = render(&text, 80);
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(&to_ansi(&lines));
    let screen = parser.screen();
    for row in 0..24 {
        assert!(!screen.row_wrapped(row), "row {row} wrapped");
    }
}

#[test]
fn escapes_never_reach_terminal() {
    let text = read_fixture("hostile.md");
    let lines = render_markdown(&text, &options(80));
    for line in &lines {
        for span in &line.spans {
            for ch in span.content.chars() {
                assert!(!control_char(ch), "a span contains a control character");
            }
        }
    }
    let mut bytes = b"MARKER\r\n".to_vec();
    bytes.extend(to_ansi(&lines));
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(&bytes);
    let screen = parser.screen();
    let contents = screen.contents();
    let picture = char::from_u32(0x241b).unwrap();
    assert!(contents.contains("MARKER"), "the earlier line is gone");
    assert!(
        contents.contains(&format!("{picture}[2J")),
        "the clear sequence should be visible text"
    );
    assert!(contents.contains("‹U+202E›"));
    let expected_row = u16::try_from(1 + lines.len()).unwrap_or(u16::MAX);
    assert_eq!(screen.cursor_position(), (expected_row, 0));
}

#[test]
fn bold_reaches_cells() {
    let lines = render("say **bold** now\n", 80);
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(&to_ansi(&lines));
    let screen = parser.screen();
    for col in 4..8 {
        let cell = screen.cell(0, col).expect("cell");
        assert!(cell.bold(), "column {col} is {}", cell.contents());
    }
}
