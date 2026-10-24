//! Executing the respond effects: the exact wire keys, the note follow-up, failures, and the
//! deny that goes out before an interrupt.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use hermes_client::{Client, InboundStream};
use mock_gateway::{MockGateway, Reply, SESSION_ID, next_event, session_event};
use serde_json::{Value, json};
use talaria_app::prompts::{Executed, PromptService};
use talaria_core::session::{
    AnswerRefusal, Effect, Event, PromptKey, SessionId, SessionState, update,
};

const NOTE: &str = "use a dry run first";

fn submission_ids() -> Box<dyn FnMut() -> String + Send> {
    let mut next = 0u32;
    Box::new(move || {
        next += 1;
        format!("sub-{next}")
    })
}

fn key(prompt_id: &str, generation: i64) -> PromptKey {
    PromptKey {
        session_id: SessionId::new(SESSION_ID),
        prompt_id: prompt_id.to_owned(),
        execution_generation: generation,
    }
}

fn key_names(params: &Value) -> BTreeSet<String> {
    match params.as_object() {
        Some(object) => object.keys().cloned().collect(),
        None => panic!("the params should be an object"),
    }
}

fn names(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

fn respond_approval(prompt_key: &PromptKey, choice: &str) -> Effect {
    Effect::RespondApproval {
        key: prompt_key.clone(),
        choice: choice.to_owned(),
    }
}

fn respond_clarify(prompt_key: &PromptKey, answer: &str) -> Effect {
    Effect::RespondClarify {
        key: prompt_key.clone(),
        answer: answer.to_owned(),
    }
}

/// Feed every event the mock has written and the client has delivered into the state.
async fn deliver(inbound: &mut InboundStream, state: &mut SessionState, count: usize) {
    for _ in 0..count {
        let event = next_event(inbound).await;
        let _ = update(state, session_event(&event));
    }
}

/// The text of the one notice, which must be the only one.
fn only_notice(executed: &Executed) -> String {
    assert_eq!(executed.notices.len(), 1, "exactly one notice was expected");
    assert_eq!(
        executed.notices[0].session_id.as_deref(),
        Some(SESSION_ID),
        "the notice belongs to the prompt's session"
    );
    executed.notices[0].text.clone()
}

#[tokio::test]
async fn respond_params_have_exactly_four_keys() {
    let (mock, client, mut inbound) = MockGateway::start().await;
    let mut state = SessionState::empty(SessionId::new(SESSION_ID));

    // The turn runs at generation 5. The approval is from generation 2 and the question from
    // generation 3, so an answer built from the session's generation would show.
    mock.set_envelope_generation(5);
    mock.send_event("message.start", json!({"session_id": SESSION_ID}))
        .await;
    mock.approval_request("approval-gen", 2, "ls -la", &["once", "deny", "session"])
        .await;
    mock.clarify_request(
        "question-gen",
        3,
        "Which environment?",
        &["staging", "prod"],
        false,
    )
    .await;
    deliver(&mut inbound, &mut state, 3).await;
    assert_eq!(state.execution_generation(), Some(5));
    assert_eq!(state.open_prompts().count(), 2);

    let approval_key = key("approval-gen", 2);
    let clarify_key = key("question-gen", 3);

    let approval_effects = update(
        &mut state,
        Event::AnswerApproval {
            key: approval_key.clone(),
            choice: "session".to_owned(),
        },
    );
    let clarify_effects = update(
        &mut state,
        Event::AnswerClarify {
            key: clarify_key.clone(),
            answer: "prod".to_owned(),
        },
    );
    assert_eq!(
        approval_effects,
        vec![respond_approval(&approval_key, "session")]
    );
    assert_eq!(clarify_effects, vec![respond_clarify(&clarify_key, "prod")]);

    let mut service = PromptService::new(submission_ids());
    for effect in approval_effects.into_iter().chain(clarify_effects) {
        let executed = service.execute(&client, effect).await;
        assert!(executed.events.is_empty());
        assert!(executed.notices.is_empty());
    }

    let approval = mock.wait_for_request("approval.respond", 0).await;
    assert_eq!(
        key_names(&approval.params),
        names(&["session_id", "execution_generation", "prompt_id", "choice"])
    );
    assert!(approval.params.get("profile").is_none());
    assert_eq!(
        approval.params,
        json!({
            "session_id": SESSION_ID,
            "execution_generation": 2,
            "prompt_id": "approval-gen",
            "choice": "session",
        })
    );

    let clarify = mock.wait_for_request("clarify.respond", 0).await;
    assert_eq!(
        key_names(&clarify.params),
        names(&["session_id", "execution_generation", "prompt_id", "answer"])
    );
    assert!(clarify.params.get("profile").is_none());
    assert_eq!(
        clarify.params,
        json!({
            "session_id": SESSION_ID,
            "execution_generation": 3,
            "prompt_id": "question-gen",
            "answer": "prod",
        })
    );

    assert_eq!(mock.requests("approval.respond").len(), 1);
    assert_eq!(mock.requests("clarify.respond").len(), 1);
    // The service only answers; the follow-up prompt is the caller's to send.
    assert!(mock.requests("prompt.submit").is_empty());
}

#[tokio::test]
async fn note_becomes_submit_event_only_after_resolved() {
    // A resolved reply turns the note into an ordinary prompt, once per answer.
    let (mock, client, _inbound) = MockGateway::start().await;
    let mut service = PromptService::new(submission_ids());
    let first = key("note-1", 1);
    service.set_note(&first, NOTE.to_owned());
    let executed = service
        .execute(&client, respond_approval(&first, "session"))
        .await;
    assert_eq!(
        executed.events,
        vec![Event::Submit {
            text: NOTE.to_owned(),
            submission_id: "sub-1".to_owned(),
        }]
    );
    assert!(executed.notices.is_empty());

    let second = key("note-2", 1);
    service.set_note(&second, "second note".to_owned());
    let executed = service
        .execute(&client, respond_approval(&second, "once"))
        .await;
    assert_eq!(
        executed.events,
        vec![Event::Submit {
            text: "second note".to_owned(),
            submission_id: "sub-2".to_owned(),
        }]
    );

    // An answer with no note gives no follow-up prompt.
    let plain = key("note-3", 1);
    let executed = service
        .execute(&client, respond_approval(&plain, "once"))
        .await;
    assert!(executed.events.is_empty());

    // Nothing was sent as a prompt by the service itself.
    assert!(mock.requests("prompt.submit").is_empty());
    assert_eq!(mock.requests("approval.respond").len(), 3);

    // already_resolved and every error keep the note back and say so in the notice.
    let (mock, client, _inbound) = MockGateway::start().await;
    let mut service = PromptService::new(submission_ids());

    mock.queue_respond_reply(Reply::AlreadyResolved);
    let held = key("note-4", 1);
    service.set_note(&held, NOTE.to_owned());
    let executed = service
        .execute(&client, respond_approval(&held, "once"))
        .await;
    assert!(
        executed
            .events
            .iter()
            .all(|event| !matches!(event, Event::Submit { .. })),
        "an answer found already resolved must not send the note"
    );
    assert_eq!(
        executed.events,
        vec![Event::AnswerAlreadyResolved { key: held.clone() }]
    );
    assert_eq!(
        only_notice(&executed),
        format!("answered elsewhere or timed out; note not sent: {NOTE}")
    );

    for reason in ["stale_generation", "invalid_params", "permission_denied"] {
        mock.queue_respond_reply(Reply::Error(reason));
        let failed = key(&format!("note-{reason}"), 1);
        service.set_note(&failed, NOTE.to_owned());
        let executed = service
            .execute(&client, respond_approval(&failed, "once"))
            .await;
        assert_eq!(
            executed.events,
            vec![Event::AnswerFailed {
                key: failed.clone()
            }],
            "a failed answer must not send the note ({reason})"
        );
        assert_eq!(
            only_notice(&executed),
            format!("could not answer: {reason}; note not sent: {NOTE}")
        );
    }
    assert!(mock.requests("prompt.submit").is_empty());
}

#[tokio::test]
async fn respond_failures_and_already_resolved() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let mut service = PromptService::new(submission_ids());

    // Gateway refusals reopen the card and say why, for both kinds of prompt.
    for reason in ["stale_generation", "invalid_params", "permission_denied"] {
        let approval = key(&format!("approval-{reason}"), 1);
        mock.queue_respond_reply(Reply::Error(reason));
        let executed = service
            .execute(&client, respond_approval(&approval, "once"))
            .await;
        assert_eq!(
            executed.events,
            vec![Event::AnswerFailed {
                key: approval.clone()
            }]
        );
        assert_eq!(
            only_notice(&executed),
            format!("could not answer: {reason}")
        );

        let clarify = key(&format!("clarify-{reason}"), 1);
        mock.queue_respond_reply(Reply::Error(reason));
        let executed = service
            .execute(&client, respond_clarify(&clarify, "prod"))
            .await;
        assert_eq!(
            executed.events,
            vec![Event::AnswerFailed {
                key: clarify.clone()
            }]
        );
        assert_eq!(
            only_notice(&executed),
            format!("could not answer: {reason}")
        );
    }

    // already_resolved: the reducer is told, and the person gets exactly one notice.
    for approval in [true, false] {
        mock.queue_respond_reply(Reply::AlreadyResolved);
        let prompt = key(if approval { "gone-a" } else { "gone-c" }, 1);
        let effect = if approval {
            respond_approval(&prompt, "once")
        } else {
            respond_clarify(&prompt, "prod")
        };
        let executed = service.execute(&client, effect).await;
        assert_eq!(
            executed.events,
            vec![Event::AnswerAlreadyResolved {
                key: prompt.clone()
            }]
        );
        assert_eq!(only_notice(&executed), "answered elsewhere or timed out");
        // The prompt's later settle event adds nothing.
        assert!(service.on_prompts_changed(&[]).is_empty());
    }

    // Refusals from the reducer send nothing. An answer given twice is silent; the rest say why.
    let frames_before = mock.frame_count();
    let executed = service
        .execute(
            &client,
            Effect::RefuseAnswer {
                key: key("refused", 1),
                reason: AnswerRefusal::AlreadyAnswered,
            },
        )
        .await;
    assert!(executed.events.is_empty());
    assert!(executed.notices.is_empty());
    for reason in [
        AnswerRefusal::NoSuchPrompt,
        AnswerRefusal::WrongKind,
        AnswerRefusal::ChoiceNotOffered,
    ] {
        let executed = service
            .execute(
                &client,
                Effect::RefuseAnswer {
                    key: key("refused", 1),
                    reason,
                },
            )
            .await;
        let text = only_notice(&executed);
        assert!(
            text.starts_with("could not answer: ") && text.len() > "could not answer: ".len(),
            "a refusal says why it could not answer"
        );
    }
    assert!(mock.no_new_frames_within(Duration::from_millis(100)).await);
    assert_eq!(mock.frame_count(), frames_before);

    // The connection ends while the answer is in flight: the outcome is unknown.
    mock.set_respond_reply(Reply::Silent);
    let lost = key("lost", 1);
    let nth = mock.requests("approval.respond").len();
    let (executed, ()) = tokio::join!(
        service.execute(&client, respond_approval(&lost, "once")),
        async {
            let _ = mock.wait_for_request("approval.respond", nth).await;
            mock.kill_connection().await;
        },
    );
    assert_eq!(
        executed.events,
        vec![Event::AnswerFailed { key: lost.clone() }]
    );
    assert_eq!(only_notice(&executed), "could not answer: connection lost");

    // No reply arrives before the call's own deadline: 300 seconds for an approval, and the
    // client's default for a question. The clock is paused so the wait costs no real time.
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.set_respond_reply(Reply::Silent);
    let mut service = PromptService::new(submission_ids());
    tokio::time::pause();

    let slow_approval = key("slow-approval", 1);
    let (executed, ()) = tokio::join!(
        service.execute(&client, respond_approval(&slow_approval, "once")),
        expire(&client, 299),
    );
    assert_eq!(
        executed.events,
        vec![Event::AnswerFailed {
            key: slow_approval.clone()
        }]
    );
    assert_eq!(only_notice(&executed), "could not answer: no reply in time");

    let slow_question = key("slow-question", 1);
    let (executed, ()) = tokio::join!(
        service.execute(&client, respond_clarify(&slow_question, "prod")),
        expire(&client, 119),
    );
    assert_eq!(
        executed.events,
        vec![Event::AnswerFailed {
            key: slow_question.clone()
        }]
    );
    assert_eq!(only_notice(&executed), "could not answer: no reply in time");
}

/// Wait for the one call to be pending, then move the paused clock past its deadline.
async fn expire(client: &Client, almost: u64) {
    let mut waited = 0u32;
    while client.stats().await.pending_calls != 1 {
        waited += 1;
        assert!(waited < 100_000, "the call never became pending");
        tokio::task::yield_now().await;
    }
    tokio::time::advance(Duration::from_secs(almost)).await;
    tokio::time::advance(Duration::from_secs(2)).await;
}

/// The mock and a client, and a state holding two approvals that are open, one that is being
/// answered, and one question. Returns the four keys in that order.
async fn four_prompts() -> (
    MockGateway,
    Client,
    InboundStream,
    SessionState,
    Vec<PromptKey>,
) {
    let (mock, client, mut inbound) = MockGateway::start().await;
    let mut state = SessionState::empty(SessionId::new(SESSION_ID));
    let offered = ["once", "deny", "session", "always"];
    mock.approval_request("deny-one", 1, "rm -rf build", &offered)
        .await;
    mock.approval_request("deny-two", 1, "rm -rf cache", &offered)
        .await;
    mock.approval_request("sending", 1, "git push --force", &offered)
        .await;
    mock.clarify_request(
        "question",
        1,
        "Which environment?",
        &["staging", "prod"],
        false,
    )
    .await;
    deliver(&mut inbound, &mut state, 4).await;
    let keys = vec![
        key("deny-one", 1),
        key("deny-two", 1),
        key("sending", 1),
        key("question", 1),
    ];
    let effects = update(
        &mut state,
        Event::AnswerApproval {
            key: keys[2].clone(),
            choice: "once".to_owned(),
        },
    );
    assert_eq!(effects, vec![respond_approval(&keys[2], "once")]);
    assert_eq!(state.open_prompts().count(), 4);
    (mock, client, inbound, state, keys)
}

fn deny_params(prompt_id: &str) -> Value {
    json!({
        "session_id": SESSION_ID,
        "execution_generation": 1,
        "prompt_id": prompt_id,
        "choice": "deny",
    })
}

fn assert_two_denies(mock: &MockGateway) {
    let denies = mock.requests("approval.respond");
    assert_eq!(denies.len(), 2, "one deny per open approval");
    let mut sent: Vec<Value> = denies.iter().map(|seen| seen.params.clone()).collect();
    sent.sort_by_key(|params| params["prompt_id"].to_string());
    assert_eq!(sent, vec![deny_params("deny-one"), deny_params("deny-two")]);
    assert!(
        mock.requests("clarify.respond").is_empty(),
        "questions are withdrawn by Hermes, not denied"
    );
}

#[tokio::test]
async fn deny_before_interrupt_awaits_replies() {
    let wait = Duration::from_secs(2);

    // Prompt by prompt: the two open approvals are denied, the one being answered and the
    // question are left alone, and the call returns as soon as both replies are in.
    let (mock, client, _inbound, state, keys) = four_prompts().await;
    let mut service = PromptService::new(submission_ids());
    assert!(service.on_prompts_changed(&keys).is_empty());
    let started = Instant::now();
    let notices = service
        .deny_before_interrupt(&client, &state, SESSION_ID, wait)
        .await;
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(notices.is_empty(), "both denies were confirmed");
    assert_two_denies(&mock);
    assert!(mock.no_new_frames_within(Duration::from_millis(100)).await);
    assert_eq!(mock.frame_count(), 2);
    // The denied approvals leave later without an "answered elsewhere" notice.
    assert!(
        service
            .on_prompts_changed(&[keys[2].clone(), keys[3].clone()])
            .is_empty()
    );

    // The denies go out together and the call waits for the slowest reply.
    let (mock, client, _inbound, state, keys) = four_prompts().await;
    let mut service = PromptService::new(submission_ids());
    assert!(service.on_prompts_changed(&keys).is_empty());
    mock.queue_respond_reply(Reply::ResolvedAfter(Duration::from_millis(500)));
    mock.queue_respond_reply(Reply::ResolvedAfter(Duration::from_millis(500)));
    let started = Instant::now();
    let notices = service
        .deny_before_interrupt(&client, &state, SESSION_ID, wait)
        .await;
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(450),
        "it must wait for the replies"
    );
    assert!(
        took < Duration::from_millis(900),
        "the two denies must not be sent one after the other"
    );
    assert!(notices.is_empty());
    assert_two_denies(&mock);
    let denies = mock.requests("approval.respond");
    let apart = denies[1].at.saturating_duration_since(denies[0].at);
    assert!(
        apart < Duration::from_millis(250),
        "the denies go out together"
    );

    // A gateway that never replies: the call returns when the wait is over, with one notice,
    // and the denied approvals still leave without a second one.
    let (mock, client, _inbound, state, keys) = four_prompts().await;
    mock.set_respond_reply(Reply::Silent);
    let mut service = PromptService::new(submission_ids());
    assert!(service.on_prompts_changed(&keys).is_empty());
    let started = Instant::now();
    let notices = service
        .deny_before_interrupt(&client, &state, SESSION_ID, wait)
        .await;
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(1900),
        "it waits the whole time"
    );
    assert!(took < Duration::from_millis(3500), "and then stops waiting");
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].text, "deny not confirmed; interrupting anyway");
    assert_two_denies(&mock);
    assert!(
        service
            .on_prompts_changed(&[keys[2].clone(), keys[3].clone()])
            .is_empty(),
        "a deny that was tried counts as answered here"
    );
}
