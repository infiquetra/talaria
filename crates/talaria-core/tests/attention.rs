//! The foreground attention tracker: pane state, alert rules, and limits.
//!
//! Behavior contract: issue 228.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use proptest::prelude::*;
use talaria_core::attention::{
    Activity, AlertKind, AttentionConfig, AttentionTracker, NeedKey, NeedSummary, PaneState,
    TurnOutcome,
};
use talaria_core::session::{PromptKey, SessionId};

fn all_channels() -> AttentionConfig {
    AttentionConfig {
        bell: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
        desktop: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
        done_after: Duration::from_secs(20),
    }
}

fn prompt_key(session: &str, id: &str) -> NeedKey {
    NeedKey::Prompt(PromptKey {
        session_id: SessionId::new(session),
        prompt_id: id.to_owned(),
        execution_generation: 1,
    })
}

fn approval(command: &str, description: &str) -> NeedSummary {
    NeedSummary::Approval {
        command: command.to_owned(),
        description: description.to_owned(),
    }
}

fn opened(key: NeedKey, summary: NeedSummary, at: Instant) -> Activity {
    Activity::NeedOpened {
        key,
        summary,
        restored: false,
        at,
    }
}

#[test]
fn idle_working_blocked_follow_activity() {
    let mut tracker = AttentionTracker::new(all_channels());
    let base = Instant::now();
    let key = prompt_key("session-a", "prompt-1");

    let out = tracker.observe(Activity::TurnStarted { at: base });
    assert_eq!(out.alert, None);
    assert_eq!(
        out.pane.as_ref().map(|pane| &pane.state),
        Some(&PaneState::Working)
    );

    let out = tracker.observe(Activity::TurnStarted {
        at: base + Duration::from_secs(1),
    });
    assert_eq!(out.pane, None, "no change while already working");

    let out = tracker.observe(opened(key.clone(), approval("ls", ""), base));
    assert!(out.alert.is_some());
    assert_eq!(
        out.pane.as_ref().map(|pane| &pane.state),
        Some(&PaneState::Blocked)
    );

    let out = tracker.observe(Activity::NeedClosed { key });
    assert_eq!(out.alert, None);
    assert_eq!(
        out.pane.as_ref().map(|pane| &pane.state),
        Some(&PaneState::Working)
    );

    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: base + Duration::from_secs(2),
    });
    assert_eq!(out.alert, None, "a short turn stays silent");
    assert_eq!(
        out.pane.as_ref().map(|pane| &pane.state),
        Some(&PaneState::Idle)
    );
}

#[test]
fn needs_you_alerts_once_per_prompt_key() {
    let mut tracker = AttentionTracker::new(all_channels());
    let base = Instant::now();
    let key = prompt_key("session-a", "prompt-1");

    let out = tracker.observe(opened(key.clone(), approval("ls", ""), base));
    assert_eq!(
        out.alert.as_ref().map(|alert| &alert.kind),
        Some(&AlertKind::NeedsYou)
    );

    let out = tracker.observe(Activity::NeedClosed { key: key.clone() });
    assert_eq!(out.alert, None);

    let out = tracker.observe(opened(
        key,
        approval("ls", ""),
        base + Duration::from_secs(3),
    ));
    assert_eq!(out.alert, None, "the same key never alerts twice");

    let out = tracker.observe(opened(
        prompt_key("session-a", "prompt-2"),
        approval("ls", ""),
        base + Duration::from_secs(3),
    ));
    assert_eq!(
        out.alert.as_ref().map(|alert| &alert.kind),
        Some(&AlertKind::NeedsYou),
        "a new key alerts after the limit passed"
    );
}

#[test]
fn restored_needs_block_but_never_alert() {
    let mut tracker = AttentionTracker::new(all_channels());
    let base = Instant::now();

    let out = tracker.observe(Activity::NeedOpened {
        key: prompt_key("session-a", "prompt-1"),
        summary: approval("ls", ""),
        restored: true,
        at: base,
    });
    assert_eq!(out.alert, None);
    assert_eq!(
        out.pane.as_ref().map(|pane| &pane.state),
        Some(&PaneState::Blocked)
    );
}

#[test]
fn done_needs_threshold_failed_always() {
    let mut tracker = AttentionTracker::new(all_channels());
    let base = Instant::now();

    let out = tracker.observe(Activity::TurnStarted { at: base });
    assert_eq!(out.alert, None);
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: base + Duration::from_secs(19),
    });
    assert_eq!(out.alert, None, "19 seconds stays silent");

    let out = tracker.observe(Activity::TurnStarted {
        at: base + Duration::from_secs(30),
    });
    assert_eq!(out.alert, None);
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: base + Duration::from_secs(50),
    });
    assert_eq!(
        out.alert.as_ref().map(|alert| &alert.kind),
        Some(&AlertKind::Done),
        "20 seconds alerts"
    );

    let out = tracker.observe(Activity::TurnStarted {
        at: base + Duration::from_secs(60),
    });
    assert_eq!(out.alert, None);
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Failed,
        at: base + Duration::from_secs(61),
    });
    assert_eq!(
        out.alert.as_ref().map(|alert| &alert.kind),
        Some(&AlertKind::Failed),
        "a failed turn alerts whatever its length"
    );
}

#[test]
fn interrupted_unseen_start_and_stray_end() {
    let mut tracker = AttentionTracker::new(all_channels());
    let base = Instant::now();

    let out = tracker.observe(Activity::TurnStarted { at: base });
    assert_eq!(out.alert, None);
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Interrupted,
        at: base + Duration::from_secs(25),
    });
    assert_eq!(out.alert, None, "an interrupted turn never alerts");

    let out = tracker.observe(Activity::TurnStarted {
        at: base + Duration::from_secs(30),
    });
    assert_eq!(out.alert, None);
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: base + Duration::from_secs(55),
    });
    assert_eq!(
        out.alert.as_ref().map(|alert| &alert.kind),
        Some(&AlertKind::Done),
        "duration runs from the first seen start"
    );

    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: base + Duration::from_secs(60),
    });
    assert_eq!(out.alert, None);
    assert_eq!(out.pane, None, "an end with no turn changes nothing");
}

/// Open one need on a fresh tracker and return its alert text.
fn alert_text(summary: NeedSummary) -> String {
    let mut tracker = AttentionTracker::new(all_channels());
    let out = tracker.observe(opened(
        prompt_key("session-a", "prompt-1"),
        summary,
        Instant::now(),
    ));
    out.alert
        .map(|alert| alert.text)
        .expect("a fresh need alerts")
}

#[test]
fn channels_follow_configuration_and_texts() {
    let mut tracker = AttentionTracker::new(AttentionConfig {
        bell: vec![AlertKind::NeedsYou],
        desktop: vec![AlertKind::Done],
        done_after: Duration::from_secs(20),
    });
    let base = Instant::now();
    let out = tracker.observe(opened(
        prompt_key("session-a", "prompt-1"),
        approval("ls", ""),
        base,
    ));
    let alert = out.alert.expect("needs you is configured");
    assert_eq!(alert.kind, AlertKind::NeedsYou);
    assert!(alert.bell);
    assert!(!alert.desktop);

    let mut tracker = AttentionTracker::new(AttentionConfig {
        bell: vec![],
        desktop: vec![AlertKind::Done],
        done_after: Duration::from_secs(20),
    });
    tracker.observe(Activity::TurnStarted { at: base });
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: base + Duration::from_secs(25),
    });
    let alert = out.alert.expect("done is configured");
    assert!(!alert.bell);
    assert!(alert.desktop);

    let mut tracker = AttentionTracker::new(AttentionConfig {
        bell: vec![],
        desktop: vec![],
        done_after: Duration::from_secs(20),
    });
    tracker.observe(Activity::TurnStarted { at: base });
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Failed,
        at: base + Duration::from_secs(1),
    });
    assert_eq!(out.alert, None, "a kind in neither list alerts nowhere");
    assert_eq!(
        out.pane.as_ref().map(|pane| &pane.state),
        Some(&PaneState::Idle),
        "the pane still updates"
    );

    assert_eq!(
        alert_text(approval("ls\nrm x", "")),
        "needs you: approval: ls"
    );
    assert_eq!(
        alert_text(approval("", "Delete caches")),
        "needs you: approval: Delete caches"
    );
    assert_eq!(alert_text(approval("  ", "  ")), "needs you: approval");
    assert_eq!(
        alert_text(NeedSummary::Question("Why?\nmore".to_owned())),
        "needs you: question: Why?"
    );
    assert_eq!(
        alert_text(NeedSummary::Sudo),
        "needs you: password for sudo"
    );
    assert_eq!(alert_text(NeedSummary::Secret), "needs you: a secret");
    assert_eq!(
        alert_text(NeedSummary::VaultCode),
        "needs you: a vault code"
    );
    assert_eq!(
        alert_text(NeedSummary::VaultUnlock),
        "needs you: a password manager unlock"
    );

    let long = "q".repeat(100);
    let cut: String = "q".repeat(59) + "…";
    assert_eq!(
        alert_text(NeedSummary::Question(long)),
        format!("needs you: question: {cut}")
    );

    let mut tracker = AttentionTracker::new(all_channels());
    tracker.observe(Activity::SessionChanged {
        session_id: "session-a".to_owned(),
        label: "proj".to_owned(),
    });
    tracker.observe(Activity::TurnStarted { at: base });
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: base + Duration::from_secs(25),
    });
    assert_eq!(
        out.alert.map(|alert| alert.text),
        Some("done: proj".to_owned())
    );
}

#[test]
fn rate_limit_keeps_the_more_urgent_alert() {
    let mut tracker = AttentionTracker::new(all_channels());
    let base = Instant::now();
    let mut alerts = 0;
    for (index, millis) in [0, 500, 1900].into_iter().enumerate() {
        let out = tracker.observe(opened(
            prompt_key("session-a", &format!("prompt-{index}")),
            approval("ls", ""),
            base + Duration::from_millis(millis),
        ));
        alerts += usize::from(out.alert.is_some());
    }
    assert_eq!(alerts, 1, "three needs inside the window ring once");

    let out = tracker.observe(opened(
        prompt_key("session-a", "prompt-3"),
        approval("ls", ""),
        base + Duration::from_millis(2100),
    ));
    assert!(out.alert.is_some(), "a need past the window rings again");

    let reopened = prompt_key("session-a", "prompt-1");
    tracker.observe(Activity::NeedClosed {
        key: reopened.clone(),
    });
    let out = tracker.observe(opened(
        reopened,
        approval("ls", ""),
        base + Duration::from_secs(5),
    ));
    assert_eq!(out.alert, None, "a dropped alert still marks its key");

    let mut tracker = AttentionTracker::new(all_channels());
    tracker.observe(Activity::TurnStarted {
        at: base - Duration::from_secs(25),
    });
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: base,
    });
    assert!(out.alert.is_some());
    let out = tracker.observe(opened(
        prompt_key("session-a", "prompt-1"),
        approval("ls", ""),
        base + Duration::from_secs(1),
    ));
    assert!(out.alert.is_some(), "a need beats an earlier done");

    let mut tracker = AttentionTracker::new(all_channels());
    let out = tracker.observe(opened(
        prompt_key("session-a", "prompt-1"),
        approval("ls", ""),
        base,
    ));
    assert!(out.alert.is_some());
    tracker.observe(Activity::TurnStarted {
        at: base - Duration::from_secs(25),
    });
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: base + Duration::from_secs(1),
    });
    assert_eq!(out.alert, None, "a done never beats an earlier need");
}

#[test]
fn session_change_clears_needs_and_turn() {
    let mut tracker = AttentionTracker::new(all_channels());
    let base = Instant::now();
    let key = prompt_key("session-a", "prompt-1");
    tracker.observe(Activity::TurnStarted { at: base });
    tracker.observe(opened(key.clone(), approval("ls", ""), base));

    let out = tracker.observe(Activity::SessionChanged {
        session_id: "session-b".to_owned(),
        label: "other".to_owned(),
    });
    assert_eq!(out.alert, None);
    assert_eq!(
        out.pane.as_ref().map(|pane| &pane.state),
        Some(&PaneState::Idle)
    );

    let out = tracker.observe(opened(key, approval("ls", ""), base));
    assert_eq!(out.alert, None, "the key memory survives a session change");
    assert_eq!(
        out.pane.as_ref().map(|pane| &pane.state),
        Some(&PaneState::Blocked)
    );
}

#[test]
fn turn_end_with_open_need_stays_blocked() {
    let mut tracker = AttentionTracker::new(all_channels());
    let base = Instant::now();
    let key = prompt_key("session-a", "prompt-1");
    tracker.observe(Activity::TurnStarted { at: base });
    let out = tracker.observe(opened(key.clone(), approval("ls", ""), base));
    assert_eq!(
        out.pane.as_ref().map(|pane| &pane.state),
        Some(&PaneState::Blocked)
    );

    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: base + Duration::from_secs(1),
    });
    assert_eq!(out.alert, None);
    assert_eq!(out.pane, None, "still blocked while the need is open");

    let out = tracker.observe(Activity::NeedClosed { key });
    assert_eq!(
        out.pane.as_ref().map(|pane| &pane.state),
        Some(&PaneState::Idle)
    );
}

/// One scripted step for the pane state property.
#[derive(Debug, Clone)]
enum Step {
    Start,
    EndComplete,
    OpenPrompt(u8),
    OpenMasked(u8),
    ClosePrompt(u8),
    CloseMasked(u8),
    ChangeSession(u8),
}

fn step_strategy() -> impl Strategy<Value = Step> {
    prop_oneof![
        Just(Step::Start),
        Just(Step::EndComplete),
        (0..4u8).prop_map(Step::OpenPrompt),
        (0..4u8).prop_map(Step::OpenMasked),
        (0..4u8).prop_map(Step::ClosePrompt),
        (0..4u8).prop_map(Step::CloseMasked),
        (0..3u8).prop_map(Step::ChangeSession),
    ]
}

proptest! {
    #[test]
    fn prop_blocked_iff_open_needs(steps in proptest::collection::vec(step_strategy(), 0..25)) {
        let mut tracker = AttentionTracker::new(AttentionConfig {
            bell: vec![AlertKind::NeedsYou],
            desktop: vec![AlertKind::NeedsYou],
            done_after: Duration::from_secs(3600),
        });
        let base = Instant::now();
        let mut open: HashSet<NeedKey> = HashSet::new();
        let mut running = false;
        let mut alerted: HashSet<NeedKey> = HashSet::new();
        let mut session: Option<String> = None;

        for (index, step) in steps.iter().enumerate() {
            // Steps land 3 seconds apart, past the rate window, so every new key rings.
            let at = base + Duration::from_secs(3 * index as u64);
            let prompt = |id: u8| prompt_key("session-a", &format!("prompt-{id}"));
            let masked = |id: u8| NeedKey::Masked(format!("masked-{id}"));
            let activity = match step {
                Step::Start => Activity::TurnStarted { at },
                Step::EndComplete => Activity::TurnEnded { outcome: TurnOutcome::Complete, at },
                Step::OpenPrompt(id) => opened(prompt(*id), approval("ls", ""), at),
                Step::OpenMasked(id) => opened(masked(*id), NeedSummary::Sudo, at),
                Step::ClosePrompt(id) => Activity::NeedClosed { key: prompt(*id) },
                Step::CloseMasked(id) => Activity::NeedClosed { key: masked(*id) },
                Step::ChangeSession(id) => Activity::SessionChanged {
                    session_id: format!("session-{id}"),
                    label: "label".to_owned(),
                },
            };
            let opened_key = match &activity {
                Activity::NeedOpened { key, .. } => Some(key.clone()),
                _ => None,
            };
            let output = tracker.observe(activity);

            match step {
                Step::Start => running = true,
                Step::EndComplete => running = false,
                Step::OpenPrompt(id) => { open.insert(prompt(*id)); }
                Step::OpenMasked(id) => { open.insert(masked(*id)); }
                Step::ClosePrompt(id) => { open.remove(&prompt(*id)); }
                Step::CloseMasked(id) => { open.remove(&masked(*id)); }
                Step::ChangeSession(id) => {
                    // A new session id clears needs and the turn but keeps the key memory;
                    // the same id only updates the label.
                    let next = format!("session-{id}");
                    if session.as_ref() != Some(&next) {
                        open.clear();
                        running = false;
                        session = Some(next);
                    }
                }
            }
            let expected = if open.is_empty() {
                if running { PaneState::Working } else { PaneState::Idle }
            } else {
                PaneState::Blocked
            };
            prop_assert_eq!(
                tracker.pane().state,
                expected,
                "after step {:?}",
                step
            );

            if output.alert.is_some() {
                let key = opened_key.expect("only a need opening alerts here");
                prop_assert!(alerted.insert(key), "no key alerts twice");
            }
        }
    }
}
