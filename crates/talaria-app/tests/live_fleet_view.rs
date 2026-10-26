//! Live peek, reply, answer and stop against a running Hermes gateway.
//!
//! Ignored unless someone asks for them. Each test prints one result line:
//! `LIVE_FLEET_VIEW_RESULT: PASSED (<name>)`, `FAILED (<name>: <reason>)`, or
//! `SKIPPED (<reason>)`. A skip fails when `TALARIA_LIVE_REQUIRE=1`.

#[path = "support/fake_fleet.rs"]
mod fake_fleet;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use fake_fleet::FakeFleetSink;
use hermes_client::{
    CallOptions, Client, ConnectOptions, DiscoverOptions, EnsurePolicy, Inbound, InboundStream,
    Purpose, Scope, connect_with, default_home, discover_with, handshake, mint_ticket,
};
use serde_json::{Value, json};
use talaria_app::launch::{Launch, StartMode};
use talaria_app::peek::{CardAnswer, PeekError, PeekService, PeekTimeouts};
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet_view::{ReplyOutcome, StopOutcome};
use talaria_core::session::{Cell, PromptKind, PromptState, SessionState, TurnOutcome, TurnState};
use talaria_tui::prompts::PromptIntent;

const NAME_PEEK: &str = "live_peek_shows_latest_output";
const NAME_REPLY: &str = "live_reply_queues_behind_running_turn";
const NAME_ANSWER: &str = "live_answer_approval_from_peek";
const NAME_STOP: &str = "live_stop_from_fleet";

fn home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn call_options() -> CallOptions {
    CallOptions {
        timeout: Some(Duration::from_secs(30)),
    }
}

fn next_submission_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("talaria-{:016x}", NEXT.fetch_add(1, Ordering::Relaxed))
}

fn skip(name: &str, reason: &str) {
    if std::env::var_os("TALARIA_LIVE_REQUIRE").is_some_and(|value| value == "1") {
        fail(name, reason);
    }
    println!("LIVE_FLEET_VIEW_RESULT: SKIPPED ({reason})");
}

fn pass(name: &str) {
    println!("LIVE_FLEET_VIEW_RESULT: PASSED ({name})");
}

fn fail(name: &str, reason: &str) -> ! {
    println!("LIVE_FLEET_VIEW_RESULT: FAILED ({name}: {reason})");
    panic!("{name}: {reason}");
}

struct Wire {
    client: Client,
    methods: Vec<&'static str>,
}

async fn gate(name: &str) -> Option<Wire> {
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home(), &options).await {
        Ok(endpoint) => endpoint,
        Err(_) => {
            skip(name, "no Hermes gateway reachable");
            return None;
        }
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        skip(name, &reason);
        return None;
    }
    let profile = match endpoint.resolve_profile("fixture") {
        Ok(profile) => profile,
        Err(_) => {
            skip(name, "the fixture profile is not served");
            return None;
        }
    };
    let ticket = match mint_ticket(
        &home(),
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    {
        Ok(ticket) => ticket,
        Err(_) => {
            skip(name, "a profile ticket could not be minted");
            return None;
        }
    };
    let mut connection = match connect_with(&endpoint, ticket, &ConnectOptions::default()).await {
        Ok(connection) => connection,
        Err(_) => {
            skip(name, "the profile connection could not be opened");
            return None;
        }
    };
    if handshake(&mut connection).await.is_err() {
        skip(name, "the profile handshake failed");
        return None;
    }
    let (client, inbound) = Client::start(connection.into_parts());
    tokio::spawn(drain(inbound));
    Some(Wire {
        client,
        methods: Vec::new(),
    })
}

async fn drain(mut inbound: InboundStream) {
    while let Some(item) = inbound.next().await {
        if let Inbound::ServerRequest(request) = item {
            request.responder.ignore();
        }
    }
}

async fn call(wire: &mut Wire, method: &'static str, params: Value) -> Result<Value, ()> {
    wire.methods.push(method);
    wire.client
        .call_raw(method, params, call_options())
        .await
        .map_err(|_| ())
}

fn peek_for(folder: &Path) -> PeekService {
    PeekService::new(
        Launch {
            mode: StartMode::New,
            model: Some("minimax-m3.1-flash".to_owned()),
            provider: Some("ollama-cloud".to_owned()),
            effort: None,
            profile: "fixture".to_owned(),
            folder: folder.to_path_buf(),
            record: None,
            bench_replay: None,
            no_ensure: true,
        },
        home(),
        Arc::new(FakeFleetSink::new()),
        Box::new(next_submission_id),
        PeekTimeouts::default(),
    )
}

fn row_key(session_id: &str) -> RowKey {
    RowKey {
        profile: "fixture".to_owned(),
        session_id: session_id.to_owned(),
    }
}

struct Created {
    folder: tempfile::TempDir,
    session_id: String,
    subscription_id: Value,
}

async fn create_session(name: &str, wire: &mut Wire) -> Created {
    let folder = tempfile::tempdir().expect("temporary folder");
    let created = match call(
        wire,
        "session.create",
        json!({
            "source": "cli",
            "cwd": folder.path(),
            "model": "minimax-m3.1-flash",
            "provider": "ollama-cloud",
        }),
    )
    .await
    {
        Ok(created) => created,
        Err(()) => fail(name, "session.create failed"),
    };
    let session_id = created["session_id"].as_str().unwrap_or("").to_owned();
    if session_id.is_empty() {
        fail(name, "session.create returned no session id");
    }
    let subscription_id = created["subscription_id"].clone();
    Created {
        folder,
        session_id,
        subscription_id,
    }
}

async fn submit(name: &str, wire: &mut Wire, session_id: &str, text: &str) {
    let sent = call(
        wire,
        "prompt.submit",
        json!({
            "session_id": session_id,
            "text": text,
            "submission_id": next_submission_id(),
        }),
    )
    .await;
    if sent.is_err() {
        fail(name, "prompt.submit failed");
    }
}

async fn detach_creator(name: &str, wire: &mut Wire, session_id: &str, subscription_id: &Value) {
    let detached = call(
        wire,
        "session.detach",
        json!({
            "session_id": session_id,
            "subscription_id": subscription_id,
        }),
    )
    .await;
    if detached.is_err() {
        fail(name, "the creating connection did not detach");
    }
    wire.client.close().await;
}

async fn timed_open(name: &str, peek: &mut PeekService, key: &RowKey) -> u128 {
    let started = Instant::now();
    match peek.open(key.clone()).await {
        Ok(()) => started.elapsed().as_millis(),
        Err(error) => fail(name, &open_reason(&error)),
    }
}

fn open_reason(error: &PeekError) -> String {
    match error {
        PeekError::Connect(reason) => format!("peek did not open: {reason}"),
        PeekError::NotFound => "peek did not open: session not found".to_owned(),
        PeekError::Refused { reason } => format!("peek did not open: {reason}"),
        PeekError::TooLarge => "peek did not open: snapshot too large".to_owned(),
        PeekError::Timeout => "peek did not open: timed out".to_owned(),
        PeekError::Dropped => "peek did not open: connection dropped".to_owned(),
    }
}

/// The peek link's events, as the app's worker feeds them to the service.
type Events = Option<tokio::sync::mpsc::Receiver<hermes_client::liveness::LinkEvent>>;

/// Feed the peek link's events to the service for `wait`. A new link's events are taken first.
async fn pump_for(peek: &mut PeekService, events: &mut Events, wait: Duration) {
    if let Some(fresh) = peek.take_events() {
        *events = Some(fresh);
    }
    let Some(receiver) = events.as_mut() else {
        tokio::time::sleep(wait).await;
        return;
    };
    let deadline = tokio::time::Instant::now() + wait;
    while let Ok(Some(event)) = tokio::time::timeout_at(deadline, receiver.recv()).await {
        let _ = peek.on_link_event(event);
    }
}

async fn until_state(
    peek: &mut PeekService,
    events: &mut Events,
    budget: Duration,
    mut ready: impl FnMut(&SessionState) -> bool,
) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if let Some((_, state)) = peek.state()
            && ready(state)
        {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        pump_for(peek, events, Duration::from_millis(50)).await;
    }
}

fn tool_context_has(state: &SessionState, needle: &str) -> bool {
    state.cells().iter().any(|cell| match cell {
        Cell::Tool(tool) => tool
            .context
            .as_deref()
            .is_some_and(|context| context.contains(needle)),
        _ => false,
    })
}

fn output_has(state: &SessionState, needle: &str) -> bool {
    state.cells().iter().any(|cell| match cell {
        Cell::Assistant(cell) => cell.text.contains(needle),
        Cell::Tool(tool) => {
            tool.result
                .as_deref()
                .is_some_and(|result| result.contains(needle))
                || tool
                    .summary
                    .as_deref()
                    .is_some_and(|summary| summary.contains(needle))
        }
        _ => false,
    })
}

/// The word in a tool's output, not in the command that was asked for.
fn command_output_has(state: &SessionState, needle: &str) -> bool {
    state.cells().iter().any(|cell| match cell {
        Cell::Tool(tool) => tool
            .result
            .as_deref()
            .is_some_and(|result| result.contains(needle)),
        _ => false,
    })
}

fn median(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[tokio::test]
#[ignore = "needs a Hermes gateway on the fixture profile"]
async fn live_peek_shows_latest_output() {
    let Some(mut wire) = gate(NAME_PEEK).await else {
        return;
    };
    let created = create_session(NAME_PEEK, &mut wire).await;
    submit(
        NAME_PEEK,
        &mut wire,
        &created.session_id,
        "Run this exact shell command: sleep 5 && echo peek-ok",
    )
    .await;
    detach_creator(
        NAME_PEEK,
        &mut wire,
        &created.session_id,
        &created.subscription_id,
    )
    .await;

    let key = row_key(&created.session_id);
    let mut peek = peek_for(created.folder.path());
    let mut events: Events = None;
    let mut samples = vec![timed_open(NAME_PEEK, &mut peek, &key).await];
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut saw_tool = false;
    loop {
        if let Some((_, state)) = peek.state() {
            if tool_context_has(state, "sleep 5") {
                saw_tool = true;
            }
            if saw_tool && output_has(state, "peek-ok") {
                break;
            }
        }
        if Instant::now() >= deadline {
            if !saw_tool {
                fail(NAME_PEEK, "no tool cell contained sleep 5");
            }
            fail(
                NAME_PEEK,
                "assistant or tool output did not contain peek-ok",
            );
        }
        pump_for(&mut peek, &mut events, Duration::from_millis(50)).await;
    }
    for _ in 0..4 {
        peek.close().await;
        samples.push(timed_open(NAME_PEEK, &mut peek, &key).await);
    }
    println!("peek_open_ms={}", median(&mut samples));
    pass(NAME_PEEK);
}

#[tokio::test]
#[ignore = "needs a Hermes gateway on the fixture profile"]
async fn live_reply_queues_behind_running_turn() {
    let Some(mut wire) = gate(NAME_REPLY).await else {
        return;
    };
    let created = create_session(NAME_REPLY, &mut wire).await;
    submit(
        NAME_REPLY,
        &mut wire,
        &created.session_id,
        "Run this exact shell command: sleep 20",
    )
    .await;
    detach_creator(
        NAME_REPLY,
        &mut wire,
        &created.session_id,
        &created.subscription_id,
    )
    .await;
    if wire.methods.contains(&"session.interrupt") {
        fail(NAME_REPLY, "session.interrupt was sent");
    }

    let key = row_key(&created.session_id);
    let mut peek = peek_for(created.folder.path());
    let mut events: Events = None;
    timed_open(NAME_REPLY, &mut peek, &key).await;
    let running = until_state(&mut peek, &mut events, Duration::from_secs(30), |state| {
        matches!(state.turn(), TurnState::Running)
    })
    .await;
    if !running {
        fail(NAME_REPLY, "the peeked turn was not running");
    }
    let outcome = peek
        .reply("Now reply with the single word: queued-ok".to_owned())
        .await;
    if outcome != ReplyOutcome::Queued {
        fail(
            NAME_REPLY,
            &format!("the reply was not queued: {outcome:?}"),
        );
    }

    let mut saw_complete = false;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Some((_, state)) = peek.state() {
            match state.turn() {
                TurnState::Finished(TurnOutcome::Interrupted) => {
                    fail(NAME_REPLY, "session.interrupt was sent");
                }
                TurnState::Finished(TurnOutcome::Failed) => {
                    fail(NAME_REPLY, "the first turn failed");
                }
                TurnState::Finished(TurnOutcome::Completed) => saw_complete = true,
                TurnState::Idle | TurnState::Running => {}
            }
            // The reply's own text is in a user cell at once; the agent's answer shows the
            // queued turn ran.
            if saw_complete && output_has(state, "queued-ok") {
                break;
            }
        }
        if Instant::now() >= deadline {
            if !saw_complete {
                fail(NAME_REPLY, "the first turn did not complete");
            }
            fail(NAME_REPLY, "the queued reply did not start");
        }
        pump_for(&mut peek, &mut events, Duration::from_millis(50)).await;
    }
    pass(NAME_REPLY);
}

#[tokio::test]
#[ignore = "needs a Hermes gateway on the fixture profile"]
async fn live_answer_approval_from_peek() {
    let Some(mut wire) = gate(NAME_ANSWER).await else {
        return;
    };
    let created = create_session(NAME_ANSWER, &mut wire).await;
    let probe = created.folder.path().join("talaria-peek-probe");
    if std::fs::create_dir(&probe).is_err() {
        fail(NAME_ANSWER, "the probe folder could not be created");
    }
    submit(
        NAME_ANSWER,
        &mut wire,
        &created.session_id,
        "Run this exact shell command: rm -rf ./talaria-peek-probe",
    )
    .await;
    detach_creator(
        NAME_ANSWER,
        &mut wire,
        &created.session_id,
        &created.subscription_id,
    )
    .await;

    let key = row_key(&created.session_id);
    let mut peek = peek_for(created.folder.path());
    let mut events: Events = None;
    timed_open(NAME_ANSWER, &mut peek, &key).await;

    let deadline = Instant::now() + Duration::from_secs(60);
    let prompt_key = loop {
        if let Some((_, state)) = peek.state()
            && let Some(prompt) = state
                .open_prompts()
                .find(|prompt| prompt.kind == PromptKind::Approval)
        {
            break prompt.key.clone();
        }
        if Instant::now() >= deadline {
            fail(
                NAME_ANSWER,
                "no approval appeared; check that fixture's approval mode is manual",
            );
        }
        pump_for(&mut peek, &mut events, Duration::from_millis(50)).await;
    };
    let intent = PromptIntent::Approve {
        key: prompt_key,
        choice: "deny".to_owned(),
        note: None,
    };
    let card = match intent {
        PromptIntent::Approve { key, choice, note } => CardAnswer::Approve { key, choice, note },
        PromptIntent::Clarify { .. } | PromptIntent::Masked { .. } => {
            fail(NAME_ANSWER, "the approval intent was not an approval");
        }
    };
    let notices = peek.answer(card).await;
    if notices.iter().any(|notice| failure_notice(&notice.text)) {
        fail(
            NAME_ANSWER,
            "answering the approval returned a failure notice",
        );
    }
    let settled = until_state(
        &mut peek,
        &mut events,
        Duration::from_secs(5),
        approval_settled,
    )
    .await;
    if !settled {
        fail(NAME_ANSWER, "approval.settled did not arrive");
    }
    if !probe.exists() {
        fail(NAME_ANSWER, "the probe folder was removed");
    }
    pass(NAME_ANSWER);
}

fn failure_notice(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("fail") || lower.contains("error") || lower.contains("refus")
}

fn approval_settled(state: &SessionState) -> bool {
    state.cells().iter().any(|cell| match cell {
        Cell::Prompt(prompt) => {
            prompt.kind == PromptKind::Approval && matches!(prompt.state, PromptState::Settled(_))
        }
        _ => false,
    })
}

#[tokio::test]
#[ignore = "needs a Hermes gateway on the fixture profile"]
async fn live_stop_from_fleet() {
    let Some(mut wire) = gate(NAME_STOP).await else {
        return;
    };
    let created = create_session(NAME_STOP, &mut wire).await;
    submit(
        NAME_STOP,
        &mut wire,
        &created.session_id,
        "sleep 60 && echo too-late",
    )
    .await;
    detach_creator(
        NAME_STOP,
        &mut wire,
        &created.session_id,
        &created.subscription_id,
    )
    .await;

    let key = row_key(&created.session_id);
    let mut peek = peek_for(created.folder.path());
    let mut events: Events = None;
    timed_open(NAME_STOP, &mut peek, &key).await;
    let queued = peek
        .reply("Reply with the single word: after-stop".to_owned())
        .await;
    if queued != ReplyOutcome::Queued {
        fail(
            NAME_STOP,
            &format!("the follow-up reply was not queued: {queued:?}"),
        );
    }
    match peek.stop().await {
        StopOutcome::Stopping => {}
        other => fail(
            NAME_STOP,
            &format!("stop did not report stopping: {other:?}"),
        ),
    }

    let started_stop = Instant::now();
    let mut interrupt_at: Option<Instant> = None;
    let mut started = false;
    loop {
        let now = Instant::now();
        if let Some((_, state)) = peek.state() {
            // The prompt names the word itself; only the command's output means it ran.
            if command_output_has(state, "too-late") {
                fail(NAME_STOP, "too-late appeared in a cell");
            }
            let interrupted = matches!(state.turn(), TurnState::Finished(TurnOutcome::Interrupted))
                || state.cells().iter().any(|cell| {
                    matches!(
                        cell,
                        Cell::Notice(notice)
                            if notice.kind == talaria_core::session::NoticeKind::Interrupted
                    )
                });
            if interrupted && interrupt_at.is_none() {
                interrupt_at = Some(now);
            }
            if interrupt_at.is_some() && output_has(state, "after-stop") {
                started = true;
            }
        }
        if interrupt_at.is_none() && now.duration_since(started_stop) >= Duration::from_secs(10) {
            fail(NAME_STOP, "the running turn was not interrupted");
        }
        if let Some(when) = interrupt_at
            && !started
            && now.duration_since(when) >= Duration::from_secs(60)
        {
            println!("QUEUED_AFTER_STOP: not_started");
            fail(
                NAME_STOP,
                "queued prompt did not start after a stop: stop condition 4",
            );
        }
        if now.duration_since(started_stop) >= Duration::from_secs(70) {
            if started {
                println!("QUEUED_AFTER_STOP: started");
                pass(NAME_STOP);
                return;
            }
            println!("QUEUED_AFTER_STOP: not_started");
            fail(
                NAME_STOP,
                "queued prompt did not start after a stop: stop condition 4",
            );
        }
        pump_for(&mut peek, &mut events, Duration::from_millis(50)).await;
    }
}
