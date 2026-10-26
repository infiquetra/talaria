//! Queue identity, refinement, and the invariants.

use proptest::prelude::*;
use talaria_core::attention::{AlertKind, NeedSummary};
use talaria_core::fleet::{
    CallError, FleetEffect, FleetInput, FleetRegistry, LiveItem, LiveStatus, NeedDetail, NeedSlot,
    Outcome, PolledApproval, RosterRow, RowKey, RowState, SubscribedNeed, SubscribedTurn,
    SubscribedView, update,
};

fn key(session: &str) -> RowKey {
    RowKey {
        profile: "fixture".into(),
        session_id: session.into(),
    }
}

fn roster(session: &str) -> RosterRow {
    RosterRow {
        session_id: session.into(),
        title: String::new(),
        source: Some("tui".into()),
        started_at: Some(1_700_000_000.0),
        message_count: 1,
        running: true,
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

fn listed(reg: &mut FleetRegistry, session: &str) {
    let _ = update(
        reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 1,
            rows: vec![roster(session)],
            malformed: 0,
            at: 10.0,
        },
    );
}

fn approval_view(prompt: &str, generation: i64) -> SubscribedView {
    SubscribedView {
        turn: SubscribedTurn::Running {
            started_at: Some(10.0),
        },
        needs: vec![SubscribedNeed {
            slot: NeedSlot::Prompt(prompt.into()),
            summary: NeedSummary::Approval {
                command: "rm -rf ./talaria-fleet-probe".into(),
                description: "recursive delete".into(),
            },
            choices: vec!["once".into(), "deny".into()],
            execution_generation: Some(generation),
            timeout_s: None,
        }],
        activity: None,
        cost_usd: None,
        model: None,
        title: None,
    }
}

fn effects_kind(effects: &[FleetEffect]) -> Vec<&str> {
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

#[test]
fn unsubscribe_downgrades_exact_items() {
    let mut reg = FleetRegistry::new();
    listed(&mut reg, "s1");
    let _ = update(
        &mut reg,
        FleetInput::Subscribed {
            key: key("s1"),
            view: approval_view("a1b2", 3),
            at: 11.0,
        },
    );
    let before = reg.queue()[0];
    let seq = before.seq;
    assert!(before.exact);
    assert_eq!(before.execution_generation, Some(3));
    let _ = update(
        &mut reg,
        FleetInput::Unsubscribed {
            key: key("s1"),
            at: 12.0,
        },
    );
    let after = &reg.queue()[0];
    assert_eq!(after.seq, seq);
    assert!(!after.exact);
    assert_eq!(after.execution_generation, None);
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Waiting, 1)],
            malformed: 0,
            at: 13.0,
        },
    );
    assert_eq!(reg.queue().len(), 1);
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 3,
            items: vec![live("s1", LiveStatus::Idle, 1)],
            malformed: 0,
            at: 14.0,
        },
    );
    assert!(reg.queue().is_empty());
}

#[test]
fn waiting_placeholder_refines_without_attention() {
    let mut reg = FleetRegistry::new();
    listed(&mut reg, "s1");
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
    assert!(effects_kind(&first).contains(&"fetch"));
    let waited = update(
        &mut reg,
        FleetInput::ApprovalsFailed {
            key: key("s1"),
            request: 3,
            error: CallError::NoReply,
            at: 12.0,
        },
    );
    assert!(effects_kind(&waited).contains(&"needs"));
    let seq = reg.queue()[0].seq;
    let since = reg.queue()[0].since;
    let refined = update(
        &mut reg,
        FleetInput::Approvals {
            key: key("s1"),
            request: 4,
            approvals: vec![PolledApproval {
                prompt_id: Some("a1b2".into()),
                command: Some("rm -rf ./talaria-fleet-probe".into()),
                description: None,
                choices: vec!["once".into(), "deny".into()],
            }],
            at: 13.0,
        },
    );
    assert!(!effects_kind(&refined).contains(&"needs"));
    assert_eq!(reg.queue().len(), 1);
    assert_eq!(reg.queue()[0].seq, seq);
    assert_eq!(reg.queue()[0].since, since);
    assert!(matches!(reg.queue()[0].id.slot, NeedSlot::Prompt(ref id) if id == "a1b2"));
}

#[test]
fn prompt_identity_spans_sources() {
    let mut reg = FleetRegistry::new();
    listed(&mut reg, "s1");
    let _ = update(
        &mut reg,
        FleetInput::Subscribed {
            key: key("s1"),
            view: approval_view("a1b2", 3),
            at: 11.0,
        },
    );
    let seq = reg.queue()[0].seq;
    let _ = update(
        &mut reg,
        FleetInput::Approvals {
            key: key("s1"),
            request: 4,
            approvals: vec![PolledApproval {
                prompt_id: Some("a1b2".into()),
                command: Some("other".into()),
                description: None,
                choices: vec!["deny".into()],
            }],
            at: 12.0,
        },
    );
    assert_eq!(reg.queue().len(), 1);
    assert_eq!(reg.queue()[0].seq, seq);
    assert!(reg.queue()[0].exact);
    assert_eq!(reg.queue()[0].execution_generation, Some(3));
}

#[test]
fn queue_order_is_first_seen_and_stable() {
    let mut reg = FleetRegistry::new();
    let _ = update(
        &mut reg,
        FleetInput::Roster {
            profile: "fixture".into(),
            request: 1,
            rows: vec![roster("s1"), roster("s2")],
            malformed: 0,
            at: 10.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Subscribed {
            key: key("s2"),
            view: approval_view("b", 1),
            at: 11.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Subscribed {
            key: key("s1"),
            view: approval_view("a", 1),
            at: 12.0,
        },
    );
    let seqs: Vec<u64> = reg.queue().iter().map(|item| item.seq).collect();
    assert_eq!(seqs, vec![1, 2]);
    let ids: Vec<String> = reg
        .queue()
        .iter()
        .map(|item| item.id.key.session_id.clone())
        .collect();
    assert_eq!(ids, vec!["s2".to_string(), "s1".to_string()]);
}

#[test]
fn liveness_settles_polled_items_only() {
    let mut reg = FleetRegistry::new();
    listed(&mut reg, "s1");
    let _ = update(
        &mut reg,
        FleetInput::Subscribed {
            key: key("s1"),
            view: approval_view("exact", 2),
            at: 11.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Approvals {
            key: key("s1"),
            request: 4,
            approvals: vec![PolledApproval {
                prompt_id: Some("polled".into()),
                command: Some("ls".into()),
                description: None,
                choices: vec!["once".into()],
            }],
            at: 12.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 5,
            items: vec![live("s1", LiveStatus::Idle, 1)],
            malformed: 0,
            at: 13.0,
        },
    );
    let slots: Vec<NeedSlot> = reg
        .queue()
        .iter()
        .map(|item| item.id.slot.clone())
        .collect();
    assert_eq!(slots, vec![NeedSlot::Prompt("exact".into())]);
}

#[test]
fn approval_reply_edge_cases() {
    let mut reg = FleetRegistry::new();
    listed(&mut reg, "s1");
    let opened = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s1", LiveStatus::Waiting, 1)],
            malformed: 0,
            at: 11.0,
        },
    );
    assert_eq!(reg.row(&key("s1")).expect("row").state, RowState::Working);
    assert!(reg.queue().is_empty());
    assert!(effects_kind(&opened).contains(&"fetch"));
    let null_id = update(
        &mut reg,
        FleetInput::Approvals {
            key: key("s1"),
            request: 3,
            approvals: vec![PolledApproval {
                prompt_id: None,
                command: Some("hidden".into()),
                description: None,
                choices: Vec::new(),
            }],
            at: 12.0,
        },
    );
    assert_eq!(reg.row(&key("s1")).expect("row").state, RowState::NeedsYou);
    assert!(matches!(reg.queue()[0].detail, NeedDetail::Waiting));
    assert!(effects_kind(&null_id).contains(&"diagnostic"));
    let seq = reg.queue()[0].seq;
    let refined = update(
        &mut reg,
        FleetInput::Approvals {
            key: key("s1"),
            request: 4,
            approvals: vec![PolledApproval {
                prompt_id: Some("a1b2".into()),
                command: Some("rm -rf ./talaria-fleet-probe".into()),
                description: None,
                choices: vec!["once".into(), "deny".into()],
            }],
            at: 13.0,
        },
    );
    assert_eq!(reg.queue()[0].seq, seq);
    assert!(!effects_kind(&refined).contains(&"needs"));
    let cleared = update(
        &mut reg,
        FleetInput::Approvals {
            key: key("s1"),
            request: 5,
            approvals: Vec::new(),
            at: 14.0,
        },
    );
    assert!(reg.queue().is_empty());
    assert_eq!(reg.row(&key("s1")).expect("row").state, RowState::Working);
    assert!(!effects_kind(&cleared).contains(&"needs"));
    let _ = update(
        &mut reg,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 6,
            items: vec![live("s1", LiveStatus::Waiting, 1)],
            malformed: 0,
            at: 15.0,
        },
    );
    let _ = update(
        &mut reg,
        FleetInput::Approvals {
            key: key("s1"),
            request: 7,
            approvals: Vec::new(),
            at: 16.0,
        },
    );
    assert!(matches!(reg.queue()[0].id.slot, NeedSlot::Waiting));
    let failed = update(
        &mut reg,
        FleetInput::ApprovalsFailed {
            key: key("s1"),
            request: 8,
            error: CallError::NoReply,
            at: 17.0,
        },
    );
    let _ = failed;
    let mut fresh = FleetRegistry::new();
    listed(&mut fresh, "s2");
    let _ = update(
        &mut fresh,
        FleetInput::Liveness {
            profile: "fixture".into(),
            request: 2,
            items: vec![live("s2", LiveStatus::Waiting, 1)],
            malformed: 0,
            at: 11.0,
        },
    );
    let _ = update(
        &mut fresh,
        FleetInput::ApprovalsFailed {
            key: key("s2"),
            request: 3,
            error: CallError::NoReply,
            at: 12.0,
        },
    );
    assert!(matches!(fresh.queue()[0].id.slot, NeedSlot::Waiting));
}

fn profile_name(index: u8) -> String {
    if index == 0 {
        "fixture".into()
    } else {
        "fixture-1".into()
    }
}

fn any_input() -> impl Strategy<Value = FleetInput> {
    (0u8..14, 0u8..2, 0u8..4, 0u8..3, 0u8..4).prop_map(
        |(kind, profile, session, prompt, status)| {
            let profile = profile_name(profile);
            let session = format!("s{session}");
            let key = RowKey {
                profile: profile.clone(),
                session_id: session.clone(),
            };
            let prompt = format!("p{prompt}");
            match kind {
                0 => FleetInput::Profiles {
                    served: vec!["fixture".into(), "fixture-1".into()],
                    parked: vec![],
                    in_scope: vec![profile],
                    at: 10.0,
                },
                1 => FleetInput::Roster {
                    profile,
                    request: 1,
                    rows: vec![RosterRow {
                        session_id: session,
                        title: String::new(),
                        source: None,
                        started_at: None,
                        message_count: 1,
                        running: true,
                        model: None,
                    }],
                    malformed: 0,
                    at: 11.0,
                },
                2 => FleetInput::Liveness {
                    profile,
                    request: 2,
                    items: vec![LiveItem {
                        session_id: session,
                        status: match status % 4 {
                            0 => LiveStatus::Idle,
                            1 => LiveStatus::Waiting,
                            2 => LiveStatus::Working,
                            _ => LiveStatus::Starting,
                        },
                        last_active: None,
                        message_count: 1,
                        model: None,
                        title: String::new(),
                    }],
                    malformed: 0,
                    at: 12.0,
                },
                3 => FleetInput::Approvals {
                    key,
                    request: 3,
                    approvals: vec![PolledApproval {
                        prompt_id: Some(prompt),
                        command: Some("ls".into()),
                        description: None,
                        choices: vec!["once".into()],
                    }],
                    at: 13.0,
                },
                4 => FleetInput::Subscribed {
                    key,
                    view: SubscribedView {
                        turn: SubscribedTurn::Idle,
                        needs: vec![SubscribedNeed {
                            slot: NeedSlot::Prompt(prompt),
                            summary: NeedSummary::Question("which".into()),
                            choices: Vec::new(),
                            execution_generation: Some(1),
                            timeout_s: None,
                        }],
                        activity: None,
                        cost_usd: None,
                        model: None,
                        title: None,
                    },
                    at: 14.0,
                },
                5 => FleetInput::Unsubscribed { key, at: 15.0 },
                6 => FleetInput::Foreground {
                    key: Some(key),
                    at: 16.0,
                },
                7 => FleetInput::Acknowledge { key, at: 17.0 },
                8 => FleetInput::ConnectionLost { at: 18.0 },
                9 => FleetInput::OutcomeProbed {
                    key,
                    outcome: Outcome::Completed,
                    found_complete: true,
                    at: 19.0,
                },
                10 => FleetInput::RosterFailed {
                    profile,
                    request: 4,
                    error: CallError::NoReply,
                    at: 20.0,
                },
                11 => FleetInput::LivenessFailed {
                    profile,
                    request: 5,
                    error: CallError::NoReply,
                    at: 21.0,
                },
                12 => FleetInput::ApprovalsFailed {
                    key,
                    request: 6,
                    error: CallError::NoReply,
                    at: 22.0,
                },
                _ => FleetInput::Profiles {
                    served: vec!["fixture".into(), "fixture-1".into()],
                    parked: vec![("fixture-2".into(), "draining".into())],
                    in_scope: vec!["fixture".into()],
                    at: 23.0,
                },
            }
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 8, ..ProptestConfig::default() })]
    #[test]
    fn queue_invariants(steps in prop::collection::vec(any_input(), 0..200)) {
        let mut reg = FleetRegistry::new();
        let mut previous = reg.version();
        let mut last = None;
        for step in steps {
            let repeated = last.as_ref().is_some_and(|prior| same_kind(prior, &step));
            let before = reg.version();
            let _ = update(&mut reg, step.clone());
            if repeated {
                let _ = update(&mut reg, step.clone());
                prop_assert!(reg.version() <= before.saturating_add(1));
            }
            let queue = reg.queue();
            let mut seen = std::collections::BTreeSet::new();
            let mut last_seq = 0;
            for (index, item) in queue.iter().enumerate() {
                prop_assert!(seen.insert(item.id.clone()));
                if index > 0 {
                    prop_assert!(item.seq > last_seq);
                }
                last_seq = item.seq;
                prop_assert!(reg.row(&item.id.key).is_some());
            }
            for row in reg.rows() {
                let has_item = reg.queue().iter().any(|item| item.id.key == row.key);
                prop_assert_eq!(row.state == RowState::NeedsYou, has_item);
                if row.foreground {
                    prop_assert!(!matches!(row.state, RowState::Done | RowState::Failed));
                }
            }
            prop_assert!(reg.version() >= previous);
            previous = reg.version();
            last = Some(step);
        }
    }
}

fn same_kind(left: &FleetInput, right: &FleetInput) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}
