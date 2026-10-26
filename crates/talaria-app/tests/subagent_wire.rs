//! Red-run tests 19 to 31, 34 to 37, 53 and 54 (#242): the hub on the wire.
//!
//! The hub runs against the mock gateway through the real client, on tokio's
//! paused time. Answers travel the wire into `rx` and the tests feed them
//! back through `on_message`, so generations always match. Red: the stub hub
//! sends nothing and returns no outcome, and every test asserts at least one
//! positive result (a recorded request, an outcome text, a row state).

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::Duration;

use hermes_client::Client;
use hermes_contract::events::{EventFrame, Notification};
use mock_gateway::{Behavior, MockGateway, SESSION_ID};
use serde_json::{Value, json};
use talaria_app::app::{AppMessage, UserAction};
use talaria_app::subagents::{
    ControlOutcome, SubagentAction, SubagentHub, SubagentMsg, SubagentsConfig, outcome_text,
};
use talaria_core::directory::rows::RowKey;
use talaria_core::subagents::{Clock, RowState};
use tokio::sync::mpsc;

fn key() -> RowKey {
    RowKey {
        profile: "fixture".to_owned(),
        session_id: SESSION_ID.to_owned(),
    }
}

fn clock_ms(ms: u64) -> Clock {
    Clock {
        mono_ms: ms,
        wall_s: ms as f64 / 1000.0,
    }
}

async fn rig() -> (
    MockGateway,
    Client,
    SubagentHub,
    mpsc::UnboundedReceiver<AppMessage>,
) {
    let (gateway, client, _inbound) = MockGateway::start().await;
    let (tx, rx) = mpsc::unbounded_channel();
    let hub = SubagentHub::new(SubagentsConfig::default(), tx);
    (gateway, client, hub, rx)
}

/// What one drain saw: hub messages (fed back through `on_message`) and
/// operator notices.
#[derive(Default)]
struct Drained {
    hub_msgs: Vec<SubagentMsg>,
    notices: Vec<String>,
}

fn drain(
    hub: &mut SubagentHub,
    rx: &mut mpsc::UnboundedReceiver<AppMessage>,
    now: Clock,
) -> Drained {
    let mut out = Drained::default();
    while let Ok(msg) = rx.try_recv() {
        match msg {
            AppMessage::Subagents(inner) => {
                hub.on_message(inner.clone(), now);
                out.hub_msgs.push(inner);
            }
            AppMessage::User(UserAction::Notice(text)) => out.notices.push(text),
            _ => {}
        }
    }
    out
}

/// Wait until the mock records a call, then drain whatever the hub emitted.
/// The clock moves only while waiting.
async fn wait_call(
    hub: &mut SubagentHub,
    rx: &mut mpsc::UnboundedReceiver<AppMessage>,
    gateway: &MockGateway,
    method: &str,
    now_ms: &mut u64,
) -> Drained {
    let expected = gateway.next_expected_request(method);
    let mut out = Drained::default();
    for _ in 0..200 {
        append(&mut out, drain(hub, rx, clock_ms(*now_ms)));
        if gateway.requests(method).len() >= expected {
            break;
        }
        tokio::time::advance(Duration::ZERO).await;
        tokio::task::yield_now().await;
    }
    assert!(
        gateway.requests(method).len() >= expected,
        "the hub sent request {expected} for {method}"
    );
    for _ in 0..50 {
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::ZERO).await;
        append(&mut out, drain(hub, rx, clock_ms(*now_ms)));
    }
    append(&mut out, drain(hub, rx, clock_ms(*now_ms)));
    out
}

fn append(out: &mut Drained, mut more: Drained) {
    out.hub_msgs.append(&mut more.hub_msgs);
    out.notices.append(&mut more.notices);
}

async fn settle(
    hub: &mut SubagentHub,
    rx: &mut mpsc::UnboundedReceiver<AppMessage>,
    now_ms: &mut u64,
) -> Drained {
    let mut out = Drained::default();
    for _ in 0..50 {
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::ZERO).await;
        append(&mut out, drain(hub, rx, clock_ms(*now_ms)));
    }
    append(&mut out, drain(hub, rx, clock_ms(*now_ms)));
    out
}

fn notification(kind: &str, payload: Value) -> Notification {
    let params = json!({
        "type": kind,
        "session_id": SESSION_ID,
        "seq": 1,
        "replay_epoch": "epoch",
        "payload": payload,
    });
    EventFrame::from_params(&params)
        .expect("contract-shaped params")
        .notification
}

fn payload(id: &str) -> Value {
    json!({
        "subagent_id": id,
        "goal": format!("goal {id}"),
        "task_index": 0,
        "task_count": 1,
    })
}

fn with(payload: Value, key: &str, value: Value) -> Value {
    let mut object = payload.as_object().cloned().expect("object");
    object.insert(key.to_owned(), value);
    Value::Object(object)
}

fn without(payload: Value, key: &str) -> Value {
    let mut object = payload.as_object().cloned().expect("object");
    object.remove(key);
    Value::Object(object)
}

fn live_child(hub: &mut SubagentHub, key: &RowKey, id: &str, now: Clock) {
    let started = notification("subagent.start", payload(id));
    assert!(hub.on_notification(key, &started, now));
}

fn params_of(gateway: &MockGateway, method: &str, index: usize) -> Value {
    gateway.requests(method)[index].params.clone()
}

#[tokio::test(start_paused = true)]
async fn hydrate_sends_exact_list_params() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    hub.attach(key.clone(), client);
    let drained = wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    assert_eq!(gateway.requests("subagent.list").len(), 1);
    assert_eq!(
        params_of(&gateway, "subagent.list", 0),
        json!({"session_id": SESSION_ID})
    );
    assert!(
        drained
            .hub_msgs
            .iter()
            .any(|msg| matches!(msg, SubagentMsg::Listed { .. })),
        "attach lists at once"
    );

    hub.on_reconnected(&key);
    let drained = wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    assert_eq!(gateway.requests("subagent.list").len(), 2);
    assert_eq!(
        params_of(&gateway, "subagent.list", 1),
        json!({"session_id": SESSION_ID})
    );
    assert!(
        drained
            .hub_msgs
            .iter()
            .any(|msg| matches!(msg, SubagentMsg::Listed { .. })),
        "a reconnect lists again"
    );

    hub.open_panel(key.clone());
    let drained = wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    assert_eq!(gateway.requests("subagent.list").len(), 3);
    assert_eq!(
        params_of(&gateway, "subagent.list", 2),
        json!({"session_id": SESSION_ID})
    );
    assert!(
        drained
            .hub_msgs
            .iter()
            .any(|msg| matches!(msg, SubagentMsg::Listed { .. })),
        "opening the panel lists again"
    );
}

#[tokio::test(start_paused = true)]
async fn poll_only_while_live_or_open() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    assert_eq!(gateway.requests("subagent.list").len(), 1);

    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));
    for expected in [2, 3] {
        gateway.push_result(json!({"subagents": [], "delegations": []}));
        tokio::time::advance(Duration::from_secs(5)).await;
        now_ms += 5_000;
        wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
        assert_eq!(gateway.requests("subagent.list").len(), expected);
    }

    let done = notification(
        "subagent.complete",
        with(payload("child-1"), "status", json!("completed")),
    );
    hub.on_notification(&key, &done, clock_ms(now_ms));
    hub.close_panel();
    tokio::time::advance(Duration::from_secs(30)).await;
    now_ms += 30_000;
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(
        gateway.requests("subagent.list").len(),
        3,
        "idle and closed: no call in 30 s"
    );

    live_child(&mut hub, &key, "child-2", clock_ms(now_ms));
    gateway.queue_behavior(
        "subagent.list",
        Behavior::RefuseCode {
            reason: "invalid_params".to_owned(),
            code: 4001,
        },
    );
    gateway.queue_behavior(
        "subagent.list",
        Behavior::RefuseCode {
            reason: "invalid_params".to_owned(),
            code: 4001,
        },
    );
    gateway.set_behavior("subagent.list", Behavior::Silent);
    let before = gateway.requests("subagent.list").len();
    // Events arm the periodic list; they are not an immediate-list trigger.
    // Three polls are needed for two refusals and the unanswered third call.
    for _ in 0..3 {
        tokio::time::advance(Duration::from_secs(5)).await;
        now_ms += 5_000;
        settle(&mut hub, &mut rx, &mut now_ms).await;
    }
    assert_eq!(
        gateway.requests("subagent.list").len(),
        before + 3,
        "two refusals, then a third call that is never answered"
    );
    tokio::time::advance(Duration::from_secs(4)).await;
    now_ms += 4_000;
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(
        gateway.requests("subagent.list").len(),
        before + 3,
        "no fourth call while the third is still in flight"
    );
    tokio::time::advance(Duration::from_secs(1)).await;
    now_ms += 1_000;
    settle(&mut hub, &mut rx, &mut now_ms).await;
    tokio::time::advance(Duration::from_secs(30)).await;
    now_ms += 30_000;
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(
        gateway.requests("subagent.list").len(),
        before + 3,
        "the third call fails 5 s after it was sent; calls stop"
    );

    hub.open_panel(key.clone());
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    assert_eq!(
        gateway.requests("subagent.list").len(),
        before + 4,
        "opening the panel calls again"
    );
}

#[tokio::test(start_paused = true)]
async fn missing_list_method_disables_once() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.set_behavior(
        "subagent.list",
        Behavior::RefuseCode {
            reason: "unknown_method".to_owned(),
            code: -32601,
        },
    );
    hub.attach(key.clone(), client);
    let drained = wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    assert_eq!(
        drained.notices,
        vec!["This Hermes does not list subagents; showing events only.".to_owned()]
    );
    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));
    tokio::time::advance(Duration::from_secs(30)).await;
    now_ms += 30_000;
    let drained = settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(
        gateway.requests("subagent.list").len(),
        1,
        "no further list call"
    );
    assert!(drained.notices.is_empty(), "the notice comes once");
    assert_eq!(
        hub.tree(&key).expect("tree").counts().live,
        1,
        "events still build rows"
    );
}

#[tokio::test(start_paused = true)]
async fn tail_polls_while_open_only() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));

    gateway.push_result(json!({
        "subagent_id": "child-1",
        "available": true,
        "text": "line one\nline two\n",
        "truncated": false,
    }));
    hub.act(
        &key,
        SubagentAction::OpenTail("child-1".to_owned()),
        clock_ms(now_ms),
    );
    wait_call(&mut hub, &mut rx, &gateway, "subagent.tail", &mut now_ms).await;
    assert_eq!(
        params_of(&gateway, "subagent.tail", 0),
        json!({"session_id": SESSION_ID, "subagent_id": "child-1"})
    );
    for expected in [2, 3] {
        gateway.push_result(json!({
            "subagent_id": "child-1",
            "available": true,
            "text": "line one\nline two\n",
            "truncated": false,
        }));
        tokio::time::advance(Duration::from_millis(2000)).await;
        now_ms += 2000;
        wait_call(&mut hub, &mut rx, &gateway, "subagent.tail", &mut now_ms).await;
        assert_eq!(gateway.requests("subagent.tail").len(), expected);
    }

    hub.act(&key, SubagentAction::CloseTail, clock_ms(now_ms));
    tokio::time::advance(Duration::from_secs(10)).await;
    now_ms += 10_000;
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(
        gateway.requests("subagent.tail").len(),
        3,
        "none after close"
    );

    gateway.set_behavior("subagent.tail", Behavior::Silent);
    hub.act(
        &key,
        SubagentAction::OpenTail("child-1".to_owned()),
        clock_ms(now_ms),
    );
    wait_call(&mut hub, &mut rx, &gateway, "subagent.tail", &mut now_ms).await;
    assert_eq!(gateway.requests("subagent.tail").len(), 4);
    tokio::time::advance(Duration::from_secs(5)).await;
    now_ms += 5_000;
    let drained = settle(&mut hub, &mut rx, &mut now_ms).await;
    assert!(
        drained
            .hub_msgs
            .iter()
            .any(|msg| matches!(msg, SubagentMsg::Tail { result: Err(_), .. })),
        "a tail the mock never answers fails after 5 s"
    );
    // The tail texts (`Live transcript unavailable.`, `No live transcript
    // yet.`) render from the TUI's tail view; the drawing tests pin them
    // there. The hub holds no tail-text getter, so this test pins the calls.
}

fn control_outcomes(drained: &Drained) -> Vec<ControlOutcome> {
    drained
        .hub_msgs
        .iter()
        .filter_map(|msg| match msg {
            SubagentMsg::Control { outcome, .. } => Some(outcome.clone()),
            _ => None,
        })
        .collect()
}

async fn steer_once(
    hub: &mut SubagentHub,
    rx: &mut mpsc::UnboundedReceiver<AppMessage>,
    gateway: &MockGateway,
    key: &RowKey,
    id: &str,
    text: &str,
    now_ms: &mut u64,
) -> Vec<ControlOutcome> {
    hub.act(
        key,
        SubagentAction::Steer {
            id: id.to_owned(),
            text: text.to_owned(),
        },
        clock_ms(*now_ms),
    );
    let drained = wait_call(hub, rx, gateway, "subagent.steer", now_ms).await;
    control_outcomes(&drained)
}

#[tokio::test(start_paused = true)]
async fn steer_trims_and_interprets() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));

    gateway.push_result(json!({
        "subagent_id": "child-1",
        "status": "queued",
        "text": "steer me",
    }));
    let outcomes = steer_once(
        &mut hub,
        &mut rx,
        &gateway,
        &key,
        "child-1",
        "  steer me  ",
        &mut now_ms,
    )
    .await;
    assert_eq!(
        params_of(&gateway, "subagent.steer", 0)["text"],
        json!("steer me")
    );
    assert_eq!(
        outcomes,
        vec![ControlOutcome::SteerQueued {
            goal: "goal child-1".to_owned()
        }]
    );
    assert_eq!(
        outcome_text(&outcomes[0]),
        "Steer queued for \"goal child-1\". It reaches the subagent at its next step."
    );

    gateway.push_result(json!({
        "subagent_id": "child-1",
        "status": "rejected",
        "text": "steer me",
    }));
    let outcomes = steer_once(
        &mut hub,
        &mut rx,
        &gateway,
        &key,
        "child-1",
        "steer me",
        &mut now_ms,
    )
    .await;
    assert_eq!(
        outcomes,
        vec![ControlOutcome::SteerRejected {
            goal: "goal child-1".to_owned()
        }]
    );
    assert_eq!(
        outcome_text(&outcomes[0]),
        "\"goal child-1\" is no longer accepting steering."
    );

    gateway.queue_behavior(
        "subagent.steer",
        Behavior::RefuseCode {
            reason: "invalid_params".to_owned(),
            code: 4001,
        },
    );
    let outcomes = steer_once(
        &mut hub,
        &mut rx,
        &gateway,
        &key,
        "child-1",
        "steer me",
        &mut now_ms,
    )
    .await;
    assert_eq!(
        outcome_text(&outcomes[0]),
        "Hermes refused subagent.steer: Talaria sent a request Hermes did not accept (please report this)."
    );

    gateway.set_behavior("subagent.steer", Behavior::Silent);
    hub.act(
        &key,
        SubagentAction::Steer {
            id: "child-1".to_owned(),
            text: "steer me".to_owned(),
        },
        clock_ms(now_ms),
    );
    wait_call(&mut hub, &mut rx, &gateway, "subagent.steer", &mut now_ms).await;
    tokio::time::advance(Duration::from_secs(10)).await;
    now_ms += 10_000;
    let drained = settle(&mut hub, &mut rx, &mut now_ms).await;
    let outcomes = control_outcomes(&drained);
    assert_eq!(
        outcomes,
        vec![ControlOutcome::Unknown {
            action: "steer",
            secs: 10
        }]
    );
    assert_eq!(
        outcome_text(&outcomes[0]),
        "No answer from Hermes within 10 s; the outcome of steer is unknown."
    );
}

#[tokio::test(start_paused = true)]
async fn blank_steer_is_never_sent() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));

    for blank in ["", "   "] {
        hub.act(
            &key,
            SubagentAction::Steer {
                id: "child-1".to_owned(),
                text: blank.to_owned(),
            },
            clock_ms(now_ms),
        );
    }
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert!(
        gateway.requests("subagent.steer").is_empty(),
        "blank steers send nothing (the panel flashes `Type an instruction first.`)"
    );

    gateway.push_result(json!({
        "subagent_id": "child-1",
        "status": "queued",
        "text": "go",
    }));
    hub.act(
        &key,
        SubagentAction::Steer {
            id: "child-1".to_owned(),
            text: "  go  ".to_owned(),
        },
        clock_ms(now_ms),
    );
    wait_call(&mut hub, &mut rx, &gateway, "subagent.steer", &mut now_ms).await;
    assert_eq!(gateway.requests("subagent.steer").len(), 1);
    assert_eq!(
        params_of(&gateway, "subagent.steer", 0)["text"],
        json!("go")
    );
}

#[tokio::test(start_paused = true)]
async fn stop_interprets_found() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));

    gateway.push_result(json!({"subagent_id": "child-1", "found": true}));
    hub.act(
        &key,
        SubagentAction::Stop("child-1".to_owned()),
        clock_ms(now_ms),
    );
    let drained = wait_call(
        &mut hub,
        &mut rx,
        &gateway,
        "subagent.interrupt",
        &mut now_ms,
    )
    .await;
    let outcomes = control_outcomes(&drained);
    assert_eq!(
        outcomes,
        vec![ControlOutcome::StopRequested {
            goal: "goal child-1".to_owned()
        }]
    );
    assert_eq!(
        outcome_text(&outcomes[0]),
        "Stop requested for \"goal child-1\"."
    );
    assert!(
        hub.tree(&key)
            .expect("tree")
            .get("child-1")
            .expect("row")
            .stop_requested
    );

    live_child(&mut hub, &key, "child-2", clock_ms(now_ms));
    gateway.push_result(json!({"subagent_id": "child-2", "found": false}));
    hub.act(
        &key,
        SubagentAction::Stop("child-2".to_owned()),
        clock_ms(now_ms),
    );
    let drained = wait_call(
        &mut hub,
        &mut rx,
        &gateway,
        "subagent.interrupt",
        &mut now_ms,
    )
    .await;
    let outcomes = control_outcomes(&drained);
    assert_eq!(
        outcomes,
        vec![ControlOutcome::AlreadyFinished {
            goal: "goal child-2".to_owned()
        }]
    );
    assert_eq!(
        outcome_text(&outcomes[0]),
        "\"goal child-2\" had already finished."
    );
    assert!(
        !hub.tree(&key)
            .expect("tree")
            .get("child-2")
            .expect("row")
            .stop_requested
    );
}

#[tokio::test(start_paused = true)]
async fn stop_all_needs_second_press() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));
    live_child(&mut hub, &key, "child-2", clock_ms(now_ms));

    let armed_ms = now_ms;
    hub.act(&key, SubagentAction::StopAll, clock_ms(armed_ms));
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert!(
        gateway.requests("subagent.interrupt").is_empty(),
        "the first press only arms (the panel flashes the confirmation)"
    );

    gateway.push_result(json!({"subagent_id": "child-1", "found": true}));
    gateway.push_result(json!({"subagent_id": "child-2", "found": true}));
    hub.act(
        &key,
        SubagentAction::ConfirmStopAll,
        clock_ms(armed_ms + 2_999),
    );
    now_ms = armed_ms + 2_999;
    wait_call(
        &mut hub,
        &mut rx,
        &gateway,
        "subagent.interrupt",
        &mut now_ms,
    )
    .await;
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(
        gateway.requests("subagent.interrupt").len(),
        2,
        "one interrupt per running row"
    );

    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));
    let armed_ms = now_ms;
    hub.act(&key, SubagentAction::StopAll, clock_ms(armed_ms));
    settle(&mut hub, &mut rx, &mut now_ms).await;
    hub.act(
        &key,
        SubagentAction::ConfirmStopAll,
        clock_ms(armed_ms + 3_001),
    );
    now_ms = armed_ms + 3_001;
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert!(
        gateway.requests("subagent.interrupt").is_empty(),
        "past the window a press arms again and sends nothing"
    );
}

#[tokio::test(start_paused = true)]
async fn pause_confirms_and_resume_does_not() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;

    hub.act(&key, SubagentAction::ConfirmPause(false), clock_ms(now_ms));
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert!(
        gateway.requests("delegation.pause").is_empty(),
        "n sends nothing"
    );

    gateway.push_result(json!({"paused": true}));
    gateway.push_result(json!({
        "active": [],
        "paused": true,
        "max_spawn_depth": 1,
        "max_concurrent_children": 10,
    }));
    hub.act(&key, SubagentAction::ConfirmPause(true), clock_ms(now_ms));
    let mut drained = wait_call(&mut hub, &mut rx, &gateway, "delegation.pause", &mut now_ms).await;
    assert_eq!(
        params_of(&gateway, "delegation.pause", 0),
        json!({"paused": true})
    );
    let status = wait_call(
        &mut hub,
        &mut rx,
        &gateway,
        "delegation.status",
        &mut now_ms,
    )
    .await;
    append(&mut drained, status);
    assert_eq!(params_of(&gateway, "delegation.status", 0), json!({}));
    append(&mut drained, settle(&mut hub, &mut rx, &mut now_ms).await);
    assert_eq!(
        control_outcomes(&drained),
        vec![ControlOutcome::Paused(true)]
    );
    assert!(matches!(
        hub.spawning(),
        talaria_core::subagents::Spawning::Paused(_)
    ));

    gateway.push_result(json!({"paused": false}));
    gateway.push_result(json!({
        "active": [],
        "paused": false,
        "max_spawn_depth": 1,
        "max_concurrent_children": 10,
    }));
    hub.act(&key, SubagentAction::RequestPause, clock_ms(now_ms));
    wait_call(&mut hub, &mut rx, &gateway, "delegation.pause", &mut now_ms).await;
    assert_eq!(
        params_of(&gateway, "delegation.pause", 1),
        json!({"paused": false}),
        "with spawning paused, p sends resume at once"
    );

    // The `/agents` forms run through the slash dispatcher, never the hub
    // directly, so the dispatch order is what is tested.
    use talaria_app::app::{AppState, update};
    use talaria_app::sessions::OpenedSession;

    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let snapshot = serde_json::from_value(mock_gateway::recorded_resume_snapshot())
        .expect("a contract-shaped snapshot");
    let mut state = AppState::new("agents-forms");
    update(
        &mut state,
        AppMessage::Opened {
            opened: OpenedSession::from_snapshot(snapshot),
            notice: None,
        },
    );
    let opened = state.opened.as_ref().expect("session opened");
    let fkey = RowKey {
        profile: state.profile.clone(),
        session_id: opened.session_id.clone(),
    };
    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(fkey.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    state.subagents = Some(hub);

    gateway.push_result(json!({"paused": false}));
    gateway.push_result(json!({
        "active": [],
        "paused": false,
        "max_spawn_depth": 1,
        "max_concurrent_children": 10,
    }));
    update(
        &mut state,
        AppMessage::User(UserAction::Submit("/agents resume".to_owned())),
    );
    let hub = state.subagents.as_mut().expect("hub installed");
    wait_call(hub, &mut rx, &gateway, "delegation.pause", &mut now_ms).await;
    assert_eq!(
        params_of(&gateway, "delegation.pause", 0),
        json!({"paused": false}),
        "`/agents resume` sends paused false at once without opening the panel"
    );
}

#[test]
fn refusals_in_plain_words() {
    let goal = "goal child-1".to_owned();
    let cases = [
        (
            ControlOutcome::SteerQueued { goal: goal.clone() },
            "Steer queued for \"goal child-1\". It reaches the subagent at its next step.",
        ),
        (
            ControlOutcome::SteerRejected { goal: goal.clone() },
            "\"goal child-1\" is no longer accepting steering.",
        ),
        (
            ControlOutcome::StopRequested { goal: goal.clone() },
            "Stop requested for \"goal child-1\".",
        ),
        (
            ControlOutcome::AlreadyFinished { goal: goal.clone() },
            "\"goal child-1\" had already finished.",
        ),
        (
            ControlOutcome::NotStarted { goal: goal.clone() },
            "\"goal child-1\" has not started yet; stop it once it is running.",
        ),
        (
            ControlOutcome::AlreadyStopping { goal: goal.clone() },
            "Stop already requested for \"goal child-1\".",
        ),
        (
            ControlOutcome::StopAllSent { sent: 2, of: 3 },
            "Stop requested for 2 of 3 subagents.",
        ),
        (
            ControlOutcome::Paused(true),
            "New subagents are paused for every session on this gateway.",
        ),
        (
            ControlOutcome::Paused(false),
            "New subagents are allowed again.",
        ),
        (
            ControlOutcome::MethodMissing {
                method: "subagent.list",
            },
            "This Hermes does not offer subagent.list.",
        ),
        (
            ControlOutcome::Unknown {
                action: "steer",
                secs: 10,
            },
            "No answer from Hermes within 10 s; the outcome of steer is unknown.",
        ),
        (
            ControlOutcome::Lost { action: "stop" },
            "The connection to Hermes dropped; the outcome of stop is unknown.",
        ),
        (
            ControlOutcome::NotSent { action: "pause" },
            "Not connected to the gateway; pause was not sent.",
        ),
        (
            ControlOutcome::NotSent { action: "resume" },
            "Not connected to the gateway; resume was not sent.",
        ),
    ];
    for (outcome, text) in cases {
        assert_eq!(outcome_text(&outcome), text);
    }

    let refused = [
        (
            "invalid_params",
            "Talaria sent a request Hermes did not accept (please report this)",
        ),
        ("not_found", "the session is not live on the gateway"),
        (
            "permission_denied",
            "this connection may not control the session",
        ),
        ("profile_mismatch", "the session belongs to another profile"),
        ("stale_generation", "the session moved on; reopen it"),
        (
            "unsupported_projection",
            "the session runs in a separate worker that Talaria cannot reach",
        ),
        ("storage_unavailable", "Hermes storage is busy; try again"),
        ("frobnicate", "frobnicate"),
    ];
    for (reason, plain) in refused {
        let outcome = ControlOutcome::Refused {
            method: "subagent.steer",
            reason: reason.to_owned(),
        };
        assert_eq!(
            outcome_text(&outcome),
            format!("Hermes refused subagent.steer: {plain}.")
        );
    }
    let pause_refused = ControlOutcome::Refused {
        method: "delegation.pause",
        reason: "permission_denied".to_owned(),
    };
    assert_eq!(
        outcome_text(&pause_refused),
        "Hermes refused delegation.pause: this connection may not change the gateway's delegation setting."
    );

    let long = ControlOutcome::StopRequested {
        goal: "g".repeat(60),
    };
    assert_eq!(
        outcome_text(&long),
        format!("Stop requested for \"{}\".", "g".repeat(40))
    );
}

#[tokio::test(start_paused = true)]
async fn dropped_call_is_unknown() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));

    gateway.queue_behavior("subagent.interrupt", Behavior::DropBeforeReply);
    hub.act(
        &key,
        SubagentAction::Stop("child-1".to_owned()),
        clock_ms(now_ms),
    );
    let mut drained = wait_call(
        &mut hub,
        &mut rx,
        &gateway,
        "subagent.interrupt",
        &mut now_ms,
    )
    .await;
    append(&mut drained, settle(&mut hub, &mut rx, &mut now_ms).await);
    let outcomes = control_outcomes(&drained);
    assert_eq!(outcomes, vec![ControlOutcome::Lost { action: "stop" }]);
    assert_eq!(
        outcome_text(&outcomes[0]),
        "The connection to Hermes dropped; the outcome of stop is unknown."
    );
    let row = hub.tree(&key).expect("tree").get("child-1").expect("row");
    assert!(!row.stop_requested);
    assert_eq!(row.state, RowState::Running, "the tree is unchanged");

    // The socket is gone, so the next control is never written.
    hub.act(
        &key,
        SubagentAction::Steer {
            id: "child-1".to_owned(),
            text: "steer me".to_owned(),
        },
        clock_ms(now_ms),
    );
    let drained = settle(&mut hub, &mut rx, &mut now_ms).await;
    let outcomes = control_outcomes(&drained);
    assert_eq!(outcomes, vec![ControlOutcome::NotSent { action: "steer" }]);
    assert_eq!(
        outcome_text(&outcomes[0]),
        "Not connected to the gateway; steer was not sent."
    );
    assert!(
        gateway.requests("subagent.steer").is_empty(),
        "the mock records no request"
    );
}

#[tokio::test(start_paused = true)]
async fn late_complete_after_parent_interrupt_lands() {
    use hermes_client::channel::Generation;
    use hermes_client::liveness::LinkEvent;
    use hermes_client::{Inbound, InboundEvent};
    use talaria_app::app::{AppState, update};
    use talaria_app::sessions::{ExecuteOutcome, OpenedSession};

    fn link(kind: &str, payload: Value, session: &str, epoch: &str, seq: u64) -> AppMessage {
        let raw = json!({
            "type": kind,
            "session_id": session,
            "replay_epoch": epoch,
            "seq": seq,
            "execution_generation": 3,
            "payload": payload,
        });
        AppMessage::Link(LinkEvent::Inbound {
            item: Inbound::Event(InboundEvent {
                generation: Generation(1),
                frame: EventFrame::from_params(&raw).expect("contract-shaped event"),
                raw,
            }),
            replayed: false,
        })
    }

    let (gateway, client, mut hub, _rx) = rig().await;
    let snapshot = serde_json::from_value(mock_gateway::recorded_resume_snapshot())
        .expect("a contract-shaped snapshot");
    let mut state = AppState::new("late-complete");
    update(
        &mut state,
        AppMessage::Opened {
            opened: OpenedSession::from_snapshot(snapshot),
            notice: None,
        },
    );
    let opened = state.opened.as_ref().expect("session opened");
    let session_id = opened.session_id.clone();
    let epoch = opened.cursor.replay_epoch.clone();
    let mut seq = opened.cursor.last_sequence + 1;
    let fkey = RowKey {
        profile: state.profile.clone(),
        session_id: session_id.clone(),
    };
    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(fkey.clone(), client);
    state.subagents = Some(hub);

    let mut next = || {
        seq += 1;
        seq
    };
    update(
        &mut state,
        link(
            "subagent.start",
            payload("child-1"),
            &session_id,
            &epoch,
            next(),
        ),
    );
    update(
        &mut state,
        AppMessage::InterruptDone {
            outcome: ExecuteOutcome::Sent,
            notices: Vec::new(),
        },
    );
    update(
        &mut state,
        link(
            "message.complete",
            json!({"status": "interrupted"}),
            &session_id,
            &epoch,
            next(),
        ),
    );
    update(
        &mut state,
        link(
            "subagent.complete",
            with(payload("child-1"), "status", json!("interrupted")),
            &session_id,
            &epoch,
            next(),
        ),
    );
    let hub = state.subagents.as_ref().expect("hub installed");
    assert_eq!(
        hub.tree(&fkey)
            .expect("hub has the foreground tree")
            .get("child-1")
            .expect("the late complete lands")
            .state,
        RowState::Interrupted,
        "the reducer's interrupt latch cannot hide it: routing runs first"
    );

    // The out-of-turn form: no execution stamp at all.
    update(
        &mut state,
        link(
            "subagent.start",
            payload("child-2"),
            &session_id,
            &epoch,
            next(),
        ),
    );
    let raw = json!({
        "type": "subagent.complete",
        "session_id": session_id,
        "replay_epoch": epoch,
        "seq": next(),
        "payload": with(payload("child-2"), "status", json!("interrupted")),
    });
    update(
        &mut state,
        AppMessage::Link(LinkEvent::Inbound {
            item: Inbound::Event(InboundEvent {
                generation: Generation(1),
                frame: EventFrame::from_params(&raw).expect("contract-shaped event"),
                raw,
            }),
            replayed: false,
        }),
    );
    let hub = state.subagents.as_ref().expect("hub installed");
    assert_eq!(
        hub.tree(&fkey)
            .expect("hub has the foreground tree")
            .get("child-2")
            .expect("the unstamped complete lands")
            .state,
        RowState::Interrupted
    );
}

#[tokio::test(start_paused = true)]
async fn only_allowed_methods_are_sent() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));

    gateway.push_result(json!({
        "subagent_id": "child-1",
        "available": true,
        "text": "line one\n",
        "truncated": false,
    }));
    hub.act(
        &key,
        SubagentAction::OpenTail("child-1".to_owned()),
        clock_ms(now_ms),
    );
    wait_call(&mut hub, &mut rx, &gateway, "subagent.tail", &mut now_ms).await;
    hub.act(&key, SubagentAction::CloseTail, clock_ms(now_ms));

    gateway.push_result(json!({
        "subagent_id": "child-1",
        "status": "queued",
        "text": "steer me",
    }));
    steer_once(
        &mut hub,
        &mut rx,
        &gateway,
        &key,
        "child-1",
        "steer me",
        &mut now_ms,
    )
    .await;

    gateway.push_result(json!({"subagent_id": "child-1", "found": true}));
    hub.act(
        &key,
        SubagentAction::Stop("child-1".to_owned()),
        clock_ms(now_ms),
    );
    wait_call(
        &mut hub,
        &mut rx,
        &gateway,
        "subagent.interrupt",
        &mut now_ms,
    )
    .await;

    gateway.push_result(json!({"paused": true}));
    gateway.push_result(json!({
        "active": [],
        "paused": true,
        "max_spawn_depth": 1,
        "max_concurrent_children": 10,
    }));
    hub.act(&key, SubagentAction::ConfirmPause(true), clock_ms(now_ms));
    wait_call(&mut hub, &mut rx, &gateway, "delegation.pause", &mut now_ms).await;
    wait_call(
        &mut hub,
        &mut rx,
        &gateway,
        "delegation.status",
        &mut now_ms,
    )
    .await;
    settle(&mut hub, &mut rx, &mut now_ms).await;

    for method in [
        "subagent.list",
        "subagent.tail",
        "subagent.steer",
        "subagent.interrupt",
        "delegation.status",
        "delegation.pause",
    ] {
        assert!(
            !gateway.requests(method).is_empty(),
            "all six allowed calls go out: {method} was sent"
        );
        for seen in gateway.requests(method) {
            assert!(
                seen.params.get("profile").is_none(),
                "{method} never carries a profile key"
            );
            assert!(
                seen.params.get("unattended").is_none(),
                "{method} never carries unattended"
            );
        }
    }
    for forbidden in [
        "session.interrupt",
        "spawn_tree.list",
        "spawn_tree.load",
        "spawn_tree.save",
    ] {
        assert!(
            gateway.requests(forbidden).is_empty(),
            "nothing else is sent: {forbidden}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn status_line_counts_from_tree() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    assert!(
        hub.tree(&key).is_none(),
        "before the first list there is no tree, so the status line reads the usage value and 0"
    );
    gateway.push_result(json!({
        "subagents": [
            {"subagent_id": "child-1", "goal": "goal child-1", "status": "running",
             "task_index": 0, "task_count": 1},
            {"subagent_id": "child-2", "goal": "goal child-2", "status": "running",
             "task_index": 0, "task_count": 1},
        ],
        "delegations": [],
    }));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    let tree = hub.tree(&key).expect("tree after the first list");
    assert!(tree.hydrated());
    let counts = hub.counts(&key).expect("counts after the first list");
    assert_eq!(counts.live, 2);
    assert_eq!(counts.terminal, 0);
    // The status line reads these counts once hydrated; the wiring lives in
    // the status screen with the implementation.
}

#[tokio::test(start_paused = true)]
async fn missing_id_notice_once_per_connection() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;

    for kind in ["subagent.start", "subagent.tool"] {
        let idless = notification(kind, without(payload("child-1"), "subagent_id"));
        hub.on_notification(&key, &idless, clock_ms(now_ms));
    }
    let drained = drain(&mut hub, &mut rx, clock_ms(now_ms));
    assert_eq!(
        drained.notices,
        vec!["Hermes sent a subagent event without an id; it was ignored.".to_owned()],
        "two events without an id give one notice"
    );

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.on_reconnected(&key);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    let third = notification(
        "subagent.progress",
        without(payload("child-2"), "subagent_id"),
    );
    hub.on_notification(&key, &third, clock_ms(now_ms));
    let drained = drain(&mut hub, &mut rx, clock_ms(now_ms));
    assert_eq!(
        drained.notices,
        vec!["Hermes sent a subagent event without an id; it was ignored.".to_owned()],
        "a new connection generation notices again"
    );
}

#[tokio::test(start_paused = true)]
async fn delegation_status_missing_shows_unknown() {
    use talaria_core::subagents::Spawning;

    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    assert_eq!(hub.spawning(), Spawning::Unknown);

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    gateway.queue_behavior(
        "delegation.status",
        Behavior::RefuseCode {
            reason: "unknown_method".to_owned(),
            code: -32601,
        },
    );
    hub.open_panel(key.clone());
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    wait_call(
        &mut hub,
        &mut rx,
        &gateway,
        "delegation.status",
        &mut now_ms,
    )
    .await;
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(hub.spawning(), Spawning::Unknown);

    gateway.set_behavior(
        "delegation.status",
        Behavior::RefuseCode {
            reason: "unknown_method".to_owned(),
            code: -32601,
        },
    );
    gateway.push_result(json!({"paused": true}));
    hub.act(&key, SubagentAction::ConfirmPause(true), clock_ms(now_ms));
    wait_call(&mut hub, &mut rx, &gateway, "delegation.pause", &mut now_ms).await;
    assert_eq!(
        params_of(&gateway, "delegation.pause", 0),
        json!({"paused": true}),
        "p then y still sends the pause while status is unknown (pause is owner-served)"
    );
    // The confirmation without a running count renders in the panel; the
    // drawing tests pin both wordings there.
}

#[tokio::test(start_paused = true)]
async fn stop_on_queued_or_finished_row_sends_nothing() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;

    let queued = notification("subagent.spawn_requested", payload("child-q"));
    hub.on_notification(&key, &queued, clock_ms(now_ms));
    hub.act(
        &key,
        SubagentAction::Stop("child-q".to_owned()),
        clock_ms(now_ms),
    );
    let drained = settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(
        control_outcomes(&drained),
        vec![ControlOutcome::NotStarted {
            goal: "goal child-q".to_owned()
        }]
    );
    assert!(
        gateway.requests("subagent.interrupt").is_empty(),
        "a queued child is not registered: nothing is sent"
    );

    live_child(&mut hub, &key, "child-done", clock_ms(now_ms));
    let done = notification(
        "subagent.complete",
        with(payload("child-done"), "status", json!("completed")),
    );
    hub.on_notification(&key, &done, clock_ms(now_ms));
    hub.act(
        &key,
        SubagentAction::Stop("child-done".to_owned()),
        clock_ms(now_ms),
    );
    let drained = settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(
        control_outcomes(&drained),
        vec![ControlOutcome::AlreadyFinished {
            goal: "goal child-done".to_owned()
        }]
    );
    assert!(gateway.requests("subagent.interrupt").is_empty());

    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));
    gateway.push_result(json!({"subagent_id": "child-1", "found": true}));
    hub.act(
        &key,
        SubagentAction::Stop("child-1".to_owned()),
        clock_ms(now_ms),
    );
    wait_call(
        &mut hub,
        &mut rx,
        &gateway,
        "subagent.interrupt",
        &mut now_ms,
    )
    .await;
    hub.act(
        &key,
        SubagentAction::Stop("child-1".to_owned()),
        clock_ms(now_ms),
    );
    let drained = settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(
        control_outcomes(&drained),
        vec![ControlOutcome::AlreadyStopping {
            goal: "goal child-1".to_owned()
        }]
    );
    assert_eq!(
        gateway.requests("subagent.interrupt").len(),
        1,
        "one interrupt per child is enough"
    );

    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    let queued = notification("subagent.spawn_requested", payload("child-q"));
    hub.on_notification(&key, &queued, clock_ms(now_ms));
    live_child(&mut hub, &key, "child-1", clock_ms(now_ms));
    hub.act(&key, SubagentAction::StopAll, clock_ms(now_ms));
    settle(&mut hub, &mut rx, &mut now_ms).await;
    gateway.push_result(json!({"subagent_id": "child-1", "found": true}));
    hub.act(
        &key,
        SubagentAction::ConfirmStopAll,
        clock_ms(now_ms + 1_000),
    );
    now_ms += 1_000;
    wait_call(
        &mut hub,
        &mut rx,
        &gateway,
        "subagent.interrupt",
        &mut now_ms,
    )
    .await;
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(
        gateway.requests("subagent.interrupt").len(),
        1,
        "stop all with one queued and one running row sends one interrupt"
    );
}

#[test]
fn settings_out_of_range_use_default() {
    use talaria_app::config::parse;

    for (text, poll_s, tail_ms, dock_rows) in [
        ("[subagents]\npoll_s = 2", 2, 2000, 3),
        ("[subagents]\npoll_s = 60", 60, 2000, 3),
        ("[subagents]\ntail_poll_ms = 1000", 5, 1000, 3),
        ("[subagents]\ntail_poll_ms = 10000", 5, 10000, 3),
        ("[subagents]\ndock_rows = 0", 5, 2000, 0),
        ("[subagents]\ndock_rows = 10", 5, 2000, 10),
    ] {
        let loaded = parse(text);
        assert!(loaded.notices.is_empty(), "{text}: {:?}", loaded.notices);
        assert_eq!(loaded.config.subagents.poll_s, poll_s, "{text}");
        assert_eq!(loaded.config.subagents.tail_poll_ms, tail_ms, "{text}");
        assert_eq!(loaded.config.subagents.dock_rows, dock_rows, "{text}");
    }

    for bad in ["poll_s = 1", "poll_s = 61", "poll_s = \"x\""] {
        let loaded = parse(&format!("[subagents]\n{bad}"));
        assert_eq!(loaded.config.subagents.poll_s, 5, "{bad}");
        assert_eq!(
            loaded.notices,
            vec!["[subagents] poll_s must be an integer from 2 to 60; using 5.".to_owned()],
            "{bad}"
        );
    }
    let loaded = parse("[subagents]\npoll_s = 1\ntail_poll_ms = 999\ndock_rows = 11");
    assert_eq!(loaded.config.subagents.poll_s, 5);
    assert_eq!(loaded.config.subagents.tail_poll_ms, 2000);
    assert_eq!(loaded.config.subagents.dock_rows, 3);
    assert_eq!(
        loaded.notices,
        vec![
            "[subagents] poll_s must be an integer from 2 to 60; using 5.".to_owned(),
            "[subagents] tail_poll_ms must be an integer from 1000 to 10000; using 2000."
                .to_owned(),
            "[subagents] dock_rows must be an integer from 0 to 10; using 3.".to_owned(),
        ]
    );

    let loaded = parse("[theme]\nmode = \"dark\"");
    assert_eq!(loaded.config.subagents, SubagentsConfig::default());
    assert!(loaded.notices.is_empty());
}

#[tokio::test(start_paused = true)]
async fn switch_and_background_batches() {
    use talaria_core::subagents::SubagentTree;

    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key_a = key();
    let key_b = RowKey {
        profile: "fixture".to_owned(),
        session_id: "local-session-2".to_owned(),
    };

    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key_a.clone(), client.clone());
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key_a, "child-1", clock_ms(now_ms));

    hub.detach(&key_a);
    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key_b.clone(), client.clone());
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    hub.detach(&key_b);

    gateway.push_result(json!({
        "subagents": [
            {"subagent_id": "child-1", "goal": "goal child-1", "status": "running",
             "task_index": 0, "task_count": 1},
        ],
        "delegations": [],
    }));
    hub.attach(key_a.clone(), client.clone());
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    let row = hub
        .tree(&key_a)
        .expect("reattached tree")
        .get("child-1")
        .expect("relearnt row");
    assert!(
        !row.seen_live,
        "switching back relearns the row with seen_live false"
    );

    let done = notification(
        "subagent.complete",
        with(payload("child-1"), "status", json!("completed")),
    );
    hub.on_notification(&key_a, &done, clock_ms(now_ms));
    let row = hub.tree(&key_a).expect("tree").get("child-1").expect("row");
    assert_eq!(row.state, RowState::Completed);
    // No batch can come back without seen_live; the tree-level oracle pins it
    // (the hub takes its batches internally and surfaces no drain).
    let mut oracle = SubagentTree::new();
    let listed: hermes_contract::schemas::SubagentListResult = serde_json::from_value(json!({
        "subagents": [
            {"subagent_id": "child-1", "goal": "goal child-1", "status": "running",
             "task_index": 0, "task_count": 1},
        ],
        "delegations": [],
    }))
    .expect("contract-shaped list answer");
    oracle.reconcile(&listed, clock_ms(now_ms));
    let complete = talaria_core::subagents::SubagentEvent::from_notification(&notification(
        "subagent.complete",
        with(payload("child-1"), "status", json!("completed")),
    ))
    .expect("subagent event");
    oracle.apply(&complete, clock_ms(now_ms));
    assert!(
        oracle.take_finished_batches().is_empty(),
        "when it completes, no block is committed"
    );

    let start = notification("message.start", json!({}));
    hub.on_notification(&key_a, &start, clock_ms(now_ms));
    assert!(
        hub.tree(&key_a).expect("tree").get("child-1").is_none(),
        "the next message.start prunes it"
    );

    let key_c = RowKey {
        profile: "fixture".to_owned(),
        session_id: "local-session-3".to_owned(),
    };
    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key_c.clone(), client.clone());
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key_c, "child-c", clock_ms(now_ms));
    let done_c = notification(
        "subagent.complete",
        with(payload("child-c"), "status", json!("completed")),
    );
    hub.on_notification(&key_c, &done_c, clock_ms(now_ms));
    let start_c = notification("message.start", json!({}));
    hub.on_notification(&key_c, &start_c, clock_ms(now_ms));
    assert!(
        hub.tree(&key_c).expect("tree").get("child-c").is_none(),
        "another owner's rows are pruned at that session's next message.start (and no block is committed for a background key)"
    );
}

#[tokio::test(start_paused = true)]
async fn replies_from_a_replaced_attachment_change_nothing() {
    use talaria_app::subagents::hub::TimerKind;
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();
    hub.attach(key.clone(), client.clone());
    let initial = wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    let generation = initial
        .hub_msgs
        .iter()
        .find_map(|msg| match msg {
            SubagentMsg::Listed { generation, .. } => Some(*generation),
            _ => None,
        })
        .expect("the first attachment's generation");
    hub.detach(&key);
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key, "child", clock_ms(now_ms));
    hub.act(
        &key,
        SubagentAction::OpenTail("child".into()),
        clock_ms(now_ms),
    );
    let listed = serde_json::from_value(
        json!({"subagents":[{"subagent_id":"old","goal":"old connection"}]}),
    )
    .unwrap();
    for msg in [
        SubagentMsg::Listed {
            key: key.clone(),
            generation,
            result: Ok(listed),
        },
        SubagentMsg::Control {
            key: key.clone(),
            generation,
            id: Some("child".into()),
            outcome: ControlOutcome::StopRequested { goal: "old".into() },
        },
        SubagentMsg::Tail {
            key: key.clone(),
            generation,
            view: 1,
            id: "child".into(),
            result: Ok(serde_json::from_value(
                json!({"subagent_id":"child","available":true,"text":"old"}),
            )
            .unwrap()),
        },
        SubagentMsg::DelegationStatus {
            key: key.clone(),
            generation,
            result: Ok(serde_json::from_value(
                json!({"active":[],"paused":true,"max_spawn_depth":1,"max_concurrent_children":10}),
            )
            .unwrap()),
        },
        SubagentMsg::Scheduled {
            key: key.clone(),
            generation,
            kind: TimerKind::Poll,
        },
    ] {
        assert!(!hub.on_message(msg, clock_ms(now_ms)));
    }
    assert!(hub.tree(&key).unwrap().get("old").is_none());
    assert!(!hub.tree(&key).unwrap().get("child").unwrap().stop_requested);
    assert_eq!(hub.tail(&key).unwrap().text, None);
    assert_eq!(hub.spawning(), talaria_core::subagents::Spawning::Unknown);
    assert!(hub.flash().is_none());
}

#[tokio::test(start_paused = true)]
async fn omitted_list_is_a_failure_and_tail_failure_keeps_the_trimmed_text() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();
    gateway.push_result(json!({"delegations":[]}));
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    assert!(!hub.tree(&key).unwrap().hydrated());
    assert!(hub.known_counts(&key).is_none());
    live_child(&mut hub, &key, "child", clock_ms(now_ms));
    gateway.push_matched_result("subagent.tail",json!({"session_id":SESSION_ID,"subagent_id":"child"}),json!({"subagent_id":"child","available":true,"truncated":true,"text":"partial\nfirst complete\nlast\n"}));
    hub.act(
        &key,
        SubagentAction::OpenTail("child".into()),
        clock_ms(now_ms),
    );
    wait_call(&mut hub, &mut rx, &gateway, "subagent.tail", &mut now_ms).await;
    assert_eq!(
        hub.tail(&key).unwrap().text.as_deref(),
        Some("first complete\nlast\n")
    );
    gateway.queue_behavior(
        "subagent.tail",
        Behavior::RefuseCode {
            reason: "not_found".into(),
            code: 4001,
        },
    );
    tokio::time::advance(Duration::from_millis(2000)).await;
    now_ms += 2000;
    wait_call(&mut hub, &mut rx, &gateway, "subagent.tail", &mut now_ms).await;
    assert_eq!(
        hub.tail(&key).unwrap().text.as_deref(),
        Some("first complete\nlast\n")
    );
    assert!(hub.tail(&key).unwrap().ever_failed);
}

#[tokio::test(start_paused = true)]
async fn foreground_batches_are_drained_once_and_background_batches_are_not_printed() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();
    hub.set_foreground(Some(key.clone()));
    hub.attach(key.clone(), client.clone());
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &key, "first", clock_ms(now_ms));
    hub.on_notification(
        &key,
        &notification(
            "subagent.complete",
            with(payload("first"), "status", json!("completed")),
        ),
        clock_ms(now_ms),
    );
    assert_eq!(hub.take_finished_batches().len(), 1);
    assert!(hub.take_finished_batches().is_empty());
    let background = RowKey {
        profile: "other".into(),
        session_id: "background".into(),
    };
    gateway.push_result(json!({"subagents":[],"delegations":[]}));
    hub.attach(background.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    live_child(&mut hub, &background, "other", clock_ms(now_ms));
    hub.on_notification(
        &background,
        &notification(
            "subagent.complete",
            with(payload("other"), "status", json!("completed")),
        ),
        clock_ms(now_ms),
    );
    assert!(hub.take_finished_batches().is_empty());
    hub.on_notification(
        &background,
        &notification("message.start", json!({})),
        clock_ms(now_ms),
    );
    assert!(hub.tree(&background).unwrap().get("other").is_none());
}

#[tokio::test(start_paused = true)]
async fn stop_all_limits_concurrent_calls_to_ten() {
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    for index in 0..12 {
        live_child(&mut hub, &key, &format!("child-{index}"), clock_ms(now_ms));
    }
    gateway.set_behavior("subagent.interrupt", Behavior::Silent);
    hub.act(&key, SubagentAction::StopAll, clock_ms(now_ms));
    hub.act(&key, SubagentAction::ConfirmStopAll, clock_ms(now_ms + 1));
    wait_call(
        &mut hub,
        &mut rx,
        &gateway,
        "subagent.interrupt",
        &mut now_ms,
    )
    .await;
    assert_eq!(gateway.requests("subagent.interrupt").len(), 10);
    tokio::time::advance(Duration::from_secs(10)).await;
    now_ms += 10_000;
    settle(&mut hub, &mut rx, &mut now_ms).await;
    assert_eq!(gateway.requests("subagent.interrupt").len(), 12);
    assert!(
        hub.tree(&key).unwrap().live_ids().iter().all(|id| !hub
            .tree(&key)
            .unwrap()
            .get(id)
            .unwrap()
            .stop_requested)
    );
}

#[tokio::test(start_paused = true)]
async fn elapsed_ticks_only_while_the_open_panel_has_live_rows() {
    use talaria_app::subagents::hub::TimerKind;
    let (gateway, client, mut hub, mut rx) = rig().await;
    let mut now_ms = 0;
    let key = key();
    hub.attach(key.clone(), client);
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    hub.open_panel(key.clone());
    wait_call(&mut hub, &mut rx, &gateway, "subagent.list", &mut now_ms).await;
    let elapsed = |drained: &Drained| {
        drained
            .hub_msgs
            .iter()
            .filter(|message| {
                matches!(
                    message,
                    SubagentMsg::Scheduled {
                        kind: TimerKind::Elapsed { .. },
                        ..
                    }
                )
            })
            .count()
    };
    tokio::time::advance(Duration::from_secs(1)).await;
    now_ms += 1000;
    assert_eq!(elapsed(&settle(&mut hub, &mut rx, &mut now_ms).await), 0);
    live_child(&mut hub, &key, "child", clock_ms(now_ms));
    tokio::time::advance(Duration::from_secs(1)).await;
    now_ms += 1000;
    assert_eq!(elapsed(&settle(&mut hub, &mut rx, &mut now_ms).await), 1);
    hub.on_notification(
        &key,
        &notification(
            "subagent.complete",
            with(payload("child"), "status", json!("completed")),
        ),
        clock_ms(now_ms),
    );
    tokio::time::advance(Duration::from_secs(1)).await;
    now_ms += 1000;
    settle(&mut hub, &mut rx, &mut now_ms).await;
    tokio::time::advance(Duration::from_secs(1)).await;
    now_ms += 1000;
    assert_eq!(elapsed(&settle(&mut hub, &mut rx, &mut now_ms).await), 0);
}
