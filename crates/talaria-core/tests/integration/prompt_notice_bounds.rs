//! Settled prompt cells and the notice store stay within fixed bounds.

use crate::drive;
use crate::events;

use serde_json::json;
use talaria_core::session::{Cell, Effect, Event, PromptState, update};

use events::{SESSION, approval_request, ev, key};

const SETTLED_CAP: usize = 256;
const NOTICE_CAP: usize = 64;
const TEXT_CAP: usize = 4096;
const SUFFIX: &str = "… (truncated)";

fn prompt_ids(state: &talaria_core::session::SessionState) -> Vec<(String, PromptState)> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Prompt(prompt) => Some((prompt.key.prompt_id.clone(), prompt.state.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn settled_prompt_history_is_pruned_and_open_prompts_stay() {
    let mut events = Vec::new();
    let mut seq = 1_u64;
    events.push(ev(
        seq,
        "approval.request",
        approval_request("open", 1, &["once", "deny"]),
    ));
    seq += 1;
    for n in 0..300 {
        let id = format!("p{n}");
        events.push(ev(
            seq,
            "approval.request",
            approval_request(&id, 1, &["once", "deny"]),
        ));
        seq += 1;
        events.push(ev(
            seq,
            "approval.settled",
            json!({"prompt_id": id, "execution_generation": 1}),
        ));
        seq += 1;
    }
    let (mut state, _) = drive::reduce(SESSION, events);
    let prompts = prompt_ids(&state);
    let settled = prompts
        .iter()
        .filter(|(_, prompt_state)| matches!(prompt_state, PromptState::Settled(_)))
        .count();
    let open = prompts
        .iter()
        .filter(|(id, prompt_state)| id == "open" && matches!(prompt_state, PromptState::Open))
        .count();
    assert_eq!(settled, SETTLED_CAP);
    assert_eq!(open, 1, "the open prompt was pruned");
    assert!(prompts.iter().all(|(id, _)| id != "p0"));
    assert!(prompts.iter().any(|(id, _)| id == "p44"));
    assert!(prompts.iter().any(|(id, _)| id == "p299"));

    let effects = update(
        &mut state,
        Event::AnswerApproval {
            key: key("open", 1),
            choice: "once".to_owned(),
        },
    );
    assert!(
        matches!(effects.as_slice(), [Effect::RespondApproval { .. }]),
        "the surviving open prompt was not answerable"
    );
}

#[test]
fn open_prompt_cells_are_not_pruned_to_make_room() {
    let mut events = Vec::new();
    for n in 0..300 {
        events.push(ev(
            u64::try_from(n + 1).expect("seq"),
            "approval.request",
            approval_request(&format!("open-{n}"), 1, &["deny"]),
        ));
    }
    let (state, _) = drive::reduce(SESSION, events);
    let open: Vec<_> = prompt_ids(&state)
        .into_iter()
        .filter(|(_, prompt_state)| matches!(prompt_state, PromptState::Open))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(open.len(), 64, "open prompts grew past 64");
    assert_eq!(open.first().map(String::as_str), Some("open-0"));
    assert_eq!(open.last().map(String::as_str), Some("open-63"));
    assert!(!open.iter().any(|id| id == "open-64"));
}

#[test]
fn oversized_prompt_strings_are_truncated_before_they_are_stored() {
    let huge = "A".repeat(8 * 1024 * 1024);
    let wide = "é".repeat(3000);
    let mut approval = approval_request("big", 1, &["deny", wide.as_str()]);
    approval["command"] = json!(huge.clone());
    approval["description"] = json!(wide.clone());
    let clarify = json!({
        "kind": "clarify",
        "prompt_id": "q1",
        "execution_generation": 1,
        "question": huge,
        "choices": [wide.clone(), huge.clone()],
        "multi_select": false,
    });
    let (state, _) = drive::reduce(
        SESSION,
        vec![
            ev(1, "approval.request", approval),
            ev(2, "clarify.request", clarify),
        ],
    );
    for cell in state.cells() {
        let Cell::Prompt(prompt) = cell else {
            continue;
        };
        for (name, value) in [
            ("command", prompt.command.as_deref()),
            ("description", prompt.description.as_deref()),
            ("question", prompt.question.as_deref()),
        ] {
            let Some(value) = value else {
                continue;
            };
            assert!(
                value.len() <= TEXT_CAP,
                "{name} stored {} bytes",
                value.len()
            );
            if value.len() > 64 {
                assert!(
                    value.ends_with(SUFFIX),
                    "{name} was stored without a visible truncation"
                );
            }
        }
        for choice in &prompt.choices {
            assert!(
                choice.len() <= 16 * 1024,
                "a choice over the exact-answer cap was stored"
            );
            if choice.starts_with('é') {
                assert_eq!(choice, &wide, "the clarify answer was rewritten");
            }
        }
        if prompt.kind == talaria_core::session::PromptKind::Clarify {
            assert!(
                !prompt.choices.iter().any(|choice| choice.len() > 16 * 1024),
                "the oversized choice was offered"
            );
            assert!(prompt.choices.iter().any(|choice| choice == &wide));
        }
    }
}

#[test]
fn pruning_a_committed_prompt_leaves_the_new_final_cell_past_the_cursor() {
    let mut events = Vec::new();
    let mut seq = 1_u64;
    for n in 0..=SETTLED_CAP {
        let id = format!("p{n}");
        events.push(ev(
            seq,
            "approval.request",
            approval_request(&id, 1, &["once", "deny"]),
        ));
        seq += 1;
        events.push(ev(
            seq,
            "approval.settled",
            json!({"prompt_id": id, "execution_generation": 1}),
        ));
        seq += 1;
    }
    let (mut state, _) = drive::reduce(SESSION, events);
    assert_eq!(
        state
            .cells()
            .iter()
            .filter(|cell| matches!(cell, Cell::Prompt(_)))
            .count(),
        SETTLED_CAP
    );
    state.publish_commit_cursor(state.final_prefix_len());
    let before = state.commit_cursor();
    let effects = update(
        &mut state,
        ev(
            seq,
            "approval.request",
            approval_request("newest", 1, &["once", "deny"]),
        ),
    );
    assert!(effects.is_empty());
    let _ = update(
        &mut state,
        ev(
            seq + 1,
            "approval.settled",
            json!({"prompt_id": "newest", "execution_generation": 1}),
        ),
    );
    let cursor = state.commit_cursor();
    assert!(
        cursor < before,
        "compaction left the commit cursor on the old index"
    );
    assert!(
        state.final_prefix_len() > cursor,
        "the newly final cell is not after the adjusted cursor"
    );
    assert!(
        state.cells()[cursor..state.final_prefix_len()]
            .iter()
            .any(|cell| matches!(cell, Cell::Prompt(prompt) if prompt.key.prompt_id == "newest"))
    );
}

#[test]
fn resumed_prompt_fields_keep_exact_choices_and_a_bounded_label() {
    let wide = "é".repeat(3000);
    let huge = "B".repeat(8 * 1024 * 1024);
    let raw = json!({
        "session_id": SESSION,
        "stored_session_id": SESSION,
        "messages": [],
        "message_count": 0,
        "running": false,
        "authority_epoch": 1,
        "replay_epoch": "epoch-1",
        "last_sequence": 1,
        "subscription_id": "sub-1",
        "revision": 1,
        "execution_generation": 1,
        "pending": [],
        "prompts": [{
            "kind": "clarify",
            "prompt_id": "q-resume",
            "execution_generation": 1,
            "question": huge,
            "choices": [wide.clone(), "B".repeat(20 * 1024)]
        }],
        "info": {"cwd": "/work", "desktop_protocol": "hermes-gateway-v1"}
    });
    let snapshot = serde_json::from_value::<hermes_contract::schemas::CanonicalSnapshot>(raw)
        .expect("snapshot");
    let state = talaria_core::session::SessionState::from_snapshot(snapshot);
    let Cell::Prompt(prompt) = &state.cells()[0] else {
        panic!("the resumed prompt was dropped");
    };
    let question = prompt.question.as_deref().expect("question");
    assert!(question.len() <= TEXT_CAP);
    assert!(question.ends_with(SUFFIX));
    assert_eq!(prompt.choices, vec![wide]);
}

#[test]
fn notice_store_keeps_at_most_64_entries() {
    let mut events = Vec::new();
    for n in 0..70 {
        events.push(ev(
            u64::try_from(n + 1).expect("seq"),
            "notification.show",
            json!({
                "text": format!("notice {n}"),
                "level": "info",
                "kind": "sticky",
                "key": format!("n{n}"),
            }),
        ));
    }
    let (state, _) = drive::reduce(SESSION, events);
    assert!(
        state.notices().by_key("n0").is_none(),
        "the oldest notice stayed"
    );
    assert!(state.notices().by_key("n5").is_none());
    assert_eq!(
        state.notices().by_key("n6").map(|row| row.text.as_str()),
        Some("notice 6")
    );
    assert_eq!(
        state.notices().by_key("n69").map(|row| row.text.as_str()),
        Some("notice 69")
    );
    let mut present = 0;
    for n in 0..70 {
        if state.notices().by_key(&format!("n{n}")).is_some() {
            present += 1;
        }
    }
    assert_eq!(present, NOTICE_CAP);
}
