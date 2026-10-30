//! Keys, replies, stops, answers and the open-at cursor for the fleet overlay.

use proptest::prelude::*;
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::{FleetRow, Outcome, ProfileStatus, RowState};
use talaria_core::fleet_view::{
    FleetAction, FleetViewEffect, FleetViewInput, FleetViewState, PeekStatus, ReplyOutcome,
    StopOutcome, update,
};

const NOW: f64 = 1_792_700_000.0;

fn key(profile: &str, id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: id.to_owned(),
    }
}

fn row(profile: &str, id: &str, title: &str, state: RowState) -> FleetRow {
    FleetRow {
        terminal_location: None,
        key: key(profile, id),
        title: title.to_owned(),
        model: Some("ollama-cloud/minimax-m3.1-flash".to_owned()),
        source: Some("tui".to_owned()),
        started_at: Some(NOW - 60.0),
        message_count: 2,
        state,
        state_since: NOW - 20.0,
        outcome: None,
        foreground: false,
        subscribed: false,
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

fn snap(rows: Vec<FleetRow>) -> FleetViewInput {
    FleetViewInput::Snapshot {
        version: 1,
        rows,
        queue: Vec::new(),
        profiles: vec![
            (
                "fixture".to_owned(),
                ProfileStatus::Listed { truncated: false },
            ),
            (
                "default".to_owned(),
                ProfileStatus::Listed { truncated: false },
            ),
            (
                "<profile>".to_owned(),
                ProfileStatus::Listed { truncated: false },
            ),
        ],
        scope_note: None,
        link_note: None,
    }
}

fn opened() -> (FleetViewState, FleetRow, FleetRow) {
    let background = row("fixture", "s-work", "compile", RowState::Working);
    let idle = row("fixture", "s-idle", "rest", RowState::Idle);
    let mut state = FleetViewState::new(None, NOW);
    update(&mut state, snap(vec![background.clone(), idle.clone()]), 4);
    (state, background, idle)
}

fn effects_of(state: &mut FleetViewState, input: FleetViewInput) -> Vec<FleetViewEffect> {
    update(state, input, 4)
}

#[test]
fn space_peeks_and_moving_never_does() {
    let (mut state, background, idle) = opened();
    let effects = effects_of(&mut state, FleetViewInput::Action(FleetAction::Peek));
    assert_eq!(
        effects,
        vec![FleetViewEffect::OpenPeek(background.key.clone())]
    );
    assert!(matches!(state.peek(), Some((_, PeekStatus::Opening))));
    let again = effects_of(&mut state, FleetViewInput::Action(FleetAction::Peek));
    assert_eq!(again, vec![FleetViewEffect::ClosePeek]);
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Down));
    let other = effects_of(&mut state, FleetViewInput::Action(FleetAction::Peek));
    assert_eq!(other, vec![FleetViewEffect::OpenPeek(idle.key.clone())]);
    let moved = effects_of(&mut state, FleetViewInput::Action(FleetAction::Down));
    assert!(
        moved
            .iter()
            .all(|effect| !matches!(effect, FleetViewEffect::OpenPeek(_)))
    );

    let mut done = row("fixture", "s-done", "ship", RowState::Done);
    done.outcome = Some(Outcome::Completed);
    let failed = row("default", "s-fail", "migrate", RowState::Failed);
    let mut state = FleetViewState::new(None, NOW);
    // Groups come in registry order: FAILED before DONE.
    update(&mut state, snap(vec![failed.clone(), done.clone()]), 4);
    let peek_failed = effects_of(&mut state, FleetViewInput::Action(FleetAction::Peek));
    assert!(peek_failed.contains(&FleetViewEffect::Acknowledge(failed.key)));
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Down));
    let peek_done = effects_of(&mut state, FleetViewInput::Action(FleetAction::Peek));
    assert!(peek_done.contains(&FleetViewEffect::OpenPeek(done.key.clone())));
    assert!(peek_done.contains(&FleetViewEffect::Acknowledge(done.key.clone())));
}

#[test]
fn reply_editor_rules() {
    let (mut state, background, idle) = opened();
    let open = effects_of(&mut state, FleetViewInput::Action(FleetAction::Reply));
    assert_eq!(
        open,
        vec![FleetViewEffect::OpenPeek(background.key.clone())]
    );
    effects_of(&mut state, FleetViewInput::Text('h'));
    effects_of(&mut state, FleetViewInput::Paste("i\nthere".to_owned()));
    let line = state.reply_line().map(|(_, text)| text.to_owned());
    assert_eq!(line.as_deref(), Some("hi\nthere"));
    assert_eq!(state.notice(), Some("Reply to compile: hi\u{21b5}there"));
    effects_of(&mut state, FleetViewInput::Escape);
    assert!(state.reply_line().is_some(), "escape keeps the draft");
    let mut state = FleetViewState::new(None, NOW);
    update(&mut state, snap(vec![background.clone()]), 4);
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Reply));
    effects_of(&mut state, FleetViewInput::Text(' '));
    let blank = effects_of(&mut state, FleetViewInput::Enter);
    assert!(blank.is_empty(), "a whitespace draft sends nothing");
    effects_of(&mut state, FleetViewInput::Text('g'));
    effects_of(&mut state, FleetViewInput::Text('o'));
    let sent = effects_of(&mut state, FleetViewInput::Enter);
    assert_eq!(
        sent,
        vec![FleetViewEffect::SendReply {
            key: background.key.clone(),
            text: " go".to_owned()
        }]
    );
    assert!(state.row_flags(&background.key).reply_in_flight);
    let blocked = effects_of(&mut state, FleetViewInput::Action(FleetAction::Reply));
    assert!(blocked.is_empty());
    assert_eq!(state.notice(), Some("Sending the last reply\u{2026}"));

    effects_of(
        &mut state,
        FleetViewInput::ReplyDone {
            key: background.key.clone(),
            outcome: ReplyOutcome::Unknown {
                why: "no reply".to_owned(),
            },
        },
    );
    assert!(state.row_flags(&background.key).unconfirmed);
    assert!(!state.row_flags(&background.key).reply_in_flight);
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Reply));
    let resend = effects_of(&mut state, FleetViewInput::Enter);
    assert_eq!(
        resend,
        vec![FleetViewEffect::ResendReply(background.key.clone())]
    );
    effects_of(
        &mut state,
        FleetViewInput::ReplyDone {
            key: background.key.clone(),
            outcome: ReplyOutcome::Unknown {
                why: "connection dropped".to_owned(),
            },
        },
    );
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Reply));
    effects_of(&mut state, FleetViewInput::Text('x'));
    let held = effects_of(&mut state, FleetViewInput::Enter);
    assert!(
        held.is_empty(),
        "new text is not sent while a reply is unconfirmed"
    );
    assert_eq!(
        state.notice(),
        Some(
            "The last reply to \"compile\" may not have been sent. Clear this line and press enter to resend it."
        )
    );
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Down));
    let moved = effects_of(&mut state, FleetViewInput::Action(FleetAction::Peek));
    assert!(
        !moved
            .iter()
            .any(|effect| matches!(effect, FleetViewEffect::Close))
    );
    assert!(state.row_flags(&background.key).unconfirmed);
    let unsettled = state.unsettled_replies();
    assert!(unsettled.iter().any(|(key, title, why)| {
        key == &background.key
            && title == "compile"
            && (why == "no reply" || why == "connection dropped")
    }));
    let _ = idle;
}

#[test]
fn stop_asks_and_refuses_idle_rows() {
    let (mut state, background, _) = opened();
    let ask = effects_of(&mut state, FleetViewInput::Action(FleetAction::Stop));
    assert!(ask.is_empty());
    assert_eq!(
        state.notice(),
        Some(
            "Stop \"compile\"? Its running turn and background subagents stop and its open approvals are denied; queued prompts still run. Press x again to stop, any other key to keep it running."
        )
    );
    let stop = effects_of(&mut state, FleetViewInput::Action(FleetAction::Stop));
    assert!(stop.contains(&FleetViewEffect::Stop(background.key.clone())));
    assert!(state.row_flags(&background.key).stop_in_flight);

    for cancel in [
        FleetViewInput::Action(FleetAction::Down),
        FleetViewInput::Escape,
        FleetViewInput::CtrlC,
    ] {
        let (mut state, _, _) = opened();
        effects_of(&mut state, FleetViewInput::Action(FleetAction::Stop));
        let effects = effects_of(&mut state, cancel);
        assert!(effects.is_empty(), "a cancelled stop sends nothing");
        assert_eq!(state.notice(), Some("Stop cancelled."));
    }

    let (mut state, _, idle) = opened();
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Down));
    let idle_stop = effects_of(&mut state, FleetViewInput::Action(FleetAction::Stop));
    assert!(idle_stop.is_empty());
    assert_eq!(state.notice(), Some("Nothing is running in \"rest\"."));

    let mut foreground = background.clone();
    foreground.foreground = true;
    let mut state = FleetViewState::new(Some(foreground.key.clone()), NOW);
    update(&mut state, snap(vec![foreground]), 4);
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Stop));
    assert!(
        state
            .notice()
            .unwrap_or("")
            .starts_with("Stop \"compile\"?")
    );
    let _ = idle;
}

#[test]
fn enter_acknowledges_then_opens() {
    let mut done = row("fixture", "s-done", "ship", RowState::Done);
    done.outcome = Some(Outcome::Completed);
    let working = row("fixture", "s-work", "compile", RowState::Working);
    let mut state = FleetViewState::new(None, NOW);
    // Groups come in registry order: WORKING before DONE.
    update(&mut state, snap(vec![working.clone(), done.clone()]), 4);
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Down));
    let opened = effects_of(&mut state, FleetViewInput::Enter);
    assert_eq!(
        opened,
        vec![
            FleetViewEffect::Acknowledge(done.key.clone()),
            FleetViewEffect::Open(done.key),
        ]
    );
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Up));
    let plain = effects_of(&mut state, FleetViewInput::Enter);
    assert_eq!(plain, vec![FleetViewEffect::Open(working.key)]);
}

#[test]
fn esc_unwinds_and_ctrl_c_never_stops() {
    let (mut state, _, _) = opened();
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Filter));
    effects_of(&mut state, FleetViewInput::Text('c'));
    effects_of(&mut state, FleetViewInput::Enter);
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Peek));
    let clear_filter = effects_of(&mut state, FleetViewInput::Escape);
    assert!(clear_filter.is_empty());
    let close_peek = effects_of(&mut state, FleetViewInput::Escape);
    assert_eq!(close_peek, vec![FleetViewEffect::ClosePeek]);
    let close = effects_of(&mut state, FleetViewInput::Escape);
    assert_eq!(close, vec![FleetViewEffect::Close]);

    let (mut state, _, _) = opened();
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Reply));
    effects_of(&mut state, FleetViewInput::Text('z'));
    effects_of(&mut state, FleetViewInput::CtrlC);
    assert!(
        state.reply_line().is_some(),
        "ctrl-c in the reply keeps the draft"
    );
    let (mut state, _, _) = opened();
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Filter));
    effects_of(&mut state, FleetViewInput::Text('z'));
    effects_of(&mut state, FleetViewInput::CtrlC);
    let (mut state, _, _) = opened();
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Stop));
    effects_of(&mut state, FleetViewInput::CtrlC);
    assert_eq!(state.notice(), Some("Stop cancelled."));
    let (mut state, background, _) = opened();
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Peek));
    let closed = effects_of(&mut state, FleetViewInput::CtrlC);
    assert_eq!(
        closed,
        vec![FleetViewEffect::ClosePeek, FleetViewEffect::Close]
    );
    assert!(closed.iter().all(|effect| !matches!(
        effect,
        FleetViewEffect::Stop(_) | FleetViewEffect::SendReply { .. }
    )));
    let _ = background;
}

#[test]
fn answer_focus_rules() {
    let (mut state, background, _) = opened();
    let none = effects_of(&mut state, FleetViewInput::Action(FleetAction::Answer));
    assert!(none.is_empty());
    assert_eq!(state.notice(), Some("Nothing to answer in this session."));
    effects_of(
        &mut state,
        FleetViewInput::Peek {
            key: background.key.clone(),
            status: PeekStatus::Open {
                has_open_prompt: false,
                running: true,
            },
        },
    );
    let still = effects_of(&mut state, FleetViewInput::Action(FleetAction::Answer));
    assert!(still.is_empty());
    effects_of(
        &mut state,
        FleetViewInput::Peek {
            key: background.key.clone(),
            status: PeekStatus::Open {
                has_open_prompt: true,
                running: true,
            },
        },
    );
    let focus = effects_of(&mut state, FleetViewInput::Action(FleetAction::Answer));
    assert_eq!(focus, vec![FleetViewEffect::FocusAnswer]);
    let stayed = effects_of(&mut state, FleetViewInput::Escape);
    assert!(stayed.is_empty(), "escape in an answer belongs to the card");
    assert!(matches!(
        state.focus(),
        talaria_core::fleet_view::Focus::Answer
    ));
    effects_of(&mut state, FleetViewInput::CtrlC);
    assert!(matches!(
        state.focus(),
        talaria_core::fleet_view::Focus::List
    ));
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Answer));
    effects_of(&mut state, FleetViewInput::AnswerFocusEnded);
    assert!(matches!(
        state.focus(),
        talaria_core::fleet_view::Focus::List
    ));

    let mut foreground = background.clone();
    foreground.foreground = true;
    let mut state = FleetViewState::new(Some(foreground.key.clone()), NOW);
    update(&mut state, snap(vec![foreground.clone()]), 4);
    effects_of(
        &mut state,
        FleetViewInput::Peek {
            key: foreground.key.clone(),
            status: PeekStatus::Open {
                has_open_prompt: true,
                running: false,
            },
        },
    );
    let leave = effects_of(&mut state, FleetViewInput::Action(FleetAction::Answer));
    assert_eq!(leave, vec![FleetViewEffect::Close]);
}

#[test]
fn results_update_rows() {
    let (mut state, background, _) = opened();
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Reply));
    effects_of(&mut state, FleetViewInput::Text('g'));
    effects_of(&mut state, FleetViewInput::Text('o'));
    effects_of(&mut state, FleetViewInput::Enter);
    let samples = [
        (ReplyOutcome::Sent, "Sent to \"compile\"."),
        (
            ReplyOutcome::Queued,
            "Queued for \"compile\": it runs after the current turn.",
        ),
        (
            ReplyOutcome::AlreadyRan,
            "Hermes had already run this reply in \"compile\".",
        ),
        (
            ReplyOutcome::StateUnknown,
            "Hermes accepted the reply for \"compile\" but reports its state as unknown.",
        ),
        (
            ReplyOutcome::VoiceStopped,
            "Hermes took this as a voice stop phrase; nothing was queued.",
        ),
        (
            ReplyOutcome::Refused {
                reason: "queue_full".to_owned(),
            },
            "\"compile\" already has 32 queued prompts. Wait for some to run.",
        ),
        (
            ReplyOutcome::Refused {
                reason: "permission_denied".to_owned(),
            },
            "Hermes refused the reply (permission_denied).",
        ),
        (
            ReplyOutcome::NotAttached {
                reason: "permission_denied".to_owned(),
            },
            "Did not send the reply: could not open \"compile\" (permission_denied).",
        ),
        (
            ReplyOutcome::Unknown {
                why: "no reply".to_owned(),
            },
            "The reply may not have been sent (no reply). Press r, then enter, to send it again; Hermes will not queue it twice.",
        ),
        (
            ReplyOutcome::Unknown {
                why: "connection dropped".to_owned(),
            },
            "The reply may not have been sent (connection dropped). Press r, then enter, to send it again; Hermes will not queue it twice.",
        ),
    ];
    for (outcome, text) in samples {
        effects_of(
            &mut state,
            FleetViewInput::ReplyDone {
                key: background.key.clone(),
                outcome: outcome.clone(),
            },
        );
        assert!(!state.row_flags(&background.key).reply_in_flight);
        assert_eq!(state.notice(), Some(text));
        let restores = matches!(
            outcome,
            ReplyOutcome::Refused { .. } | ReplyOutcome::NotAttached { .. }
        );
        if restores {
            effects_of(&mut state, FleetViewInput::Action(FleetAction::Reply));
            assert_eq!(state.reply_line().map(|(_, line)| line), Some("go"));
            effects_of(&mut state, FleetViewInput::Escape);
        }
        if matches!(outcome, ReplyOutcome::Unknown { .. }) {
            assert!(state.row_flags(&background.key).unconfirmed);
        }
    }
    effects_of(
        &mut state,
        FleetViewInput::ReplyDone {
            key: background.key.clone(),
            outcome: ReplyOutcome::Sent,
        },
    );
    assert!(!state.row_flags(&background.key).unconfirmed);

    effects_of(&mut state, FleetViewInput::Action(FleetAction::Stop));
    effects_of(&mut state, FleetViewInput::Action(FleetAction::Stop));
    let again = effects_of(&mut state, FleetViewInput::Action(FleetAction::Stop));
    assert!(again.is_empty());
    assert_eq!(state.notice(), Some("Stopping \"compile\"\u{2026}"));
    let stops = [
        (StopOutcome::Stopping, "Stopping \"compile\"\u{2026}"),
        (
            StopOutcome::NothingRunning,
            "Nothing is running in \"compile\".",
        ),
        (
            StopOutcome::Refused {
                reason: "stale_generation".to_owned(),
            },
            "Hermes did not stop the turn (stale_generation). If it is still running, press x, then x again.",
        ),
        (
            StopOutcome::Refused {
                reason: "permission_denied".to_owned(),
            },
            "Hermes did not stop the turn (permission_denied).",
        ),
        (
            StopOutcome::NotAttached {
                reason: "permission_denied".to_owned(),
            },
            "Did not stop \"compile\": could not open the session (permission_denied).",
        ),
        (
            StopOutcome::Unknown {
                why: "no reply".to_owned(),
            },
            "The stop may not have reached Hermes (no reply). If the turn is still running, press x again.",
        ),
    ];
    for (outcome, text) in stops {
        effects_of(
            &mut state,
            FleetViewInput::StopDone {
                key: background.key.clone(),
                outcome,
            },
        );
        assert!(!state.row_flags(&background.key).stop_in_flight);
        assert_eq!(state.notice(), Some(text));
    }
}

#[test]
fn open_at_places_cursor_and_peeks() {
    let needs = row("fixture", "s-need", "deploy", RowState::NeedsYou);
    let mut done = row("fixture", "s-done", "ship", RowState::Done);
    done.outcome = Some(Outcome::Completed);
    let mut state = FleetViewState::new(None, NOW);
    let held = state.open_at(needs.key.clone());
    assert!(held.is_empty());
    assert!(state.notice().is_none());
    let applied = update(&mut state, snap(vec![needs.clone(), done.clone()]), 4);
    assert!(applied.contains(&FleetViewEffect::OpenPeek(needs.key.clone())));
    assert_eq!(state.cursor(), Some(&needs.key));

    let mut state = FleetViewState::new(None, NOW);
    update(&mut state, snap(vec![needs.clone(), done.clone()]), 4);
    let peek_done = state.open_at(done.key.clone());
    assert!(peek_done.contains(&FleetViewEffect::OpenPeek(done.key.clone())));
    assert!(peek_done.contains(&FleetViewEffect::Acknowledge(done.key)));

    let missing = state.open_at(key("fixture", "s-gone"));
    assert!(missing.is_empty());
    assert_eq!(
        state.notice(),
        Some("That session is no longer in the fleet.")
    );

    let mut early = FleetViewState::new(None, NOW);
    let _ = early.open_at(key("fixture", "s-missing"));
    let late = update(&mut early, snap(vec![needs]), 4);
    assert!(
        late.is_empty(),
        "a held key that is absent from the first snapshot emits nothing"
    );
    assert_eq!(
        early.notice(),
        Some("That session is no longer in the fleet.")
    );
}

fn profile(index: usize) -> String {
    ["default", "fixture", "<profile>"][index % 3].to_owned()
}

fn strategy_row(profile_index: usize, session: usize, state: RowState) -> FleetRow {
    row(
        &profile(profile_index),
        &format!("s-{session}"),
        &format!("title {session}"),
        state,
    )
}

fn input_strategy() -> impl Strategy<Value = FleetViewInput> {
    let states = prop_oneof![
        Just(RowState::NeedsYou),
        Just(RowState::Working),
        Just(RowState::Failed),
        Just(RowState::Done),
        Just(RowState::Idle),
    ];
    let row_strategy = (0..3usize, 0..6usize, states)
        .prop_map(|(profile, session, state)| strategy_row(profile, session, state));
    let action = prop_oneof![
        Just(FleetAction::Up),
        Just(FleetAction::Down),
        Just(FleetAction::PageUp),
        Just(FleetAction::PageDown),
        Just(FleetAction::Top),
        Just(FleetAction::Bottom),
        Just(FleetAction::Open),
        Just(FleetAction::Peek),
        Just(FleetAction::Reply),
        Just(FleetAction::Stop),
        Just(FleetAction::Answer),
        Just(FleetAction::Filter),
        Just(FleetAction::ProfileNext),
        Just(FleetAction::ProfilePrev),
        Just(FleetAction::Close),
    ];
    let outcome = prop_oneof![
        Just(ReplyOutcome::Sent),
        Just(ReplyOutcome::Queued),
        Just(ReplyOutcome::AlreadyRan),
        Just(ReplyOutcome::StateUnknown),
        Just(ReplyOutcome::VoiceStopped),
        Just(ReplyOutcome::Refused {
            reason: "queue_full".to_owned()
        }),
        Just(ReplyOutcome::NotAttached {
            reason: "no reply".to_owned()
        }),
        Just(ReplyOutcome::Unknown {
            why: "no reply".to_owned()
        }),
        Just(ReplyOutcome::Unknown {
            why: "connection dropped".to_owned()
        }),
    ];
    prop_oneof![
        prop::collection::vec(row_strategy, 0..6).prop_map(|rows| FleetViewInput::Snapshot {
            version: 1,
            rows,
            queue: Vec::new(),
            profiles: vec![
                (
                    "default".to_owned(),
                    ProfileStatus::Listed { truncated: false }
                ),
                (
                    "fixture".to_owned(),
                    ProfileStatus::Listed { truncated: false }
                ),
                (
                    "<profile>".to_owned(),
                    ProfileStatus::Failed {
                        reason: "down".to_owned()
                    }
                ),
            ],
            scope_note: None,
            link_note: None,
        }),
        action.prop_map(FleetViewInput::Action),
        any::<char>().prop_map(FleetViewInput::Text),
        Just(FleetViewInput::Paste("line\nmore".to_owned())),
        Just(FleetViewInput::Backspace),
        Just(FleetViewInput::Left),
        Just(FleetViewInput::Right),
        Just(FleetViewInput::Home),
        Just(FleetViewInput::End),
        Just(FleetViewInput::Enter),
        Just(FleetViewInput::Escape),
        Just(FleetViewInput::CtrlC),
        Just(FleetViewInput::AnyOtherKey),
        Just(FleetViewInput::PeekClosed),
        Just(FleetViewInput::AnswerFocusEnded),
        Just(FleetViewInput::Notice("note".to_owned())),
        Just(FleetViewInput::Resize { cols: 80, rows: 24 }),
        Just(FleetViewInput::Tick { now: NOW + 30.0 }),
        (0..3usize, 0..6usize, outcome).prop_map(|(which, session, outcome)| {
            FleetViewInput::ReplyDone {
                key: key(&profile(which), &format!("s-{session}")),
                outcome,
            }
        }),
        (0..3usize, 0..6usize).prop_map(|(which, session)| FleetViewInput::StopDone {
            key: key(&profile(which), &format!("s-{session}")),
            outcome: StopOutcome::Stopping,
        }),
        (0..3usize, 0..6usize).prop_map(|(which, session)| FleetViewInput::Peek {
            key: key(&profile(which), &format!("s-{session}")),
            status: PeekStatus::Open {
                has_open_prompt: true,
                running: true,
            },
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn fleet_view_invariants(inputs in prop::collection::vec(input_strategy(), 0..200)) {
        let mut state = FleetViewState::new(None, NOW);
        let mut confirm_for: Option<RowKey> = None;
        for input in inputs {
            let entered_confirm = matches!(input, FleetViewInput::Action(FleetAction::Stop));
            let is_key = matches!(
                input,
                FleetViewInput::Action(_)
                    | FleetViewInput::Text(_)
                    | FleetViewInput::Paste(_)
                    | FleetViewInput::Backspace
                    | FleetViewInput::Left
                    | FleetViewInput::Right
                    | FleetViewInput::Home
                    | FleetViewInput::End
                    | FleetViewInput::Enter
                    | FleetViewInput::Escape
                    | FleetViewInput::CtrlC
                    | FleetViewInput::AnyOtherKey
            );
            let effects = update(&mut state, input, 4);
            let cursor = state.cursor().cloned();
            let visible: Vec<RowKey> = state
                .visible()
                .iter()
                .flat_map(|group| group.rows.iter().map(|(row, _)| row.key.clone()))
                .collect();
            prop_assert!(cursor.as_ref().map(|key| visible.contains(key)).unwrap_or(true));
            let _ = state.peek();
            for effect in &effects {
                if let FleetViewEffect::Stop(key) = effect {
                    prop_assert_eq!(confirm_for.as_ref(), Some(key));
                }
            }
            let close_is_last = effects.iter().all(|effect| {
                !matches!(effect, FleetViewEffect::Close) || effect == effects.last().unwrap()
            });
            prop_assert!(close_is_last);
            let closes = effects.iter().any(|effect| matches!(effect, FleetViewEffect::Close));
            let sends = effects.iter().any(|effect| matches!(effect, FleetViewEffect::SendReply { .. }));
            prop_assert!(!(closes && sends));
            // A snapshot or a result between the two presses keeps the confirmation (edge case
            // E7: the second x re-checks the row); any other key ends it.
            if entered_confirm {
                confirm_for = state.cursor().cloned();
            } else if is_key {
                confirm_for = None;
            }
        }
    }
}
