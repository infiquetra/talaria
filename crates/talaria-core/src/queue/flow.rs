//! The reducer's side of the queue: fanouts, receipts, refusals, lost replies and cells.
//!
//! Issue 232. Queue and steer. [`crate::session::update`] calls into this module. Nothing here
//! reads a clock or does input or output. The calls themselves leave as effects.

use hermes_contract::canonical_events::SessionInfoFrame;
use hermes_contract::schemas::{
    AdmissionStatus, CanonicalCorrectionAck, CanonicalCorrectionAckStatus,
    CanonicalCorrectionResult, Status,
};
use serde_json::Value;

use super::notices::{self, preview, refusal_notice};
use super::{
    AdmissionId, InFlight, LedgerEntry, MovePlan, QueueAction, QueuedRow, RowStatus, SubmissionId,
};
use crate::session::{Effect, Event, NoticeKind, SessionState, TurnState};

/// Append a final notice cell.
pub(crate) fn note(state: &mut SessionState, kind: NoticeKind, text: String) {
    state.notice(kind, text, None);
}

/// The generation of the running turn, when a turn runs and its generation is known.
pub(crate) fn running_generation(state: &SessionState) -> Option<i64> {
    if state.turn == TurnState::Running {
        state.execution_generation
    } else {
        None
    }
}

/// Take the interrupt transition of [`Event::Interrupt`].
pub(crate) fn interrupt_now(state: &mut SessionState) -> Vec<Effect> {
    crate::session::reducer::interrupt(state)
}

/// Submit text exactly as [`Event::Submit`] does.
pub(crate) fn submit_text(
    state: &mut SessionState,
    text: &str,
    submission_id: &str,
) -> Vec<Effect> {
    crate::session::reducer::submit(state, text, text, submission_id, &[])
}

/// Record a submit of this client. `placed` is true when the idle rule drew its cell already.
pub(crate) fn note_submit(state: &mut SessionState, text: &str, submission_id: &str, placed: bool) {
    state.queue.ledger.insert(
        SubmissionId(submission_id.to_string()),
        LedgerEntry {
            text: text.to_string(),
            admission_id: None,
            attachments: Vec::new(),
            cell_drawn: placed,
            awaiting_reconcile: false,
            retried: false,
        },
    );
}

/// A submit will start no turn: forget it.
pub(crate) fn submit_failed(state: &mut SessionState, submission_id: &str) {
    state
        .queue
        .ledger
        .remove(&SubmissionId(submission_id.to_string()));
    state
        .queue
        .fifo_of
        .retain(|_, waiting| waiting != submission_id);
}

/// The notice for a resumed `steer` row. The wrapper lines Hermes adds are cut off.
pub(crate) fn history_steer(content: &str) -> String {
    let mut lines: Vec<&str> = content.lines().collect();
    if lines
        .first()
        .is_some_and(|line| line.starts_with("[OUT-OF-BAND USER MESSAGE"))
    {
        lines.remove(0);
    }
    if lines
        .last()
        .is_some_and(|line| line.trim_end() == "[/OUT-OF-BAND USER MESSAGE]")
    {
        lines.pop();
    }
    format!("⤷ steer: \"{}\"", preview(&lines.join("\n")))
}

/// A `session.info` frame. Only a frame with `pending` touches the queue.
pub(crate) fn session_info(
    state: &mut SessionState,
    payload: &Value,
    replay_epoch: Option<&str>,
    seq: Option<u64>,
) -> Vec<Effect> {
    if payload.get("pending").is_none_or(Value::is_null) {
        return Vec::new();
    }
    let Ok(info) = SessionInfoFrame::from_payload(payload) else {
        if !state.queue.decode_noted {
            state.queue.decode_noted = true;
            note(
                state,
                NoticeKind::Diagnostic,
                "Hermes sent a queue update this version of Talaria could not read. The last known queue is kept."
                    .to_string(),
            );
        }
        return Vec::new();
    };
    let (epoch, seq) = match (replay_epoch, seq) {
        (Some(epoch), Some(seq)) if !epoch.is_empty() => (epoch, seq),
        _ => ("", 0),
    };
    let waiting: Vec<SubmissionId> = state
        .queue
        .ledger
        .iter()
        .filter(|(_, entry)| entry.awaiting_reconcile)
        .map(|(key, _)| key.clone())
        .collect();
    let edit_before = state.queue.edit.clone();
    let delta = state.queue.apply_fanout(&info, epoch, seq);
    if !delta.applied {
        return Vec::new();
    }
    for row in &delta.started {
        draw_started_row(state, row);
    }
    for _ in &delta.returned_from_steer {
        note(
            state,
            NoticeKind::System,
            notices::STEER_RETURNED.to_string(),
        );
    }
    note_foreign_steers(state);
    if let Some(edit) = edit_before
        && state.queue.edit.is_none()
    {
        let started = state.queue.row(&edit.admission_id).is_some();
        let text = if started {
            notices::EDIT_STARTED
        } else {
            notices::NOT_IN_QUEUE
        };
        note(state, NoticeKind::System, text.to_string());
    }
    let mut effects = Vec::new();
    for key in &waiting {
        effects.extend(settle_submit(state, key));
    }
    effects.extend(reconcile_in_flight(state));
    check_verify(state);
    effects
}

/// A steer this client did not send shows as a notice, once. Its own steers show theirs when
/// Hermes answers the call, so a steering row it is waiting on is skipped.
fn note_foreign_steers(state: &mut SessionState) {
    let steering: Vec<QueuedRow> = state
        .queue
        .rows
        .iter()
        .filter(|row| row.steering && row.status == RowStatus::Started)
        .cloned()
        .collect();
    for row in steering {
        if !state.queue.steer_noted.insert(row.admission_id.clone()) {
            continue;
        }
        let own = state.queue.in_flight.as_ref().is_some_and(|call| {
            call.action == QueueAction::Steer
                && call.admission_id.as_ref() == Some(&row.admission_id)
        });
        if !own {
            let text = format!("⤷ steer: \"{}\"", preview(&row.text));
            note(state, NoticeKind::System, text);
        }
    }
}

/// The user cell of a row the fanout shows `started`, when this client owes one.
fn draw_started_row(state: &mut SessionState, row: &QueuedRow) {
    let id = &row.admission_id;
    if state.queue.drawn.contains(id) {
        forget_waiting(state, id);
        return;
    }
    if let Some(entry) = state.queue.ledger.remove(&row.input_id) {
        state
            .queue
            .fifo_of
            .insert(id.clone(), row.input_id.0.clone());
        state.queue.drawn.insert(id.clone());
        if !entry.cell_drawn {
            let display = state
                .submissions
                .iter()
                .find(|entry| entry.id == row.input_id.0)
                .map_or_else(|| row.text.clone(), |entry| entry.text.clone());
            state.user(display);
        }
        forget_waiting(state, id);
    } else if state.queue.known.contains(id) {
        state.queue.drawn.insert(id.clone());
        state.user(row.text.clone());
    }
}

/// `message.start`: draw the user cell of the prompt that starts, matched by admission.
///
/// Without an envelope admission, or with one this client has never heard of, the oldest waiting
/// submit of this client is taken, as it was before the queue was tracked.
pub(crate) fn start_cell(state: &mut SessionState, admission: Option<&str>) {
    let Some(admission) = admission.filter(|admission| !admission.is_empty()) else {
        start_from_waiting(state, None);
        return;
    };
    let id = AdmissionId(admission.to_string());
    if state.queue.drawn.contains(&id) {
        forget_waiting(state, &id);
        return;
    }
    let own = state
        .queue
        .ledger
        .iter()
        .find(|(_, entry)| entry.admission_id.as_ref() == Some(&id))
        .map(|(key, _)| key.clone());
    if let Some(key) = own
        && let Some(entry) = state.queue.ledger.remove(&key)
    {
        state.queue.fifo_of.insert(id.clone(), key.0.clone());
        state.queue.drawn.insert(id.clone());
        if !entry.cell_drawn {
            let text = state
                .submissions
                .iter()
                .find(|entry| entry.id == key.0)
                .map(|entry| entry.text.clone())
                .unwrap_or_else(|| {
                    state
                        .queue
                        .row(&id)
                        .map_or(entry.text, |row| row.text.clone())
                });
            state.user(text);
        }
        forget_waiting(state, &id);
        return;
    }
    if state.queue.known.contains(&id) {
        let text = state.queue.row(&id).map(|row| row.text.clone());
        if let Some(text) = text {
            state.queue.drawn.insert(id.clone());
            state.user(text);
            forget_waiting(state, &id);
        }
        return;
    }
    start_from_waiting(state, Some(id));
}

/// Take the oldest waiting submit. It owns the start, whatever the start's admission is.
fn start_from_waiting(state: &mut SessionState, admission: Option<AdmissionId>) {
    let Some(entry) = state.submissions.pop_front() else {
        return;
    };
    if !entry.placed {
        state.user(entry.text);
    }
    let key = SubmissionId(entry.id.clone());
    if let Some(own) = state.queue.ledger.get_mut(&key) {
        // The cell exists now. The receipt or the fanout that names it later draws nothing.
        own.cell_drawn = true;
        if let Some(admission) = &admission {
            own.admission_id = Some(admission.clone());
        }
    }
    state
        .queue
        .fifo_of
        .retain(|_, waiting| *waiting != entry.id);
    if let Some(admission) = admission {
        state.queue.drawn.insert(admission);
    }
}

/// The waiting submit that became `id` is no longer waiting.
fn forget_waiting(state: &mut SessionState, id: &AdmissionId) {
    if let Some(waiting) = state.queue.fifo_of.remove(id) {
        state.submissions.retain(|entry| entry.id != waiting);
    }
}

/// One of the queue call events. Anything else changes nothing.
pub(crate) fn on_event(state: &mut SessionState, event: &Event) -> Vec<Effect> {
    match event {
        Event::QueueCallResult {
            action,
            admission_id,
            submission_id,
            result,
        } => {
            let action = real_action(
                state,
                *action,
                admission_id.as_ref(),
                submission_id.as_ref(),
            );
            on_result(state, action, admission_id, submission_id, result)
        }
        Event::QueueCallRefused {
            action,
            admission_id,
            reason,
            detail,
        } => {
            let action = real_action(state, *action, admission_id.as_ref(), None);
            on_refused(state, action, reason, detail.as_deref())
        }
        Event::QueueCallUnknown {
            action,
            admission_id,
            submission_id,
        } => {
            let action = real_action(
                state,
                *action,
                admission_id.as_ref(),
                submission_id.as_ref(),
            );
            on_unknown(state, action, admission_id, submission_id)
        }
        Event::SteerResult(result) => on_steer_result(state, result),
        Event::QueueReconcileDue {
            action,
            submission_id,
            ..
        } => on_due(state, *action, submission_id.as_ref()),
        _ => Vec::new(),
    }
}

/// The service cannot tell an edit save from a move or send-now step, so it reports every
/// replace as an edit. The call in flight knows which it was.
fn real_action(
    state: &SessionState,
    reported: QueueAction,
    admission_id: Option<&AdmissionId>,
    submission_id: Option<&SubmissionId>,
) -> QueueAction {
    let replace = |action| {
        matches!(
            action,
            QueueAction::Edit | QueueAction::Move | QueueAction::SendNow
        )
    };
    if !replace(reported) {
        return reported;
    }
    match &state.queue.in_flight {
        Some(call)
            if replace(call.action)
                && (submission_id.is_some() && call.submission_id.as_ref() == submission_id
                    || submission_id.is_none() && call.admission_id.as_ref() == admission_id) =>
        {
            call.action
        }
        _ => reported,
    }
}

fn on_result(
    state: &mut SessionState,
    action: QueueAction,
    admission_id: &Option<AdmissionId>,
    submission_id: &Option<SubmissionId>,
    result: &AdmissionStatus,
) -> Vec<Effect> {
    let receipt = AdmissionId(result.admission_id.clone());
    match action {
        QueueAction::Submit => on_submit_receipt(state, submission_id.as_ref(), receipt, result),
        QueueAction::Edit | QueueAction::Move | QueueAction::SendNow => replaced(
            state,
            action,
            admission_id.clone(),
            submission_id.clone(),
            receipt,
            result.status == Status::Queued,
        ),
        QueueAction::Remove => {
            let id = admission_id.clone().unwrap_or(receipt);
            on_cancel_receipt(state, id, result)
        }
        QueueAction::Discard => {
            state.queue.in_flight = None;
            let id = admission_id.clone().unwrap_or(receipt);
            take_row(state, &id);
            Vec::new()
        }
        QueueAction::Steer => {
            let id = admission_id.clone().unwrap_or(receipt);
            on_steer_queued_receipt(state, &id);
            Vec::new()
        }
        QueueAction::SteerText => Vec::new(),
    }
}

fn on_submit_receipt(
    state: &mut SessionState,
    submission_id: Option<&SubmissionId>,
    receipt: AdmissionId,
    result: &AdmissionStatus,
) -> Vec<Effect> {
    let Some(key) = submission_id else {
        return Vec::new();
    };
    state.queue.known.insert(receipt.clone());
    if let Some(entry) = state.queue.ledger.get_mut(key) {
        entry.admission_id = Some(receipt.clone());
        entry.awaiting_reconcile = false;
        state.queue.fifo_of.insert(receipt.clone(), key.0.clone());
        if entry.cell_drawn {
            // An idle submit drew its cell at submit. This is the one and only cell.
            state.queue.drawn.insert(receipt.clone());
            state.queue.ledger.remove(key);
        }
    }
    if state.queue.send_now_drafts.remove(key) && result.status == Status::Queued {
        return send_draft_plan(state, key, receipt);
    }
    Vec::new()
}

/// The draft is queued. Move it to the head, then interrupt the turn it waited behind.
fn send_draft_plan(
    state: &mut SessionState,
    draft: &SubmissionId,
    receipt: AdmissionId,
) -> Vec<Effect> {
    let Some(generation) = running_generation(state) else {
        return Vec::new();
    };
    let mut current = state.queue.queued_ids();
    if !current.contains(&receipt) {
        current.push(receipt.clone());
    }
    let target = super::reorder::move_target(&current, &receipt, super::reorder::Placement::Head);
    let blocked = current
        .iter()
        .zip(&target)
        .filter(|(before, after)| before != after)
        .flat_map(|(before, after)| [before, after])
        .find(|id| state.queue.row(id).is_some_and(|row| !row.controls.replace))
        .cloned();
    if let Some(blocked) = blocked {
        let text = state
            .queue
            .row(&blocked)
            .map(|row| preview(&row.text))
            .unwrap_or_default();
        note(
            state,
            NoticeKind::System,
            format!("{} \"{text}\"", notices::UNSUPPORTED),
        );
        return Vec::new();
    }
    let readmit = super::reorder::plan_reorder(&current, &target);
    if readmit.is_empty() {
        return interrupt_now(state);
    }
    let steps: Vec<super::MoveStep> = readmit
        .iter()
        .enumerate()
        .map(|(index, id)| super::MoveStep {
            admission_id: id.clone(),
            text: state
                .queue
                .row(id)
                .map(|row| row.text.clone())
                .unwrap_or_default(),
            submission_id: super::intents::derived_id(draft, index),
        })
        .collect();
    let plan = MovePlan {
        steps,
        next: 0,
        target,
        then_interrupt: Some(generation),
    };
    let effect = send_step(state, &plan);
    state.queue.plan = Some(plan);
    vec![effect]
}

/// Record the step at `plan.next` as the call in flight and build its effect.
fn send_step(state: &mut SessionState, plan: &MovePlan) -> Effect {
    let step = &plan.steps[plan.next];
    state.queue.in_flight = Some(InFlight {
        action: if plan.then_interrupt.is_some() {
            QueueAction::SendNow
        } else {
            QueueAction::Move
        },
        admission_id: Some(step.admission_id.clone()),
        submission_id: Some(step.submission_id.clone()),
        params_text: Some(step.text.clone()),
        retried: false,
        awaiting_reconcile: false,
    });
    Effect::ReplaceQueued {
        session_id: state.session_id.clone(),
        admission_id: step.admission_id.clone(),
        submission_id: step.submission_id.clone(),
        text: step.text.clone(),
    }
}

/// A replace was committed: `old` is retired and `receipt` took its text at the back.
fn replaced(
    state: &mut SessionState,
    action: QueueAction,
    old: Option<AdmissionId>,
    submission_id: Option<SubmissionId>,
    receipt: AdmissionId,
    queued: bool,
) -> Vec<Effect> {
    let text = submission_id.as_ref().and_then(|key| sent_text(state, key));
    if let Some(old) = old {
        state.queue.id_map.insert(old.clone(), receipt.clone());
        state.queue.known.remove(&old);
        drop_entries_of(state, &old);
        if state.queue.selection.as_ref() == Some(&old) {
            state.queue.selection = Some(receipt.clone());
        }
    }
    state.queue.known.insert(receipt.clone());
    if let (Some(key), Some(text)) = (submission_id.clone(), text) {
        state.queue.ledger.insert(
            key,
            LedgerEntry {
                text,
                admission_id: Some(receipt),
                attachments: Vec::new(),
                cell_drawn: false,
                awaiting_reconcile: false,
                retried: false,
            },
        );
    }

    let Some(plan) = state.queue.plan.clone() else {
        state.queue.in_flight = None;
        return Vec::new();
    };
    let is_step = plan
        .steps
        .get(plan.next)
        .is_some_and(|step| Some(&step.submission_id) == submission_id.as_ref());
    // The save of an edit comes before the first step of its restore plan.
    if !is_step && action != QueueAction::Edit {
        return Vec::new();
    }
    let mut plan = plan;
    if is_step {
        plan.next += 1;
    }
    if plan.next < plan.steps.len() {
        let effect = send_step(state, &plan);
        state.queue.plan = Some(plan);
        return vec![effect];
    }
    finish_plan(state, &plan, queued)
}

/// The last step answered. Check the order, then interrupt when send-now asked for it.
fn finish_plan(state: &mut SessionState, plan: &MovePlan, last_queued: bool) -> Vec<Effect> {
    state.queue.plan = None;
    state.queue.in_flight = None;
    state.queue.verify = Some(plan.target.clone());
    check_verify(state);
    let Some(generation) = plan.then_interrupt else {
        return Vec::new();
    };
    if last_queued && running_generation(state) == Some(generation) {
        return interrupt_now(state);
    }
    note(
        state,
        NoticeKind::System,
        notices::TURN_ENDED_FIRST.to_string(),
    );
    Vec::new()
}

/// The text a replace carried, from the call in flight or the plan.
fn sent_text(state: &SessionState, key: &SubmissionId) -> Option<String> {
    let flight = state
        .queue
        .in_flight
        .as_ref()
        .filter(|call| call.submission_id.as_ref() == Some(key))
        .and_then(|call| call.params_text.clone());
    flight.or_else(|| {
        state.queue.plan.as_ref().and_then(|plan| {
            plan.steps
                .iter()
                .find(|step| step.submission_id == *key)
                .map(|step| step.text.clone())
        })
    })
}

/// Forget the ledger entries and the waiting submit that belong to a row that is gone.
fn drop_entries_of(state: &mut SessionState, id: &AdmissionId) {
    let keys: Vec<SubmissionId> = state
        .queue
        .ledger
        .iter()
        .filter(|(_, entry)| entry.admission_id.as_ref() == Some(id))
        .map(|(key, _)| key.clone())
        .collect();
    for key in keys {
        state.queue.ledger.remove(&key);
        state.submissions.retain(|entry| entry.id != key.0);
    }
    forget_waiting(state, id);
}

/// Remove a row from the queue. Returns its text when it is known.
fn take_row(state: &mut SessionState, id: &AdmissionId) -> Option<String> {
    let counted_before = state.queue.counted_ids();
    let from_row = state.queue.row(id).map(|row| row.text.clone());
    let from_ledger = state
        .queue
        .ledger
        .values()
        .find(|entry| entry.admission_id.as_ref() == Some(id))
        .map(|entry| entry.text.clone());
    state.queue.rows.retain(|row| row.admission_id != *id);
    state.queue.known.remove(id);
    drop_entries_of(state, id);
    state.queue.fix_selection(&counted_before);
    from_row.or(from_ledger)
}

fn on_cancel_receipt(
    state: &mut SessionState,
    id: AdmissionId,
    result: &AdmissionStatus,
) -> Vec<Effect> {
    let flight_text = state
        .queue
        .in_flight
        .take()
        .and_then(|call| call.params_text);
    match (result.status, result.outcome.as_deref()) {
        (Status::Terminal, Some("cancelled")) => {
            if let Some(text) = take_row(state, &id).or(flight_text) {
                take_back(state, text);
            }
        }
        (Status::Started, _) => note(state, NoticeKind::System, notices::JUST_STARTED.to_string()),
        (Status::Terminal, _) => {
            take_row(state, &id);
            note(state, NoticeKind::System, notices::NOT_IN_QUEUE.to_string());
        }
        _ => {}
    }
    Vec::new()
}

/// Hand text back to the person. The reducer cannot see the composer, so the text is also
/// shown in the transcript: nothing is lost and no draft is touched.
pub(super) fn take_back(state: &mut SessionState, text: String) {
    state.queue.returned.push(text.clone());
    note(
        state,
        NoticeKind::System,
        format!("{}\n{text}", notices::DRAFT_KEPT),
    );
}

fn on_steer_queued_receipt(state: &mut SessionState, id: &AdmissionId) {
    state.queue.in_flight = None;
    let counted_before = state.queue.counted_ids();
    let mut text = None;
    if let Some(row) = state
        .queue
        .rows
        .iter_mut()
        .find(|row| row.admission_id == *id)
    {
        row.steering = true;
        row.status = RowStatus::Started;
        text = Some(row.text.clone());
    }
    state.queue.steer_noted.insert(id.clone());
    // A steering row is no longer selectable.
    state.queue.fix_selection(&counted_before);
    close_segment(state);
    if let Some(text) = text {
        note(state, NoticeKind::System, steer_sent(&text));
    }
}

/// End the streaming segment so text after a steer starts a new one below the notice.
fn close_segment(state: &mut SessionState) {
    state.seal_assistant();
    state.seal_reasoning();
}

fn steer_sent(text: &str) -> String {
    format!(
        "⤷ steer: \"{}\"{}",
        preview(text),
        notices::STEER_SENT_SUFFIX
    )
}

fn on_refused(
    state: &mut SessionState,
    action: QueueAction,
    reason: &str,
    detail: Option<&str>,
) -> Vec<Effect> {
    let (text, kind) = match refusal_notice(action, reason) {
        Some(text) => (text.to_string(), NoticeKind::System),
        None => (unmapped(reason, detail), NoticeKind::Error),
    };
    match action {
        QueueAction::Submit => Vec::new(),
        QueueAction::Edit | QueueAction::Move | QueueAction::SendNow => {
            note(state, kind, text);
            stop_plan(state, true);
            Vec::new()
        }
        QueueAction::Remove | QueueAction::Discard | QueueAction::Steer => {
            state.queue.in_flight = None;
            // A row that started or was lost stays in the queue until the next fanout says so.
            note(state, kind, text);
            Vec::new()
        }
        QueueAction::SteerText => {
            let flight = take_steer_flight(state);
            let fallback = matches!(
                reason,
                "stale_generation" | "execution_not_ready" | "unsupported_control"
            );
            note(state, kind, text);
            match flight.text {
                Some(text) if fallback => resubmit(state, &text, flight.submission_id),
                Some(text) => {
                    state.queue.returned.push(text);
                    Vec::new()
                }
                None => Vec::new(),
            }
        }
    }
}

fn unmapped(reason: &str, detail: Option<&str>) -> String {
    match detail {
        Some(detail) if !detail.is_empty() => format!("Hermes refused: {reason} {detail}"),
        _ => format!("Hermes refused: {reason}"),
    }
}

/// A step was refused or never confirmed. The rest of the plan is not sent.
///
/// A refusal also reports an order unlike the target. An unconfirmed step has its own notice.
fn stop_plan(state: &mut SessionState, report_order: bool) {
    state.queue.in_flight = None;
    let Some(plan) = state.queue.plan.take() else {
        return;
    };
    if !report_order {
        return;
    }
    let mapped: Vec<AdmissionId> = plan.target.iter().map(|id| resolve(state, id)).collect();
    let held: Vec<AdmissionId> = state
        .queue
        .queued_ids()
        .into_iter()
        .filter(|id| mapped.contains(id))
        .collect();
    let expected: Vec<AdmissionId> = mapped.into_iter().filter(|id| held.contains(id)).collect();
    if held != expected {
        note(
            state,
            NoticeKind::System,
            notices::QUEUE_CHANGED.to_string(),
        );
    }
}

/// Follow the id map from a retired admission to the one that replaced it last.
fn resolve(state: &SessionState, id: &AdmissionId) -> AdmissionId {
    let mut current = id.clone();
    // A chain longer than the queue cap cannot be real; the bound stops a cycle.
    for _ in 0..64 {
        match state.queue.id_map.get(&current) {
            Some(next) => current = next.clone(),
            None => break,
        }
    }
    current
}

/// Check the order a finished plan reached, once every row of the target is in the queue.
fn check_verify(state: &mut SessionState) {
    let Some(target) = state.queue.verify.clone() else {
        return;
    };
    let mapped: Vec<AdmissionId> = target.iter().map(|id| resolve(state, id)).collect();
    let held: Vec<AdmissionId> = state
        .queue
        .queued_ids()
        .into_iter()
        .filter(|id| mapped.contains(id))
        .collect();
    if held.len() < mapped.len() {
        return;
    }
    state.queue.verify = None;
    if held != mapped {
        note(
            state,
            NoticeKind::System,
            notices::QUEUE_CHANGED.to_string(),
        );
    }
}

struct SteerFlight {
    text: Option<String>,
    submission_id: Option<SubmissionId>,
}

fn take_steer_flight(state: &mut SessionState) -> SteerFlight {
    match state.queue.in_flight.take() {
        Some(call) if call.action == QueueAction::SteerText => SteerFlight {
            text: call.params_text,
            submission_id: call.submission_id,
        },
        other => {
            state.queue.in_flight = other;
            SteerFlight {
                text: None,
                submission_id: None,
            }
        }
    }
}

/// Submit text that a steer could not deliver, as the next prompt.
fn resubmit(state: &mut SessionState, text: &str, id: Option<SubmissionId>) -> Vec<Effect> {
    let id = id.map(|id| id.0).unwrap_or_else(|| {
        state.queue.minted += 1;
        format!("talaria-steer-{}", state.queue.minted)
    });
    submit_text(state, text, &id)
}

fn on_steer_result(state: &mut SessionState, result: &CanonicalCorrectionResult) -> Vec<Effect> {
    let flight = take_steer_flight(state);
    let CanonicalCorrectionResult::CorrectionAck(ack) = result else {
        note(state, NoticeKind::System, notices::VOICE_ENDED.to_string());
        return Vec::new();
    };
    let text = flight.text.unwrap_or_else(|| ack.text.clone());
    steer_answer(state, ack, text, flight.submission_id)
}

fn steer_answer(
    state: &mut SessionState,
    ack: &CanonicalCorrectionAck,
    text: String,
    id: Option<SubmissionId>,
) -> Vec<Effect> {
    match ack.status {
        CanonicalCorrectionAckStatus::Queued | CanonicalCorrectionAckStatus::Redirected => {
            close_segment(state);
            note(state, NoticeKind::System, steer_sent(&text));
            Vec::new()
        }
        CanonicalCorrectionAckStatus::Rejected if ack.delivered == Some(false) => {
            note(
                state,
                NoticeKind::System,
                format!(
                    "The running turn could not take the steer. It is queued as the next prompt: \"{}\"",
                    preview(&text)
                ),
            );
            resubmit(state, &text, id)
        }
        CanonicalCorrectionAckStatus::Rejected => {
            // A plain rejection can follow a delivery. Sending it again could run it twice.
            note(
                state,
                NoticeKind::System,
                notices::STEER_UNCONFIRMED.to_string(),
            );
            state.queue.returned.push(text);
            Vec::new()
        }
    }
}

fn on_unknown(
    state: &mut SessionState,
    action: QueueAction,
    admission_id: &Option<AdmissionId>,
    submission_id: &Option<SubmissionId>,
) -> Vec<Effect> {
    match action {
        QueueAction::Submit => {
            if let Some(key) = submission_id {
                if !state.queue.ledger.contains_key(key) {
                    let waiting = state.submissions.iter().find(|entry| entry.id == key.0);
                    let (text, placed) = waiting.map_or((String::new(), false), |entry| {
                        (entry.text.clone(), entry.placed)
                    });
                    note_submit(state, &text, &key.0, placed);
                }
                if let Some(entry) = state.queue.ledger.get_mut(key) {
                    entry.awaiting_reconcile = true;
                }
            }
            Vec::new()
        }
        QueueAction::SteerText => {
            // A typed steer names no row, so no fanout can tell whether it landed.
            let flight = take_steer_flight(state);
            note(
                state,
                NoticeKind::Warning,
                notices::STEER_UNCONFIRMED.to_string(),
            );
            if let Some(text) = flight.text {
                state.queue.returned.push(text);
            }
            Vec::new()
        }
        _ => {
            let flight = state.queue.in_flight.get_or_insert_with(|| InFlight {
                action,
                admission_id: admission_id.clone(),
                submission_id: submission_id.clone(),
                params_text: None,
                retried: false,
                awaiting_reconcile: false,
            });
            flight.awaiting_reconcile = true;
            Vec::new()
        }
    }
}

fn on_due(
    state: &mut SessionState,
    action: QueueAction,
    submission_id: Option<&SubmissionId>,
) -> Vec<Effect> {
    if action == QueueAction::Submit {
        let Some(key) = submission_id else {
            return Vec::new();
        };
        let waiting = state
            .queue
            .ledger
            .get(key)
            .is_some_and(|entry| entry.awaiting_reconcile);
        return if waiting {
            settle_submit(state, key)
        } else {
            Vec::new()
        };
    }
    reconcile_in_flight(state)
}

/// A lost submit reply, after a fanout or the deadline. Committed, retried once, or given up.
fn settle_submit(state: &mut SessionState, key: &SubmissionId) -> Vec<Effect> {
    let Some(entry) = state.queue.ledger.get_mut(key) else {
        return Vec::new();
    };
    entry.awaiting_reconcile = false;
    if entry.admission_id.is_some() {
        return Vec::new();
    }
    if !entry.retried && !entry.text.is_empty() {
        entry.retried = true;
        return vec![Effect::SendPrompt {
            session_id: state.session_id.clone(),
            text: entry.text.clone(),
            submission_id: key.0.clone(),
            attachments: state
                .submissions
                .iter()
                .find(|entry| entry.id == key.0)
                .map_or_else(Vec::new, |entry| entry.attachments.clone()),
        }];
    }
    let text = entry.text.clone();
    state.queue.ledger.remove(key);
    state.submissions.retain(|entry| entry.id != key.0);
    note(
        state,
        NoticeKind::Warning,
        notices::PROMPT_UNCONFIRMED.to_string(),
    );
    if !text.is_empty() {
        state.queue.returned.push(text);
    }
    Vec::new()
}

/// Settle the one outstanding row call that lost its reply, from the queue as it is now.
fn reconcile_in_flight(state: &mut SessionState) -> Vec<Effect> {
    let Some(call) = state.queue.in_flight.clone() else {
        return Vec::new();
    };
    if !call.awaiting_reconcile {
        return Vec::new();
    }
    match call.action {
        QueueAction::Edit | QueueAction::Move | QueueAction::SendNow => {
            reconcile_replace(state, &call)
        }
        QueueAction::Remove => {
            state.queue.in_flight = None;
            let id = call.admission_id.clone();
            let row = id.as_ref().and_then(|id| state.queue.row(id)).cloned();
            match row {
                None => {
                    if let Some(text) = call.params_text {
                        if let Some(id) = &id {
                            take_row(state, id);
                        }
                        take_back(state, text);
                    }
                }
                Some(row) if row.status == RowStatus::Queued && !row.steering => {
                    note(
                        state,
                        NoticeKind::Warning,
                        notices::REMOVAL_UNCONFIRMED.to_string(),
                    );
                }
                Some(_) => note(
                    state,
                    NoticeKind::System,
                    notices::REMOVAL_TOO_LATE.to_string(),
                ),
            }
            Vec::new()
        }
        QueueAction::Discard => {
            state.queue.in_flight = None;
            let listed = call
                .admission_id
                .as_ref()
                .is_some_and(|id| state.queue.row(id).is_some());
            if listed {
                note(
                    state,
                    NoticeKind::Warning,
                    notices::DISCARD_UNCONFIRMED.to_string(),
                );
            }
            Vec::new()
        }
        QueueAction::Steer => {
            state.queue.in_flight = None;
            let back_in_line = call
                .admission_id
                .as_ref()
                .and_then(|id| state.queue.row(id))
                .is_some_and(|row| row.status == RowStatus::Queued && !row.steering);
            if back_in_line {
                note(
                    state,
                    NoticeKind::Warning,
                    notices::STEER_RETURNED.to_string(),
                );
            }
            Vec::new()
        }
        QueueAction::Submit | QueueAction::SteerText => Vec::new(),
    }
}

fn reconcile_replace(state: &mut SessionState, call: &InFlight) -> Vec<Effect> {
    let committed = call.submission_id.as_ref().and_then(|key| {
        state
            .queue
            .rows
            .iter()
            .find(|row| row.input_id == *key)
            .map(|row| row.admission_id.clone())
    });
    if let Some(receipt) = committed {
        return replaced(
            state,
            call.action,
            call.admission_id.clone(),
            call.submission_id.clone(),
            receipt,
            true,
        );
    }
    let (Some(old), Some(key), Some(text)) = (
        call.admission_id.clone(),
        call.submission_id.clone(),
        call.params_text.clone(),
    ) else {
        stop_plan(state, false);
        note(
            state,
            NoticeKind::Warning,
            notices::CHANGE_UNCONFIRMED.to_string(),
        );
        return Vec::new();
    };
    if call.retried {
        stop_plan(state, false);
        note(
            state,
            NoticeKind::Warning,
            notices::CHANGE_UNCONFIRMED.to_string(),
        );
        return Vec::new();
    }
    // Hermes answers an exact retry with the same new admission, so this is safe whether or
    // not the first call committed.
    if let Some(flight) = state.queue.in_flight.as_mut() {
        flight.retried = true;
        flight.awaiting_reconcile = false;
    }
    vec![Effect::ReplaceQueued {
        session_id: state.session_id.clone(),
        admission_id: old,
        submission_id: key,
        text,
    }]
}
