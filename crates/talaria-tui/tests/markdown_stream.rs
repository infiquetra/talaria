mod markdown_support;

use markdown_support::{assert_dump, dump, markdown_deltas, options, read_fixture};
use proptest::prelude::*;
use ratatui::style::Style;
use talaria_tui::markdown::{MarkdownOptions, MarkdownStream, StreamUpdate, render_markdown};

fn stream(width: u16) -> MarkdownStream {
    MarkdownStream::new(options(width))
}

#[test]
fn empty_push_returns_empty_update() {
    let mut markdown = stream(80);
    let update = markdown.push("");
    assert!(update.committed.is_empty());
    assert!(update.tail.is_empty());
    assert!(update.is_empty());
}

#[test]
fn lone_block_not_committed() {
    let mut markdown = stream(80);
    let update = markdown.push("Hello world\n");
    assert!(update.committed.is_empty());
    assert_dump(&update.tail, &["|Hello world|"]);
}

#[test]
fn second_block_commits_first() {
    let mut open = stream(80);
    let update = open.push("Hello world\n\nNext");
    assert!(update.committed.is_empty());
    assert_dump(&update.tail, &["|Hello world|", "||", "|Next|"]);

    let mut closed = stream(80);
    let update = closed.push("Hello world\n\nNext\n");
    assert_dump(&update.committed, &["|Hello world|"]);
    assert_dump(&update.tail, &["||", "|Next|"]);
}

#[test]
fn half_typed_line_closes_nothing() {
    let mut markdown = stream(80);
    let first = markdown.push("Hello\n***");
    assert!(first.committed.is_empty());
    let second = markdown.push("bold***\n");
    assert!(second.committed.is_empty(), "a lone paragraph stays open");
    let whole = render_markdown("Hello\n***bold***\n", &options(80));
    assert_eq!(dump(&second.tail), dump(&whole));
    assert_dump(&whole, &["|Hello|", "|<b,i>bold</>|"]);
}

#[test]
fn closed_fence_commits_open_stays_tail() {
    let mut markdown = stream(80);
    let open = markdown.push("```rust\nlet a = 1;\n");
    assert!(open.committed.is_empty());
    assert_dump(&open.tail, &["|<d>rust</>|", "|let a = 1;|"]);
    let closed = markdown.push("```\n");
    assert_dump(&closed.committed, &["|<d>rust</>|", "|let a = 1;|"]);
    assert!(closed.tail.is_empty());

    let mut partial = stream(80);
    let update = partial.push("```rust\nlet a = 1;\n`\n");
    assert!(update.committed.is_empty());
    assert_dump(&update.tail, &["|<d>rust</>|", "|let a = 1;|", "|`|"]);
}

#[test]
fn loose_list_is_one_block() {
    let mut markdown = stream(80);
    let update = markdown.push("- a\n\n- b\n\ntext\n");
    assert_dump(&update.committed, &["|• a|", "||", "|• b|"]);
    assert_dump(&update.tail, &["||", "|text|"]);

    let mut open = stream(80);
    let update = open.push("- a\n\n- b\n\n  more\n");
    assert!(update.committed.is_empty());
    assert_dump(&update.tail, &["|• a|", "||", "|• b|", "|  more|"]);
}

#[test]
fn table_stays_open_and_relayouts() {
    let mut markdown = stream(80);
    let open = markdown.push("| A | B |\n|---|---|\n| 1 | 2 |\n\n");
    assert!(open.committed.is_empty());
    let closed = markdown.push("para\n");
    assert_dump(
        &closed.committed,
        &[
            "|<d>┌───┬───┐</>|",
            "|<d>│ </><b>A</><d> │ </><b>B</><d> │</>|",
            "|<d>├───┼───┤</>|",
            "|<d>│ </>1<d> │ </>2<d> │</>|",
            "|<d>└───┴───┘</>|",
        ],
    );
    assert_dump(&closed.tail, &["||", "|para|"]);
}

#[test]
fn crlf_split_across_deltas() {
    let cr = char::from_u32(0x240d).unwrap();
    let mut markdown = stream(80);
    let held = markdown.push("a\r");
    assert!(held.committed.is_empty());
    assert_dump(&held.tail, &["|a|"]);
    let joined = markdown.push("\nb\n");
    assert!(joined.committed.is_empty());
    assert_dump(&joined.tail, &["|a|", "|b|"]);

    let mut finished = stream(80);
    finished.push("a\r");
    let update = finished.finish();
    assert!(update.tail.is_empty());
    assert_dump(&update.committed, &[&format!("|a{cr}|")]);
}

#[test]
fn set_width_rewraps_tail_only() {
    let sentence = "The quick brown fox jumps over the lazy dog and keeps running far away.";
    let mut markdown = stream(80);
    let update = markdown.push(sentence);
    assert!(update.committed.is_empty());
    let narrow = markdown.set_width(30);
    assert!(narrow.committed.is_empty());
    assert_dump(
        &narrow.tail,
        &[
            "|The quick brown fox jumps over|",
            "|the lazy dog and keeps running|",
            "|far away.|",
        ],
    );
    let raised = markdown.set_width(3);
    assert!(raised.committed.is_empty());
    assert_dump(
        &raised.tail,
        &[
            "|The|",
            "|quick|",
            "|brown|",
            "|fox|",
            "|jumps|",
            "|over the|",
            "|lazy dog|",
            "|and|",
            "|keeps|",
            "|running|",
            "|far|",
            "|away.|",
        ],
    );

    let mut empty = stream(80);
    let update = empty.set_width(40);
    assert!(update.is_empty());
}

#[test]
fn finish_flushes_and_stream_reusable() {
    let mut markdown = stream(80);
    markdown.push("Hello");
    let first = markdown.finish();
    assert_dump(&first.committed, &["|Hello|"]);
    assert!(first.tail.is_empty());
    let second = markdown.finish();
    assert!(second.is_empty());
    markdown.push("Next\n");
    let third = markdown.finish();
    assert_dump(&third.committed, &["|Next|"]);
    assert!(third.tail.is_empty());
}

#[test]
fn no_leading_or_trailing_blank() {
    let text = "Hello world\n\nNext\n";
    let lines = render_markdown(text, &options(80));
    assert_dump(&lines, &["|Hello world|", "||", "|Next|"]);

    let mut markdown = stream(80);
    let mut got = Vec::new();
    let update = markdown.push(text);
    got.extend(update.committed);
    let done = markdown.finish();
    assert!(done.tail.is_empty());
    got.extend(done.committed);
    assert_eq!(dump(&got), dump(&lines));
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    })]

    #[test]
    fn streamed_equals_whole(deltas in markdown_deltas()) {
        let whole = deltas.concat();
        prop_assume!(!whole.contains("]:"));
        let opts = options(40);
        let mut markdown = MarkdownStream::new(opts.clone());
        let mut got = Vec::new();
        for delta in &deltas {
            got.extend(markdown.push(delta).committed);
        }
        let done = markdown.finish();
        prop_assert!(done.tail.is_empty());
        got.extend(done.committed);
        let expected = render_markdown(&whole, &opts);
        prop_assert_eq!(dump(&got), dump(&expected));
    }

    #[test]
    fn finish_leaves_no_tail(deltas in markdown_deltas()) {
        let mut markdown = MarkdownStream::new(options(32));
        for delta in &deltas {
            markdown.push(delta);
        }
        let first = markdown.finish();
        prop_assert!(first.tail.is_empty());
        let second = markdown.finish();
        prop_assert!(second.is_empty());
    }
}

#[test]
fn one_char_deltas_match_whole() {
    let text = read_fixture("sample.md");
    assert!(
        !text.contains("]:"),
        "the sample fixture must stream equal to a whole render"
    );
    let opts = options(80);
    let mut markdown = MarkdownStream::new(opts.clone());
    let mut got = Vec::new();
    for ch in text.chars() {
        got.extend(markdown.push(&ch.to_string()).committed);
    }
    let done = markdown.finish();
    assert!(done.tail.is_empty());
    got.extend(done.committed);
    assert_eq!(dump(&got), dump(&render_markdown(&text, &opts)));
}

#[test]
fn defaults_and_helpers() {
    let opts = MarkdownOptions::default();
    assert_eq!(opts.width, 80);
    assert!(opts.color);
    assert_eq!(opts.base_style, Style::default());
    let update = StreamUpdate::default();
    assert!(update.committed.is_empty());
    assert!(update.tail.is_empty());
    let _ = update.is_empty();
    let markdown = MarkdownStream::new(opts);
    assert_eq!(markdown.pending_bytes(), 0);
}

#[test]
fn nested_fence_commits_with_container() {
    // A fence inside a list or quote is not a top-level block, so its closing
    // line does not commit it. The container commits when the next top-level
    // block's line is complete. Hanging indent is two spaces. Quote bars are
    // dim and merge with a dim code label.
    let mut listed = stream(80);
    let open = listed.push("- item\n\n  ```rust\n  let a = 1;\n  ```\n");
    assert!(open.committed.is_empty());
    let closed = listed.push("\ntext\n");
    assert_dump(
        &closed.committed,
        &["|• item|", "||", "|  <d>rust</>|", "|  let a = 1;|"],
    );
    assert_dump(&closed.tail, &["||", "|text|"]);

    let mut quoted = stream(80);
    let open = quoted.push("> ```rust\n> let a = 1;\n> ```\n");
    assert!(open.committed.is_empty());
    let closed = quoted.push("\ntext\n");
    assert_dump(
        &closed.committed,
        &["|<d>▎ rust</>|", "|<d>▎ </>let a = 1;|"],
    );
    assert_dump(&closed.tail, &["||", "|text|"]);
}
