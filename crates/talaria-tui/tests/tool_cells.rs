//! Tool cell rows. Issue 221. Expected rows are written out by hand.

use std::time::{Duration, Instant};

use ratatui::style::{Color, Modifier};
use ratatui::text::Line;
use serde_json::json;
use talaria_core::session::{Cell, CellId, ToolCell, ToolProgress, ToolStatus};
use talaria_tui::live::LiveSection;
use talaria_tui::tool_cell::{
    Detail, DraftingLine, ToolClock, ToolRenderOptions, ToolTheme, tool_lines,
};
use unicode_width::UnicodeWidthStr;

fn cell(name: &str, status: ToolStatus) -> ToolCell {
    ToolCell {
        id: CellId::new(1),
        tool_id: "call-1".to_string(),
        name: name.to_string(),
        context: None,
        args: None,
        labels: Vec::new(),
        status,
        progress: None,
        summary: None,
        result: None,
        diff: None,
        duration_s: None,
    }
}

fn options(width: u16) -> ToolRenderOptions {
    ToolRenderOptions {
        width,
        theme: ToolTheme::default(),
        color: true,
        detail: Detail::Compact,
        expand_hint: None,
        elapsed: None,
    }
}

fn row(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn rows(lines: &[Line<'_>]) -> Vec<String> {
    lines.iter().map(row).collect()
}

/// The inner document is longer than 65,536 characters. The outer result keeps 32,768.
fn hermes_cut() -> (String, String) {
    let mut inner = String::from("{\"output\": \"");
    let mut number = 1u32;
    while inner.len() <= 65_536 {
        inner.push_str(&number.to_string());
        inner.push_str("\\n");
        number += 1;
    }
    inner.push_str("\", \"exit_code\": 0, \"error\": null}");
    let kept: String = inner.chars().take(32_768).collect();
    let outer = serde_json::to_string(&json!({"output": &kept, "truncated": true})).unwrap();
    (outer, kept)
}

fn cut_lines(kept: &str) -> usize {
    let rest = kept
        .strip_prefix("{\"output\": \"")
        .expect("the kept text starts at the output key");
    let rest = rest.strip_suffix('\\').unwrap_or(rest);
    let escapes = rest.matches("\\n").count();
    let after = rest
        .rsplit_once("\\n")
        .map(|(_, tail)| tail)
        .unwrap_or(rest);
    escapes + usize::from(!after.is_empty())
}

const INLINE: &str = "\
  ┊ review diff
a/notes.txt → b/notes.txt
@@ -1,2 +1,2 @@
 alpha
-beta
+gamma
";

#[test]
fn running_header_with_elapsed_and_progress() {
    // synthetic: name and context. source: contract schema ToolProgressPayload
    let mut tool = cell("mcp_indexer_build", ToolStatus::Running);
    tool.context = Some("src".to_string());
    tool.progress = Some(ToolProgress {
        progress: 42.0,
        total: Some(100.0),
        percent: Some(42),
        message: Some("indexing".to_string()),
    });
    for (seconds, header) in [
        (
            3.2,
            "⏺ Mcp indexer build(src)  … running 3s · 42% · indexing",
        ),
        (
            3.9,
            "⏺ Mcp indexer build(src)  … running 3s · 42% · indexing",
        ),
        (0.4, "⏺ Mcp indexer build(src)  … running · 42% · indexing"),
    ] {
        let mut opts = options(100);
        opts.elapsed = Some(Duration::from_secs_f64(seconds));
        let lines = tool_lines(&tool, &opts);
        assert_eq!(row(&lines[0]), header);
    }
}

#[test]
fn succeeded_shell_preview_and_more_line_with_and_without_hint() {
    // synthetic: ten numbered lines
    let mut tool = cell("terminal", ToolStatus::Succeeded);
    tool.args = Some(json!({"command": "seq 1 10"}));
    tool.duration_s = Some(0.1);
    tool.result = Some(
        json!({"output": "1\n2\n3\n4\n5\n6\n7\n8\n9\n10", "exit_code": 0, "error": null})
            .to_string(),
    );
    let mut hinted = options(100);
    hinted.expand_hint = Some("Ctrl+O to view".to_string());
    let lines = tool_lines(&tool, &hinted);
    assert_eq!(
        rows(&lines),
        vec![
            "⏺ Shell(seq 1 10)  ✓ · 0.1s".to_string(),
            "  ⎿  1".to_string(),
            "     2".to_string(),
            "     3".to_string(),
            "     … +7 lines (Ctrl+O to view)".to_string(),
        ]
    );
    let plain = tool_lines(&tool, &options(100));
    assert_eq!(row(plain.last().unwrap()), "     … +7 lines");

    tool.result = Some(json!({"output": "", "exit_code": 0, "error": null}).to_string());
    let empty = tool_lines(&tool, &options(100));
    assert_eq!(row(&empty[1]), "  ⎿  (no output)");

    let (outer, kept) = hermes_cut();
    tool.result = Some(outer);
    let cut = tool_lines(&tool, &options(100));
    let hidden = cut_lines(&kept).saturating_sub(3);
    assert_eq!(row(&cut[1]), "  ⎿  1");
    assert_eq!(row(&cut[2]), "     2");
    assert_eq!(row(&cut[3]), "     3");
    assert_eq!(
        row(cut.last().unwrap()),
        format!("     … +{hidden} lines (cut by Hermes at 32,768 characters)")
    );
}

#[test]
fn failed_shell_output_rows() {
    // synthetic: twelve output lines on the terminal failure shape
    let mut tool = cell("terminal", ToolStatus::Failed);
    tool.args = Some(json!({"command": "pytest -q"}));
    tool.duration_s = Some(0.1);
    let output = (1..=12)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    tool.result = Some(json!({"output": output, "exit_code": 1, "error": null}).to_string());
    let lines = tool_lines(&tool, &options(100));
    assert_eq!(row(&lines[0]), "⏺ Shell(pytest -q)  ✗ exit 1 · 0.1s");
    assert!(
        lines[0]
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Red))
    );
    assert_eq!(
        rows(&lines[1..]),
        vec![
            "  ⎿  1".to_string(),
            "     2".to_string(),
            "     3".to_string(),
            "     4".to_string(),
            "     5".to_string(),
            "     6".to_string(),
            "     … +6 lines".to_string(),
        ]
    );

    tool.result = Some(
        json!({"output": "", "exit_code": -1, "error": "Failed to execute command: boom"})
            .to_string(),
    );
    let launch = tool_lines(&tool, &options(100));
    assert_eq!(
        row(&launch[0]),
        "⏺ Shell(pytest -q)  ✗ exit -1: Failed to execute command: boom · 0.1s"
    );
    assert_eq!(row(&launch[1]), "  ⎿  Failed to execute command: boom");
    assert!(
        launch[1]
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Red))
    );
}

#[test]
fn other_tool_failure_shows_detail_rows() {
    // synthetic: an eight-line error
    let mut tool = cell("read_file", ToolStatus::Failed);
    tool.context = Some("notes.txt".to_string());
    tool.result =
        Some(json!({"success": false, "error": "e1\ne2\ne3\ne4\ne5\ne6\ne7\ne8"}).to_string());
    let lines = tool_lines(&tool, &options(100));
    assert_eq!(row(&lines[0]), "⏺ Read(notes.txt)  ✗ e1");
    assert_eq!(
        rows(&lines[1..]),
        [
            "  ⎿  e1",
            "     e2",
            "     e3",
            "     e4",
            "     e5",
            "     e6",
            "     … +2 lines"
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>()
    );
    assert!(
        lines[1]
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Red))
    );
}

#[test]
fn edit_cell_numbered_coloured_diff() {
    let mut tool = cell("patch", ToolStatus::Succeeded);
    tool.context = Some("notes.txt".to_string());
    tool.duration_s = Some(0.1);
    tool.diff = Some(INLINE.to_string());
    let lines = tool_lines(&tool, &options(100));
    assert_eq!(row(&lines[0]), "⏺ Edit notes.txt  ✓ +1 -1 · 0.1s");
    assert!(rows(&lines).contains(&"    1   alpha".to_string()));
    let removed = lines
        .iter()
        .find(|line| row(line).contains("2 - beta"))
        .expect("removed");
    let added = lines
        .iter()
        .find(|line| row(line).contains("2 + gamma"))
        .expect("added");
    assert_eq!(row(removed), "    2 - beta");
    assert_eq!(row(added), "    2 + gamma");
    assert!(
        removed
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Red))
    );
    assert!(
        added
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Green))
    );
    let context = lines
        .iter()
        .find(|line| row(line) == "    1   alpha")
        .expect("context");
    assert!(context.spans.iter().all(|span| span.style.fg.is_none()));
}

#[test]
fn multi_hunk_and_multi_file_structure() {
    let text = "\
a/x.py → b/x.py
@@ -1 +1 @@
-a
+b
a/y.py → b/y.py
@@ -5,2 +5,2 @@
 c
-d
+e
… omitted 3 diff line(s) across 1 additional file(s)/section(s)
";
    let mut tool = cell("patch", ToolStatus::Succeeded);
    tool.diff = Some(text.to_string());
    let lines = tool_lines(&tool, &options(100));
    assert!(lines.iter().any(|line| {
        row(line).contains("y.py")
            && line
                .spans
                .iter()
                .any(|span| span.style.add_modifier.contains(Modifier::BOLD))
    }));
    let two = "\
--- a/q.txt
+++ b/q.txt
@@ -1 +1 @@
-a
+b
@@ -3 +3 @@
-c
+d
";
    tool.diff = Some(two.to_string());
    let lines = tool_lines(&tool, &options(100));
    let hunk = lines
        .iter()
        .find(|line| row(line).contains("@@ -3 +3 @@"))
        .expect("second hunk");
    assert!(
        hunk.spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Cyan))
    );
    let note_tool_lines = {
        let mut noted = tool.clone();
        noted.diff = Some(text.to_owned());
        tool_lines(&noted, &options(100))
    };
    let note = note_tool_lines
        .iter()
        .find(|line| row(line).contains("omitted"))
        .expect("note");
    assert!(
        note.spans
            .iter()
            .any(|span| span.style.add_modifier.contains(Modifier::DIM))
    );
}

#[test]
fn diff_capped_at_40_rows_in_compact_full_in_detail() {
    let mut body = String::from("--- a/n.txt\n+++ b/n.txt\n@@ -1,0 +1,100 @@\n");
    for _ in 0..100 {
        body.push_str("+row\n");
    }
    let mut tool = cell("patch", ToolStatus::Succeeded);
    tool.diff = Some(body);
    let lines = tool_lines(&tool, &options(100));
    let diff_rows: Vec<_> = lines
        .iter()
        .filter(|line| row(line).contains("+ row") || row(line).contains("more diff"))
        .cloned()
        .collect();
    assert_eq!(
        diff_rows
            .iter()
            .filter(|line| row(line).contains("+ row"))
            .count(),
        40
    );
    assert_eq!(row(diff_rows.last().unwrap()), "    … +60 more diff lines");
    let mut full = options(100);
    full.detail = Detail::Full;
    let lines = tool_lines(&tool, &full);
    assert_eq!(
        lines
            .iter()
            .filter(|line| row(line).contains("+ row"))
            .count(),
        100
    );
    assert!(lines.iter().all(|line| !row(line).contains("more diff")));
}

#[test]
fn wrapped_diff_rows_keep_number_column_blank() {
    // synthetic: ninety digits on one added row
    let text = "0123456789".repeat(9);
    let diff = format!("--- a/n.txt\n+++ b/n.txt\n@@ -6,0 +7,1 @@\n+{text}\n");
    let mut tool = cell("patch", ToolStatus::Succeeded);
    tool.diff = Some(diff);
    let mut opts = options(40);
    opts.detail = Detail::Compact;
    let lines = tool_lines(&tool, &opts);
    let wrapped: Vec<_> = lines
        .iter()
        .map(row)
        .filter(|text| {
            text.chars().any(|ch| ch.is_ascii_digit())
                && !text.contains("@@")
                && !text.starts_with('⏺')
        })
        .collect();
    assert_eq!(
        wrapped,
        vec![
            "    7 + 01234567890123456789012345678901".to_string(),
            "        23456789012345678901234567890123".to_string(),
            "        45678901234567890123456789".to_string(),
        ]
    );
}

#[test]
fn rows_fit_every_width() {
    let mut tool = cell("terminal", ToolStatus::Failed);
    tool.context = Some("审".to_string());
    tool.args = Some(json!({"command": "echo 审"}));
    tool.result = Some(json!({"output": "审", "exit_code": 1, "error": "审"}).to_string());
    tool.diff = Some("@@ -1 +1 @@\n-审\n+审\n".to_string());
    for width in [1u16, 2, 5, 8] {
        let lines = tool_lines(&tool, &options(width));
        for line in &lines {
            assert!(
                row(line).width() <= usize::from(width),
                "width {width}: {}",
                row(line)
            );
        }
        if width >= 3 && tool.status != ToolStatus::Unknown {
            let header = row(&lines[0]);
            assert!(
                header.contains('✓')
                    || header.contains('✗')
                    || header.contains('…')
                    || header.contains('◌'),
                "{header}"
            );
        }
        if width == 8 {
            // A failed edit shows no diff rows, so the narrow diff row is checked on a
            // succeeded copy of the same cell.
            let mut succeeded = tool.clone();
            succeeded.status = ToolStatus::Succeeded;
            let succeeded_lines = tool_lines(&succeeded, &options(width));
            assert!(
                rows(&succeeded_lines)
                    .iter()
                    .any(|text| text.contains("+审") || text == "+审")
            );
        }
        if width == 5 {
            assert!(rows(&lines).iter().all(|text| !text.contains('⎿')));
        }
    }
    let mut runner =
        proptest::test_runner::TestRunner::new(proptest::test_runner::Config::with_cases(32));
    let strategy = proptest::prelude::any::<(u16, u8, String, String)>();
    runner
        .run(&strategy, |(raw_width, outcome, subject, output)| {
            let width = raw_width % 200 + 1;
            let status = match outcome % 5 {
                0 => ToolStatus::Running,
                1 => ToolStatus::Succeeded,
                2 => ToolStatus::Failed,
                3 => ToolStatus::Unfinished,
                _ => ToolStatus::Unknown,
            };
            let mut tool = cell("terminal", status);
            tool.context = Some(format!("{subject}审"));
            tool.result = Some(output);
            tool.diff = Some(format!("@@ -1 +1 @@\n+{subject}审\n"));
            let lines = tool_lines(&tool, &options(width));
            for line in &lines {
                proptest::prop_assert!(row(line).width() <= usize::from(width));
            }
            if width >= 3 && status != ToolStatus::Unknown {
                let header = row(&lines[0]);
                proptest::prop_assert!(
                    header.contains('✓')
                        || header.contains('✗')
                        || header.contains('…')
                        || header.contains('◌')
                );
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn no_color_keeps_signs_and_marks() {
    let mut shell = cell("terminal", ToolStatus::Failed);
    shell.args = Some(json!({"command": "pytest -q"}));
    shell.duration_s = Some(0.1);
    shell.result = Some(json!({"output": "nope", "exit_code": 1, "error": null}).to_string());
    let mut edit = cell("patch", ToolStatus::Succeeded);
    edit.context = Some("notes.txt".to_string());
    edit.duration_s = Some(0.1);
    edit.diff = Some(INLINE.to_string());
    for tool in [&shell, &edit] {
        let mut opts = options(100);
        opts.color = false;
        let lines = tool_lines(tool, &opts);
        let text = rows(&lines).join("\n");
        assert!(text.contains('✓') || text.contains('✗'));
        if tool.name == "patch" {
            assert!(text.contains('+') && text.contains('-'));
        }
        for line in &lines {
            for span in &line.spans {
                assert!(span.style.fg.is_none());
                assert!(span.style.bg.is_none());
            }
        }
    }
}

struct WindowTitle {
    title: String,
}

impl vt100::Callbacks for WindowTitle {
    fn set_window_title(&mut self, _screen: &mut vt100::Screen, title: &[u8]) {
        self.title = String::from_utf8_lossy(title).into_owned();
    }
}

#[test]
fn untrusted_text_is_literal() {
    // synthetic: a subject, an output and a diff row that carry a title change, a clear and a
    // right-to-left override. The screen already holds KEEP and a title.
    let mut parser = vt100::Parser::new_with_callbacks(
        24,
        80,
        0,
        WindowTitle {
            title: String::new(),
        },
    );
    parser.process(b"\x1b]0;kept\x07KEEP\r\n");
    assert_eq!(parser.callbacks().title, "kept");
    let mut tool = cell("terminal", ToolStatus::Succeeded);
    tool.context = Some("ls\u{1b}]0;pwned\u{7}".to_string());
    tool.result = Some("a\u{202e}b".to_string());
    tool.diff = Some("@@ -1 +1 @@\n+x\u{1b}[2Jy\n".to_string());
    let lines = tool_lines(&tool, &options(80));
    for line in &lines {
        for span in &line.spans {
            let text = span.content.as_ref();
            assert!(!text.contains('\u{1b}'), "{text}");
            assert!(!text.contains('\u{7}'), "{text}");
            assert!(!text.contains('\u{202e}'), "{text}");
        }
    }
    let mut bytes = Vec::new();
    for line in &lines {
        for span in &line.spans {
            bytes.extend(span.content.as_bytes());
        }
        bytes.extend(b"\r\n");
    }
    parser.process(&bytes);
    assert_eq!(parser.callbacks().title, "kept");
    assert!(parser.screen().contents().contains("KEEP"));
}

#[test]
fn unfinished_unknown_and_failed_edit_cells() {
    let mut sleep = cell("terminal", ToolStatus::Unfinished);
    sleep.args = Some(json!({"command": "sleep 30"}));
    let lines = tool_lines(&sleep, &options(100));
    assert_eq!(row(&lines[0]), "⏺ Shell(sleep 30)  ◌ not finished");

    let mut unknown = cell("patch", ToolStatus::Unknown);
    unknown.context = Some("notes.txt".to_string());
    unknown.diff = Some(INLINE.to_string());
    let lines = tool_lines(&unknown, &options(100));
    assert_eq!(row(&lines[0]), "⏺ Edit notes.txt  +1 -1");
    assert!(
        lines[0]
            .spans
            .iter()
            .any(|span| span.style.add_modifier.contains(Modifier::DIM))
    );
    assert!(!row(&lines[0]).contains('✓') && !row(&lines[0]).contains('✗'));

    let mut failed = cell("patch", ToolStatus::Failed);
    failed.context = Some("notes.txt".to_string());
    failed.diff = Some(INLINE.to_string());
    failed.result = Some(json!({"success": false, "error": "old_string not found"}).to_string());
    let lines = tool_lines(&failed, &options(100));
    assert_eq!(row(&lines[0]), "⏺ Edit notes.txt  ✗ old_string not found");
    assert!(
        lines
            .iter()
            .all(|line| !row(line).contains("+ gamma") && !row(line).contains("- beta"))
    );

    let tasks = tool_lines(&cell("todo_list", ToolStatus::Succeeded), &options(100));
    assert_eq!(row(&tasks[0]), "⏺ Tasks  ✓");

    let mut labeled = cell("terminal", ToolStatus::Succeeded);
    labeled.labels = vec![
        json!({"text": "Linear: create issue"}),
        json!({"text": "other"}),
    ];
    let lines = tool_lines(&labeled, &options(100));
    assert_eq!(row(&lines[0]), "⏺ Linear: create issue +1  ✓");
}

#[test]
fn full_detail_shows_arguments_output_and_whole_diff() {
    let mut tool = cell("terminal", ToolStatus::Succeeded);
    tool.args = Some(json!({"command": "ls"}));
    tool.result = Some(
        json!({"output": "one\ntwo", "exit_code": 0, "error": null, "truncation_note": "head and tail"}).to_string(),
    );
    tool.diff = Some(INLINE.to_string());
    let mut opts = options(100);
    opts.detail = Detail::Full;
    opts.expand_hint = Some("Ctrl+O to view".to_string());
    let lines = tool_lines(&tool, &opts);
    let text = rows(&lines);
    assert!(text.iter().any(|row| row == "  Arguments"));
    assert!(text.iter().any(|row| row == "    {"));
    assert!(text.iter().any(|row| row.contains("\"command\": \"ls\"")));
    assert!(text.iter().any(|row| row == "  Output"));
    assert!(text.iter().any(|row| row.contains("one")));
    assert!(text.iter().any(|row| row.contains("head and tail")));
    assert!(text.iter().any(|row| row.contains("gamma")));
    assert!(text.iter().all(|row| !row.contains("Ctrl+O")));

    let mut failed = cell("patch", ToolStatus::Failed);
    failed.context = Some("notes.txt".to_string());
    failed.diff = Some(INLINE.to_string());
    failed.result = Some(json!({"success": false, "error": "old_string not found"}).to_string());
    let lines = tool_lines(&failed, &opts);
    let text = rows(&lines);
    assert!(text.iter().any(|row| row == "  Error"));
    assert!(text.iter().any(|row| row.contains("old_string not found")));
    assert!(text.iter().any(|row| row.contains("gamma")));
}

#[test]
fn drafting_line_shows_and_hides() {
    let theme = ToolTheme::default();
    let shown = DraftingLine {
        drafted: Some("terminal"),
        theme: &theme,
    };
    assert_eq!(shown.height(80), 1);
    let area = ratatui::layout::Rect::new(0, 0, 40, 1);
    let mut buf = ratatui::buffer::Buffer::empty(area);
    shown.render(area, &mut buf);
    let text: String = (0..40).map(|x| buf[(x, 0)].symbol().to_string()).collect();
    assert!(text.contains("Preparing Shell"));
    assert!(buf[(0, 0)].style().add_modifier.contains(Modifier::DIM) || text.contains('⏺'));
    let hidden = DraftingLine {
        drafted: None,
        theme: &theme,
    };
    assert_eq!(hidden.height(80), 0);
}

#[test]
fn tool_clock_ticks_only_while_running() {
    let start = Instant::now();
    let mut first = cell("terminal", ToolStatus::Running);
    first.id = CellId::new(1);
    let mut clock = ToolClock::new();
    clock.observe(&[Cell::Tool(first.clone())], start);
    assert_eq!(
        clock.next_tick(start + Duration::from_millis(2300)),
        Some(start + Duration::from_secs(3))
    );
    let mut second = cell("read_file", ToolStatus::Running);
    second.id = CellId::new(2);
    clock.observe(
        &[Cell::Tool(first.clone()), Cell::Tool(second.clone())],
        start + Duration::from_millis(500),
    );
    assert_eq!(
        clock.next_tick(start + Duration::from_millis(2600)),
        Some(start + Duration::from_secs(3))
    );
    first.status = ToolStatus::Succeeded;
    second.status = ToolStatus::Succeeded;
    clock.observe(
        &[Cell::Tool(first), Cell::Tool(second)],
        start + Duration::from_secs(4),
    );
    assert_eq!(clock.next_tick(start + Duration::from_secs(4)), None);
}
