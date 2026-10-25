//! Pure screen helpers for the phase 2a steps, on the recorded screens.
//!
//! All fail until the stubs in `src/screen2a.rs` are replaced, except
//! `phase_one_counting_rules_unchanged`, which locks the first suite's helpers and passes.

use proptest::prelude::*;
use smoke::{
    AgentState, EditHeader, Mark, MentionRow, PaletteRow, PasteLabel, TaskView, ThemeReport, Token,
    ToolHeader, agent_state, count_token, counting_values, edit_header, exit_code, file_label,
    has_debris, has_diff_rows, mention_list, pager_state, palette_state, paste_labels,
    prompt_after, rows_after, shell_cell, side_cell, task_records, task_view, theme_report,
    tool_header,
};

fn screens(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/screens/phase2a")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {name}: {e}"))
}

#[test]
fn tool_header_found_with_mark_and_exit() {
    let text = screens("tool_shell_failed.txt");
    assert_eq!(
        tool_header(&text, "ls /talaria-no-such-dir"),
        Some(ToolHeader {
            line_index: 9,
            mark: Mark::Failure,
            exit: Some(2),
        })
    );
    assert!(
        rows_after(&text, 9, 8, "No such file or directory"),
        "the output row sits under the header"
    );
    assert!(
        !rows_after(&text, 9, 8, "No such file or somewhere-else"),
        "a missing row is not found"
    );
}

#[test]
fn tool_header_for_another_command_is_refused() {
    let text = screens("tool_shell_failed.txt");
    assert_eq!(
        tool_header(&text, "ls /other"),
        None,
        "a header for another command is refused"
    );
    assert_eq!(
        tool_header(
            "agent: the earlier ⏺ Shell(ls /talaria-no-such-dir) run failed\nagent: done",
            "ls /talaria-no-such-dir"
        ),
        None,
        "a reply that merely quotes the header without the mark is refused"
    );
}

#[test]
fn edit_header_diff_rows_and_no_debris() {
    let text = screens("tool_edit_diff.txt");
    assert_eq!(
        edit_header(&text, "notes.txt"),
        Some(EditHeader {
            line_index: 9,
            stat_ok: true,
        })
    );
    assert!(has_diff_rows(&text, 9), "the numbered rows are found");
    assert!(!has_debris(&text), "the recorded diff holds no debris");
    assert!(
        has_debris("    2 - beta ┊ debris"),
        "the header bar is debris"
    );
    assert!(has_debris("output \u{1b}[0m plain"), "an escape is debris");
    assert!(
        has_debris("coloured [38;2;1;2;3m text"),
        "a colour sequence is debris"
    );
}

#[test]
fn palette_rows_footer_and_group() {
    let open = palette_state(&screens("palette_open.txt"));
    assert!(open.footer, "the filtered list shows its footer");
    // Hermes's own commands that fuzzy-match `/vers`, in the palette's order; `/version` is
    // listed twice by the real binary.
    assert_eq!(
        open.rows,
        vec![
            PaletteRow {
                name: "/version".to_owned(),
                tag: Some("hermes".to_owned()),
            },
            PaletteRow {
                name: "/update".to_owned(),
                tag: Some("hermes".to_owned()),
            },
            PaletteRow {
                name: "/version".to_owned(),
                tag: Some("hermes".to_owned()),
            },
            PaletteRow {
                name: "/redraw".to_owned(),
                tag: Some("hermes".to_owned()),
            },
            PaletteRow {
                name: "/history".to_owned(),
                tag: Some("hermes".to_owned()),
            },
            PaletteRow {
                name: "/save".to_owned(),
                tag: Some("hermes".to_owned()),
            },
            PaletteRow {
                name: "/refine".to_owned(),
                tag: Some("hermes".to_owned()),
            },
            PaletteRow {
                name: "/reload-mcp".to_owned(),
                tag: Some("hermes".to_owned()),
            },
        ]
    );
    let browse = palette_state(&screens("palette_browse.txt"));
    assert!(browse.footer, "the browsed list shows its footer");
    assert_eq!(
        browse.rows,
        // The window holds eight rows: `/skin` and `/theme` sit below it.
        vec![
            PaletteRow {
                name: "/quit".to_owned(),
                tag: Some("talaria".to_owned()),
            },
            PaletteRow {
                name: "/tasks".to_owned(),
                tag: Some("talaria".to_owned()),
            },
            PaletteRow {
                name: "/model".to_owned(),
                tag: Some("talaria".to_owned()),
            },
            PaletteRow {
                name: "/effort".to_owned(),
                tag: Some("talaria".to_owned()),
            },
            PaletteRow {
                name: "/context".to_owned(),
                tag: Some("talaria".to_owned()),
            },
            PaletteRow {
                name: "/compact".to_owned(),
                tag: Some("talaria".to_owned()),
            },
            PaletteRow {
                name: "/usage".to_owned(),
                tag: Some("talaria".to_owned()),
            },
            PaletteRow {
                name: "/btw".to_owned(),
                tag: Some("talaria".to_owned()),
            },
        ],
        "the Talaria group rows come first, tagged, in order"
    );
}

#[test]
fn palette_footer_absent_when_closed() {
    let closed = palette_state("> ");
    assert!(!closed.footer, "a closed palette has no footer");
    assert!(closed.rows.is_empty(), "a closed palette has no rows");
    let quoted = palette_state("agent: press Tab complete to finish\n> ");
    assert!(
        !quoted.footer,
        "Tab complete in a reply is not the footer: it lacks the separator"
    );
    let mut far = String::from("↑↓ choose · Tab complete · Enter run · Esc close  50 of 50\n");
    for _ in 0..30 {
        far.push_str("filler\n");
    }
    assert!(
        !palette_state(&far).footer,
        "the footer must sit within the last 25 lines"
    );
}

#[test]
fn mention_row_and_inserted_text() {
    assert_eq!(
        mention_list(&screens("mention_list.txt")),
        Some(MentionRow {
            file: "calc.py".to_owned(),
            folder: "src".to_owned(),
        })
    );
    assert_eq!(
        mention_list("> Summarize @calc\ncalculator.py  src"),
        None,
        "a list without the row finds nothing"
    );
    assert!(
        "> Summarize @file:src/calc.py ".contains("@file:src/calc.py"),
        "Tab inserts the file reference into the composer"
    );
}

#[test]
fn paste_label_with_count_and_number() {
    assert_eq!(
        paste_labels(&screens("paste_label.txt")),
        vec![PasteLabel {
            number: 1,
            lines: 120,
        }]
    );
    assert_eq!(
        paste_labels("[Pasted text #1 +120 lines] then [Pasted text #2 +45 lines]"),
        vec![
            PasteLabel {
                number: 1,
                lines: 120,
            },
            PasteLabel {
                number: 2,
                lines: 45,
            },
        ],
        "labels come in order"
    );
    assert!(
        paste_labels("> plain composer").is_empty(),
        "plain text holds no label"
    );
}

#[test]
fn file_label_with_name() {
    let text = screens("file_label.txt");
    assert!(
        file_label(&text, "notes-0a1b2c3d.txt"),
        "the dropped file label names the file"
    );
    assert!(
        !file_label(&text, "notes-other.txt"),
        "another name is not the label"
    );
}

#[test]
fn shell_cell_header_and_output_row() {
    let cell =
        shell_cell(&screens("shell_cells.txt"), "echo heron-4243").expect("the echo cell is found");
    assert_eq!(cell.header_index, 0);
    assert!(
        cell.output_has.iter().any(|row| row == "heron-4243"),
        "the output rows hold the token: {:?}",
        cell.output_has
    );
}

#[test]
fn shell_refusal_prefix() {
    let text = screens("shell_cells.txt");
    let cell = shell_cell(&text, "echo heron-4243").expect("the echo cell is found");
    assert!(
        rows_after(
            &text,
            cell.header_index,
            8,
            "Hermes refused to run this: it looks dangerous"
        ),
        "the refusal prefix is checked, not the whole sentence"
    );
}

#[test]
fn side_cell_header_footer_and_answer_region() {
    let cell = side_cell(
        &screens("side_answer.txt"),
        "which two words did I ask you to join in my first message?",
    )
    .expect("the side cell is found");
    assert_eq!(cell.header_index, 0);
    assert_eq!(cell.footer_index, 2);
    assert!(!cell.failed, "an answer is not a failure");
    assert!(!cell.empty, "an answer is not empty");
    assert!(
        cell.answer.contains("heron"),
        "the answer region holds the word: {}",
        cell.answer
    );
}

#[test]
fn side_cell_failure_and_empty_answer() {
    let failed = side_cell(
        &screens("side_failed.txt"),
        "which two words did I ask you to join in my first message?",
    )
    .expect("the failed cell is found");
    assert!(failed.failed, "the failure prefix marks the cell");
    assert!(!failed.empty, "a failure is not empty");
    assert!(
        failed.answer.contains("not found"),
        "the answer keeps the failure rest: {}",
        failed.answer
    );
    let empty = side_cell(
        "btw · the time?\n(no answer)\nside answer · not part of the conversation",
        "the time?",
    )
    .expect("the empty cell is found");
    assert!(empty.empty, "the marker marks the empty answer");
    assert!(!empty.failed, "an empty answer is not a failure");
}

#[test]
fn pager_header_following_and_k_of_n() {
    let open = pager_state(&screens("pager_open.txt"));
    assert!(open.open, "the header opens the pager");
    assert!(open.following, "the footer says following");
    assert_eq!(open.search, None, "no search is confirmed");
    assert!(!open.no_match, "no search means no mismatch");
    let searched = pager_state(&screens("pager_search.txt"));
    assert!(searched.open, "the search keeps the pager open");
    assert_eq!(
        searched.search,
        Some((3, 3, "heron-4242".to_owned())),
        "the footer confirms match, count and query"
    );
    assert!(!searched.no_match, "a match is not a mismatch");
    let missed = pager_state(&screens("pager_nomatch.txt"));
    assert!(missed.open, "a miss keeps the pager open");
    assert!(missed.no_match, "the footer reports no match");
}

#[test]
fn inline_view_has_no_transcript_header() {
    assert!(
        !pager_state("> draft-one\nagent: reply").open,
        "the inline view is not the pager"
    );
    assert!(
        !pager_state("agent: open the Transcript · view to search\n> ").open,
        "Transcript quoted mid-line must still start the line"
    );
}

#[test]
fn theme_report_and_usage_lines() {
    assert_eq!(
        theme_report(&screens("theme_report.txt")),
        ThemeReport {
            present: [true, true, true, true, true],
        }
    );
    assert_eq!(
        theme_report("/theme\nTheme\nSkin: default\nBackground: dark (set with /theme)"),
        ThemeReport {
            present: [true, true, true, false, false],
        },
        "missing lines are reported per key"
    );
    let usage = screens("usage_lines.txt");
    assert!(
        usage.contains("Usage: /skin [name]."),
        "the skin usage line is printed"
    );
    assert!(
        usage.contains("Usage: /theme [light|dark|auto]"),
        "the theme usage line is printed"
    );
    assert!(
        !usage.contains("Hermes refused"),
        "a bad argument never reaches Hermes"
    );
}

#[test]
fn task_panel_collapsed_expanded_and_record() {
    assert_eq!(
        task_view(&screens("tasks_expanded.txt")),
        TaskView::Expanded {
            active_row: Some("[>] run sleep 30".to_owned()),
        }
    );
    assert_eq!(
        task_view(&screens("tasks_collapsed.txt")),
        TaskView::Collapsed {
            active_row: Some("▸ Tasks 1/3 · run sleep 30 · Ctrl+T to expand".to_owned()),
        },
        "the collapsed header names the step in progress"
    );
    assert_eq!(task_view("> "), TaskView::Absent, "no panel, no view");
    assert_eq!(
        task_records(&screens("tasks_record.txt")),
        vec![(3, 3)],
        "the record carries completed and total"
    );
}

#[test]
fn record_count_exactly_one() {
    assert_eq!(
        task_records("Tasks 3/3 · done").len(),
        1,
        "one record passes the count rule"
    );
    assert_eq!(
        task_records("Tasks 3/3 · done\nTasks 2/3 · 1 not finished").len(),
        2,
        "two records are both found, so the step can refuse them"
    );
    assert!(
        task_records("no record here").is_empty(),
        "plain text holds no record"
    );
}

#[test]
fn pane_get_state_words() {
    assert_eq!(
        agent_state(&screens("pane_get.txt")),
        Some(AgentState::Idle),
        "the idle report parses"
    );
    assert_eq!(
        agent_state("talaria\nworking"),
        Some(AgentState::Working),
        "the working report parses"
    );
    assert_eq!(
        agent_state("talaria\nblocked\napproval: rm -rf .pytest_cache"),
        Some(AgentState::Blocked),
        "the blocked report parses"
    );
    assert_eq!(
        agent_state("nothing about any agent"),
        None,
        "no talaria label, no state"
    );
    assert_eq!(
        agent_state("talaria"),
        None,
        "the label without a state word is not a state"
    );
}

#[test]
fn phase_one_counting_rules_unchanged() {
    assert_eq!(
        counting_values(
            "you: Count from 1 to 2000, one number per line, and do not stop early.\n1\n2\n3",
            "Count from 1 to 2000"
        ),
        vec![1, 2, 3],
        "counting still reads bare numbers after the prompt"
    );
    assert_eq!(
        exit_code("you: /quit\nSession detached.\nSMOKE_EXIT:7\n$ "),
        Some(7),
        "the exit marker still parses"
    );
    assert_eq!(exit_code("no marker here"), None);
    assert_eq!(count_token("maple-1 and MAPLE-1", "maple-1"), 2);
}

#[test]
fn escape_and_box_characters_ignored() {
    let text = "⏺ Shell(ls /x)  ✗ exit 2 · 0.0s\n│ output ── box\nagent: hi \u{1b}[0m";
    assert_eq!(
        tool_header(text, "ls /x").map(|header| header.exit),
        Some(Some(2)),
        "box characters and escapes around the rows change nothing"
    );
    let palette = palette_state("│ /version  talaria │\n│ ↑↓ choose · Tab complete · Enter run │");
    assert!(palette.footer, "the footer parses through box characters");
    assert_eq!(
        mention_list("│ calc.py  src │"),
        Some(MentionRow {
            file: "calc.py".to_owned(),
            folder: "src".to_owned(),
        }),
        "the mention row parses through box characters"
    );
}

proptest! {
    #[test]
    fn property_labels_never_match_plain_text(text in "[^\\[]*") {
        prop_assert!(
            paste_labels(&text).is_empty(),
            "text without [ never yields a label"
        );
    }
}

#[test]
fn prompt_after_builds_the_sentence_and_keeps_the_token_out() {
    let token = Token {
        word: "maple",
        number: 1234,
    };
    assert_eq!(
        prompt_after(&token, "Run the shell command ls /x"),
        "Run the shell command ls /x, then join these two words with a hyphen and reply \
         with only the result: maple and 1234"
    );
    assert!(
        !prompt_after(&token, "Summarize").contains("maple-1234"),
        "the joined token is not in the prompt"
    );
}
