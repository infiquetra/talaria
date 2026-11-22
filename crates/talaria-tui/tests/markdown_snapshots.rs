mod markdown_support;

use markdown_support::{
    assert_dump, dump_row, line_columns, markdown_text, options, plain_parts, render,
};
use proptest::prelude::*;
use ratatui::style::{Modifier, Style};

#[test]
fn paragraph_wraps_on_words() {
    let lines = render(
        "The quick brown fox jumps over the lazy dog and keeps running far away.",
        30,
    );
    assert_dump(
        &lines,
        &[
            "|The quick brown fox jumps over|",
            "|the lazy dog and keeps running|",
            "|far away.|",
        ],
    );
}

#[test]
fn headings_and_inline_styles() {
    // Levels 4, 5 and 6 use the same bold italic style as level 3.
    let headings = render(
        "# One\n\n## Two\n\n### Three\n\n#### Four\n\n##### Five\n\n###### Six\n\ntext",
        80,
    );
    assert_dump(
        &headings,
        &[
            "|<b,u>One</>|",
            "||",
            "|<b>Two</>|",
            "||",
            "|<b,i>Three</>|",
            "||",
            "|<b,i>Four</>|",
            "||",
            "|<b,i>Five</>|",
            "||",
            "|<b,i>Six</>|",
            "||",
            "|text|",
        ],
    );

    // Nested strong and emphasis combine. Inline code keeps its spaces.
    let nested = render("**bold *both* word** and *em* and `a    b` code.", 80);
    assert_dump(
        &nested,
        &["|<b>bold </><b,i>both</><b> word</> and <i>em</> and <fg=cyan>a    b</> code.|"],
    );

    // A run of ordinary spaces prints as one space. Soft and hard breaks are both line breaks.
    let spaces = render("a    b", 80);
    assert_dump(&spaces, &["|a b|"]);
    let breaks = render("line one\nline two  \nline three", 80);
    assert_dump(&breaks, &["|line one|", "|line two|", "|line three|"]);
}

#[test]
fn lists_all_kinds() {
    let bullets = render("- a\n  - b\n    - c\n- d", 80);
    assert_dump(&bullets, &["|• a|", "|  ◦ b|", "|    ▪ c|", "|• d|"]);

    let ordered = render("3. aaaa bbbb\n4. y", 8);
    assert_dump(&ordered, &["|3. aaaa|", "|   bbbb|", "|4. y|"]);

    let tasks = render("- [ ] todo\n- [x] done", 80);
    assert_dump(&tasks, &["|☐ todo|", "|☑ done|"]);

    let loose = render("- a\n\n- b", 80);
    assert_dump(&loose, &["|• a|", "||", "|• b|"]);

    let tens = render("9. a\n10. b", 80);
    assert_dump(&tens, &["|9.  a|", "|10. b|"]);
}

#[test]
fn empty_list_items_keep_their_markers() {
    for (input, expected) in [
        ("- \n- next", vec!["•", "• next"]),
        ("3. \n4. next", vec!["3.", "4. next"]),
        ("- [ ]\n- [x]\n- [x] done", vec!["☐", "☑", "☑ done"]),
        (
            "- outer\n  - first\n  - \n  - nested",
            vec!["• outer", "  ◦ first", "  ◦", "  ◦ nested"],
        ),
    ] {
        let actual: Vec<String> = render(input, 80)
            .iter()
            .map(|line| line.to_string().trim_end().to_owned())
            .collect();
        assert_eq!(actual, expected, "{input:?}");
    }
}

#[test]
fn nested_quotes_with_list_code() {
    // The blank row between the outer paragraph and the nested quote is the
    // outer bar alone, with no trailing space. Nested bars merge into one dim span.
    let quotes = render("> a\n> > b", 80);
    assert_dump(&quotes, &["|<d>▎ </>a|", "|<d>▎</>|", "|<d>▎ ▎ </>b|"]);

    // A list and a code block inside a quote repeat the bar on every line.
    // The dim label merges with the bar. Hanging indent stays outside the bar.
    let mixed = render("> - a\n>   - b\n>\n> ```rust\n> let n = 1;\n> ```", 80);
    assert_dump(
        &mixed,
        &[
            "|<d>▎ </>• a|",
            "|<d>▎ </>  ◦ b|",
            "|<d>▎</>|",
            "|<d>▎ rust ─────────────────────────────────────────────────────────────────────────</>|",
            "|<d>▎ </>let n = 1;|",
        ],
    );
}

#[test]
fn code_labels_and_indented() {
    // The label is the info string's first word, lower-cased. An alias used
    // for highlighting does not change the printed label. An empty info string
    // and indented code have no label line.
    let lines = render(
        "```Rust extra\nlet a = 1;\n```\n\n```shell\necho hi\n```\n\n```\nplain\n```\n\n    indented\n",
        80,
    );
    assert_dump(
        &lines,
        &[
            "|<d>rust ───────────────────────────────────────────────────────────────────────────</>|",
            "|let a = 1;|",
            "||",
            "|<d>shell ──────────────────────────────────────────────────────────────────────────</>|",
            "|echo hi|",
            "||",
            "|plain|",
            "||",
            "|indented|",
        ],
    );
}

#[test]
fn code_label_is_a_header_with_and_without_colour() {
    // The label is followed by a rule to the block's width, so it reads as a header
    // even when the terminal shows no colour or dimming.
    let lines = render("```sh\ncd work\n```", 20);
    assert_dump(&lines, &["|<d>sh ─────────────────</>|", "|cd work|"]);

    let mut plain = options(20);
    plain.color = false;
    let lines = talaria_tui::markdown::render_markdown("```sh\ncd work\n```", &plain);
    assert_eq!(rows(&lines)[0], "sh ─────────────────");
    assert!(rows(&lines).iter().all(|row| row != "sh"));

    // A label too wide for a rule still wraps inside the width and is never cut off.
    let lines = render("```averyveryverylonglanguagename\nx\n```", 12);
    assert_eq!(rows(&lines), ["averyveryver", "ylonglanguag", "ename", "x"]);
}

#[test]
fn wide_code_line_breaks_at_a_space_in_the_last_third() {
    // Row one holds "alpha beta gamma del" in twenty columns; the last space is at
    // column 17, inside the last third, so the break goes there and no word is split.
    let lines = render("```\nalpha beta gamma delta epsilon\n```", 20);
    assert_dump(&lines, &["|alpha beta gamma |", "|<d>↪ </>delta epsilon|"]);
}

#[test]
fn wide_code_line_hard_breaks_when_no_space_is_near_the_end() {
    // The only space is early in the row, so the line is cut at the width instead.
    let lines = render("```\nab cdefghijklmnopqrstuvwxyz0123\n```", 20);
    assert_dump(&lines, &["|ab cdefghijklmnopqrs|", "|<d>↪ </>tuvwxyz0123|"]);
    // A long unbroken word is cut at the width too.
    let lines = render("```\nabcdefghijklmnopqrstuvwxyz0123\n```", 20);
    assert_dump(&lines, &["|abcdefghijklmnopqrst|", "|<d>↪ </>uvwxyz0123|"]);
}

#[test]
fn code_continuation_rows_keep_the_indent() {
    let lines = render("```\n    abcdefghijklmnopqrstuvwxyz\n```", 20);
    assert_dump(
        &lines,
        &["|    abcdefghijklmnop|", "|<d>↪ </>    qrstuvwxyz|"],
    );
}

#[test]
fn wrapped_code_line_rejoins_to_the_original_text() {
    let line = "build:sprites      src/sprites.py, tests/test_sprites.py                    worker-a / provider-b";
    for width in 8..=60u16 {
        let lines = render(&format!("```\n{line}\n```"), width);
        let body = &rows(&lines)[..];
        let mut joined = String::new();
        for (index, row) in body.iter().enumerate() {
            assert!(
                line_columns(&lines[index]) <= usize::from(width),
                "width {width}: row {index} is too wide: {row:?}"
            );
            joined.push_str(if index == 0 {
                row
            } else {
                row.strip_prefix("↪ ").expect("continuation marker")
            });
        }
        // The line has no indent, so dropping each marker gives back the original text.
        assert_eq!(joined, line, "width {width}: text was split or lost");
    }
}

#[test]
fn code_line_that_fits_is_unchanged() {
    let lines = render("```\n\tfn main() {}\n\n  x\n```", 40);
    assert_dump(&lines, &["|    fn main() {}|", "||", "|  x|"]);
}

fn rows(lines: &[ratatui::text::Line<'static>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

#[test]
fn table_fit_shrink_fallback() {
    assert_table_fit_and_fallback();
    assert_table_shrink();
    assert_table_narrow_and_ragged();
}

fn assert_table_fit_and_fallback() {
    let fit_top = [("d", "┌───────┬─────┐")];
    let fit_header = [
        ("d", "│ "),
        ("b", "Name "),
        ("d", " │ "),
        ("b", "Qty"),
        ("d", " │"),
    ];
    let fit_mid = [("d", "├───────┼─────┤")];
    let fit_body = [
        ("d", "│ "),
        ("", "apple"),
        ("d", " │ "),
        ("", "  3"),
        ("d", " │"),
    ];
    let fit_bottom = [("d", "└───────┴─────┘")];
    assert_eq!(plain_parts(&fit_header), "│ Name  │ Qty │");
    assert_eq!(plain_parts(&fit_body), "│ apple │   3 │");
    let fit = render("| Name | Qty |\n|:-----|----:|\n| apple | 3 |", 40);
    assert_dump(
        &fit,
        &[
            &dump_row(&fit_top),
            &dump_row(&fit_header),
            &dump_row(&fit_mid),
            &dump_row(&fit_body),
            &dump_row(&fit_bottom),
        ],
    );

    let fallback = render("| Name | Qty |\n|:-----|----:|\n| apple | 3 |", 12);
    assert_dump(&fallback, &["|<b>Name</>: apple|", "|<b>Qty</>: 3|"]);
}

fn assert_table_shrink() {
    let shrink_header = [
        ("d", "│ "),
        ("b", "A"),
        ("d", " │ "),
        ("b", "Long header here      "),
        ("d", " │"),
    ];
    let shrink_body = [
        ("d", "│ "),
        ("", "x"),
        ("d", " │ "),
        ("", "some long text in this"),
        ("d", " │"),
    ];
    let shrink_wrap = [
        ("d", "│ "),
        ("", " "),
        ("d", " │ "),
        ("", "cell                  "),
        ("d", " │"),
    ];
    assert_eq!(
        plain_parts(&shrink_header),
        "│ A │ Long header here       │"
    );
    assert_eq!(plain_parts(&shrink_body), "│ x │ some long text in this │");
    assert_eq!(plain_parts(&shrink_wrap), "│   │ cell                   │");
    let shrink = render(
        "| A | Long header here |\n|---|---|\n| x | some long text in this cell |",
        30,
    );
    assert_dump(
        &shrink,
        &[
            &dump_row(&[("d", "┌───┬────────────────────────┐")]),
            &dump_row(&shrink_header),
            &dump_row(&[("d", "├───┼────────────────────────┤")]),
            &dump_row(&shrink_body),
            &dump_row(&shrink_wrap),
            &dump_row(&[("d", "└───┴────────────────────────┘")]),
        ],
    );
}

fn assert_table_narrow_and_ragged() {
    let narrow = render("| a | b | c |\n|---|---|---|\n| 1 | 2 | 3 |", 8);
    assert_dump(&narrow, &["|<b>a</>: 1|", "|<b>b</>: 2|", "|<b>c</>: 3|"]);

    // A short row gains an empty cell. A long row drops the extra cell.
    // Natural widths are 4 and 1, so the box uses six and three dashes.
    let short_header = [
        ("d", "│ "),
        ("b", "A   "),
        ("d", " │ "),
        ("b", "B"),
        ("d", " │"),
    ];
    let short_row = [
        ("d", "│ "),
        ("", "only"),
        ("d", " │ "),
        ("", " "),
        ("d", " │"),
    ];
    let long_row = [
        ("d", "│ "),
        ("", "x   "),
        ("d", " │ "),
        ("", "y"),
        ("d", " │"),
    ];
    assert_eq!(plain_parts(&short_header), "│ A    │ B │");
    assert_eq!(plain_parts(&short_row), "│ only │   │");
    assert_eq!(plain_parts(&long_row), "│ x    │ y │");
    let ragged = render("| A | B |\n|---|---|\n| only |\n| x | y | z |", 80);
    assert_dump(
        &ragged,
        &[
            &dump_row(&[("d", "┌──────┬───┐")]),
            &dump_row(&short_header),
            &dump_row(&[("d", "├──────┼───┤")]),
            &dump_row(&short_row),
            &dump_row(&long_row),
            &dump_row(&[("d", "└──────┴───┘")]),
        ],
    );
}

#[test]
fn links_images_html_rules() {
    let links = render(
        "see [docs](https://example.org/x) and <https://example.org/y>",
        80,
    );
    assert_dump(
        &links,
        &[
            "|see <u,fg=blue>docs</><d> (https://example.org/x)</> and <u,fg=blue>https://example.org/y</>|",
        ],
    );
    let same = render("[https://example.org/y](https://example.org/y)", 80);
    assert_dump(&same, &["|<u,fg=blue>https://example.org/y</>|"]);
    let bare = render("https://example.org", 80);
    assert_dump(&bare, &["|https://example.org|"]);

    let image = render("![alt text](https://example.org/z)", 80);
    assert_dump(
        &image,
        &["|<i>[image: alt text]</><d> (https://example.org/z)</>|"],
    );
    let html = render("<div>raw</div>", 80);
    assert_dump(&html, &["|<div>raw</div>|"]);

    let at_40 = "─".repeat(40);
    let at_80 = "─".repeat(80);
    assert_dump(&render("---", 40), &[&format!("|<d>{at_40}</>|")]);
    assert_dump(&render("---", 120), &[&format!("|<d>{at_80}</>|")]);
}

#[test]
fn wide_chars_wrap_by_columns() {
    // Eight ideographs are sixteen columns, so width 8 cuts them into two lines of four.
    let cjk = render("漢字漢字漢字漢字", 8);
    assert_dump(&cjk, &["|漢字漢字|", "|漢字漢字|"]);

    // A joiner is a visible stand-in, so a family sequence is no longer one grapheme.
    // A seven-letter prefix fills seven columns. The stand-in then wraps at width 8.
    let family = render("abcdefg👨‍👩‍👧‍👦", 8);
    assert_dump(
        &family,
        &[
            "|abcdefg|",
            "|👨‹U+200|",
            "|D›👩‹U+2|",
            "|00D›👧‹U|",
            "|+200D›👦|",
        ],
    );
    // The variation-selector heart is still one grapheme of width 2. The same prefix
    // leaves it whole on the next line.
    let heart = render("abcdefg❤️", 8);
    assert_dump(&heart, &["|abcdefg|", "|❤️|"]);

    // The same heart without the selector is one column, so it stays on the first line.
    let narrow_heart = render("abcdefg❤", 8);
    assert_dump(&narrow_heart, &["|abcdefg❤|"]);

    // An e plus a combining acute is one column and stays on the same line.
    let combined = format!("abcdefg{}", "e\u{0301}");
    let accented = render(&combined, 8);
    assert_dump(&accented, &[&format!("|{combined}|")]);
}

#[test]
fn long_word_hard_cut() {
    let word = "a".repeat(100);
    let lines = render(&word, 30);
    assert_dump(
        &lines,
        &[
            &format!("|{}|", "a".repeat(30)),
            &format!("|{}|", "a".repeat(30)),
            &format!("|{}|", "a".repeat(30)),
            &format!("|{}|", "a".repeat(10)),
        ],
    );
}

#[test]
fn color_and_base_style() {
    let mut plain = options(80);
    plain.color = false;
    let lines = render_at("`code` and [docs](https://example.org/x)", &plain);
    assert_dump(
        &lines,
        &["|code and <u>docs</><d> (https://example.org/x)</>|"],
    );

    let mut dim = options(80);
    dim.base_style = Style::default().add_modifier(Modifier::DIM);
    let lines = render_at("# One\n\nplain", &dim);
    assert_dump(&lines, &["|<b,u,d>One</>|", "||", "|<d>plain</>|"]);
}

fn render_at(
    text: &str,
    opts: &talaria_tui::markdown::MarkdownOptions,
) -> Vec<ratatui::text::Line<'static>> {
    talaria_tui::markdown::render_markdown(text, opts)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    })]

    #[test]
    fn no_line_wider_than_width(text in markdown_text(), width in 8u16..=120u16) {
        let lines = render(&text, width);
        for line in &lines {
            let columns = line_columns(line);
            prop_assert!(
                columns <= usize::from(width),
                "line is {columns} columns at width {width}"
            );
        }
    }
}

#[test]
fn unsupported_literal_empty_table() {
    let literal = render("a ~~ b\n\n[^1]\n\n$x$\n\n\"smart\"", 80);
    assert_dump(
        &literal,
        &[
            "|a ~~ b|",
            "||",
            "|[^1]|",
            "||",
            "|$x$|",
            "||",
            "|\"smart\"|",
        ],
    );
    let struck = render("~~gone~~", 80);
    assert_dump(&struck, &["|<s>gone</>|"]);
    let headers = render("| a | b |\n|---|---|", 80);
    assert_dump(&headers, &["|<b>a</>|", "|<b>b</>|"]);
}
