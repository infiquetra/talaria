//! Pure history text: what `sanitize` replaces, how many rows `wrap_lines` gives, and which cells
//! the commit ledger hands out.
//!
//! Hostile inputs live here, never under `src/`: the source tree may not contain an escape byte
//! outside the terminal module.

mod support;

use std::borrow::Cow;

use ratatui::buffer::CellWidth;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use talaria_core::session::Cell;
use talaria_tui::history::{CommitLedger, sanitize, wrap_lines};
use unicode_segmentation::UnicodeSegmentation;

#[test]
fn sanitize_replaces_controls_with_visible_stand_ins() {
    let cases: Vec<(&str, &str)> = vec![
        // The screen-clearing sequence and its relatives.
        ("\u{1b}[2J", "\u{241b}[2J"),
        ("\u{1b}[H", "\u{241b}[H"),
        ("\u{7}", "\u{2407}"),
        ("\r", "\u{240d}"),
        ("\n", "\u{240a}"),
        ("a\nb", "a\u{240a}b"),
        ("\u{0}", "\u{2400}"),
        ("\u{8}", "\u{2408}"),
        ("\u{1f}", "\u{241f}"),
        // Tab is four spaces, not a Control Picture.
        ("\t", "    "),
        ("a\tb", "a    b"),
        // Delete.
        ("\u{7f}", "\u{2421}"),
        // The C1 controls, at both ends of the range and the single-byte CSI.
        ("\u{80}", "\u{2039}U+0080\u{203a}"),
        ("\u{9b}", "\u{2039}U+009B\u{203a}"),
        ("\u{9f}", "\u{2039}U+009F\u{203a}"),
        // Direction marks and separators.
        ("\u{61c}", "\u{2039}U+061C\u{203a}"),
        ("\u{200e}", "\u{2039}U+200E\u{203a}"),
        ("\u{200f}", "\u{2039}U+200F\u{203a}"),
        ("\u{202a}", "\u{2039}U+202A\u{203a}"),
        ("\u{202b}", "\u{2039}U+202B\u{203a}"),
        ("\u{202c}", "\u{2039}U+202C\u{203a}"),
        ("\u{202d}", "\u{2039}U+202D\u{203a}"),
        ("\u{202e}", "\u{2039}U+202E\u{203a}"),
        ("\u{2066}", "\u{2039}U+2066\u{203a}"),
        ("\u{2067}", "\u{2039}U+2067\u{203a}"),
        ("\u{2068}", "\u{2039}U+2068\u{203a}"),
        ("\u{2069}", "\u{2039}U+2069\u{203a}"),
        ("\u{2028}", "\u{2039}U+2028\u{203a}"),
        ("\u{2029}", "\u{2039}U+2029\u{203a}"),
        // The tag block: five hex digits, upper case.
        ("\u{e0000}", "\u{2039}U+E0000\u{203a}"),
        ("\u{e0020}", "\u{2039}U+E0020\u{203a}"),
        ("\u{e007f}", "\u{2039}U+E007F\u{203a}"),
        // Hermes's 24-bit colour sequence is shown, not obeyed.
        (
            "\u{1b}[38;2;255;255;255;48;2;120;20;20m",
            "\u{241b}[38;2;255;255;255;48;2;120;20;20m",
        ),
        // A hostile line as a whole.
        (
            "HOSTILE \u{1b}[2J\u{1b}[H\u{7}\u{202e} END",
            "HOSTILE \u{241b}[2J\u{241b}[H\u{2407}\u{2039}U+202E\u{203a} END",
        ),
        // Characters next to the replaced ranges are left alone.
        ("\u{a0}", "\u{a0}"),
        ("\u{ff}", "\u{ff}"),
        ("\u{200b}", "\u{200b}"),
        ("\u{200d}", "\u{200d}"),
        ("\u{2027}", "\u{2027}"),
        ("\u{202f}", "\u{202f}"),
        ("\u{2065}", "\u{2065}"),
        ("\u{206a}", "\u{206a}"),
        ("\u{dffff}", "\u{dffff}"),
        ("\u{e0080}", "\u{e0080}"),
        ("\u{ffff}", "\u{ffff}"),
        ("\u{10ffff}", "\u{10ffff}"),
        // Several replacements in a row.
        ("\u{1b}\u{1b}", "\u{241b}\u{241b}"),
        (
            "\u{9b}\u{9b}",
            "\u{2039}U+009B\u{203a}\u{2039}U+009B\u{203a}",
        ),
    ];
    for (input, expected) in cases {
        let got = sanitize(input);
        assert_eq!(got, expected, "input {input:?}");
        assert!(
            matches!(got, Cow::Owned(_)) || input == expected,
            "changed text must come back owned: {input:?}"
        );
    }

    // Clean text is borrowed, not copied.
    for clean in [
        "",
        "plain ascii",
        "caf\u{e9} \u{754c} \u{1f642}",
        "e\u{301}",
        "\u{2764}\u{fe0f}",
        "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
        "\u{a0}\u{2027}\u{202f}",
    ] {
        assert!(
            matches!(sanitize(clean), Cow::Borrowed(text) if text == clean),
            "clean text must be borrowed: {clean:?}"
        );
    }
}

/// The row text and the style of every character, for rows made of any number of spans.
fn styled_chars(row: &Line<'_>) -> Vec<(char, Style)> {
    row.spans
        .iter()
        .flat_map(|span| span.content.chars().map(|c| (c, span.style)))
        .collect()
}

fn row_text(row: &Line<'_>) -> String {
    row.spans.iter().map(|span| span.content.as_ref()).collect()
}

fn row_width(row: &Line<'_>) -> usize {
    row_text(row)
        .graphemes(true)
        .map(|grapheme| usize::from(grapheme.cell_width()))
        .sum()
}

/// Wrap one line of `text` at `width` and check what holds for every wrap: no row is wider than
/// the width, and the rows together hold exactly the original graphemes in order, none split.
fn wrap_text(text: &str, width: u16) -> Vec<String> {
    let rows = wrap_lines(vec![Line::from(text.to_string())], width);
    let limit = usize::from(width.max(1));
    for row in &rows {
        assert!(
            row_width(row) <= limit,
            "row {:?} is wider than {limit} for input {text:?}",
            row_text(row)
        );
    }
    let before: Vec<&str> = text.graphemes(true).collect();
    let texts: Vec<String> = rows.iter().map(row_text).collect();
    let after: Vec<&str> = texts.iter().flat_map(|row| row.graphemes(true)).collect();
    assert_eq!(
        after, before,
        "rows must hold the original graphemes whole and in order: {texts:?}"
    );
    texts
}

#[test]
fn wrap_lines_counts_rows_exactly() {
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    let heart = "\u{2764}\u{fe0f}";
    let combining = "e\u{301}";
    let cases: Vec<(&str, String, u16, usize)> = vec![
        ("empty line is one row", String::new(), 10, 1),
        ("ten letters", "abcdefghij".to_string(), 10, 1),
        ("eleven letters", "abcdefghijk".to_string(), 10, 2),
        (
            "nine letters and a wide character",
            "abcdefghi\u{754c}".to_string(),
            10,
            2,
        ),
        ("five wide characters", "\u{754c}".repeat(5), 10, 1),
        ("six smiling faces", "\u{1f642}".repeat(6), 10, 2),
        ("five family emoji", family.repeat(5), 10, 1),
        ("six family emoji", family.repeat(6), 10, 2),
        ("six red hearts with selector", heart.repeat(6), 10, 2),
        (
            "ten letters with combining marks",
            combining.repeat(10),
            10,
            1,
        ),
        (
            "eleven letters with combining marks",
            combining.repeat(11),
            10,
            2,
        ),
        ("width zero counts as one", "abc".to_string(), 0, 3),
        ("width one", "abc".to_string(), 1, 3),
        ("exactly two rows", "a".repeat(20), 10, 2),
        ("one more than two rows", "a".repeat(21), 10, 3),
    ];
    for (name, text, width, expected) in cases {
        let rows = wrap_text(&text, width);
        assert_eq!(rows.len(), expected, "{name}: rows {rows:?}");
    }

    // The wide character that does not fit moves down whole.
    assert_eq!(
        wrap_text("abcdefghi\u{754c}", 10),
        vec!["abcdefghi".to_string(), "\u{754c}".to_string()]
    );
    // Eleven letters break after the tenth.
    assert_eq!(
        wrap_text("abcdefghijk", 10),
        vec!["abcdefghij".to_string(), "k".to_string()]
    );
    // A family emoji never splits, so the sixth starts the second row.
    assert_eq!(
        wrap_text(&family.repeat(6), 10),
        vec![family.repeat(5), family.to_string()]
    );
    assert_eq!(
        wrap_text(&heart.repeat(6), 10),
        vec![heart.repeat(5), heart.to_string()]
    );
    assert_eq!(wrap_text("abc", 0), vec!["a", "b", "c"]);

    // A zero-width character stays on the current row and takes no column.
    assert_eq!(wrap_text("a\u{200b}b", 2), vec!["a\u{200b}b".to_string()]);

    // Each source line is wrapped on its own: an empty line stays one empty row.
    let rows = wrap_lines(
        vec![
            Line::from("ab"),
            Line::from(""),
            Line::from("abcdefghijk"),
            Line::from("cd"),
        ],
        10,
    );
    let texts: Vec<String> = rows.iter().map(row_text).collect();
    assert_eq!(texts, vec!["ab", "", "abcdefghij", "k", "cd"]);
    assert!(wrap_lines(Vec::new(), 10).is_empty());
}

#[test]
fn wrap_lines_keeps_span_styles() {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let red = Style::default().fg(Color::Red);
    let plain = Style::default();
    let line = Line::from(vec![
        Span::styled("abcdef", bold),
        Span::styled("ghijkl", red),
        Span::raw("mn"),
    ]);
    let rows = wrap_lines(vec![line], 5);
    let got: Vec<Vec<(char, Style)>> = rows.iter().map(styled_chars).collect();
    let expected: Vec<Vec<(char, Style)>> = vec![
        "abcde".chars().map(|c| (c, bold)).collect(),
        std::iter::once(('f', bold))
            .chain("ghij".chars().map(|c| (c, red)))
            .collect(),
        "kl".chars()
            .map(|c| (c, red))
            .chain("mn".chars().map(|c| (c, plain)))
            .collect(),
    ];
    assert_eq!(got, expected);
}

fn ids(cells: &[Cell]) -> Vec<String> {
    cells.iter().map(|cell| format!("{cell:?}")).collect()
}

#[test]
fn ledger_commits_longest_final_prefix_once() {
    let mut ledger = CommitLedger::new();

    // Final, final, live, final: the live cell holds back the one after it.
    let waiting = support::cells(&[true, true, false, true]);
    let first = ledger.take_ready(&waiting);
    assert_eq!(first, &waiting[0..2]);
    assert_eq!(ids(first), ids(&waiting[0..2]));

    // Asking again with nothing changed gives nothing.
    assert!(ledger.take_ready(&waiting).is_empty());

    // The third cell becomes final: the two remaining cells come out together.
    let settled = support::cells(&[true; 4]);
    let second = ledger.take_ready(&settled);
    assert_eq!(second, &settled[2..4]);

    // Everything has been handed out.
    assert!(ledger.take_ready(&settled).is_empty());

    // A new foreground session starts again from the first cell.
    ledger.reset();
    assert_eq!(ledger.take_ready(&settled), &settled[..]);
    assert!(ledger.take_ready(&settled).is_empty());
}

#[test]
fn ledger_holds_back_everything_behind_a_live_cell() {
    let mut ledger = CommitLedger::new();
    assert!(ledger.take_ready(&[]).is_empty());

    // A live first cell holds back every later cell, even final ones.
    let live_first = support::cells(&[false, true, true]);
    assert!(ledger.take_ready(&live_first).is_empty());

    // Cells appended after the ones already handed out come out on their own.
    let all = support::cells(&[true, true, true, true, false]);
    assert_eq!(ledger.take_ready(&all), &all[0..4]);
    let more = support::cells(&[true, true, true, true, true, true]);
    assert_eq!(ledger.take_ready(&more), &more[4..6]);
    assert!(ledger.take_ready(&more).is_empty());
}
