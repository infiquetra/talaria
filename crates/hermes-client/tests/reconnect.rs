//! Mock-gateway tests for liveness, reconnect and replay (rows S1 and 27 to 49).
//!
//! Every socket test runs in real time on the multi-thread runtime with
//! `LinkConfig::scaled_for_tests()` (durations divided by 100) and holds `serial()` for its
//! whole body. Production time in a comment is the scaled time times 100.
//!
//! Wait budgets are generous (seconds); a correct link finishes in tens of milliseconds. The
//! assertions that matter are exact values (`retry_in`, attempt numbers, sequence lists) and the
//! timing bounds in the issue table: lower bounds exact, upper bounds with 200 ms of slack.

#[path = "support/replay_mock.rs"]
mod replay_mock;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hermes_client::liveness::{
    Backoff, Dial, DialFailure, Dialed, GatewayDial, Link, LinkConfig, LinkEvent, LinkState,
};
use hermes_client::replay::{Cursor, GapReason, ReplayGap};
use hermes_client::{CallError, CallOptions, Client, Generation, Inbound};
use hermes_contract::canonical_events::SessionReplayGap;
use hermes_contract::events::{EventFrame, Notification};
use hermes_contract::methods::canonical::{session_events_since, session_resume};
use hermes_contract::schemas::{CanonicalEventsParams, CanonicalResumeParams, CanonicalSnapshot};
use replay_mock::{MockDial, MockGateway, RecordedRequest, serial};
use serde_json::{Value, json};
use tokio::sync::mpsc;

const WAIT: Duration = Duration::from_secs(8);

// ---------------------------------------------------------------------------------------
// Test rig
// ---------------------------------------------------------------------------------------

/// What the tests keep of one event. `LinkEvent` is neither `Clone` nor `Debug`.
#[derive(Clone, Debug)]
struct Ev {
    generation: u64,
    session: Option<String>,
    seq: Option<u64>,
    epoch: Option<String>,
    kind: String,
    replayed: bool,
    text: String,
}

#[derive(Clone, Debug)]
enum Rec {
    State(LinkState),
    Event(Ev),
    Gap(ReplayGap),
    Reattached {
        session: String,
        subscription: String,
    },
    Gone(String),
    Other(String),
}

#[derive(Clone, Debug)]
struct Logged {
    at: Instant,
    rec: Rec,
}

fn record(event: LinkEvent) -> Rec {
    match event {
        LinkEvent::State(state) => Rec::State(state),
        LinkEvent::Gap(gap) => Rec::Gap(gap),
        LinkEvent::Reattached {
            session_id,
            subscription_id,
        } => Rec::Reattached {
            session: session_id,
            subscription: subscription_id,
        },
        LinkEvent::SessionGone { session_id } => Rec::Gone(session_id),
        LinkEvent::Inbound { item, replayed } => match item {
            Inbound::Event(event) => Rec::Event(Ev {
                generation: event.generation.0,
                session: event.frame.session_id.clone(),
                seq: event.frame.seq,
                epoch: event.frame.replay_epoch.clone(),
                kind: event.frame.type_name().unwrap_or("").to_owned(),
                replayed,
                text: format!("{event:?} replayed={replayed}"),
            }),
            Inbound::ServerRequest(request) => Rec::Other(format!("{request:?}")),
            Inbound::Malformed(frame) => Rec::Other(format!("{frame:?}")),
            Inbound::Closed(closed) => Rec::Other(format!("{closed:?}")),
        },
    }
}

/// Counts every dial the link asks for, so a test can prove "one dial" or "no more dials".
struct Counted {
    inner: MockDial,
    calls: Arc<AtomicU64>,
}

impl Dial for Counted {
    fn dial(
        &mut self,
        generation: u64,
    ) -> impl std::future::Future<Output = Result<Dialed, DialFailure>> + Send {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.dial(generation)
    }
}

struct Rig {
    gw: MockGateway,
    link: Link,
    rx: mpsc::Receiver<LinkEvent>,
    log: Vec<Logged>,
    dials: Arc<AtomicU64>,
    home: PathBuf,
}

fn counted(dial: MockDial) -> (Counted, Arc<AtomicU64>, PathBuf) {
    let calls = Arc::new(AtomicU64::new(0));
    let home = dial.home_path();
    let counted = Counted {
        inner: dial,
        calls: Arc::clone(&calls),
    };
    (counted, calls, home)
}

impl Rig {
    async fn start(gw: &MockGateway, cfg: LinkConfig) -> Rig {
        Self::start_with(gw, MockDial::new(gw), cfg).await
    }

    async fn start_with(gw: &MockGateway, dial: MockDial, cfg: LinkConfig) -> Rig {
        let (dial, dials, home) = counted(dial);
        let (link, rx) = match Link::start(dial, cfg).await {
            Ok(started) => started,
            Err(failure) => panic!(
                "Link::start failed: {} ({})",
                failure.reason(),
                failure.next_step()
            ),
        };
        let mut rig = Rig {
            gw: gw.clone(),
            link,
            rx,
            log: Vec::new(),
            dials,
            home,
        };
        rig.until("first event is Connected generation 1", |log| {
            matches!(
                log.first().map(|logged| &logged.rec),
                Some(Rec::State(LinkState::Connected { generation: 1 }))
            )
        })
        .await;
        rig
    }

    fn take(&mut self, event: LinkEvent) {
        self.log.push(Logged {
            at: Instant::now(),
            rec: record(event),
        });
    }

    /// Read until `pred` holds over the whole log. Panics after `WAIT`.
    async fn until(&mut self, what: &str, pred: impl Fn(&[Logged]) -> bool) {
        self.until_within(WAIT, what, pred).await;
    }

    async fn until_within(
        &mut self,
        limit: Duration,
        what: &str,
        pred: impl Fn(&[Logged]) -> bool,
    ) {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            if pred(&self.log) {
                return;
            }
            match tokio::time::timeout_at(deadline, self.rx.recv()).await {
                Ok(Some(event)) => self.take(event),
                Ok(None) => panic!(
                    "event channel ended while waiting for {what}\n{}",
                    dump(&self.log)
                ),
                Err(_) => panic!("timed out waiting for {what}\n{}", dump(&self.log)),
            }
        }
    }

    /// Keep reading for `dur`, whatever arrives.
    async fn pump(&mut self, dur: Duration) {
        let deadline = tokio::time::Instant::now() + dur;
        while let Ok(Some(event)) = tokio::time::timeout_at(deadline, self.rx.recv()).await {
            self.take(event);
        }
    }

    /// Read until the mock has recorded a request matching `pred`.
    async fn wait_request(&mut self, what: &str, pred: impl Fn(&RecordedRequest) -> bool) {
        let deadline = Instant::now() + WAIT;
        loop {
            if self.gw.requests().iter().any(&pred) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "no request: {what}\n{}",
                dump(&self.log)
            );
            self.pump(Duration::from_millis(2)).await;
        }
    }

    fn client(&self) -> Client {
        self.link.client()
    }

    /// `close()`, then drain the channel to its end. Returns how long `close()` took.
    async fn close(self) -> (Duration, Vec<Logged>) {
        let Rig {
            link,
            mut rx,
            mut log,
            ..
        } = self;
        let started = Instant::now();
        link.close().await;
        let took = started.elapsed();
        while let Ok(Some(event)) = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
            log.push(Logged {
                at: Instant::now(),
                rec: record(event),
            });
        }
        (took, log)
    }
}

fn dump(log: &[Logged]) -> String {
    let first = log.first().map(|logged| logged.at);
    let mut out = String::from("event log so far:\n");
    for logged in log {
        let ms = first.map_or(0, |at| logged.at.duration_since(at).as_millis());
        out.push_str(&format!("  +{ms:>5} ms {:?}\n", logged.rec));
    }
    out
}

// ---------------------------------------------------------------------------------------
// Log queries
// ---------------------------------------------------------------------------------------

/// Items that are not events, states, gaps or session notices (server requests, malformed).
fn others(log: &[Logged]) -> Vec<&str> {
    log.iter()
        .filter_map(|l| match &l.rec {
            Rec::Other(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn event_count(log: &[Logged]) -> usize {
    log.iter()
        .filter(|l| matches!(l.rec, Rec::Event(_)))
        .count()
}

/// Sequence numbers delivered for `session`, in delivery order.
fn delivered(log: &[Logged], session: &str) -> Vec<u64> {
    log.iter()
        .filter_map(|l| match &l.rec {
            Rec::Event(ev) if ev.session.as_deref() == Some(session) => ev.seq,
            _ => None,
        })
        .collect()
}

fn delivered_in(log: &[Logged], session: &str, epoch: &str) -> Vec<u64> {
    log.iter()
        .filter_map(|l| match &l.rec {
            Rec::Event(ev)
                if ev.session.as_deref() == Some(session) && ev.epoch.as_deref() == Some(epoch) =>
            {
                ev.seq
            }
            _ => None,
        })
        .collect()
}

fn last_delivered(log: &[Logged], session: &str) -> Option<u64> {
    delivered(log, session).last().copied()
}

fn gaps(log: &[Logged]) -> Vec<ReplayGap> {
    log.iter()
        .filter_map(|l| match &l.rec {
            Rec::Gap(gap) => Some(gap.clone()),
            _ => None,
        })
        .collect()
}

fn gone(log: &[Logged]) -> Vec<String> {
    log.iter()
        .filter_map(|l| match &l.rec {
            Rec::Gone(id) => Some(id.clone()),
            _ => None,
        })
        .collect()
}

fn reattached(log: &[Logged], session: &str) -> Vec<String> {
    log.iter()
        .filter_map(|l| match &l.rec {
            Rec::Reattached {
                session: id,
                subscription,
            } if id == session => Some(subscription.clone()),
            _ => None,
        })
        .collect()
}

fn reconnecting(log: &[Logged]) -> Vec<(u32, Duration, String)> {
    log.iter()
        .filter_map(|l| match &l.rec {
            Rec::State(LinkState::Reconnecting {
                attempt,
                retry_in,
                reason,
            }) => Some((*attempt, *retry_in, reason.clone())),
            _ => None,
        })
        .collect()
}

fn failed(log: &[Logged]) -> Vec<(String, String)> {
    log.iter()
        .filter_map(|l| match &l.rec {
            Rec::State(LinkState::Failed { reason, next_step }) => {
                Some((reason.clone(), next_step.clone()))
            }
            _ => None,
        })
        .collect()
}

fn connected_ix(log: &[Logged], generation: u64) -> Option<usize> {
    log.iter().position(|l| {
        matches!(&l.rec, Rec::State(LinkState::Connected { generation: g }) if *g == generation)
    })
}

fn is_connected(log: &[Logged], generation: u64) -> bool {
    connected_ix(log, generation).is_some()
}

// ---------------------------------------------------------------------------------------
// Session helpers
// ---------------------------------------------------------------------------------------

struct Watched {
    id: String,
    epoch: String,
    subscription: String,
}

async fn resume(client: &Client, id: &str) -> CanonicalSnapshot {
    let params = CanonicalResumeParams {
        session_id: id.to_owned(),
        ..Default::default()
    };
    client
        .call_with::<session_resume::Marker>(
            params,
            CallOptions {
                timeout: Some(Duration::from_secs(5)),
            },
        )
        .await
        .unwrap_or_else(|error| panic!("session.resume {id}: {error}"))
}

fn cursor_of(snapshot: &CanonicalSnapshot) -> Cursor {
    Cursor {
        replay_epoch: snapshot.replay_epoch.clone(),
        last_sequence: u64::try_from(snapshot.last_sequence).expect("non-negative sequence"),
    }
}

/// A new mock session, attached through the link's client and tracked from the snapshot.
async fn watch(rig: &Rig) -> Watched {
    let id = rig.gw.add_session();
    let snapshot = resume(&rig.client(), &id).await;
    rig.link.track(&id, cursor_of(&snapshot));
    Watched {
        id,
        epoch: snapshot.replay_epoch,
        subscription: snapshot.subscription_id,
    }
}

fn emit(gw: &MockGateway, session: &str, count: u32) {
    for n in 0..count {
        gw.publish(session, "message.delta", json!({ "text": n.to_string() }));
    }
}

/// Ladder with a long first wait, so a test can publish while the link is surely down.
fn roomy() -> LinkConfig {
    LinkConfig {
        backoff: Backoff {
            floor: Duration::from_millis(200),
            cap: Duration::from_millis(400),
        },
        ..LinkConfig::scaled_for_tests()
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn requests_on(gw: &MockGateway, min_connection: u64, method: &str) -> Vec<RecordedRequest> {
    gw.requests()
        .into_iter()
        .filter(|r| r.connection >= min_connection && r.method == method)
        .collect()
}

fn keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn key_set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn session_param(request: &RecordedRequest) -> &str {
    request.params["session_id"].as_str().unwrap_or("")
}

// ---------------------------------------------------------------------------------------
// S1: the mock speaks the pinned contract (uses no liveness code)
// ---------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mock_speaks_the_pinned_contract() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let id = gw.add_session();
    let mut dial = MockDial::new(&gw);

    // gateway.ready decodes as GatewayReadyPayload inside the attach handshake.
    let dialed = dial
        .dial(1)
        .await
        .unwrap_or_else(|failure| panic!("mock dial: {}", failure.reason()));
    assert_eq!(dialed.process_epoch, "proc-1");
    assert!(dialed.heartbeat, "heartbeat: true is advertised by default");
    assert!(
        dialed
            .capabilities
            .iter()
            .any(|capability| capability == "event-replay-v1")
    );
    gw.set_heartbeat(None);
    let without = dial
        .dial(2)
        .await
        .unwrap_or_else(|failure| panic!("mock dial: {}", failure.reason()));
    assert!(!without.heartbeat, "a missing heartbeat is not advertised");
    drop(without);
    gw.set_heartbeat(Some(true));

    let (client, mut inbound) = Client::start(dialed.parts);
    let options = CallOptions {
        timeout: Some(Duration::from_secs(5)),
    };

    // session.resume decodes as CanonicalSnapshot; a second resume keeps the subscription.
    let snapshot = resume(&client, &id).await;
    assert_eq!(snapshot.session_id, id);
    assert_eq!(snapshot.last_sequence, 0);
    assert!(!snapshot.replay_epoch.is_empty());
    assert!(!snapshot.subscription_id.is_empty());
    assert_eq!(
        resume(&client, &id).await.subscription_id,
        snapshot.subscription_id
    );

    // A published event arrives live with session_id, seq and replay_epoch.
    gw.publish(&id, "message.delta", json!({ "text": "one" }));
    gw.publish(&id, "message.delta", json!({ "text": "two" }));
    for expected in 1..=2u64 {
        let Some(Inbound::Event(event)) = next_inbound(&mut inbound).await else {
            panic!("expected a live event");
        };
        assert_eq!(event.frame.session_id.as_deref(), Some(id.as_str()));
        assert_eq!(event.frame.seq, Some(expected));
        assert_eq!(
            event.frame.replay_epoch.as_deref(),
            Some(snapshot.replay_epoch.as_str())
        );
        assert!(matches!(
            event.frame.notification,
            Notification::MessageDelta(_)
        ));
    }

    // session.events.since decodes as CanonicalEventsResult and its events decode as frames.
    let since = |epoch: Option<&str>, cursor: i64| CanonicalEventsParams {
        last_seen: 0,
        last_sequence: cursor,
        profile: None,
        replay_epoch: epoch.map(str::to_owned),
        session_id: id.clone(),
    };
    let window = client
        .call_with::<session_events_since::Marker>(since(Some(&snapshot.replay_epoch), 0), options)
        .await
        .unwrap_or_else(|error| panic!("session.events.since: {error}"));
    assert!(!window.truncated && !window.snapshot_required);
    assert_eq!(
        (window.count, window.latest_seq, window.last_sequence),
        (2, 2, 2)
    );
    assert_eq!(window.epoch, snapshot.replay_epoch);
    assert_eq!(window.replay_epoch, snapshot.replay_epoch);
    let seqs: Vec<i64> = window.events.iter().map(|event| event.seq).collect();
    assert_eq!(seqs, vec![1, 2]);
    for event in &window.events {
        let params = serde_json::to_value(event).expect("event encodes");
        let frame = EventFrame::from_params(&params).expect("replay event decodes as a frame");
        assert_eq!(frame.seq, u64::try_from(event.seq).ok());
        assert!(matches!(frame.notification, Notification::MessageDelta(_)));
    }

    // Behind the cursor's epoch or ahead of the latest sequence: nothing, both flags (R6).
    for (epoch, cursor) in [
        (Some("other-epoch"), 0),
        (Some(snapshot.replay_epoch.as_str()), 9),
    ] {
        let forced = client
            .call_with::<session_events_since::Marker>(since(epoch, cursor), options)
            .await
            .unwrap_or_else(|error| panic!("session.events.since: {error}"));
        assert!(forced.truncated && forced.snapshot_required && forced.events.is_empty());
    }
    // No replay_epoch at all always demands a snapshot (R7).
    let bare = client
        .call_with::<session_events_since::Marker>(since(None, 0), options)
        .await
        .unwrap_or_else(|error| panic!("session.events.since: {error}"));
    assert!(bare.snapshot_required && bare.events.is_empty());

    // The server's own session.replay_gap: no seq, typed payload, subscription retired.
    gw.overflow(&id);
    let Some(Inbound::Event(event)) = next_inbound(&mut inbound).await else {
        panic!("expected the server gap");
    };
    assert_eq!(event.frame.seq, None);
    assert_eq!(event.frame.session_id.as_deref(), Some(id.as_str()));
    let Notification::SessionReplayGap(SessionReplayGap {
        latest_seq,
        replay_epoch,
    }) = &event.frame.notification
    else {
        panic!("session.replay_gap must decode as SessionReplayGap");
    };
    assert_eq!(*latest_seq, Some(2));
    assert_eq!(
        replay_epoch.as_deref(),
        Some(snapshot.replay_epoch.as_str())
    );
    let renewed = resume(&client, &id).await;
    assert_ne!(
        renewed.subscription_id, snapshot.subscription_id,
        "a retired subscription is replaced by a new id"
    );

    // A refusal for an unknown key is 4001 with reason invalid_params (R9).
    let refused = client
        .call_raw(
            "session.events.since",
            json!({
                "session_id": id,
                "last_sequence": 0,
                "replay_epoch": snapshot.replay_epoch,
                "bogus": true
            }),
            options,
        )
        .await;
    let Err(CallError::Rpc(error)) = refused else {
        panic!("an unknown key must be refused");
    };
    assert_eq!(error.code, 4001);
    assert_eq!(error.reason.as_deref(), Some("invalid_params"));

    // An unknown session is refused as not_found (R11), an unknown method as -32601 (R10).
    let missing = client
        .call_raw(
            "session.resume",
            json!({ "session_id": "no-such-session" }),
            options,
        )
        .await;
    let Err(CallError::Rpc(error)) = missing else {
        panic!("an unknown session must be refused");
    };
    assert_eq!(error.reason.as_deref(), Some("not_found"));
    let unknown = client.call_raw("session.create", json!({}), options).await;
    let Err(CallError::Rpc(error)) = unknown else {
        panic!("session.create is not implemented by the mock");
    };
    assert!(error.is_method_not_found());

    // gateway.ping is answered with {"ok": true}.
    let pong = client
        .call_raw("gateway.ping", json!({}), options)
        .await
        .expect("ping answered");
    assert_eq!(pong, json!({ "ok": true }));
}

async fn next_inbound(inbound: &mut hermes_client::InboundStream) -> Option<Inbound> {
    tokio::time::timeout(Duration::from_secs(5), inbound.next())
        .await
        .expect("an inbound item within 5 s")
}

// ---------------------------------------------------------------------------------------
// 27 to 37: replay
// ---------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn killed_socket_mid_stream_delivers_every_event_once_in_order() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let cfg = LinkConfig {
        event_buffer: 8,
        ..LinkConfig::scaled_for_tests()
    };
    let mut rig = Rig::start(&gw, cfg).await;
    let a = watch(&rig).await;
    let b = watch(&rig).await;

    // 200 events per session, the socket killed after event 120, the gateway publishing on
    // through the outage.
    let publisher = tokio::spawn({
        let gw = gw.clone();
        let ids = [a.id.clone(), b.id.clone()];
        async move {
            for n in 1..=200u32 {
                for id in &ids {
                    gw.publish(id, "message.delta", json!({ "text": n.to_string() }));
                }
                if n == 120 {
                    gw.kill();
                }
                tokio::time::sleep(ms(1)).await;
            }
        }
    });

    // The consumer stops reading for 200 ms (20 s production) after event 60.
    rig.until("60 events delivered", |log| event_count(log) >= 60)
        .await;
    tokio::time::sleep(ms(200)).await;
    rig.until("both sessions delivered up to 200", |log| {
        last_delivered(log, &a.id) == Some(200) && last_delivered(log, &b.id) == Some(200)
    })
    .await;
    publisher.await.expect("publisher finished");
    rig.pump(ms(200)).await;

    let expected: Vec<u64> = (1..=200).collect();
    assert_eq!(delivered(&rig.log, &a.id), expected, "session a");
    assert_eq!(delivered(&rig.log, &b.id), expected, "session b");
    assert!(
        gaps(&rig.log).is_empty(),
        "no gap is expected\n{}",
        dump(&rig.log)
    );
    assert!(
        others(&rig.log).is_empty(),
        "unexpected items: {:?}",
        others(&rig.log)
    );
    assert!(is_connected(&rig.log, 2), "the link reconnected");
    assert!(
        rig.log
            .iter()
            .any(|l| matches!(&l.rec, Rec::Event(ev) if ev.replayed)),
        "some events can only have come from replay"
    );
    let stats = rig.link.stats();
    assert!(stats.reconnects >= 1);
    assert!(stats.replayed_events > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconnect_uses_fresh_ticket_and_reattaches_before_replay() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, roomy()).await;
    let a = watch(&rig).await;
    let b = watch(&rig).await;
    emit(&gw, &a.id, 3);
    emit(&gw, &b.id, 3);
    rig.until("3 events each", |log| {
        last_delivered(log, &a.id) == Some(3) && last_delivered(log, &b.id) == Some(3)
    })
    .await;

    // Two drops. Events published while the link is down can only arrive by replay.
    let mut next = 4;
    for generation in [2u64, 3] {
        let before = reconnecting(&rig.log).len();
        gw.kill();
        rig.until("Reconnecting", |log| reconnecting(log).len() > before)
            .await;
        emit(&gw, &a.id, 2);
        emit(&gw, &b.id, 2);
        rig.until("reconnected and caught up", |log| {
            is_connected(log, generation)
                && last_delivered(log, &a.id) == Some(next + 1)
                && last_delivered(log, &b.id) == Some(next + 1)
        })
        .await;
        next += 2;
    }
    rig.pump(ms(200)).await;

    // Three connections, three tickets, one upgrade each.
    assert_eq!(gw.upgrades(), 3);
    assert_eq!(gw.distinct_upgrade_tickets(), 3);

    let requests = gw.requests();
    for (connection, cursor) in [(2u64, 3u64), (3, 5)] {
        for watched in [&a, &b] {
            let mine: Vec<(usize, &RecordedRequest)> = requests
                .iter()
                .enumerate()
                .filter(|(_, r)| r.connection == connection && session_param(r) == watched.id)
                .collect();
            let resume_at = mine
                .iter()
                .find(|(_, r)| r.method == "session.resume")
                .map(|(index, _)| *index)
                .unwrap_or_else(|| {
                    panic!("no resume for {} on connection {connection}", watched.id)
                });
            let since = mine
                .iter()
                .find(|(_, r)| r.method == "session.events.since")
                .unwrap_or_else(|| {
                    panic!("no since for {} on connection {connection}", watched.id)
                });
            assert!(resume_at < since.0, "resume must come before since");
            assert_eq!(
                keys(&requests[resume_at].params),
                key_set(&["session_id"]),
                "re-attach sends exactly session_id"
            );
            assert_eq!(
                keys(&since.1.params),
                key_set(&["session_id", "last_sequence", "replay_epoch"]),
                "since sends exactly three keys"
            );
            assert_eq!(since.1.params["last_sequence"], json!(cursor));
            assert_eq!(since.1.params["replay_epoch"], json!(watched.epoch));
        }
    }

    // One Reattached per session per reconnect, carrying that connection's subscription.
    for watched in [&a, &b] {
        let subscriptions = reattached(&rig.log, &watched.id);
        assert_eq!(subscriptions.len(), 2, "{}", dump(&rig.log));
        assert!(subscriptions[0].starts_with("sub-2-"), "{subscriptions:?}");
        assert!(subscriptions[1].starts_with("sub-3-"), "{subscriptions:?}");
        assert_ne!(subscriptions[0], watched.subscription);
    }

    // Replayed events follow their session's Reattached, never precede it.
    for watched in [&a, &b] {
        for generation in [2u64, 3] {
            let from = connected_ix(&rig.log, generation).expect("connected");
            let tail = &rig.log[from..];
            let reattach_at = tail
                .iter()
                .position(
                    |l| matches!(&l.rec, Rec::Reattached { session, .. } if *session == watched.id),
                )
                .expect("Reattached after Connected");
            let first_replayed = tail.iter().position(|l| {
                matches!(&l.rec, Rec::Event(ev) if ev.replayed && ev.session.as_deref() == Some(watched.id.as_str()))
            });
            // Replayed events are items of the current connection (REQ-18 step 6).
            if let Some(Rec::Event(ev)) = first_replayed.map(|at| &tail[at].rec) {
                assert_eq!(ev.generation, generation, "replayed event generation");
            }
            assert!(
                first_replayed.is_some_and(|at| at > reattach_at),
                "a replayed event must follow Reattached\n{}",
                dump(&rig.log)
            );
        }
    }
    for watched in [&a, &b] {
        assert_eq!(
            delivered(&rig.log, &watched.id),
            (1..=7).collect::<Vec<u64>>()
        );
    }
    assert!(gaps(&rig.log).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_frame_racing_replay_is_delivered_once() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, roomy()).await;
    let a = watch(&rig).await;
    emit(&gw, &a.id, 3);
    rig.until("3 events", |log| last_delivered(log, &a.id) == Some(3))
        .await;

    // Both replay calls are slow (50 ms against a 100 ms timeout), so live frames arrive
    // while the link is mid-replay: some before the resume answer, some before the window.
    gw.delay_replies("session.resume", &a.id, ms(50));
    gw.delay_replies("session.events.since", &a.id, ms(50));
    gw.kill();
    rig.until("Reconnecting", |log| !reconnecting(log).is_empty())
        .await;
    emit(&gw, &a.id, 3); // 4 to 6, missed
    rig.until("Connected 2", |log| is_connected(log, 2)).await;
    for _ in 0..6 {
        emit(&gw, &a.id, 1); // 7 to 12, live, racing the replay
        rig.pump(ms(15)).await;
    }
    rig.until("12 events", |log| last_delivered(log, &a.id) == Some(12))
        .await;
    rig.pump(ms(300)).await;

    assert_eq!(delivered(&rig.log, &a.id), (1..=12).collect::<Vec<u64>>());
    assert!(gaps(&rig.log).is_empty(), "{}", dump(&rig.log));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn truncated_replay_yields_exactly_one_gap() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    gw.set_ring_max(16);
    let mut rig = Rig::start(&gw, roomy()).await;
    let a = watch(&rig).await;
    emit(&gw, &a.id, 3);
    rig.until("3 events", |log| last_delivered(log, &a.id) == Some(3))
        .await;

    // 40 events are missed; the ring keeps 16, so the cursor is below its watermark.
    gw.kill();
    rig.until("Reconnecting", |log| !reconnecting(log).is_empty())
        .await;
    emit(&gw, &a.id, 40);
    rig.until("a gap", |log| !gaps(log).is_empty()).await;
    rig.pump(ms(400)).await;

    let seen = gaps(&rig.log);
    assert_eq!(seen.len(), 1, "exactly one gap\n{}", dump(&rig.log));
    assert_eq!(seen[0].session_id, a.id);
    assert_eq!(seen[0].reason, GapReason::SnapshotRequired);
    assert_eq!(
        delivered(&rig.log, &a.id),
        vec![1, 2, 3],
        "nothing from the window"
    );
    assert_eq!(
        rig.link.stats().gaps.get(&GapReason::SnapshotRequired),
        Some(&1)
    );

    // The consumer resumes, resyncs, and events flow again, each once.
    let snapshot = resume(&rig.client(), &a.id).await;
    assert_eq!(snapshot.last_sequence, 43);
    rig.link.resync(&a.id, cursor_of(&snapshot));
    emit(&gw, &a.id, 3);
    rig.until("44 to 46", |log| last_delivered(log, &a.id) == Some(46))
        .await;
    rig.pump(ms(200)).await;
    assert_eq!(delivered(&rig.log, &a.id), vec![1, 2, 3, 44, 45, 46]);
    assert_eq!(gaps(&rig.log).len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn epoch_change_forces_resume() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, LinkConfig::scaled_for_tests()).await;
    let a = watch(&rig).await;
    let b = watch(&rig).await;
    emit(&gw, &a.id, 2);
    emit(&gw, &b.id, 2);
    rig.until("2 events each", |log| {
        last_delivered(log, &a.id) == Some(2) && last_delivered(log, &b.id) == Some(2)
    })
    .await;

    // The gateway process restarts: a new process epoch, new session epochs, numbering from 0.
    gw.restart("proc-2");
    gw.kill();
    rig.until("a gap for each session", |log| gaps(log).len() >= 2)
        .await;
    rig.pump(ms(400)).await;

    let seen = gaps(&rig.log);
    assert_eq!(
        seen.len(),
        2,
        "exactly one gap per session\n{}",
        dump(&rig.log)
    );
    let sessions: BTreeSet<&str> = seen.iter().map(|gap| gap.session_id.as_str()).collect();
    assert_eq!(sessions, BTreeSet::from([a.id.as_str(), b.id.as_str()]));
    assert!(
        seen.iter()
            .all(|gap| gap.reason == GapReason::BackendRestarted)
    );
    // No replay calls at all on the new connection.
    assert!(requests_on(&gw, 2, "session.events.since").is_empty());
    assert!(requests_on(&gw, 2, "session.resume").is_empty());
    assert!(is_connected(&rig.log, 2));

    // The consumer resumes each session, resyncs, and the new numbering flows through.
    let client = rig.client();
    let mut fresh = Vec::new();
    for watched in [&a, &b] {
        let snapshot = resume(&client, &watched.id).await;
        assert_ne!(
            snapshot.replay_epoch, watched.epoch,
            "restart() renews the epoch"
        );
        assert_eq!(snapshot.last_sequence, 0);
        rig.link.resync(&watched.id, cursor_of(&snapshot));
        fresh.push(snapshot.replay_epoch);
    }
    emit(&gw, &a.id, 3);
    emit(&gw, &b.id, 3);
    rig.until("3 new events each", |log| {
        delivered_in(log, &a.id, &fresh[0]).len() == 3
            && delivered_in(log, &b.id, &fresh[1]).len() == 3
    })
    .await;
    rig.pump(ms(200)).await;
    assert_eq!(delivered_in(&rig.log, &a.id, &fresh[0]), vec![1, 2, 3]);
    assert_eq!(delivered_in(&rig.log, &b.id, &fresh[1]), vec![1, 2, 3]);
    assert_eq!(delivered_in(&rig.log, &a.id, &a.epoch), vec![1, 2]);
    assert_eq!(gaps(&rig.log).len(), 2, "no further gap");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn server_overflow_gap_then_resync_resumes_live_delivery() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, LinkConfig::scaled_for_tests()).await;
    let a = watch(&rig).await;
    emit(&gw, &a.id, 3);
    rig.until("3 events", |log| last_delivered(log, &a.id) == Some(3))
        .await;

    gw.overflow(&a.id);
    rig.until("a gap", |log| !gaps(log).is_empty()).await;
    rig.pump(ms(300)).await;
    let seen = gaps(&rig.log);
    assert_eq!(seen.len(), 1, "exactly one gap\n{}", dump(&rig.log));
    assert_eq!(seen[0].session_id, a.id);
    assert_eq!(seen[0].reason, GapReason::ServerOverflow);
    assert!(
        !rig.log
            .iter()
            .any(|l| matches!(&l.rec, Rec::Event(ev) if ev.kind == "session.replay_gap")),
        "the server gap is not delivered as an event"
    );

    // The subscription was retired, so event 4 reaches nobody. The consumer resumes, which
    // re-subscribes under a new id and reports sequence 4, and resyncs there.
    emit(&gw, &a.id, 1);
    let snapshot = resume(&rig.client(), &a.id).await;
    assert_ne!(snapshot.subscription_id, a.subscription);
    assert_eq!(snapshot.last_sequence, 4);
    rig.link.resync(&a.id, cursor_of(&snapshot));
    emit(&gw, &a.id, 3);
    rig.until("5 to 7", |log| last_delivered(log, &a.id) == Some(7))
        .await;
    rig.pump(ms(200)).await;
    assert_eq!(delivered(&rig.log, &a.id), vec![1, 2, 3, 5, 6, 7]);
    assert_eq!(gaps(&rig.log).len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn drop_during_replay_repeats_the_window() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, roomy()).await;
    let a = watch(&rig).await;
    emit(&gw, &a.id, 3);
    rig.until("3 events", |log| last_delivered(log, &a.id) == Some(3))
        .await;

    // The window answer is held back for a second, so the drop lands while it is pending.
    gw.delay_replies("session.events.since", &a.id, Duration::from_secs(1));
    gw.kill();
    rig.until("Reconnecting", |log| reconnecting(log).len() == 1)
        .await;
    emit(&gw, &a.id, 3); // 4 to 6, missed
    rig.until("Connected 2", |log| is_connected(log, 2)).await;
    rig.wait_request("since on connection 2", |r| {
        r.connection == 2 && r.method == "session.events.since"
    })
    .await;
    gw.kill();
    rig.until("second Reconnecting", |log| reconnecting(log).len() == 2)
        .await;
    gw.clear_delays();
    rig.until("Connected 3 and the window", |log| {
        is_connected(log, 3) && last_delivered(log, &a.id) == Some(6)
    })
    .await;
    rig.pump(ms(300)).await;

    assert_eq!(delivered(&rig.log, &a.id), vec![1, 2, 3, 4, 5, 6]);
    assert!(gaps(&rig.log).is_empty(), "{}", dump(&rig.log));
    assert!(gone(&rig.log).is_empty());
    // The same cursor was asked on both connections.
    for connection in [2u64, 3] {
        let asked = requests_on(&gw, connection, "session.events.since");
        let asked: Vec<&RecordedRequest> = asked
            .iter()
            .filter(|r| r.connection == connection)
            .collect();
        assert_eq!(asked.len(), 1, "connection {connection}");
        assert_eq!(asked[0].params["last_sequence"], json!(3));
        assert_eq!(asked[0].params["replay_epoch"], json!(a.epoch));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gone_session_is_forgotten_others_replay() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, roomy()).await;
    let a = watch(&rig).await; // vanishes between its resume and its since
    let b = watch(&rig).await; // replays normally
    let ghost = "ghost-session";
    // A tracked session the gateway has never heard of: its resume is refused `not_found`.
    rig.link.track(
        ghost,
        Cursor {
            replay_epoch: "ghost-epoch".to_owned(),
            last_sequence: 5,
        },
    );
    emit(&gw, &a.id, 2);
    emit(&gw, &b.id, 2);
    rig.until("2 events each", |log| {
        last_delivered(log, &a.id) == Some(2) && last_delivered(log, &b.id) == Some(2)
    })
    .await;

    gw.delay_replies("session.resume", &a.id, ms(40));
    gw.kill();
    rig.until("Reconnecting", |log| !reconnecting(log).is_empty())
        .await;
    emit(&gw, &b.id, 3); // 3 to 5, missed
    rig.until("Connected 2", |log| is_connected(log, 2)).await;
    let a_id = a.id.clone();
    rig.wait_request("resume of a on connection 2", |r| {
        r.connection == 2 && r.method == "session.resume" && session_param(r) == a_id
    })
    .await;
    gw.remove_session(&a.id); // gone before the link asks for its window
    rig.until("both gone and b caught up", |log| {
        gone(log).len() >= 2 && last_delivered(log, &b.id) == Some(5)
    })
    .await;
    rig.pump(ms(300)).await;

    let mut forgotten = gone(&rig.log);
    forgotten.sort();
    let mut expected = vec![a.id.clone(), ghost.to_owned()];
    expected.sort();
    assert_eq!(forgotten, expected, "SessionGone once per vanished session");
    assert!(gaps(&rig.log).is_empty(), "{}", dump(&rig.log));
    assert_eq!(delivered(&rig.log, &b.id), vec![1, 2, 3, 4, 5]);
    assert_eq!(delivered(&rig.log, &a.id), vec![1, 2]);

    // Forgotten means forgotten: the next reconnect re-attaches only b.
    gw.kill();
    rig.until("Connected 3", |log| is_connected(log, 3)).await;
    rig.until("b re-attached again", |log| {
        reattached(log, &b.id).len() == 2
    })
    .await;
    rig.pump(ms(300)).await;
    let asked: BTreeSet<String> = requests_on(&gw, 3, "session.resume")
        .iter()
        .map(|r| session_param(r).to_owned())
        .collect();
    assert_eq!(
        asked,
        BTreeSet::from([b.id.clone()]),
        "only b is re-attached on connection 3"
    );
    assert_eq!(gone(&rig.log).len(), 2, "no second SessionGone");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resume_naming_another_session_is_a_gap() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, LinkConfig::scaled_for_tests()).await;
    let a = watch(&rig).await;
    let other = gw.add_session(); // b: never tracked
    emit(&gw, &a.id, 2);
    rig.until("2 events", |log| last_delivered(log, &a.id) == Some(2))
        .await;

    // A resume of `a` now attaches and returns `other` (a re-routed compressed session).
    gw.redirect(&a.id, &other);
    gw.kill();
    rig.until("a gap", |log| !gaps(log).is_empty()).await;
    rig.pump(ms(400)).await;

    let seen = gaps(&rig.log);
    assert_eq!(seen.len(), 1, "exactly one gap\n{}", dump(&rig.log));
    assert_eq!(seen[0].session_id, a.id);
    assert_eq!(seen[0].reason, GapReason::ReplayFailed);
    assert!(
        reattached(&rig.log, &a.id).is_empty(),
        "no Reattached for the old id"
    );
    assert!(
        reattached(&rig.log, &other).is_empty(),
        "the new id is not tracked"
    );
    assert!(
        gw.requests().iter().all(|r| r.method != "session.detach"),
        "the link never detaches"
    );
    assert!(requests_on(&gw, 2, "session.events.since").is_empty());

    // The connection is subscribed to the other session now; its frames pass through.
    emit(&gw, &other, 1);
    rig.until("an event of the other session", |log| {
        !delivered(log, &other).is_empty()
    })
    .await;
    let passed = rig
        .log
        .iter()
        .find_map(|l| match &l.rec {
            Rec::Event(ev) if ev.session.as_deref() == Some(other.as_str()) => Some(ev.clone()),
            _ => None,
        })
        .expect("event of the other session");
    assert!(!passed.replayed, "an ordinary live event");
    assert_eq!(passed.seq, Some(1));
    assert_eq!(gaps(&rig.log).len(), 1);
}

async fn hung_replay_case(method: &str, resume_completes: bool) {
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, roomy()).await;
    let a = watch(&rig).await;
    let b = watch(&rig).await;
    emit(&gw, &a.id, 2);
    emit(&gw, &b.id, 2);
    rig.until("2 events each", |log| {
        last_delivered(log, &a.id) == Some(2) && last_delivered(log, &b.id) == Some(2)
    })
    .await;

    // The answer for `a` is delayed 10 times the replay timeout (100 ms, 10 s production
    // turned into 1 s here).
    gw.delay_replies(method, &a.id, Duration::from_secs(1));
    gw.kill();
    rig.until("Reconnecting", |log| !reconnecting(log).is_empty())
        .await;
    emit(&gw, &a.id, 3);
    emit(&gw, &b.id, 3);
    rig.until("a gap", |log| !gaps(log).is_empty()).await;
    rig.until("b caught up", |log| last_delivered(log, &b.id) == Some(5))
        .await;

    let connected_at = rig.log[connected_ix(&rig.log, 2).expect("connected")].at;
    let gap_at = rig
        .log
        .iter()
        .find(|l| matches!(l.rec, Rec::Gap(_)))
        .map(|l| l.at)
        .expect("gap logged");
    let waited = gap_at.duration_since(connected_at);
    assert!(
        waited >= ms(90),
        "the gap cannot come before the timeout: {waited:?}"
    );
    assert!(
        waited <= ms(300),
        "the gap comes at the timeout, not at the late answer: {waited:?}"
    );

    // Let the late answer arrive; it must change nothing.
    rig.pump(Duration::from_millis(1200)).await;
    let seen = gaps(&rig.log);
    assert_eq!(seen.len(), 1, "exactly one gap\n{}", dump(&rig.log));
    assert_eq!(seen[0].session_id, a.id);
    assert_eq!(seen[0].reason, GapReason::ReplayFailed);
    assert_eq!(
        delivered(&rig.log, &a.id),
        vec![1, 2],
        "nothing delivered for a"
    );
    assert_eq!(
        delivered(&rig.log, &b.id),
        vec![1, 2, 3, 4, 5],
        "b is unaffected"
    );
    assert!(gone(&rig.log).is_empty());
    assert_eq!(
        reattached(&rig.log, &a.id).len(),
        usize::from(resume_completes),
        "Reattached only when the resume itself completed"
    );
    assert_eq!(reattached(&rig.log, &b.id).len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hung_replay_times_out_into_a_gap() {
    let _serial = serial().await;
    // Case 1: the resume hangs. Case 2: the resume is fine and the window hangs.
    hung_replay_case("session.resume", false).await;
    hung_replay_case("session.events.since", true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_replay_capability_means_gap() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, LinkConfig::scaled_for_tests()).await;
    let a = watch(&rig).await;
    let b = watch(&rig).await;
    emit(&gw, &a.id, 2);
    emit(&gw, &b.id, 2);
    rig.until("2 events each", |log| {
        last_delivered(log, &a.id) == Some(2) && last_delivered(log, &b.id) == Some(2)
    })
    .await;

    // The next gateway describes itself without event-replay-v1.
    gw.set_capabilities(vec!["session-authority-v1".to_owned()]);
    gw.kill();
    rig.until("a gap for each session", |log| gaps(log).len() >= 2)
        .await;
    rig.pump(ms(400)).await;

    let seen = gaps(&rig.log);
    assert_eq!(
        seen.len(),
        2,
        "exactly one gap per session\n{}",
        dump(&rig.log)
    );
    let sessions: BTreeSet<&str> = seen.iter().map(|gap| gap.session_id.as_str()).collect();
    assert_eq!(sessions, BTreeSet::from([a.id.as_str(), b.id.as_str()]));
    assert!(
        seen.iter()
            .all(|gap| gap.reason == GapReason::ReplayUnsupported)
    );
    assert!(requests_on(&gw, 2, "session.events.since").is_empty());
    assert!(requests_on(&gw, 2, "session.resume").is_empty());
}

// ---------------------------------------------------------------------------------------
// 38 to 40: heartbeat
// ---------------------------------------------------------------------------------------

fn pings(gw: &MockGateway) -> Vec<RecordedRequest> {
    gw.requests()
        .into_iter()
        .filter(|r| r.method.contains("ping"))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn silent_gateway_is_declared_dead_within_60s() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, LinkConfig::scaled_for_tests()).await;

    // Wait for two pings, then go silent just after a pong so the last frame is well defined.
    rig.wait_request("a ping", |r| r.method == "gateway.ping")
        .await;
    let seen = pings(&gw).len();
    let target = seen + 1;
    let deadline = Instant::now() + WAIT;
    while pings(&gw).len() < target {
        assert!(Instant::now() < deadline, "pings must keep coming");
        rig.pump(ms(2)).await;
    }
    rig.pump(ms(30)).await;
    gw.silence();
    rig.pump(ms(30)).await;
    let last_frame = rig
        .client()
        .stats()
        .await
        .last_frame_at
        .expect("frames were received");

    rig.until("a Reconnecting", |log| !reconnecting(log).is_empty())
        .await;
    let death_at = rig
        .log
        .iter()
        .find(|l| matches!(l.rec, Rec::State(LinkState::Reconnecting { .. })))
        .map(|l| l.at)
        .expect("Reconnecting logged");
    let (attempt, retry_in, reason) = reconnecting(&rig.log).remove(0);
    assert_eq!(reason, "heartbeat timeout");
    assert_eq!((attempt, retry_in), (1, ms(10)));
    let silence = death_at.duration_since(last_frame);
    assert!(silence >= ms(450), "dead after only {silence:?} of silence");
    assert!(silence <= ms(800), "dead only after {silence:?} of silence");

    // Pings are gateway.ping with empty params, never the contract's `ping`.
    let sent = pings(&gw);
    assert!(sent.len() >= 2, "pings were sent");
    for ping in &sent {
        assert_eq!(ping.method, "gateway.ping");
        assert_eq!(ping.params, json!({}));
    }
    // The link comes back on its own.
    rig.until("Connected 2", |log| is_connected(log, 2)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reply_traffic_counts_as_life() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    gw.ignore_pings();
    let mut rig = Rig::start(&gw, LinkConfig::scaled_for_tests()).await;

    // A call is answered every 50 ms (5 s production); the pings never are.
    let answered = Arc::new(AtomicU32::new(0));
    let failed = Arc::new(AtomicU32::new(0));
    let caller = tokio::spawn({
        let client = rig.client();
        let answered = Arc::clone(&answered);
        let failed = Arc::clone(&failed);
        async move {
            loop {
                let outcome = client
                    .call_raw(
                        "session.list",
                        json!({ "scope": "live", "limit": 5 }),
                        CallOptions {
                            timeout: Some(Duration::from_secs(1)),
                        },
                    )
                    .await;
                match outcome {
                    Ok(_) => answered.fetch_add(1, Ordering::SeqCst),
                    Err(_) => failed.fetch_add(1, Ordering::SeqCst),
                };
                tokio::time::sleep(ms(50)).await;
            }
        }
    });
    rig.pump(ms(1200)).await; // 120 s production
    caller.abort();

    assert!(
        reconnecting(&rig.log).is_empty(),
        "reply traffic is life\n{}",
        dump(&rig.log)
    );
    assert_eq!(
        rig.log.len(),
        1,
        "only Connected generation 1\n{}",
        dump(&rig.log)
    );
    assert!(answered.load(Ordering::SeqCst) >= 15);
    assert_eq!(failed.load(Ordering::SeqCst), 0);
    assert!(
        pings(&gw).len() >= 5,
        "the heartbeat ran and went unanswered: {} pings",
        pings(&gw).len()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_heartbeat_when_not_advertised() {
    let _serial = serial().await;
    // Missing and false both mean "no heartbeat" (REQ-1).
    for advertised in [None, Some(false)] {
        let gw = MockGateway::start().await;
        gw.set_heartbeat(advertised);
        let mut rig = Rig::start(&gw, LinkConfig::scaled_for_tests()).await;
        gw.silence();
        rig.pump(ms(1200)).await; // 120 s production
        assert!(
            pings(&gw).is_empty(),
            "heartbeat {advertised:?}: no pings expected"
        );
        assert!(
            reconnecting(&rig.log).is_empty(),
            "heartbeat {advertised:?}: no death by silence\n{}",
            dump(&rig.log)
        );
        assert_eq!(rig.log.len(), 1, "{}", dump(&rig.log));
        let (took, _) = rig.close().await;
        assert!(took < Duration::from_secs(2));
    }
}

// ---------------------------------------------------------------------------------------
// 41 to 43: the ladder and close
// ---------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn flapping_socket_does_not_reset_the_ladder() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    gw.close_after(ms(10)); // 1 s production, under the 50 ms (5 s) stable time
    let mut rig = Rig::start(&gw, LinkConfig::scaled_for_tests()).await;
    rig.until("three Reconnecting", |log| reconnecting(log).len() >= 3)
        .await;

    let seen = reconnecting(&rig.log);
    let ladder: Vec<(u32, Duration)> = seen.iter().take(3).map(|(k, d, _)| (*k, *d)).collect();
    assert_eq!(
        ladder,
        vec![(1, ms(10)), (2, ms(20)), (3, ms(40))],
        "{}",
        dump(&rig.log)
    );
    assert!(seen.iter().all(|(_, _, reason)| !reason.is_empty()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stable_connection_resets_the_ladder() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    gw.close_after(ms(10));
    let mut rig = Rig::start(&gw, LinkConfig::scaled_for_tests()).await;
    rig.until("three Reconnecting", |log| reconnecting(log).len() >= 3)
        .await;

    // Lift the flapping before the third wait (40 ms) ends, so the next connection stays up.
    gw.clear_close_after();
    let third = reconnecting(&rig.log).len();
    assert_eq!(third, 3, "{}", dump(&rig.log));
    let after = rig.log.len();
    rig.until("a stable connection", |log| {
        log[after..]
            .iter()
            .any(|l| matches!(l.rec, Rec::State(LinkState::Connected { .. })))
    })
    .await;
    // Three times the 50 ms stable time.
    rig.pump(ms(150)).await;
    assert_eq!(
        reconnecting(&rig.log).len(),
        third,
        "the connection stayed up"
    );

    gw.kill();
    rig.until("the next Reconnecting", |log| {
        reconnecting(log).len() > third
    })
    .await;
    let (attempt, retry_in, _) = reconnecting(&rig.log)[third].clone();
    assert_eq!((attempt, retry_in), (1, ms(10)), "{}", dump(&rig.log));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_cancels_the_backoff_sleep() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    // Unscaled on purpose: a 30 s sleep that only close() can end.
    let cfg = LinkConfig {
        backoff: Backoff {
            floor: Duration::from_secs(30),
            cap: Duration::from_secs(30),
        },
        ..LinkConfig::scaled_for_tests()
    };
    let mut rig = Rig::start(&gw, cfg).await;
    gw.kill();
    rig.until("Reconnecting", |log| !reconnecting(log).is_empty())
        .await;
    assert_eq!(reconnecting(&rig.log)[0].1, Duration::from_secs(30));

    let dials = Arc::clone(&rig.dials);
    let dialed_before_close = dials.load(Ordering::SeqCst);
    let (took, log) = rig.close().await;
    assert!(took < ms(100), "close() took {took:?}");
    assert!(
        matches!(
            log.last().map(|l| &l.rec),
            Some(Rec::State(LinkState::Closed))
        ),
        "the last event is Closed\n{}",
        dump(&log)
    );
    tokio::time::sleep(ms(100)).await;
    assert_eq!(gw.upgrades(), 1, "no new upgrade after close");
    assert_eq!(
        dials.load(Ordering::SeqCst),
        dialed_before_close,
        "no dial after close"
    );
}

// ---------------------------------------------------------------------------------------
// 44 and 45: calls across a drop
// ---------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pending_call_at_drop_is_unknown_never_success() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, LinkConfig::scaled_for_tests()).await;

    // A call the gateway answers only after a second.
    gw.delay_replies("session.list", "", Duration::from_secs(1));
    let call = tokio::spawn({
        let client = rig.client();
        async move {
            client
                .call_raw(
                    "session.list",
                    json!({ "scope": "live", "limit": 5 }),
                    CallOptions {
                        timeout: Some(Duration::from_secs(5)),
                    },
                )
                .await
        }
    });
    rig.wait_request("the call reached the gateway", |r| {
        r.method == "session.list"
    })
    .await;
    gw.kill();

    let outcome = tokio::time::timeout(Duration::from_secs(3), call)
        .await
        .expect("the call ends when the connection drops, not at its own timeout")
        .expect("call task");
    match outcome {
        Ok(value) => panic!("a call cut off by a drop must not succeed: {value}"),
        Err(error) => {
            assert!(matches!(error, CallError::Unknown { .. }), "{error:?}");
            assert!(error.outcome_unknown());
        }
    }

    // The link never replays the call by itself.
    rig.until("Connected 2", |log| is_connected(log, 2)).await;
    rig.pump(ms(300)).await;
    let sent = gw
        .requests()
        .iter()
        .filter(|r| r.method == "session.list")
        .count();
    assert_eq!(sent, 1, "the call was not retried");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn client_survives_reconnect() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, roomy()).await;
    let old = rig.client(); // taken before the drop
    let options = CallOptions {
        timeout: Some(Duration::from_secs(2)),
    };
    let params = json!({ "scope": "live", "limit": 5 });
    old.call_raw("session.list", params.clone(), options)
        .await
        .expect("works while connected");

    gw.kill();
    rig.until("Reconnecting", |log| !reconnecting(log).is_empty())
        .await;
    let during = old.call_raw("session.list", params.clone(), options).await;
    assert!(
        matches!(during, Err(CallError::NotConnected)),
        "while reconnecting: {during:?}"
    );

    rig.until("Connected 2", |log| is_connected(log, 2)).await;
    old.call_raw("session.list", params, options)
        .await
        .expect("the same clone works after the reconnect");
    assert_eq!(old.generation(), Generation(2));
}

// ---------------------------------------------------------------------------------------
// 46 to 48: dial failures
// ---------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn first_dial_failure_is_returned_not_retried() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let failures = [
        DialFailure::Retry {
            reason: "the gateway did not answer".to_owned(),
            next_step: "Start the gateway and try again.".to_owned(),
        },
        DialFailure::GiveUp {
            reason: "the gateway refused this user".to_owned(),
            next_step: "Check the gateway owner.".to_owned(),
        },
    ];
    for failure in failures {
        let mut mock = MockDial::new(&gw);
        mock.fail_on(1, failure.clone());
        let (dial, dials, _) = counted(mock);
        let outcome = Link::start(dial, LinkConfig::scaled_for_tests()).await;
        let Err(returned) = outcome else {
            panic!("a failed first dial must be returned");
        };
        assert_eq!(returned.reason(), failure.reason());
        assert_eq!(returned.next_step(), failure.next_step());
        assert!(!returned.reason().is_empty() && !returned.next_step().is_empty());

        // Longer than two ladder steps at the cap: a retry would have happened by now.
        tokio::time::sleep(ms(700)).await;
        assert_eq!(dials.load(Ordering::SeqCst), 1, "one dial, no retry");
        assert_eq!(gw.upgrades(), 0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn give_up_errors_end_in_failed_state() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut mock = MockDial::new(&gw);
    mock.fail_on(
        2,
        DialFailure::GiveUp {
            reason: "the gateway no longer serves this profile".to_owned(),
            next_step: "Check the profile name.".to_owned(),
        },
    );
    let mut rig = Rig::start_with(&gw, mock, LinkConfig::scaled_for_tests()).await;
    gw.kill();
    rig.until("Failed", |log| !failed(log).is_empty()).await;
    // Two ladder steps at the cap pass; a retry would show up as another dial.
    rig.pump(ms(700)).await;

    assert_eq!(
        failed(&rig.log),
        vec![(
            "the gateway no longer serves this profile".to_owned(),
            "Check the profile name.".to_owned()
        )],
        "one Failed\n{}",
        dump(&rig.log)
    );
    assert_eq!(
        rig.dials.load(Ordering::SeqCst),
        2,
        "no dial after the GiveUp"
    );
    let failed_at = rig
        .log
        .iter()
        .position(|l| matches!(l.rec, Rec::State(LinkState::Failed { .. })))
        .expect("Failed logged");
    assert!(
        rig.log[failed_at + 1..]
            .iter()
            .all(|l| !matches!(l.rec, Rec::State(LinkState::Reconnecting { .. }))),
        "nothing reconnects after Failed"
    );
    assert_eq!(gw.upgrades(), 1);

    // After Failed the task waits only for close().
    let (took, log) = rig.close().await;
    assert!(took < Duration::from_secs(2));
    assert!(matches!(
        log.last().map(|l| &l.rec),
        Some(Rec::State(LinkState::Closed))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hung_dial_times_out_and_retries() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut rig = Rig::start(&gw, LinkConfig::scaled_for_tests()).await;

    // From now on the upgrade is never answered.
    gw.hang_upgrades();
    gw.kill();
    rig.until("three Reconnecting", |log| reconnecting(log).len() >= 3)
        .await;
    let seen = reconnecting(&rig.log);
    assert_eq!((seen[0].0, seen[0].1), (1, ms(10)));
    assert!(!seen[0].2.is_empty());
    // The dial timeout (300 ms, 30 s production) ends each hung attempt.
    for (index, (attempt, retry_in, reason)) in seen.iter().enumerate().skip(1).take(2) {
        assert_eq!(reason, "dial timed out", "{}", dump(&rig.log));
        assert_eq!(*attempt, index as u32 + 1);
        assert_eq!(*retry_in, ms(10 * (1 << index)).min(ms(300)));
    }
    let times: Vec<Instant> = rig
        .log
        .iter()
        .filter(|l| matches!(l.rec, Rec::State(LinkState::Reconnecting { .. })))
        .map(|l| l.at)
        .collect();
    let between = times[2].duration_since(times[1]);
    // retry_in 20 ms, then 300 ms of hung dial.
    assert!(between >= ms(300), "{between:?}");
    assert!(between <= ms(320 + 200), "{between:?}");
    assert_eq!(gw.upgrades(), 1, "no upgrade was ever answered");

    // The link keeps trying: once the gateway answers again it connects.
    gw.allow_upgrades();
    rig.until("Connected 2", |log| is_connected(log, 2)).await;
}

// ---------------------------------------------------------------------------------------
// 49: no ticket, path or profile name in any text
// ---------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_ticket_or_path_in_events_or_debug() {
    let _serial = serial().await;
    let gw = MockGateway::start().await;
    let mut texts: Vec<String> = Vec::new();
    let mut paths: Vec<String> = vec![std::env::temp_dir().to_string_lossy().into_owned()];
    fn note_path(path: &std::path::Path, paths: &mut Vec<String>) {
        paths.push(path.to_string_lossy().into_owned());
        if let Ok(canonical) = path.canonicalize() {
            paths.push(canonical.to_string_lossy().into_owned());
        }
    }

    // First dial fails with a parked profile named p3probe: a GiveUp, returned by start.
    let mut first = MockDial::new(&gw);
    first.fail_parked_on(1, "p3probe");
    note_path(&first.home_path(), &mut paths);
    let (dial, _, _) = counted(first);
    let Err(returned) = Link::start(dial, LinkConfig::scaled_for_tests()).await else {
        panic!("a GiveUp on the first dial is returned");
    };
    texts.push(format!("{returned:?}"));
    texts.push(returned.reason().to_owned());
    texts.push(returned.next_step().to_owned());

    // A full run: connect, replay, a server gap, a reconnect, then a GiveUp that names
    // p3probe on the third dial.
    let mut second = MockDial::new(&gw);
    second.fail_parked_on(3, "p3probe");
    let mut rig = Rig::start_with(&gw, second, LinkConfig::scaled_for_tests()).await;
    note_path(&rig.home, &mut paths);
    texts.push(format!("{:?}", rig.link));
    let a = watch(&rig).await;
    emit(&gw, &a.id, 3);
    rig.until("3 events", |log| last_delivered(log, &a.id) == Some(3))
        .await;
    gw.kill();
    rig.until("Connected 2 and Reattached", |log| {
        is_connected(log, 2) && !reattached(log, &a.id).is_empty()
    })
    .await;
    gw.overflow(&a.id);
    rig.until("a gap", |log| !gaps(log).is_empty()).await;
    texts.push(format!("{:?}", rig.link));
    gw.kill();
    rig.until("Failed", |log| !failed(log).is_empty()).await;
    rig.pump(ms(200)).await;
    texts.push(format!("{:?}", rig.link));
    texts.push(format!("{:?}", rig.link.stats()));
    let (reason, next_step) = failed(&rig.log).remove(0);
    assert!(!reason.is_empty() && !next_step.is_empty());
    for logged in &rig.log {
        texts.push(format!("{:?}", logged.rec));
        if let Rec::Event(ev) = &logged.rec {
            texts.push(ev.text.clone());
        }
    }

    // The production dial and a profile named p3probe.
    let probe_home = rig.home.clone();
    let gateway_dial = GatewayDial::new(probe_home, "p3probe".to_owned());
    texts.push(format!("{gateway_dial:?}"));
    texts.push(format!("{gateway_dial:#?}"));

    for text in &texts {
        assert!(
            !gw.text_contains_ticket(text),
            "a ticket leaked into: {text}"
        );
        assert!(!text.contains("hermes-gateway-ticket"), "{text}");
        assert!(
            !text.contains("p3probe"),
            "a profile name leaked into: {text}"
        );
        for path in &paths {
            assert!(!text.contains(path.as_str()), "a path leaked into: {text}");
        }
    }
    assert!(texts.len() >= 10, "the run produced text to check");
    let (_, _) = rig.close().await;
}
