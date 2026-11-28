//! A refused turn that carries a usage limit opens a card; confirming it changes the model and
//! retries only after Hermes accepted the change.

use crate::mock_gateway;

use hermes_client::channel::Generation;
use hermes_client::liveness::LinkEvent;
use hermes_client::{Inbound, InboundEvent};
use hermes_contract::events::{EventFrame, Notification};
use serde_json::{Value, json};
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::OpenedSession;
use talaria_app::usage_limit::after_usage_limit_switch;
use talaria_core::mutation::{MutationOutcome, MutationRequest};
use talaria_core::rewind::RewindInput;
use talaria_core::session::{
    Cell, NoticeKind, UsageLimitCard, usage_limit_card, usage_limit_model_payload,
};

const TITLE: &str = "Foreground title";
const NOT_CONFIRMED: &str = "The model switch was not confirmed";

/// The `error_surface` of a refused turn: a message, a small model with no provider, a medium
/// model on `ollama-cloud`, and the `add_credits` action.
fn sample_detail() -> Value {
    json!({
        "code": "usage_limit",
        "layer": "provider",
        "retryable": false,
        "usage_limit": {
            "credits": {"action": "add_credits"},
            "current_model": "big-1",
            "gauge_id": "daily",
            "limit_name": "Daily",
            "message": "Daily limit reached",
            "scope": "account",
            "suggestions": [
                {"label": "Small", "note": "included", "model": "small-1"},
                {"label": "Medium", "note": "slower", "model": "med-1", "provider": "ollama-cloud"}
            ]
        }
    })
}

fn sample_card() -> UsageLimitCard {
    let mut card = usage_limit_card(&sample_detail()).expect("the sample makes a card");
    card.session_id = "local-session-1".to_owned();
    card
}

fn refused() -> MutationOutcome {
    MutationOutcome::Refused {
        reason: String::new(),
        field: None,
        detail: None,
        rpc_code: 0,
    }
}

fn committed() -> MutationOutcome {
    MutationOutcome::Committed(Value::Null)
}

#[test]
fn usage_limit_confirm_mutates_model_then_retries() {
    let card = sample_card();

    // The payload carries the model, and a provider only when the suggestion named one. It
    // never saves a default and never sets a reasoning effort.
    let payload = usage_limit_model_payload(&card);
    assert_eq!(payload.get("model"), Some(&json!("small-1")));
    assert!(!payload.contains_key("provider"), "{payload:?}");
    assert!(!payload.contains_key("save"), "{payload:?}");
    assert!(!payload.contains_key("reasoning"), "{payload:?}");

    let medium = UsageLimitCard {
        model: "med-1".to_owned(),
        provider: Some("ollama-cloud".to_owned()),
        ..card.clone()
    };
    let payload = usage_limit_model_payload(&medium);
    assert_eq!(payload.get("model"), Some(&json!("med-1")));
    assert_eq!(payload.get("provider"), Some(&json!("ollama-cloud")));
    assert!(!payload.contains_key("save"), "{payload:?}");
    assert!(!payload.contains_key("reasoning"), "{payload:?}");

    // A refused change retries nothing and leaves the card.
    let mut open = Some(card.clone());
    assert_eq!(after_usage_limit_switch(&mut open, &refused()), None);
    assert_eq!(open, Some(card.clone()));

    // So does every other reply that is not a commit.
    for outcome in [
        MutationOutcome::FenceRefused {
            reason: String::new(),
            revision: None,
            execution_generation: None,
        },
        MutationOutcome::Deferred(Value::Null),
        MutationOutcome::Unknown,
        MutationOutcome::NotSent {
            reason: String::new(),
        },
    ] {
        let mut open = Some(card.clone());
        assert_eq!(after_usage_limit_switch(&mut open, &outcome), None);
        assert_eq!(open, Some(card.clone()));
    }

    // A committed change retries once and clears the card.
    let mut open = Some(card.clone());
    assert_eq!(
        after_usage_limit_switch(&mut open, &committed()),
        Some(RewindInput::Retry {
            turn_running: false
        })
    );
    assert_eq!(open, None);
    assert_eq!(after_usage_limit_switch(&mut open, &committed()), None);

    // With no card open nothing is retried.
    let mut none: Option<UsageLimitCard> = None;
    assert_eq!(after_usage_limit_switch(&mut none, &committed()), None);
    assert_eq!(none, None);
}

// ---------------------------------------------------------------------------------------------
// The app
// ---------------------------------------------------------------------------------------------

/// The frame the client delivers for `raw`: the decoded one, or, for a payload the contract
/// does not know (a `status` it has no name for), an unknown notification that keeps the raw
/// params.
fn frame_of(raw: &Value) -> EventFrame {
    EventFrame::from_params(raw).unwrap_or_else(|_| EventFrame {
        session_id: raw["session_id"].as_str().map(str::to_owned),
        seq: raw["seq"].as_u64(),
        replay_epoch: raw["replay_epoch"].as_str().map(str::to_owned),
        authority_epoch: None,
        execution_generation: None,
        admission_id: None,
        notification: Notification::Unknown(raw.clone()),
    })
}

/// An app with one foreground session and the events it receives, numbered from the cursor.
struct Rig {
    state: AppState,
    epoch: String,
    next_seq: u64,
}

impl Rig {
    fn new() -> Self {
        let snapshot = serde_json::from_value(mock_gateway::recorded_resume_snapshot())
            .expect("a contract-shaped snapshot");
        let mut state = AppState::new("usage-limit-test");
        update(
            &mut state,
            AppMessage::Opened {
                opened: OpenedSession::from_snapshot(snapshot),
                notice: None,
            },
        );
        let opened = state.opened.as_mut().expect("the session opened");
        opened.title = Some(TITLE.to_owned());
        let epoch = opened.cursor.replay_epoch.clone();
        let next_seq = opened.cursor.last_sequence + 1;
        Self {
            state,
            epoch,
            next_seq,
        }
    }

    fn session_id(&self) -> String {
        self.state
            .opened
            .as_ref()
            .expect("a foreground session")
            .session_id
            .clone()
    }

    fn event(&mut self, kind: &str, payload: Value) -> Vec<AppEffect> {
        let raw = json!({
            "session_id": self.session_id(),
            "replay_epoch": self.epoch,
            "seq": self.next_seq,
            "type": kind,
            "payload": payload,
        });
        self.next_seq += 1;
        self.deliver(raw)
    }

    fn deliver(&mut self, raw: Value) -> Vec<AppEffect> {
        update(
            &mut self.state,
            AppMessage::Link(LinkEvent::Inbound {
                item: Inbound::Event(InboundEvent {
                    generation: Generation(1),
                    frame: frame_of(&raw),
                    raw,
                }),
                replayed: false,
            }),
        )
    }

    fn open_snapshot(&mut self, snapshot: Value) {
        let opened = OpenedSession::from_snapshot(
            serde_json::from_value(snapshot).expect("a contract-shaped snapshot"),
        );
        self.epoch = opened.cursor.replay_epoch.clone();
        self.next_seq = opened.cursor.last_sequence + 1;
        update(
            &mut self.state,
            AppMessage::Opened {
                opened,
                notice: None,
            },
        );
    }

    /// A new turn that ends with `status` and the given error surface.
    fn failed_turn(&mut self, status: &str, detail: &Value) -> Vec<AppEffect> {
        self.event("message.start", json!({}));
        self.event(
            "message.complete",
            json!({"status": status, "error": "limit", "error_surface": detail}),
        )
    }

    fn user(&mut self, action: UserAction) -> Vec<AppEffect> {
        update(&mut self.state, AppMessage::User(action))
    }

    fn reply(&mut self, request: MutationRequest, outcome: MutationOutcome) -> Vec<AppEffect> {
        update(&mut self.state, AppMessage::Mutated { request, outcome })
    }

    fn title(&self) -> Option<String> {
        self.state.opened.as_ref().and_then(|o| o.title.clone())
    }

    fn error_notices(&self) -> Vec<String> {
        self.state
            .opened
            .as_ref()
            .expect("a foreground session")
            .state
            .cells()
            .iter()
            .filter_map(|cell| match cell {
                Cell::Notice(notice) if notice.kind == NoticeKind::Error => {
                    Some(notice.text.clone())
                }
                _ => None,
            })
            .collect()
    }

    fn card(&self) -> Option<UsageLimitCard> {
        self.state.usage_limit.clone()
    }

    fn viewed_card(&self) -> Option<UsageLimitCard> {
        self.state.view().usage_limit.cloned()
    }
}

fn mutations(effects: &[AppEffect]) -> Vec<(MutationRequest, String)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::Mutate {
                request,
                session_id,
            } => Some((request.clone(), session_id.clone())),
            _ => None,
        })
        .collect()
}

fn committed_lines(effects: &[AppEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::CommitLines(lines) => Some(lines.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// A rig with the card open after a failed turn.
fn open_card() -> Rig {
    let mut rig = Rig::new();
    rig.failed_turn("error", &sample_detail());
    assert!(rig.card().is_some(), "the failed turn opens the card");
    rig
}

#[test]
fn failed_error_opens_the_card_and_confirm_does_not_rename() {
    let mut rig = Rig::new();
    let session = rig.session_id();
    assert_eq!(rig.card(), None);
    assert_eq!(rig.state.usage_limit_request, None);

    // A status that is not exactly `error` never opens the card, even with the same detail.
    rig.failed_turn("weird", &sample_detail());
    assert_eq!(rig.card(), None);
    assert_eq!(rig.viewed_card(), None);

    // Status `error` with a usage-limit detail opens it, for the session in front, and the
    // error notice stays.
    rig.failed_turn("error", &sample_detail());
    let card = rig.card().expect("status error opens the card");
    assert_eq!(card.session_id, session);
    assert_eq!(card.title, "Limit reached");
    assert_eq!(card.confirm.as_deref(), Some("Switch to Small (included)"));
    assert_eq!(card.model, "small-1");
    assert_eq!(rig.viewed_card(), Some(card.clone()));
    assert!(
        rig.error_notices().iter().any(|text| text == "limit"),
        "the error notice stays: {:?}",
        rig.error_notices()
    );

    // A completion that arrives for the finished turn leaves the open card in place.
    rig.event(
        "message.complete",
        json!({"status": "complete", "text": "Done."}),
    );
    assert_eq!(rig.card(), Some(card.clone()));
    assert_eq!(rig.viewed_card(), Some(card.clone()));

    // Dismiss clears the card, sends nothing and keeps the notice.
    let effects = rig.user(UserAction::UsageLimit { confirm: false });
    assert!(mutations(&effects).is_empty());
    assert_eq!(rig.card(), None);
    assert_eq!(rig.viewed_card(), None);
    assert_eq!(rig.state.usage_limit_request, None);
    assert!(rig.error_notices().iter().any(|text| text == "limit"));

    // Confirm sends the model change for the card's session and remembers the request.
    rig.failed_turn("error", &sample_detail());
    assert!(rig.card().is_some());
    let effects = rig.user(UserAction::UsageLimit { confirm: true });
    let sent = mutations(&effects);
    assert_eq!(sent.len(), 1, "confirm sends exactly one mutation");
    let (request, effect_session) = sent[0].clone();
    assert_eq!(effect_session, session);
    assert_eq!(request.session_id, session);
    assert_eq!(request.operation, "model");
    assert_eq!(request.payload.get("model"), Some(&json!("small-1")));
    assert!(!request.payload.contains_key("provider"));
    assert!(!request.payload.contains_key("save"));
    assert_eq!(rig.state.usage_limit_request, Some(request.clone()));
    assert!(rig.card().is_some(), "the card stays until Hermes answers");

    // While one switch is outstanding, confirm does nothing.
    let again = rig.user(UserAction::UsageLimit { confirm: true });
    assert!(mutations(&again).is_empty());
    assert_eq!(rig.state.usage_limit_request, Some(request.clone()));

    // The committed reply is a model change: no rename line, no new title, no card.
    let effects = rig.reply(request, committed());
    assert!(
        committed_lines(&effects)
            .iter()
            .all(|line| !line.contains("Renamed to")),
        "{:?}",
        committed_lines(&effects)
    );
    assert_eq!(rig.title().as_deref(), Some(TITLE));
    assert_eq!(rig.card(), None);
    assert_eq!(rig.state.usage_limit_request, None);

    // A refused reply keeps the card and says the switch was not confirmed.
    let mut rig = open_card();
    let effects = rig.user(UserAction::UsageLimit { confirm: true });
    let (request, _) = mutations(&effects).remove(0);
    let effects = rig.reply(request, refused());
    let lines = committed_lines(&effects);
    assert!(
        lines.iter().any(|line| line
            .trim_start_matches('\u{b7}')
            .trim_start()
            .starts_with(NOT_CONFIRMED)),
        "{lines:?}"
    );
    assert!(lines.iter().all(|line| !line.contains("Renamed to")));
    assert!(
        mutations(&effects).is_empty(),
        "a refused switch is not resent"
    );
    assert_eq!(rig.title().as_deref(), Some(TITLE));
    assert!(rig.card().is_some(), "the card stays after a refusal");
    assert_eq!(rig.state.usage_limit_request, None);

    // A new turn for that session clears an open card.
    rig.event("message.start", json!({}));
    assert_eq!(rig.card(), None);
    assert_eq!(rig.viewed_card(), None);
}

#[test]
fn cross_session_committed_switch_preserves_the_new_foreground_card() {
    let mut rig = open_card();
    let first_session = rig.session_id();
    let confirmation = rig.user(UserAction::UsageLimit { confirm: true });
    let [(request, target)]: [(MutationRequest, String); 1] =
        mutations(&confirmation).try_into().unwrap();
    assert_eq!(target, first_session);
    assert_eq!(request.session_id, first_session);

    let second_session = "local-session-2";
    rig.open_snapshot(second_session_snapshot());
    assert_eq!(rig.session_id(), second_session);

    let mut second_detail = sample_detail();
    second_detail["usage_limit"]["suggestions"][0]["model"] = json!("small-2");
    let refused_turn = rig.failed_turn("error", &second_detail);
    assert!(mutations(&refused_turn).is_empty());
    let second_card = rig
        .viewed_card()
        .expect("the second session shows its own card");
    assert_eq!(second_card.session_id, second_session);
    assert_eq!(second_card.model, "small-2");

    let late_reply = rig.reply(request, committed());
    let retry_sessions: Vec<_> = late_reply
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::Rewind {
                owner,
                call: talaria_core::rewind::Call::ReadSession,
                ..
            } => Some(owner.session_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        (rig.viewed_card(), retry_sessions),
        (Some(second_card), Vec::new()),
        "the first session's committed reply must preserve the second session's card without starting a retry"
    );
}

fn second_session_snapshot() -> Value {
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!("local-session-2");
    snapshot["stored_session_id"] = json!("stored-2");
    snapshot
}

#[test]
fn cross_session_turn_started_in_background_retires_the_old_card() {
    let mut rig = open_card();
    let first_session = rig.session_id();
    let epoch = rig.epoch.clone();
    let sequence = rig.next_seq;
    rig.open_snapshot(second_session_snapshot());
    assert_ne!(rig.session_id(), first_session);
    assert_eq!(rig.viewed_card(), None);

    rig.deliver(json!({
        "session_id": first_session,
        "replay_epoch": epoch,
        "seq": sequence,
        "type": "message.start",
        "payload": {},
    }));

    // The retained cursor must reject that sequence a second time and accept the next one.
    // Completed side answers survive snapshot replacement, making the cursor visible here.
    for (seq, question) in [(sequence, "duplicate"), (sequence + 1, "next")] {
        rig.deliver(json!({
            "session_id": first_session,
            "replay_epoch": epoch,
            "seq": seq,
            "type": "btw.complete",
            "payload": {"task_id": question, "question": question, "text": "answer"},
        }));
    }

    let mut running_snapshot = mock_gateway::data_snapshot("running_turn.json");
    running_snapshot["replay_epoch"] = json!(epoch);
    running_snapshot["last_sequence"] = json!(sequence + 1);
    rig.open_snapshot(running_snapshot);
    assert_eq!(rig.session_id(), first_session);
    assert!(rig.state.view().turn_running());
    let side_questions: Vec<_> = rig
        .state
        .opened
        .as_ref()
        .expect("the first session is foreground")
        .state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Side(side) => Some(side.question.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        side_questions,
        ["next"],
        "the retained turn start advanced the cursor"
    );
    assert_eq!(
        rig.viewed_card(),
        None,
        "a new turn in the retained session supersedes its old offer"
    );
}

#[test]
fn a_switch_sent_for_a_closed_card_never_settles_the_next_card() {
    let mut rig = open_card();
    let session = rig.session_id();
    let effects = rig.user(UserAction::UsageLimit { confirm: true });
    let (request, _) = mutations(&effects).remove(0);

    // The operator dismisses that card, and a later turn in the same session is refused too.
    rig.user(UserAction::UsageLimit { confirm: false });
    let mut later = sample_detail();
    later["usage_limit"]["suggestions"][0]["model"] = json!("small-3");
    rig.failed_turn("error", &later);
    let next_card = rig
        .viewed_card()
        .expect("the later refusal opens its own card");
    assert_eq!(next_card.session_id, session);
    assert_eq!(next_card.model, "small-3");

    // The first switch commits late: the later card stays and nothing is retried.
    let effects = rig.reply(request, committed());
    let retries = effects
        .iter()
        .filter(|effect| matches!(effect, AppEffect::Rewind { .. }))
        .count();
    assert_eq!(
        retries, 0,
        "a closed card's switch started a retry: {effects:?}"
    );
    assert_eq!(rig.viewed_card(), Some(next_card));
    assert!(
        committed_lines(&effects)
            .iter()
            .any(|line| line.contains("the turn was not retried")),
        "{:?}",
        committed_lines(&effects)
    );
    assert_eq!(rig.state.usage_limit_request, None);
}
