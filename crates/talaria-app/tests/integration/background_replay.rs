//! Recorded requests and replies keep their correlation through the real session reducer.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;

use hermes_client::ReplaySource;
use hermes_contract::events::EventFrame;
use hermes_contract::schemas::{
    ProcessKillResult, ProcessListResult, ProcessStopResult, TaskIdResult,
};
use serde_json::Value;
use talaria_app::background::{kill_outcome, stop_outcome};
use talaria_core::background::{
    BackgroundEffect, BackgroundEvent, BgOrigin, BgResult, ListedProcess, ProcessId, ProcessStatus,
};
use talaria_core::session::{Cell, Effect, Event, SessionId, SessionState, update};
use talaria_core::side::RequestId;

enum Pending {
    List(RequestId),
    Kill(RequestId, ProcessId),
    Background(RequestId),
    Stop(RequestId),
}

fn apply(state: &mut SessionState, event: Event, available: &mut VecDeque<BackgroundEffect>) {
    available.extend(
        update(state, event)
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Background(effect) => Some(effect),
                _ => None,
            }),
    );
}

fn background(
    state: &mut SessionState,
    event: BackgroundEvent,
    available: &mut VecDeque<BackgroundEffect>,
) {
    apply(state, Event::Background(event), available);
}

#[test]
fn recorded_background_work_reduces_to_the_observed_result() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames/background-work.jsonl");
    let source = ReplaySource::open(&path).expect("the real background-work recording must exist");
    assert!(!source.truncated());
    assert_eq!(source.header().scenario.as_deref(), Some("background-work"));
    let session = source
        .outbound()
        .find(|frame| frame.frame["method"] == "process.list")
        .and_then(|frame| frame.frame["params"]["session_id"].as_str())
        .expect("recorded process-list session")
        .to_owned();
    let mut frames: Vec<_> = source
        .inbound()
        .map(|frame| (false, frame))
        .chain(source.outbound().map(|frame| (true, frame)))
        .collect();
    frames.sort_by_key(|(_, frame)| frame.seq);
    let mut state = SessionState::empty(SessionId::new(&session));
    let mut available = VecDeque::new();
    let mut pending = BTreeMap::new();
    let mut paired = [0_usize; 4];
    background(
        &mut state,
        BackgroundEvent::Attached { now_ms: 0 },
        &mut available,
    );
    background(
        &mut state,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 0,
        },
        &mut available,
    );
    for (outbound, recorded) in frames {
        let frame = &recorded.frame;
        let now_ms = recorded.t_ms;
        if outbound {
            let method = frame["method"].as_str().unwrap_or_default();
            let event = match method {
                "process.list" => BackgroundEvent::RefreshRequested { now_ms },
                "process.kill" => BackgroundEvent::KillRequested {
                    id: ProcessId(
                        frame["params"]["process_id"]
                            .as_str()
                            .expect("recorded process id")
                            .into(),
                    ),
                    now_ms,
                },
                "prompt.background" => BackgroundEvent::BgCommand {
                    text: frame["params"]["text"]
                        .as_str()
                        .expect("recorded background prompt")
                        .into(),
                    now_ms,
                },
                "process.stop" => BackgroundEvent::StopCommand {
                    turn_running: false,
                    now_ms,
                },
                _ => continue,
            };
            assert_eq!(
                frame["params"]["session_id"].as_str(),
                Some(session.as_str())
            );
            background(&mut state, event, &mut available);
            // Launch, tool completion, and kill replies may already have requested the next
            // list. Keep those real emitted effects until the recorder makes that call.
            let index = available
                .iter()
                .position(|effect| match (method, effect) {
                    ("process.list", BackgroundEffect::ListProcesses { .. }) => true,
                    ("process.kill", BackgroundEffect::KillProcess { id, .. }) => {
                        frame["params"]["process_id"] == id.0
                    }
                    ("prompt.background", BackgroundEffect::StartBackground { text, .. }) => {
                        frame["params"]["text"] == *text
                    }
                    ("process.stop", BackgroundEffect::StopProcesses { .. }) => true,
                    _ => false,
                })
                .unwrap_or_else(|| panic!("recorded {method} has no matching emitted core effect"));
            let effect = available.remove(index).expect("the matching effect");
            let tracked = match effect {
                BackgroundEffect::ListProcesses { request } => {
                    paired[0] += 1;
                    Pending::List(request)
                }
                BackgroundEffect::KillProcess { request, id } => {
                    paired[1] += 1;
                    Pending::Kill(request, id)
                }
                BackgroundEffect::StartBackground { request, .. } => {
                    paired[2] += 1;
                    Pending::Background(request)
                }
                BackgroundEffect::StopProcesses { request } => {
                    paired[3] += 1;
                    Pending::Stop(request)
                }
                _ => unreachable!("only call effects are matched"),
            };
            assert!(
                pending.insert(frame["id"].to_string(), tracked).is_none(),
                "unique JSON-RPC call id"
            );
        } else if frame["method"] == "event" {
            let params = &frame["params"];
            if params["session_id"].as_str() != Some(session.as_str()) {
                continue;
            }
            apply(
                &mut state,
                Event::Session {
                    session_id: SessionId::new(&session),
                    frame: EventFrame::from_params(params)
                        .expect("the recorded session event decodes"),
                    raw: params.clone(),
                },
                &mut available,
            );
        } else if let Some(tracked) = pending.remove(&frame["id"].to_string()) {
            assert!(
                frame.get("error").is_none(),
                "the recorded background calls succeeded"
            );
            let result: Value = frame.get("result").expect("a recorded call result").clone();
            let event = match tracked {
                Pending::List(request) => {
                    let result: ProcessListResult =
                        serde_json::from_value(result).expect("typed process.list result");
                    BackgroundEvent::Listed {
                        request,
                        processes: result
                            .processes
                            .iter()
                            .map(ListedProcess::from_entry)
                            .collect(),
                        now_ms,
                    }
                }
                Pending::Kill(request, process_id) => {
                    let result: ProcessKillResult =
                        serde_json::from_value(result).expect("typed process.kill result");
                    BackgroundEvent::KillReplied {
                        request,
                        process_id,
                        outcome: kill_outcome(Ok(result)),
                        now_ms,
                    }
                }
                Pending::Background(request) => {
                    let result: TaskIdResult =
                        serde_json::from_value(result).expect("typed prompt.background result");
                    BackgroundEvent::BgAccepted {
                        request,
                        task_id: result.task_id,
                        now_ms,
                    }
                }
                Pending::Stop(request) => {
                    let result: ProcessStopResult =
                        serde_json::from_value(result).expect("typed process.stop result");
                    BackgroundEvent::StopReplied {
                        request,
                        outcome: stop_outcome(Ok(result)),
                        now_ms,
                    }
                }
            };
            background(&mut state, event, &mut available);
        }
    }
    assert!(
        pending.is_empty(),
        "every recorded background request has its own reply"
    );
    assert_eq!(paired, [2, 1, 1, 1]);
    let processes: Vec<_> = state.background().rows().collect();
    assert_eq!(processes.len(), 1);
    assert_eq!(processes[0].status, ProcessStatus::Killed);
    assert_eq!(processes[0].exit_code, Some(-15));
    assert!(
        state
            .background()
            .output(&processes[0].id)
            .expect("recorded process output")
            .raw()
            .contains("tick 1")
    );
    let answers: Vec<_> = state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Background(cell) => Some(cell),
            _ => None,
        })
        .collect();
    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].origin, BgOrigin::Here);
    assert_eq!(
        answers[0].prompt.as_deref(),
        Some("Reply with exactly: bg-ok")
    );
    assert_eq!(answers[0].result, BgResult::Answer("bg-ok".into()));
    assert!(state.cells().iter().any(|cell| matches!(cell, Cell::Notice(notice) if notice.text == "No background processes were running.")));
    assert!(state.background().bg_tasks().is_empty());
}
