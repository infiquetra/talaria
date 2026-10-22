//! Offline tests: frame decoding, the full-turn state machine, and render snapshots.
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use serde_json::{json, Value};
use slice::proto::{decode, Event, Incoming};
use slice::state::{wrap, Kind, OutLine, Phase, State};
use slice::ui;

fn ev(ty: &str, seq: i64, payload: Value) -> String {
    json!({"jsonrpc":"2.0","method":"event","params":{
        "type": ty, "session_id": "s1", "seq": seq, "execution_generation": 1, "payload": payload}})
    .to_string()
}

fn resp(id: i64, result: Value) -> String {
    json!({"jsonrpc":"2.0","id":id,"result":result}).to_string()
}

fn describe() -> String {
    resp(101, json!({"instance_id":"i","profile_id":"p","session_create":{"sources":["cli","tui"],"parameters":[]}}))
}

fn snapshot() -> String {
    resp(102, json!({"session_id":"s1","stored_session_id":"s1","subscription_id":"sub1","execution_generation":0,"info":{"model":"m-test"}}))
}

fn texts(lines: &[OutLine]) -> Vec<String> {
    lines.iter().map(|l| l.text.clone()).collect()
}

fn screen(term: &Terminal<TestBackend>) -> Vec<String> {
    let buf = term.backend().buffer();
    (0..buf.area.height)
        .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>().trim_end().to_string())
        .collect()
}

#[test]
fn decodes_wire_frames() {
    match decode(&ev("message.delta", 9, json!({"text":"hi"}))) {
        Incoming::Event(f) => {
            assert_eq!(f.seq, Some(9));
            assert_eq!(f.session_id.as_deref(), Some("s1"));
            assert_eq!(f.event, Event::MessageDelta { text: Some("hi".into()) });
        }
        o => panic!("{o:?}"),
    }
    // null text is a segment boundary
    match decode(&ev("message.delta", 10, json!({"text":null}))) {
        Incoming::Event(f) => assert_eq!(f.event, Event::MessageDelta { text: None }),
        o => panic!("{o:?}"),
    }
    match decode(&ev("tool.start", 11, json!({"tool_id":"t","name":"terminal","context":"ls -la","args":{"command":"ls -la"}}))) {
        Incoming::Event(f) => assert_eq!(f.event, Event::ToolStart { id: "t".into(), name: "terminal".into(), preview: "ls -la".into() }),
        o => panic!("{o:?}"),
    }
    match decode(&ev("approval.request", 12, json!({"prompt_id":"p1","command":"rm -rf x","description":"recursive delete","choices":["once","deny","session","always"],"execution_generation":2}))) {
        Incoming::Event(f) => match f.event {
            Event::ApprovalRequest(a) => {
                assert_eq!(a.prompt_id, "p1");
                assert_eq!(a.choices, ["once", "deny", "session", "always"]);
                assert_eq!(a.execution_generation, Some(2));
            }
            e => panic!("{e:?}"),
        },
        o => panic!("{o:?}"),
    }
    assert!(matches!(decode(&resp(5, json!({"ok":1}))), Incoming::Response { id: Some(5), .. }));
    assert!(matches!(decode("not json"), Incoming::Other(_)));
    assert!(matches!(decode(r#"{"jsonrpc":"2.0","id":1,"method":"x"}"#), Incoming::Other(_)));
    // thinking.delta (what the gateway really sends) is treated as reasoning
    match decode(&ev("thinking.delta", 1, json!({"text":"(x) building..."}))) {
        Incoming::Event(f) => assert!(matches!(f.event, Event::ReasoningDelta { .. })),
        o => panic!("{o:?}"),
    }
}

#[test]
fn full_turn_start_deltas_tool_approval_complete() {
    let mut s = State::new("minimax-m3.1-flash", "ollama-cloud", "/proj", 60);
    // connect handshake chains: ready -> describe request -> create request -> submit request
    assert!(s.submit_prompt("run it", 0).is_none(), "queued until the session exists");
    let o = s.on_frame(&ev("gateway.ready", 0, json!({})), 5);
    assert_eq!(o[0].method, "runtime.describe");
    assert_eq!(s.metrics.ready_ms, Some(5));
    let o = s.on_frame(&describe(), 6);
    assert_eq!(o[0].method, "session.create");
    assert_eq!(o[0].params["model"], "minimax-m3.1-flash");
    assert_eq!(o[0].params["provider"], "ollama-cloud");
    assert_eq!(o[0].params["source"], "tui");
    let o = s.on_frame(&snapshot(), 7);
    assert_eq!(o[0].method, "prompt.submit");
    assert_eq!(o[0].params["session_id"], "s1");
    assert_eq!(o[0].params["text"], "run it");
    assert!(o[0].params["input_id"].as_str().is_some_and(|x| !x.is_empty()), "server requires input_id");
    assert_eq!(s.model, "m-test");
    assert!(s.running);
    assert_eq!(s.phase, Phase::Thinking);

    s.on_frame(&ev("message.start", 1, json!({})), 10);
    s.on_frame(&ev("message.delta", 2, json!({"text":"I will "})), 1000);
    s.on_frame(&ev("message.delta", 3, json!({"text":"run it.\n\nSecond paragraph"})), 1100);
    assert_eq!(s.phase, Phase::Streaming);
    assert_eq!(s.metrics.first_char_ms, vec![993]); // 1000 - turn start (7)
    s.on_frame(&ev("message.delta", 4, json!({"text":null})), 1200);
    s.on_frame(&ev("tool.start", 5, json!({"tool_id":"t1","name":"terminal","context":"rm -rf .cache"})), 1300);
    assert_eq!(s.phase, Phase::Tool);
    assert!(s.live_lines().iter().any(|l| l.text.contains("terminal: rm -rf .cache")));
    s.on_frame(&ev("approval.request", 6, json!({"prompt_id":"p1","command":"rm -rf .cache","description":"recursive delete","choices":["once","deny","session","always"],"execution_generation":1})), 1400);
    assert_eq!(s.phase, Phase::WaitingApproval);
    assert!(s.approval.is_some());
    let out = s.approval_answer(Some(0)).expect("answer");
    assert_eq!(out.method, "approval.respond");
    assert_eq!(out.params["choice"], "once");
    assert_eq!(out.params["prompt_id"], "p1");
    assert_eq!(out.params["session_id"], "s1");
    assert_eq!(out.params["execution_generation"], 1);
    s.on_frame(&ev("approval.settled", 7, json!({"prompt_id":"p1"})), 1500);
    s.on_frame(&ev("tool.complete", 8, json!({"tool_id":"t1","name":"terminal","context":"rm -rf .cache","duration_s":1.234,"is_error":false})), 2600);
    s.on_frame(&ev("session.usage", 9, json!({"usage":{"model":"minimax-m3.1-flash","input":10,"output":2}})), 2650);
    s.on_frame(&ev("message.delta", 10, json!({"text":"\n\nAll done."})), 2700);
    s.on_frame(&ev("message.complete", 11, json!({"text":"All done.","status":"complete"})), 2800);
    assert!(!s.running);
    assert_eq!(s.phase, Phase::Done);
    assert_eq!(s.elapsed_ms(9999), 2793);

    let lines = texts(&s.take_committed());
    assert_eq!(lines[0], "› run it");
    assert!(lines.contains(&"I will run it.".to_string()));
    assert!(lines.contains(&"Second paragraph".to_string()));
    assert!(lines.contains(&"  approval: once".to_string()));
    assert!(lines.contains(&"⏺ terminal: rm -rf .cache ✓ 1.2s".to_string()), "{lines:?}");
    assert!(lines.contains(&"All done.".to_string()));
    // leading blank lines of a segment after a tool call are dropped
    let tool_at = lines.iter().position(|l| l.starts_with("⏺")).unwrap();
    assert_eq!(lines[tool_at + 1], "All done.");
}

#[test]
fn tool_diff_is_colored_by_sign() {
    let mut s = State::new("m", "p", "/", 80);
    s.on_frame(&ev("tool.complete", 1, json!({"tool_id":"t","name":"patch","context":"calc.py",
        "inline_diff":"--- a/calc.py\n+++ b/calc.py\n@@ -1,2 +1,2 @@\n def add(a, b):\n-    return a - b\n+    return a + b"})), 0);
    let l = s.take_committed();
    let kinds: Vec<Kind> = l.iter().map(|x| x.kind).collect();
    assert_eq!(kinds, vec![Kind::Tool, Kind::DiffHunk, Kind::DiffHunk, Kind::DiffHunk, Kind::Dim, Kind::DiffDel, Kind::DiffAdd]);
}

#[test]
fn error_and_interrupt_end_the_turn() {
    let mut s = State::new("m", "p", "/", 80);
    s.on_frame(&describe(), 0);
    s.on_frame(&snapshot(), 0);
    s.submit_prompt("x", 1);
    assert!(s.running);
    s.on_frame(&ev("error", 1, json!({"message":"boom"})), 5);
    assert!(!s.running);
    assert_eq!(s.phase, Phase::Failed);
    assert!(texts(&s.take_committed()).iter().any(|l| l == "✗ boom"));
    s.submit_prompt("y", 10);
    let i = s.interrupt().expect("interrupt");
    assert_eq!(i.method, "session.interrupt");
    assert_eq!(i.params["session_id"], "s1");
    s.on_frame(&ev("message.complete", 2, json!({"status":"interrupted"})), 20);
    assert_eq!(s.phase, Phase::Done);
    assert!(texts(&s.take_committed()).iter().any(|l| l.contains("interrupted")));
    assert_eq!(s.detach().unwrap().method, "session.detach");
}

#[test]
fn wrapping_is_stable_and_width_aware() {
    assert_eq!(wrap("hello world foo", 11), vec!["hello world", "foo"]);
    assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    assert_eq!(wrap("", 10), vec![""]);
    assert_eq!(wrap("日本語日本語", 6), vec!["日本語", "日本語"]);
}

#[test]
fn streaming_tail_commits_overflow_only() {
    let mut s = State::new("m", "p", "/", 20);
    s.on_frame(&describe(), 0);
    s.on_frame(&snapshot(), 0);
    s.submit_prompt("go", 0);
    s.take_committed();
    let text: String = (0..12).map(|i| format!("line{i}\n")).collect::<String>() + "partial";
    s.on_frame(&ev("message.delta", 1, json!({"text": text})), 10);
    let live = s.live_lines();
    assert!(live.len() <= slice::state::TAIL_ROWS);
    assert_eq!(live.last().unwrap().text, "partial");
    let done = texts(&s.take_committed());
    assert_eq!(done.first().map(String::as_str), Some("line0"));
    // committed lines + live lines reassemble the text exactly, in order
    let mut all = done;
    all.extend(texts(&live));
    let want: Vec<String> = (0..12).map(|i| format!("line{i}")).chain(["partial".to_string()]).collect();
    assert_eq!(all, want);
}

#[test]
fn render_snapshot_streaming_live_region() {
    let mut s = State::new("minimax-m3.1-flash", "ollama-cloud", "/", 50);
    s.on_frame(&describe(), 0);
    s.on_frame(&snapshot(), 0);
    s.submit_prompt("hi", 0);
    s.on_frame(&ev("message.delta", 1, json!({"text":"Hello there, this is streaming"})), 1500);
    let mut term = Terminal::new(TestBackend::new(50, ui::live_height(&s))).unwrap();
    term.draw(|f| ui::draw_live(f, &s, 2000)).unwrap();
    let rows = screen(&term);
    assert_eq!(rows.len(), 7);
    assert_eq!(rows[0], "Hello there, this is streaming");
    assert!(rows[5].ends_with("m-test · streaming · 2.0s") || rows[5].contains("minimax") || rows[5].contains("m-test"), "{rows:?}");
    assert!(rows[5].contains("streaming · 2.0s"), "{rows:?}");
    assert!(rows[6].contains("Ctrl+C to interrupt"), "{rows:?}");
}

#[test]
fn render_snapshot_approval_box() {
    let mut s = State::new("m1", "p", "/", 50);
    s.on_frame(&ev("approval.request", 1, json!({"prompt_id":"p1","command":"rm -rf .pytest_cache","description":"recursive delete","choices":["once","deny","session","always"]})), 0);
    s.approval_move(1);
    let h = ui::live_height(&s);
    let mut term = Terminal::new(TestBackend::new(50, h)).unwrap();
    term.draw(|f| ui::draw_live(f, &s, 0)).unwrap();
    let rows = screen(&term);
    let expect = vec![
        "⏸ approval required",
        "  $ rm -rf .pytest_cache",
        "  recursive delete",
        "   1. once",
        " › 2. deny",
        "   3. session",
        "   4. always",
        "m1 · waiting for approval",
        "  1-4 or ↑/↓ + Enter to answer",
    ];
    assert_eq!(rows, expect);
}

/// The recording taken from a live run: decode + state machine over the real frames.
#[test]
fn replays_recorded_live_session() {
    let text = include_str!("fixtures/live_two_turns.jsonl");
    let mut s = State::new("minimax-m3.1-flash", "ollama-cloud", "/p", 100);
    let mut sent = vec![];
    let mut approvals = 0;
    for l in text.lines() {
        let v: Value = serde_json::from_str(l).unwrap();
        let raw = v["frame"].as_str().unwrap();
        sent.extend(s.on_frame(raw, 0).into_iter().map(|o| o.method));
        if s.approval.is_some() && approvals == 0 {
            approvals = 1;
            assert_eq!(s.phase, Phase::WaitingApproval);
            s.approval_answer(Some(0)).unwrap();
        }
    }
    assert_eq!(sent, ["runtime.describe", "session.create"]);
    assert_eq!(approvals, 1);
    assert!(!s.running);
    assert_eq!(s.phase, Phase::Done);
    assert_eq!(s.metrics.turns, 2);
    let lines = texts(&s.take_committed());
    assert!(lines.iter().any(|l| l.starts_with("⏺ terminal: rm -rf .pytest_cache") && l.contains('✓')), "{lines:#?}");
    assert!(lines.iter().any(|l| l.starts_with("⏺ search_files")), "{lines:#?}");
}

#[test]
fn ansi_colored_diff_from_hermes_is_cleaned_capped_and_classified() {
    let mut s = State::new("m", "p", "/", 40);
    let esc = "\x1b[38;2;255;255;255;48;2;14;127;10m";
    let mut diff = String::from("  ┊ review diff\n\x1b[38;2;97;175;239ma/x.md → b/x.md\x1b[0m\n\x1b[38;2;107;114;128m@@ -0,0 +1,40 @@\x1b[0m\n");
    for i in 0..40 {
        diff.push_str(&format!("{esc}+row {i}\x1b[0m\n"));
    }
    s.on_frame(&ev("tool.complete", 1, json!({"tool_id":"t","name":"write_file","context":"x.md","inline_diff":diff})), 0);
    let l = s.take_committed();
    assert!(l.iter().all(|x| !x.text.contains('\x1b')), "escapes must be stripped");
    assert_eq!(l[1].kind, Kind::Dim);
    assert_eq!(l[2].kind, Kind::DiffHunk);
    assert_eq!(l[3].kind, Kind::DiffHunk);
    assert_eq!(l[4].kind, Kind::DiffAdd);
    assert_eq!(l[4].text, "  +row 0");
    assert!(l.last().unwrap().text.contains("more diff lines"), "{:?}", l.last());
    assert!(l.len() <= 27);
}
