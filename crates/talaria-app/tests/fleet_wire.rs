//! Fleet link against the mock gateway. Real time; the clock is never paused.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mock_gateway::{MockGateway, Seen, StandaloneGateway};
use serde_json::json;
use talaria_app::fleet::{FleetConfig, FleetHandle, FleetLinkState, FleetService, FleetSnapshot};
use talaria_app::side_connection::SideScope;
use talaria_core::fleet::{ProfileStatus, RowState};

fn home() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary home")
}

fn names_30() -> Vec<String> {
    (1..=30)
        .map(|index| format!("fixture-{index:02}"))
        .collect()
}

fn start(home: &std::path::Path, cfg: FleetConfig) -> (StandaloneGateway, FleetHandle) {
    let gateway = StandaloneGateway::start(home);
    let handle = FleetService::start(home.to_path_buf(), "fixture".into(), cfg, |_| {});
    (gateway, handle)
}

async fn wait_connected(handle: &FleetHandle, budget: Duration) -> Arc<FleetSnapshot> {
    let started = Instant::now();
    loop {
        let snapshot = handle.snapshot();
        if matches!(snapshot.link, FleetLinkState::Connected) || started.elapsed() >= budget {
            return snapshot;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn methods(gateway: &StandaloneGateway) -> Vec<String> {
    gateway
        .seen()
        .into_iter()
        .filter_map(|seen| seen.method)
        .collect()
}

/// Poll the snapshot until `check` holds or `budget` passes.
async fn wait_for(
    handle: &FleetHandle,
    budget: Duration,
    check: impl Fn(&FleetSnapshot) -> bool,
) -> Arc<FleetSnapshot> {
    let started = Instant::now();
    loop {
        let snapshot = handle.snapshot();
        if check(&snapshot) || started.elapsed() >= budget {
            return snapshot;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// One session row for the mock's fleet tables.
fn session(id: &str, status: &str, count: u64) -> serde_json::Value {
    json!({
        "id": id, "title": "", "source": "tui", "status": status,
        "message_count": count, "model": "minimax-m3.1-flash", "started_at": 1_792_690_000.0
    })
}

fn approval(id: &str) -> serde_json::Value {
    json!({
        "request_id": id, "command": "rm -rf ./talaria-fleet-probe",
        "description": "recursive delete", "choices": ["once", "deny"], "allow_permanent": true
    })
}

fn params_keys(seen: &Seen) -> BTreeSet<String> {
    seen.params
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn lists_every_served_profile_with_exact_params() {
    let home = home();
    let names = names_30();
    let (gateway, handle) = start(home.path(), FleetConfig::default());
    gateway.set_fleet_profiles(names.clone());
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    assert!(
        matches!(snapshot.link, FleetLinkState::Connected),
        "the link is connected"
    );
    assert_eq!(
        snapshot.profiles.len(),
        names.len() + 2,
        "every served profile is listed"
    );
    for name in &names {
        assert!(
            snapshot.profiles.iter().any(|(profile, status)| {
                profile == name && matches!(status, ProfileStatus::Listed { truncated: false })
            }),
            "{name} is listed"
        );
        let lists = gateway.requests("session.list");
        let matched = lists
            .iter()
            .find(|seen| seen.params["profile"] == name.as_str());
        let Some(seen) = matched else {
            panic!("session.list named {name}");
        };
        assert_eq!(
            params_keys(seen),
            BTreeSet::from(["profile".into(), "scope".into(), "limit".into()])
        );
        assert_eq!(seen.params["scope"], "live");
        assert_eq!(seen.params["limit"], 200);
        let live = gateway
            .requests("session.active_list")
            .into_iter()
            .find(|seen| seen.params["profile"] == name.as_str());
        let Some(seen) = live else {
            panic!("session.active_list named {name}");
        };
        assert_eq!(params_keys(&seen), BTreeSet::from(["profile".into()]));
    }
}

#[tokio::test]
async fn host_scope_refused_falls_back_to_one_profile() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    gateway.set_refuse_host_scope(true);
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    match &snapshot.scope {
        SideScope::ProfileOnly { profile, why } => {
            assert_eq!(profile, "fixture");
            assert_eq!(why, "the gateway refused a host-scoped ticket");
        }
        other => panic!("expected one profile, saw {other:?}"),
    }
    for seen in gateway.seen() {
        if seen.params["profile"].is_string() {
            assert_eq!(
                seen.params["profile"], "fixture",
                "no call names another profile"
            );
        }
    }
    gateway.set_refuse_host_scope(false);
    handle.refresh_now();
    let again = wait_connected(&handle, Duration::from_secs(2)).await;
    assert!(
        matches!(again.scope, SideScope::Host { .. }),
        "a later dial retries host scope"
    );
    assert!(
        gateway
            .ticket_records()
            .iter()
            .any(|ticket| ticket.scope == "host"),
        "a host-scoped ticket was minted on the retry"
    );
}

#[tokio::test]
async fn sessions_changed_coalesces_and_respects_gaps() {
    let home = home();
    // Polls slowed so the only reads after the connect round are the ones the events cause.
    let mut cfg = FleetConfig::scaled_for_tests();
    cfg.hot_poll = Duration::from_secs(30);
    cfg.cold_poll = Duration::from_secs(30);
    cfg.backstop = Duration::from_secs(30);
    let (gateway, handle) = start(home.path(), cfg);
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    let per_round = snapshot.profiles.len();
    assert!(
        per_round >= 1,
        "the connect round listed the served profiles"
    );
    let live_before = gateway.requests("session.active_list").len();
    let list_before = gateway.requests("session.list").len();
    let mock = MockGateway::clone(&gateway);
    for _ in 0..5 {
        mock.send_event("sessions.changed", json!({})).await;
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        gateway.requests("session.active_list").len() - live_before,
        per_round,
        "one liveness round"
    );
    assert_eq!(
        gateway.requests("session.list").len() - list_before,
        per_round,
        "one roster round"
    );
    mock.send_event("sessions.changed", json!({})).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        gateway.requests("session.active_list").len() - live_before,
        2 * per_round,
        "one more liveness round"
    );
    assert_eq!(
        gateway.requests("session.list").len() - list_before,
        2 * per_round,
        "one trailing roster"
    );
    let _ = handle;
}

#[tokio::test]
async fn hot_and_cold_polls_follow_their_intervals() {
    let home = home();
    let cfg = FleetConfig::scaled_for_tests();
    let gateway = StandaloneGateway::start(home.path());
    let (sender, mut notices) = tokio::sync::mpsc::unbounded_channel();
    let handle = FleetService::start(
        home.path().to_path_buf(),
        "fixture".into(),
        cfg.clone(),
        move |notice| {
            let _ = sender.send(notice);
        },
    );
    gateway.set_fleet_profiles(vec![
        "fixture".into(),
        "fixture-1".into(),
        "fixture-2".into(),
    ]);
    gateway.set_fleet_session("fixture", session("hot-1", "waiting", 1));
    gateway.set_fleet_approvals("fixture", "hot-1", vec![approval("a1b2")]);
    gateway.set_fleet_profile_empty("fixture-1");
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    assert!(
        snapshot
            .queue
            .iter()
            .any(|item| item.id.key.session_id == "hot-1"),
        "a waiting session's approval is in the first snapshot"
    );
    tokio::time::sleep(cfg.hot_poll * 4).await;
    let hot = gateway
        .requests("session.active_list")
        .into_iter()
        .filter(|seen| seen.params["profile"] == "fixture")
        .collect::<Vec<_>>();
    assert!(hot.len() >= 3, "the hot profile is polled on its interval");
    for pair in hot[1..].windows(2) {
        let gap = pair[1].at.saturating_duration_since(pair[0].at);
        let low = cfg.hot_poll / 2;
        let high = cfg.hot_poll + cfg.hot_poll / 2;
        assert!(gap >= low && gap <= high, "hot interval drifted: {gap:?}");
    }
    let cold = gateway
        .requests("session.active_list")
        .into_iter()
        .filter(|seen| seen.params["profile"] == "fixture-1")
        .collect::<Vec<_>>();
    assert!(cold.len() >= 2, "a cold profile is polled");
    let gap = cold[cold.len() - 1]
        .at
        .saturating_duration_since(cold[cold.len() - 2].at);
    assert!(
        gap >= cfg.cold_poll / 2 && gap <= cfg.cold_poll + cfg.cold_poll / 2,
        "cold interval drifted: {gap:?}"
    );
    while notices.try_recv().is_ok() {}
    tokio::time::sleep(cfg.cold_poll * 2).await;
    assert!(
        notices.try_recv().is_err(),
        "an unchanged poll sends no changed notice"
    );
    let opened = Instant::now();
    gateway.set_fleet_session("fixture-1", session("cold-1", "waiting", 1));
    gateway.set_fleet_approvals("fixture-1", "cold-1", vec![approval("c1")]);
    let seen = wait_for(&handle, Duration::from_secs(2), |snapshot| {
        snapshot
            .queue
            .iter()
            .any(|item| item.id.key.session_id == "cold-1")
    })
    .await;
    assert!(
        seen.queue
            .iter()
            .any(|item| item.id.key.session_id == "cold-1"),
        "a cold profile's approval is queued"
    );
    assert!(
        opened.elapsed() <= cfg.cold_poll + cfg.cold_poll / 2 + Duration::from_millis(100),
        "within the cold interval plus two calls: {:?}",
        opened.elapsed()
    );
}

#[tokio::test]
async fn backstop_rediscovers_profiles() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    gateway.set_fleet_profiles(vec!["fixture".into()]);
    let _ = wait_connected(&handle, Duration::from_secs(2)).await;
    gateway.set_fleet_profiles(vec!["fixture".into(), "fixture-1".into()]);
    tokio::time::sleep(FleetConfig::scaled_for_tests().backstop + Duration::from_millis(400)).await;
    let snapshot = handle.snapshot();
    assert!(
        snapshot.profiles.iter().any(|(name, status)| {
            name == "fixture-1" && matches!(status, ProfileStatus::Listed { .. })
        }),
        "a profile found by the backstop is listed"
    );
}

#[tokio::test]
async fn waiting_session_reads_pending_approvals() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    gateway.set_live_session_ids(["local-3f2a"]);
    gateway.set_fleet_session("fixture", session("local-3f2a", "waiting", 4));
    gateway.set_fleet_approvals("fixture", "local-3f2a", vec![approval("a1b2")]);
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    let reads = gateway.requests("approval.pending");
    assert!(
        !reads.is_empty(),
        "a waiting session reads approval.pending"
    );
    let seen = &reads[0];
    assert_eq!(seen.params["session_id"], "local-3f2a");
    assert_eq!(seen.params["profile"], "fixture");
    assert_eq!(
        params_keys(seen),
        BTreeSet::from(["session_id".into(), "profile".into()])
    );
    assert!(
        snapshot
            .queue
            .iter()
            .any(|item| item.id.key.session_id == "local-3f2a"),
        "the approval is queued"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    let reads = gateway.requests("approval.pending");
    assert!(reads.len() >= 2, "the waiting session is read again");
    let gap = FleetConfig::scaled_for_tests().approvals_gap;
    for pair in reads.windows(2) {
        assert!(
            pair[1].at.saturating_duration_since(pair[0].at) >= gap / 2,
            "approval reads keep their gap"
        );
    }
}

#[tokio::test]
async fn never_reads_a_session_that_is_not_live() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    gateway.set_live_session_ids(["local-live"]);
    gateway.set_fleet_session("fixture", session("local-live", "waiting", 1));
    gateway.set_fleet_approvals("fixture", "local-live", vec![approval("a1b2")]);
    let _ = wait_connected(&handle, Duration::from_secs(2)).await;
    gateway.set_fleet_approvals("fixture", "local-live", Vec::new());
    gateway.set_fleet_session("fixture", session("local-live", "idle", 3));
    let started = Instant::now();
    while gateway.requests("session.events.since").is_empty()
        && started.elapsed() < Duration::from_secs(2)
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let approvals = gateway.requests("approval.pending");
    let events = gateway.requests("session.events.since");
    assert!(
        approvals
            .iter()
            .any(|seen| seen.params["session_id"] == "local-live"),
        "approval.pending was sent for a live id"
    );
    assert!(
        events
            .iter()
            .any(|seen| seen.params["session_id"] == "local-live"),
        "session.events.since was sent for a live id"
    );
    gateway.set_fleet_session("fixture", session("local-live", "working", 4));
    tokio::time::sleep(Duration::from_millis(150)).await;
    gateway.remove_fleet_session("fixture", "local-live");
    gateway.set_live_session_ids(["local-other"]);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        gateway.fleet_non_live_reads(),
        0,
        "a session that left the live list is not read"
    );
    let _ = handle;
}

fn completion(seq: u64, status: &str) -> serde_json::Value {
    json!({"type": "message.complete", "session_id": "local-3f2a", "seq": seq,
           "replay_epoch": "e1", "payload": {"text": "done", "status": status}})
}

fn delta(seq: u64) -> serde_json::Value {
    json!({"type": "message.delta", "session_id": "local-3f2a", "seq": seq,
           "replay_epoch": "e1", "payload": {"text": "x"}})
}

#[tokio::test]
async fn outcome_probe_uses_epoch_and_tail() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    gateway.set_live_session_ids(["local-3f2a"]);
    gateway.set_fleet_session("fixture", session("local-3f2a", "working", 4));
    let mut ring: Vec<serde_json::Value> = (50..=120).map(delta).collect();
    ring[118 - 50] = completion(118, "error");
    gateway.set_fleet_ring("fixture", "local-3f2a", "e1", ring, 50);
    let _ = wait_connected(&handle, Duration::from_secs(2)).await;
    gateway.set_fleet_session("fixture", session("local-3f2a", "idle", 6));
    let snapshot = wait_for(&handle, Duration::from_secs(2), |snapshot| {
        snapshot
            .rows
            .iter()
            .any(|row| row.key.session_id == "local-3f2a" && row.state == RowState::Failed)
    })
    .await;
    let probes = gateway.requests("session.events.since");
    assert!(probes.len() >= 2, "the probe makes two calls");
    assert_eq!(probes[0].params["last_sequence"], 0);
    assert!(probes[0].params.get("replay_epoch").is_none());
    assert_eq!(probes[1].params["last_sequence"], 56);
    assert_eq!(probes[1].params["replay_epoch"], "e1");
    assert!(
        snapshot
            .rows
            .iter()
            .any(|row| { row.key.session_id == "local-3f2a" && row.state == RowState::Failed }),
        "an error completion is failed"
    );
}

#[tokio::test]
async fn delayed_probe_head_does_not_advance_a_newer_probe() {
    let home = home();
    let mut cfg = FleetConfig::scaled_for_tests();
    cfg.probe_deadline = Duration::from_secs(6);
    cfg.call_timeout = Duration::from_secs(6);
    let (gateway, handle) = start(home.path(), cfg);
    gateway.set_live_session_ids(["local-3f2a"]);
    gateway.set_fleet_session("fixture", session("local-3f2a", "working", 4));
    gateway.set_fleet_ring(
        "fixture",
        "local-3f2a",
        "e1",
        vec![completion(1, "complete")],
        1,
    );
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    assert!(matches!(snapshot.link, FleetLinkState::Connected));
    gateway.push_method_result_after(
        "session.events.since",
        Duration::from_millis(1500),
        json!({"latest_seq": 1, "replay_epoch": "e1"}),
    );
    gateway.push_method_result_after(
        "session.events.since",
        Duration::from_millis(3000),
        json!({"latest_seq": 1, "replay_epoch": "e1"}),
    );
    gateway.set_fleet_session("fixture", session("local-3f2a", "idle", 5));
    tokio::time::timeout(Duration::from_secs(2), async {
        while gateway.requests("session.events.since").is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let live_before = gateway.requests("session.active_list").len();
    gateway.set_fleet_session("fixture", session("local-3f2a", "working", 6));
    tokio::time::timeout(Duration::from_secs(2), async {
        while gateway.requests("session.active_list").len() == live_before {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    gateway.set_fleet_session("fixture", session("local-3f2a", "idle", 7));
    tokio::time::timeout(Duration::from_secs(2), async {
        while gateway.requests("session.events.since").len() < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(1700)).await;
    assert_eq!(
        gateway.requests("session.events.since").len(),
        2,
        "the old head must not start a tail for the newer probe"
    );
    let snapshot = handle.snapshot();
    assert!(
        snapshot
            .rows
            .iter()
            .any(|row| row.key.session_id == "local-3f2a" && row.outcome.is_none())
    );
    let snapshot = wait_for(&handle, Duration::from_secs(3), |snapshot| {
        snapshot
            .rows
            .iter()
            .any(|row| row.key.session_id == "local-3f2a" && row.state == RowState::Done)
    })
    .await;
    assert!(
        snapshot
            .rows
            .iter()
            .any(|row| row.key.session_id == "local-3f2a" && row.state == RowState::Done)
    );
}

#[tokio::test]
async fn outcome_probe_unknown_cases() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    gateway.set_fleet_profiles(vec![
        "fixture".into(),
        "fixture-1".into(),
        "fixture-2".into(),
    ]);
    // Evicted: the tail starts below the oldest frame held.
    gateway.set_fleet_session("fixture", session("s-evicted", "working", 1));
    let ring: Vec<serde_json::Value> = (110..=120).map(delta).collect();
    gateway.set_fleet_ring("fixture", "s-evicted", "e1", ring, 110);
    // No events at all: the latest sequence is 0.
    gateway.set_fleet_session("fixture", session("s-empty", "working", 1));
    // Events but no completion.
    gateway.set_fleet_session("fixture", session("s-open", "working", 1));
    gateway.set_fleet_ring("fixture", "s-open", "e1", (1..=5).map(delta).collect(), 1);
    // A refused read.
    gateway.set_fleet_session("fixture-1", session("s-refused", "working", 1));
    gateway.set_fleet_ring(
        "fixture-1",
        "s-refused",
        "e1",
        vec![completion(1, "complete")],
        1,
    );
    for reason in ["not_found", "storage_unavailable"] {
        gateway.script_error("session.events.since", Some("fixture-1"), 4001, reason);
    }
    // No reply before the probe deadline.
    gateway.set_fleet_session("fixture-2", session("s-silent", "working", 1));
    gateway.set_fleet_ring(
        "fixture-2",
        "s-silent",
        "e1",
        vec![completion(1, "complete")],
        1,
    );
    gateway.set_silent_profile("fixture-2", "session.events.since", true);
    let _ = wait_connected(&handle, Duration::from_secs(2)).await;
    for (profile, id) in [
        ("fixture", "s-evicted"),
        ("fixture", "s-empty"),
        ("fixture", "s-open"),
        ("fixture-1", "s-refused"),
        ("fixture-2", "s-silent"),
    ] {
        gateway.set_fleet_session(profile, session(id, "idle", 1));
    }
    let ids = ["s-evicted", "s-empty", "s-open", "s-refused", "s-silent"];
    let snapshot = wait_for(&handle, Duration::from_secs(3), |snapshot| {
        ids.iter().all(|id| {
            snapshot
                .rows
                .iter()
                .any(|row| row.key.session_id == *id && row.outcome.is_some())
        })
    })
    .await;
    for id in ids {
        assert!(
            snapshot.rows.iter().any(|row| {
                row.key.session_id == id
                    && row.state == RowState::Done
                    && row.outcome == Some(talaria_core::fleet::Outcome::Unknown)
            }),
            "a probe that cannot see the completion stays unknown: {id}"
        );
    }
}

#[tokio::test]
async fn one_failing_profile_keeps_the_others() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    gateway.set_fleet_profiles(vec![
        "fixture".into(),
        "fixture-1".into(),
        "fixture-2".into(),
        "fixture-3".into(),
        "fixture-4".into(),
    ]);
    for reason in [
        "permission_denied",
        "runtime_retired",
        "profile_mismatch",
        "not_found",
    ] {
        gateway.script_error("session.list", Some("fixture-1"), 4001, reason);
    }
    gateway.script_error(
        "session.list",
        Some("fixture-1"),
        5001,
        "storage_unavailable",
    );
    gateway.script_error("session.list", Some("fixture-1"), -32601, "unknown_method");
    for profile in ["fixture-2", "fixture-3", "fixture-4"] {
        gateway.set_silent_profile(profile, "session.list", true);
        gateway.set_silent_profile(profile, "session.active_list", true);
    }
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    assert!(
        snapshot.profiles.iter().any(|(name, status)| {
            name == "fixture" && matches!(status, ProfileStatus::Listed { .. })
        }),
        "a healthy profile stays listed"
    );
    assert!(
        snapshot.profiles.iter().any(|(name, status)| {
            name == "fixture-1"
                && matches!(status, ProfileStatus::Failed { reason } if reason == "permission_denied")
        }),
        "a refused profile is failed and the others continue"
    );
    let healthy = gateway
        .requests("session.active_list")
        .into_iter()
        .filter(|seen| seen.params["profile"] == "fixture")
        .count();
    assert!(healthy >= 1, "the healthy profile still gets liveness");
    let cold = FleetConfig::scaled_for_tests().cold_poll;
    tokio::time::sleep(cold * 5).await;
    let later = gateway
        .requests("session.active_list")
        .into_iter()
        .filter(|seen| seen.params["profile"] == "fixture")
        .count();
    assert!(
        later >= healthy + 3,
        "profiles that never answer do not starve the healthy one: {healthy} then {later}"
    );
}

#[tokio::test]
async fn reconnect_marks_stale_then_refreshes() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    gateway.set_fleet_session("fixture", session("local-3f2a", "idle", 1));
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    assert!(
        matches!(snapshot.link, FleetLinkState::Connected),
        "the link connected before the drop"
    );
    assert!(!snapshot.rows.is_empty(), "a row is listed");
    let mock = MockGateway::clone(&gateway);
    mock.kill_connection().await;
    let dropped = wait_for(&handle, Duration::from_secs(2), |snapshot| {
        matches!(snapshot.link, FleetLinkState::Reconnecting { .. })
    })
    .await;
    assert!(
        dropped.rows.iter().all(|row| row.stale),
        "a dropped link marks every row stale"
    );
    let refreshed = wait_connected(&handle, Duration::from_secs(5)).await;
    assert!(
        matches!(refreshed.link, FleetLinkState::Connected),
        "the link is connected again"
    );
    assert!(
        refreshed.rows.iter().all(|row| !row.stale),
        "the new generation's roster clears stale"
    );
}

#[tokio::test]
async fn server_requests_are_refused() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    let _ = wait_connected(&handle, Duration::from_secs(2)).await;
    let mock = MockGateway::clone(&gateway);
    mock.server_request("approval").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let refused = gateway.answers().iter().any(|seen| {
        seen.params
            .to_string()
            .contains("does not support server request")
    });
    assert!(refused, "the server request is refused at once");
    assert_eq!(handle.snapshot().diagnostics.len(), 1, "one diagnostic");
}

#[tokio::test(flavor = "current_thread")]
async fn start_returns_at_once() {
    let home = home();
    let gateway = StandaloneGateway::start(home.path());
    let frames = gateway.frame_count();
    let (sender, mut notices) = tokio::sync::mpsc::unbounded_channel();
    let handle = FleetService::start(
        home.path().to_path_buf(),
        "fixture".into(),
        FleetConfig::scaled_for_tests(),
        move |notice| {
            let _ = sender.send(notice);
        },
    );
    assert_eq!(
        gateway.frame_count(),
        frames,
        "start returns before the mock sees a frame"
    );
    let started = Instant::now();
    let mut connected = false;
    while started.elapsed() < Duration::from_secs(2) {
        let snapshot = handle.snapshot();
        if matches!(snapshot.link, FleetLinkState::Connected)
            && snapshot
                .profiles
                .iter()
                .any(|(_, status)| matches!(status, ProfileStatus::Listed { .. }))
        {
            connected = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(connected, "the link is connected and the profile is listed");
    assert!(notices.try_recv().is_ok(), "a changed notice was published");
}

#[tokio::test]
async fn fleet_link_sends_only_allowed_methods() {
    let home = home();
    let allowed = [
        "client.capabilities",
        "runtime.describe",
        "gateway.ping",
        "session.list",
        "session.active_list",
        "approval.pending",
        "session.events.since",
    ];
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    gateway.set_live_session_ids(["local-3f2a"]);
    gateway.set_fleet_session("fixture", session("local-3f2a", "waiting", 1));
    gateway.set_fleet_approvals("fixture", "local-3f2a", vec![approval("a1b2")]);
    let _ = wait_connected(&handle, Duration::from_secs(2)).await;
    gateway.set_fleet_session("fixture", session("local-3f2a", "idle", 2));
    let started = Instant::now();
    while gateway.requests("session.events.since").is_empty()
        && started.elapsed() < Duration::from_secs(2)
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let seen = methods(&gateway);
    for method in [
        "session.list",
        "session.active_list",
        "approval.pending",
        "session.events.since",
    ] {
        assert!(seen.iter().any(|name| name == method), "{method} was sent");
    }
    for method in &seen {
        assert!(
            allowed.contains(&method.as_str()),
            "{method} is not on the fleet allow-list"
        );
    }
    for method in [
        "session.resume",
        "session.create",
        "session.detach",
        "prompt.submit",
        "approval.respond",
        "session.interrupt",
        "config.set",
    ] {
        assert!(!seen.iter().any(|name| name == method), "{method} was sent");
    }
}
