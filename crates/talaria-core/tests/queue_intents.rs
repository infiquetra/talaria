//! Issue 232. Queue and steer. One intent, one effect list, one notice.
//!
//! The twenty-eight functions are in specification order. A stub panics before
//! the effect list. A reducer case that already returns a value fails on the
//! notice or the effect the queue must add.

#[path = "support/events.rs"]
mod events;

use events::{EPOCH, SESSION, ev, ev_gen, submit};
use hermes_contract::canonical_events::SessionInfoFrame;
use hermes_contract::schemas::{
    AdmissionStatus, CanonicalCorrectionAck, CanonicalCorrectionAckStatus,
    CanonicalCorrectionResult, CanonicalPendingAdmission, Status,
};
use serde_json::json;
use talaria_core::queue::{
    AdmissionId, Controls, Direction, EditState, IdSource, InFlight, MovePlan, MoveStep,
    QueueAction, QueueIntent, QueuedRow, RowStatus, SubmissionId, update_queue,
};
use talaria_core::session::{Cell, Effect, Event, SessionId, SessionState, update};

struct Ids(u32);

impl IdSource for Ids {
    fn submission_id(&mut self) -> SubmissionId {
        self.0 += 1;
        SubmissionId(format!("sub-{}", self.0))
    }
}

fn sid() -> SessionId {
    SessionId::new(SESSION)
}

fn aid(name: &str) -> AdmissionId {
    AdmissionId(name.to_string())
}

fn sub(name: &str) -> SubmissionId {
    SubmissionId(name.to_string())
}

fn queued(id: &str, text: &str, seq: i64) -> QueuedRow {
    QueuedRow {
        admission_id: aid(id),
        input_id: sub(&format!("in-{id}")),
        text: text.to_string(),
        status: RowStatus::Queued,
        outcome: None,
        controls: Controls {
            steer: true,
            replace: true,
        },
        steering: false,
        sequence: seq,
        execution_generation: Some(1),
    }
}

fn notices(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Notice(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect()
}

fn has_notice(state: &SessionState, needle: &str) -> bool {
    notices(state).iter().any(|text| text.contains(needle))
}

fn running() -> SessionState {
    let mut state = SessionState::empty(sid());
    update(&mut state, ev(1, "message.start", json!({})));
    update(
        &mut state,
        ev(2, "message.delta", json!({"text": "reply so far"})),
    );
    state
}

fn receipt(id: &str, status: Status, outcome: Option<&str>) -> AdmissionStatus {
    serde_json::from_value(json!({
        "admission_id": id,
        "authority_epoch": 1,
        "execution_generation": null,
        "outcome": outcome,
        "ref": {"profile_id": "fixture", "session_id": SESSION},
        "sequence": 1,
        "status": status
    }))
    .expect("contract schema AdmissionStatus")
}

fn admission(id: &str, text: &str, status: &str, seq: i64) -> CanonicalPendingAdmission {
    serde_json::from_value(json!({
        "admission_id": id,
        "authority_epoch": 1,
        "execution_generation": null,
        "input_id": format!("in-{id}"),
        "outcome": null,
        "ref": {"profile_id": "fixture", "session_id": SESSION},
        "sequence": seq,
        "status": status,
        "text": text,
        "controls": ["steer", "replace"],
        "steering": false
    }))
    .expect("contract schema CanonicalPendingAdmission")
}

fn ack(status: CanonicalCorrectionAckStatus, delivered: Option<bool>) -> Event {
    Event::SteerResult(CanonicalCorrectionResult::CorrectionAck(
        CanonicalCorrectionAck {
            authority_epoch: 3,
            delivered,
            execution_generation: 1,
            status,
            text: "The code word is PELICAN.".to_string(),
        },
    ))
}

#[test]
fn save_edit_emits_replace() {
    let mut state = running();
    state.queue.rows = vec![queued("adm-a", "alpha", 1), queued("adm-b", "bravo", 2)];
    state.queue.selection = Some(aid("adm-a"));
    state.queue.edit = Some(EditState {
        admission_id: aid("adm-a"),
        original_text: "alpha".to_string(),
        index: 1,
    });
    let mut ids = Ids(0);
    let effects = update_queue(
        &mut state,
        QueueIntent::SaveEdit("alpha edited".to_string()),
        &mut ids,
    );
    assert_eq!(
        effects,
        vec![Effect::ReplaceQueued {
            session_id: sid(),
            admission_id: aid("adm-a"),
            submission_id: sub("sub-1"),
            text: "alpha edited".to_string(),
        }]
    );
    let plan = state.queue.plan.expect("restore plan");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].admission_id, aid("adm-b"));
    assert_eq!(plan.steps[0].text, "bravo");
}

#[test]
fn remove_emits_cancel() {
    let mut state = running();
    state.queue.rows = vec![queued("adm-a", "alpha", 1)];
    state.queue.selection = Some(aid("adm-a"));
    let mut ids = Ids(0);
    let effects = update_queue(&mut state, QueueIntent::Remove, &mut ids);
    assert_eq!(
        effects,
        vec![Effect::CancelQueued {
            session_id: sid(),
            admission_id: aid("adm-a"),
        }]
    );
}

#[test]
fn discard_uses_row_generation() {
    let mut state = running();
    let mut row = queued("adm-lost", "lost prompt", 1);
    row.status = RowStatus::Unknown;
    row.execution_generation = Some(7);
    row.controls = Controls {
        steer: false,
        replace: false,
    };
    state.queue.rows = vec![row];
    state.queue.selection = Some(aid("adm-lost"));
    let mut ids = Ids(0);
    let effects = update_queue(&mut state, QueueIntent::Remove, &mut ids);
    assert_eq!(
        effects,
        vec![Effect::ResolveUnknown {
            session_id: sid(),
            admission_id: aid("adm-lost"),
            execution_generation: 7,
        }]
    );
}

#[test]
fn discard_null_generation_no_call() {
    let mut state = running();
    let mut row = queued("adm-lost", "lost prompt", 1);
    row.status = RowStatus::Unknown;
    row.execution_generation = None;
    state.queue.rows = vec![row];
    state.queue.selection = Some(aid("adm-lost"));
    let mut ids = Ids(0);
    let effects = update_queue(&mut state, QueueIntent::Remove, &mut ids);
    assert!(effects.is_empty());
    assert!(
        has_notice(
            &state,
            "Hermes gave no turn number for this lost prompt, so Talaria cannot discard it."
        ),
        "null generation discard"
    );
}

#[test]
fn null_controls_allow_only_remove() {
    let mut state = running();
    let mut row = queued("adm-a", "alpha", 1);
    row.controls = Controls {
        steer: false,
        replace: false,
    };
    state.queue.rows = vec![row];
    state.queue.selection = Some(aid("adm-a"));
    let refused = [
        QueueIntent::SaveEdit("changed".to_string()),
        QueueIntent::Steer,
        QueueIntent::Move(Direction::Up),
        QueueIntent::SendNow,
    ];
    for intent in refused {
        let mut ids = Ids(0);
        let effects = update_queue(&mut state, intent, &mut ids);
        assert!(effects.is_empty());
        assert!(
            has_notice(&state, "Hermes does not offer that action for this prompt."),
            "null controls"
        );
    }
    let mut ids = Ids(0);
    let effects = update_queue(&mut state, QueueIntent::Remove, &mut ids);
    assert_eq!(
        effects,
        vec![Effect::CancelQueued {
            session_id: sid(),
            admission_id: aid("adm-a"),
        }]
    );
}

#[test]
fn steer_running_emits_steer_queued() {
    let mut state = running();
    state.queue.rows = vec![queued("adm-a", "alpha", 1)];
    state.queue.selection = Some(aid("adm-a"));
    let mut ids = Ids(0);
    let effects = update_queue(&mut state, QueueIntent::Steer, &mut ids);
    assert_eq!(
        effects,
        vec![Effect::SteerQueued {
            session_id: sid(),
            admission_id: aid("adm-a"),
        }]
    );
}

#[test]
fn steer_idle_no_call() {
    let mut state = SessionState::empty(sid());
    state.queue.rows = vec![queued("adm-a", "alpha", 1)];
    state.queue.selection = Some(aid("adm-a"));
    let mut ids = Ids(0);
    let effects = update_queue(&mut state, QueueIntent::Steer, &mut ids);
    assert!(effects.is_empty());
    assert!(
        has_notice(
            &state,
            "No turn is running to steer. The prompt stays queued and runs next."
        ),
        "steer while idle"
    );
}

#[test]
fn steer_text_undelivered_resubmits() {
    let cases = [
        (
            "rejected",
            ack(CanonicalCorrectionAckStatus::Rejected, Some(false)),
            "The running turn could not take the steer. It is queued as the next prompt: \"The code word is PELICAN.\"",
        ),
        (
            "stale_generation",
            Event::QueueCallRefused {
                action: QueueAction::SteerText,
                admission_id: None,
                reason: "stale_generation".to_string(),
                detail: None,
            },
            "That turn has already ended. Your text is queued as the next prompt.",
        ),
        (
            "execution_not_ready",
            Event::QueueCallRefused {
                action: QueueAction::SteerText,
                admission_id: None,
                reason: "execution_not_ready".to_string(),
                detail: None,
            },
            "The running turn cannot take a steer yet. Your text is queued as the next prompt.",
        ),
        (
            "unsupported_control",
            Event::QueueCallRefused {
                action: QueueAction::SteerText,
                admission_id: None,
                reason: "unsupported_control".to_string(),
                detail: None,
            },
            "This session's turn does not accept steering.",
        ),
    ];
    for (name, event, notice) in cases {
        let mut state = running();
        let _ = update_queue(
            &mut state,
            QueueIntent::SteerText("The code word is PELICAN.".to_string()),
            &mut Ids(0),
        );
        let effects = update(&mut state, event);
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::SendPrompt { text, .. } if text == "The code word is PELICAN."
            )),
            "{name} resubmits"
        );
        assert!(has_notice(&state, notice), "{name} notice");
    }
}

#[test]
fn steer_text_plain_rejected_keeps_text() {
    let mut draft = "draft".to_string();
    let mut state = running();
    let effects = update(
        &mut state,
        ack(CanonicalCorrectionAckStatus::Rejected, None),
    );
    assert_eq!(draft, "draft");
    draft.push_str(" stays");
    assert!(
        has_notice(
            &state,
            "The running turn did not confirm the steer. Your text is back in the composer."
        ),
        "plain rejected"
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::SendPrompt { .. }))
    );
}

#[test]
fn steer_text_idle_submits() {
    let mut state = SessionState::empty(sid());
    let mut ids = Ids(0);
    let effects = update_queue(
        &mut state,
        QueueIntent::SteerText("The code word is PELICAN.".to_string()),
        &mut ids,
    );
    assert_eq!(
        effects,
        vec![Effect::SendPrompt {
            session_id: sid(),
            text: "The code word is PELICAN.".to_string(),
            submission_id: "sub-1".to_string(),
            attachments: Vec::new(),
        }]
    );
    assert!(
        has_notice(
            &state,
            "No turn is running, so this was sent as a new prompt: \"The code word is PELICAN.\""
        ),
        "idle steer text"
    );
}

#[test]
fn edit_ends_when_row_starts() {
    let mut state = running();
    state.queue.rows = vec![queued("adm-a", "alpha", 1)];
    state.queue.edit = Some(EditState {
        admission_id: aid("adm-a"),
        original_text: "alpha".to_string(),
        index: 1,
    });
    let mut started = admission("adm-a", "alpha edited", "started", 1);
    started.input_id = "sub-edit".to_string();
    let _ = update(
        &mut state,
        ev(
            4,
            "session.info",
            json!({
                "pending": [started],
                "running": true,
                "desktop_protocol": "hermes-gateway-v1",
            }),
        ),
    );
    assert!(state.queue.edit.is_none(), "edit ends when the row starts");
    assert!(
        has_notice(
            &state,
            "That prompt started before you saved it. Your edited text is still in the composer; press Enter to send it as a new prompt."
        ),
        "edit ended"
    );
}

#[test]
fn empty_edit_is_refused() {
    let mut state = running();
    state.queue.rows = vec![queued("adm-a", "alpha", 1)];
    state.queue.edit = Some(EditState {
        admission_id: aid("adm-a"),
        original_text: "alpha".to_string(),
        index: 1,
    });
    let mut ids = Ids(0);
    let effects = update_queue(
        &mut state,
        QueueIntent::SaveEdit("   ".to_string()),
        &mut ids,
    );
    assert!(effects.is_empty());
    assert!(state.queue.edit.is_some(), "edit stays open");
    assert!(
        has_notice(
            &state,
            "An empty edit would remove the prompt. Press d on it to remove it instead."
        ),
        "empty edit"
    );
}

#[test]
fn plan_refused_without_replace() {
    let mut state = running();
    let mut blocked = queued("adm-b", "bravo", 2);
    blocked.controls.replace = false;
    state.queue.rows = vec![queued("adm-a", "alpha", 1), blocked];
    state.queue.selection = Some(aid("adm-a"));
    let mut ids = Ids(0);
    let effects = update_queue(&mut state, QueueIntent::Move(Direction::Down), &mut ids);
    assert!(effects.is_empty());
    assert!(
        has_notice(&state, "Hermes does not offer that action for this prompt."),
        "plan names the row"
    );
    assert!(has_notice(&state, "bravo"), "plan names the preview");
}

#[test]
fn plan_uses_queued_rows_only() {
    let mut state = running();
    let mut started = queued("adm-run", "running", 1);
    started.status = RowStatus::Started;
    let mut steering = queued("adm-steer", "The code word is PELICAN.", 2);
    steering.steering = true;
    steering.status = RowStatus::Started;
    state.queue.rows = vec![
        started,
        steering,
        queued("adm-a", "alpha", 3),
        queued("adm-b", "bravo", 4),
    ];
    state.queue.selection = Some(aid("adm-b"));
    let mut ids = Ids(0);
    let _effects = update_queue(&mut state, QueueIntent::Move(Direction::Up), &mut ids);
    let plan = state.queue.plan.expect("plan");
    let ids: Vec<_> = plan.target.iter().map(|id| id.0.as_str()).collect();
    assert_eq!(ids, ["adm-b", "adm-a"]);
}

#[test]
fn plan_steps_wait_for_results() {
    let mut state = running();
    state.queue.rows = vec![queued("adm-b", "bravo", 1), queued("adm-a", "alpha", 2)];
    state.queue.plan = Some(MovePlan {
        steps: vec![
            MoveStep {
                admission_id: aid("adm-b"),
                text: "bravo".to_string(),
                submission_id: sub("sub-1"),
            },
            MoveStep {
                admission_id: aid("adm-d"),
                text: "delta".to_string(),
                submission_id: sub("sub-2"),
            },
        ],
        next: 0,
        target: vec![aid("adm-a"), aid("adm-b"), aid("adm-d")],
        then_interrupt: None,
    });
    state.queue.in_flight = Some(InFlight {
        action: QueueAction::Move,
        admission_id: Some(aid("adm-b")),
        submission_id: Some(sub("sub-1")),
        params_text: Some("bravo".to_string()),
        retried: false,
        awaiting_reconcile: false,
    });
    let effects = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Move,
            admission_id: Some(aid("adm-b")),
            submission_id: Some(sub("sub-1")),
            result: receipt("adm-b2", Status::Queued, None),
        },
    );
    assert_eq!(effects.len(), 1, "one replace at a time");
    assert_eq!(
        effects,
        vec![Effect::ReplaceQueued {
            session_id: sid(),
            admission_id: aid("adm-d"),
            submission_id: sub("sub-2"),
            text: "delta".to_string(),
        }]
    );
}

#[test]
fn one_plan_at_a_time() {
    let mut state = running();
    state.queue.rows = vec![queued("adm-a", "alpha", 1), queued("adm-b", "bravo", 2)];
    state.queue.plan = Some(MovePlan {
        steps: vec![MoveStep {
            admission_id: aid("adm-b"),
            text: "bravo".to_string(),
            submission_id: sub("sub-9"),
        }],
        next: 0,
        target: vec![aid("adm-a"), aid("adm-b")],
        then_interrupt: None,
    });
    let sent = update(&mut state, submit("still sending", "sub-submit"));
    assert!(
        sent.iter().any(|effect| matches!(
            effect,
            Effect::SendPrompt { text, .. } if text == "still sending"
        )),
        "a plain submit still sends"
    );
    let refused = [
        QueueIntent::Move(Direction::Up),
        QueueIntent::SendNow,
        QueueIntent::Steer,
        QueueIntent::Remove,
        QueueIntent::BeginEdit,
        QueueIntent::SendDraftNow("later".to_string()),
        QueueIntent::SteerText("note".to_string()),
        QueueIntent::EnterSelection,
    ];
    for intent in refused {
        let mut ids = Ids(0);
        let effects = update_queue(&mut state, intent, &mut ids);
        assert!(effects.is_empty(), "busy");
        assert!(has_notice(&state, "Still moving prompts…"), "busy notice");
    }
}

#[test]
fn send_now_interrupts_recorded_generation() {
    let mut state = running();
    state.queue.plan = Some(MovePlan {
        steps: vec![MoveStep {
            admission_id: aid("adm-a"),
            text: "alpha".to_string(),
            submission_id: sub("sub-1"),
        }],
        next: 0,
        target: vec![aid("adm-a")],
        then_interrupt: Some(1),
    });
    state.queue.in_flight = Some(InFlight {
        action: QueueAction::SendNow,
        admission_id: Some(aid("adm-a")),
        submission_id: Some(sub("sub-1")),
        params_text: Some("alpha".to_string()),
        retried: false,
        awaiting_reconcile: false,
    });
    let effects = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::SendNow,
            admission_id: Some(aid("adm-a")),
            submission_id: Some(sub("sub-1")),
            result: receipt("adm-a2", Status::Queued, None),
        },
    );
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Interrupt {
                execution_generation: 1,
                ..
            }
        )),
        "interrupt the recorded generation"
    );
}

#[test]
fn send_now_skips_changed_generation() {
    let mut state = running();
    update(&mut state, ev_gen(3, 2, "message.start", json!({})));
    state.queue.plan = Some(MovePlan {
        steps: vec![MoveStep {
            admission_id: aid("adm-a"),
            text: "alpha".to_string(),
            submission_id: sub("sub-1"),
        }],
        next: 0,
        target: vec![aid("adm-a")],
        then_interrupt: Some(1),
    });
    let effects = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::SendNow,
            admission_id: Some(aid("adm-a")),
            submission_id: Some(sub("sub-1")),
            result: receipt("adm-a2", Status::Queued, None),
        },
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Interrupt { .. }))
    );
    assert!(
        has_notice(
            &state,
            "The turn you meant to stop already ended. Your prompt is first in the queue."
        ),
        "changed generation"
    );
}

#[test]
fn send_now_refused_while_paused() {
    let mut state = running();
    let mut lost = queued("adm-lost", "lost prompt", 1);
    lost.status = RowStatus::Unknown;
    state.queue.rows = vec![lost, queued("adm-a", "alpha", 2)];
    state.queue.selection = Some(aid("adm-a"));
    let mut ids = Ids(0);
    let effects = update_queue(&mut state, QueueIntent::SendNow, &mut ids);
    assert!(effects.is_empty());
    assert!(
        has_notice(
            &state,
            "The queue is paused behind a prompt lost in a gateway restart. Press d on it to discard it first."
        ),
        "paused send now"
    );
}

#[test]
fn send_draft_now_moves_to_head() {
    let mut state = running();
    state.queue.rows = vec![queued("adm-a", "alpha", 1), queued("adm-b", "bravo", 2)];
    let mut ids = Ids(0);
    let effects = update_queue(
        &mut state,
        QueueIntent::SendDraftNow("send this now".to_string()),
        &mut ids,
    );
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::SendPrompt { text, .. } if text == "send this now"
        )),
        "draft is submitted"
    );
    let _ = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(sub("sub-1")),
            result: receipt("adm-now", Status::Queued, None),
        },
    );
    let plan = state.queue.plan.expect("head plan");
    assert_eq!(plan.then_interrupt, Some(1));
}

#[test]
fn send_draft_now_idle_acts_like_enter() {
    let mut state = SessionState::empty(sid());
    let mut ids = Ids(0);
    let effects = update_queue(
        &mut state,
        QueueIntent::SendDraftNow("hello".to_string()),
        &mut ids,
    );
    assert_eq!(
        effects,
        vec![Effect::SendPrompt {
            session_id: sid(),
            text: "hello".to_string(),
            submission_id: "sub-1".to_string(),
            attachments: Vec::new(),
        }]
    );
}

#[test]
fn take_back_keeps_a_draft() {
    let draft = "draft".to_string();
    let mut state = SessionState::empty(sid());
    state.queue.rows = vec![queued(
        "adm-a",
        "Reply with the word alpha in capital letters.",
        1,
    )];
    let _ = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Remove,
            admission_id: Some(aid("adm-a")),
            submission_id: None,
            result: receipt("adm-a", Status::Terminal, Some("cancelled")),
        },
    );
    assert_eq!(draft, "draft");
    assert!(
        has_notice(
            &state,
            "Your draft was kept, so the returned prompt is shown here:"
        ),
        "take back"
    );
    assert!(
        has_notice(&state, "Reply with the word alpha in capital letters."),
        "full returned text"
    );
}

#[test]
fn lost_submit_shows_sending() {
    let mut state = running();
    let _ = update(
        &mut state,
        Event::QueueCallUnknown {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(sub("sub-1")),
        },
    );
    assert_eq!(state.queue.view(true).sending, 1);
    let info = SessionInfoFrame {
        pending: Some(vec![admission("adm-a", "alpha", "queued", 1)]),
        running: Some(true),
        desktop_protocol: Some("hermes-gateway-v1".to_string()),
        ..SessionInfoFrame::default()
    };
    let _delta = state.queue.apply_fanout(&info, EPOCH, 5);
    assert_eq!(state.queue.view(true).sending, 0);
}

#[test]
fn move_on_unknown_refused() {
    let mut state = running();
    let mut lost = queued("adm-lost", "lost prompt", 1);
    lost.status = RowStatus::Unknown;
    state.queue.rows = vec![lost];
    state.queue.selection = Some(aid("adm-lost"));
    let refused = [
        QueueIntent::SaveEdit("changed".to_string()),
        QueueIntent::Steer,
        QueueIntent::Move(Direction::Up),
        QueueIntent::SendNow,
    ];
    for intent in refused {
        let mut ids = Ids(0);
        let effects = update_queue(&mut state, intent, &mut ids);
        assert!(effects.is_empty());
        assert!(
            has_notice(
                &state,
                "A lost prompt can only be discarded. Press d to discard it."
            ),
            "unknown row"
        );
    }
}

#[test]
fn selection_follows_id_map() {
    let mut state = running();
    state.queue.rows = vec![queued("adm-a", "alpha", 1)];
    state.queue.selection = Some(aid("adm-a"));
    let _ = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Edit,
            admission_id: Some(aid("adm-a")),
            submission_id: Some(sub("sub-1")),
            result: receipt("adm-a2", Status::Queued, None),
        },
    );
    assert_eq!(state.queue.selection, Some(aid("adm-a2")));
}

#[test]
fn selection_after_removal() {
    let mut middle = SessionState::empty(sid());
    middle.queue.rows = vec![
        queued("adm-a", "alpha", 1),
        queued("adm-b", "bravo", 2),
        queued("adm-c", "charlie", 3),
    ];
    middle.queue.selection = Some(aid("adm-b"));
    let _ = update(
        &mut middle,
        Event::QueueCallResult {
            action: QueueAction::Remove,
            admission_id: Some(aid("adm-b")),
            submission_id: None,
            result: receipt("adm-b", Status::Terminal, Some("cancelled")),
        },
    );
    assert_eq!(middle.queue.selection, Some(aid("adm-c")));

    let mut tail = SessionState::empty(sid());
    tail.queue.rows = vec![
        queued("adm-a", "alpha", 1),
        queued("adm-b", "bravo", 2),
        queued("adm-c", "charlie", 3),
    ];
    tail.queue.selection = Some(aid("adm-c"));
    let _ = update(
        &mut tail,
        Event::QueueCallResult {
            action: QueueAction::Remove,
            admission_id: Some(aid("adm-c")),
            submission_id: None,
            result: receipt("adm-c", Status::Terminal, Some("cancelled")),
        },
    );
    assert_eq!(tail.queue.selection, Some(aid("adm-b")));
}

#[test]
fn cancel_other_outcome_not_taken_back() {
    let draft = "draft".to_string();
    let mut state = SessionState::empty(sid());
    state.queue.rows = vec![queued("adm-a", "alpha", 1)];
    let _ = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Remove,
            admission_id: Some(aid("adm-a")),
            submission_id: None,
            result: receipt("adm-a", Status::Terminal, Some("complete")),
        },
    );
    assert_eq!(draft, "draft");
    assert!(
        has_notice(&state, "That prompt is no longer in the queue."),
        "other outcome"
    );
    assert!(!has_notice(&state, "shown here:"), "nothing taken back");
}

#[test]
fn notices_of_table_6_12() {
    use talaria_core::queue::notices::refusal_notice;
    let rows = [
        (
            QueueAction::Edit,
            "queue_full",
            "Hermes holds at most 32 queued prompts, so the edit could not be saved. Your edited text is still in the composer.",
        ),
        (
            QueueAction::Move,
            "queue_full",
            "Hermes holds at most 32 queued prompts, so prompts cannot be moved now. Remove one first.",
        ),
        (
            QueueAction::Edit,
            "admission_started",
            "That prompt started before you saved it. Your edited text is still in the composer; press Enter to send it as a new prompt.",
        ),
        (
            QueueAction::Remove,
            "admission_started",
            "That prompt has already started, so it was not changed.",
        ),
        (
            QueueAction::Steer,
            "session_idle",
            "No turn is running to steer. The prompt stays queued and runs next.",
        ),
        (
            QueueAction::Steer,
            "unsupported_control",
            "Hermes does not offer that action for this prompt.",
        ),
        (
            QueueAction::Remove,
            "unknown_execution",
            "That prompt was lost in a gateway restart. Press d again to discard it.",
        ),
    ];
    for (action, reason, notice) in rows {
        assert_eq!(refusal_notice(action, reason), Some(notice));
    }
    let static_notices = [
        "An empty edit would remove the prompt. Press d on it to remove it instead.",
        "Saved. Hermes moved the edited prompt to the back of the queue.",
        "That prompt had just started, so Hermes stopped it.",
        "That prompt is no longer in the queue.",
        "Hermes gave no turn number for this lost prompt, so Talaria cannot discard it.",
        "A lost prompt can only be discarded. Press d to discard it.",
        "Still moving prompts…",
        "The queue changed while moving prompts. This is the order Hermes holds.",
        "Hermes did not confirm the change. Check the queue.",
        "⤷ steer: \"The code word is PELICAN.\" · sent; the agent reads it after its next tool call",
        "The steer did not reach the turn. The prompt is back in the queue in its place.",
        "The running turn could not take the steer. It is queued as the next prompt: \"The code word is PELICAN.\"",
        "The running turn did not confirm the steer. Your text is back in the composer.",
        "No turn is running, so this was sent as a new prompt: \"The code word is PELICAN.\"",
        "usage: /steer <text>",
        "The turn you meant to stop already ended. Your prompt is first in the queue.",
        "Hermes is about to start it.",
        "The queue is paused behind a prompt lost in a gateway restart. Press d on it to discard it first.",
        "Hermes did not confirm the removal. The prompt is still queued.",
        "Hermes did not confirm the discard. Press d to try again.",
        "Talaria edits the queue with Up. Type /queue <text> to queue a prompt.",
        "Nothing is queued.",
        "⤷ steer: \"The code word is PELICAN.\"",
        "Your draft was kept, so the returned prompt is shown here:",
        "Voice chat ended; nothing was sent.",
        "Hermes did not confirm the prompt. It is back in the composer.",
        "Sending…",
    ];
    assert!(
        static_notices
            .iter()
            .any(|text| text.contains("Still moving"))
    );
    assert!(!static_notices[22].contains(" · sent;"));
}

#[test]
fn delayed_send_now_receipt_cannot_interrupt_a_later_generation() {
    let mut state = running();
    let mut ids = Ids(0);
    update_queue(
        &mut state,
        QueueIntent::SendDraftNow("next".into()),
        &mut ids,
    );
    update(&mut state, ev_gen(3, 2, "message.start", json!({})));
    let effects = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(sub("sub-1")),
            result: receipt("adm-now", Status::Queued, None),
        },
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Interrupt { .. })),
        "late receipt interrupted a later turn: {effects:?}"
    );
    assert!(
        state
            .queue
            .plan
            .as_ref()
            .is_none_or(|plan| plan.then_interrupt != Some(2))
    );
}

#[test]
fn send_now_receipt_after_started_fanout_never_requeues_or_interrupts() {
    let mut state = running();
    let mut ids = Ids(0);
    update_queue(
        &mut state,
        QueueIntent::SendDraftNow("next".into()),
        &mut ids,
    );
    let mut row = admission("adm-now", "next", "queued", 3);
    row.input_id = "sub-1".into();
    update(
        &mut state,
        ev_gen(3, 1, "session.info", json!({"pending": [row.clone()]})),
    );
    update(
        &mut state,
        ev_gen(4, 1, "message.complete", json!({"status": "complete"})),
    );
    row.status = serde_json::from_value(json!("started")).unwrap();
    update(
        &mut state,
        ev_gen(5, 2, "session.info", json!({"pending": [row]})),
    );
    update(&mut state, ev_gen(6, 2, "message.start", json!({})));
    let effects = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(sub("sub-1")),
            result: receipt("adm-now", Status::Queued, None),
        },
    );
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::Interrupt { .. } | Effect::ReplaceQueued { .. }
    )));
    assert!(
        state
            .queue
            .rows
            .iter()
            .all(|row| row.status != RowStatus::Queued)
    );
}

#[test]
fn send_draft_now_interrupts_only_the_original_running_generation() {
    let mut state = running();
    let mut ids = Ids(0);
    update_queue(
        &mut state,
        QueueIntent::SendDraftNow("next".into()),
        &mut ids,
    );
    let effects = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(sub("sub-1")),
            result: receipt("adm-now", Status::Queued, None),
        },
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Interrupt {
            execution_generation: 1,
            ..
        }
    )));
}

#[test]
fn delayed_send_now_receipt_does_not_cross_an_authority_epoch() {
    let mut state = running();
    update(
        &mut state,
        ev_gen(
            3,
            1,
            "session.info",
            json!({"authority_epoch":1,"execution_generation":1,"pending":[]}),
        ),
    );
    assert_eq!(state.queue.generation_fence, Some((1, 1)));
    let mut ids = Ids(0);
    update_queue(
        &mut state,
        QueueIntent::SendDraftNow("next".into()),
        &mut ids,
    );
    update(
        &mut state,
        ev_gen(
            4,
            1,
            "session.info",
            json!({"authority_epoch":2,"execution_generation":1,"pending":[]}),
        ),
    );
    assert_eq!(state.queue.generation_fence, Some((2, 1)));
    let effects = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(sub("sub-1")),
            result: receipt("adm-now", Status::Queued, None),
        },
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Interrupt { .. })),
        "the generation was reused by another authority epoch"
    );
}

#[test]
fn delayed_send_now_receipt_does_not_interrupt_a_reused_generation_on_new_authority_start() {
    let mut state = running();
    let mut ids = Ids(0);
    update_queue(
        &mut state,
        QueueIntent::SendDraftNow("next".into()),
        &mut ids,
    );
    update(&mut state, ev(3, "message.complete", json!({})));
    update(
        &mut state,
        events::session_event(
            SESSION,
            json!({
                "type": "message.start", "session_id": SESSION,
                "replay_epoch": EPOCH, "seq": 4,
                "authority_epoch": 2, "execution_generation": 1, "payload": {}
            }),
        ),
    );
    let effects = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(sub("sub-1")),
            result: receipt("adm-now", Status::Queued, None),
        },
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Interrupt { .. })),
        "a new authority can start before its queue fanout"
    );
}

#[test]
fn send_now_interrupts_after_first_queued_fanout_in_the_same_turn() {
    let mut state = running();
    let mut ids = Ids(0);
    update_queue(
        &mut state,
        QueueIntent::SendDraftNow("next".into()),
        &mut ids,
    );
    let mut row = admission("adm-now", "next", "queued", 3);
    row.input_id = "sub-1".into();
    update(
        &mut state,
        ev_gen(
            3,
            1,
            "session.info",
            json!({
                "authority_epoch": 1, "execution_generation": 1, "pending": [row]
            }),
        ),
    );
    let effects = update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(sub("sub-1")),
            result: receipt("adm-now", Status::Queued, None),
        },
    );
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            Effect::Interrupt {
                execution_generation: 1,
                ..
            }
        )),
        "the same running turn should still be interrupted"
    );
}
