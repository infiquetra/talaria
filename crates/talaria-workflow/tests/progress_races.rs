//! A background snapshot must not repeat an in-flight control.
#[path = "support/lines.rs"]
mod lines;

use serde_json::json;
use talaria_core::subagents::{SubagentEvent, SubagentEventKind};
use talaria_workflow::progress::{
    AgentState, RunProgress,
    live::{NodeKey, ProgressEffect, ProgressInput, ProgressKey, ProgressState, update},
};

#[test]
fn background_attach_does_not_repeat_an_inflight_steer_or_interrupt() {
    let key = lines::KEY_RUNNING;
    let mut state = ProgressState::new(RunProgress::from_records(
        &lines::manifest(),
        &lines::live_running(key, "sess-running", 7),
    ));
    update(&mut state, ProgressInput::Connected(true));
    state.select_agent(key);
    state.steer_input = Some("look again".into());
    assert!(matches!(
        update(&mut state, ProgressInput::Key(ProgressKey::Enter)).as_slice(),
        [ProgressEffect::Steer { generation: 7, .. }]
    ));
    let attached = || ProgressInput::Attached {
        session_id: "sess-running".into(),
        runtime_id: "sess-running".into(),
        generation: Some(7),
    };
    assert!(update(&mut state, attached()).is_empty());
    assert_eq!(state.footer.as_deref(), Some("sent"));
    update(
        &mut state,
        ProgressInput::GatewayDone {
            key: key.into(),
            result: Ok(json!({"status": "queued"})),
        },
    );
    assert_eq!(
        state.footer.as_deref(),
        Some("steer queued for its next step")
    );
    assert!(matches!(
        update(&mut state, ProgressInput::Key(ProgressKey::StopAgent)).as_slice(),
        [ProgressEffect::Interrupt { generation: 7, .. }]
    ));
    assert!(update(&mut state, attached()).is_empty());
    assert_eq!(
        state.progress().agent(key).unwrap().state,
        AgentState::Stopping
    );
    update(
        &mut state,
        ProgressInput::GatewayDone {
            key: key.into(),
            result: Ok(json!({"interrupted": true})),
        },
    );
    assert!(update(&mut state, attached()).is_empty());
    assert!(update(&mut state, ProgressInput::Key(ProgressKey::StopAgent)).is_empty());
}

fn child_event(kind: SubagentEventKind, status: &str) -> ProgressInput {
    ProgressInput::Subagent {
        session_id: "sess-running".into(),
        event: SubagentEvent {
            kind,
            payload: serde_json::from_value(json!({
                "api_calls": null, "child_session_id": null, "delegation_id": null,
                "depth": 1, "duration_seconds": null, "files_read": null,
                "files_written": null, "goal": "review", "input_tokens": null,
                "model": null, "output_tail": null, "output_tokens": null,
                "parent_id": null, "reasoning_tokens": null, "status": status,
                "subagent_id": "child-1", "summary": null, "task_count": 1,
                "task_index": 0, "text": null, "tool_count": null,
                "tool_name": null, "tool_preview": null, "toolsets": null
            }))
            .unwrap(),
        },
    }
}

#[test]
fn subagent_stop_acknowledgement_waits_for_terminal_evidence() {
    let key = lines::KEY_RUNNING;
    let mut state = ProgressState::new(RunProgress::from_records(
        &lines::manifest(),
        &lines::live_running(key, "sess-running", 7),
    ));
    update(&mut state, ProgressInput::Connected(true));
    update(&mut state, child_event(SubagentEventKind::Start, "running"));
    state.selection = NodeKey::Subagent {
        key: key.into(),
        id: "child-1".into(),
    };
    assert!(matches!(
        update(&mut state, ProgressInput::Key(ProgressKey::StopAgent)).as_slice(),
        [ProgressEffect::SubInterrupt { subagent_id, .. }] if subagent_id == "child-1"
    ));
    update(
        &mut state,
        ProgressInput::GatewayDone {
            key: key.into(),
            result: Ok(json!({"found": true})),
        },
    );
    assert!(update(&mut state, ProgressInput::Key(ProgressKey::StopAgent)).is_empty());
    update(
        &mut state,
        child_event(SubagentEventKind::Complete, "completed"),
    );
    state.select_agent(key);
    assert!(matches!(
        update(&mut state, ProgressInput::Key(ProgressKey::StopAgent)).as_slice(),
        [ProgressEffect::Interrupt { generation: 7, .. }]
    ));
}

#[test]
fn queued_subagent_refuses_steer_locally() {
    let key = lines::KEY_RUNNING;
    let mut state = ProgressState::new(RunProgress::from_records(
        &lines::manifest(),
        &lines::live_running(key, "sess-running", 7),
    ));
    update(&mut state, ProgressInput::Connected(true));
    update(
        &mut state,
        child_event(SubagentEventKind::SpawnRequested, "queued"),
    );
    state.selection = NodeKey::Subagent {
        key: key.into(),
        id: "child-1".into(),
    };
    assert!(update(&mut state, ProgressInput::Key(ProgressKey::Steer)).is_empty());
    assert!(state.steer_input.is_none());
}

#[test]
fn sessions_arriving_after_open_attach_once_and_again_after_reconnect() {
    let mut state = ProgressState::blank(&lines::manifest());
    assert!(update(&mut state, ProgressInput::Connected(true)).is_empty());
    let records = lines::live_running(lines::KEY_RUNNING, "sess-running", 7);
    let attached = update(&mut state, ProgressInput::Journal(records.clone()));
    assert!(matches!(attached.as_slice(),
        [ProgressEffect::Attach { session_id }] if session_id == "sess-running"));
    assert!(update(&mut state, ProgressInput::Journal(records)).is_empty());
    update(&mut state, ProgressInput::Connected(false));
    let reconnected = update(&mut state, ProgressInput::Connected(true));
    assert!(matches!(reconnected.as_slice(),
        [ProgressEffect::Attach { session_id }] if session_id == "sess-running"));
}

#[test]
fn interrupted_settlement_stops_without_waiting_for_call_finished() {
    use talaria_workflow::journal::{Record, RecordLine};
    let key = lines::KEY_RUNNING;
    let records = lines::live_running(key, "sess-running", 7);
    let next = records.last().unwrap().seq + 1;
    let mut state = ProgressState::new(RunProgress::from_records(&lines::manifest(), &records));
    update(&mut state, ProgressInput::Connected(true));
    state.select_agent(key);
    update(&mut state, ProgressInput::Key(ProgressKey::StopAgent));
    update(
        &mut state,
        ProgressInput::GatewayDone {
            key: key.into(),
            result: Ok(json!({"interrupted": true})),
        },
    );
    update(
        &mut state,
        ProgressInput::Journal(vec![RecordLine {
            v: 1,
            seq: next,
            at_ms: 9000,
            record: Record::TurnSettled {
                key: key.into(),
                attempt: 0,
                turn: 1,
                admission_id: "running-admission".into(),
                outcome: "interrupted".into(),
                final_text: None,
                final_text_ref: None,
                result_meta: json!({}),
                usage: None,
            },
        }]),
    );
    let agent = state.progress.agent(key).unwrap();
    assert_eq!(agent.state, AgentState::Failed);
    assert!(agent.stopped);
    assert_eq!(agent.finished_at_ms, Some(9000));
    assert_eq!(
        state.footer.as_deref(),
        Some("stopped — restart (r) runs it again")
    );
    // A delayed live event must not revive the interrupted row.
    update(&mut state, child_event(SubagentEventKind::Start, "running"));
    assert_eq!(state.progress.agent(key).unwrap().state, AgentState::Failed);
}

#[test]
fn stop_receipt_settles_standalone_view_and_rejects_stale_attempts() {
    let key = lines::KEY_RUNNING;
    let mut state = ProgressState::new(RunProgress::from_records(
        &lines::manifest(),
        &lines::live_running(key, "sess-running", 7),
    ));
    update(&mut state, ProgressInput::Connected(true));
    state.select_agent(key);
    update(&mut state, ProgressInput::Key(ProgressKey::StopAgent));
    update(
        &mut state,
        ProgressInput::GatewayDone {
            key: key.into(),
            result: Ok(json!({"interrupted":true})),
        },
    );
    let admission = state
        .progress
        .agent(key)
        .unwrap()
        .admission_id
        .clone()
        .unwrap();
    let seq = state.progress.last_seq;
    let effects = update(&mut state, ProgressInput::Clock { now_ms: 1000 });
    assert!(
        matches!(effects.as_slice(), [ProgressEffect::CheckStopped { session_id, admission_id, attempt:0 }]
        if session_id == "sess-running" && admission_id == &admission)
    );
    assert!(update(&mut state, ProgressInput::Clock { now_ms: 1500 }).is_empty());
    update(
        &mut state,
        ProgressInput::StopReceipt {
            session_id: "sess-running".into(),
            admission_id: admission.clone(),
            attempt: 1,
            outcome: Some("interrupted".into()),
        },
    );
    assert_eq!(
        state.progress.agent(key).unwrap().state,
        AgentState::Stopping
    );
    update(
        &mut state,
        ProgressInput::StopReceipt {
            session_id: "sess-running".into(),
            admission_id: admission.clone(),
            attempt: 0,
            outcome: None,
        },
    );
    assert!(matches!(
        update(&mut state, ProgressInput::Clock { now_ms: 2000 }).as_slice(),
        [ProgressEffect::CheckStopped { .. }]
    ));
    update(
        &mut state,
        ProgressInput::StopReceipt {
            session_id: "sess-running".into(),
            admission_id: admission,
            attempt: 0,
            outcome: Some("interrupted".into()),
        },
    );
    assert_eq!(state.progress.agent(key).unwrap().state, AgentState::Failed);
    assert_eq!(
        state.footer.as_deref(),
        Some("stopped — restart (r) runs it again")
    );
    assert_eq!(
        state.progress.last_seq, seq,
        "live evidence must not forge journal records"
    );
    assert!(update(&mut state, ProgressInput::Clock { now_ms: 3000 }).is_empty());
}

#[test]
fn dump_rows_include_launch_duration_and_keep_unknown_cost_explicit() {
    use talaria_workflow::progress::text::agent_line;
    let progress = RunProgress::from_records(&lines::manifest(), &lines::finished_lines());
    let agent = progress.agent(lines::KEY_CORRECTNESS).unwrap();
    let row = agent_line(agent, 99_000);
    assert!(row.contains("minimax-m3.1-flash on ollama-cloud"), "{row}");
    assert!(row.contains("done · 1.2s"), "{row}");
    assert!(row.contains("tokens · $0.10"), "{row}");
    let unknown = RunProgress::from_records(&lines::manifest(), &lines::cost_lines("unknown"));
    for agent in &unknown.agents {
        let row = agent_line(agent, 99_000);
        assert!(row.contains("cost unknown"));
        assert!(!row.contains("$0.00"));
    }
}

#[test]
fn bound_queued_admission_attaches_until_a_resume_observes_running() {
    use talaria_workflow::journal::Record;
    let key = lines::KEY_RUNNING;
    let mut records = lines::live_running(key, "sess-running", 7);
    for line in &mut records {
        if let Record::TurnAdmitted { status, .. } = &mut line.record {
            *status = talaria_workflow::journal::AdmissionState::Queued;
        }
    }
    let mut state = ProgressState::new(RunProgress::from_records(&lines::manifest(), &records));
    assert_eq!(state.progress.agent(key).unwrap().state, AgentState::Queued);
    assert!(
        matches!(update(&mut state, ProgressInput::Connected(true)).as_slice(),
        [ProgressEffect::Attach { session_id }] if session_id == "sess-running")
    );
    // Repeated ticks cannot pile up resumes while the first one is still pending.
    assert!(update(&mut state, ProgressInput::Tick).is_empty());
    let mut snapshot: hermes_contract::schemas::CanonicalSnapshot = serde_json::from_str(
        include_str!("../../talaria-app/tests/data/running_turn.json"),
    )
    .unwrap();
    update(
        &mut state,
        ProgressInput::Attached {
            session_id: "sess-running".into(),
            runtime_id: "runtime-running".into(),
            generation: Some(7),
        },
    );
    snapshot.running = false;
    update(
        &mut state,
        ProgressInput::Hydrated {
            session_id: "runtime-running".into(),
            snapshot: snapshot.clone(),
        },
    );
    assert!(matches!(update(&mut state, ProgressInput::Tick).as_slice(),
        [ProgressEffect::Attach { session_id }] if session_id == "sess-running"));
    snapshot.running = true;
    update(
        &mut state,
        ProgressInput::Hydrated {
            session_id: "runtime-running".into(),
            snapshot,
        },
    );
    assert_eq!(
        state.progress.agent(key).unwrap().state,
        AgentState::Running
    );
    assert!(update(&mut state, ProgressInput::Tick).is_empty());
    state.select_agent(key);
    update(&mut state, ProgressInput::Key(ProgressKey::Steer));
    assert!(state.steer_input.is_some());
    // A local queued call with no session must never attach.
    let mut unbound =
        ProgressState::new(RunProgress::from_records(&lines::manifest(), &records[..2]));
    assert!(update(&mut unbound, ProgressInput::Connected(true)).is_empty());
    assert!(update(&mut unbound, ProgressInput::Tick).is_empty());
}
