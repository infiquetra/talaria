//! The automation service against the mock gateway. Behavior contract: issue 236.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::{Duration, Instant};

use futures_util::future::join_all;
use hermes_client::replay::Cursor;
use serde_json::{Value, json};
use talaria_app::app::{self, AppEffect, AppMessage, AppState};
use talaria_app::automation::{AutomationService, AutomationTicker, automation_actions};
use talaria_app::commands::{AppAction, CommandService};
use talaria_app::sessions::{OpenedSession, SessionInfoSummary};
use talaria_core::automation::{
    AutomationEffect, AutomationEvent, AutomationState, CommandResult, ModeKey, PRESENTED_COMMANDS,
    Presentation, ReadFailure, canonical_name, present_command, update,
};
use talaria_core::commands::{CatalogResult, CommandCatalog, CommandOutcome};
use talaria_core::session::{Cell, Effect, Event, SessionId, SessionState};

const SESSION: &str = "local-session-1";

fn null_snapshot() -> Value {
    json!({"goal": null, "loop": null, "heartbeat": null, "revision": "", "updated_at": 0})
}

fn param_keys(params: &Value) -> Vec<String> {
    let mut keys: Vec<String> = params
        .as_object()
        .expect("object params")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

#[tokio::test]
async fn control_read_params_and_timeout() {
    // Keys exactly session_id; the snapshot decodes.
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_control_snapshot(null_snapshot());
    let service = AutomationService::new(client);
    let event = service
        .execute(AutomationEffect::ReadControl {
            session_id: SESSION.to_owned(),
            token: 7,
        })
        .await;
    match event {
        Some(AutomationEvent::ControlRead { token, result }) => {
            assert_eq!(token, 7);
            let view = result.expect("the snapshot decodes");
            assert_eq!(view.revision, "");
            assert!(view.goal.is_none() && view.loop_.is_none() && view.heartbeat.is_none());
        }
        other => panic!("a control read answers a ControlRead, got {other:?}"),
    }
    let reads = mock.requests("session.control.read");
    assert_eq!(reads.len(), 1);
    assert_eq!(param_keys(&reads[0].params), ["session_id"]);
    assert_eq!(
        reads[0].params.get("session_id").and_then(Value::as_str),
        Some(SESSION)
    );
    assert!(mock.violations().is_empty());

    // Slow: a mock that never answers runs the 5 s timeout on a real socket, so
    // the clock is never paused here.
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_behavior("session.control.read", mock_gateway::Behavior::Silent);
    let service = AutomationService::new(client);
    let started = Instant::now();
    let event = service
        .execute(AutomationEffect::ReadControl {
            session_id: SESSION.to_owned(),
            token: 8,
        })
        .await;
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_secs(5) && waited < Duration::from_millis(6500),
        "the timeout fires at 5 s, waited {waited:?}"
    );
    assert!(
        matches!(
            event,
            Some(AutomationEvent::ControlRead {
                token: 8,
                result: Err(ReadFailure::TimedOut),
            })
        ),
        "silence maps to TimedOut, got {event:?}"
    );

    // An older Hermes answers -32601.
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    let service = AutomationService::new(client);
    let event = service
        .execute(AutomationEffect::ReadControl {
            session_id: SESSION.to_owned(),
            token: 9,
        })
        .await;
    assert!(
        matches!(
            event,
            Some(AutomationEvent::ControlRead {
                result: Err(ReadFailure::MethodMissing),
                ..
            })
        ),
        "an older Hermes maps to MethodMissing, got {event:?}"
    );
    assert!(mock.violations().is_empty());

    // A refusal carries its reason.
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.queue_behavior(
        "session.control.read",
        mock_gateway::Behavior::Refuse("not_found".to_owned()),
    );
    let service = AutomationService::new(client);
    let event = service
        .execute(AutomationEffect::ReadControl {
            session_id: SESSION.to_owned(),
            token: 10,
        })
        .await;
    match &event {
        Some(AutomationEvent::ControlRead {
            result: Err(ReadFailure::Refused(reason)),
            ..
        }) => assert_eq!(reason, "not_found"),
        other => panic!("a refusal maps to Refused, got {other:?}"),
    }

    // A closing connection is a lost read.
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_control_snapshot(null_snapshot());
    mock.queue_behavior(
        "session.control.read",
        mock_gateway::Behavior::DropBeforeReply,
    );
    let service = AutomationService::new(client);
    let event = service
        .execute(AutomationEffect::ReadControl {
            session_id: SESSION.to_owned(),
            token: 11,
        })
        .await;
    assert!(
        matches!(
            event,
            Some(AutomationEvent::ControlRead {
                result: Err(ReadFailure::Lost),
                ..
            })
        ),
        "a close maps to Lost, got {event:?}"
    );
    assert!(mock.violations().is_empty());
}

#[tokio::test]
async fn config_get_params() {
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_config_value("yolo", json!("1"));
    mock.set_config_value("fast", json!("normal"));
    let service = AutomationService::new(client);

    let event = service
        .execute(AutomationEffect::ReadMode {
            session_id: SESSION.to_owned(),
            key: ModeKey::Yolo,
            token: 3,
        })
        .await;
    match &event {
        Some(AutomationEvent::ModeRead {
            key: ModeKey::Yolo,
            token: 3,
            result: Ok(Some(value)),
            ..
        }) => assert_eq!(value, "1"),
        other => panic!("yolo reads back, got {other:?}"),
    }
    let event = service
        .execute(AutomationEffect::ReadMode {
            session_id: SESSION.to_owned(),
            key: ModeKey::Fast,
            token: 4,
        })
        .await;
    match &event {
        Some(AutomationEvent::ModeRead {
            key: ModeKey::Fast,
            token: 4,
            result: Ok(Some(value)),
            ..
        }) => assert_eq!(value, "normal"),
        other => panic!("fast reads back, got {other:?}"),
    }

    let gets = mock.requests("config.get");
    assert_eq!(gets.len(), 2);
    for seen in &gets {
        assert_eq!(param_keys(&seen.params), ["key", "session_id"]);
        assert_eq!(
            seen.params.get("session_id").and_then(Value::as_str),
            Some(SESSION)
        );
    }
    assert_eq!(
        gets[0].params.get("key").and_then(Value::as_str),
        Some("yolo")
    );
    assert_eq!(
        gets[1].params.get("key").and_then(Value::as_str),
        Some("fast")
    );
    assert!(mock.violations().is_empty());

    // An unscripted key reads as a missing method.
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    let service = AutomationService::new(client);
    let event = service
        .execute(AutomationEffect::ReadMode {
            session_id: SESSION.to_owned(),
            key: ModeKey::Yolo,
            token: 5,
        })
        .await;
    assert!(
        matches!(
            event,
            Some(AutomationEvent::ModeRead {
                result: Err(ReadFailure::MethodMissing),
                ..
            })
        ),
        "an unscripted key maps to MethodMissing, got {event:?}"
    );
    assert!(mock.violations().is_empty());
}

#[test]
fn idle_app_wakes_and_draws_nothing() {
    const NOW: u64 = 1790000000000;
    let mut ticker = AutomationTicker::default();

    // No automation state: no wake-up, and the second call redraws nothing.
    let state = AutomationState::default();
    let wake = ticker.on_wake(&state, NOW);
    assert_eq!(wake.next_in, None);
    let wake = ticker.on_wake(&state, NOW + 30_000);
    assert!(!wake.redraw);
    assert_eq!(wake.next_in, None);

    // A loop due in 45 minutes: three redraws over the next three minutes.
    let mut state = AutomationState::default();
    update(
        &mut state,
        AutomationEvent::Attached {
            session_id: SESSION.to_owned(),
        },
    );
    update(
        &mut state,
        AutomationEvent::ControlUpdated {
            session_id: SESSION.to_owned(),
            raw: json!({
                "goal": null,
                "heartbeat": null,
                "loop": {
                    "prompt": "check the build",
                    "status": "active",
                    "mode": "interval",
                    "interval_seconds": 2700,
                    "current_delay": 0,
                    "times": 0,
                    "until": "",
                    "max_ticks": 0,
                    "ticks_fired": 0,
                    "created_at": 1790000000,
                    "last_fired_at": 0,
                    "next_due_at": 1790002700,
                    "awaiting_response": false,
                    "deferred_by_goal": false
                },
                "revision": "r1",
                "updated_at": 1790000000
            }),
        },
    );
    let mut ticker = AutomationTicker::default();
    let mut now = NOW;
    ticker.on_wake(&state, now);
    let mut wakes = 0;
    loop {
        let ask = ticker.on_wake(&state, now);
        assert!(!ask.redraw, "asking twice at one instant redraws nothing");
        let Some(next) = ask.next_in else {
            panic!("a shown countdown always schedules its next change");
        };
        now += next.as_millis() as u64;
        if now > NOW + 180_000 {
            break;
        }
        let wake = ticker.on_wake(&state, now);
        wakes += 1;
        assert!(wake.redraw, "every wake changes the text");
        assert_eq!(wake.next_in, Some(Duration::from_secs(60)));
    }
    assert_eq!(wakes, 3, "three redraws: 44m, 43m, 42m");
}

fn automation_catalog() -> CommandCatalog {
    let pairs: Vec<Value> = PRESENTED_COMMANDS
        .iter()
        .map(|name| json!([format!("/{name}"), format!("{name} help")]))
        .collect();
    let mut commands = serde_json::Map::new();
    for name in PRESENTED_COMMANDS {
        commands.insert(format!("/{name}"), json!({"argument_mode": "text"}));
    }
    let result: CatalogResult = serde_json::from_value(json!({
        "pairs": pairs,
        "canon": {"/proactive": "/loop", "/hb": "/heartbeat"},
        "commands": commands,
        "categories": [],
        "skills": {},
    }))
    .expect("the catalogue decodes");
    CommandCatalog::from_result(&result)
}

fn presented_line(name: &str) -> String {
    match name {
        "plan" => "/plan write a haiku about rain".to_owned(),
        "goal" => "/goal ship the release".to_owned(),
        "subgoal" => "/subgoal draft the migration notes".to_owned(),
        "loop" => "/loop 45m standup digest".to_owned(),
        "heartbeat" => "/heartbeat standup digest".to_owned(),
        "reasoning" => "/reasoning brief".to_owned(),
        "busy" => "/busy off".to_owned(),
        "verbose" => "/verbose on".to_owned(),
        "footer" => "/footer short".to_owned(),
        "approvals" => "/approvals list".to_owned(),
        "personality" => "/personality default".to_owned(),
        "codex-runtime" => "/codex-runtime status".to_owned(),
        _ => format!("/{name}"),
    }
}

async fn drain_effects(
    service: &AutomationService,
    state: &mut AutomationState,
    mut pending: Vec<AutomationEffect>,
) {
    while let Some(effect) = pending.pop() {
        if let Some(feedback) = service.execute(effect).await {
            pending.extend(update(state, feedback));
        }
    }
}

fn assert_clean_tape(seen: &[mock_gateway::Seen]) {
    for frame in seen {
        let Some(method) = frame.method.as_deref() else {
            continue;
        };
        assert!(
            !matches!(
                method,
                "config.set" | "session.control.pause" | "session.control.resume"
            ),
            "no scope-changing call, saw {method}"
        );
        if method == "prompt.submit" {
            assert_eq!(
                param_keys(&frame.params),
                ["session_id", "submission_id", "text"]
            );
            assert!(
                frame.params.get("unattended").is_none(),
                "no unattended flag"
            );
        }
        if matches!(method, "session.control.read" | "config.get") {
            assert_eq!(
                frame.params.get("session_id").and_then(Value::as_str),
                Some(SESSION)
            );
        }
    }
}

/// Every presented command, run for real: nothing in the run may set the
/// unattended flag or call a scope-changing method.
#[tokio::test]
async fn presented_commands_stay_inside_their_scope() {
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_control_snapshot(null_snapshot());
    let catalog = automation_catalog();
    let mut commands = CommandService::new(client.clone());
    let mut lines: Vec<String> = PRESENTED_COMMANDS
        .iter()
        .map(|name| presented_line(name))
        .collect();
    lines.push("/proactive 45m standup digest".to_owned());
    lines.push("/hb status".to_owned());
    let mut runs: Vec<(String, CommandResult)> = Vec::new();
    for typed in &lines {
        let outcome = commands.execute(SESSION, typed).await;
        let canonical =
            canonical_name(typed, Some(&catalog)).unwrap_or_else(|| panic!("{typed} is presented"));
        let presentation = present_command(&canonical, typed, &outcome);
        let result = match &outcome {
            CommandOutcome::Submit { .. } => {
                assert!(
                    matches!(presentation, Some(Presentation::Submit { .. })),
                    "{typed} queues a turn"
                );
                CommandResult::Submit
            }
            CommandOutcome::Output { .. } => {
                assert!(
                    matches!(presentation, Some(Presentation::Output(_))),
                    "{typed} shows a cell"
                );
                CommandResult::Output
            }
            CommandOutcome::OutcomeUnknown { .. } => CommandResult::Unknown,
            _ => CommandResult::NotRun,
        };
        runs.push((canonical, result));
    }
    let service = AutomationService::new(client);
    let mut state = AutomationState::default();
    for (canonical, result) in runs {
        let pending = update(
            &mut state,
            AutomationEvent::CommandFinished {
                session_id: SESSION.to_owned(),
                canonical,
                result,
            },
        );
        drain_effects(&service, &mut state, pending).await;
    }
    let seen = mock.seen();
    let submits: Vec<&mock_gateway::Seen> = seen
        .iter()
        .filter(|frame| frame.method.as_deref() == Some("prompt.submit"))
        .collect();
    assert_eq!(submits.len(), 1, "only /plan queues a turn here");
    assert!(
        submits[0].params["text"]
            .as_str()
            .unwrap_or_default()
            .contains("write a haiku about rain"),
        "the queued turn carries the payload"
    );
    assert_clean_tape(&seen);
    assert!(mock.violations().is_empty());
}

/// `/yolo` runs, then attach lists nothing new: the attach listing stays the
/// control probe plus the two config reads, with no unattended flag anywhere.
#[tokio::test]
async fn attach_listing_after_yolo_carries_no_new_requests() {
    let (mock, client, _inbound) = mock_gateway::MockGateway::start().await;
    mock.set_control_snapshot(null_snapshot());
    let catalog = automation_catalog();
    let mut commands = CommandService::new(client.clone());

    let typed = "/yolo";
    let outcome = commands.execute(SESSION, typed).await;
    let canonical = canonical_name(typed, Some(&catalog)).expect("/yolo is presented");
    let presentation = present_command(&canonical, typed, &outcome);
    assert!(matches!(presentation, Some(Presentation::Output(_))));
    let service = AutomationService::new(client);
    let mut state = AutomationState::default();
    let pending = update(
        &mut state,
        AutomationEvent::CommandFinished {
            session_id: SESSION.to_owned(),
            canonical,
            result: CommandResult::Output,
        },
    );
    drain_effects(&service, &mut state, pending).await;

    let mark = mock.seen().len();
    mock.set_hold_automation_reads();
    let pending = update(
        &mut state,
        AutomationEvent::Attached {
            session_id: SESSION.to_owned(),
        },
    );
    let (reads, rest): (Vec<_>, Vec<_>) = pending.into_iter().partition(|effect| {
        matches!(
            effect,
            AutomationEffect::ReadControl { .. } | AutomationEffect::ReadMode { .. }
        )
    });
    assert_eq!(reads.len(), 3, "attach reads everything once");
    for effect in rest {
        let _ = service.execute(effect).await;
    }
    let answers = join_all(reads.into_iter().map(|effect| service.execute(effect))).await;
    for feedback in answers.into_iter().flatten() {
        for effect in update(&mut state, feedback) {
            let _ = service.execute(effect).await;
        }
    }

    let seen = mock.seen();
    let mut methods: Vec<&str> = seen[mark..]
        .iter()
        .filter_map(|frame| frame.method.as_deref())
        .collect();
    methods.sort_unstable();
    assert_eq!(
        methods,
        ["config.get", "config.get", "session.control.read"]
    );
    let mut keys: Vec<&str> = seen[mark..]
        .iter()
        .filter(|frame| frame.method.as_deref() == Some("config.get"))
        .filter_map(|frame| frame.params.get("key").and_then(Value::as_str))
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["fast", "yolo"]);
    assert_clean_tape(&seen);
    assert!(mock.violations().is_empty());
}

/// Finished presented commands map to their session actions; anything else
/// stays on the existing path.
#[test]
fn presented_outcomes_map_to_automation_actions() {
    let catalog = automation_catalog();
    let typed = "/yolo";
    let outcome = CommandOutcome::Output {
        line: typed.to_owned(),
        text: "YOLO mode ON for this session".to_owned(),
    };
    let actions = automation_actions(typed, &outcome, Some(&catalog));
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        AppAction::Event(Event::HermesCommand { cell }) => {
            assert_eq!(cell.name, "yolo");
            assert_eq!(cell.typed, typed);
            assert!(cell.body.contains("ON"));
        }
        _ => panic!("an exec reply commits a command cell"),
    }

    let typed = "/plan write a haiku about rain";
    let outcome = CommandOutcome::Submit {
        text: "the plan prompt".to_owned(),
        display: typed.to_owned(),
        notice: Some("Planning: write a haiku about rain".to_owned()),
    };
    let actions = automation_actions(typed, &outcome, Some(&catalog));
    assert_eq!(actions.len(), 2, "the cell plus the resubmission");
    match &actions[0] {
        AppAction::Event(Event::HermesCommand { cell }) => {
            assert_eq!(cell.name, "plan");
        }
        _ => panic!("a queued turn commits a command cell"),
    }
    match &actions[1] {
        AppAction::Event(Event::SubmitCommand { text, display, .. }) => {
            assert_eq!(text, "the plan prompt");
            assert_eq!(display, typed);
        }
        _ => panic!("a queued turn resubmits"),
    }

    let typed = "/proactive 45m standup digest";
    let outcome = CommandOutcome::Output {
        line: typed.to_owned(),
        text: "standing by".to_owned(),
    };
    let actions = automation_actions(typed, &outcome, Some(&catalog));
    assert_eq!(actions.len(), 1);
    assert!(
        matches!(
            &actions[0],
            AppAction::Event(Event::HermesCommand { cell }) if cell.name == "loop"
        ),
        "an alias resolves to its canonical name"
    );

    let typed = "/loop 45m standup digest";
    let outcome = CommandOutcome::Failed {
        message: "the agent is running; wait or /stop first".to_owned(),
    };
    assert!(
        automation_actions(typed, &outcome, Some(&catalog)).is_empty(),
        "a refusal keeps the existing path"
    );

    for typed in ["/new", "/help", "/version", "/"] {
        let outcome = CommandOutcome::Output {
            line: typed.to_owned(),
            text: "something".to_owned(),
        };
        assert!(
            automation_actions(typed, &outcome, Some(&catalog)).is_empty(),
            "{typed} is not presented"
        );
    }
    assert!(
        automation_actions("/loop 45m x", &CommandOutcome::Nothing, Some(&catalog)).is_empty(),
        "nothing never maps"
    );
}

fn opened_state() -> AppState {
    let mut state = AppState::new("red-run");
    state.opened = Some(OpenedSession {
        state: SessionState::empty(SessionId::new(SESSION)),
        session_id: SESSION.to_owned(),
        stored_session_id: SESSION.to_owned(),
        subscription_id: "sub-1".to_owned(),
        info: SessionInfoSummary {
            model: None,
            provider: None,
            cwd: None,
            branch: None,
        },
        title: None,
        cursor: Cursor {
            replay_epoch: String::new(),
            last_sequence: 0,
        },
        revision: 0,
        execution_generation: 0,
        launch_notices: Vec::new(),
        effort: None,
    });
    state
}

fn opened_cells(state: &AppState) -> &[Cell] {
    state
        .opened
        .as_ref()
        .expect("an opened session")
        .state
        .cells()
}

/// A finished presented command lands as a command cell; a queued turn still
/// resubmits, without its notice. Unpresented lines keep the existing path.
#[test]
fn finished_presented_commands_commit_their_cell() {
    let mut state = opened_state();
    let typed = "/yolo".to_owned();
    let outcome = CommandOutcome::Output {
        line: typed.clone(),
        text: "YOLO mode ON for this session".to_owned(),
    };
    let effects = app::update(
        &mut state,
        AppMessage::CommandDone {
            typed: typed.clone(),
            outcome,
        },
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Send(_))),
        "an exec reply never submits"
    );
    let cells = opened_cells(&state);
    assert!(
        cells
            .iter()
            .any(|cell| matches!(cell, Cell::HermesCommand(cmd) if cmd.name == "yolo")),
        "the command cell lands, got {cells:?}"
    );
    assert!(
        !cells.iter().any(|cell| matches!(cell, Cell::Notice(_))),
        "no notice cell"
    );

    let mut state = opened_state();
    let typed = "/plan write a haiku about rain".to_owned();
    let outcome = CommandOutcome::Submit {
        text: "the plan prompt".to_owned(),
        display: typed.clone(),
        notice: Some("Planning: write a haiku about rain".to_owned()),
    };
    let effects = app::update(
        &mut state,
        AppMessage::CommandDone {
            typed: typed.clone(),
            outcome,
        },
    );
    let sent: Vec<&str> = effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::Send(Effect::SendPrompt { text, .. }) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(sent, ["the plan prompt"]);
    let cells = opened_cells(&state);
    assert!(
        cells
            .iter()
            .any(|cell| matches!(cell, Cell::HermesCommand(cmd) if cmd.name == "plan")),
        "the queued turn commits its cell, got {cells:?}"
    );
    assert!(
        !cells.iter().any(|cell| matches!(cell, Cell::Notice(_))),
        "the send notice is dropped"
    );

    let mut state = opened_state();
    let outcome = CommandOutcome::Output {
        line: "/version".to_owned(),
        text: "Hermes Agent v0.22.4".to_owned(),
    };
    let _effects = app::update(
        &mut state,
        AppMessage::CommandDone {
            typed: "/version".to_owned(),
            outcome,
        },
    );
    let cells = opened_cells(&state);
    assert!(
        !cells
            .iter()
            .any(|cell| matches!(cell, Cell::HermesCommand(_))),
        "unpresented lines commit no command cell"
    );
    assert!(
        cells.iter().any(|cell| matches!(cell, Cell::Notice(_))),
        "unpresented output keeps its notice"
    );
}
