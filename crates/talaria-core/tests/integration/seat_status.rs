//! Seat state transitions on the foreground attention tracker.

use std::time::{Duration, Instant};

use talaria_core::attention::{
    Activity, AlertKind, AttentionConfig, AttentionTracker, NeedKey, NeedSummary, TurnOutcome,
};
use talaria_core::session::{PromptKey, SessionId};
use talaria_core::{BlockKind, SeatState};

fn config() -> AttentionConfig {
    AttentionConfig {
        bell: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
        desktop: vec![],
        speech: vec![],
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

fn reach_done(tracker: &mut AttentionTracker, start: Instant) {
    tracker.observe(Activity::TurnStarted { at: start });
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: start + Duration::from_secs(1),
    });
    assert_eq!(tracker.seat_state(), SeatState::Done);
    assert_eq!(out.seat, Some(SeatState::Done));
    assert_eq!(out.alert, None);
}

#[test]
fn idle_when_no_turn_has_finished() {
    let mut tracker = AttentionTracker::new(config());
    assert_eq!(tracker.seat_state(), SeatState::Idle);

    let out = tracker.observe(Activity::SessionChanged {
        session_id: "session-a".to_owned(),
        label: "proj".to_owned(),
    });
    assert_eq!(tracker.seat_state(), SeatState::Idle);
    assert_eq!(out.seat, None);
}

#[test]
fn working_while_a_turn_runs() {
    let mut tracker = AttentionTracker::new(config());
    let base = Instant::now();

    let out = tracker.observe(Activity::TurnStarted { at: base });
    assert_eq!(tracker.seat_state(), SeatState::Working);
    assert_eq!(out.seat, Some(SeatState::Working));

    let out = tracker.observe(Activity::TurnStarted {
        at: base + Duration::from_secs(1),
    });
    assert_eq!(tracker.seat_state(), SeatState::Working);
    assert_eq!(out.seat, None);
}

#[test]
fn blocked_wins_over_a_running_turn() {
    let mut tracker = AttentionTracker::new(config());
    let base = Instant::now();
    let key = prompt_key("session-a", "prompt-1");

    let out = tracker.observe(Activity::TurnStarted { at: base });
    assert_eq!(out.seat, Some(SeatState::Working));

    let out = tracker.observe(opened(key.clone(), approval("ls", ""), base));
    assert_eq!(
        tracker.seat_state(),
        SeatState::Blocked(Some(BlockKind::Permission))
    );
    assert_eq!(
        out.seat,
        Some(SeatState::Blocked(Some(BlockKind::Permission)))
    );

    let out = tracker.observe(Activity::NeedClosed { key });
    assert_eq!(tracker.seat_state(), SeatState::Working);
    assert_eq!(out.seat, Some(SeatState::Working));
}

#[test]
fn kind_for_each_of_the_six_needs() {
    let base = Instant::now();
    let cases = [
        (approval("ls", ""), BlockKind::Permission),
        (
            NeedSummary::Question("Proceed?".to_owned()),
            BlockKind::Question,
        ),
        (NeedSummary::Sudo, BlockKind::Auth),
        (NeedSummary::Secret, BlockKind::Auth),
        (NeedSummary::VaultCode, BlockKind::Auth),
        (NeedSummary::VaultUnlock, BlockKind::Auth),
    ];

    for (summary, expected_kind) in cases {
        let mut tracker = AttentionTracker::new(config());
        let out = tracker.observe(opened(prompt_key("session-a", "prompt-1"), summary, base));
        let expected = SeatState::Blocked(Some(expected_kind));
        assert_eq!(tracker.seat_state(), expected);
        assert_eq!(out.seat, Some(expected));
    }
}

#[test]
fn first_open_need_sets_the_kind() {
    let mut tracker = AttentionTracker::new(config());
    let base = Instant::now();
    let question_key = prompt_key("session-a", "question-1");
    let approval_key = prompt_key("session-a", "approval-1");

    let out = tracker.observe(opened(
        question_key.clone(),
        NeedSummary::Question("Proceed?".to_owned()),
        base,
    ));
    assert_eq!(
        tracker.seat_state(),
        SeatState::Blocked(Some(BlockKind::Question))
    );
    assert_eq!(
        out.seat,
        Some(SeatState::Blocked(Some(BlockKind::Question)))
    );

    let out = tracker.observe(opened(approval_key, approval("ls", ""), base));
    assert_eq!(
        tracker.seat_state(),
        SeatState::Blocked(Some(BlockKind::Question))
    );
    assert_eq!(out.seat, None);

    let out = tracker.observe(Activity::NeedClosed { key: question_key });
    assert_eq!(
        tracker.seat_state(),
        SeatState::Blocked(Some(BlockKind::Permission))
    );
    assert_eq!(
        out.seat,
        Some(SeatState::Blocked(Some(BlockKind::Permission)))
    );
}

#[test]
fn done_lasts_until_the_next_turn() {
    let mut tracker = AttentionTracker::new(config());
    let base = Instant::now();

    tracker.observe(Activity::TurnStarted { at: base });
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Complete,
        at: base + Duration::from_secs(1),
    });
    assert_eq!(tracker.seat_state(), SeatState::Done);
    assert_eq!(out.seat, Some(SeatState::Done));
    assert_eq!(out.alert, None);

    assert_eq!(tracker.seat_state(), SeatState::Done);

    let out = tracker.observe(Activity::TurnStarted {
        at: base + Duration::from_secs(2),
    });
    assert_eq!(tracker.seat_state(), SeatState::Working);
    assert_eq!(out.seat, Some(SeatState::Working));
}

#[test]
fn failed_turn_is_error() {
    let mut tracker = AttentionTracker::new(config());
    let base = Instant::now();

    tracker.observe(Activity::TurnStarted { at: base });
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Failed,
        at: base + Duration::from_secs(1),
    });
    assert_eq!(tracker.seat_state(), SeatState::Error);
    assert_eq!(out.seat, Some(SeatState::Error));
    let alert = out.alert.expect("a failed turn emits a failed alert");
    assert_eq!(alert.kind, AlertKind::Failed);
}

#[test]
fn interrupted_turn_is_idle() {
    let mut tracker = AttentionTracker::new(config());
    let base = Instant::now();

    tracker.observe(Activity::TurnStarted { at: base });
    let out = tracker.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Interrupted,
        at: base + Duration::from_secs(1),
    });
    assert_eq!(tracker.seat_state(), SeatState::Idle);
    assert_eq!(out.seat, Some(SeatState::Idle));
    assert_eq!(out.alert, None);
}

#[test]
fn session_change_is_idle() {
    let base = Instant::now();

    let mut tracker = AttentionTracker::new(config());
    reach_done(&mut tracker, base);
    let out = tracker.observe(Activity::SessionChanged {
        session_id: "session-a".to_owned(),
        label: "proj".to_owned(),
    });
    assert_eq!(tracker.seat_state(), SeatState::Idle);
    assert_eq!(out.seat, Some(SeatState::Idle));

    let mut same_session = AttentionTracker::new(config());
    same_session.observe(Activity::SessionChanged {
        session_id: "S".to_owned(),
        label: "first".to_owned(),
    });
    reach_done(&mut same_session, base);
    let out = same_session.observe(Activity::SessionChanged {
        session_id: "S".to_owned(),
        label: "second".to_owned(),
    });
    assert_eq!(same_session.seat_state(), SeatState::Done);
    assert_eq!(out.seat, None);
}

#[test]
fn stray_turn_end_leaves_the_state() {
    let base = Instant::now();

    let mut fresh = AttentionTracker::new(config());
    let out = fresh.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Failed,
        at: base,
    });
    assert_eq!(fresh.seat_state(), SeatState::Idle);
    assert_eq!(out.seat, None);

    let mut finished = AttentionTracker::new(config());
    reach_done(&mut finished, base);
    let out = finished.observe(Activity::TurnEnded {
        outcome: TurnOutcome::Failed,
        at: base + Duration::from_secs(2),
    });
    assert_eq!(finished.seat_state(), SeatState::Done);
    assert_eq!(out.seat, None);
}

#[test]
fn blocked_keeps_its_kind_when_the_turn_ends() {
    let mut tracker = AttentionTracker::new(config());
    let base = Instant::now();
    let key = prompt_key("session-a", "prompt-1");

    let out = tracker.observe(opened(key.clone(), approval("ls", ""), base));
    assert_eq!(
        tracker.seat_state(),
        SeatState::Blocked(Some(BlockKind::Permission))
    );
    assert_eq!(
        out.seat,
        Some(SeatState::Blocked(Some(BlockKind::Permission)))
    );

    for (index, outcome) in [
        TurnOutcome::Complete,
        TurnOutcome::Failed,
        TurnOutcome::Interrupted,
    ]
    .into_iter()
    .enumerate()
    {
        let offset = Duration::from_secs((index as u64 + 1) * 2);
        let out = tracker.observe(Activity::TurnStarted { at: base + offset });
        assert_eq!(out.seat, None);

        let out = tracker.observe(Activity::TurnEnded {
            outcome,
            at: base + offset + Duration::from_secs(1),
        });
        assert_eq!(
            tracker.seat_state(),
            SeatState::Blocked(Some(BlockKind::Permission))
        );
        assert_eq!(out.seat, None);
    }

    let out = tracker.observe(Activity::NeedClosed { key });
    assert_eq!(tracker.seat_state(), SeatState::Idle);
    assert_eq!(out.seat, Some(SeatState::Idle));
}
