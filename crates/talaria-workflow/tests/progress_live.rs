//! Live reducer and run-picker discovery.

#[path = "support/lines.rs"]
mod lines;
#[path = "runner_support/mod.rs"]
#[allow(dead_code)]
mod runner_folders;

use hermes_contract::events::Notification;
use hermes_contract::schemas::{SessionUsagePayload, SubagentEventPayload, ToolStartPayload};
use serde_json::json;
use talaria_core::subagents::{SubagentEvent, SubagentEventKind};
use talaria_workflow::journal::{CallOutcome, Record, RecordLine};
use talaria_workflow::progress::live::{
    GatewayFailure, NodeKey, ProgressInput, ProgressKey, ProgressState, update,
};
use talaria_workflow::progress::{
    AgentState, ControlOutcome, ControlRequest, Filter, ProgressEffect, RunProgress,
};

fn seeded(rows: &[RecordLine]) -> ProgressState {
    let mut state = ProgressState::new(RunProgress::from_records(&lines::manifest(), rows));
    let _linked = update(&mut state, ProgressInput::Connected(true));
    assert!(state.connected, "journal rows are not a live connection");
    state
}

fn tool_start() -> Notification {
    Notification::ToolStart(ToolStartPayload {
        args: None,
        args_text: None,
        context: None,
        labels: None,
        name: "read".to_string(),
        preview: Some("src".to_string()),
        tool_id: "tool-1".to_string(),
    })
}

fn subagent(kind: SubagentEventKind, id: &str, status: &str) -> SubagentEvent {
    let payload = serde_json::from_value::<SubagentEventPayload>(serde_json::json!({
        "api_calls": null,
        "child_session_id": null,
        "delegation_id": null,
        "depth": 1,
        "duration_seconds": null,
        "files_read": null,
        "files_written": null,
        "goal": "review",
        "input_tokens": null,
        "model": null,
        "output_tail": null,
        "output_tokens": null,
        "parent_id": null,
        "reasoning_tokens": null,
        "status": status,
        "subagent_id": id,
        "summary": null,
        "task_count": 1,
        "task_index": 0,
        "text": null,
        "tool_count": null,
        "tool_name": null,
        "tool_preview": null,
        "toolsets": null
    }))
    .expect("subagent payload");
    SubagentEvent { kind, payload }
}

fn approval(prompt_id: &str) -> Notification {
    Notification::ApprovalRequest(hermes_contract::ApprovalRequest {
        prompt_id: prompt_id.to_string(),
        kind: None,
        execution_generation: Some(4),
        command: None,
        description: Some("read a file".to_string()),
        choices: None,
        edit: None,
        timeout_s: None,
    })
}

fn approval_settled(prompt_id: &str) -> Notification {
    Notification::ApprovalSettled(hermes_contract::ApprovalSettled {
        prompt_id: prompt_id.to_string(),
        kind: None,
        execution_generation: Some(4),
    })
}

#[test]
fn picker_renders_discovery_rows_unchanged() {
    use talaria_workflow::runner::{ProgressPicker, RunStatus, discover_runs};
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    let older = runner_folders::id(41);
    let newer = runner_folders::id(42);
    runner_folders::folder(
        &runs,
        &older,
        1_000,
        &[
            runner_folders::started(&older),
            runner_folders::finished("completed"),
        ],
    );
    runner_folders::folder(
        &runs,
        &newer,
        2_000,
        &[
            runner_folders::started(&newer),
            runner_folders::finished("failed"),
        ],
    );
    let discovered = discover_runs(&runs);
    assert_eq!(discovered.len(), 2);
    assert_eq!(
        (&discovered[0].run_id, discovered[0].status),
        (&newer, RunStatus::Failed)
    );
    assert_eq!(
        (&discovered[1].run_id, discovered[1].status),
        (&older, RunStatus::Completed)
    );
    let picker = ProgressPicker::load(&runs);
    assert_eq!(picker.rows(), discovered.as_slice());
    assert_eq!(picker.empty_message(), None);
}

#[test]
fn live_events_enrich_without_creating_agents() {
    let mut state = seeded(&lines::live_running(lines::KEY_RUNNING, "sess-running", 3));
    let _effects = update(
        &mut state,
        ProgressInput::Gateway {
            session_id: "sess-unknown".to_string(),
            event: tool_start(),
        },
    );
    assert_eq!(state.progress().agents().len(), 1);
    assert!(state.progress().agent("sess-unknown").is_none());
    assert_eq!(
        state
            .progress()
            .agent(lines::KEY_RUNNING)
            .expect("known")
            .last_tool
            .as_deref(),
        None
    );

    let _effects = update(
        &mut state,
        ProgressInput::Gateway {
            session_id: "sess-running".to_string(),
            event: tool_start(),
        },
    );
    assert_eq!(state.progress().agents().len(), 1);
    assert_eq!(
        state
            .progress()
            .agent(lines::KEY_RUNNING)
            .expect("known")
            .last_tool
            .as_deref(),
        Some("read src")
    );

    let cumulative = SessionUsagePayload {
        total_tokens: Some(140),
        ..Default::default()
    };
    let _effects = update(
        &mut state,
        ProgressInput::Gateway {
            session_id: "sess-running".to_string(),
            event: Notification::SessionUsage(cumulative),
        },
    );
    assert_eq!(
        state
            .progress()
            .agent(lines::KEY_RUNNING)
            .expect("known")
            .tokens,
        140
    );

    let effects = update(
        &mut state,
        ProgressInput::ReplayGap {
            session_id: "sess-running".to_string(),
        },
    );
    assert!(matches!(
        effects.as_slice(),
        [ProgressEffect::AttachAndReconcile { session_id }] if session_id == "sess-running"
    ));
    assert_eq!(state.progress().agents().len(), 1);
}

#[test]
fn subagents_nest_and_stay_terminal() {
    let mut state = seeded(&lines::live_running(lines::KEY_RUNNING, "sess-running", 3));
    let _effects = update(
        &mut state,
        ProgressInput::Subagent {
            session_id: "sess-running".to_string(),
            event: subagent(SubagentEventKind::Start, "child-1", "running"),
        },
    );
    let parent = state.progress().agent(lines::KEY_RUNNING).expect("parent");
    assert_eq!(parent.subagents.rows_in_order().len(), 1);
    assert_eq!(
        parent.subagents.get("child-1").expect("child").id,
        "child-1"
    );

    let _effects = update(
        &mut state,
        ProgressInput::Subagent {
            session_id: "sess-running".to_string(),
            event: subagent(SubagentEventKind::Complete, "child-1", "completed"),
        },
    );
    assert!(
        state
            .progress()
            .agent(lines::KEY_RUNNING)
            .expect("parent")
            .subagents
            .get("child-1")
            .expect("child")
            .state
            .is_terminal()
    );

    let _effects = update(
        &mut state,
        ProgressInput::Subagent {
            session_id: "sess-running".to_string(),
            event: subagent(SubagentEventKind::Start, "child-1", "running"),
        },
    );
    let row = state
        .progress()
        .agent(lines::KEY_RUNNING)
        .expect("parent")
        .subagents
        .get("child-1")
        .expect("child");
    assert!(row.state.is_terminal());
    assert_eq!(state.progress().agents().len(), 1);
    assert_eq!(state.progress().run().counts.total, 1);
}

#[test]
fn waiting_badge_follows_approval_lifecycle() {
    let mut state = seeded(&lines::live_running(lines::KEY_RUNNING, "sess-running", 4));
    let _effects = update(
        &mut state,
        ProgressInput::Gateway {
            session_id: "sess-running".to_string(),
            event: approval("prompt-1"),
        },
    );
    assert_eq!(
        state
            .progress()
            .agent(lines::KEY_RUNNING)
            .expect("row")
            .state,
        AgentState::Waiting
    );
    assert_eq!(state.progress().run().counts.needs_you, 1);

    let _effects = update(
        &mut state,
        ProgressInput::Gateway {
            session_id: "sess-running".to_string(),
            event: approval_settled("prompt-1"),
        },
    );
    assert_eq!(
        state
            .progress()
            .agent(lines::KEY_RUNNING)
            .expect("row")
            .state,
        AgentState::Running
    );
    assert_eq!(state.progress().run().counts.needs_you, 0);
}

fn effect_fields(effect: &ProgressEffect) -> (String, i64, String) {
    match effect {
        ProgressEffect::CheckStopped {
            session_id,
            admission_id,
            ..
        } => (session_id.clone(), 0, admission_id.clone()),
        ProgressEffect::Attach { session_id } => (session_id.clone(), 0, String::new()),
        ProgressEffect::AttachAndReconcile { session_id } => (session_id.clone(), 0, String::new()),
        ProgressEffect::Steer {
            session_id,
            generation,
            text,
        } => (session_id.clone(), *generation, text.clone()),
        ProgressEffect::Interrupt {
            session_id,
            generation,
        } => (session_id.clone(), *generation, String::new()),
        ProgressEffect::CancelQueued {
            session_id,
            admission_id,
        } => (session_id.clone(), 0, admission_id.clone()),
        ProgressEffect::SubSteer {
            session_id,
            subagent_id,
            text,
        } => (session_id.clone(), 0, format!("{subagent_id} {text}")),
        ProgressEffect::SubInterrupt {
            session_id,
            subagent_id,
        } => (session_id.clone(), 0, subagent_id.clone()),
        ProgressEffect::OpenSession { session_id } => (session_id.clone(), 0, String::new()),
        ProgressEffect::WriteControl(_, request) => match request {
            ControlRequest::Pause => (String::new(), 0, "pause".to_string()),
            ControlRequest::Resume => (String::new(), 0, "resume".to_string()),
            ControlRequest::Stop => (String::new(), 0, "stop".to_string()),
            ControlRequest::Restart { key } => (key.clone(), 0, "restart".to_string()),
        },
    }
}

#[test]
fn steer_and_stop_effects_carry_exact_calls() {
    let mut state = seeded(&lines::live_running(lines::KEY_RUNNING, "sess-running", 7));
    state.select_agent(lines::KEY_RUNNING);
    state.steer_input = Some("look again".to_string());
    let effects = update(&mut state, ProgressInput::Key(ProgressKey::Enter));
    let (session_id, generation, text) = effect_fields(effects.first().expect("steer"));
    assert!(matches!(
        effects.as_slice(),
        [ProgressEffect::Steer { session_id, generation, text }]
            if session_id == "sess-running" && *generation == 7 && text == "look again"
    ));
    assert_eq!(session_id, "sess-running");
    assert_eq!(generation, 7);
    assert_eq!(text, "look again");

    let acknowledged = update(
        &mut state,
        ProgressInput::GatewayDone {
            key: lines::KEY_RUNNING.to_string(),
            result: Ok(json!({ "status": "queued" })),
        },
    );
    assert!(
        acknowledged.is_empty(),
        "a queued acknowledgement is not another call, got {acknowledged:?}"
    );
    assert_eq!(
        state.footer.as_deref(),
        Some("steer queued for its next step")
    );

    let effects = update(&mut state, ProgressInput::Key(ProgressKey::StopAgent));
    assert!(matches!(
        effects.as_slice(),
        [ProgressEffect::Interrupt { session_id, generation }]
            if session_id == "sess-running" && *generation == 7
    ));
    assert_eq!(
        state
            .progress()
            .agent(lines::KEY_RUNNING)
            .expect("row")
            .state,
        AgentState::Stopping
    );
}

#[test]
fn journal_settlement_reports_whether_completion_beat_pending_interrupt() {
    for completed in [true, false] {
        let rows = lines::live_running(lines::KEY_RUNNING, "sess-running", 7);
        let mut state = seeded(&rows);
        state.select_agent(lines::KEY_RUNNING);
        assert!(matches!(
            update(&mut state, ProgressInput::Key(ProgressKey::StopAgent)).as_slice(),
            [ProgressEffect::Interrupt { generation: 7, .. }]
        ));
        assert_eq!(
            state.progress().agent(lines::KEY_RUNNING).unwrap().state,
            AgentState::Stopping
        );

        // The journal can settle before the interrupt RPC acknowledges.
        update(
            &mut state,
            ProgressInput::Journal(vec![RecordLine {
                v: 1,
                seq: rows.last().unwrap().seq + 1,
                at_ms: 9_000,
                record: Record::CallFinished {
                    key: lines::KEY_RUNNING.into(),
                    attempt: 0,
                    turns: 1,
                    outcome: if completed {
                        CallOutcome::Completed {
                            value: Some(json!("done")),
                            value_ref: None,
                        }
                    } else {
                        CallOutcome::Failed {
                            reason: "interrupted".into(),
                        }
                    },
                },
            }]),
        );
        assert_eq!(
            state.progress().agent(lines::KEY_RUNNING).unwrap().state,
            if completed {
                AgentState::Done
            } else {
                AgentState::Failed
            }
        );
        assert_eq!(
            state.footer.as_deref(),
            Some(if completed {
                "finished before stop"
            } else {
                "stopped — restart (r) runs it again"
            })
        );
    }
}

fn stale_failure() -> GatewayFailure {
    GatewayFailure {
        reason: "stale_generation".to_string(),
        stale_generation: true,
        disconnected: false,
    }
}

#[test]
fn stale_generation_refreshes_once() {
    let mut state = seeded(&lines::live_running(lines::KEY_RUNNING, "sess-running", 7));
    state.select_agent(lines::KEY_RUNNING);
    let sent = update(&mut state, ProgressInput::Key(ProgressKey::StopAgent));
    assert!(matches!(
        sent.as_slice(),
        [ProgressEffect::Interrupt { session_id, generation }]
            if session_id == "sess-running" && *generation == 7
    ));
    let first = update(
        &mut state,
        ProgressInput::GatewayDone {
            key: lines::KEY_RUNNING.to_string(),
            result: Err(stale_failure()),
        },
    );
    assert!(matches!(
        first.as_slice(),
        [ProgressEffect::Attach { session_id }] if session_id == "sess-running"
    ));
    let retry = update(
        &mut state,
        ProgressInput::Attached {
            session_id: "sess-running".to_string(),
            runtime_id: "sess-running".to_string(),
            generation: Some(8),
        },
    );
    assert!(matches!(
        retry.as_slice(),
        [ProgressEffect::Interrupt { session_id, generation }]
            if session_id == "sess-running" && *generation == 8
    ));
    let second = update(
        &mut state,
        ProgressInput::GatewayDone {
            key: lines::KEY_RUNNING.to_string(),
            result: Err(stale_failure()),
        },
    );
    assert!(second.is_empty());
    assert_eq!(
        state.footer.as_deref(),
        Some("generation unknown — controls unavailable until it next speaks")
    );
    assert!(
        state
            .progress()
            .agent(lines::KEY_RUNNING)
            .expect("row")
            .controls_disabled
    );
    let blocked = update(&mut state, ProgressInput::Key(ProgressKey::StopAgent));
    assert!(blocked.is_empty());
}

#[test]
fn restart_never_touches_finished_work() {
    let mut state = seeded(&lines::live_restart());
    state.select_agent(lines::KEY_FAILED);
    let armed = update(&mut state, ProgressInput::Key(ProgressKey::RestartAgent));
    assert!(armed.is_empty());
    assert_eq!(
        state.confirmation_text(),
        Some("restart? r or y confirms · esc cancels")
    );
    let effects = update(&mut state, ProgressInput::Key(ProgressKey::Confirm));
    let id = match effects.as_slice() {
        [ProgressEffect::WriteControl(id, ControlRequest::Restart { key })]
            if key == lines::KEY_FAILED =>
        {
            *id
        }
        other => panic!("expected a confirmed restart request, got {other:?}"),
    };
    let _published = update(
        &mut state,
        ProgressInput::ControlDone(id, ControlOutcome::Published),
    );
    assert_eq!(
        state.footer.as_deref(),
        Some("restart requested — runs again on resume")
    );
    state.select_agent(lines::KEY_CORRECTNESS);
    let refused = update(&mut state, ProgressInput::Key(ProgressKey::RestartAgent));
    assert!(refused.is_empty());
    assert_eq!(
        state.footer.as_deref(),
        Some("finished agents are kept — restart would pay for the work twice")
    );
    assert_eq!(state.confirmation_text(), None);
}

#[test]
fn filter_keeps_needs_you_ancestors() {
    let mut state = seeded(&lines::live_filter());
    assert_eq!(state.filter, Filter::All);
    let _effects = update(&mut state, ProgressInput::Key(ProgressKey::CycleFilter));
    assert_eq!(state.filter, Filter::Working);
    let _effects = update(&mut state, ProgressInput::Key(ProgressKey::CycleFilter));
    assert_eq!(state.filter, Filter::NeedsYou);
    let visible = state.visible_nodes();
    assert!(
        visible
            .iter()
            .any(|node| { matches!(&node.key, NodeKey::Phase { title, .. } if title == "plan") })
    );
    assert!(
        visible
            .iter()
            .any(|node| { matches!(&node.key, NodeKey::Agent(key) if key == lines::KEY_PARKED) })
    );
    assert!(
        visible
            .iter()
            .all(|node| { !matches!(&node.key, NodeKey::Agent(key) if key == lines::KEY_RUNNING) })
    );
}

#[test]
fn picker_agrees_with_runner_discovery() {
    use talaria_workflow::runner::{ProgressPicker, RunStatus, discover_runs};
    let temp = tempfile::tempdir().unwrap();
    let runs = temp.path().join("runs");
    assert!(discover_runs(&runs).is_empty());
    let empty = ProgressPicker::load(&runs);
    assert!(empty.rows().is_empty());
    assert_eq!(
        empty.empty_message(),
        Some("no runs yet — talaria workflow run starts one")
    );
    assert!(!runs.exists());

    let finished = runner_folders::id(10);
    let interrupted = runner_folders::id(11);
    runner_folders::folder(
        &runs,
        &finished,
        1_000,
        &[
            runner_folders::started(&finished),
            runner_folders::finished("completed"),
        ],
    );
    runner_folders::folder(
        &runs,
        &interrupted,
        2_000,
        &[runner_folders::started(&interrupted)],
    );
    let broken = runs.join(runner_folders::id(12));
    std::fs::create_dir(&broken).unwrap();
    std::fs::write(broken.join("manifest.json"), b"not-json").unwrap();
    let discovered = discover_runs(&runs);
    assert_eq!(discovered.len(), 3);
    assert!(
        discovered
            .iter()
            .any(|row| row.run_id == interrupted && row.status == RunStatus::Interrupted)
    );
    assert!(
        discovered
            .iter()
            .any(|row| row.run_id == finished && row.status == RunStatus::Completed)
    );
    assert!(
        discovered
            .iter()
            .any(|row| row.run_dir == broken.to_string_lossy()
                && row.status == RunStatus::Unreadable
                && row.warning.is_some())
    );
    let picker = ProgressPicker::load(&runs);
    assert_eq!(picker.rows(), discovered.as_slice());
    assert_eq!(picker.empty_message(), None);
}
