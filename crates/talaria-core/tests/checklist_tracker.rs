//! When the checklist is visible and when it is committed.

use proptest::prelude::*;
use serde_json::{Value, json};
use talaria_core::checklist::{ChecklistTracker, CommitKind};
use talaria_core::session::{TodoState, TurnOutcome, TurnState};

fn pending_todos(count: usize) -> Vec<Value> {
    (0..count)
        .map(|index| json!({"id": format!("s{index}"), "content": "step", "status": "pending"}))
        .collect()
}

fn todo(revision: i64, todos: Vec<Value>) -> TodoState {
    TodoState { revision, todos }
}

fn three_pending(revision: i64) -> TodoState {
    todo(revision, pending_todos(3))
}

fn state_41() -> TodoState {
    todo(
        2,
        vec![
            json!({"id": "1", "content": "Read the failing test", "status": "completed"}),
            json!({"id": "2", "content": "Fix average()", "status": "in_progress"}),
            json!({"id": "3", "content": "Run the test suite", "status": "pending"}),
        ],
    )
}

#[test]
fn update_while_running_goes_live() {
    let mut tracker = ChecklistTracker::new();
    let seen = tracker.observe(Some(&three_pending(1)), &TurnState::Running);
    assert!(seen.commit.is_none());
    assert!(seen.live_changed);
    assert!(tracker.live().is_some());
}

#[test]
fn same_turn_update_replaces_live_list() {
    let mut tracker = ChecklistTracker::new();
    let _ = tracker.observe(Some(&three_pending(1)), &TurnState::Running);
    let replaced = tracker.observe(Some(&state_41()), &TurnState::Running);
    assert!(replaced.live_changed);
    let same_revision = TodoState {
        revision: 2,
        todos: pending_todos(2),
    };
    let again = tracker.observe(Some(&same_revision), &TurnState::Running);
    assert!(again.live_changed);
    let identical = tracker.observe(Some(&same_revision), &TurnState::Running);
    assert!(!identical.live_changed);
    assert!(identical.commit.is_none());
}

#[test]
fn turn_end_commits_once_and_hides() {
    let mut tracker = ChecklistTracker::new();
    let _ = tracker.observe(Some(&state_41()), &TurnState::Running);
    let ended = tracker.observe(
        Some(&state_41()),
        &TurnState::Finished(TurnOutcome::Completed),
    );
    let commit = ended.commit.expect("one commit");
    assert_eq!(commit.kind, CommitKind::TurnEnded);
    assert_eq!(commit.checklist.revision, 2);
    assert!(tracker.live().is_none());
    let repeat = tracker.observe(
        Some(&state_41()),
        &TurnState::Finished(TurnOutcome::Completed),
    );
    assert!(repeat.commit.is_none());
}

#[test]
fn interrupted_and_failed_turns_commit_the_last_list() {
    for outcome in [TurnOutcome::Interrupted, TurnOutcome::Failed] {
        let mut tracker = ChecklistTracker::new();
        let _ = tracker.observe(Some(&state_41()), &TurnState::Running);
        let ended = tracker.observe(Some(&state_41()), &TurnState::Finished(outcome));
        let commit = ended.commit.expect("one commit");
        assert_eq!(commit.kind, CommitKind::TurnEnded);
        assert!(commit.checklist.is_active());
    }
}

#[test]
fn committed_list_is_not_shown_again_until_it_changes() {
    let mut tracker = ChecklistTracker::new();
    let _ = tracker.observe(Some(&state_41()), &TurnState::Running);
    let _ = tracker.observe(
        Some(&state_41()),
        &TurnState::Finished(TurnOutcome::Completed),
    );
    let same = tracker.observe(Some(&state_41()), &TurnState::Running);
    assert!(same.commit.is_none());
    assert!(!same.live_changed);
    assert!(tracker.live().is_none());
    let newer = tracker.observe(Some(&three_pending(4)), &TurnState::Running);
    assert!(newer.live_changed);
    assert!(tracker.live().is_some());
}

#[test]
fn empty_list_hides_without_commit() {
    for todos in [Vec::new(), vec![json!(42)]] {
        let mut tracker = ChecklistTracker::new();
        let _ = tracker.observe(Some(&state_41()), &TurnState::Running);
        let cleared = tracker.observe(Some(&todo(3, todos)), &TurnState::Running);
        assert!(cleared.commit.is_none());
        assert!(cleared.live_changed);
        assert!(tracker.live().is_none());
        let ended = tracker.observe(
            Some(&todo(3, Vec::new())),
            &TurnState::Finished(TurnOutcome::Completed),
        );
        assert!(ended.commit.is_none());
    }
}

#[test]
fn idle_update_waits_for_the_next_running_turn() {
    let mut tracker = ChecklistTracker::new();
    let idle = tracker.observe(Some(&three_pending(5)), &TurnState::Idle);
    assert!(idle.commit.is_none());
    assert!(!idle.live_changed);
    assert!(tracker.live().is_none());
    let running = tracker.observe(Some(&three_pending(5)), &TurnState::Running);
    assert!(running.live_changed);
    assert!(tracker.live().is_some());
}

#[test]
fn attach_running_idle_and_empty() {
    let mut tracker = ChecklistTracker::new();
    let commit = tracker.attach(Some(&state_41()), true);
    assert!(commit.is_none());
    assert!(tracker.live().is_some());
    let mut tracker = ChecklistTracker::new();
    let commit = tracker.attach(Some(&state_41()), false);
    let commit = commit.expect("one resumed commit");
    assert_eq!(commit.kind, CommitKind::Resumed);
    assert!(tracker.live().is_none());
    let mut tracker = ChecklistTracker::new();
    assert!(tracker.attach(None, false).is_none());
    assert!(tracker.attach(Some(&todo(3, Vec::new())), false).is_none());
    assert!(tracker.live().is_none());
}

#[test]
fn toggle_and_reset() {
    let mut tracker = ChecklistTracker::new();
    assert!(tracker.expanded());
    assert!(!tracker.toggle());
    assert!(!tracker.expanded());
    let _ = tracker.observe(Some(&state_41()), &TurnState::Running);
    assert!(tracker.toggle());
    assert!(tracker.expanded());
    tracker.reset();
    assert!(tracker.expanded());
    assert!(tracker.live().is_none());
}

#[test]
fn reset_then_attach_behaves_like_a_resume() {
    let mut tracker = ChecklistTracker::new();
    let _ = tracker.observe(Some(&state_41()), &TurnState::Running);
    let _ = tracker.observe(
        Some(&state_41()),
        &TurnState::Finished(TurnOutcome::Completed),
    );
    tracker.reset();
    let commit = tracker.attach(Some(&state_41()), false);
    assert_eq!(commit.map(|commit| commit.kind), Some(CommitKind::Resumed));
    let running = tracker.observe(Some(&state_41()), &TurnState::Running);
    assert!(!running.live_changed);
    assert!(tracker.live().is_none());
}

#[derive(Clone, Copy, Debug)]
enum Step {
    Update { revision: u64, items: usize },
    Start,
    End(TurnOutcome),
    Toggle,
}

impl Step {
    /// Apply the step; the committed revision, if one was committed.
    fn apply(self, tracker: &mut ChecklistTracker, running: &mut bool) -> Option<u64> {
        match self {
            Step::Update { revision, items } => {
                let turn = if *running {
                    TurnState::Running
                } else {
                    TurnState::Idle
                };
                let revision = i64::try_from(revision).unwrap_or(i64::MAX);
                let seen = tracker.observe(Some(&todo(revision, pending_todos(items))), &turn);
                seen.commit.map(|commit| commit.checklist.revision)
            }
            Step::Start => {
                *running = true;
                let seen = tracker.observe(None, &TurnState::Running);
                seen.commit.map(|commit| commit.checklist.revision)
            }
            Step::End(outcome) => {
                *running = false;
                let seen = tracker.observe(None, &TurnState::Finished(outcome));
                seen.commit.map(|commit| commit.checklist.revision)
            }
            Step::Toggle => {
                tracker.toggle();
                None
            }
        }
    }
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (1..10u64, 0..6usize).prop_map(|(revision, items)| Step::Update { revision, items }),
        Just(Step::Start),
        prop_oneof![
            Just(Step::End(TurnOutcome::Completed)),
            Just(Step::End(TurnOutcome::Failed)),
            Just(Step::End(TurnOutcome::Interrupted)),
        ],
        Just(Step::Toggle),
    ]
}

proptest! {
    #[test]
    fn at_most_one_commit_per_turn(steps in prop::collection::vec(step(), 1..20)) {
        let mut tracker = ChecklistTracker::new();
        let mut running = false;
        let mut commits_since_start = 0;
        let mut last_commit_revision = 0u64;
        let mut floor = 0u64;
        for step in steps {
            // The reducer gate never lets a revision fall within a session.
            let step = match step {
                Step::Update { revision, items } => {
                    let revision = floor.max(revision);
                    floor = revision;
                    Step::Update { revision, items }
                }
                other => other,
            };
            if matches!(step, Step::Start) {
                commits_since_start = 0;
            }
            let committed = step.apply(&mut tracker, &mut running);
            if running {
                assert!(committed.is_none(), "no commit while running");
            } else if let Some(revision) = committed {
                commits_since_start += 1;
                assert!(commits_since_start <= 1, "at most one commit per turn end");
                assert!(
                    revision > last_commit_revision,
                    "a commit is newer than the last one"
                );
                last_commit_revision = revision;
            }
        }
    }
}
