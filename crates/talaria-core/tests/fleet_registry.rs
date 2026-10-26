//! Polled row rules, membership, order, and profile status.

use talaria_core::attention::{AlertKind, NeedSummary};
use talaria_core::fleet::{
    CallError, FleetEffect, FleetInput, FleetRegistry, LiveItem, LiveStatus, NeedSlot, Outcome,
    PolledApproval, ProfileStatus, RosterRow, RowKey, RowState, SubscribedNeed, SubscribedTurn,
    SubscribedView, update,
};

fn key(profile: &str, session: &str) -> RowKey {
    RowKey {
        profile: profile.into(),
        session_id: session.into(),
    }
}

fn fixture(session: &str) -> RowKey {
    key("fixture", session)
}

fn roster(session: &str, running: bool, count: u64) -> RosterRow {
    RosterRow {
        session_id: session.into(),
        title: String::new(),
        source: Some("tui".into()),
        started_at: Some(1_700_000_000.0),
        message_count: count,
        running,
        model: None,
    }
}

fn live(session: &str, status: LiveStatus, count: u64) -> LiveItem {
    LiveItem {
        session_id: session.into(),
        status,
        last_active: Some(1_700_000_010.0),
        message_count: count,
        model: None,
        title: String::new(),
    }
}

fn names(effects: &[FleetEffect]) -> Vec<&str> {
    effects
        .iter()
        .map(|effect| match effect {
            FleetEffect::FetchApprovals { .. } => "fetch",
            FleetEffect::ProbeOutcome { .. } => "probe",
            FleetEffect::Attention { kind, .. } => match kind {
                AlertKind::NeedsYou => "needs",
                AlertKind::Done => "done",
                AlertKind::Failed => "failed",
            },
            FleetEffect::Diagnostic(_) => "diagnostic",
        })
        .collect()
}

fn list(reg: &mut FleetRegistry, session: &str) {
    let _ = update(
        reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 1,
            rows: vec![roster(session, true, 4)],
            malformed: 0,
            at: 10.0,
        },
    );
}

fn view_running() -> SubscribedView {
    SubscribedView {
        turn: SubscribedTurn::Running {
            started_at: Some(10.0),
        },
        needs: Vec::new(),
        activity: None,
        cost_usd: None,
        model: None,
        title: None,
    }
}

#[test]
fn unsubscribed_state_table() {
    let mut reg = FleetRegistry::new();
    list(&mut reg, "s1");
    let cases = [
        (LiveStatus::Waiting, RowState::Working, true),
        (LiveStatus::Working, RowState::Working, false),
        (LiveStatus::Starting, RowState::Working, false),
        (LiveStatus::Other("paused".into()), RowState::Working, false),
    ];
    for (index, (status, state, fetches)) in cases.into_iter().enumerate() {
        let mut one = FleetRegistry::new();
        list(&mut one, "s1");
        let effects = update(
            &mut one,
            FleetInput::Liveness {
                profile: "fixture".into(),
                request: 2 + index as u64,
                items: vec![live("s1", status, 4)],
                malformed: 0,
                at: 11.0,
            },
        );
        assert_eq!(one.row(&fixture("s1")).expect("row").state, state);
        assert_eq!(names(&effects).contains(&"fetch"), fetches);
    }
    let mut idle_after = FleetRegistry::new();
    list(&mut idle_after, "s1");
    let _ = update(
        &mut idle_after,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Working, 4)],
            malformed: 0,
            at: 11.0,
        },
    );
    let probed = update(
        &mut idle_after,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 3,
            items: vec![live("s1", LiveStatus::Idle, 4)],
            malformed: 0,
            at: 12.0,
        },
    );
    assert_eq!(names(&probed), vec!["probe"]);
    let _ = reg;
}

#[test]
fn missed_turn_detected_by_message_count() {
    let mut reg = FleetRegistry::new();
    list(&mut reg, "s1");
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Idle, 4)],
            malformed: 0,
            at: 11.0,
        },
    );
    let before = reg.version();
    let probed = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 3,
            items: vec![live("s1", LiveStatus::Idle, 6)],
            malformed: 0,
            at: 12.0,
        },
    );
    assert_eq!(names(&probed), vec!["probe"]);
    let done = update(
        &mut reg,
        FleetInput::OutcomeProbed {
            key: fixture("s1"),
            outcome: Outcome::Completed,
            found_complete: true,
            at: 13.0,
        },
    );
    assert_eq!(reg.row(&fixture("s1")).expect("row").state, RowState::Done);
    assert!(names(&done).contains(&"done"));

    let mut silent = FleetRegistry::new();
    list(&mut silent, "s1");
    let _ = update(
        &mut silent,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Idle, 4)],
            malformed: 0,
            at: 11.0,
        },
    );
    let version = silent.version();
    let _ = update(
        &mut silent,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 3,
            items: vec![live("s1", LiveStatus::Idle, 6)],
            malformed: 0,
            at: 12.0,
        },
    );
    let effects = update(
        &mut silent,
        FleetInput::OutcomeProbed {
            key: fixture("s1"),
            outcome: Outcome::Unknown,
            found_complete: false,
            at: 13.0,
        },
    );
    assert!(effects.is_empty());
    assert_eq!(
        silent.row(&fixture("s1")).expect("row").state,
        RowState::Idle
    );
    assert_eq!(silent.version(), version);
    let _ = before;
}

#[test]
fn failed_liveness_keeps_state_and_vanished_turn_is_done() {
    let mut reg = FleetRegistry::new();
    list(&mut reg, "s1");
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Waiting, 4)],
            malformed: 0,
            at: 11.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Approvals {
            key: fixture("s1"),
            request: 3,
            approvals: vec![PolledApproval {
                prompt_id: Some("a1b2".into()),
                command: Some("rm -rf ./talaria-fleet-probe".into()),
                description: None,
                choices: vec!["deny".into()],
            }],
            at: 12.0,
        },
    );
    let failed = update(
        &mut reg,
        FleetInput::LivenessFailed {
            profile: "fixture".into(),
            request: 4,
            error: CallError::NoReply,
            at: 13.0,
        },
    );
    assert_eq!(
        reg.row(&fixture("s1")).expect("row").state,
        RowState::NeedsYou
    );
    assert_eq!(reg.queue().len(), 1);
    assert!(reg.row(&fixture("s1")).expect("row").stale);
    assert!(failed.is_empty() || !names(&failed).contains(&"probe"));

    let mut gone = FleetRegistry::new();
    list(&mut gone, "s1");
    let _ = update(
        &mut gone,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Working, 4)],
            malformed: 0,
            at: 11.0,
        },
    );
    let effects = update(
        &mut gone,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 3,
            items: Vec::new(),
            malformed: 0,
            at: 12.0,
        },
    );
    let row = gone.row(&fixture("s1")).expect("row");
    assert_eq!(row.state, RowState::Done);
    assert_eq!(row.outcome, Some(Outcome::Unknown));
    assert!(names(&effects).contains(&"done"));
    assert!(!names(&effects).contains(&"probe"));
    assert!(!names(&effects).contains(&"fetch"));
}

#[test]
fn subscribed_view_overrides_polled_status() {
    let mut reg = FleetRegistry::new();
    list(&mut reg, "s1");
    let _ = update(
        &mut reg,
        FleetInput::Subscribed {
            key: fixture("s1"),
            view: SubscribedView {
                turn: SubscribedTurn::Running {
                    started_at: Some(10.0),
                },
                needs: vec![SubscribedNeed {
                    slot: NeedSlot::Prompt("a1b2".into()),
                    summary: NeedSummary::Question("which file".into()),
                    choices: Vec::new(),
                    execution_generation: Some(1),
                    timeout_s: None,
                }],
                activity: Some("reading".into()),
                cost_usd: Some(0.01),
                model: Some("minimax-m3.1-flash".into()),
                title: Some("notes".into()),
            },
            at: 11.0,
        },
    );
    assert_eq!(
        reg.row(&fixture("s1")).expect("row").state,
        RowState::NeedsYou
    );
    let waiting = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Waiting, 9)],
            malformed: 0,
            at: 12.0,
        },
    );
    assert!(!names(&waiting).contains(&"fetch"));
    assert!(!names(&waiting).contains(&"probe"));
    assert_eq!(reg.queue().len(), 1);
    let idle = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 3,
            items: vec![live("s1", LiveStatus::Idle, 9)],
            malformed: 0,
            at: 13.0,
        },
    );
    assert!(idle.is_empty());
    assert_eq!(
        reg.row(&fixture("s1")).expect("row").state,
        RowState::NeedsYou
    );

    let _ = update(
        &mut reg,
        FleetInput::Foreground {
            key: Some(fixture("s1")),
            at: 14.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Subscribed {
            key: fixture("s1"),
            view: SubscribedView {
                turn: SubscribedTurn::Finished(Outcome::Completed),
                needs: Vec::new(),
                activity: None,
                cost_usd: None,
                model: None,
                title: None,
            },
            at: 15.0,
        },
    );
    assert_eq!(reg.row(&fixture("s1")).expect("row").state, RowState::Idle);
}

#[test]
fn stale_replies_are_dropped() {
    let mut reg = FleetRegistry::new();
    let first = update(
        &mut reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 5,
            rows: vec![roster("s1", true, 1)],
            malformed: 0,
            at: 10.0,
        },
    );
    assert_eq!(reg.rows().len(), 1);
    let version = reg.version();
    let stale = update(
        &mut reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 4,
            rows: vec![roster("s2", true, 1)],
            malformed: 0,
            at: 11.0,
        },
    );
    assert!(stale.is_empty());
    assert_eq!(reg.version(), version);
    assert!(reg.row(&fixture("s2")).is_none());
    let _ = first;
}

#[test]
fn liveness_before_roster_is_kept() {
    let mut reg = FleetRegistry::new();
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 1,
            items: vec![live("s1", LiveStatus::Waiting, 4)],
            malformed: 0,
            at: 10.0,
        },
    );
    assert!(reg.rows().is_empty());
    let effects = update(
        &mut reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 2,
            rows: vec![roster("s1", true, 4)],
            malformed: 0,
            at: 11.0,
        },
    );
    let row = reg.row(&fixture("s1")).expect("row");
    assert_eq!(row.state, RowState::Working);
    assert!(reg.queue().is_empty());
    assert!(names(&effects).contains(&"fetch"));
    let _ = update(
        &mut reg,
        FleetInput::Approvals {
            key: fixture("s1"),
            request: 3,
            approvals: vec![PolledApproval {
                prompt_id: Some("a1b2".into()),
                command: Some("rm -rf ./talaria-fleet-probe".into()),
                description: None,
                choices: vec!["deny".into()],
            }],
            at: 12.0,
        },
    );
    assert_eq!(
        reg.row(&fixture("s1")).expect("row").state,
        RowState::NeedsYou
    );
}

#[test]
fn waiting_session_outside_the_list_gets_a_row() {
    let mut reg = FleetRegistry::new();
    let _ = update(
        &mut reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 1,
            rows: vec![roster("listed", true, 1)],
            malformed: 0,
            at: 10.0,
        },
    );
    let effects = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![
                live("listed", LiveStatus::Idle, 1),
                live("hidden", LiveStatus::Waiting, 2),
            ],
            malformed: 0,
            at: 11.0,
        },
    );
    let hidden = reg.row(&fixture("hidden")).expect("unlisted");
    assert!(hidden.unlisted);
    assert!(names(&effects).contains(&"fetch"));
    let opened = update(
        &mut reg,
        FleetInput::Approvals {
            key: fixture("hidden"),
            request: 3,
            approvals: vec![PolledApproval {
                prompt_id: Some("a1b2".into()),
                command: Some("rm -rf ./talaria-fleet-probe".into()),
                description: None,
                choices: vec!["deny".into()],
            }],
            at: 12.0,
        },
    );
    assert!(names(&opened).contains(&"needs"));
    let probed = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 4,
            items: vec![live("hidden", LiveStatus::Idle, 3)],
            malformed: 0,
            at: 13.0,
        },
    );
    assert!(reg.queue().is_empty());
    assert!(names(&probed).contains(&"probe"));
    let done = update(
        &mut reg,
        FleetInput::OutcomeProbed {
            key: fixture("hidden"),
            outcome: Outcome::Completed,
            found_complete: true,
            at: 14.0,
        },
    );
    let row = reg
        .row(&fixture("hidden"))
        .expect("still listed until acknowledged");
    assert_eq!(row.state, RowState::Done);
    assert!(row.unlisted);
    assert!(names(&done).contains(&"done"));
    let _ = update(
        &mut reg,
        FleetInput::Acknowledge {
            key: fixture("hidden"),
            at: 15.0,
        },
    );
    assert!(reg.row(&fixture("hidden")).is_none());

    let mut working = FleetRegistry::new();
    let _ = update(
        &mut working,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 1,
            items: vec![live("past", LiveStatus::Waiting, 1)],
            malformed: 0,
            at: 10.0,
        },
    );
    let _ = update(
        &mut working,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("past", LiveStatus::Working, 2)],
            malformed: 0,
            at: 11.0,
        },
    );
    assert_eq!(
        working.row(&fixture("past")).expect("row").state,
        RowState::Working
    );
}

#[test]
fn rows_leave_after_two_rosters_and_gone_rows_wait() {
    let mut reg = FleetRegistry::new();
    let _ = update(
        &mut reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 1,
            rows: vec![roster("s1", true, 1), roster("s2", false, 1)],
            malformed: 0,
            at: 10.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 2,
            rows: vec![roster("s2", false, 1)],
            malformed: 0,
            at: 11.0,
        },
    );
    assert!(reg.row(&fixture("s1")).is_some());
    let _ = update(
        &mut reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 3,
            rows: vec![roster("s2", false, 1)],
            malformed: 0,
            at: 12.0,
        },
    );
    assert!(reg.row(&fixture("s1")).is_none());

    let mut held = FleetRegistry::new();
    list(&mut held, "s1");
    let _ = update(
        &mut held,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Working, 1)],
            malformed: 0,
            at: 11.0,
        },
    );
    let _ = update(
        &mut held,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 3,
            items: Vec::new(),
            malformed: 0,
            at: 12.0,
        },
    );
    let _ = update(
        &mut held,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 4,
            rows: Vec::new(),
            malformed: 0,
            at: 13.0,
        },
    );
    let _ = update(
        &mut held,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 5,
            rows: Vec::new(),
            malformed: 0,
            at: 14.0,
        },
    );
    let row = held.row(&fixture("s1")).expect("gone row stays");
    assert!(row.gone);
    assert_eq!(row.state, RowState::Done);

    let mut cap = FleetRegistry::new();
    let many: Vec<RosterRow> = (0..51)
        .map(|index| roster(&format!("g{index}"), true, 1))
        .collect();
    let _ = update(
        &mut cap,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 1,
            rows: many,
            malformed: 0,
            at: 10.0,
        },
    );
    for index in 0..51 {
        let _ = update(
            &mut cap,
            FleetInput::Liveness {
                profile: "fixture".into(),
                request: 2 + index,
                items: vec![live(&format!("g{index}"), LiveStatus::Working, 1)],
                malformed: 0,
                at: 11.0,
            },
        );
        let _ = update(
            &mut cap,
            FleetInput::Liveness {
                profile: "fixture".into(),
                request: 100 + index,
                items: Vec::new(),
                malformed: 0,
                at: 12.0,
            },
        );
    }
    let _ = update(
        &mut cap,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 200,
            rows: Vec::new(),
            malformed: 0,
            at: 13.0,
        },
    );
    let _ = update(
        &mut cap,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 201,
            rows: Vec::new(),
            malformed: 0,
            at: 14.0,
        },
    );
    let gone = cap.rows().iter().filter(|row| row.gone).count();
    assert!(gone <= 50);
}

#[test]
fn done_acknowledge_and_new_turn() {
    let mut reg = FleetRegistry::new();
    list(&mut reg, "s1");
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Working, 1)],
            malformed: 0,
            at: 11.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 3,
            items: Vec::new(),
            malformed: 0,
            at: 12.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Acknowledge {
            key: fixture("s1"),
            at: 13.0,
        },
    );
    assert_eq!(reg.row(&fixture("s1")).expect("row").state, RowState::Idle);
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 4,
            items: vec![live("s1", LiveStatus::Working, 2)],
            malformed: 0,
            at: 14.0,
        },
    );
    assert_eq!(
        reg.row(&fixture("s1")).expect("row").state,
        RowState::Working
    );
}

#[test]
fn attention_once_per_episode() {
    let mut reg = FleetRegistry::new();
    list(&mut reg, "s1");
    let first = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Waiting, 1)],
            malformed: 0,
            at: 11.0,
        },
    );
    assert!(!names(&first).contains(&"needs"));
    let opened = update(
        &mut reg,
        FleetInput::Approvals {
            key: fixture("s1"),
            request: 3,
            approvals: vec![PolledApproval {
                prompt_id: Some("a1b2".into()),
                command: Some("ls".into()),
                description: None,
                choices: vec!["once".into()],
            }],
            at: 12.0,
        },
    );
    assert_eq!(
        names(&opened)
            .iter()
            .filter(|name| **name == "needs")
            .count(),
        1
    );
    let again = update(
        &mut reg,
        FleetInput::Approvals {
            key: fixture("s1"),
            request: 4,
            approvals: vec![PolledApproval {
                prompt_id: Some("a1b2".into()),
                command: Some("ls -l".into()),
                description: None,
                choices: vec!["once".into()],
            }],
            at: 13.0,
        },
    );
    assert!(!names(&again).contains(&"needs"));
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 5,
            items: vec![live("s1", LiveStatus::Idle, 1)],
            malformed: 0,
            at: 14.0,
        },
    );
    let second = update(
        &mut reg,
        FleetInput::Approvals {
            key: fixture("s1"),
            request: 6,
            approvals: vec![PolledApproval {
                prompt_id: Some("b2".into()),
                command: Some("pwd".into()),
                description: None,
                choices: vec!["once".into()],
            }],
            at: 16.0,
        },
    );
    let _ = second;
}

#[test]
fn identical_poll_changes_nothing() {
    let mut reg = FleetRegistry::new();
    list(&mut reg, "s1");
    let input = FleetInput::Liveness {
        profile: "fixture".into(),
        request: 2,
        items: vec![live("s1", LiveStatus::Idle, 4)],
        malformed: 0,
        at: 11.0,
    };
    let _ = update(&mut reg, input.clone());
    let version = reg.version();
    let effects = update(&mut reg, input);
    assert!(effects.is_empty());
    assert_eq!(reg.version(), version);
}

#[test]
fn profile_status_transitions() {
    let errors = [
        (CallError::NoReply, "no reply"),
        (
            CallError::ProfileMismatch,
            "no longer served by the gateway",
        ),
        (CallError::StorageUnavailable, "Hermes storage busy"),
        (CallError::UnknownMethod, "unknown method"),
        (CallError::NotFound, "not_found"),
        (
            CallError::Refused("permission_denied".into()),
            "permission_denied",
        ),
        (
            CallError::Refused("runtime_retired".into()),
            "runtime_retired",
        ),
    ];
    for (index, (error, reason)) in errors.into_iter().enumerate() {
        let mut reg = FleetRegistry::new();
        list(&mut reg, "s1");
        let _ = update(
            &mut reg,
            FleetInput::RosterFailed {
                profile: "fixture".into(),
                request: 2 + index as u64,
                error,
                at: 11.0,
            },
        );
        match reg.profiles().iter().find(|(name, _)| *name == "fixture") {
            Some((_, ProfileStatus::Failed { reason: got })) => assert_eq!(got, reason),
            other => panic!("profile status {other:?}"),
        }
        assert!(reg.row(&fixture("s1")).expect("kept").stale);
    }
    let mut parked = FleetRegistry::new();
    let _ = update(
        &mut parked,
        FleetInput::Profiles {
            served: vec!["fixture".into()],
            parked: vec![("fixture-1".into(), "draining".into())],
            in_scope: vec!["fixture".into()],
            at: 10.0,
        },
    );
    assert!(matches!(
        parked.profiles().iter().find(|(name, _)| *name == "fixture-1"),
        Some((_, ProfileStatus::Parked { reason })) if reason == "draining"
    ));
    let mut truncated = FleetRegistry::new();
    let rows: Vec<RosterRow> = (0..200)
        .map(|index| roster(&format!("n{index}"), false, 1))
        .collect();
    let _ = update(
        &mut truncated,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 1,
            rows,
            malformed: 0,
            at: 10.0,
        },
    );
    assert!(matches!(
        truncated
            .profiles()
            .iter()
            .find(|(name, _)| *name == "fixture"),
        Some((_, ProfileStatus::Listed { truncated: true }))
    ));
}

#[test]
fn connection_lost_marks_stale() {
    let mut reg = FleetRegistry::new();
    list(&mut reg, "s1");
    let _ = update(&mut reg, FleetInput::ConnectionLost { at: 11.0 });
    assert!(reg.row(&fixture("s1")).expect("kept").stale);
    let _ = update(
        &mut reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 2,
            rows: vec![roster("s1", true, 1)],
            malformed: 0,
            at: 12.0,
        },
    );
    assert!(!reg.row(&fixture("s1")).expect("refreshed").stale);
}

#[test]
fn foreground_items_flagged_and_not_counted() {
    let mut reg = FleetRegistry::new();
    list(&mut reg, "s1");
    let _ = update(
        &mut reg,
        FleetInput::Foreground {
            key: Some(fixture("s1")),
            at: 11.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Working, 1)],
            malformed: 0,
            at: 12.0,
        },
    );
    let effects = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 3,
            items: vec![live("s1", LiveStatus::Idle, 1)],
            malformed: 0,
            at: 13.0,
        },
    );
    assert_eq!(reg.row(&fixture("s1")).expect("row").state, RowState::Idle);
    assert!(!names(&effects).contains(&"probe"));
    assert!(
        !names(&effects)
            .iter()
            .any(|name| matches!(*name, "done" | "failed"))
    );
    let _ = update(
        &mut reg,
        FleetInput::Subscribed {
            key: fixture("s1"),
            view: view_running(),
            at: 14.0,
        },
    );
    let after = update(
        &mut reg,
        FleetInput::Subscribed {
            key: fixture("s1"),
            view: SubscribedView {
                turn: SubscribedTurn::Finished(Outcome::Failed),
                needs: Vec::new(),
                activity: None,
                cost_usd: None,
                model: None,
                title: None,
            },
            at: 15.0,
        },
    );
    assert_eq!(reg.row(&fixture("s1")).expect("row").state, RowState::Idle);
    assert!(!names(&after).contains(&"failed"));
    let mut done = FleetRegistry::new();
    list(&mut done, "s1");
    let _ = update(
        &mut done,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: Vec::new(),
            malformed: 0,
            at: 11.0,
        },
    );
    let _ = update(
        &mut done,
        FleetInput::Foreground {
            key: Some(fixture("s1")),
            at: 12.0,
        },
    );
    assert_eq!(done.row(&fixture("s1")).expect("row").state, RowState::Idle);
    assert_eq!(done.counts().needs_you, 0);
    assert!(done.row(&fixture("s1")).expect("row").foreground);
}

#[test]
fn canonical_row_order() {
    let mut reg = FleetRegistry::new();
    let _ = update(
        &mut reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 1,
            rows: vec![roster("b", false, 1), roster("a", true, 1)],
            malformed: 0,
            at: 10.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Roster {
            profile: "fixture-1".into(),
            request: 2,
            rows: vec![roster("a", false, 1)],
            malformed: 0,
            at: 10.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 3,
            items: vec![
                live("a", LiveStatus::Working, 1),
                live("b", LiveStatus::Idle, 1),
            ],
            malformed: 0,
            at: 11.0,
        },
    );
    let ids: Vec<String> = reg
        .rows()
        .iter()
        .map(|row| format!("{}:{}", row.key.profile, row.key.session_id))
        .collect();
    assert_eq!(
        ids.first().map(String::as_str),
        Some("fixture:a"),
        "{ids:?}"
    );
    let states: Vec<RowState> = reg.rows().iter().map(|row| row.state).collect();
    let mut rank = 0;
    for state in states {
        let next = match state {
            RowState::NeedsYou => 0,
            RowState::Working => 1,
            RowState::Failed => 2,
            RowState::Done => 3,
            RowState::Idle => 4,
        };
        assert!(next >= rank);
        rank = next;
    }
}
