//! Attention driven by recorded frames and by the application loop.
//!
//! Behavior contract: issue 228.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use hermes_client::ReplaySource;
use hermes_contract::events::EventFrame;
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::Value;
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::comforts::activities_for;
use talaria_app::prompts::{MaskedKind, MaskedRequestInfo};
use talaria_core::attention::{
    Activity, AlertKind, AttentionConfig, AttentionTracker, NeedKey, NeedSummary, PaneState,
};
use talaria_core::session::{Event, SessionId, SessionState, update as reduce};
use zeroize::Zeroizing;

fn fixture_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames")
        .join(file)
}

fn all_channels() -> AttentionConfig {
    AttentionConfig {
        bell: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
        desktop: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
        done_after: Duration::from_secs(20),
    }
}

/// Reducer events from the inbound frames of a replay source.
fn replay_events(source: &ReplaySource) -> Vec<Event> {
    let mut events = Vec::new();
    for frame in source.inbound() {
        let Some(object) = frame.frame.as_object() else {
            continue;
        };
        if object.get("method").and_then(Value::as_str) != Some("event") {
            continue;
        }
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        let Ok(decoded) = EventFrame::from_params(&params) else {
            continue;
        };
        let session_id = decoded.session_id.clone().unwrap_or_default();
        events.push(Event::Session {
            session_id: SessionId::new(session_id),
            frame: decoded,
            raw: params,
        });
    }
    events
}

#[test]
fn approval_fixture_drives_states_and_one_alert() {
    let path = fixture_path("approval.jsonl");
    let source = ReplaySource::open(&path).expect("the recorded approval turn");
    let events = replay_events(&source);
    assert!(!events.is_empty(), "the fixture holds reducer events");

    let session_id = events
        .iter()
        .find_map(|event| match event {
            Event::Session { session_id, .. } if !session_id.as_str().is_empty() => {
                Some(session_id.as_str().to_owned())
            }
            _ => None,
        })
        .unwrap_or_else(|| "local-session-1".to_owned());

    let mut tracker = AttentionTracker::new(all_channels());
    let base = Instant::now();
    let mut panes: Vec<PaneState> = Vec::new();
    let mut alerts = 0;
    // The recording answers the approval once and the turn completes.
    replay_into(
        events,
        &session_id,
        base,
        &mut tracker,
        &mut panes,
        &mut alerts,
    );
    assert_eq!(
        panes,
        [
            PaneState::Working,
            PaneState::Blocked,
            PaneState::Working,
            PaneState::Idle
        ]
    );
    assert_eq!(alerts, 1, "exactly one needs-you alert, no done");

    replay_into(
        replay_events(&source),
        &session_id,
        base,
        &mut tracker,
        &mut panes,
        &mut alerts,
    );
    assert_eq!(alerts, 1, "a second replay adds no alert");
}

/// Reduce every event, mapping each step to activities for the tracker.
fn replay_into(
    events: Vec<Event>,
    session_id: &str,
    base: Instant,
    tracker: &mut AttentionTracker,
    panes: &mut Vec<PaneState>,
    alerts: &mut usize,
) {
    let mut state = SessionState::empty(SessionId::new(session_id));
    for (index, event) in events.into_iter().enumerate() {
        let before = state.clone();
        reduce(&mut state, event);
        let at = base + Duration::from_millis(10 * index as u64);
        for activity in activities_for(&before, &state, &[], &[], false, at) {
            let out = tracker.observe(activity);
            if let Some(pane) = out.pane {
                panes.push(pane.state);
            }
            *alerts += usize::from(out.alert.is_some());
        }
    }
}

#[test]
fn snapshot_prompt_blocks_without_alert() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../talaria-core/tests/data/snapshot_pending_vs_prompts.jsonl");
    let text = std::fs::read_to_string(&path).expect("the recorded snapshot file");
    let result = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|record| record.get("frame")?.get("result").cloned())
        .expect("a session result in the snapshot file");
    let snapshot: CanonicalSnapshot = serde_json::from_value(result).expect("the snapshot decodes");
    let before = SessionState::empty(SessionId::new("local-session-1"));
    let after = SessionState::from_snapshot(snapshot);

    let mut tracker = AttentionTracker::new(all_channels());
    let mut panes: Vec<PaneState> = Vec::new();
    let mut alerts = 0;
    for activity in activities_for(&before, &after, &[], &[], true, Instant::now()) {
        let out = tracker.observe(activity);
        if let Some(pane) = out.pane {
            panes.push(pane.state);
        }
        alerts += usize::from(out.alert.is_some());
    }
    assert_eq!(panes, [PaneState::Blocked]);
    assert_eq!(alerts, 0, "a launch snapshot never alerts");
}

#[test]
fn masked_request_opens_and_closes_a_need() {
    let state = SessionState::empty(SessionId::new("local-session-1"));
    let info = MaskedRequestInfo {
        id: "masked-1".to_owned(),
        session_id: Some("local-session-1".to_owned()),
        kind: MaskedKind::Sudo,
        fields: vec![("command", "ls".to_owned())],
    };
    let now = Instant::now();

    let opened = activities_for(&state, &state, &[], std::slice::from_ref(&info), false, now);
    assert_eq!(opened.len(), 1);
    match &opened[0] {
        Activity::NeedOpened {
            key,
            summary,
            restored,
            ..
        } => {
            assert_eq!(key, &NeedKey::Masked("masked-1".to_owned()));
            assert_eq!(summary, &NeedSummary::Sudo);
            assert!(!restored);
        }
        other => panic!("expected a need opening, saw {other:?}"),
    }

    let closed = activities_for(&state, &state, &[info], &[], false, now);
    assert!(
        matches!(closed.as_slice(), [Activity::NeedClosed { key }] if key == &NeedKey::Masked("masked-1".to_owned())),
        "a locally answered id closes its need: {closed:?}"
    );
}

fn has_pane(effects: &[AppEffect], state: PaneState) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, AppEffect::ReportPane(pane) if pane.state == state))
}

#[test]
fn local_masked_answer_closes_need_through_update() {
    // A sudo server request shaped by the contract's server request entry.
    let params = mock_gateway::server_request_params("sudo", mock_gateway::SESSION_ID);
    let command = params
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let info = MaskedRequestInfo {
        id: "masked-1".to_owned(),
        session_id: Some(mock_gateway::SESSION_ID.to_owned()),
        kind: MaskedKind::Sudo,
        fields: vec![("command", command)],
    };

    let mut state = AppState::new("0123456789abcdef");
    let effects = update(
        &mut state,
        AppMessage::PromptDone {
            events: Vec::new(),
            notices: Vec::new(),
            masked: Some(vec![info]),
        },
    );
    assert!(has_pane(&effects, PaneState::Blocked));

    let effects = update(
        &mut state,
        AppMessage::User(UserAction::Masked {
            request_id: "masked-1".to_owned(),
            value: Zeroizing::new(String::new()),
        }),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::AnswerMasked { .. })),
        "the local refusal is answered"
    );

    // No frame arrives between the refusal and the service reporting the id gone.
    let effects = update(
        &mut state,
        AppMessage::PromptDone {
            events: Vec::new(),
            notices: Vec::new(),
            masked: Some(Vec::new()),
        },
    );
    assert!(has_pane(&effects, PaneState::Idle));
}
