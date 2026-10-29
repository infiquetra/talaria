//! Keys and commands aimed at the queue, turned into effects.
//!
//! Issue 232. Queue and steer. Every intent that changes the queue checks the busy rule first:
//! while a plan or a row call is outstanding, only plain submits, typing and leaving selection
//! pass. Calls leave through effects. The results come back as events and are handled in
//! [`super::flow`].

use super::flow::{interrupt_now, note, running_generation, submit_text};
use super::notices::{self, preview};
use super::reorder::{Placement, move_target, plan_reorder};
use super::{
    AdmissionId, Direction, EditState, IdSource, InFlight, MovePlan, MoveStep, QueueAction,
    QueueIntent, QueuedRow, RowStatus, SubmissionId,
};
use crate::session::{Effect, NoticeKind, SessionState};

/// Apply one queue intent. See [`super::update_queue`].
pub(crate) fn update_queue(
    state: &mut SessionState,
    intent: QueueIntent,
    ids: &mut dyn IdSource,
) -> Vec<Effect> {
    let effects = dispatch(state, intent, ids);
    state.advance_prefix();
    effects
}

/// What a row action needs from the row's controls.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Need {
    Replace,
    Steer,
}

fn dispatch(state: &mut SessionState, intent: QueueIntent, ids: &mut dyn IdSource) -> Vec<Effect> {
    match intent {
        QueueIntent::Select(direction) => {
            select(state, direction);
            Vec::new()
        }
        QueueIntent::Leave => {
            state.queue.selection = None;
            Vec::new()
        }
        QueueIntent::CancelEdit => {
            state.queue.edit = None;
            Vec::new()
        }
        QueueIntent::EnterSelection => enter_selection(state),
        QueueIntent::BeginEdit => begin_edit(state),
        QueueIntent::SaveEdit(text) => save_edit(state, &text, ids),
        QueueIntent::Remove => remove(state),
        QueueIntent::Steer => steer(state),
        QueueIntent::SendNow => send_now(state, ids),
        QueueIntent::Move(direction) => move_row(state, direction, ids),
        QueueIntent::SteerText(text) => steer_text(state, text, ids),
        QueueIntent::SendDraftNow(text) => send_draft_now(state, &text, ids),
    }
}

/// Push the busy notice and return true while a plan or a row call is outstanding.
fn refuse_busy(state: &mut SessionState) -> bool {
    if state.queue.busy() {
        note(state, NoticeKind::System, notices::BUSY.to_string());
        return true;
    }
    false
}

/// The selected row, when it is still counted.
fn selected(state: &SessionState) -> Option<QueuedRow> {
    let id = state.queue.selection.as_ref()?;
    state
        .queue
        .counted()
        .find(|row| row.admission_id == *id)
        .cloned()
}

/// True when `row` may take an action that needs `need`. Pushes the reason when it may not.
fn allowed(state: &mut SessionState, row: &QueuedRow, need: Need) -> bool {
    if row.status == RowStatus::Unknown {
        note(state, NoticeKind::System, notices::LOST_ROW.to_string());
        return false;
    }
    let offered = match need {
        Need::Replace => row.controls.replace,
        Need::Steer => row.controls.steer,
    };
    if !offered {
        note(state, NoticeKind::System, notices::UNSUPPORTED.to_string());
    }
    offered
}

fn select(state: &mut SessionState, direction: Direction) {
    let counted = state.queue.counted_ids();
    let Some(last) = counted.len().checked_sub(1) else {
        state.queue.selection = None;
        return;
    };
    let at = state
        .queue
        .selection
        .as_ref()
        .and_then(|id| counted.iter().position(|row| row == id));
    let next = match (at, direction) {
        (None, _) => last,
        (Some(at), Direction::Up) => at.saturating_sub(1),
        (Some(at), Direction::Down) => (at + 1).min(last),
    };
    state.queue.selection = counted.get(next).cloned();
}

fn enter_selection(state: &mut SessionState) -> Vec<Effect> {
    if refuse_busy(state) {
        return Vec::new();
    }
    match state.queue.counted_ids().pop() {
        Some(newest) => state.queue.selection = Some(newest),
        None => note(
            state,
            NoticeKind::System,
            notices::NOTHING_QUEUED.to_string(),
        ),
    }
    Vec::new()
}

fn begin_edit(state: &mut SessionState) -> Vec<Effect> {
    if refuse_busy(state) {
        return Vec::new();
    }
    let Some(row) = selected(state) else {
        return Vec::new();
    };
    if !allowed(state, &row, Need::Replace) {
        return Vec::new();
    }
    let index = state
        .queue
        .counted()
        .position(|counted| counted.admission_id == row.admission_id)
        .map_or(1, |place| place + 1);
    state.queue.edit = Some(EditState {
        admission_id: row.admission_id,
        original_text: row.text,
        index,
    });
    Vec::new()
}

fn save_edit(state: &mut SessionState, text: &str, ids: &mut dyn IdSource) -> Vec<Effect> {
    if refuse_busy(state) {
        return Vec::new();
    }
    let open = state
        .queue
        .edit
        .as_ref()
        .map(|edit| edit.admission_id.clone());
    let row = match &open {
        Some(id) => state
            .queue
            .counted()
            .find(|row| row.admission_id == *id)
            .cloned(),
        None => selected(state),
    };
    let Some(row) = row else {
        if open.is_some() {
            state.queue.edit = None;
            note(state, NoticeKind::System, notices::NOT_IN_QUEUE.to_string());
        }
        return Vec::new();
    };
    if !allowed(state, &row, Need::Replace) {
        return Vec::new();
    }
    if text.trim().is_empty() {
        note(state, NoticeKind::System, notices::EMPTY_EDIT.to_string());
        return Vec::new();
    }
    if state
        .queue
        .edit
        .as_ref()
        .is_some_and(|edit| edit.original_text == text)
    {
        state.queue.edit = None;
        return Vec::new();
    }

    // The new admission lands at the back. The old id stands for it in the target order, so
    // the restore plan puts the rows that were behind the old row after it again.
    let queued = state.queue.queued_ids();
    let mut after_replace: Vec<AdmissionId> = queued
        .iter()
        .filter(|id| **id != row.admission_id)
        .cloned()
        .collect();
    after_replace.push(row.admission_id.clone());
    let readmit = plan_reorder(&after_replace, &queued);

    let submission_id = ids.submission_id();
    let replace = Effect::ReplaceQueued {
        session_id: state.session_id.clone(),
        admission_id: row.admission_id.clone(),
        submission_id: submission_id.clone(),
        text: text.to_string(),
    };
    state.queue.in_flight = Some(InFlight {
        action: QueueAction::Edit,
        admission_id: Some(row.admission_id.clone()),
        submission_id: Some(submission_id),
        params_text: Some(text.to_string()),
        retried: false,
        awaiting_reconcile: false,
    });
    state.queue.edit = None;
    if !readmit.is_empty() {
        if readmit.iter().all(|id| has_replace(state, id)) {
            let steps = steps_for(state, &readmit, ids);
            state.queue.plan = Some(MovePlan {
                steps,
                next: 0,
                target: queued,
                then_interrupt: None,
            });
        } else {
            note(
                state,
                NoticeKind::System,
                notices::SAVED_AT_BACK.to_string(),
            );
        }
    }
    vec![replace]
}

fn remove(state: &mut SessionState) -> Vec<Effect> {
    if refuse_busy(state) {
        return Vec::new();
    }
    let Some(row) = selected(state) else {
        return Vec::new();
    };
    if row.status == RowStatus::Unknown {
        let Some(execution_generation) = row.execution_generation else {
            note(
                state,
                NoticeKind::System,
                notices::NO_GENERATION.to_string(),
            );
            return Vec::new();
        };
        state.queue.in_flight = Some(call(QueueAction::Discard, &row));
        return vec![Effect::ResolveUnknown {
            session_id: state.session_id.clone(),
            admission_id: row.admission_id,
            execution_generation,
        }];
    }
    state.queue.in_flight = Some(call(QueueAction::Remove, &row));
    vec![Effect::CancelQueued {
        session_id: state.session_id.clone(),
        admission_id: row.admission_id,
    }]
}

fn steer(state: &mut SessionState) -> Vec<Effect> {
    if refuse_busy(state) {
        return Vec::new();
    }
    let Some(row) = selected(state) else {
        return Vec::new();
    };
    if !allowed(state, &row, Need::Steer) {
        return Vec::new();
    }
    if running_generation(state).is_none() {
        note(state, NoticeKind::System, notices::SESSION_IDLE.to_string());
        return Vec::new();
    }
    state.queue.in_flight = Some(call(QueueAction::Steer, &row));
    vec![Effect::SteerQueued {
        session_id: state.session_id.clone(),
        admission_id: row.admission_id,
    }]
}

fn move_row(state: &mut SessionState, direction: Direction, ids: &mut dyn IdSource) -> Vec<Effect> {
    if refuse_busy(state) {
        return Vec::new();
    }
    let Some(row) = selected(state) else {
        return Vec::new();
    };
    if !allowed(state, &row, Need::Replace) {
        return Vec::new();
    }
    let placement = match direction {
        Direction::Up => Placement::Up,
        Direction::Down => Placement::Down,
    };
    let current = state.queue.queued_ids();
    let target = move_target(&current, &row.admission_id, placement);
    if target == current {
        return Vec::new();
    }
    start_plan(state, &current, target, None, QueueAction::Move, ids)
}

fn send_now(state: &mut SessionState, ids: &mut dyn IdSource) -> Vec<Effect> {
    if refuse_busy(state) {
        return Vec::new();
    }
    let Some(row) = selected(state) else {
        return Vec::new();
    };
    if !allowed(state, &row, Need::Replace) {
        return Vec::new();
    }
    if state.queue.paused() {
        note(state, NoticeKind::System, notices::PAUSED.to_string());
        return Vec::new();
    }
    let current = state.queue.queued_ids();
    let target = move_target(&current, &row.admission_id, Placement::Head);
    start_plan(
        state,
        &current,
        target,
        running_generation(state),
        QueueAction::SendNow,
        ids,
    )
}

/// Send the draft as a prompt. While a turn runs, its receipt starts the head plan.
fn send_draft_now(state: &mut SessionState, text: &str, ids: &mut dyn IdSource) -> Vec<Effect> {
    if refuse_busy(state) {
        super::flow::take_back(state, text.to_owned());
        return Vec::new();
    }
    if text.trim().is_empty() {
        return Vec::new();
    }
    let submission_id = ids.submission_id();
    if let Some(generation) = running_generation(state) {
        let authority_epoch = state.queue.generation_fence.map(|(epoch, _)| epoch);
        state
            .queue
            .send_now_drafts
            .insert(submission_id.clone(), (generation, authority_epoch));
    }
    submit_text(state, text, &submission_id.0)
}

fn steer_text(state: &mut SessionState, text: String, ids: &mut dyn IdSource) -> Vec<Effect> {
    if refuse_busy(state) {
        super::flow::take_back(state, text);
        return Vec::new();
    }
    if text.trim().is_empty() {
        return Vec::new();
    }
    let submission_id = ids.submission_id();
    let Some(execution_generation) = running_generation(state) else {
        let effects = submit_text(state, &text, &submission_id.0);
        note(
            state,
            NoticeKind::System,
            format!(
                "No turn is running, so this was sent as a new prompt: \"{}\"",
                preview(&text)
            ),
        );
        return effects;
    };
    // The id is minted now. A refusal that falls back to a plain submit has no id source.
    state.queue.in_flight = Some(InFlight {
        action: QueueAction::SteerText,
        admission_id: None,
        submission_id: Some(submission_id),
        params_text: Some(text.clone()),
        retried: false,
        awaiting_reconcile: false,
    });
    vec![Effect::Steer {
        session_id: state.session_id.clone(),
        execution_generation,
        text,
    }]
}

/// Build a plan from `current` to `target` and send its first step.
///
/// Nothing is sent when a row whose place changes lacks `replace`. An empty plan with an
/// interrupt wanted interrupts at once, after the checks of a finished plan.
fn start_plan(
    state: &mut SessionState,
    current: &[AdmissionId],
    target: Vec<AdmissionId>,
    then_interrupt: Option<i64>,
    action: QueueAction,
    ids: &mut dyn IdSource,
) -> Vec<Effect> {
    let blocked = current
        .iter()
        .zip(&target)
        .filter(|(before, after)| before != after)
        .flat_map(|(before, after)| [before, after])
        .find(|id| !has_replace(state, id))
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
    let readmit = plan_reorder(current, &target);
    if readmit.is_empty() {
        return match (action, then_interrupt) {
            (QueueAction::SendNow, Some(_)) => interrupt_now(state),
            (QueueAction::SendNow, None) => {
                note(
                    state,
                    NoticeKind::System,
                    notices::ABOUT_TO_START.to_string(),
                );
                Vec::new()
            }
            _ => Vec::new(),
        };
    }
    let steps = steps_for(state, &readmit, ids);
    let first = steps[0].clone();
    state.queue.plan = Some(MovePlan {
        steps,
        next: 0,
        target,
        then_interrupt,
    });
    state.queue.in_flight = Some(InFlight {
        action,
        admission_id: Some(first.admission_id.clone()),
        submission_id: Some(first.submission_id.clone()),
        params_text: Some(first.text.clone()),
        retried: false,
        awaiting_reconcile: false,
    });
    vec![Effect::ReplaceQueued {
        session_id: state.session_id.clone(),
        admission_id: first.admission_id,
        submission_id: first.submission_id,
        text: first.text,
    }]
}

fn steps_for(
    state: &SessionState,
    readmit: &[AdmissionId],
    ids: &mut dyn IdSource,
) -> Vec<MoveStep> {
    readmit
        .iter()
        .map(|id| MoveStep {
            admission_id: id.clone(),
            text: state
                .queue
                .row(id)
                .map(|row| row.text.clone())
                .unwrap_or_default(),
            submission_id: ids.submission_id(),
        })
        .collect()
}

fn has_replace(state: &SessionState, id: &AdmissionId) -> bool {
    state.queue.row(id).is_some_and(|row| row.controls.replace)
}

fn call(action: QueueAction, row: &QueuedRow) -> InFlight {
    InFlight {
        action,
        admission_id: Some(row.admission_id.clone()),
        submission_id: None,
        params_text: Some(row.text.clone()),
        retried: false,
        awaiting_reconcile: false,
    }
}

/// Ids for the steps of a plan that starts when a receipt arrives, where no id source exists.
///
/// Each one derives from the draft's own submission id, so it is as unique as that id.
pub(crate) fn derived_id(base: &SubmissionId, step: usize) -> SubmissionId {
    SubmissionId(format!("{}-{}", base.0, step + 1))
}
