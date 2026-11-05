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

fn goal_completion_snapshot(status: &str, revision: &str) -> Value {
    let mut snapshot = full_snapshot();
    snapshot["revision"] = json!(revision);
    snapshot["goal"]["status"] = json!(status);
    snapshot["goal"]["last_verdict"] = json!("done");
    snapshot["goal"]["last_reason"] = json!("Release notes reviewed and shipped");
    snapshot
}

fn goal_completion_notices(effects: &[AutomationEffect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AutomationEffect::Notice { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn goal_completion_note() -> AutomationEvent {
    AutomationEvent::StatusNote {
        session_id: SESSION.to_owned(),
        kind: "goal".to_owned(),
        text: "✓ Goal achieved: Release notes reviewed and shipped".to_owned(),
    }
}

#[test]
fn goal_completion_snapshot_emits_verdict_reason_and_turns_once() {
    for initial_status in ["active", "paused"] {
        let mut state = AutomationState::default();
        talaria_core::automation::update(&mut state, attached(SESSION));
        let mut initial = full_snapshot();
        initial["goal"]["status"] = json!(initial_status);
        talaria_core::automation::update(&mut state, updated(SESSION, initial));

        let done = goal_completion_snapshot("done", "r2");
        let effects = talaria_core::automation::update(&mut state, updated(SESSION, done.clone()));
        let notices = goal_completion_notices(&effects);
        assert_eq!(
            notices.len(),
            1,
            "{initial_status} to done emits one notice"
        );
        for part in ["done", "Release notes reviewed and shipped", "3", "20"] {
            assert!(
                notices[0].contains(part),
                "notice must contain {part:?}: {notices:?}"
            );
        }
        assert!(
            rows(&state, NOW_MS)
                .iter()
                .all(|row| !row.label.contains("goal")),
            "the completed goal stays hidden"
        );
        let effects = talaria_core::automation::update(&mut state, updated(SESSION, done));
        assert!(
            goal_completion_notices(&effects).is_empty(),
            "replayed snapshot is silent"
        );
    }
}

#[test]
fn goal_completion_snapshot_before_status_note_does_not_duplicate() {
    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
    let effects = talaria_core::automation::update(
        &mut state,
        updated(SESSION, goal_completion_snapshot("done", "r2")),
    );
    assert_eq!(
        goal_completion_notices(&effects).len(),
        1,
        "snapshot emits immediately"
    );
    let effects = talaria_core::automation::update(&mut state, goal_completion_note());
    assert!(
        goal_completion_notices(&effects).is_empty(),
        "late Hermes note is a duplicate"
    );
}

#[test]
fn goal_completion_status_note_before_snapshot_does_not_duplicate() {
    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
    let effects = talaria_core::automation::update(&mut state, goal_completion_note());
    assert_eq!(
        goal_completion_notices(&effects).len(),
        1,
        "Hermes note emits immediately"
    );
    let effects = talaria_core::automation::update(
        &mut state,
        updated(SESSION, goal_completion_snapshot("done", "r2")),
    );
    assert!(
        goal_completion_notices(&effects).is_empty(),
        "later snapshot is a duplicate"
    );
}

#[test]
fn goal_completion_already_done_at_attach_is_silent() {
    for use_read in [false, true] {
        let mut state = AutomationState::default();
        let effects = talaria_core::automation::update(&mut state, attached(SESSION));
        let (token, _, _) = attach_tokens(&effects);
        let done = goal_completion_snapshot("done", "r2");
        let event = if use_read {
            AutomationEvent::ControlRead {
                token,
                result: Ok(talaria_core::automation::ControlView::decode(&done).unwrap()),
            }
        } else {
            updated(SESSION, done)
        };
        let effects = talaria_core::automation::update(&mut state, event);
        assert!(
            goal_completion_notices(&effects).is_empty(),
            "attach must not announce old completion"
        );
    }
}

#[test]
fn goal_completion_without_verdict_or_reason_names_final_status() {
    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
    let mut done = full_snapshot();
    done["goal"]["status"] = json!("done");
    done["revision"] = json!("r2");
    let effects = talaria_core::automation::update(&mut state, updated(SESSION, done));
    let notices = goal_completion_notices(&effects);
    assert_eq!(
        notices.len(),
        1,
        "missing judge fields still emits one notice"
    );
    assert!(
        notices[0].contains("done"),
        "fallback names final status: {notices:?}"
    );
}

#[test]
fn goal_completion_note_before_initial_read_does_not_duplicate() {
    let mut state = AutomationState::default();
    let effects = talaria_core::automation::update(&mut state, attached(SESSION));
    let (token, _, _) = attach_tokens(&effects);
    let effects = talaria_core::automation::update(&mut state, goal_completion_note());
    assert_eq!(goal_completion_notices(&effects).len(), 1);
    talaria_core::automation::update(
        &mut state,
        AutomationEvent::ControlRead {
            token,
            result: Ok(talaria_core::automation::ControlView::decode(&full_snapshot()).unwrap()),
        },
    );
    let effects = talaria_core::automation::update(
        &mut state,
        updated(SESSION, goal_completion_snapshot("done", "r2")),
    );
    assert!(goal_completion_notices(&effects).is_empty());
}

#[test]
fn goal_completion_tracking_preserves_other_goal_notes() {
    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
    for text in [
        "↻ Continuing toward goal (3/20): still reviewing",
        "⏸ Goal paused: waiting for input",
    ] {
        let effects = talaria_core::automation::update(
            &mut state,
            AutomationEvent::StatusNote {
                session_id: SESSION.to_owned(),
                kind: "goal".to_owned(),
                text: text.to_owned(),
            },
        );
        assert_eq!(goal_completion_notices(&effects), [text]);
    }
    talaria_core::automation::update(&mut state, goal_completion_note());
    // A later active snapshot can precede the final snapshot without rearming the notice.
    let mut active = full_snapshot();
    active["revision"] = json!("r2");
    talaria_core::automation::update(&mut state, updated(SESSION, active));
    let effects = talaria_core::automation::update(
        &mut state,
        updated(SESSION, goal_completion_snapshot("done", "r3")),
    );
    assert!(goal_completion_notices(&effects).is_empty());
}

#[test]
fn goal_completion_tracking_rearms_after_resume_clear_and_replacement() {
    for change in ["resume", "clear", "replacement"] {
        let mut state = AutomationState::default();
        talaria_core::automation::update(&mut state, attached(SESSION));
        talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
        if change != "replacement" {
            talaria_core::automation::update(
                &mut state,
                updated(SESSION, goal_completion_snapshot("done", "r2")),
            );
        } else {
            talaria_core::automation::update(&mut state, goal_completion_note());
        }
        if change == "clear" {
            let mut cleared = full_snapshot();
            cleared["goal"] = Value::Null;
            cleared["revision"] = json!("r3");
            talaria_core::automation::update(&mut state, updated(SESSION, cleared));
        }
        let mut active = full_snapshot();
        active["revision"] = json!("r4");
        if change == "replacement" {
            active["goal"]["created_at"] = json!(1790000100);
        }
        talaria_core::automation::update(&mut state, updated(SESSION, active.clone()));
        active["goal"]["status"] = json!("done");
        active["goal"]["last_verdict"] = json!("done");
        active["goal"]["last_reason"] = json!("Second completion");
        active["revision"] = json!("r5");
        let effects = talaria_core::automation::update(&mut state, updated(SESSION, active));
        assert_eq!(
            goal_completion_notices(&effects),
            ["✓ Goal done after 3 of 20 turns: Second completion"],
            "{change} permits the next completion"
        );
    }
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
fn finished_verdict_stays_until_a_new_goal_or_a_session_switch() {
    let mut done = full_snapshot();
    done["goal"]["status"] = json!("done");
    done["goal"]["last_verdict"] = json!("needs-revision");
    done["goal"]["last_reason"] = json!("the notes shipped");
    done["revision"] = json!("r2");

    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    talaria_core::automation::update(&mut state, updated(SESSION, full_snapshot()));
    talaria_core::automation::update(&mut state, updated(SESSION, done));
    assert_eq!(
        state.panel_verdict().as_deref(),
        Some("needs-revision"),
        "a finished goal's explicit verdict is available to the panel"
    );

    let mut cleared = full_snapshot();
    cleared["goal"] = Value::Null;
    cleared["revision"] = json!("r3");
    talaria_core::automation::update(&mut state, updated(SESSION, cleared));
    assert_eq!(
        state.panel_verdict().as_deref(),
        Some("needs-revision"),
        "the verdict stays after the goal leaves the snapshot"
    );

    let mut replacement = full_snapshot();
    replacement["goal"]["title"] = json!("Plan the next release");
    replacement["goal"]["created_at"] = json!(1790000100.0);
    replacement["revision"] = json!("r4");
    talaria_core::automation::update(&mut state, updated(SESSION, replacement));
    assert_eq!(
        state.panel_verdict(),
        None,
        "a new goal drops the previous verdict"
    );

    let mut state = AutomationState::default();
    talaria_core::automation::update(&mut state, attached(SESSION));
    let mut done = full_snapshot();
    done["goal"]["status"] = json!("done");
    done["goal"]["last_verdict"] = json!("needs-revision");
    done["revision"] = json!("r2");
    talaria_core::automation::update(&mut state, updated(SESSION, done));
    talaria_core::automation::update(&mut state, attached("s2"));
    assert_eq!(
        state.panel_verdict(),
        None,
        "a session switch drops the previous verdict"
    );
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
