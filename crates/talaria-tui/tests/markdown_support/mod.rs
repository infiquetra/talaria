//! Helpers shared by the markdown integration tests.
//!
//! Each test file is its own crate and calls only the helpers it needs.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use proptest::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use talaria_tui::markdown::highlight::is_loaded;
use talaria_tui::markdown::{MarkdownOptions, MarkdownTheme, render_markdown, warm_up};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const ESC: u8 = 0x1b;

pub fn options(width: u16) -> MarkdownOptions {
    MarkdownOptions {
        width,
        theme: MarkdownTheme::default(),
        color: true,
        base_style: Style::default(),
    }
}

pub fn render(text: &str, width: u16) -> Vec<Line<'static>> {
    render_markdown(text, &options(width))
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/markdown")
        .join(name)
}

pub fn read_fixture(name: &str) -> String {
    let path = fixture(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()))
}

pub fn frames_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames")
}

pub fn dump_row(parts: &[(&str, &str)]) -> String {
    let mut out = String::from("|");
    for (tags, text) in parts {
        if tags.is_empty() {
            out.push_str(text);
        } else {
            out.push('<');
            out.push_str(tags);
            out.push('>');
            out.push_str(text);
            out.push_str("</>");
        }
    }
    out.push('|');
    out
}

pub fn plain_parts(parts: &[(&str, &str)]) -> String {
    parts.iter().map(|(_, text)| *text).collect()
}

pub fn dump(lines: &[Line<'static>]) -> String {
    let mut rows = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            rows.push('\n');
        }
        rows.push('|');
        for (style, text) in merged(line) {
            let tag = tags(style);
            if tag.is_empty() {
                rows.push_str(&text);
            } else {
                rows.push('<');
                rows.push_str(&tag);
                rows.push('>');
                rows.push_str(&text);
                rows.push_str("</>");
            }
        }
        rows.push('|');
    }
    rows
}

pub fn assert_dump(lines: &[Line<'static>], expected: &[&str]) {
    let actual = dump(lines);
    let rows: Vec<&str> = if lines.is_empty() {
        Vec::new()
    } else {
        actual.lines().collect()
    };
    assert_eq!(rows, expected, "dump was:\n{actual}");
}

pub fn line_columns(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| {
            span.content
                .graphemes(true)
                .map(UnicodeWidthStr::width)
                .sum::<usize>()
        })
        .sum()
}

pub fn to_ansi(lines: &[Line<'static>]) -> Vec<u8> {
    let mut out = Vec::new();
    for line in lines {
        for (style, text) in merged(line) {
            write_sgr(&mut out, style);
            out.extend_from_slice(text.as_bytes());
        }
        out.extend_from_slice(b"\r\n");
    }
    out
}

pub fn ensure_highlighter() {
    warm_up();
    let started = Instant::now();
    while !is_loaded() {
        if started.elapsed() >= Duration::from_secs(10) {
            panic!("highlighter was not ready within 10 seconds");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn span_colour(lines: &[Line<'static>], needle: &str) -> Option<Color> {
    for line in lines {
        for span in &line.spans {
            if span.content.contains(needle) {
                return line.style.patch(span.style).fg;
            }
        }
    }
    None
}

pub fn any_colour(lines: &[Line<'static>]) -> bool {
    lines.iter().any(|line| {
        line.spans
            .iter()
            .any(|span| line.style.patch(span.style).fg.is_some())
    })
}

pub fn markdown_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(piece(), 1..=6).prop_map(|parts| parts.join("\n\n"))
}

pub fn markdown_deltas() -> impl Strategy<Value = Vec<String>> {
    markdown_text()
        .prop_flat_map(|text| {
            let count = text.chars().count();
            (Just(text), proptest::collection::vec(0..=count, 0..=39))
        })
        .prop_map(|(text, cuts)| split_on_chars(text, cuts))
}

fn piece() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z]{1,12}".prop_map(|word| word),
        Just("**bold**".to_string()),
        Just("*em*".to_string()),
        Just("`code`".to_string()),
        Just("[docs](https://example.org/x)".to_string()),
        Just("# Title".to_string()),
        Just("## Sub".to_string()),
        Just("- item".to_string()),
        Just("  - nested".to_string()),
        Just("1. step".to_string()),
        Just("> quote".to_string()),
        Just("> > inner".to_string()),
        Just("```rust\nlet a = 1;\n```".to_string()),
        Just("```py\nx = 1\n```".to_string()),
        Just("| A | B |\n|---|---|\n| 1 | 2 |".to_string()),
        Just("---".to_string()),
        Just("~~gone~~".to_string()),
        Just("a ~~ b".to_string()),
        Just("[^1]".to_string()),
        Just("$x$".to_string()),
        Just(hostile_piece()),
    ]
}

fn hostile_piece() -> String {
    let esc = char::from_u32(0x1b).unwrap_or('?');
    let rlo = char::from_u32(0x202e).unwrap_or('?');
    format!("a{esc}b{rlo}c")
}

fn split_on_chars(text: String, mut cuts: Vec<usize>) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    cuts.retain(|cut| *cut > 0 && *cut < chars.len());
    cuts.sort_unstable();
    cuts.dedup();
    if cuts.len() > 39 {
        cuts.truncate(39);
    }
    let mut bounds = vec![0];
    bounds.extend(cuts);
    bounds.push(chars.len());
    let mut out = Vec::new();
    for pair in bounds.windows(2) {
        if pair[0] < pair[1] {
            out.push(chars[pair[0]..pair[1]].iter().collect());
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn merged(line: &Line<'_>) -> Vec<(Style, String)> {
    let mut out: Vec<(Style, String)> = Vec::new();
    for span in &line.spans {
        if span.content.is_empty() {
            continue;
        }
        let style = line.style.patch(span.style);
        if let Some((prev, text)) = out.last_mut()
            && *prev == style
        {
            text.push_str(&span.content);
            continue;
        }
        out.push((style, span.content.to_string()));
    }
    out
}

fn tags(style: Style) -> String {
    let mut names = Vec::new();
    if flagged(style, Modifier::BOLD) {
        names.push("b");
    }
    if flagged(style, Modifier::ITALIC) {
        names.push("i");
    }
    if flagged(style, Modifier::UNDERLINED) {
        names.push("u");
    }
    if flagged(style, Modifier::CROSSED_OUT) {
        names.push("s");
    }
    if flagged(style, Modifier::DIM) {
        names.push("d");
    }
    let mut text = names.join(",");
    if let Some(colour) = style.fg {
        let name = colour_name(colour);
        if !name.is_empty() {
            if !text.is_empty() {
                text.push(',');
            }
            text.push_str("fg=");
            text.push_str(name);
        }
    }
    text
}

fn flagged(style: Style, modifier: Modifier) -> bool {
    style.add_modifier.contains(modifier) && !style.sub_modifier.contains(modifier)
}

fn colour_name(colour: Color) -> &'static str {
    match colour {
        Color::Black => "black",
        Color::Red => "red",
        Color::Green => "green",
        Color::Yellow => "yellow",
        Color::Blue => "blue",
        Color::Magenta => "magenta",
        Color::Cyan => "cyan",
        Color::White => "white",
        Color::Gray => "gray",
        Color::DarkGray => "darkgray",
        Color::Reset => "",
        _ => "other",
    }
}

fn colour_sgr(colour: Color) -> Option<u8> {
    Some(match colour {
        Color::Black => 30,
        Color::Red => 31,
        Color::Green => 32,
        Color::Yellow => 33,
        Color::Blue => 34,
        Color::Magenta => 35,
        Color::Cyan => 36,
        Color::Gray => 37,
        Color::DarkGray => 90,
        Color::LightRed => 91,
        Color::LightGreen => 92,
        Color::LightYellow => 93,
        Color::LightBlue => 94,
        Color::LightMagenta => 95,
        Color::LightCyan => 96,
        Color::White => 97,
        _ => return None,
    })
}

fn write_sgr(out: &mut Vec<u8>, style: Style) {
    let mut params: Vec<u16> = Vec::new();
    if flagged(style, Modifier::BOLD) {
        params.push(1);
    }
    if flagged(style, Modifier::DIM) {
        params.push(2);
    }
    if flagged(style, Modifier::ITALIC) {
        params.push(3);
    }
    if flagged(style, Modifier::UNDERLINED) {
        params.push(4);
    }
    if flagged(style, Modifier::CROSSED_OUT) {
        params.push(9);
    }
    if let Some(colour) = style.fg
        && let Some(code) = colour_sgr(colour)
    {
        params.push(u16::from(code));
    }
    out.push(ESC);
    out.push(b'[');
    if params.is_empty() {
        out.push(b'0');
    } else {
        for (index, param) in params.iter().enumerate() {
            if index > 0 {
                out.push(b';');
            }
            out.extend_from_slice(param.to_string().as_bytes());
        }
    }
    out.push(b'm');
}
