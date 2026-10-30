//! Fleet notices become alerts, counts, and the jump key.

#![allow(unreachable_code)]

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use talaria_app::app::{AppEffect, AppMessage, AppState, update};
use talaria_app::fleet::{FleetLinkState, FleetNotice, FleetSnapshot};
use talaria_app::fleet_attention::{
    AttentionHub, NextNeed, StatusCounts, next_need, status_counts, wake_task,
};
use talaria_app::side_connection::SideScope;
use talaria_core::alert_gate::urgency;
use talaria_core::attention::{Alert, AlertKind, NeedSummary};
use talaria_core::fleet::{
    FleetCounts, FleetRow, NeedDetail, NeedId, NeedItem, NeedSlot, Outcome, ProfileStatus, RowKey,
    RowState,
};
use talaria_core::fleet_attention::{FleetAttentionConfig, FleetScope};

fn cfg() -> FleetAttentionConfig {
    FleetAttentionConfig {
        bell: vec![AlertKind::NeedsYou, AlertKind::Failed],
        desktop: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
        done_after: Duration::from_secs(20),
        needs_you: FleetScope::All,
        finished: FleetScope::Watched,
    }
}

fn key(session: &str) -> RowKey {
    RowKey {
        profile: "fixture".to_owned(),
        session_id: session.to_owned(),
    }
}

fn row(
    key: RowKey,
    title: &str,
    state: RowState,
    outcome: Option<Outcome>,
    foreground: bool,
) -> FleetRow {
    FleetRow {
        terminal_location: None,
        key,
        title: title.to_owned(),
        model: None,
        source: None,
        started_at: None,
        message_count: 0,
        state,
        state_since: 0.0,
        outcome,
        foreground,
        subscribed: !foreground,
        stale: false,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: None,
        backend: talaria_core::fleet::FleetBackend::Hermes,
        agent: None,
        occupancy: None,
        cost: None,
    }
}

fn approval(command: &str) -> NeedDetail {
    NeedDetail::Known(NeedSummary::Approval {
        command: command.to_owned(),
        description: String::new(),
    })
}

fn item(key: &RowKey, slot: &str, seq: u64, foreground: bool, detail: NeedDetail) -> NeedItem {
    NeedItem {
        id: NeedId {
            key: key.clone(),
            slot: NeedSlot::Prompt(slot.to_owned()),
        },
        seq,
        since: 0.0,
        detail,
        choices: Vec::new(),
        execution_generation: None,
        timeout_s: None,
        exact: true,
        foreground,
        external_options: Vec::new(),
    }
}

fn counted(rows: &[FleetRow], queue: &[NeedItem]) -> FleetCounts {
    let background = |row: &FleetRow| !row.foreground;
    FleetCounts {
        needs_you: rows
            .iter()
            .filter(|row| background(row) && row.state == RowState::NeedsYou)
            .count(),
        needs_you_items: queue.iter().filter(|item| !item.foreground).count(),
        working: rows
            .iter()
            .filter(|row| background(row) && row.state == RowState::Working)
            .count(),
        failed: rows
            .iter()
            .filter(|row| background(row) && row.state == RowState::Failed)
            .count(),
        done: rows
            .iter()
            .filter(|row| background(row) && row.state == RowState::Done)
            .count(),
        idle: rows
            .iter()
            .filter(|row| background(row) && row.state == RowState::Idle)
            .count(),
    }
}

fn snapshot(
    link: FleetLinkState,
    profiles: Vec<(String, ProfileStatus)>,
    rows: Vec<FleetRow>,
    queue: Vec<NeedItem>,
) -> FleetSnapshot {
    let counts = counted(&rows, &queue);
    let as_of = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        - 10.0;
    FleetSnapshot {
        version: 1,
        scope: SideScope::ProfileOnly {
            profile: "fixture".to_owned(),
            why: String::new(),
        },
        link,
        profiles,
        rows,
        queue,
        counts,
        diagnostics: Vec::new(),
        as_of,
    }
}

fn listed() -> Vec<(String, ProfileStatus)> {
    vec![(
        "fixture".to_owned(),
        ProfileStatus::Listed { truncated: false },
    )]
}

fn drive(state: &mut AppState, message: AppMessage) -> Vec<AppEffect> {
    update(state, message)
}

fn alerts(effects: &[AppEffect]) -> Vec<&Alert> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::Alert(alert) => Some(alert),
            _ => None,
        })
        .collect()
}

fn notices(effects: &[AppEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::CommitLines(lines) => Some(lines.join("\n")),
            _ => None,
        })
        .collect()
}

fn wakes(effects: &[AppEffect]) -> Vec<Instant> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::WakeAt(at) => Some(*at),
            _ => None,
        })
        .collect()
}

fn near(at: Instant, target: Instant) -> bool {
    at.saturating_duration_since(target) < Duration::from_secs(2)
        && target.saturating_duration_since(at) < Duration::from_secs(2)
}

fn api_snapshot(state_of_row: RowState, outcome: Option<Outcome>) -> FleetSnapshot {
    let id = key("sess-api");
    let waiting = item(&id, "p1", 1, false, approval("rm -rf build/"));
    snapshot(
        FleetLinkState::Connected,
        listed(),
        vec![row(id, "api-refactor", state_of_row, outcome, false)],
        vec![waiting],
    )
}

#[test]
fn status_counts_add_background_to_foreground() {
    let id = key("sess-two");
    let rows = vec![row(
        id.clone(),
        "api-refactor",
        RowState::NeedsYou,
        None,
        false,
    )];
    let queue = vec![
        item(&id, "p1", 1, false, approval("rm -rf build/")),
        item(&id, "p2", 2, false, NeedDetail::Waiting),
    ];
    let snap = snapshot(FleetLinkState::Connected, listed(), rows, queue);
    let open = 3;
    assert_eq!(
        status_counts(open, Some(&snap)),
        StatusCounts {
            needs_you: open + 2,
            working: 0,
            failed: 0,
            done: 0,
        }
    );
    assert_eq!(
        status_counts(open, None),
        StatusCounts {
            needs_you: open,
            working: 0,
            failed: 0,
            done: 0,
        }
    );

    let reconnecting = FleetSnapshot {
        link: FleetLinkState::Reconnecting {
            reason: "dropped".to_owned(),
        },
        counts: FleetCounts {
            needs_you_items: 5,
            working: 4,
            failed: 2,
            done: 3,
            ..FleetCounts::default()
        },
        ..snapshot(FleetLinkState::Connected, listed(), Vec::new(), Vec::new())
    };
    assert_eq!(
        status_counts(1, Some(&reconnecting)),
        StatusCounts {
            needs_you: 6,
            working: 4,
            failed: 2,
            done: 3,
        }
    );

    let failed = FleetSnapshot {
        link: FleetLinkState::Failed {
            reason: "stopped".to_owned(),
        },
        counts: FleetCounts {
            needs_you_items: 1,
            working: 0,
            failed: 7,
            done: 8,
            ..FleetCounts::default()
        },
        ..snapshot(FleetLinkState::Connected, listed(), Vec::new(), Vec::new())
    };
    assert_eq!(status_counts(0, Some(&failed)).failed, 7);
    assert_eq!(status_counts(0, Some(&failed)).done, 8);

    let above = usize::try_from(u64::from(u32::MAX) + 1).unwrap_or(usize::MAX);
    let saturated = FleetSnapshot {
        counts: FleetCounts {
            needs_you_items: above,
            ..FleetCounts::default()
        },
        ..snapshot(FleetLinkState::Connected, listed(), Vec::new(), Vec::new())
    };
    assert_eq!(status_counts(1, Some(&saturated)).needs_you, u32::MAX);
}

#[test]
fn next_need_targets_oldest_background_item() {
    let older = key("sess-old");
    let newer = key("sess-new");
    let front = key("sess-front");
    let queue = vec![
        item(&front, "fg", 1, true, approval("pwd")),
        item(&newer, "late", 9, false, approval("ls")),
        item(&older, "early", 4, false, approval("true")),
    ];
    let snap = snapshot(FleetLinkState::Connected, listed(), Vec::new(), queue);
    assert_eq!(next_need(Some(&snap), true), NextNeed::Peek(older));
}

#[test]
fn next_need_notices() {
    let off = snapshot(FleetLinkState::Connected, listed(), Vec::new(), Vec::new());
    assert_eq!(
        next_need(Some(&off), false),
        NextNeed::Notice(
            "The fleet view is off: [fleet] enabled = false in Talaria's configuration.".into()
        )
    );
    assert_eq!(
        next_need(None, true),
        NextNeed::Notice("The fleet is still loading; try again in a moment.".into())
    );

    let id = key("sess-wait");
    let queue = vec![item(&id, "p4", 4, false, approval("ls"))];
    let reconnecting = snapshot(
        FleetLinkState::Reconnecting {
            reason: "dropped".into(),
        },
        listed(),
        Vec::new(),
        queue,
    );
    assert_eq!(next_need(Some(&reconnecting), true), NextNeed::Peek(id));

    let failed = snapshot(
        FleetLinkState::Failed { reason: "x".into() },
        listed(),
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        next_need(Some(&failed), true),
        NextNeed::Notice("The fleet is not available: x.".into())
    );

    let connecting = snapshot(
        FleetLinkState::Connecting,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        next_need(Some(&connecting), true),
        NextNeed::Notice("The fleet is still loading; try again in a moment.".into())
    );

    let loading = snapshot(
        FleetLinkState::Connected,
        vec![
            ("fixture".into(), ProfileStatus::Listed { truncated: false }),
            ("default".into(), ProfileStatus::Loading),
        ],
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        next_need(Some(&loading), true),
        NextNeed::Notice("The fleet is still loading; try again in a moment.".into())
    );

    let front = key("sess-front");
    let only_front = snapshot(
        FleetLinkState::Connected,
        listed(),
        vec![row(front.clone(), "front", RowState::NeedsYou, None, true)],
        vec![item(&front, "fg", 1, true, approval("pwd"))],
    );
    assert_eq!(
        next_need(Some(&only_front), true),
        NextNeed::Notice("Nothing in the background needs you.".into())
    );
}

#[test]
fn fleet_notice_becomes_one_alert_effect() {
    let mut state = AppState::new("launch");
    let snap = api_snapshot(RowState::NeedsYou, None);
    let id = key("sess-api");
    let _ = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Changed(Arc::new(snap))),
    );
    let effects = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Attention {
            kind: AlertKind::NeedsYou,
            key: id,
            need: Some(NeedId {
                key: key("sess-api"),
                slot: NeedSlot::Prompt("p1".into()),
            }),
            foreground: false,
        }),
    );
    let found = alerts(&effects);
    assert_eq!(found.len(), 1, "missing alert effect");
    assert_eq!(
        found[0],
        &Alert {
            kind: AlertKind::NeedsYou,
            bell: true,
            desktop: true,
            text: "api-refactor: needs you: approval: rm -rf build/".into(),
        }
    );
    assert_eq!(urgency(found[0].kind), 2);
}

#[test]
fn attention_before_changed_waits_for_the_snapshot() {
    let id = key("sess-api");
    let attention = AppMessage::Fleet(FleetNotice::Attention {
        kind: AlertKind::NeedsYou,
        key: id.clone(),
        need: Some(NeedId {
            key: id.clone(),
            slot: NeedSlot::Prompt("p1".into()),
        }),
        foreground: false,
    });
    let mut state = AppState::new("launch");
    let held = drive(&mut state, attention);
    assert!(alerts(&held).is_empty(), "the alert waits for the snapshot");
    let decided = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Changed(Arc::new(api_snapshot(
            RowState::NeedsYou,
            None,
        )))),
    );
    let found = alerts(&decided);
    assert_eq!(found.len(), 1, "missing alert effect");
    assert_eq!(
        found[0].text,
        "api-refactor: needs you: approval: rm -rf build/"
    );

    let mut state = AppState::new("launch");
    let _ = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Attention {
            kind: AlertKind::Done,
            key: id.clone(),
            need: None,
            foreground: false,
        }),
    );
    let interrupted = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Changed(Arc::new(api_snapshot(
            RowState::Done,
            Some(Outcome::Interrupted),
        )))),
    );
    assert!(
        alerts(&interrupted).is_empty(),
        "an interrupted turn stays quiet"
    );

    let mut state = AppState::new("launch");
    let fallback_key = key("abcd9f3c2a");
    let _ = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Attention {
            kind: AlertKind::NeedsYou,
            key: fallback_key,
            need: Some(NeedId {
                key: key("abcd9f3c2a"),
                slot: NeedSlot::Prompt("p1".into()),
            }),
            foreground: false,
        }),
    );
    let woken = drive(&mut state, AppMessage::AttentionWake);
    let woken_alerts = alerts(&woken);
    assert_eq!(woken_alerts.len(), 1, "missing fallback alert");
    assert!(
        woken_alerts[0].text.contains("#9f3c2a"),
        "{}",
        woken_alerts[0].text
    );
    assert!(
        woken_alerts[0].text.contains("needs you"),
        "{}",
        woken_alerts[0].text
    );

    let mut state = AppState::new("launch");
    let _ = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Changed(Arc::new(api_snapshot(
            RowState::NeedsYou,
            None,
        )))),
    );
    let immediate = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Attention {
            kind: AlertKind::NeedsYou,
            key: id,
            need: Some(NeedId {
                key: key("sess-api"),
                slot: NeedSlot::Prompt("p1".into()),
            }),
            foreground: false,
        }),
    );
    assert_eq!(
        alerts(&immediate).len(),
        1,
        "a known row is decided at once"
    );

    let mut state = AppState::new("launch");
    let foreground = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Attention {
            kind: AlertKind::NeedsYou,
            key: key("sess-front"),
            need: Some(NeedId {
                key: key("sess-front"),
                slot: NeedSlot::Prompt("fg".into()),
            }),
            foreground: true,
        }),
    );
    assert!(alerts(&foreground).is_empty());
    assert!(
        wakes(&foreground).is_empty(),
        "a foreground notice is not held"
    );
}

#[test]
fn foreground_and_fleet_alerts_share_one_gate() {
    let base = Instant::now();
    let mut hub = AttentionHub::new(cfg(), "fixture".into(), Some("Alt+A".into()));
    let foreground = Alert {
        kind: AlertKind::NeedsYou,
        bell: true,
        desktop: true,
        text: "front: needs you: approval: pwd".into(),
    };
    let admitted = hub.on_foreground_alert(foreground, base);
    let id = key("sess-api");
    let snap = api_snapshot(RowState::NeedsYou, None);
    let fleet = hub.on_notice(
        &FleetNotice::Attention {
            kind: AlertKind::NeedsYou,
            key: id,
            need: Some(NeedId {
                key: key("sess-api"),
                slot: NeedSlot::Prompt("p1".into()),
            }),
            foreground: false,
        },
        &snap,
        base + Duration::from_secs(1),
    );
    let kept = u8::from(admitted.is_some()) + u8::try_from(fleet.alerts.len()).unwrap_or(0);
    assert_eq!(kept, 1, "one effect across the shared gate");
    assert!(fleet.alerts.is_empty(), "the later fleet alert is dropped");

    let mut hub = AttentionHub::new(cfg(), "fixture".into(), None);
    assert!(
        hub.on_foreground_alert(
            Alert {
                kind: AlertKind::NeedsYou,
                bell: true,
                desktop: true,
                text: "needs".into(),
            },
            base,
        )
        .is_some()
    );
    assert!(
        hub.on_foreground_alert(
            Alert {
                kind: AlertKind::Failed,
                bell: true,
                desktop: true,
                text: "failed".into(),
            },
            base + Duration::from_secs(3),
        )
        .is_some()
    );
    assert!(
        hub.on_foreground_alert(
            Alert {
                kind: AlertKind::Done,
                bell: false,
                desktop: true,
                text: "done".into(),
            },
            base + Duration::from_secs(6),
        )
        .is_some()
    );
}

#[tokio::test]
async fn quiet_window_wake_gives_one_notice() {
    let mut state = AppState::new("launch");
    let started = Instant::now();
    let loading = snapshot(
        FleetLinkState::Connected,
        vec![("fixture".into(), ProfileStatus::Loading)],
        Vec::new(),
        Vec::new(),
    );
    let connected = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Changed(Arc::new(loading))),
    );
    let opening = wakes(&connected);
    assert_eq!(opening.len(), 1, "missing WakeAt");
    assert!(
        near(opening[0], started + Duration::from_secs(15)),
        "the first window ends about 15 seconds later"
    );

    let id = key("sess-wait");
    let listed_waiting = snapshot(
        FleetLinkState::Connected,
        listed(),
        vec![row(
            id.clone(),
            "api-refactor",
            RowState::NeedsYou,
            None,
            false,
        )],
        vec![item(&id, "p1", 1, false, NeedDetail::Waiting)],
    );
    let settled_at = Instant::now();
    let settled = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Changed(Arc::new(listed_waiting.clone()))),
    );
    let shortened = wakes(&settled);
    assert_eq!(shortened.len(), 1, "missing shortened WakeAt");
    assert!(near(shortened[0], settled_at + Duration::from_secs(4)));

    let during = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Attention {
            kind: AlertKind::NeedsYou,
            key: id,
            need: Some(NeedId {
                key: key("sess-wait"),
                slot: NeedSlot::Prompt("p1".into()),
            }),
            foreground: false,
        }),
    );
    assert!(
        alerts(&during).is_empty(),
        "the quiet window emits no alert"
    );

    let notice = drive(&mut state, AppMessage::AttentionWake);
    let lines = notices(&notice);
    assert_eq!(lines.len(), 1, "missing startup notice");
    assert!(
        lines[0].contains("1 background session needs you"),
        "{}",
        lines[0]
    );
    assert!(
        alerts(&notice).is_empty(),
        "the startup notice does not ring"
    );

    let stale = drive(&mut state, AppMessage::AttentionWake);
    assert!(
        alerts(&stale).is_empty() && notices(&stale).is_empty() && wakes(&stale).is_empty(),
        "a stale wake adds nothing"
    );
    assert!(
        stale
            .iter()
            .all(|effect| !matches!(effect, AppEffect::Redraw)),
        "a stale wake draws nothing"
    );

    let (sender, receiver) = tokio::sync::oneshot::channel();
    let called = Instant::now();
    let task = wake_task(called + Duration::from_millis(50), move || {
        let _ = sender.send(());
    });
    tokio::time::timeout(Duration::from_secs(1), receiver)
        .await
        .expect("the wake arrives within 1 second")
        .expect("the wake sends once");
    assert!(called.elapsed() >= Duration::from_millis(50));
    task.abort();
}

fn working_snapshot() -> FleetSnapshot {
    snapshot(
        FleetLinkState::Connected,
        listed(),
        vec![row(
            key("sess-api"),
            "api-refactor",
            RowState::Working,
            None,
            false,
        )],
        Vec::new(),
    )
}

#[test]
fn no_new_frame_when_counts_unchanged() {
    let mut state = AppState::new("launch");
    let first = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Changed(Arc::new(working_snapshot()))),
    );
    assert!(
        first
            .iter()
            .any(|effect| matches!(effect, AppEffect::Redraw)),
        "the first snapshot draws"
    );
    let second = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Changed(Arc::new(working_snapshot()))),
    );
    assert!(
        second
            .iter()
            .all(|effect| !matches!(effect, AppEffect::Redraw)),
        "redraw after the first"
    );

    let base = working_snapshot();
    let idle = FleetSnapshot {
        counts: FleetCounts {
            idle: 1,
            ..base.counts
        },
        rows: vec![
            row(
                key("sess-api"),
                "api-refactor",
                RowState::Working,
                None,
                false,
            ),
            row(key("sess-idle"), "idle", RowState::Idle, None, false),
        ],
        ..base
    };
    let third = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Changed(Arc::new(idle))),
    );
    assert!(
        third
            .iter()
            .all(|effect| !matches!(effect, AppEffect::Redraw)),
        "an idle-only change draws nothing"
    );
}

#[test]
fn attention_key_opens_fleet_view_or_notice() {
    let mut state = AppState::new("launch");
    let id = key("sess-bg");
    let waiting = snapshot(
        FleetLinkState::Connected,
        listed(),
        vec![row(
            id.clone(),
            "api-refactor",
            RowState::NeedsYou,
            None,
            false,
        )],
        vec![item(&id, "p1", 3, false, approval("ls"))],
    );
    let _ = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Changed(Arc::new(waiting))),
    );
    let effects = drive(&mut state, AppMessage::AttentionNext);
    let opened: Vec<&RowKey> = effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::OpenFleetViewAt(key) => Some(key),
            _ => None,
        })
        .collect();
    assert_eq!(opened.len(), 1, "missing OpenFleetViewAt");
    assert_eq!(opened[0], &id);
    assert!(
        notices(&effects)
            .iter()
            .all(|line| !line.contains("Nothing in the background")),
        "a peek is not a notice"
    );

    let mut state = AppState::new("launch");
    let quiet = snapshot(FleetLinkState::Connected, listed(), Vec::new(), Vec::new());
    let _ = drive(
        &mut state,
        AppMessage::Fleet(FleetNotice::Changed(Arc::new(quiet))),
    );
    let effects = drive(&mut state, AppMessage::AttentionNext);
    let lines = notices(&effects);
    assert_eq!(lines.len(), 1, "missing notice");
    assert!(
        lines[0].contains("Nothing in the background needs you."),
        "{}",
        lines[0]
    );
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, AppEffect::OpenFleetViewAt(_)))
    );
}
