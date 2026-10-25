//! The automation reducer: attach, reads, updates, modes, notes. Behavior contract: issue 236.

use serde_json::{Value, json};
use talaria_core::automation::{
    AutomationEffect, AutomationEvent, AutomationState, CommandResult, FastMode, ModeKey,
    ModeValue, ReadFailure, rows,
};

const SESSION: &str = "s1";

fn goal(status: &str) -> Value {
    json!({
        "title": "Write the release notes",
        "status": status,
        "turns_used": 3,
        "max_turns": 20,
        "contract": {"outcome": "", "verification": "", "constraints": "", "boundaries": "", "stop_when": ""},
        "subgoals": ["draft", "review"],
        "gates": [{"command": "make check", "timeout_seconds": 300, "max_retries": 2, "attempts": 1}],
    })
}

fn loop_() -> Value {
    json!({
        "prompt": "check the build",
        "status": "active",
        "mode": "interval",
        "interval_seconds": 2700,
        "current_delay": 0,
        "times": 0,
        "until": "",
        "max_ticks": 0,
        "ticks_fired": 0,
        "created_at": 1790000000,
        "last_fired_at": 0,
        "next_due_at": 1790002700,
        "awaiting_response": false,
        "deferred_by_goal": false
    })
}

fn heartbeat() -> Value {
    json!({
        "prompt": "ping the team",
        "status": "active",
        "interval_seconds": 3600,
        "created_at": 1789997120,
        "last_fired_at": 0,
        "fire_count": 4
    })
}

fn full_snapshot() -> Value {
    json!({
        "goal": goal("active"), "loop": loop_(), "heartbeat": heartbeat(),
        "revision": "r1", "updated_at": 1790000000
    })
}

fn attached(session: &str) -> AutomationEvent {
    AutomationEvent::Attached {
        session_id: session.to_owned(),
    }
}

fn updated(session: &str, raw: Value) -> AutomationEvent {
    AutomationEvent::ControlUpdated {
        session_id: session.to_owned(),
        raw,
    }
}

/// The three read tokens of an attach's effects, in effect order.
fn attach_tokens(effects: &[AutomationEffect]) -> (u64, u64, u64) {
    let mut tokens = Vec::new();
    for effect in effects {
        match effect {
            AutomationEffect::ReadControl { token, .. }
            | AutomationEffect::ReadMode { token, .. } => tokens.push(*token),
            AutomationEffect::Notice { .. } | AutomationEffect::Redraw => {}
        }
    }
    assert_eq!(tokens.len(), 3, "one control and two mode reads");
    (tokens[0], tokens[1], tokens[2])
}

fn mode_token(effects: &[AutomationEffect], key: ModeKey) -> u64 {
    effects
        .iter()
        .find_map(|effect| match effect {
            AutomationEffect::ReadMode {
                key: found, token, ..
            } if *found == key => Some(*token),
            _ => None,
        })
        .expect("a mode read")
}

#[test]
fn attach_reads_control_and_both_modes() {
    let mut state = AutomationState::default();
    let effects = talaria_core::automation::update(&mut state, attached(SESSION));
    assert_eq!(effects.len(), 4);
    assert!(matches!(
        effects[0],
        AutomationEffect::ReadControl { ref session_id, .. } if session_id == SESSION
    ));
    assert!(matches!(
        effects[1],
        AutomationEffect::ReadMode { ref session_id, key: ModeKey::Yolo, .. }
        if session_id == SESSION
    ));
    assert!(matches!(
        effects[2],
        AutomationEffect::ReadMode { ref session_id, key: ModeKey::Fast, .. }
        if session_id == SESSION
    ));
    assert_eq!(effects[3], AutomationEffect::Redraw);
    assert_eq!(state.modes(), (ModeValue::NotRead, ModeValue::NotRead));

    // Known values reset to NotRead on the next attach.
    let (control, yolo, fast) = attach_tokens(&effects);
    talaria_core::automation::update(
        &mut state,
        AutomationEvent::ModeRead {
            session_id: SESSION.to_owned(),
            key: ModeKey::Yolo,
            token: yolo,
            result: Ok(Some("1".to_owned())),
        },
    );
    talaria_core::automation::update(
        &mut state,
        AutomationEvent::ModeRead {
            session_id: SESSION.to_owned(),
            key: ModeKey::Fast,
            token: fast,
            result: Ok(Some("fast".to_owned())),
        },
    );
    assert_eq!(
        state.modes(),
        (ModeValue::Known(true), ModeValue::Known(FastMode::Fast))
    );
    let effects = talaria_core::automation::update(&mut state, attached(SESSION));
    assert_eq!(state.modes(), (ModeValue::NotRead, ModeValue::NotRead));
    let (control2, yolo2, fast2) = attach_tokens(&effects);
    assert!(control < yolo && yolo < fast, "tokens rise in effect order");
    for token in [control2, yolo2, fast2] {
        assert!(
            token > control && token > yolo && token > fast,
            "tokens rise across attaches"
        );
    }
}

#[test]
fn update_overtakes_pending_read() {
    let mut state = AutomationState::default();
    let effects = talaria_core::automation::update(&mut state, attached(SESSION));
    let (control, _, _) = attach_tokens(&effects);

    let mut older = full_snapshot();
    older["goal"]["title"] = json!("The older title");
    older["revision"] = json!("r0");
    talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
    let before = rows(&state, NOW_MS);
    assert!(
        before
            .iter()
            .any(|row| row.tail == "Write the release notes")
    );

    let effects = talaria_core::automation::update(
        &mut state,
        AutomationEvent::ControlRead {
            token: control,
            result: Ok(talaria_core::automation::ControlView::decode(&older)
                .expect("the older view decodes")),
        },
    );
    assert!(effects.is_empty(), "the late read is ignored");
    assert_eq!(rows(&state, NOW_MS), before, "the update's view stays");

    // A reply to a read sent before the latest attach is ignored too.
    talaria_core::automation::update(&mut state, attached(SESSION));
    let effects = talaria_core::automation::update(
        &mut state,
        AutomationEvent::ControlRead {
            token: control,
            result: Ok(
                talaria_core::automation::ControlView::decode(&full_snapshot())
                    .expect("the view decodes"),
            ),
        },
    );
    assert!(effects.is_empty(), "the pre-attach read is ignored");
}

const NOW_MS: u64 = 1790000000000;

#[test]
fn same_revision_does_not_redraw() {
    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    let effects = talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
    assert_eq!(effects, vec![AutomationEffect::Redraw]);
    let effects = talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
    assert!(effects.is_empty(), "the same revision draws nothing");
}

#[test]
fn read_failures_hide_rows_without_notices() {
    for failure in [
        ReadFailure::Refused("not_found".to_owned()),
        ReadFailure::MethodMissing,
        ReadFailure::TimedOut,
        ReadFailure::Lost,
        ReadFailure::BadResult,
    ] {
        let mut state = AutomationState::default();
        let effects = talaria_core::automation::update(&mut state, attached(SESSION));
        let yolo = mode_token(&effects, ModeKey::Yolo);
        talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
        assert_eq!(rows(&state, NOW_MS).len(), 3);

        talaria_core::automation::update(
            &mut state,
            AutomationEvent::ModeRead {
                session_id: SESSION.to_owned(),
                key: ModeKey::Yolo,
                token: yolo,
                result: Ok(Some("1".to_owned())),
            },
        );
        let effects = talaria_core::automation::update(&mut state, attached(SESSION));
        let (control, yolo, _) = attach_tokens(&effects);
        talaria_core::automation::update(
            &mut state,
            AutomationEvent::ModeRead {
                session_id: SESSION.to_owned(),
                key: ModeKey::Yolo,
                token: yolo,
                result: Ok(Some("1".to_owned())),
            },
        );
        let effects = talaria_core::automation::update(
            &mut state,
            AutomationEvent::ControlRead {
                token: control,
                result: Err(failure.clone()),
            },
        );
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AutomationEffect::Notice { .. })),
            "{failure:?}: no notice"
        );
        let shown = rows(&state, NOW_MS);
        assert_eq!(shown.len(), 1, "{failure:?}: only the modes row");
        assert_eq!(shown[0].label, "⚠ YOLO");

        talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
        assert_eq!(
            rows(&state, NOW_MS).len(),
            4,
            "{failure:?}: an update fills the rows"
        );
    }
}

#[test]
fn other_session_and_bad_payload_ignored() {
    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
    let before = rows(&state, NOW_MS);
    assert_eq!(before.len(), 3);

    let effects = talaria_core::automation::update(&mut state, updated("s2", full_snapshot()));
    assert!(effects.is_empty(), "another session is ignored");
    assert_eq!(rows(&state, NOW_MS), before);

    let mut bad = full_snapshot();
    bad["loop"]["next_due_at"] = json!(-5);
    let effects = talaria_core::automation::update(&mut state, updated(SESSION, bad));
    assert!(effects.is_empty(), "a bad payload is ignored");
    assert_eq!(rows(&state, NOW_MS), before);
}

#[test]
fn goal_status_note_becomes_one_notice() {
    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    for kind in ["goal", "loop", "heartbeat"] {
        let effects = talaria_core::automation::update(
            &mut state,
            AutomationEvent::StatusNote {
                session_id: SESSION.to_owned(),
                kind: kind.to_owned(),
                text: "✓ Goal achieved: notes shipped".to_owned(),
            },
        );
        assert_eq!(
            effects,
            vec![AutomationEffect::Notice {
                text: "✓ Goal achieved: notes shipped".to_owned()
            }],
            "{kind} notes once"
        );
    }
    for (kind, text) in [
        ("compacting", "compacting now"),
        ("status", "just a status"),
        ("goal", "   "),
    ] {
        let effects = talaria_core::automation::update(
            &mut state,
            AutomationEvent::StatusNote {
                session_id: SESSION.to_owned(),
                kind: kind.to_owned(),
                text: text.to_owned(),
            },
        );
        assert!(effects.is_empty(), "{kind:?} {text:?}: no notice");
    }
    let effects = talaria_core::automation::update(
        &mut state,
        AutomationEvent::StatusNote {
            session_id: "s2".to_owned(),
            kind: "goal".to_owned(),
            text: "✓ Goal achieved: elsewhere".to_owned(),
        },
    );
    assert!(effects.is_empty(), "another session: no notice");
}

#[test]
fn yolo_and_fast_read_back_unless_not_run() {
    for result in [
        CommandResult::Output,
        CommandResult::Submit,
        CommandResult::Unknown,
    ] {
        let mut state = AutomationState::default();
        talaria_core::automation::update(&mut state, attached(SESSION));
        let effects = talaria_core::automation::update(
            &mut state,
            AutomationEvent::CommandFinished {
                session_id: SESSION.to_owned(),
                canonical: "yolo".to_owned(),
                result,
            },
        );
        assert_eq!(effects.len(), 1, "{result:?} reads back");
        assert!(matches!(
            effects[0],
            AutomationEffect::ReadMode { key: ModeKey::Yolo, ref session_id, .. }
            if session_id == SESSION
        ));
    }

    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    let effects = talaria_core::automation::update(
        &mut state,
        AutomationEvent::CommandFinished {
            session_id: SESSION.to_owned(),
            canonical: "yolo".to_owned(),
            result: CommandResult::NotRun,
        },
    );
    assert!(effects.is_empty(), "NotRun reads nothing");

    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    let effects = talaria_core::automation::update(
        &mut state,
        AutomationEvent::CommandFinished {
            session_id: SESSION.to_owned(),
            canonical: "fast".to_owned(),
            result: CommandResult::Output,
        },
    );
    assert!(matches!(
        effects.as_slice(),
        [AutomationEffect::ReadMode {
            key: ModeKey::Fast,
            ..
        }]
    ));

    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    let effects = talaria_core::automation::update(
        &mut state,
        AutomationEvent::CommandFinished {
            session_id: "s2".to_owned(),
            canonical: "yolo".to_owned(),
            result: CommandResult::Output,
        },
    );
    assert!(effects.is_empty(), "another session reads nothing");
}

#[test]
fn mode_read_failure_after_command_is_unknown() {
    // After a command, every failure and a missing value mean Unknown.
    for result in [
        Err(ReadFailure::Refused("permission_denied".to_owned())),
        Err(ReadFailure::MethodMissing),
        Err(ReadFailure::TimedOut),
        Err(ReadFailure::Lost),
        Err(ReadFailure::BadResult),
        Ok::<Option<String>, ReadFailure>(None),
    ] {
        let mut state = AutomationState::default();
        talaria_core::automation::update(&mut state, attached(SESSION));
        let effects = talaria_core::automation::update(
            &mut state,
            AutomationEvent::CommandFinished {
                session_id: SESSION.to_owned(),
                canonical: "yolo".to_owned(),
                result: CommandResult::Output,
            },
        );
        let token = mode_token(&effects, ModeKey::Yolo);
        talaria_core::automation::update(
            &mut state,
            AutomationEvent::ModeRead {
                session_id: SESSION.to_owned(),
                key: ModeKey::Yolo,
                token,
                result: result.clone(),
            },
        );
        assert_eq!(state.modes().0, ModeValue::Unknown, "{result:?} is unknown");
        let rows = rows(&state, NOW_MS);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].detail, "state unknown: Hermes did not answer");
    }

    // After an attach, transport failures mean Unknown but a missing or
    // refused method means this Hermes has no value to show.
    for (result, known) in [
        (Err(ReadFailure::TimedOut), false),
        (Err(ReadFailure::Lost), false),
        (Err(ReadFailure::BadResult), false),
        (Ok(Some("maybe".to_owned())), false),
        (Err(ReadFailure::MethodMissing), true),
        (
            Err(ReadFailure::Refused("permission_denied".to_owned())),
            true,
        ),
    ] {
        let mut state = AutomationState::default();
        let effects = talaria_core::automation::update(&mut state, attached(SESSION));
        let token = mode_token(&effects, ModeKey::Yolo);
        talaria_core::automation::update(
            &mut state,
            AutomationEvent::ModeRead {
                session_id: SESSION.to_owned(),
                key: ModeKey::Yolo,
                token,
                result: result.clone(),
            },
        );
        if known {
            assert_eq!(
                state.modes().0,
                ModeValue::NotRead,
                "{result:?} stays unread"
            );
            assert!(rows(&state, NOW_MS).is_empty());
        } else {
            assert_eq!(state.modes().0, ModeValue::Unknown, "{result:?} is unknown");
        }
    }

    // Two quick commands: only the newer read's result applies.
    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    let first = talaria_core::automation::update(
        &mut state,
        AutomationEvent::CommandFinished {
            session_id: SESSION.to_owned(),
            canonical: "yolo".to_owned(),
            result: CommandResult::Output,
        },
    );
    let second = talaria_core::automation::update(
        &mut state,
        AutomationEvent::CommandFinished {
            session_id: SESSION.to_owned(),
            canonical: "yolo".to_owned(),
            result: CommandResult::Output,
        },
    );
    let stale = mode_token(&first, ModeKey::Yolo);
    let fresh = mode_token(&second, ModeKey::Yolo);
    assert!(fresh > stale);
    let effects = talaria_core::automation::update(
        &mut state,
        AutomationEvent::ModeRead {
            session_id: SESSION.to_owned(),
            key: ModeKey::Yolo,
            token: stale,
            result: Ok(Some("1".to_owned())),
        },
    );
    assert!(effects.is_empty(), "the older read is dropped");
    assert_eq!(state.modes().0, ModeValue::NotRead);

    // A read for the previous session after a new attach is dropped.
    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    let effects = talaria_core::automation::update(
        &mut state,
        AutomationEvent::CommandFinished {
            session_id: SESSION.to_owned(),
            canonical: "yolo".to_owned(),
            result: CommandResult::Output,
        },
    );
    let token = mode_token(&effects, ModeKey::Yolo);
    talaria_core::automation::update(&mut state, attached("s2"));
    let effects = talaria_core::automation::update(
        &mut state,
        AutomationEvent::ModeRead {
            session_id: SESSION.to_owned(),
            key: ModeKey::Yolo,
            token,
            result: Ok(Some("1".to_owned())),
        },
    );
    assert!(effects.is_empty(), "the old session's read is dropped");
}

#[test]
fn plan_note_once_per_session() {
    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    let plan = |session: &str, result: CommandResult| AutomationEvent::CommandFinished {
        session_id: session.to_owned(),
        canonical: "plan".to_owned(),
        result,
    };
    let effects =
        talaria_core::automation::update(&mut state, plan(SESSION, CommandResult::Submit));
    assert_eq!(
        effects,
        vec![AutomationEffect::Notice {
            text: talaria_core::automation::PLAN_MODE_NOTE.to_owned()
        }]
    );
    let effects =
        talaria_core::automation::update(&mut state, plan(SESSION, CommandResult::Submit));
    assert!(effects.is_empty(), "the second submit notes nothing");

    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    let effects =
        talaria_core::automation::update(&mut state, plan(SESSION, CommandResult::Output));
    assert!(effects.is_empty(), "an Output notes nothing");
    let effects =
        talaria_core::automation::update(&mut state, plan(SESSION, CommandResult::Submit));
    assert_eq!(effects.len(), 1, "the next Submit still notes");

    talaria_core::automation::update(&mut state, attached(SESSION));
    let effects =
        talaria_core::automation::update(&mut state, plan(SESSION, CommandResult::Submit));
    assert!(effects.is_empty(), "a re-attach notes nothing");

    talaria_core::automation::update(&mut state, attached("s2"));
    let effects = talaria_core::automation::update(&mut state, plan("s2", CommandResult::Submit));
    assert_eq!(effects.len(), 1, "another session notes");
}

#[test]
fn detach_clears_everything() {
    let mut state = AutomationState::default();
    let effects = talaria_core::automation::update(&mut state, attached(SESSION));
    let yolo = mode_token(&effects, ModeKey::Yolo);
    talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
    talaria_core::automation::update(
        &mut state,
        AutomationEvent::ModeRead {
            session_id: SESSION.to_owned(),
            key: ModeKey::Yolo,
            token: yolo,
            result: Ok(Some("1".to_owned())),
        },
    );
    assert_eq!(rows(&state, NOW_MS).len(), 4);
    let effects = talaria_core::automation::update(&mut state, AutomationEvent::Detached);
    assert_eq!(effects, vec![AutomationEffect::Redraw]);
    assert!(rows(&state, NOW_MS).is_empty());
    assert_eq!(state.modes(), (ModeValue::NotRead, ModeValue::NotRead));

    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    let effects = talaria_core::automation::update(&mut state, AutomationEvent::Detached);
    assert!(effects.is_empty(), "nothing visible: no redraw");
}
