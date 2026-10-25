//! The checklist panel and block, row for row.

mod support;

use proptest::prelude::*;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use support::{ByteSink, Vt, probe};
use talaria_core::checklist::{Checklist, ChecklistCommit, CommitKind, TodoItem, TodoStatus};
use talaria_tui::checklist::{ChecklistSection, ChecklistStyle, committed_block_lines};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};
use unicode_width::UnicodeWidthStr;

fn placeholder() -> ChecklistStyle {
    let plain = Style::default();
    ChecklistStyle {
        header: plain,
        hint: plain,
        in_progress: plain,
        pending: plain,
        completed: plain,
        cancelled: plain,
        glyph_in_progress: plain,
        glyph_completed: plain,
    }
}

fn item(id: &str, content: &str, status: TodoStatus) -> TodoItem {
    TodoItem {
        id: id.to_string(),
        content: content.to_string(),
        status,
        parent: None,
    }
}

fn list_41() -> Checklist {
    Checklist {
        revision: 2,
        items: vec![
            item("1", "Read the failing test", TodoStatus::Completed),
            item("2", "Fix average()", TodoStatus::InProgress),
            item("3", "Run the test suite", TodoStatus::Pending),
        ],
        skipped: 0,
    }
}

fn text(line: &Line<'static>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn texts(lines: &[Line<'static>]) -> Vec<String> {
    lines.iter().map(text).collect()
}

#[test]
fn expanded_panel_rows() {
    let list = list_41();
    let style = placeholder();
    let section = ChecklistSection::new(&list, true, 8, Some("Ctrl+T".to_string()), &style);
    assert_eq!(
        texts(&section.lines(60)),
        vec![
            "▾ Tasks 1/3 · Ctrl+T to collapse",
            "  [x] Read the failing test",
            "  [>] Fix average()",
            "  [ ] Run the test suite",
        ]
    );
}

#[test]
fn collapsed_panel_row_names_the_focus() {
    let list = list_41();
    let style = placeholder();
    for (width, want) in [
        (60, "▸ Tasks 1/3 · Fix average() · Ctrl+T to expand"),
        (30, "▸ Tasks 1/3 · Fix average()"),
        (20, "▸ Tasks 1/3 · Fix a…"),
    ] {
        let section = ChecklistSection::new(&list, false, 8, Some("Ctrl+T".to_string()), &style);
        assert_eq!(texts(&section.lines(width)), vec![want], "width {width}");
    }
    let mut long = list_41();
    long.items[1].content = "Fix average() and rerun the whole suite".to_string();
    let section = ChecklistSection::new(&long, false, 8, Some("Ctrl+T".to_string()), &style);
    assert_eq!(
        texts(&section.lines(30)),
        vec!["▸ Tasks 1/3 · Fix average() a…"]
    );
}

#[test]
fn no_hint_when_unbound() {
    let list = list_41();
    let style = placeholder();
    let section = ChecklistSection::new(&list, true, 8, None, &style);
    assert_eq!(texts(&section.lines(60))[0], "▾ Tasks 1/3");
}

#[test]
fn long_list_window_around_focus() {
    let items: Vec<TodoItem> = (1..=20)
        .map(|number| {
            let status = if number <= 12 {
                TodoStatus::Completed
            } else if number == 13 {
                TodoStatus::InProgress
            } else {
                TodoStatus::Pending
            };
            item(&format!("s{number}"), &format!("step {number:02}"), status)
        })
        .collect();
    let list = Checklist {
        revision: 1,
        items,
        skipped: 0,
    };
    let style = placeholder();
    let section = ChecklistSection::new(&list, true, 8, Some("Ctrl+T".to_string()), &style);
    assert_eq!(
        texts(&section.lines(60)),
        vec![
            "▾ Tasks 12/20 · Ctrl+T to collapse",
            "  [x] step 12",
            "  [>] step 13",
            "  [ ] step 14",
            "  [ ] step 15",
            "  [ ] step 16",
            "  [ ] step 17",
            "  … +14 more",
        ]
    );
}

#[test]
fn small_row_budgets() {
    let list = list_41();
    let style = placeholder();
    let section = ChecklistSection::new(&list, true, 1, Some("Ctrl+T".to_string()), &style);
    assert_eq!(
        texts(&section.lines(60)),
        vec!["▸ Tasks 1/3 · Fix average() · Ctrl+T to expand"]
    );
    let section = ChecklistSection::new(&list, true, 3, Some("Ctrl+T".to_string()), &style);
    assert_eq!(
        texts(&section.lines(60)),
        vec![
            "▾ Tasks 1/3 · Ctrl+T to collapse",
            "  [>] Fix average()",
            "  … +2 more",
        ]
    );
}

fn content_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just('a'),
            Just('Z'),
            Just(' '),
            Just('漢'),
            Just('字'),
            Just('\u{301}'),
            Just('👍'),
            Just('.'),
        ],
        0..300,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

fn wide_item() -> impl Strategy<Value = TodoItem> {
    (content_text(), 0..4usize).prop_map(|(content, status)| {
        let states = [
            TodoStatus::Pending,
            TodoStatus::InProgress,
            TodoStatus::Completed,
            TodoStatus::Cancelled,
        ];
        TodoItem {
            id: "x".to_string(),
            content,
            status: states[status],
            parent: None,
        }
    })
}

#[test]
fn rows_fit_every_width() {
    let content: String = ("a".repeat(49) + "\n").repeat(80);
    assert_eq!(content.chars().count(), 4_000);
    let list = Checklist {
        revision: 1,
        items: vec![item("1", &content, TodoStatus::Pending)],
        skipped: 0,
    };
    let style = placeholder();
    let section = ChecklistSection::new(&list, true, 8, Some("Ctrl+T".to_string()), &style);
    let rows = texts(&section.lines(80));
    assert_eq!(rows.len(), 2);
    assert!(rows[1].width() <= 80);
    assert!(rows[1].ends_with('…'));
    let list = list_41();
    let section = ChecklistSection::new(&list, true, 8, Some("Ctrl+T".to_string()), &style);
    assert_eq!(texts(&section.lines(8)).len(), 1);
    let commit = ChecklistCommit {
        checklist: list_41(),
        kind: CommitKind::TurnEnded,
    };
    assert_eq!(texts(&committed_block_lines(&commit, 8, &style)).len(), 4);
    let list = list_41();
    let section = ChecklistSection::new(&list, true, 8, Some("Ctrl+T".to_string()), &style);
    assert!(section.lines(0).is_empty());
    let commit = ChecklistCommit {
        checklist: list,
        kind: CommitKind::TurnEnded,
    };
    assert!(committed_block_lines(&commit, 0, &style).is_empty());

    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(
            &(
                1u16..200u16,
                prop::collection::vec(wide_item(), 0..60),
                0..4usize,
            ),
            |(width, items, skipped)| {
                let list = Checklist {
                    revision: 1,
                    items,
                    skipped,
                };
                let style = placeholder();
                let section =
                    ChecklistSection::new(&list, true, 8, Some("Ctrl+T".to_string()), &style);
                for row in texts(&section.lines(width)) {
                    prop_assert!(row.width() <= width as usize, "{row:?} wider than {width}");
                }
                let commit = ChecklistCommit {
                    checklist: list,
                    kind: CommitKind::TurnEnded,
                };
                for row in texts(&committed_block_lines(&commit, width, &style)) {
                    prop_assert!(row.width() <= width as usize, "{row:?} wider than {width}");
                }
                Ok(())
            },
        )
        .expect("every row fits its width");
}

#[test]
fn indentation_capped_at_depth_four() {
    let items: Vec<TodoItem> = (0..7)
        .map(|depth| TodoItem {
            id: format!("n{depth}"),
            content: format!("n{depth}"),
            status: TodoStatus::Pending,
            parent: if depth == 0 {
                None
            } else {
                Some(format!("n{}", depth - 1))
            },
        })
        .collect();
    let list = Checklist {
        revision: 1,
        items,
        skipped: 0,
    };
    let style = placeholder();
    let section = ChecklistSection::new(&list, true, 8, Some("Ctrl+T".to_string()), &style);
    let rows = texts(&section.lines(80));
    let deepest = rows
        .iter()
        .find(|row| row.contains("n6"))
        .expect("seven rows");
    assert!(deepest.starts_with("          [ ]"), "{deepest:?}");
}

#[test]
fn untrusted_content_is_literal() {
    let list = Checklist {
        revision: 1,
        items: vec![item(
            "1",
            "a\u{1b}[2Jb\u{1b}]0;pwn\u{7}c\u{202E}d\ne\tf",
            TodoStatus::Pending,
        )],
        skipped: 0,
    };
    let style = placeholder();
    let section = ChecklistSection::new(&list, true, 8, Some("Ctrl+T".to_string()), &style);
    let rows = section.lines(80);
    assert_eq!(texts(&rows)[1], "  [ ] a␛[2Jb␛]0;pwn␇c‹U+202E›d e f");
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        probe(false, false, Some((0, 1))),
        RestoreLink::detached(),
    )
    .expect("terminal");
    let mut vt = Vt::new(80, 24);
    vt.feed(b"KEEP");
    term.commit_lines(rows).expect("commit");
    let bytes = vt.absorb(&sink);
    assert!(
        !bytes.windows(4).any(|window| window == b"\x1b[2J"),
        "the screen must not clear"
    );
    assert!(
        !bytes.windows(2).any(|window| window == b"\x1b]"),
        "no title sequence may reach the terminal"
    );
    let screen = vt.rows().join("\n");
    assert!(screen.contains("KEEP"), "the old row stays: {screen}");
    assert!(screen.contains("a␛[2Jb␛]0;pwn␇c‹U+202E›d e f"), "{screen}");
}

#[test]
fn committed_block_headers() {
    let style = placeholder();
    let block = |items: Vec<TodoItem>, kind: CommitKind| {
        let list = Checklist {
            revision: 1,
            items,
            skipped: 0,
        };
        committed_block_lines(
            &ChecklistCommit {
                checklist: list,
                kind,
            },
            80,
            &style,
        )
    };
    let done = block(
        vec![
            item("1", "a", TodoStatus::Completed),
            item("2", "b", TodoStatus::Completed),
            item("3", "c", TodoStatus::Completed),
        ],
        CommitKind::TurnEnded,
    );
    assert_eq!(texts(&done)[0], "Tasks 3/3 · done");
    let active = block(
        vec![
            item("1", "a", TodoStatus::Completed),
            item("2", "b", TodoStatus::InProgress),
            item("3", "c", TodoStatus::Pending),
        ],
        CommitKind::TurnEnded,
    );
    assert_eq!(texts(&active)[0], "Tasks 1/3 · 2 not finished");
    let cancelled = block(
        vec![
            item("1", "a", TodoStatus::Completed),
            item("2", "b", TodoStatus::Cancelled),
            item("3", "c", TodoStatus::Cancelled),
        ],
        CommitKind::TurnEnded,
    );
    assert_eq!(texts(&cancelled)[0], "Tasks 1/3 · done, 2 cancelled");
    let mixed = block(
        vec![
            item("1", "a", TodoStatus::Completed),
            item("2", "b", TodoStatus::InProgress),
            item("3", "c", TodoStatus::Pending),
            item("4", "d", TodoStatus::Cancelled),
        ],
        CommitKind::TurnEnded,
    );
    assert_eq!(texts(&mixed)[0], "Tasks 1/4 · 2 not finished, 1 cancelled");
    let resumed = block(
        vec![
            item("1", "a", TodoStatus::Completed),
            item("2", "b", TodoStatus::InProgress),
            item("3", "c", TodoStatus::Pending),
        ],
        CommitKind::Resumed,
    );
    assert_eq!(
        texts(&resumed)[0],
        "Tasks 1/3 · 2 not finished (from before this resume)"
    );
}

#[test]
fn committed_block_caps_and_skipped_line() {
    let style = placeholder();
    let items: Vec<TodoItem> = (1..=256)
        .map(|number| {
            item(
                &format!("i{number}"),
                &format!("item {number:03}"),
                TodoStatus::Pending,
            )
        })
        .collect();
    let list = Checklist {
        revision: 1,
        items,
        skipped: 0,
    };
    let rows = texts(&committed_block_lines(
        &ChecklistCommit {
            checklist: list,
            kind: CommitKind::TurnEnded,
        },
        80,
        &style,
    ));
    assert_eq!(rows.len(), 52);
    assert_eq!(rows[1], "  [ ] item 001");
    assert_eq!(rows[50], "  [ ] item 050");
    assert_eq!(rows[51], "  … +206 more");
    for (skipped, want) in [
        (1, "  (1 item could not be shown)"),
        (2, "  (2 items could not be shown)"),
    ] {
        let list = Checklist {
            revision: 1,
            items: vec![
                item("1", "a", TodoStatus::Pending),
                item("2", "b", TodoStatus::Pending),
                item("3", "c", TodoStatus::Pending),
            ],
            skipped,
        };
        let rows = texts(&committed_block_lines(
            &ChecklistCommit {
                checklist: list,
                kind: CommitKind::TurnEnded,
            },
            80,
            &style,
        ));
        assert_eq!(rows.last().expect("a last row"), want);
    }
}

#[test]
fn plain_style_keeps_glyphs_and_modifiers() {
    let style = ChecklistStyle::plain();
    let list = Checklist {
        revision: 1,
        items: vec![
            item("1", "Read the failing test", TodoStatus::Completed),
            item("2", "Fix average()", TodoStatus::InProgress),
        ],
        skipped: 0,
    };
    let section = ChecklistSection::new(&list, true, 8, Some("Ctrl+T".to_string()), &style);
    for line in section.lines(80) {
        for span in &line.spans {
            assert!(span.style.fg.is_none(), "no foreground colour");
            assert!(span.style.bg.is_none(), "no background colour");
        }
    }
    let rows = texts(&section.lines(80));
    assert!(rows.iter().any(|row| row.contains("[x]")));
    assert!(rows.iter().any(|row| row.contains("[>]")));
    let running = section.lines(80);
    let row = running
        .iter()
        .find(|line| text(line).contains("Fix average()"))
        .expect("the running row");
    assert!(
        row.spans
            .iter()
            .any(|span| span.style.add_modifier.contains(Modifier::BOLD)),
        "the running row is bold"
    );
}
