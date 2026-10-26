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
    let _ = handle.refresh_now();
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
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    let _ = wait_connected(&handle, Duration::from_secs(2)).await;
    let mock = MockGateway::clone(&gateway);
    for _ in 0..5 {
        mock.send_event("sessions.changed", json!({})).await;
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        gateway.requests("session.active_list").len(),
        1,
        "one liveness round"
    );
    assert_eq!(
        gateway.requests("session.list").len(),
        1,
        "one roster round"
    );
    mock.send_event("sessions.changed", json!({})).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        gateway.requests("session.active_list").len(),
        2,
        "one more liveness round"
    );
    assert_eq!(
        gateway.requests("session.list").len(),
        2,
        "one trailing roster"
    );
    let _ = handle;
}

#[tokio::test]
async fn hot_and_cold_polls_follow_their_intervals() {
    let home = home();
    let cfg = FleetConfig::scaled_for_tests();
    let (gateway, handle) = start(home.path(), cfg.clone());
    gateway.set_fleet_profiles(vec![
        "fixture".into(),
        "fixture-1".into(),
        "fixture-2".into(),
    ]);
    gateway.push_result(json!({"sessions": [{
        "session_id": "hot-1", "id": "hot-1", "title": "", "source": "tui",
        "status": "waiting", "message_count": 1, "model": "minimax-m3.1-flash", "preview": ""
    }]}));
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    assert!(
        snapshot
            .queue
            .iter()
            .any(|item| item.id.key.session_id == "hot-1"),
        "a cold profile's approval is in the snapshot"
    );
    let hot = gateway
        .requests("session.active_list")
        .into_iter()
        .filter(|seen| seen.params["profile"] == "fixture")
        .collect::<Vec<_>>();
    assert!(hot.len() >= 2, "the hot profile is polled on its interval");
    if hot.len() >= 2 {
        let gap = hot[1].at.saturating_duration_since(hot[0].at);
        let low = cfg.hot_poll / 2;
        let high = cfg.hot_poll + cfg.hot_poll / 2;
        assert!(gap >= low && gap <= high, "hot interval drifted");
    }
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
    gateway.push_result(json!({"sessions": [{
        "id": "local-3f2a", "session_id": "local-3f2a", "title": "", "status": "waiting",
        "message_count": 4, "preview": "", "source": "tui"
    }]}));
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
}

#[tokio::test]
async fn never_reads_a_session_that_is_not_live() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    gateway.set_live_session_ids(["local-live"]);
    let _ = wait_connected(&handle, Duration::from_secs(2)).await;
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
    gateway.set_live_session_ids(["local-other"]);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        gateway.fleet_non_live_reads(),
        0,
        "a session that left the live list is not read"
    );
    let _ = handle;
}

#[tokio::test]
async fn outcome_probe_uses_epoch_and_tail() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    gateway.push_result(json!({"sessions": [{
        "id": "local-3f2a", "status": "idle", "message_count": 6, "title": "", "preview": ""
    }]}));
    gateway.script_error("session.events.since", Some("fixture"), 4001, "not_found");
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    let probes = gateway.requests("session.events.since");
    assert!(probes.len() >= 2, "the probe makes two calls");
    assert!(
        snapshot
            .rows
            .iter()
            .any(|row| { row.key.session_id == "local-3f2a" && row.state == RowState::Failed }),
        "an error completion is failed"
    );
}

#[tokio::test]
async fn outcome_probe_unknown_cases() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    for reason in ["not_found", "storage_unavailable"] {
        gateway.script_error("session.events.since", Some("fixture"), 4001, reason);
    }
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    assert!(
        snapshot
            .rows
            .iter()
            .any(|row| row.outcome == Some(talaria_core::fleet::Outcome::Unknown)),
        "a probe that cannot see the completion stays unknown"
    );
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
}

#[tokio::test]
async fn reconnect_marks_stale_then_refreshes() {
    let home = home();
    let (gateway, handle) = start(home.path(), FleetConfig::scaled_for_tests());
    let snapshot = wait_connected(&handle, Duration::from_secs(2)).await;
    assert!(
        matches!(snapshot.link, FleetLinkState::Connected),
        "the link connected before the drop"
    );
    gateway.push_result_after(Duration::from_secs(5), json!({"sessions": []}));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let refreshed = handle.snapshot();
    assert!(
        refreshed.rows.iter().all(|row| !row.stale),
        "the new generation's roster clears stale"
    );
    assert!(
        matches!(refreshed.link, FleetLinkState::Connected),
        "the link is connected again"
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
    gateway.push_result(json!({"sessions": [{
        "id": "local-3f2a", "status": "waiting", "title": "", "preview": "", "message_count": 1
    }]}));
    let _ = wait_connected(&handle, Duration::from_secs(2)).await;
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
