//! Properties of `update`. Events are raw JSON. A value `EventFrame::from_params` refuses is
//! dropped, which is what the application does. Wrong-typed extras that still decode are kept.

#[path = "support/drive.rs"]
mod drive;
#[path = "support/events.rs"]
mod events;

use proptest::prelude::*;
use serde_json::{Value, json};
use talaria_core::session::{Cell, Event, PromptState, SessionId, SessionState, update};

use events::{EPOCH, SESSION, try_session_event};

fn envelope(seq: u64, type_name: &str, payload: Value) -> Value {
    json!({
        "type": type_name,
        "session_id": SESSION,
        "replay_epoch": EPOCH,
        "seq": seq,
        "execution_generation": 1,
        "authority_epoch": 1,
        "payload": payload,
    })
}

fn arb_session_raw() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        (1u64..40).prop_map(|seq| envelope(seq, "message.start", json!({}))),
        (
            1u64..40,
            prop::sample::select(vec![
                "Hello".to_string(),
                "\n".into(),
                "".into(),
                "  ".into()
            ])
        )
            .prop_map(|(seq, text)| envelope(seq, "message.delta", json!({"text": text}))),
        (1u64..40).prop_map(|seq| {
            envelope(
                seq,
                "message.complete",
                json!({"status": "nope", "text": "x", "is_error": "not-a-bool"}),
            )
        }),
        (1u64..40).prop_map(|seq| envelope(seq, "message.complete", json!({"status": "complete"}))),
        (
            1u64..40,
            prop::sample::select(vec!["once".to_string(), "deny".into()])
        )
            .prop_map(|(seq, choice)| {
                envelope(
                    seq,
                    "approval.request",
                    json!({"prompt_id": "p1", "choices": [choice], "execution_generation": 1}),
                )
            }),
        (1u64..40).prop_map(|seq| {
            envelope(
                seq,
                "approval.settled",
                json!({"prompt_id": "p1", "execution_generation": 1}),
            )
        }),
        (1u64..40).prop_map(|seq| {
            envelope(
                seq,
                "tool.complete",
                json!({"name": "terminal", "tool_id": "call-1", "is_error": "yes", "context": 4}),
            )
        }),
        (1u64..40).prop_map(|seq| {
            envelope(
                seq,
                "session.usage",
                json!({"usage": {"input": "no"}, "cache_read_tokens": "bad"}),
            )
        }),
        (1u64..40).prop_map(|seq| envelope(seq, "x.unknown", json!({"text": 1}))),
        (1u64..40).prop_map(|seq| {
            json!({
                "type": "message.delta",
                "session_id": "other-session",
                "replay_epoch": EPOCH,
                "seq": seq,
                "execution_generation": 1,
                "payload": {"text": "nope"},
            })
        }),
    ]
}

fn arb_local() -> impl Strategy<Value = Event> {
    prop_oneof![
        Just(Event::Interrupt),
        Just(Event::InterruptFailed),
        Just(Event::ConnectionOpened { generation: 1 }),
        any::<u8>().prop_map(|n| Event::Submit {
            text: format!("q{n}"),
            display: format!("q{n}"),
            submission_id: format!("s{n}"),
            attachments: Vec::new(),
        }),
        Just(Event::AnswerAlreadyResolved {
            key: events::key("p1", 1),
        }),
        Just(Event::AnswerApproval {
            key: events::key("p1", 1),
            choice: "once".to_string(),
        }),
        Just(Event::SubmitFailed {
            submission_id: "s1".to_string(),
            reason: "refused".to_string(),
        }),
    ]
}

fn decoded(raws: Vec<Value>) -> Vec<Event> {
    raws.into_iter()
        .filter_map(|raw| try_session_event(&raw))
        .collect()
}

fn apply_all(events: Vec<Event>) -> SessionState {
    let mut state = SessionState::empty(SessionId::new(SESSION));
    for event in events {
        let _ = update(&mut state, event);
    }
    state
}

fn open_assistant_count(state: &SessionState) -> usize {
    state
        .cells()
        .iter()
        .filter(|cell| matches!(cell, Cell::Assistant(cell) if !cell.sealed))
        .count()
}

fn open_reasoning_count(state: &SessionState) -> usize {
    state
        .cells()
        .iter()
        .filter(|cell| matches!(cell, Cell::Reasoning(cell) if !cell.sealed))
        .count()
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    })]
    #[test]
    fn update_never_panics_on_arbitrary_event_sequences(
        raws in prop::collection::vec(arb_session_raw(), 0..12),
        locals in prop::collection::vec(arb_local(), 0..6),
    ) {
        let mut events = decoded(raws);
        events.extend(locals);
        let _ = apply_all(events);
    }

    #[test]
    fn final_prefix_only_grows_and_final_cells_never_change(
        raws in prop::collection::vec(arb_session_raw(), 1..12),
        locals in prop::collection::vec(arb_local(), 0..4),
    ) {
        let mut events = decoded(raws);
        events.extend(locals);
        let mut state = SessionState::empty(SessionId::new(SESSION));
        let mut kept: Vec<Cell> = Vec::new();
        for event in events {
            let _ = update(&mut state, event);
            let len = state.final_prefix_len();
            prop_assert!(len >= kept.len());
            prop_assert_eq!(&state.cells()[..kept.len()], kept.as_slice());
            kept = state.cells()[..len].to_vec();
        }
    }

    #[test]
    fn chunking_deltas_never_changes_the_cells(
        left in prop::collection::vec(1usize..8, 1..5),
        right in prop::collection::vec(1usize..8, 1..5),
    ) {
        let text = "ABCDEFGH";
        let (left_state, _) = drive::reduce(SESSION, delta_events(text, &left));
        let (right_state, _) = drive::reduce(SESSION, delta_events(text, &right));
        prop_assert_eq!(left_state.cells(), right_state.cells());
    }

    #[test]
    fn open_cells_and_prompt_registry_stay_consistent(
        raws in prop::collection::vec(arb_session_raw(), 0..12),
    ) {
        let events = decoded(raws);
        let mut state = SessionState::empty(SessionId::new(SESSION));
        for event in events {
            let _ = update(&mut state, event);
            prop_assert!(open_assistant_count(&state) <= 1);
            prop_assert!(open_reasoning_count(&state) <= 1);
            let prompts: Vec<_> = state
                .cells()
                .iter()
                .filter_map(|cell| match cell {
                    Cell::Prompt(prompt) => Some(prompt),
                    _ => None,
                })
                .collect();
            let mut keys = prompts.iter().map(|prompt| prompt.key.clone()).collect::<Vec<_>>();
            keys.sort_by(|left, right| {
                left.prompt_id.cmp(&right.prompt_id).then(left.execution_generation.cmp(&right.execution_generation))
            });
            keys.dedup();
            prop_assert_eq!(keys.len(), prompts.len());
            let open_cells = prompts
                .iter()
                .filter(|prompt| matches!(prompt.state, PromptState::Open | PromptState::Answering(_)))
                .count();
            prop_assert_eq!(state.open_prompts().count(), open_cells);
        }
    }

    #[test]
    fn sequenced_events_are_idempotent_under_duplication(
        raws in prop::collection::vec(arb_session_raw(), 0..8),
    ) {
        let events: Vec<Event> = decoded(raws)
            .into_iter()
            .filter(|event| matches!(event, Event::Session { frame, .. } if frame.seq.is_some()))
            .collect();
        let once = apply_all(events.clone());
        let mut doubled = Vec::new();
        for event in &events {
            doubled.push(event.clone());
            doubled.push(event.clone());
        }
        let twice = apply_all(doubled);
        prop_assert_eq!(once.cells(), twice.cells());
        prop_assert_eq!(once.turn(), twice.turn());
        prop_assert_eq!(once.cursor(), twice.cursor());
    }
}

fn delta_events(text: &str, cuts: &[usize]) -> Vec<Event> {
    let mut points: Vec<usize> = cuts
        .iter()
        .copied()
        .filter(|cut| *cut < text.len())
        .collect();
    points.sort_unstable();
    points.dedup();
    let mut chunks = Vec::new();
    let mut prev = 0usize;
    for cut in points {
        if cut > prev {
            chunks.push(text[prev..cut].to_string());
            prev = cut;
        }
    }
    if prev < text.len() {
        chunks.push(text[prev..].to_string());
    }
    if chunks.is_empty() {
        chunks.push(text.to_string());
    }
    let mut events = vec![events::ev(1, "message.start", json!({}))];
    for (index, chunk) in chunks.into_iter().enumerate() {
        events.push(events::ev(
            (index as u64) + 2,
            "message.delta",
            json!({"text": chunk}),
        ));
    }
    events
}
