//! Replaying the recorded approval and clarification turns through the cards and the service.
//!
//! The recorded frame log is played by the in-process gateway into a real client. The reducer
//! turns the events into an open prompt, a card stack takes the key presses, the test conversion
//! turns the card's intent into reducer events, and the service executes the resulting effect.
//! What the gateway reads must equal what the recorded client sent.

#[path = "support/convert.rs"]
mod convert;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::{Duration, Instant};

use hermes_client::{Client, InboundStream};
use mock_gateway::{
    Fixture, MockGateway, SESSION_ID, fixture_outbound_params, next_event, session_event,
};
use serde_json::Value;
use talaria_app::prompts::PromptService;
use talaria_core::session::{Effect, PromptCell, PromptKey, SessionId, SessionState, update};
use talaria_tui::keys::{Key, KeyCode, KeyKind, Mods};
use talaria_tui::prompts::{KeyOutcome, PromptIntent, PromptStack};

/// What the clarification test types into the text field.
const TYPED_ANSWER: &str = "talaria-replay-answer";

fn press(code: KeyCode) -> Key {
    Key {
        code,
        mods: Mods::default(),
        kind: KeyKind::Press,
    }
}

fn typed(character: char) -> Key {
    press(KeyCode::Char(character))
}

/// The identifier generator the service is given: `sub-1`, `sub-2`, ...
fn submission_ids() -> Box<dyn FnMut() -> String + Send> {
    let mut next = 0u32;
    Box::new(move || {
        next += 1;
        format!("sub-{next}")
    })
}

/// The mock, a client attached to it, and the session state after the recorded turn reached its
/// prompt: the turn start and the request for the approval or the question.
async fn replay_to_prompt(
    fixture: Fixture,
) -> (MockGateway, Client, InboundStream, SessionState, PromptKey) {
    let (mock, client, mut inbound) = MockGateway::start().await;
    mock.set_auto_settle(true);
    assert_eq!(mock.replay_fixture(fixture, 2).await, 2);
    let mut state = SessionState::empty(SessionId::new(SESSION_ID));
    for _ in 0..2 {
        let event = next_event(&mut inbound).await;
        let _ = update(&mut state, session_event(&event));
    }
    let keys: Vec<PromptKey> = state
        .open_prompts()
        .map(|prompt| prompt.key.clone())
        .collect();
    assert_eq!(
        keys.len(),
        1,
        "the recorded turn should hold one open prompt"
    );
    let key = keys[0].clone();
    (mock, client, inbound, state, key)
}

/// A stack synced with the state's open prompts, a draft that is empty, and no masked requests.
fn focused_stack(state: &SessionState, now: Instant) -> PromptStack {
    let mut stack = PromptStack::new();
    let prompts: Vec<&PromptCell> = state.open_prompts().collect();
    stack.sync(Some(SESSION_ID), &prompts, &[], true, now);
    stack
}

fn intent_of(outcome: KeyOutcome) -> PromptIntent {
    match outcome {
        KeyOutcome::Consumed {
            intent: Some(intent),
            ..
        } => intent,
        _ => panic!("the key should have produced an answer"),
    }
}

fn has_no_intent(outcome: &KeyOutcome) -> bool {
    matches!(outcome, KeyOutcome::Consumed { intent: None, .. })
}

fn only_effect(effects: Vec<Effect>) -> Effect {
    assert_eq!(effects.len(), 1, "one answer should give one effect");
    match effects.into_iter().next() {
        Some(effect) => effect,
        None => panic!("one answer should give one effect"),
    }
}

fn sorted_keys(params: &Value) -> Vec<String> {
    let mut keys: Vec<String> = match params.as_object() {
        Some(object) => object.keys().cloned().collect(),
        None => panic!("the params should be an object"),
    };
    keys.sort();
    keys
}

#[tokio::test]
async fn approval_fixture_sends_recorded_respond() {
    let (mock, client, mut inbound, mut state, key) = replay_to_prompt(Fixture::Approval).await;
    let now = Instant::now();
    let mut stack = focused_stack(&state, now);
    let mut service = PromptService::new(submission_ids());
    assert!(
        service
            .on_prompts_changed(std::slice::from_ref(&key))
            .is_empty(),
        "a prompt that is still open gives no notice"
    );

    let outcome = stack.handle_key(typed('1'), now + Duration::from_millis(300));
    let intent = intent_of(outcome);
    let effect = only_effect(convert::apply_intent(&mut service, &mut state, intent));
    match &effect {
        Effect::RespondApproval {
            key: effect_key,
            choice,
        } => {
            assert_eq!(effect_key, &key);
            assert_eq!(choice, "once");
        }
        _ => panic!("digit 1 on the recorded approval should answer it"),
    }

    let executed = service.execute(&client, effect).await;
    assert!(
        executed.events.is_empty(),
        "a resolved answer with no note gives no follow-up event"
    );
    assert!(
        executed.notices.is_empty(),
        "a resolved answer gives no notice"
    );

    let recorded = fixture_outbound_params(Fixture::Approval, "approval.respond")
        .unwrap_or_else(|| panic!("the recorded log has no approval.respond"));
    let sent = mock.wait_for_request("approval.respond", 0).await;
    assert_eq!(
        sent.params, recorded,
        "the call must equal the recorded one"
    );
    assert_eq!(mock.requests("approval.respond").len(), 1);
    assert_eq!(sorted_keys(&sent.params), sorted_keys(&recorded));
    assert_eq!(sorted_keys(&sent.params).len(), 4);

    // The gateway settles the prompt; the card leaves and this client's own answer gives no
    // "answered elsewhere" notice.
    let settled = next_event(&mut inbound).await;
    let _ = update(&mut state, session_event(&settled));
    assert_eq!(state.open_prompts().count(), 0);
    assert!(
        service.on_prompts_changed(&[]).is_empty(),
        "a prompt this client answered leaves without a notice"
    );
}

#[tokio::test]
async fn clarification_fixture_sends_clarify_respond() {
    let (mock, client, mut inbound, mut state, key) =
        replay_to_prompt(Fixture::Clarification).await;
    let now = Instant::now();
    let mut stack = focused_stack(&state, now);
    let mut service = PromptService::new(submission_ids());
    assert!(
        service
            .on_prompts_changed(std::slice::from_ref(&key))
            .is_empty()
    );

    // The recorded question has two choices, so the Other row is row 3.
    let later = now + Duration::from_millis(300);
    let opened = stack.handle_key(typed('3'), later);
    assert!(
        has_no_intent(&opened),
        "choosing Other only opens the text field"
    );
    for character in TYPED_ANSWER.chars() {
        let outcome = stack.handle_key(typed(character), later);
        assert!(has_no_intent(&outcome), "typing sends nothing");
    }
    let intent = intent_of(stack.handle_key(press(KeyCode::Enter), later));
    let effect = only_effect(convert::apply_intent(&mut service, &mut state, intent));
    match &effect {
        Effect::RespondClarify {
            key: effect_key,
            answer,
        } => {
            assert_eq!(effect_key, &key);
            assert_eq!(answer, TYPED_ANSWER);
        }
        _ => panic!("Enter on the typed text should answer the question"),
    }

    let executed = service.execute(&client, effect).await;
    assert!(executed.events.is_empty());
    assert!(executed.notices.is_empty());

    // The recording withholds the answer, so only the other three keys can be compared.
    let recorded = fixture_outbound_params(Fixture::Clarification, "clarify.respond")
        .unwrap_or_else(|| panic!("the recorded log has no clarify.respond"));
    let sent = mock.wait_for_request("clarify.respond", 0).await;
    assert_eq!(mock.requests("clarify.respond").len(), 1);
    assert_eq!(sorted_keys(&sent.params), sorted_keys(&recorded));
    for name in ["session_id", "execution_generation", "prompt_id"] {
        assert_eq!(sent.params[name], recorded[name], "{name} should match");
    }
    assert_eq!(
        sent.params["answer"],
        Value::String(TYPED_ANSWER.to_owned())
    );
    assert!(sent.params.get("profile").is_none());

    let settled = next_event(&mut inbound).await;
    let _ = update(&mut state, session_event(&settled));
    assert_eq!(state.open_prompts().count(), 0);
    assert!(service.on_prompts_changed(&[]).is_empty());
}
