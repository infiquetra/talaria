use proptest::prelude::*;
use std::time::Duration;
use talaria_core::doctor::*;

const OLD: &str = "0123456789abcdef0123456789abcdef";
const NEW: &str = "fedcba9876543210fedcba9876543210";

fn facts(gateway: GatewayFacts) -> DoctorFacts {
    DoctorFacts {
        talaria_version: "1.0.0".into(),
        pin: Pin {
            tag: "v0.22.4".into(),
            commit: "abcdef".into(),
        },
        profile: "fixture".into(),
        gateway,
        identity: Some(IdentityFacts {
            code_version: None,
            code_sha: None,
            capabilities: vec![],
            served: vec!["default".into(), "fixture".into()],
            parked: vec![],
        }),
        attach: AttachFacts::NotTried,
        status: StatusFacts::NotTried,
    }
}
fn ready() -> DoctorFacts {
    facts(GatewayFacts::Ready {
        instance_id: OLD.into(),
        epoch: 2,
        launch: None,
    })
}
fn asking() -> RestartState {
    RestartState::Asking {
        old_instance: OLD.into(),
        profiles: vec!["default".into(), "fixture".into()],
    }
}
fn confirming(selected: bool) -> RestartState {
    RestartState::Confirming {
        old_instance: OLD.into(),
        profiles: vec!["default".into(), "fixture".into()],
        work: Some(WorkSummary { chat: 2, other: 1 }),
        restart_selected: selected,
    }
}
fn requesting() -> RestartState {
    RestartState::Requesting {
        old_instance: OLD.into(),
    }
}
fn waiting(elapsed: Duration) -> RestartState {
    RestartState::Waiting {
        old_instance: OLD.into(),
        elapsed,
        unknown_request: false,
    }
}

#[test]
fn asks_then_confirms() {
    let input = ask_input(&ready());
    assert!(
        matches!(&input, RestartInput::Ask { gateway_ready: true, instance_id, profiles } if instance_id == OLD && profiles == &["default", "fixture"])
    );
    let (state, effects) = restart_update(RestartState::Idle, input);
    assert!(
        matches!(&state, RestartState::Asking { old_instance, profiles } if old_instance == OLD && profiles == &["default", "fixture"])
    );
    assert!(matches!(
        effects.as_slice(),
        [RestartEffect::QueryActiveWork]
    ));
    let (state, effects) = restart_update(
        state,
        RestartInput::ActiveWork(Ok(WorkSummary { chat: 2, other: 1 })),
    );
    assert!(effects.is_empty());
    assert!(
        matches!(&state, RestartState::Confirming { old_instance, profiles, work: Some(WorkSummary { chat: 2, other: 1 }), restart_selected: false } if old_instance == OLD && profiles == &["default", "fixture"])
    );
    for state in [asking(), state] {
        let (state, effects) = restart_update(state, RestartInput::Cancel);
        assert!(matches!(
            state,
            RestartState::Done(RestartOutcome::Declined)
        ));
        assert!(effects.is_empty());
    }
}

#[test]
fn confirm_posts_once() {
    let (state, effects) = restart_update(confirming(false), RestartInput::Confirm);
    assert!(matches!(&state, RestartState::Requesting { old_instance } if old_instance == OLD));
    assert!(
        matches!(effects.as_slice(), [RestartEffect::Post { old_instance }] if old_instance == OLD)
    );
    let (state, effects) = restart_update(state, RestartInput::Confirm);
    assert!(matches!(&state, RestartState::Requesting { old_instance } if old_instance == OLD));
    assert!(effects.is_empty());
    let (changed, effects) = restart_update(state, RestartInput::Posted(PostResult::Changed));
    assert!(matches!(
        changed,
        RestartState::Done(RestartOutcome::Changed)
    ));
    assert!(
        matches!(effects.as_slice(), [RestartEffect::Notice(text)] if text.contains("nothing was sent"))
    );
    let (refused, effects) = restart_update(
        requesting(),
        RestartInput::Posted(PostResult::Refused {
            http_status: Some(401),
            detail: "denied".into(),
        }),
    );
    assert!(
        matches!(refused, RestartState::Done(RestartOutcome::Refused { http_status: Some(401), ref detail }) if detail == "denied")
    );
    assert!(
        matches!(effects.as_slice(), [RestartEffect::Notice(text)] if text.contains("401") && text.contains("denied"))
    );
}

#[test]
fn waits_for_a_new_instance() {
    for (posted, unknown) in [(PostResult::Accepted, false), (PostResult::Unknown, true)] {
        let (state, effects) = restart_update(requesting(), RestartInput::Posted(posted));
        assert!(
            matches!(&state, RestartState::Waiting { old_instance, elapsed, unknown_request } if old_instance == OLD && *elapsed == Duration::ZERO && *unknown_request == unknown)
        );
        assert!(
            matches!(effects.as_slice(), [RestartEffect::ProbeAfter(d)] if *d == Duration::from_secs(1))
        );
        let (mut state, effects) =
            restart_update(state, RestartInput::Tick(Duration::from_millis(3200)));
        assert!(effects.is_empty());
        for probe in [
            ProbeResult::Ready {
                instance_id: OLD.into(),
            },
            ProbeResult::NotAnswering,
            ProbeResult::NotReady,
        ] {
            let (next, effects) = restart_update(state, RestartInput::Probe(probe));
            assert!(
                matches!(&next, RestartState::Waiting { elapsed, unknown_request, .. } if *elapsed == Duration::from_millis(3200) && *unknown_request == unknown)
            );
            assert!(
                matches!(effects.as_slice(), [RestartEffect::ProbeAfter(d)] if *d == Duration::from_secs(1))
            );
            state = next;
        }
        let (state, _) = restart_update(state, RestartInput::Tick(Duration::from_millis(11400)));
        let (state, effects) = restart_update(
            state,
            RestartInput::Probe(ProbeResult::Ready {
                instance_id: NEW.into(),
            }),
        );
        assert!(
            matches!(state, RestartState::Done(RestartOutcome::Restarted { seconds: 12, ref old_instance, ref new_instance }) if old_instance == OLD && new_instance == NEW)
        );
        assert!(
            matches!(effects.as_slice(), [RestartEffect::RerunChecks, RestartEffect::Notice(text)] if text.contains("12 s"))
        );
    }
}

#[test]
fn deadline_and_refusals() {
    for (elapsed, probe, wanted) in [
        (
            Duration::from_secs(90),
            ProbeResult::Ready {
                instance_id: OLD.into(),
            },
            "same",
        ),
        (Duration::from_secs(91), ProbeResult::NotAnswering, "gone"),
        (Duration::from_secs(90), ProbeResult::NotReady, "gone"),
        (
            Duration::from_secs(90),
            ProbeResult::Ready {
                instance_id: NEW.into(),
            },
            "new",
        ),
    ] {
        let (state, effects) = restart_update(waiting(Duration::ZERO), RestartInput::Tick(elapsed));
        assert!(matches!(state, RestartState::Waiting { .. }));
        assert!(effects.is_empty());
        let (state, effects) = restart_update(state, RestartInput::Probe(probe));
        match wanted {
            "same" => assert!(matches!(
                state,
                RestartState::Done(RestartOutcome::DidNotRestart)
            )),
            "gone" => assert!(matches!(state, RestartState::Done(RestartOutcome::NotBack))),
            _ => assert!(matches!(
                state,
                RestartState::Done(RestartOutcome::Restarted { seconds: 90, .. })
            )),
        }
        assert!(matches!(
            effects.as_slice(),
            [RestartEffect::RerunChecks, RestartEffect::Notice(_)]
        ));
    }
    let (state, effects) = restart_update(
        requesting(),
        RestartInput::Posted(PostResult::Refused {
            http_status: Some(503),
            detail: "unavailable".into(),
        }),
    );
    assert!(
        matches!(state, RestartState::Done(RestartOutcome::Refused { http_status: Some(503), ref detail }) if detail == "unavailable")
    );
    assert!(matches!(effects.as_slice(), [RestartEffect::Notice(_)]));
}

#[test]
fn not_offered_when_not_ready() {
    for gateway in [
        GatewayFacts::RestartWait {
            instance_id: OLD.into(),
            epoch: 1,
        },
        GatewayFacts::NotReady {
            state: "starting".into(),
            drain_reason: None,
        },
        GatewayFacts::Unreachable {
            message: "offline".into(),
            next_step: "retry".into(),
        },
        GatewayFacts::Incompatible,
    ] {
        let input = ask_input(&facts(gateway));
        assert!(
            matches!(input, RestartInput::Ask { gateway_ready: false, ref instance_id, .. } if instance_id.is_empty())
        );
        let (state, effects) = restart_update(RestartState::Idle, input);
        assert!(matches!(
            state,
            RestartState::Done(RestartOutcome::NotReady)
        ));
        assert!(effects.is_empty());
    }
}

fn input(n: u8) -> RestartInput {
    match n % 9 {
        0 => RestartInput::Ask {
            gateway_ready: true,
            instance_id: OLD.into(),
            profiles: vec!["fixture".into()],
        },
        1 => RestartInput::ActiveWork(Ok(WorkSummary { chat: 1, other: 0 })),
        2 => RestartInput::Toggle,
        3 => RestartInput::Confirm,
        4 => RestartInput::Cancel,
        5 => RestartInput::Posted(PostResult::Accepted),
        6 => RestartInput::Posted(PostResult::Unknown),
        7 => RestartInput::Tick(Duration::from_secs(90)),
        _ => RestartInput::Probe(ProbeResult::Ready {
            instance_id: NEW.into(),
        }),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn never_posts_without_confirm(start in 0u8..6, events in proptest::collection::vec(any::<u8>(), 0..51)) {
        let mut state = match start { 0 => RestartState::Idle, 1 => asking(), 2 => confirming(false), 3 => requesting(), 4 => waiting(Duration::ZERO), _ => RestartState::Done(RestartOutcome::Declined) };
        let mut allowed = 0usize;
        let mut posts = 0usize;
        for n in events {
            let event = input(n);
            let permitted = matches!(&state, RestartState::Confirming { .. }) && matches!(&event, RestartInput::Confirm);
            if permitted { allowed += 1; }
            let (next, effects) = restart_update(state, event);
            for effect in effects { if matches!(effect, RestartEffect::Post { .. }) { prop_assert!(permitted); posts += 1; } }
            prop_assert!(posts <= allowed);
            state = next;
        }
        let (state, _) = restart_update(RestartState::Idle, RestartInput::Ask { gateway_ready: true, instance_id: OLD.into(), profiles: vec!["fixture".into()] });
        let (state, _) = restart_update(state, RestartInput::ActiveWork(Ok(WorkSummary { chat: 1, other: 0 })));
        let (_, effects) = restart_update(state, RestartInput::Confirm);
        prop_assert_eq!(effects.iter().filter(|e| matches!(e, RestartEffect::Post { .. })).count(), 1);
    }
}

#[test]
fn view_keys() {
    let mut view = DoctorView::new();
    assert_eq!(view.selected, 0);
    assert!(view.running);
    assert!(doctor_view_update(&mut view, &RestartState::Idle, DoctorKey::Restart).is_empty());
    assert!(doctor_view_update(&mut view, &RestartState::Idle, DoctorKey::Rerun).is_empty());
    view.running = false;
    view.facts = Some(ready());
    view.report = Some(Report {
        profile: "fixture".into(),
        checks: [
            CheckId::Talaria,
            CheckId::Gateway,
            CheckId::HermesVersion,
            CheckId::Capabilities,
            CheckId::Profiles,
            CheckId::Attach,
            CheckId::RuntimeHealth,
        ]
        .into_iter()
        .map(|id| Check {
            id,
            severity: Severity::Ok,
            summary: String::new(),
            details: vec![],
            fix: None,
        })
        .collect(),
    });
    assert!(matches!(
        doctor_view_update(&mut view, &RestartState::Idle, DoctorKey::Rerun).as_slice(),
        [DoctorEffect::Gather]
    ));
    assert!(matches!(
        doctor_view_update(&mut view, &RestartState::Idle, DoctorKey::Restart).as_slice(),
        [DoctorEffect::AskRestart]
    ));
    assert!(matches!(
        doctor_view_update(&mut view, &RestartState::Idle, DoctorKey::Close).as_slice(),
        [DoctorEffect::Close]
    ));
    let done = RestartState::Done(RestartOutcome::Declined);
    assert!(matches!(
        doctor_view_update(&mut view, &done, DoctorKey::Rerun).as_slice(),
        [DoctorEffect::Gather]
    ));
    assert!(matches!(
        doctor_view_update(&mut view, &done, DoctorKey::Restart).as_slice(),
        [DoctorEffect::AskRestart]
    ));
    assert!(matches!(
        doctor_view_update(&mut view, &done, DoctorKey::Close).as_slice(),
        [DoctorEffect::Close]
    ));
    doctor_view_update(&mut view, &RestartState::Idle, DoctorKey::Up);
    assert_eq!(view.selected, 0);
    doctor_view_update(&mut view, &RestartState::Idle, DoctorKey::Down);
    assert_eq!(view.selected, 1);
    doctor_view_update(&mut view, &RestartState::Idle, DoctorKey::Toggle);
    assert!(view.expanded.contains(&CheckId::Gateway));
    doctor_view_update(&mut view, &RestartState::Idle, DoctorKey::Toggle);
    assert!(!view.expanded.contains(&CheckId::Gateway));
    for key in [DoctorKey::Close, DoctorKey::No] {
        assert!(matches!(
            doctor_view_update(&mut view, &asking(), key).as_slice(),
            [DoctorEffect::Restart(RestartInput::Cancel)]
        ));
    }
    for key in [
        DoctorKey::Up,
        DoctorKey::Restart,
        DoctorKey::Rerun,
        DoctorKey::Yes,
    ] {
        assert!(doctor_view_update(&mut view, &asking(), key).is_empty());
    }
    for key in [DoctorKey::Tab, DoctorKey::Left, DoctorKey::Right] {
        assert!(matches!(
            doctor_view_update(&mut view, &confirming(false), key).as_slice(),
            [DoctorEffect::Restart(RestartInput::Toggle)]
        ));
    }
    assert!(matches!(
        doctor_view_update(&mut view, &confirming(false), DoctorKey::Yes).as_slice(),
        [DoctorEffect::Restart(RestartInput::Confirm)]
    ));
    for key in [DoctorKey::No, DoctorKey::Close] {
        assert!(matches!(
            doctor_view_update(&mut view, &confirming(false), key).as_slice(),
            [DoctorEffect::Restart(RestartInput::Cancel)]
        ));
    }
    assert!(matches!(
        doctor_view_update(&mut view, &confirming(false), DoctorKey::Toggle).as_slice(),
        [DoctorEffect::Restart(RestartInput::Cancel)]
    ));
    assert!(matches!(
        doctor_view_update(&mut view, &confirming(true), DoctorKey::Toggle).as_slice(),
        [DoctorEffect::Restart(RestartInput::Confirm)]
    ));
    for state in [requesting(), waiting(Duration::ZERO)] {
        assert!(matches!(
            doctor_view_update(&mut view, &state, DoctorKey::Close).as_slice(),
            [DoctorEffect::Close]
        ));
        for key in [
            DoctorKey::Restart,
            DoctorKey::Rerun,
            DoctorKey::Up,
            DoctorKey::Yes,
        ] {
            assert!(doctor_view_update(&mut view, &state, key).is_empty());
        }
    }
}
