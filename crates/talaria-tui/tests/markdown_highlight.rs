mod markdown_support;

use markdown_support::{any_colour, ensure_highlighter, options, render, span_colour};
use ratatui::style::Color;
use talaria_tui::markdown::MarkdownStream;

fn fence(lang: &str, body: &str) -> String {
    format!("```{lang}\n{body}\n```\n")
}

#[test]
fn string_comment_number_colours() {
    ensure_highlighter();
    let rust = render(&fence("rust", "let n = 42; // note\nlet s = \"a\";"), 80);
    assert_eq!(span_colour(&rust, "\"a\""), Some(Color::Green));
    assert_eq!(span_colour(&rust, "// note"), Some(Color::DarkGray));
    assert_eq!(span_colour(&rust, "42"), Some(Color::Yellow));

    let python = render(&fence("python", "n = 42  # note\ns = \"a\""), 80);
    assert_eq!(span_colour(&python, "\"a\""), Some(Color::Green));
    assert_eq!(span_colour(&python, "# note"), Some(Color::DarkGray));
    assert_eq!(span_colour(&python, "42"), Some(Color::Yellow));

    let shell = render(&fence("shell", "echo \"a\" # note"), 80);
    assert_eq!(span_colour(&shell, "\"a\""), Some(Color::Green));
    assert_eq!(span_colour(&shell, "# note"), Some(Color::DarkGray));
}

#[test]
fn language_tokens_resolve() {
    ensure_highlighter();
    for lang in ["Rust", "rs", "py", "Python", "shell"] {
        let body = if lang == "shell" {
            "echo \"a\""
        } else if lang == "py" || lang == "Python" {
            "s = \"a\""
        } else {
            "let s = \"a\";"
        };
        let lines = render(&fence(lang, body), 80);
        assert_eq!(
            span_colour(&lines, "\"a\""),
            Some(Color::Green),
            "{lang} should colour a string"
        );
    }
    for lang in ["ts", "toml"] {
        let lines = render(&fence(lang, "s = \"a\""), 80);
        assert!(
            !any_colour(&lines),
            "{lang} has no bundled grammar and stays plain"
        );
    }
}

#[test]
fn unknown_language_is_plain() {
    ensure_highlighter();
    let lines = render(&fence("notalang", "let s = \"a\";"), 80);
    assert!(!any_colour(&lines));
    let indented = render("    let s = \"a\";\n", 80);
    assert!(!any_colour(&indented));
}

#[test]
fn open_block_highlights_new_lines() {
    ensure_highlighter();
    let mut markdown = MarkdownStream::new(options(80));
    let first = markdown.push("```rust\nlet s = \"a\";\n");
    assert!(first.committed.is_empty());
    let before = markdown.lines_highlighted();
    let second = markdown.push("let n = 42;\n");
    let after = markdown.lines_highlighted();
    assert_eq!(after.saturating_sub(before), 1);
    assert!(second.committed.is_empty());
}

#[test]
fn long_line_not_highlighted() {
    ensure_highlighter();
    let early = "let s = \"hi\";\n";
    let long = format!("{}\n", "x".repeat(1001));
    let later = "let n = 42;\n";
    let src = format!("```rust\n{early}{long}{later}```\n");
    let lines = render(&src, 80);
    assert_eq!(span_colour(&lines, "\"hi\""), Some(Color::Green));
    let mut saw_long = false;
    for line in &lines {
        for span in &line.spans {
            if !span.content.is_empty() && span.content.chars().all(|ch| ch == 'x') {
                saw_long = true;
                assert!(line.style.patch(span.style).fg.is_none());
            }
        }
    }
    assert!(saw_long, "the long line should still be printed");
    assert_eq!(span_colour(&lines, "42"), None);
}
