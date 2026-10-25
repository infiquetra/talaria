//! The recorded Hermes commands through the automation state. Issue 236.
//!
//! The fixture is the one the recorder wrote: a session, the three attach reads, the command
//! steps and the control updates they produced. The driver pairs each recorded request with its
//! reply and walks them in the recorded order, so the rows and the cells come from the
//! recording and not from a live gateway.

use std::collections::HashMap;
use std::path::PathBuf;

use hermes_client::ReplaySource;
use serde_json::Value;
use talaria_core::automation::{
    AutomationEffect, AutomationEvent, AutomationState, CommandResult, ControlView, ModeKey,
    canonical_name, present_command, rows, update,
};
use talaria_core::commands::{CommandOutcome, interpret_result, parse_slash_line};

/// The recorded run.
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/hermes-commands.jsonl")
}

/// One step of the recorded run.
enum Step {
    /// A request and its reply.
    Call {
        /// The method the client called.
        method: String,
        /// The request params.
        params: Value,
        /// The reply's result.
        result: Value,
    },
    /// An event the gateway sent.
    Event {
        /// The event type.
        kind: String,
        /// The event params.
        params: Value,
    },
}

/// The recorded steps in order: every reply with the request it answers, and every event.
fn steps(source: &ReplaySource) -> Vec<Step> {
    let mut requests: HashMap<i64, (String, Value)> = HashMap::new();
    for frame in source.outbound() {
        let body = &frame.frame;
        let (Some(id), Some(method)) = (body["id"].as_i64(), body["method"].as_str()) else {
            continue;
        };
        requests.insert(id, (method.to_owned(), body["params"].clone()));
    }
    let mut steps = Vec::new();
    for frame in source.inbound() {
        let body = &frame.frame;
        if body["method"] == "event" {
            steps.push(Step::Event {
                kind: body["params"]["type"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                params: body["params"].clone(),
            });
            continue;
        }
        let Some(id) = body["id"].as_i64() else {
            continue;
        };
        let Some((method, params)) = requests.get(&id) else {
            continue;
        };
        steps.push(Step::Call {
            method: method.clone(),
            params: params.clone(),
            result: body["result"].clone(),
        });
    }
    steps
}

/// The rows' text, one row per line.
fn row_text(state: &AutomationState, now_ms: u64) -> Vec<String> {
    rows(state, now_ms)
        .iter()
        .map(|row| {
            let mut text = row.label.clone();
            for part in [&row.detail, &row.tail] {
                if !part.is_empty() {
                    text.push_str(" · ");
                    text.push_str(part);
                }
            }
            text
        })
        .collect()
}

/// The clock the recording implies: a loop's own start, else the snapshot's own time.
fn now_ms_of(snapshot: &Value) -> u64 {
    let seconds = snapshot["loop"]["created_at"]
        .as_f64()
        .filter(|created| *created > 0.0)
        .or_else(|| snapshot["updated_at"].as_f64())
        .unwrap_or_default();
    (seconds * 1000.0).round() as u64
}

/// What a finished command did, as the standing state reads it.
fn command_result(outcome: &CommandOutcome) -> CommandResult {
    match outcome {
        CommandOutcome::Output { .. } => CommandResult::Output,
        CommandOutcome::Submit { .. } => CommandResult::Submit,
        CommandOutcome::OutcomeUnknown { .. } => CommandResult::Unknown,
        _ => CommandResult::NotRun,
    }
}

/// The recorded run, through the standing state.
#[test]
fn recorded_commands_reduce_to_expected_rows_and_cells() {
    let path = fixture();
    let source = ReplaySource::open(&path).expect("the recorded run opens");
    assert_eq!(
        source.header().scenario.as_deref(),
        Some("hermes-commands"),
        "the fixture is the command run"
    );

    let mut state = AutomationState::default();
    let mut session_id = String::new();
    let mut control_token: Option<u64> = None;
    let mut mode_tokens: HashMap<&'static str, u64> = HashMap::new();
    let mut cells: Vec<(String, String)> = Vec::new();
    let mut submissions: Vec<String> = Vec::new();
    let mut seen: Vec<(String, Vec<String>)> = Vec::new();
    let mut first_update_decoded = false;
    // The clock the recording implies; it only moves when a snapshot says so.
    let mut clock: u64 = 0;

    for step in steps(&source) {
        match step {
            Step::Call {
                method,
                params,
                result,
            } => match method.as_str() {
                "session.create" => {
                    session_id = result["session_id"].as_str().unwrap_or_default().to_owned();
                    let effects = update(
                        &mut state,
                        AutomationEvent::Attached {
                            session_id: session_id.clone(),
                        },
                    );
                    let mut reads = 0;
                    for effect in effects {
                        match effect {
                            AutomationEffect::ReadControl { token, .. } => {
                                control_token = Some(token);
                                reads += 1;
                            }
                            AutomationEffect::ReadMode { key, token, .. } => {
                                let name = match key {
                                    ModeKey::Yolo => "yolo",
                                    ModeKey::Fast => "fast",
                                };
                                mode_tokens.insert(name, token);
                                reads += 1;
                            }
                            _ => {}
                        }
                    }
                    assert_eq!(reads, 3, "attach reads everything once");
                }
                "session.control.read" => {
                    let snapshot = &result["control"];
                    clock = now_ms_of(snapshot);
                    let decoded = ControlView::decode(snapshot);
                    let outcome = decoded.map_err(|error| error.0);
                    update(
                        &mut state,
                        AutomationEvent::ControlRead {
                            token: control_token.expect("a control token"),
                            result: outcome.map_err(|_reason| {
                                talaria_core::automation::ReadFailure::BadResult
                            }),
                        },
                    );
                }
                "config.get" => {
                    let key = params["key"].as_str().unwrap_or_default();
                    let token = mode_tokens
                        .get(key)
                        .copied()
                        .expect("a mode token for the read");
                    let value = result["value"].as_str().map(str::to_owned);
                    update(
                        &mut state,
                        AutomationEvent::ModeRead {
                            session_id: session_id.clone(),
                            key: if key == "yolo" {
                                ModeKey::Yolo
                            } else {
                                ModeKey::Fast
                            },
                            token,
                            result: Ok(value),
                        },
                    );
                }
                "slash.exec" => {
                    let command = params["command"].as_str().unwrap_or_default();
                    let typed = format!("/{command}");
                    let line = parse_slash_line(&typed).expect("a slash line");
                    let outcome = interpret_result(&line, &typed, &result);
                    let canonical = canonical_name(&typed, None).expect("a presented command");
                    if let Some(presentation) = present_command(&canonical, &typed, &outcome) {
                        let cell = match presentation {
                            talaria_core::automation::Presentation::Output(cell) => Some(cell),
                            talaria_core::automation::Presentation::Submit { cell, .. } => cell,
                        };
                        if let Some(cell) = cell {
                            cells.push((cell.name.clone(), cell.body.clone()));
                        }
                    }
                    let effects = update(
                        &mut state,
                        AutomationEvent::CommandFinished {
                            session_id: session_id.clone(),
                            canonical,
                            result: command_result(&outcome),
                        },
                    );
                    for effect in effects {
                        if let AutomationEffect::ReadMode { key, token, .. } = effect {
                            let name = match key {
                                ModeKey::Yolo => "yolo",
                                ModeKey::Fast => "fast",
                            };
                            mode_tokens.insert(name, token);
                        }
                    }
                    if let CommandOutcome::Submit { text, .. } = &outcome {
                        submissions.push(text.clone());
                    }
                }
                // The handshake calls are not part of the automation run.
                other => assert!(
                    other == "client.capabilities" || other == "runtime.describe",
                    "the recording holds an unexpected call: {other}"
                ),
            },
            Step::Event { kind, params } => match kind.as_str() {
                "session.control.update" => {
                    let snapshot = &params["payload"]["control"];
                    if !first_update_decoded {
                        assert!(
                            ControlView::decode(snapshot).is_ok(),
                            "the first update decodes"
                        );
                        first_update_decoded = true;
                    }
                    clock = now_ms_of(snapshot);
                    update(
                        &mut state,
                        AutomationEvent::ControlUpdated {
                            session_id: session_id.clone(),
                            raw: snapshot.clone(),
                        },
                    );
                }
                "status.update" => {
                    update(
                        &mut state,
                        AutomationEvent::StatusNote {
                            session_id: session_id.clone(),
                            kind: params["kind"].as_str().unwrap_or_default().to_owned(),
                            text: params["text"].as_str().unwrap_or_default().to_owned(),
                        },
                    );
                }
                _ => {}
            },
        }
        // Every step shows the rows it left behind.
        seen.push((String::new(), row_text(&state, clock)));
    }

    assert!(first_update_decoded, "the recording holds a control update");

    // The rows the recording shows, in order.
    let goal = "⊙ goal · 0/20 turns · reply with the word ok";
    let goal_with_subgoal = "⊙ goal · 0/20 turns · 1 subgoal · reply with the word ok";
    let loop_row = "↻ loop · every 45m · 0 runs · deferred to the goal · talaria fixture check";
    let heartbeat_row = "♥ heartbeat · every 1h · 0 sent · next in 1h00m · talaria fixture ping";
    let yolo_row = "⚠ YOLO · commands run without approval in this session";

    let texts: Vec<Vec<String>> = seen.iter().map(|(_, rows)| rows.clone()).collect();
    assert!(
        texts.iter().any(|rows| rows.iter().any(|row| row == goal)),
        "the goal row shows, got {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|rows| rows.iter().any(|row| row == goal_with_subgoal)),
        "the subgoal row shows, got {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|rows| rows.iter().any(|row| row == loop_row)),
        "the loop row shows, got {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|rows| rows.iter().any(|row| row == heartbeat_row)),
        "the heartbeat row shows, got {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|rows| rows.iter().any(|row| row == yolo_row)),
        "the YOLO row shows, got {texts:?}"
    );

    // The loop, the goal and the heartbeat are gone at the end, and so is the YOLO row.
    let last = texts.last().expect("the recording ends with a row");
    assert!(
        !last.iter().any(|row| row.contains("loop")
            || row.contains("goal")
            || row.contains("heartbeat")
            || row.contains("YOLO")),
        "nothing stands at the end, got {last:?}"
    );

    // The commands left their cells: the YOLO replies and the plan's notice.
    assert!(
        cells
            .iter()
            .any(|(name, body)| name == "yolo" && body.contains("YOLO mode")),
        "the YOLO replies are cells, got {cells:?}"
    );
    assert!(
        cells
            .iter()
            .any(|(name, body)| name == "plan" && body.contains("Planning")),
        "the plan notice is a cell, got {cells:?}"
    );
    assert!(
        cells
            .iter()
            .any(|(name, body)| name == "loop" && body.contains("Loop set")),
        "the loop reply is a cell, got {cells:?}"
    );
    assert_eq!(submissions.len(), 2, "the goal and the plan queued a turn");
}
