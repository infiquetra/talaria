//! Red-run tests 49 to 52 (#242): the hub against a live gateway.
//!
//! All ignored. Each checks its preconditions first (a gateway answers, the
//! pin matches, profile `fixture` is served); approval mode `manual` and
//! delegation enabled are operator-attested preconditions. Without them the
//! test prints `LIVE_SUBAGENT_RESULT: SKIPPED (<reason>)` and passes, but
//! fails under `TALARIA_LIVE_REQUIRE=1`. `TALARIA_LIVE_PAUSE=1` is the
//! operator's opt-in for test 52. Each test creates its own `tui` session in
//! profile `fixture` in a temporary folder, drives it through `SubagentHub`
//! (so the stubs make it red when a gateway is present), and detaches it at
//! the end without an `end_reason`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use hermes_client::{
    CallOptions, Client, ConnectOptions, DiscoverOptions, EnsurePolicy, Inbound, InboundStream,
    Purpose, Scope, connect_with, default_home, discover_with, handshake, mint_ticket,
};
use hermes_contract::events::Notification;
use serde_json::json;
use talaria_app::app::AppMessage;
use talaria_app::subagents::{
    ControlOutcome, SubagentAction, SubagentHub, SubagentMsg, SubagentsConfig,
};
use talaria_core::directory::rows::RowKey;
use talaria_core::subagents::Clock;
use tokio::sync::mpsc;

const PROMPT_B: &str = "Use the delegate_task tool to run two subagents in parallel: one lists the files in the current directory, the other reads notes.txt. Then summarise both results in one sentence.";
const PROMPT_L: &str = "Use the delegate_task tool to start exactly one subagent. Its goal: run the shell command sleep 3 ten times, one command at a time, then reply with the single word finished. Wait for it, then reply with the single word done.";
const STEER_TEXT: &str = "Reply with the single word steered after your current command.";

fn home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn required() -> bool {
    std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1")
}

fn skipped(reason: &str) {
    println!("LIVE_SUBAGENT_RESULT: SKIPPED ({reason})");
    assert!(!required(), "live run required but skipped: {reason}");
}

fn pause_armed() -> bool {
    std::env::var("TALARIA_LIVE_PAUSE").ok().as_deref() == Some("1")
}

async fn live_client() -> Option<(Client, InboundStream)> {
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home(), &options).await {
        Ok(endpoint) => endpoint,
        Err(_) => {
            skipped("no Hermes gateway reachable");
            return None;
        }
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        skipped(&reason);
        return None;
    }
    let profile = match endpoint.resolve_profile("fixture") {
        Ok(profile) => profile,
        Err(_) => {
            skipped("the fixture profile is not served");
            return None;
        }
    };
    let ticket = mint_ticket(
        &home(),
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .expect("ticket");
    let mut connection = connect_with(&endpoint, ticket, &ConnectOptions::default())
        .await
        .expect("connect");
    handshake(&mut connection).await.expect("handshake");
    let (client, inbound) = Client::start(connection.into_parts());
    Some((client, inbound))
}

struct LiveSession {
    session_id: String,
    subscription_id: String,
    /// Held so the folder lives until the detach runs.
    _folder: tempfile::TempDir,
}

async fn live_session(client: &Client) -> LiveSession {
    let folder = tempfile::tempdir().expect("temporary folder");
    std::fs::write(folder.path().join("notes.txt"), "alpha beta").expect("notes.txt");
    let created = client
        .call_raw(
            "session.create",
            json!({
                "source": "tui",
                "cwd": folder.path(),
                "model": "minimax-m3.1-flash",
                "provider": "ollama-cloud",
            }),
            CallOptions {
                timeout: Some(Duration::from_secs(30)),
            },
        )
        .await
        .expect("session.create");
    LiveSession {
        session_id: created["session_id"].as_str().unwrap_or("").to_owned(),
        subscription_id: created["subscription_id"].as_str().unwrap_or("").to_owned(),
        _folder: folder,
    }
}

async fn submit(client: &Client, session_id: &str, text: &str) {
    client
        .call_raw(
            "prompt.submit",
            json!({
                "session_id": session_id,
                "submission_id": format!(
                    "live-subagents-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|ago| ago.as_nanos())
                        .unwrap_or(0)
                ),
                "text": text,
            }),
            CallOptions {
                timeout: Some(Duration::from_secs(30)),
            },
        )
        .await
        .expect("prompt.submit");
}

async fn detach(client: &Client, session: &LiveSession) {
    let _ = client
        .call_raw(
            "session.detach",
            json!({
                "session_id": session.session_id,
                "subscription_id": session.subscription_id,
            }),
            CallOptions {
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .await;
}

fn live_key(session_id: &str) -> RowKey {
    RowKey {
        profile: "fixture".to_owned(),
        session_id: session_id.to_owned(),
    }
}

fn live_clock(started: Instant) -> Clock {
    Clock {
        mono_ms: started.elapsed().as_millis() as u64,
        wall_s: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|ago| ago.as_secs_f64())
            .unwrap_or(0.0),
    }
}

fn drain_hub(
    hub: &mut SubagentHub,
    rx: &mut mpsc::UnboundedReceiver<AppMessage>,
    now: Clock,
) -> Vec<SubagentMsg> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        if let AppMessage::Subagents(inner) = msg {
            hub.on_message(inner.clone(), now);
            out.push(inner);
        }
    }
    out
}

async fn next_event(inbound: &mut InboundStream, within: Duration) -> Option<Notification> {
    let item = tokio::time::timeout(within, inbound.next()).await.ok()??;
    match item {
        Inbound::Event(event) => Some(event.frame.notification),
        _ => None,
    }
}

/// Feed gateway events into the hub until `done` says stop or `budget` runs
/// out. Returns what `done` returned, if it ever did.
async fn watch_until(
    hub: &mut SubagentHub,
    rx: &mut mpsc::UnboundedReceiver<AppMessage>,
    inbound: &mut InboundStream,
    key: &RowKey,
    started: Instant,
    budget: Duration,
    mut done: impl FnMut(&Notification, &mut SubagentHub, &[SubagentMsg]) -> Option<bool>,
) -> Option<bool> {
    let deadline = Instant::now() + budget;
    let mut pending = Vec::new();
    loop {
        let now = live_clock(started);
        pending.extend(drain_hub(hub, rx, now));
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        let Some(n) = next_event(inbound, remaining.min(Duration::from_secs(1))).await else {
            continue;
        };
        pending.extend(drain_hub(hub, rx, live_clock(started)));
        hub.on_notification(key, &n, live_clock(started));
        if let Some(stop) = done(&n, hub, &pending) {
            return Some(stop);
        }
        pending.clear();
    }
}

#[tokio::test]
#[ignore = "needs a Hermes gateway on the fixture profile"]
async fn live_rows_from_events_and_list() {
    let Some((client, mut inbound)) = live_client().await else {
        return;
    };
    let session = live_session(&client).await;
    let key = live_key(&session.session_id);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut hub = SubagentHub::new(SubagentsConfig::default(), tx);
    hub.attach(key.clone(), client.clone());
    let started = Instant::now();
    submit(&client, &session.session_id, PROMPT_B).await;

    let mut first_start: Option<(String, Instant)> = None;
    let mut seen_in_list = false;
    let outcome = watch_until(
        &mut hub,
        &mut rx,
        &mut inbound,
        &key,
        started,
        Duration::from_secs(120),
        |n, hub, msgs| {
            if let Notification::SubagentStart(payload) = n
                && let Some(id) = &payload.subagent_id
                && first_start.is_none()
            {
                first_start = Some((id.clone(), Instant::now()));
                hub.act(&key, SubagentAction::Refresh, live_clock(started));
            }
            let (id, at) = first_start.clone()?;
            for msg in msgs {
                if let SubagentMsg::Listed {
                    result: Ok(list), ..
                } = msg
                {
                    seen_in_list =
                        seen_in_list || list.subagents.iter().any(|row| row.subagent_id == id);
                }
            }
            // A short child may finish before a list call sees it; either a
            // list containing the id or an early complete passes.
            if seen_in_list
                || hub
                    .tree(&key)
                    .and_then(|tree| tree.get(&id))
                    .is_some_and(|row| row.state.is_terminal())
            {
                return Some(true);
            }
            if at.elapsed() > Duration::from_secs(5) {
                return Some(false);
            }
            None
        },
    )
    .await;
    detach(&client, &session).await;
    assert!(
        first_start.is_some(),
        "at least one row appears from events within 120 s"
    );
    assert_eq!(
        outcome,
        Some(true),
        "within 5 s of the first start, either a list contains that id or a complete for it has already arrived"
    );
    println!("LIVE_SUBAGENT_RESULT: PASSED");
}

#[tokio::test]
#[ignore = "needs a Hermes gateway on the fixture profile"]
async fn live_steer_reaches_running_child() {
    let Some((client, mut inbound)) = live_client().await else {
        return;
    };
    for attempt in 1..=3 {
        let session = live_session(&client).await;
        let key = live_key(&session.session_id);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut hub = SubagentHub::new(SubagentsConfig::default(), tx);
        hub.attach(key.clone(), client.clone());
        let started = Instant::now();
        submit(&client, &session.session_id, PROMPT_L).await;

        let mut steered = false;
        let mut status = String::new();
        watch_until(
            &mut hub,
            &mut rx,
            &mut inbound,
            &key,
            started,
            Duration::from_secs(150),
            |n, hub, msgs| {
                if !steered {
                    if let Notification::SubagentTool(payload) = n
                        && let Some(id) = &payload.subagent_id
                    {
                        hub.act(
                            &key,
                            SubagentAction::Steer {
                                id: id.clone(),
                                text: STEER_TEXT.to_owned(),
                            },
                            live_clock(started),
                        );
                        steered = true;
                    }
                    return None;
                }
                for msg in msgs {
                    if let SubagentMsg::Control { outcome, .. } = msg {
                        match outcome {
                            ControlOutcome::SteerQueued { .. } => {
                                status = "queued".to_owned();
                                return Some(true);
                            }
                            ControlOutcome::SteerRejected { .. } => {
                                status = "rejected".to_owned();
                                return Some(true);
                            }
                            _ => {}
                        }
                    }
                }
                None
            },
        )
        .await;
        println!("STEER_RESULT: {status}");
        detach(&client, &session).await;
        if status == "queued" {
            println!("LIVE_SUBAGENT_RESULT: PASSED");
            return;
        }
        assert!(
            attempt < 3,
            "steer reached a running child with `queued` within three tries (last: {status:?})"
        );
    }
}

#[tokio::test]
#[ignore = "needs a Hermes gateway on the fixture profile"]
async fn live_stop_interrupts_child() {
    let Some((client, mut inbound)) = live_client().await else {
        return;
    };
    let session = live_session(&client).await;
    let key = live_key(&session.session_id);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut hub = SubagentHub::new(SubagentsConfig::default(), tx);
    hub.attach(key.clone(), client.clone());
    let started = Instant::now();
    submit(&client, &session.session_id, PROMPT_L).await;

    let mut stopped_at: Option<Instant> = None;
    let mut found = false;
    let mut interrupted_at: Option<Duration> = None;
    let mut parent_status = String::new();
    watch_until(
        &mut hub,
        &mut rx,
        &mut inbound,
        &key,
        started,
        Duration::from_secs(150),
        |n, hub, msgs| {
            found |= msgs.iter().any(|msg| {
                matches!(
                    msg,
                    SubagentMsg::Control {
                        outcome: ControlOutcome::StopRequested { .. },
                        ..
                    }
                )
            });
            match n {
                Notification::SubagentTool(payload) if stopped_at.is_none() => {
                    if let Some(id) = payload.subagent_id.clone() {
                        hub.act(&key, SubagentAction::Stop(id), live_clock(started));
                        stopped_at = Some(Instant::now());
                    }
                }
                Notification::SubagentComplete(payload) => {
                    if payload.status.as_ref().is_some_and(|status| {
                        matches!(
                            status,
                            hermes_contract::schemas::SubagentStatus::Interrupted
                        )
                    }) {
                        interrupted_at = stopped_at.map(|at| at.elapsed());
                    }
                }
                Notification::MessageComplete(payload) => {
                    parent_status = payload
                        .status
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default();
                }
                _ => {}
            }
            (interrupted_at.is_some() && !parent_status.is_empty()).then_some(true)
        },
    )
    .await;
    for msg in drain_hub(&mut hub, &mut rx, live_clock(started)) {
        if let SubagentMsg::Control {
            outcome: ControlOutcome::StopRequested { .. },
            ..
        } = msg
        {
            found = true;
        }
    }
    detach(&client, &session).await;
    assert!(found, "the interrupt answers found: true");
    let secs = interrupted_at
        .expect("subagent.complete with interrupted within 60 s")
        .as_secs();
    assert!(secs <= 60, "interrupted arrives within 60 s ({secs} s)");
    println!("STOP_INTERRUPTED_AFTER_S: {secs}");
    println!("PARENT_STATUS: {parent_status}");
    assert_eq!(
        parent_status, "complete",
        "stopping only a child must not interrupt its parent"
    );
    println!("LIVE_SUBAGENT_RESULT: PASSED");
}

#[tokio::test]
#[ignore = "needs a Hermes gateway on the fixture profile and TALARIA_LIVE_PAUSE=1"]
async fn live_pause_blocks_new_spawns() {
    if !pause_armed() {
        println!("LIVE_SUBAGENT_RESULT: SKIPPED (TALARIA_LIVE_PAUSE not set)");
        return;
    }
    let Some((client, mut inbound)) = live_client().await else {
        return;
    };

    let mut exercised = false;
    let mut saw_spawn = false;
    let mut reply_mentions_pause = false;
    for _ in 1..=3 {
        let session = live_session(&client).await;
        let key = live_key(&session.session_id);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut hub = SubagentHub::new(SubagentsConfig::default(), tx);
        hub.attach(key.clone(), client.clone());
        let started = Instant::now();
        hub.act(
            &key,
            SubagentAction::ConfirmPause(true),
            live_clock(started),
        );
        submit(&client, &session.session_id, PROMPT_B).await;

        let mut turn_done = false;
        let mut delegate_seen = false;
        watch_until(
            &mut hub,
            &mut rx,
            &mut inbound,
            &key,
            started,
            Duration::from_secs(150),
            |n, _, _| match n {
                Notification::ToolComplete(payload) if payload.name == "delegate_task" => {
                    let text = payload
                        .result_text
                        .clone()
                        .unwrap_or_else(|| payload.result.to_string());
                    if text.contains("Delegation spawning is paused") {
                        delegate_seen = true;
                    }
                    None
                }
                Notification::SubagentSpawnRequested(_) | Notification::SubagentStart(_) => {
                    saw_spawn = true;
                    None
                }
                Notification::MessageComplete(payload) => {
                    let text = payload
                        .text
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    if text.to_lowercase().contains("paus") {
                        reply_mentions_pause = true;
                    }
                    turn_done = true;
                    Some(true)
                }
                _ => None,
            },
        )
        .await;
        detach(&client, &session).await;
        if delegate_seen && turn_done {
            exercised = true;
            break;
        }
    }
    println!(
        "PAUSE_MENTIONED: {}",
        if reply_mentions_pause { "yes" } else { "no" }
    );
    hub_unpause(&client).await;
    assert!(
        !saw_spawn,
        "no subagent.spawn_requested or subagent.start arrives while paused"
    );
    assert!(
        exercised,
        "LIVE_SUBAGENT_RESULT: FAILED (pause not exercised)"
    );
}

async fn hub_unpause(client: &Client) {
    let _ = client
        .call_raw(
            "delegation.pause",
            json!({"paused": false}),
            CallOptions {
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .await;
    if let Ok(status) = client
        .call_raw(
            "delegation.status",
            json!({}),
            CallOptions {
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .await
    {
        println!(
            "PAUSE_FINAL: paused={}",
            status["paused"].as_bool().unwrap_or(true)
        );
    }
}
