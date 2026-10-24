//! Replay the seven recorded sessions from issue 201.
//!
//! A missing role fails this suite by name. It is not skipped. The golden files are the
//! `describe` text for each role; this file compares them only after the recording is present.

#[path = "support/describe.rs"]
mod describe;
#[path = "support/drive.rs"]
mod drive;
#[path = "support/frames.rs"]
mod frames;

use std::path::PathBuf;

use serde_json::Value;
use talaria_core::session::{
    Cell, Effect, Event, NoticeKind, SessionState, TurnOutcome, TurnState,
};

const ROLES: &[(&str, &str)] = &[
    ("plain answer", "plain-answer.jsonl"),
    ("tool call", "tool-diff.jsonl"),
    ("approval", "approval.jsonl"),
    ("clarification", "clarification.jsonl"),
    ("interrupted", "interrupted.jsonl"),
    ("subagent batch", "subagent-batch.jsonl"),
    ("queued prompt", "queued-prompt.jsonl"),
];

/// Fixed clarification text. Recordings withhold the real answer.
const CLARIFY_ANSWER: &str = "the clarification answer";

fn fixture_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames")
        .join(file)
}

fn golden_path(file: &str) -> PathBuf {
    let stem = file.trim_end_matches(".jsonl");
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{stem}.cells.txt"))
}

fn require_role(role: &str, file: &str) -> (String, frames::FrameLog) {
    let path = fixture_path(file);
    if !path.is_file() {
        panic!(
            "missing role {role}: fixtures/frames/{file} is not recorded; report it to issue 201"
        );
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing role {role}: fixtures/frames/{file} could not be read; report it to issue 201"
        )
    });
    let log = frames::read_str(&text).unwrap_or_else(|error| panic!("{role}: {error}"));
    (text, log)
}

fn session_id_of(log: &frames::FrameLog) -> String {
    for event in &log.events {
        if let Event::Session { session_id, .. } = event
            && !session_id.as_str().is_empty()
        {
            return session_id.as_str().to_string();
        }
    }
    for outbound in &log.outbound {
        if let Some(id) = outbound.params.get("session_id").and_then(Value::as_str)
            && !id.is_empty()
        {
            return id.to_string();
        }
    }
    "local-session-1".to_string()
}

fn reduce_log(log: &frames::FrameLog) -> (SessionState, Vec<Effect>) {
    if let Some(snapshot) = log.initial_snapshot.clone() {
        return drive::reduce_snapshot(snapshot, log.events.clone());
    }
    drive::reduce(&session_id_of(log), log.events.clone())
}

fn event_params(text: &str) -> Vec<Value> {
    let mut found = Vec::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(frame) = value.get("frame") else {
            continue;
        };
        if frame.get("method").and_then(Value::as_str) != Some("event") {
            continue;
        }
        if let Some(params) = frame.get("params") {
            found.push(params.clone());
        }
    }
    found
}

fn types_named<'a>(params: &'a [Value], type_name: &str) -> Vec<&'a Value> {
    params
        .iter()
        .filter(|params| params.get("type").and_then(Value::as_str) == Some(type_name))
        .collect()
}

fn user_texts(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect()
}

fn assistant_texts(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Assistant(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn every_recorded_fixture_reduces_to_its_golden_cells() {
    let mut missing = Vec::new();
    for (role, file) in ROLES {
        if !fixture_path(file).is_file() {
            missing.push(format!(
                "missing role {role}: fixtures/frames/{file} is not recorded; report it to issue 201"
            ));
        }
    }
    if !missing.is_empty() {
        panic!("{}", missing.join("\n"));
    }
    for (role, file) in ROLES {
        let (_text, log) = require_role(role, file);
        let (state, _) = reduce_log(&log);
        let golden = std::fs::read_to_string(golden_path(file)).unwrap_or_else(|_| {
            panic!("missing role {role}: tests/golden/{file} has no cells file")
        });
        assert_eq!(describe::describe(&state), golden, "{role}");
    }
}

#[test]
fn plain_answer_fixture_has_one_user_and_one_assistant_cell() {
    let (text, log) = require_role("plain answer", "plain-answer.jsonl");
    let params = event_params(&text);
    assert!(
        types_named(&params, "tool.start").is_empty(),
        "plain answer"
    );
    assert!(
        types_named(&params, "approval.request").is_empty(),
        "plain answer"
    );
    assert!(
        types_named(&params, "clarify.request").is_empty(),
        "plain answer"
    );
    assert_eq!(
        types_named(&params, "message.complete").len(),
        1,
        "plain answer"
    );
    let (state, _) = reduce_log(&log);
    assert_eq!(user_texts(&state).len(), 1, "plain answer");
    assert_eq!(assistant_texts(&state).len(), 1, "plain answer");
    assert!(
        state
            .cells()
            .iter()
            .all(|cell| !matches!(cell, Cell::Tool(_)))
    );
    assert_eq!(state.turn(), &TurnState::Finished(TurnOutcome::Completed));
}

#[test]
fn tool_fixture_pairs_start_and_complete_by_tool_id() {
    let (text, log) = require_role("tool call", "tool-diff.jsonl");
    let params = event_params(&text);
    let starts = types_named(&params, "tool.start");
    let completes = types_named(&params, "tool.complete");
    assert!(!starts.is_empty(), "tool call");
    let diff_ids: Vec<&str> = completes
        .iter()
        .filter(|params| {
            params
                .pointer("/payload/inline_diff")
                .and_then(Value::as_str)
                .is_some_and(|diff| !diff.is_empty())
        })
        .filter_map(|params| params.pointer("/payload/tool_id").and_then(Value::as_str))
        .collect();
    assert!(!diff_ids.is_empty(), "tool call has no inline_diff");
    let (state, _) = reduce_log(&log);
    for start in &starts {
        let tool_id = start
            .pointer("/payload/tool_id")
            .and_then(Value::as_str)
            .unwrap();
        let matches = state
            .cells()
            .iter()
            .filter(|cell| matches!(cell, Cell::Tool(tool) if tool.tool_id == tool_id))
            .count();
        assert_eq!(matches, 1, "tool id {tool_id}");
    }
    let shown = state.cells().iter().any(|cell| {
        matches!(cell, Cell::Tool(tool) if diff_ids.contains(&tool.tool_id.as_str()) && tool.diff.is_some())
    });
    assert!(shown, "inline_diff was not stored on the paired tool cell");
}

#[test]
fn approval_fixture_registers_answers_and_settles() {
    let (text, log) = require_role("approval", "approval.jsonl");
    assert!(
        event_params(&text).iter().any(|params| {
            params.get("type").and_then(Value::as_str) == Some("approval.request")
        })
    );
    let recorded = log
        .outbound
        .iter()
        .find(|outbound| outbound.method == "approval.respond")
        .expect("approval.respond");
    let (state, effects) = reduce_log(&log);
    let effect = effects
        .iter()
        .find(|effect| matches!(effect, Effect::RespondApproval { .. }))
        .expect("RespondApproval");
    match effect {
        Effect::RespondApproval { key, choice } => {
            assert_eq!(
                key.session_id.as_str(),
                recorded.params["session_id"].as_str().unwrap()
            );
            assert_eq!(
                key.execution_generation,
                recorded.params["execution_generation"].as_i64().unwrap()
            );
            assert_eq!(
                key.prompt_id,
                recorded.params["prompt_id"].as_str().unwrap()
            );
            assert_eq!(choice, recorded.params["choice"].as_str().unwrap());
        }
        _ => unreachable!(),
    }
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::SendPrompt { .. }))
    );
    let prompt_id = recorded.params["prompt_id"].as_str().unwrap();
    let settled = state.cells().iter().any(|cell| {
        matches!(cell, Cell::Prompt(prompt) if prompt.key.prompt_id == prompt_id && prompt.sealed)
    });
    assert!(settled, "approval prompt did not settle");
}

#[test]
fn clarification_fixture_registers_answers_and_settles() {
    let (text, log) = require_role("clarification", "clarification.jsonl");
    assert!(
        event_params(&text).iter().any(|params| {
            params.get("type").and_then(Value::as_str) == Some("clarify.request")
        })
    );
    let recorded = log
        .outbound
        .iter()
        .find(|outbound| outbound.method == "clarify.respond")
        .expect("clarify.respond");
    let recorded_answer = recorded
        .params
        .get("answer")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(
        recorded_answer.is_empty() || recorded_answer == "[redacted]",
        "the recording was supposed to withhold the clarification answer"
    );
    let events = with_fixed_clarify(&log, recorded);
    let (state, effects) = drive::reduce(&session_id_of(&log), events);
    let effect = effects
        .iter()
        .find(|effect| matches!(effect, Effect::RespondClarify { .. }))
        .expect("RespondClarify");
    match effect {
        Effect::RespondClarify { key, answer } => {
            assert_eq!(
                key.session_id.as_str(),
                recorded.params["session_id"].as_str().unwrap()
            );
            assert_eq!(
                key.execution_generation,
                recorded.params["execution_generation"].as_i64().unwrap()
            );
            assert_eq!(
                key.prompt_id,
                recorded.params["prompt_id"].as_str().unwrap()
            );
            assert_eq!(answer, CLARIFY_ANSWER);
        }
        _ => unreachable!(),
    }
    let prompt_id = recorded.params["prompt_id"].as_str().unwrap();
    let settled = state.cells().iter().any(|cell| {
        matches!(
            cell,
            Cell::Prompt(prompt)
                if prompt.key.prompt_id == prompt_id
                    && matches!(
                        prompt.state,
                        talaria_core::session::PromptState::Settled(
                            talaria_core::session::Settlement::Answered(ref text)
                        ) if text == CLARIFY_ANSWER
                    )
        )
    });
    assert!(
        settled,
        "clarification prompt did not settle with the fixed answer"
    );
}

fn with_fixed_clarify(log: &frames::FrameLog, recorded: &frames::Outbound) -> Vec<Event> {
    let key = talaria_core::session::PromptKey {
        session_id: talaria_core::session::SessionId::new(
            recorded.params["session_id"].as_str().unwrap_or(""),
        ),
        prompt_id: recorded.params["prompt_id"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        execution_generation: recorded.params["execution_generation"]
            .as_i64()
            .unwrap_or(0),
    };
    let fixed = Event::AnswerClarify {
        key,
        answer: CLARIFY_ANSWER.to_string(),
    };
    let mut events = Vec::new();
    let mut inserted = false;
    for event in &log.events {
        if let Event::AnswerClarify { .. } = event {
            events.push(fixed.clone());
            inserted = true;
            continue;
        }
        let settled = matches!(event, Event::Session { raw, .. } if raw.get("type").and_then(Value::as_str) == Some("clarify.settled"));
        if settled && !inserted {
            events.push(fixed.clone());
            inserted = true;
        }
        events.push(event.clone());
    }
    if !inserted {
        events.push(fixed);
    }
    events
}

#[test]
fn interrupted_fixture_ends_interrupted_with_one_trace() {
    let (text, log) = require_role("interrupted", "interrupted.jsonl");
    let interrupted = event_params(&text).iter().any(|params| {
        params.get("type").and_then(Value::as_str) == Some("message.complete")
            && params.pointer("/payload/status").and_then(Value::as_str) == Some("interrupted")
    });
    assert!(interrupted, "interrupted");
    let (state, _) = reduce_log(&log);
    assert_eq!(state.turn(), &TurnState::Finished(TurnOutcome::Interrupted));
    let notices: Vec<_> = state
        .cells()
        .iter()
        .filter(
            |cell| matches!(cell, Cell::Notice(notice) if notice.kind == NoticeKind::Interrupted),
        )
        .collect();
    assert_eq!(notices.len(), 1);
    match notices[0] {
        Cell::Notice(notice) => assert_eq!(notice.text, "Interrupted"),
        _ => unreachable!(),
    }
}

#[test]
fn subagent_fixture_adds_no_cells_or_unknown_notice() {
    let (text, log) = require_role("subagent batch", "subagent-batch.jsonl");
    assert!(
        event_params(&text)
            .iter()
            .any(|params| { params.get("type").and_then(Value::as_str) == Some("subagent.start") })
    );
    let only_subagent: Vec<Event> = log
        .events
        .iter()
        .filter(|event| {
            matches!(event, Event::Session { raw, .. } if raw
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|name| name.starts_with("subagent.")))
        })
        .cloned()
        .collect();
    assert!(!only_subagent.is_empty(), "subagent batch");
    let (alone, _) = drive::reduce(&session_id_of(&log), only_subagent);
    assert!(alone.cells().is_empty(), "subagent events added cells");
    assert!(
        alone.unknown_event_types().is_empty(),
        "subagent events were noted"
    );
    let (state, _) = reduce_log(&log);
    assert!(
        state
            .unknown_event_types()
            .iter()
            .all(|name| !name.starts_with("subagent."))
    );
    assert!(state.cells().iter().all(|cell| {
        !matches!(cell, Cell::Notice(notice) if notice.text.contains("subagent."))
    }));
}

#[test]
fn queued_prompt_fixture_orders_user_cells_by_turn() {
    let (text, log) = require_role("queued prompt", "queued-prompt.jsonl");
    let mut submits_before_complete = 0usize;
    let mut saw_complete = false;
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(frame) = value.get("frame") else {
            continue;
        };
        let outbound = value.get("dir").and_then(Value::as_str) == Some("out")
            && frame.get("method").and_then(Value::as_str) == Some("prompt.submit");
        if outbound && !saw_complete {
            submits_before_complete += 1;
        }
        if frame.get("method").and_then(Value::as_str) == Some("event")
            && frame.pointer("/params/type").and_then(Value::as_str) == Some("message.complete")
        {
            saw_complete = true;
        }
    }
    assert!(submits_before_complete >= 2, "queued prompt");
    let submit_texts: Vec<String> = log
        .outbound
        .iter()
        .filter(|outbound| outbound.method == "prompt.submit")
        .map(|outbound| outbound.params["text"].as_str().unwrap_or("").to_string())
        .collect();
    let (state, _) = reduce_log(&log);
    assert_eq!(user_texts(&state), submit_texts);
}

#[test]
fn duplicated_sequenced_frames_change_no_fixture() {
    let (_text, log) = require_role("plain answer", "plain-answer.jsonl");
    let events = log.events.clone();
    let (once, _) = drive::reduce(&session_id_of(&log), events.clone());
    let mut doubled = Vec::new();
    for event in &events {
        doubled.push(event.clone());
        if matches!(event, Event::Session { frame, .. } if frame.seq.is_some()) {
            doubled.push(event.clone());
        }
    }
    let (twice, _) = drive::reduce(&session_id_of(&log), doubled);
    assert_eq!(once.cells(), twice.cells());
    assert_eq!(once.turn(), twice.turn());
    assert_eq!(once.cursor(), twice.cursor());
}
