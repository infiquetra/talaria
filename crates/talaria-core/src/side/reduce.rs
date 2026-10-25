//! Building the side events a submit produces, and reducing them into the session.
//!
//! Every cell and notice made here goes through [`SessionState::insert_final`], so it lands
//! before the first unfinished cell and commits at once, above a reply that is still streaming.

use serde_json::Value;

use crate::session::{Cell, Effect, Event, NoticeCell, NoticeKind, SessionState};
use crate::side::text::{
    QUESTION_NOT_GIVEN, SHELL_EMPTY_HINT, SIDE_UNKNOWN_NOTICE, SIDE_USAGE, side_timeout_notice,
};
use crate::side::{
    Origin, PendingKind, PendingTask, RequestId, SIDE_ANSWER_TIMEOUT_MS, ShellCell, SideCell,
    SideOutcome,
};

/// Turn a side command argument into the event to reduce.
pub fn btw_command(arg: &str, now_ms: u64) -> Event {
    let question = arg.trim();
    if question.is_empty() {
        Event::SideUsage
    } else {
        Event::SideAsked {
            question: question.to_string(),
            started_ms: now_ms,
        }
    }
}

/// Apply one shell or side event. Other events change nothing here.
pub(crate) fn apply(state: &mut SessionState, event: &Event) -> Vec<Effect> {
    match event {
        Event::ShellRequested {
            command,
            started_ms,
        } => {
            let request_id = allocate(state);
            state.side.pending.push(PendingTask {
                request_id,
                started_ms: *started_ms,
                kind: PendingKind::Shell {
                    command: command.clone(),
                },
            });
            return vec![Effect::RunShell {
                session_id: state.session_id.clone(),
                request_id,
                command: command.clone(),
            }];
        }
        Event::SideAsked {
            question,
            started_ms,
        } => {
            let request_id = allocate(state);
            state.side.pending.push(PendingTask {
                request_id,
                started_ms: *started_ms,
                kind: PendingKind::Side {
                    question: question.clone(),
                    task_id: None,
                    accepted_ms: None,
                },
            });
            return vec![Effect::AskSide {
                session_id: state.session_id.clone(),
                request_id,
                question: question.clone(),
            }];
        }
        Event::ShellEmpty => notice(state, NoticeKind::System, SHELL_EMPTY_HINT.to_string()),
        Event::SideUsage => notice(state, NoticeKind::System, SIDE_USAGE.to_string()),
        Event::ShellFinished {
            request_id,
            outcome,
            duration_ms,
        } => {
            if let Some(PendingKind::Shell { command }) = take_pending(state, *request_id, false) {
                state.insert_final(Cell::Shell(ShellCell {
                    command,
                    outcome: outcome.clone(),
                    duration_ms: *duration_ms,
                }));
            }
        }
        Event::SideAccepted {
            request_id,
            task_id,
            accepted_ms,
        } => {
            // No row means the answer came first and took it, or the reply is a stray.
            if let Some(PendingKind::Side {
                task_id: id,
                accepted_ms: at,
                ..
            }) = state
                .side
                .pending
                .iter_mut()
                .find(|row| row.request_id == *request_id)
                .map(|row| &mut row.kind)
            {
                *id = Some(task_id.clone());
                *at = Some(*accepted_ms);
            }
        }
        Event::SideRefused {
            request_id,
            reason,
            detail,
        } => settle_side(
            state,
            *request_id,
            SideOutcome::Refused {
                reason: reason.clone(),
                detail: detail.clone(),
            },
        ),
        Event::SideNotSent {
            request_id,
            not_connected,
        } => settle_side(
            state,
            *request_id,
            SideOutcome::NotSent {
                not_connected: *not_connected,
            },
        ),
        Event::SideUnknown { request_id } => {
            if let Some(PendingKind::Side { question, .. }) = take_pending(state, *request_id, true)
            {
                state.side.unconfirmed.push((*request_id, question));
                notice(state, NoticeKind::Warning, SIDE_UNKNOWN_NOTICE.to_string());
            }
        }
        Event::SideClock { now_ms } => {
            let (expired, kept): (Vec<PendingTask>, Vec<PendingTask>) =
                std::mem::take(&mut state.side.pending)
                    .into_iter()
                    .partition(|row| match row.kind {
                        PendingKind::Side {
                            accepted_ms: Some(accepted),
                            ..
                        } => now_ms.saturating_sub(accepted) >= SIDE_ANSWER_TIMEOUT_MS,
                        _ => false,
                    });
            state.side.pending = kept;
            for row in expired {
                if let PendingKind::Side {
                    question, task_id, ..
                } = row.kind
                {
                    if let Some(task_id) = task_id {
                        state.side.timed_out.insert(task_id);
                    }
                    notice(state, NoticeKind::Warning, side_timeout_notice(&question));
                }
            }
        }
        _ => {}
    }
    Vec::new()
}

/// Match one `btw.complete` to the question it answers. It never touches the turn.
pub(crate) fn btw_complete(state: &mut SessionState, payload: &Value) {
    let (Some(task_id), Some(text)) = (payload["task_id"].as_str(), payload["text"].as_str())
    else {
        return;
    };
    if state.side.answered.contains(task_id) {
        return;
    }
    let question = payload["question"]
        .as_str()
        .map(str::trim)
        .filter(|question| !question.is_empty());
    let outcome = match text.strip_prefix("error: ") {
        Some(rest) => SideOutcome::Failed {
            text: rest.to_string(),
        },
        None => SideOutcome::Answered {
            text: text.to_string(),
        },
    };
    let pending = &mut state.side.pending;
    let by_task = pending.iter().position(
        |row| matches!(&row.kind, PendingKind::Side { task_id: Some(id), .. } if id == task_id),
    );
    let by_question = || {
        question.and_then(|asked| {
            pending.iter().position(|row| {
                matches!(&row.kind, PendingKind::Side { task_id: None, question: waiting, .. } if waiting == asked)
            })
        })
    };
    let (asked, origin, late) = if let Some(index) = by_task.or_else(by_question) {
        match pending.remove(index).kind {
            PendingKind::Side { question, .. } => (question, Origin::Here, false),
            PendingKind::Shell { .. } => return,
        }
    } else if let Some(index) = question.and_then(|asked| {
        state
            .side
            .unconfirmed
            .iter()
            .position(|(_, lost)| lost == asked)
    }) {
        (state.side.unconfirmed.remove(index).1, Origin::Here, false)
    } else {
        let shown = question.unwrap_or(QUESTION_NOT_GIVEN).to_string();
        if state.side.timed_out.remove(task_id) {
            (shown, Origin::Here, true)
        } else {
            (shown, Origin::Elsewhere, false)
        }
    };
    state.side.answered.insert(task_id.to_string());
    state.insert_final(Cell::Side(SideCell {
        question: asked,
        outcome,
        origin,
        late,
    }));
}

fn allocate(state: &mut SessionState) -> RequestId {
    state.side.last_request_id = state.side.last_request_id.saturating_add(1);
    RequestId(state.side.last_request_id)
}

/// Remove the pending row of this kind with this id, returning what it waited for.
fn take_pending(
    state: &mut SessionState,
    request_id: RequestId,
    side: bool,
) -> Option<PendingKind> {
    let index = state.side.pending.iter().position(|row| {
        row.request_id == request_id && matches!(row.kind, PendingKind::Side { .. }) == side
    })?;
    Some(state.side.pending.remove(index).kind)
}

fn settle_side(state: &mut SessionState, request_id: RequestId, outcome: SideOutcome) {
    if let Some(PendingKind::Side { question, .. }) = take_pending(state, request_id, true) {
        state.insert_final(Cell::Side(SideCell {
            question,
            outcome,
            origin: Origin::Here,
            late: false,
        }));
    }
}

fn notice(state: &mut SessionState, kind: NoticeKind, text: String) {
    let id = state.next_id();
    state.insert_final(Cell::Notice(NoticeCell {
        id,
        kind,
        text,
        detail: None,
    }));
}
