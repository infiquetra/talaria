//! Ignored live checks for liveness, reconnect and replay (rows 50 and 51).
//!
//! A missing gateway, a missing `fixture` profile, or a pin mismatch prints
//! `LIVE_RECONNECT_RESULT: SKIPPED (<reason>)` and returns; `TALARIA_LIVE_REQUIRE=1` turns
//! that skip into a failure. Every other problem panics. Output holds no ticket, no path and
//! no session id.

#[path = "support/fault_relay.rs"]
mod fault_relay;
mod support;

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use fault_relay::FaultRelay;
use hermes_client::liveness::{Backoff, GatewayDial, Link, LinkConfig, LinkEvent, LinkState};
use hermes_client::replay::{Cursor, GapReason};
use hermes_client::{CallError, Client, Inbound};
use hermes_contract::events::{EventFrame, Notification};
use hermes_contract::methods::canonical::{
    prompt_submit, session_create, session_detach, session_resume,
};
use hermes_contract::overrides::prompt_submit_params;
use hermes_contract::schemas::{
    CanonicalCreateParams, CanonicalResumeParams, CanonicalSnapshot, SessionDetachParams,
};
use serde_json::Value;
use support::{LiveEnv, live_preconditions, report_skip};
use tokio::sync::mpsc;
use tokio::time::{Instant, timeout_at};

const RESULT: &str = "LIVE_RECONNECT_RESULT";
const MODEL: &str = "minimax-m3.1-flash";
const PROVIDER: &str = "ollama-cloud";
const BUDGET: Duration = Duration::from_secs(170);
const COUNT_PROMPT: &str = "Count from 1 to 300, one number per line.";
const FILL_PROMPT: &str = "Count from 1 to 200, one number per line.";
const FINAL_PROMPT: &str = "Reply with the single word ready.";
/// Provider chunks, not one event per token. The count prompt produced 10
/// `message.delta` frames against Hermes v0.22.4, so the drop lands inside
/// that window.
const DROP_AFTER_DELTAS: u32 = 4;
/// The replay ring keeps 512 frames and this model stamps about 18 per turn,
/// so the old ten-turn cap cannot force a snapshot. Forty turns is the stop.
const FILL_TURN_CAP: u32 = 40;

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn dropped_link_mid_turn_replays_exactly_once() {
    let Some(env) = preconditions().await else {
        return;
    };
    let deadline = Instant::now() + BUDGET;
    let relay = start_relay(&env).await;
    let dial = GatewayDial::new(env.home.clone(), "fixture".to_string())
        .via_origin(relay.origin().to_string());
    let (link, mut rx) = start_link(dial, LinkConfig::default()).await;
    expect_connected(&mut rx, deadline).await;

    let cwd = tempfile::tempdir().expect("temporary folder");
    let client = link.client();
    let snapshot = create_session(&client, cwd.path()).await;
    let session = snapshot.session_id.clone();
    let cursor = cursor_of(&snapshot);
    link.track(&session, cursor.clone());
    let mut subscription = snapshot.subscription_id.clone();

    let sent = submit(&client, &session, COUNT_PROMPT, "live-reconnect-50")
        .await
        .unwrap_or_else(|err| panic!("prompt.submit failed: {err}"));
    // Row 50: the params that went out never carry `unattended: true`.
    let key = ["unat", "tended"].concat();
    assert!(
        sent.get(&key)
            .is_none_or(|value| value == &Value::Bool(false)),
        "prompt.submit params carried unattended"
    );

    let mut check = SeqCheck::default();
    check.start_at(&cursor);
    let mut deltas = 0u32;
    let mut completes = 0u32;
    let mut dropped = false;
    let mut reconnected = false;
    let mut reattached = false;
    let mut replayed = 0u64;
    loop {
        match next(&mut rx, deadline, "the turn to finish").await {
            LinkEvent::State(LinkState::Connected { generation }) => {
                say(&format!("STATE connected generation={generation}"));
                reconnected |= generation >= 2;
            }
            LinkEvent::State(LinkState::Reconnecting {
                attempt,
                retry_in,
                reason,
            }) => say(&format!(
                "STATE reconnecting attempt={attempt} retry_in={:.1}s reason={reason}",
                retry_in.as_secs_f64()
            )),
            LinkEvent::State(LinkState::Failed { reason, next_step }) => {
                panic!("link failed: {reason} | {next_step}")
            }
            LinkEvent::State(LinkState::Closed) => panic!("link closed before the turn finished"),
            LinkEvent::Reattached {
                session_id,
                subscription_id,
            } => {
                if session_id == session {
                    subscription = subscription_id;
                    reattached = true;
                    say("REATTACHED");
                }
            }
            LinkEvent::Gap(gap) => {
                say(&format!("GAP reason={}", gap.reason.as_str()));
                panic!("a dropped link must replay without a gap");
            }
            LinkEvent::SessionGone { .. } => panic!("the session vanished during the drop"),
            LinkEvent::Inbound {
                item: Inbound::Event(event),
                replayed: was_replayed,
            } => {
                if event.frame.session_id.as_deref() != Some(session.as_str()) {
                    continue;
                }
                check.see(&event.frame);
                replayed += u64::from(was_replayed);
                match classify(&event.frame) {
                    Seen::Delta => {
                        deltas += 1;
                        if deltas == DROP_AFTER_DELTAS && !dropped {
                            relay.drop();
                            dropped = true;
                        }
                    }
                    Seen::Complete => {
                        completes += 1;
                        break;
                    }
                    Seen::Other => {}
                }
            }
            LinkEvent::Inbound {
                item: Inbound::ServerRequest(request),
                ..
            } => {
                let _ = request.responder.fail(-32000, "declined by the test").await;
            }
            LinkEvent::Inbound { .. } => {}
        }
    }

    // A second `message.complete` or a late gap would arrive right behind the first.
    let quiet = Instant::now() + Duration::from_secs(1);
    while let Ok(Some(event)) = timeout_at(quiet, rx.recv()).await {
        match event {
            LinkEvent::Gap(gap) => {
                say(&format!("GAP reason={}", gap.reason.as_str()));
                panic!("a gap arrived after the turn");
            }
            LinkEvent::Inbound {
                item: Inbound::Event(event),
                ..
            } if event.frame.session_id.as_deref() == Some(session.as_str()) => {
                check.see(&event.frame);
                if matches!(classify(&event.frame), Seen::Complete) {
                    completes += 1;
                }
            }
            _ => {}
        }
    }

    assert!(
        dropped,
        "the turn ended after {deltas} message.delta events, before the drop point"
    );
    assert!(
        reconnected,
        "the link never reached generation 2 after the drop"
    );
    assert!(reattached, "the session was never re-attached");
    say(&format!("REPLAYED {replayed} events"));
    say(&format!(
        "SEQ CHECK delivered={} duplicates={} missing={}",
        check.delivered, check.duplicates, check.missing
    ));
    assert_eq!(check.duplicates, 0, "an event was delivered twice");
    assert_eq!(check.missing, 0, "an event was lost");
    assert_eq!(completes, 1, "expected exactly one message.complete");

    detach(&client, &session, &subscription).await;
    link.close().await;
    println!("{RESULT}: PASSED");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn long_outage_yields_one_gap_then_resync() {
    let Some(env) = preconditions().await else {
        return;
    };
    let deadline = Instant::now() + BUDGET;
    let relay = start_relay(&env).await;

    // Link A goes through the relay. A shorter backoff cap keeps the reconnect after
    // `restore()` inside the time budget; the ladder itself is covered by the mock tests.
    let config_a = LinkConfig {
        backoff: Backoff {
            floor: Duration::from_secs(1),
            cap: Duration::from_secs(5),
        },
        ..LinkConfig::default()
    };
    let dial_a = GatewayDial::new(env.home.clone(), "fixture".to_string())
        .via_origin(relay.origin().to_string());
    let (link_a, mut rx_a) = start_link(dial_a, config_a).await;
    expect_connected(&mut rx_a, deadline).await;

    let cwd = tempfile::tempdir().expect("temporary folder");
    let client_a = link_a.client();
    let created = create_session(&client_a, cwd.path()).await;
    let session = created.session_id.clone();
    link_a.track(&session, cursor_of(&created));

    // The outage: A cannot dial back in and its open connection is gone.
    relay.refuse();
    relay.drop();
    let mut gaps = 0u32;
    loop {
        match next(&mut rx_a, deadline, "link A to notice the outage").await {
            LinkEvent::State(LinkState::Reconnecting {
                attempt,
                retry_in,
                reason,
            }) => {
                say(&format!(
                    "STATE reconnecting attempt={attempt} retry_in={:.1}s reason={reason}",
                    retry_in.as_secs_f64()
                ));
                break;
            }
            LinkEvent::State(LinkState::Failed { reason, next_step }) => {
                panic!("link A failed: {reason} | {next_step}")
            }
            LinkEvent::Gap(gap) => {
                say(&format!("GAP reason={}", gap.reason.as_str()));
                gaps += 1;
            }
            _ => {}
        }
    }

    // Link B has no relay. It resumes the session and keeps it busy until the numbering has
    // moved past the gateway's 512-frame replay ring.
    let dial_b = GatewayDial::new(env.home.clone(), "fixture".to_string());
    let (link_b, mut rx_b) = start_link(dial_b, LinkConfig::default()).await;
    expect_connected(&mut rx_b, deadline).await;
    let client_b = link_b.client();
    let resumed_b = resume(&client_b, &session).await;
    assert_eq!(
        resumed_b.session_id, session,
        "link B resumed another session"
    );
    let epoch_b = resumed_b.replay_epoch.clone();
    let base = u64::try_from(resumed_b.last_sequence).expect("non-negative sequence");
    let mut top = base;
    let mut turns = 0u32;
    while top.saturating_sub(base) < 600 {
        turns += 1;
        assert!(
            turns <= FILL_TURN_CAP,
            "seq rose only {} in {FILL_TURN_CAP} turns",
            top - base
        );
        // Stop condition 7: a refusal here is reported, never retried.
        submit(
            &client_b,
            &session,
            FILL_PROMPT,
            &format!("live-reconnect-b-{turns}"),
        )
        .await
        .unwrap_or_else(|err| panic!("link B prompt.submit refused after resume: {err}"));
        loop {
            match next(&mut rx_b, deadline, "link B's turn to finish").await {
                LinkEvent::State(LinkState::Failed { reason, next_step }) => {
                    panic!("link B failed: {reason} | {next_step}")
                }
                LinkEvent::Inbound {
                    item: Inbound::Event(event),
                    ..
                } => {
                    if event.frame.session_id.as_deref() != Some(session.as_str()) {
                        continue;
                    }
                    if event.frame.replay_epoch.as_deref() == Some(epoch_b.as_str()) {
                        top = top.max(event.frame.seq.unwrap_or(0));
                    }
                    if matches!(classify(&event.frame), Seen::Complete) {
                        break;
                    }
                }
                LinkEvent::Inbound {
                    item: Inbound::ServerRequest(request),
                    ..
                } => {
                    let _ = request.responder.fail(-32000, "declined by the test").await;
                }
                _ => {}
            }
        }
    }

    // The network comes back. A must raise one gap and rebuild from a fresh snapshot.
    relay.restore();
    let gap = loop {
        match next(&mut rx_a, deadline, "link A's gap after restore").await {
            LinkEvent::Gap(gap) => break gap,
            LinkEvent::State(LinkState::Connected { generation }) => {
                say(&format!("STATE connected generation={generation}"));
            }
            LinkEvent::State(LinkState::Reconnecting { .. }) => {}
            LinkEvent::State(LinkState::Failed { reason, next_step }) => {
                panic!("link A failed: {reason} | {next_step}")
            }
            LinkEvent::Reattached { .. } => say("REATTACHED"),
            LinkEvent::SessionGone { .. } => panic!("the session vanished during the outage"),
            _ => {}
        }
    };
    gaps += 1;
    say(&format!("GAP reason={}", gap.reason.as_str()));
    assert!(
        matches!(
            gap.reason,
            GapReason::SnapshotRequired | GapReason::EpochChanged
        ),
        "unexpected gap reason {}",
        gap.reason.as_str()
    );
    assert_eq!(gap.session_id, session, "the gap named another session");

    // The consumer's part of the contract: resume, then resync with the snapshot's cursor.
    let snapshot = resume(&client_a, &session).await;
    assert_eq!(
        snapshot.session_id, session,
        "link A resumed another session"
    );
    let cursor = cursor_of(&snapshot);
    link_a.resync(&session, cursor.clone());
    say(&format!("RESYNCED at seq {}", cursor.last_sequence));

    // A final prompt from A streams live, contiguously from the resync cursor.
    let mut check = SeqCheck::default();
    check.start_at(&cursor);
    submit(&client_a, &session, FINAL_PROMPT, "live-reconnect-a-final")
        .await
        .unwrap_or_else(|err| panic!("link A prompt.submit refused after resync: {err}"));
    let mut deltas = 0u32;
    loop {
        match next(&mut rx_a, deadline, "link A's final turn").await {
            LinkEvent::Gap(gap) => {
                say(&format!("GAP reason={}", gap.reason.as_str()));
                panic!("a second gap after resync");
            }
            LinkEvent::State(LinkState::Failed { reason, next_step }) => {
                panic!("link A failed: {reason} | {next_step}")
            }
            LinkEvent::Inbound {
                item: Inbound::Event(event),
                ..
            } => {
                if event.frame.session_id.as_deref() != Some(session.as_str()) {
                    continue;
                }
                check.see(&event.frame);
                match classify(&event.frame) {
                    Seen::Delta => deltas += 1,
                    Seen::Complete => break,
                    Seen::Other => {}
                }
            }
            LinkEvent::Inbound {
                item: Inbound::ServerRequest(request),
                ..
            } => {
                let _ = request.responder.fail(-32000, "declined by the test").await;
            }
            _ => {}
        }
    }
    assert!(deltas >= 1, "the final reply did not stream");
    assert_eq!(
        check.duplicates, 0,
        "an event was delivered twice after resync"
    );
    assert_eq!(check.missing, 0, "an event was lost after resync");

    let quiet = Instant::now() + Duration::from_secs(1);
    while let Ok(Some(event)) = timeout_at(quiet, rx_a.recv()).await {
        if let LinkEvent::Gap(gap) = event {
            say(&format!("GAP reason={}", gap.reason.as_str()));
            gaps += 1;
        }
    }
    assert_eq!(gaps, 1, "expected exactly one gap for the outage");

    detach(&client_b, &session, &resumed_b.subscription_id).await;
    detach(&client_a, &session, &snapshot.subscription_id).await;
    link_b.close().await;
    link_a.close().await;
    println!("{RESULT}: PASSED");
}

async fn preconditions() -> Option<LiveEnv> {
    match live_preconditions().await {
        Ok(env) => Some(env),
        Err(reason) => {
            report_skip(RESULT, &reason);
            None
        }
    }
}

/// Put a relay in front of the gateway's WebSocket port, read from the discovered origin.
async fn start_relay(env: &LiveEnv) -> FaultRelay {
    let address: SocketAddr = env
        .endpoint
        .api_origin()
        .trim_start_matches("http://")
        .parse()
        .expect("the api origin is a loopback address and port");
    FaultRelay::start(address)
        .await
        .expect("the fault relay could not bind")
}

async fn start_link(dial: GatewayDial, config: LinkConfig) -> (Link, mpsc::Receiver<LinkEvent>) {
    Link::start(dial, config).await.unwrap_or_else(|failure| {
        panic!(
            "link start failed: {} | {}",
            failure.reason(),
            failure.next_step()
        )
    })
}

async fn expect_connected(rx: &mut mpsc::Receiver<LinkEvent>, deadline: Instant) {
    match next(rx, deadline, "the first connection").await {
        LinkEvent::State(LinkState::Connected { generation: 1 }) => {
            say("STATE connected generation=1");
        }
        _ => panic!("the first link event was not Connected generation 1"),
    }
}

async fn next(rx: &mut mpsc::Receiver<LinkEvent>, deadline: Instant, what: &str) -> LinkEvent {
    match timeout_at(deadline, rx.recv()).await {
        Ok(Some(event)) => event,
        Ok(None) => panic!("the link ended while waiting for {what}"),
        Err(_) => panic!("timed out waiting for {what}"),
    }
}

async fn create_session(client: &Client, cwd: &Path) -> CanonicalSnapshot {
    let params = CanonicalCreateParams {
        cwd: Some(cwd.to_string_lossy().into_owned()),
        model: Some(MODEL.to_string()),
        provider: Some(PROVIDER.to_string()),
        // Hermes v0.22.4 rejects the generated null default as an unreadable cap.
        max_turns: hermes_contract::schemas::MaxTurns::from(30),
        ..CanonicalCreateParams::default()
    };
    client
        .call::<session_create::Marker>(params)
        .await
        .unwrap_or_else(|err| panic!("session.create failed: {err}"))
}

async fn resume(client: &Client, session: &str) -> CanonicalSnapshot {
    let params = CanonicalResumeParams {
        session_id: session.to_string(),
        ..CanonicalResumeParams::default()
    };
    client
        .call::<session_resume::Marker>(params)
        .await
        .unwrap_or_else(|err| panic!("session.resume failed: {err}"))
}

fn cursor_of(snapshot: &CanonicalSnapshot) -> Cursor {
    Cursor {
        replay_epoch: snapshot.replay_epoch.clone(),
        last_sequence: u64::try_from(snapshot.last_sequence).expect("non-negative sequence"),
    }
}

/// Send one prompt through the contract's helper. Returns the params as sent.
async fn submit(
    client: &Client,
    session: &str,
    text: &str,
    submission_id: &str,
) -> Result<Value, CallError> {
    let params = prompt_submit_params(session, text, submission_id);
    let sent = serde_json::to_value(&params).expect("prompt.submit params encode");
    let key = ["unat", "tended"].concat();
    assert!(
        sent.get(&key)
            .is_none_or(|value| value == &Value::Bool(false)),
        "prompt.submit params carried unattended"
    );
    client
        .call::<prompt_submit::Marker>(params)
        .await
        .map(|_| sent)
}

/// The consumer's own detach; the link never sends one.
async fn detach(client: &Client, session: &str, subscription_id: &str) {
    let params = SessionDetachParams {
        end_reason: None,
        session_id: session.to_string(),
        subscription_id: subscription_id.to_string(),
    };
    let _ = client.call::<session_detach::Marker>(params).await;
}

enum Seen {
    Delta,
    Complete,
    Other,
}

fn classify(frame: &EventFrame) -> Seen {
    match &frame.notification {
        Notification::MessageDelta(_) => Seen::Delta,
        Notification::MessageComplete(_) => Seen::Complete,
        _ => Seen::Other,
    }
}

/// Counts what the link delivered for one session: events, repeats and skipped numbers.
#[derive(Default)]
struct SeqCheck {
    epoch: Option<String>,
    last: Option<u64>,
    delivered: u64,
    duplicates: u64,
    missing: u64,
}

impl SeqCheck {
    fn start_at(&mut self, cursor: &Cursor) {
        self.epoch = Some(cursor.replay_epoch.clone());
        self.last = Some(cursor.last_sequence);
    }

    fn see(&mut self, frame: &EventFrame) {
        let (Some(seq), Some(epoch)) = (frame.seq, frame.replay_epoch.as_deref()) else {
            return;
        };
        if self.epoch.as_deref() != Some(epoch) {
            self.epoch = Some(epoch.to_string());
            self.last = None;
        }
        self.delivered += 1;
        match self.last {
            Some(last) if seq <= last => self.duplicates += 1,
            Some(last) => {
                self.missing += seq - last - 1;
                self.last = Some(seq);
            }
            None => self.last = Some(seq),
        }
    }
}

fn say(line: &str) {
    assert!(
        !line.contains("hermes-gateway-ticket"),
        "ticket marker in live output"
    );
    assert!(!has_long_token(line), "live output has a long token");
    println!("{line}");
}

fn has_long_token(text: &str) -> bool {
    let mut run = 0;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            run += 1;
            if run >= 40 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}
