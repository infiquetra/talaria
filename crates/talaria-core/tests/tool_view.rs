//! Tool view. Issue 221. Expected headlines are written out by hand.

#[path = "support/bake.rs"]
mod bake;
#[path = "support/events.rs"]
mod events;

use serde_json::{Value, json};
use talaria_core::session::{
    Cell, CellId, OutcomeView, ToolCell, ToolProgress, ToolStatus, format_duration,
    format_progress, tool_view,
};

const TS: &str = "../../docs/handoff/2026-10-22-next-steps/reference-slices/typescript/tests/fixtures/live-two-turns.jsonl";

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

fn failed(result: &str) -> ToolCell {
    let mut tool = cell("terminal", ToolStatus::Failed);
    tool.result = Some(result.to_string());
    tool
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

#[test]
fn shell_subject_is_the_command_not_the_context() {
    // source: TypeScript bake-off recording, seq 33 and seq 36
    let events = bake::recording(TS);
    let mut state = bake::empty();
    bake::apply(&mut state, &bake::by_seq(&events, 33));
    bake::apply(&mut state, &bake::by_seq(&events, 36));
    let Cell::Tool(tool) = state
        .cells()
        .iter()
        .find(|cell| matches!(cell, Cell::Tool(tool) if tool.name == "terminal"))
        .expect("the recording has a terminal cell")
    else {
        unreachable!()
    };
    assert_eq!(
        tool.args
            .as_ref()
            .and_then(|args| args.get("command"))
            .and_then(Value::as_str),
        Some("rm -rf .pytest_cache && ls -a"),
        "the reducer kept the command"
    );
    let view = tool_view(tool);
    assert_eq!(
        view.subject.as_deref(),
        Some("rm -rf .pytest_cache && ls -a")
    );
    assert_ne!(
        view.subject.as_deref(),
        Some("rm -rf .pytest_cache + 1 command")
    );
}

#[test]
fn label_text_is_the_title_for_bridge_calls() {
    // source: contract schema ToolLabel
    let mut tool = cell("terminal", ToolStatus::Succeeded);
    tool.labels = vec![
        json!({"kind": "app", "text": "Linear: create issue", "emoji": "📌"}),
        json!({"kind": "app", "text": "other"}),
    ];
    let view = tool_view(&tool);
    assert_eq!(view.title, "Linear: create issue +1");
    assert_eq!(view.subject, None);
}

#[test]
fn titles_follow_the_table_and_humanise_unknown_names() {
    let table = [
        ("terminal", "Shell"),
        ("execute_code", "Code"),
        ("read_file", "Read"),
        ("write_file", "Write"),
        ("patch", "Edit"),
        ("search_files", "Search"),
        ("web_search", "Web search"),
        ("web_extract", "Fetch"),
        ("delegate_task", "Delegate"),
        ("skill_view", "Skill"),
        ("skill_manage", "Skill"),
        ("memory", "Memory"),
        ("clarify", "Question"),
        ("todo_list", "Tasks"),
        ("todo", "Tasks"),
        ("mcp_indexer_build", "Mcp indexer build"),
        ("", "Tool"),
    ];
    for (name, title) in table {
        let view = tool_view(&cell(name, ToolStatus::Succeeded));
        assert_eq!(view.title, title, "{name}");
    }
}

#[test]
fn failed_headlines_from_terminal_shapes() {
    // source: contract schema, terminal result
    let shapes: [(serde_json::Value, &str, Option<&str>); 4] = [
        (
            json!({"output": "ls: /talaria-no-such-dir: No such file or directory", "exit_code": 1, "error": null}),
            "exit 1",
            None,
        ),
        (
            json!({"output": "", "exit_code": 1, "error": null, "exit_code_meaning": "No matches found (not an error)"}),
            "exit 1: No matches found (not an error)",
            None,
        ),
        (
            json!({"output": "", "exit_code": -1, "error": "Failed to execute command: boom", "traceback": "<redacted>", "status": "error"}),
            "exit -1: Failed to execute command: boom",
            None,
        ),
        (
            json!({"output": "", "exit_code": 1, "error": null, "user_summary": "Command denied by the user"}),
            "exit 1: Command denied by the user",
            None,
        ),
    ];
    let first = serde_json::to_string(&shapes[0].0).unwrap();
    let view = tool_view(&failed(&first));
    assert!(
        matches!(view.outcome, OutcomeView::Failed { ref headline, detail: None } if headline == "exit 1")
    );
    for (value, headline, _detail) in &shapes {
        let view = tool_view(&failed(&serde_json::to_string(value).unwrap()));
        match view.outcome {
            OutcomeView::Failed {
                headline: got,
                detail,
            } => {
                assert_eq!(got, *headline);
                assert_eq!(detail, None);
            }
            other => panic!("failed outcome, got {other:?}"),
        }
    }
    let raw = tool_view(&failed("permission denied\nmore"));
    match raw.outcome {
        OutcomeView::Failed { headline, detail } => {
            assert_eq!(headline, "permission denied");
            assert_eq!(detail.as_deref(), Some("permission denied\nmore"));
        }
        other => panic!("raw failure, got {other:?}"),
    }
    let empty = tool_view(&failed("{}"));
    assert!(
        matches!(empty.outcome, OutcomeView::Failed { ref headline, detail: None } if headline == "failed")
    );
}

#[test]
fn success_with_nonzero_exit_shows_exit_but_stays_succeeded() {
    let mut tool = cell("read_file", ToolStatus::Succeeded);
    tool.result = Some(r#"{"content":"x","exit_code":1}"#.to_string());
    let view = tool_view(&tool);
    assert!(matches!(
        view.outcome,
        OutcomeView::Succeeded { headline: Some(ref text) } if text == "exit 1"
    ));
    tool.result = Some(r#"{"content":"x","exit_code":true}"#.to_string());
    let view = tool_view(&tool);
    assert!(matches!(
        view.outcome,
        OutcomeView::Succeeded { headline: None }
    ));
}

#[test]
fn hermes_cut_result_is_decoded() {
    let (outer, kept) = hermes_cut();
    let mut tool = cell("terminal", ToolStatus::Succeeded);
    tool.result = Some(outer);
    let view = tool_view(&tool);
    let output = view.output.expect("output");
    assert!(output.cut_by_hermes);
    assert_eq!(output.exit_code, None);
    assert_eq!(output.lines, cut_lines(&kept));
    assert!(
        output.text.starts_with('1')
            || output.text.starts_with("1\n")
            || output.text.starts_with('1')
    );
    assert!(!output.text.contains("{\"output\""));
    for line in output.text.lines() {
        assert!(!line.contains("{\"output\""), "{line}");
    }

    // synthetic: the same cut, with the outer key `error` and the call failed
    let failed_outer = serde_json::to_string(&json!({"error": kept, "truncated": true})).unwrap();
    let mut failed_tool = cell("terminal", ToolStatus::Failed);
    failed_tool.result = Some(failed_outer);
    let failed_view = tool_view(&failed_tool);
    assert!(matches!(
        failed_view.outcome,
        OutcomeView::Failed { ref headline, detail: None } if headline == "failed"
    ));

    let plain = tool_view(&{
        let mut tool = cell("terminal", ToolStatus::Succeeded);
        tool.result =
            Some(serde_json::to_string(&json!({"output": "plain", "truncated": true})).unwrap());
        tool
    });
    assert_eq!(plain.output.expect("plain").text, "plain");
}

#[test]
fn non_json_result_is_raw_text() {
    let view = tool_view(&failed_as_success("plain words"));
    let output = view.output.expect("output");
    assert_eq!(output.text, "plain words");
    assert_eq!(output.lines, 1);
}

fn failed_as_success(result: &str) -> ToolCell {
    let mut tool = cell("terminal", ToolStatus::Succeeded);
    tool.result = Some(result.to_string());
    tool
}

#[test]
fn output_cleanup_strips_escapes_and_carriage_returns() {
    let result = "{\"output\": \"\\u001b[31mred\\u001b[0m\\r\\nnext\\n10%\\r50%\\r100%\\n\\n\"}";
    let view = tool_view(&failed_as_success(result));
    let output = view.output.expect("output");
    assert_eq!(output.text, "red\nnext\n100%");
    assert_eq!(output.lines, 3);
    assert!(!output.text.contains('\u{1b}'));
}

#[test]
fn duration_wording_matches_hermes() {
    let cases = [
        (0.776_f64, Some("0.8s")),
        (9.96, Some("10.0s")),
        (12.4, Some("12s")),
        (12.5, Some("12s")),
        (59.6, Some("60s")),
        (65.0, Some("1m 5s")),
        (90.5, Some("1m 30s")),
        (120.0, Some("2m")),
        (-1.0, None),
    ];
    for (seconds, words) in cases {
        assert_eq!(format_duration(seconds).as_deref(), words, "{seconds}");
    }
    assert_eq!(format_duration(f64::NAN), None);
}

#[test]
fn progress_wording_percent_fraction_message() {
    // source: contract schema ToolProgressPayload
    let payloads = [
        ToolProgress {
            progress: 42.0,
            total: Some(100.0),
            percent: Some(42),
            message: Some("indexing".to_string()),
        },
        ToolProgress {
            progress: 3.0,
            total: None,
            percent: None,
            message: None,
        },
        ToolProgress {
            progress: 3.0,
            total: Some(10.0),
            percent: None,
            message: None,
        },
        ToolProgress {
            progress: 3.0,
            total: Some(10.0),
            percent: Some(150),
            message: None,
        },
    ];
    let words = [Some("42% · indexing"), None, Some("3/10"), Some("3/10")];
    for (payload, expected) in payloads.iter().zip(words) {
        assert_eq!(format_progress(payload).as_deref(), expected);
    }
}

#[test]
fn diff_from_inline_diff_or_patch_result() {
    let mut with_diff = cell("read_file", ToolStatus::Succeeded);
    with_diff.diff = Some(INLINE.to_string());
    let from_field = tool_view(&with_diff);
    assert_eq!(
        from_field
            .diff
            .as_ref()
            .and_then(|diff| diff.display_path()),
        Some("notes.txt")
    );

    let mut patch = cell("patch", ToolStatus::Succeeded);
    patch.result = Some(PATCH_RESULT.to_string());
    let from_result = tool_view(&patch);
    assert_eq!(
        from_result
            .diff
            .as_ref()
            .and_then(|diff| diff.display_path()),
        Some("calc.py")
    );

    let write = cell("write_file", ToolStatus::Succeeded);
    assert!(tool_view(&write).diff.is_none());
}

const INLINE: &str = "\
  ┊ review diff
a/notes.txt → b/notes.txt
@@ -1,2 +1,2 @@
 alpha
-beta
+gamma
";

const PATCH_RESULT: &str = "{\"success\": true, \"diff\": \"--- a/calc.py\\n+++ b/calc.py\\n@@ -9,3 +9,3 @@ def average(values):\\n     total = sum(values)\\n-    return total / (len(values) + 1)\\n+    return total / len(values)\\n \\n\", \"files_modified\": [\"calc.py\"]}";

#[test]
fn quiet_tools_have_no_body() {
    for name in ["clarify", "todo_list", "todo"] {
        let view = tool_view(&cell(name, ToolStatus::Succeeded));
        assert!(view.quiet, "{name}");
        assert!(view.output.is_none() || view.quiet);
    }
}
