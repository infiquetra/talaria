mod markdown_support;

use std::time::{Duration, Instant};

use markdown_support::{any_colour, options, render, span_colour};
use ratatui::style::Color;
use talaria_tui::markdown::highlight::{self, is_loaded};
use talaria_tui::markdown::{MarkdownStream, warm_up};

#[test]
fn highlighter_loads_lazily() {
    let plain = render("Hello\n\nworld\n", 80);
    assert!(!is_loaded());
    assert_eq!(highlight::load_starts(), 0);
    assert!(!any_colour(&plain));

    highlight::force_not_ready(true);
    let blocked = render("```rust\nlet s = \"a\";\n```\n", 80);
    assert!(!any_colour(&blocked));
    warm_up();
    assert_eq!(highlight::load_starts(), 0);
    assert!(!is_loaded());

    let mut markdown = MarkdownStream::new(options(80));
    let open = markdown.push("```rust\nlet s = \"a\";\nlet n = 1;\n");
    assert!(!any_colour(&open.tail));

    highlight::force_not_ready(false);
    warm_up();
    wait_until_loaded();
    assert_eq!(highlight::load_starts(), 1);

    let coloured = render("```rust\nlet s = \"a\";\n```\n", 80);
    assert_eq!(span_colour(&coloured, "\"a\""), Some(Color::Green));

    let flipped = markdown.push("let z = 2;\n");
    assert_eq!(span_colour(&flipped.tail, "\"a\""), Some(Color::Green));
}

fn wait_until_loaded() {
    let started = Instant::now();
    while !is_loaded() {
        if started.elapsed() >= Duration::from_secs(10) {
            panic!("highlighter was not ready within 10 seconds");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
