//! The pure transition.

use super::text::{
    remove_whitespace, strip_terminal_escapes, text_after_non_whitespace, trim_leading_breaks,
};
use super::{
    AssistantCell, Cell, Effect, Event, NoticeKind, ReasoningCell, SessionState, StatusNote,
    TodoState, ToolCell, ToolProgress, ToolStatus, TurnOutcome, TurnState,
};
use hermes_contract::events::Notification;
use serde_json::Value;

/// Apply one event. Total: it does not panic on a well-typed [`Event`], and it reads no clock.
pub fn update(state: &mut SessionState, event: Event) -> Vec<Effect> {
    let effects = match &event {
        Event::Session {
            session_id,
            frame,
            raw,
        } => {
            if session_id.as_str().is_empty() || session_id != &state.session_id {
                return Vec::new();
            }
            let name = frame.type_name().unwrap_or("");
            if name == "session.replay_gap" {
                state.needs_resume = true;
                return Vec::new();
            }
            if let (Some(incoming), Some(current)) =
                (&frame.replay_epoch, &state.cursor.replay_epoch)
                && incoming != current
            {
                state.needs_resume = true;
                return Vec::new();
            }
            if state.cursor.replay_epoch.is_none() {
                state.cursor.replay_epoch.clone_from(&frame.replay_epoch);
            }
            if let Some(seq) = frame.seq {
                if seq <= state.cursor.last_seq {
                    return Vec::new();
                }
                state.cursor.last_seq = seq;
            }
            // A settlement of a queued row has no turn generation. It never ends the turn.
            if name == "message.complete"
                && frame.execution_generation.is_none()
                && frame.admission_id.is_some()
            {
                return Vec::new();
            }
            if name == "session.info" {
                let effects = crate::queue::flow::session_info(
                    state,
                    &raw["payload"],
                    frame.replay_epoch.as_deref(),
                    frame.seq,
                );
                state.advance_prefix();
                return effects;
            }
            session(
                state,
                name,
                &raw["payload"],
                frame.execution_generation,
                frame.admission_id.as_deref(),
                matches!(frame.notification, Notification::Unknown(_)),
            );
            Vec::new()
        }
        Event::ConnectionOpened { .. } => {
            state.unknown.clear();
            Vec::new()
        }
        Event::Submit {
            text,
            display,
            submission_id,
            attachments,
        } => submit(state, text, display, submission_id, attachments),
        Event::SubmitFailed {
            submission_id,
            reason,
        } => {
            if let Some(index) = state
                .submissions
                .iter()
                .position(|entry| &entry.id == submission_id)
            {
                state.submissions.remove(index);
                state.notice(NoticeKind::Error, reason.clone(), None);
            }
            crate::queue::flow::submit_failed(state, submission_id);
            Vec::new()
        }
        Event::Interrupt => interrupt(state),
        Event::InterruptFailed => {
            state.interrupted = false;
            Vec::new()
        }
        Event::SubmitCommand {
            text,
            display,
            submission_id,
        } => {
            if text.trim().is_empty() {
                return Vec::new();
            }
            let placed = state.turn != TurnState::Running && state.submissions.is_empty();
            if placed {
                state.user(display.clone());
            }
            crate::queue::flow::note_submit(state, text, submission_id, placed);
            state.submissions.push_back(super::state::Submission {
                id: submission_id.clone(),
                text: text.clone(),
                attachments: Vec::new(),
                placed,
            });
            vec![Effect::SendPrompt {
                session_id: state.session_id.clone(),
                text: text.clone(),
                submission_id: submission_id.clone(),
                attachments: Vec::new(),
            }]
        }
        Event::Notice { kind, text } => {
            state.notice(*kind, text.clone(), None);
            Vec::new()
        }
        Event::ShellRequested { .. }
        | Event::ShellEmpty
        | Event::ShellFinished { .. }
        | Event::SideAsked { .. }
        | Event::SideUsage
        | Event::SideAccepted { .. }
        | Event::SideRefused { .. }
        | Event::SideNotSent { .. }
        | Event::SideUnknown { .. }
        | Event::SideClock { .. } => crate::side::reduce::apply(state, &event),
        Event::QueueCallResult { .. }
        | Event::QueueCallRefused { .. }
        | Event::QueueCallUnknown { .. }
        | Event::SteerResult(_)
        | Event::QueueReconcileDue { .. } => crate::queue::flow::on_event(state, &event),
        Event::HermesCommand { cell } => {
            state.hermes_command(cell.clone());
            Vec::new()
        }
        Event::Background(event) => {
            let produced = crate::background::update(state.background_mut(), event.clone());
            fold_background(state, produced)
        }
        _ => super::prompts::apply(state, &event),
    };
    state.advance_prefix();
    effects
}

/// Queue a prompt of this client and ask for it to be sent.
///
/// An idle session draws the user cell at once. A busy one draws it when the prompt starts.
pub(crate) fn submit(
    state: &mut SessionState,
    text: &str,
    display: &str,
    submission_id: &str,
    attachments: &[crate::attachments::Attachment],
) -> Vec<Effect> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let placed = state.turn != TurnState::Running && state.submissions.is_empty();
    if placed {
        state.user(display.to_owned());
    }
    crate::queue::flow::note_submit(state, text, submission_id, placed);
    if let Some(entry) = state
        .queue
        .ledger
        .get_mut(&crate::queue::SubmissionId(submission_id.to_owned()))
    {
        entry.attachments = attachments
            .iter()
            .map(|attachment| {
                serde_json::json!({
                    "path": attachment.path, "mime": attachment.mime,
                })
            })
            .collect();
    }
    state.submissions.push_back(super::state::Submission {
        id: submission_id.to_owned(),
        text: display.to_owned(),
        attachments: attachments.to_vec(),
        placed,
    });
    vec![Effect::SendPrompt {
        session_id: state.session_id.clone(),
        text: text.to_owned(),
        submission_id: submission_id.to_owned(),
        attachments: attachments.to_vec(),
    }]
}

/// Apply background follow-ups. Notices and result cells stay in the session. An interrupt uses
/// the turn path. Everything else is handed to the application.
fn fold_background(
    state: &mut SessionState,
    produced: Vec<crate::background::BackgroundEffect>,
) -> Vec<Effect> {
    use crate::background::{BackgroundEffect, NoticeLevel};
    let mut out = Vec::new();
    for effect in produced {
        match effect {
            BackgroundEffect::InsertNotice { level, text } => {
                let kind = match level {
                    NoticeLevel::Info => NoticeKind::System,
                    NoticeLevel::Warning => NoticeKind::Warning,
                    NoticeLevel::Error => NoticeKind::Error,
                };
                state.notice(kind, text, None);
            }
            BackgroundEffect::InsertResult { cell } => {
                state.insert_final(Cell::Background(cell));
            }
            BackgroundEffect::Interrupt => out.extend(interrupt(state)),
            other => out.push(Effect::Background(other)),
        }
    }
    out
}

/// Ask Hermes to stop the running turn. Nothing happens unless a turn runs with a known
/// generation.
pub(crate) fn interrupt(state: &mut SessionState) -> Vec<Effect> {
    state.drafting = None;
    if let (TurnState::Running, Some(execution_generation)) =
        (&state.turn, state.execution_generation)
    {
        state.interrupted = true;
        vec![Effect::Interrupt {
            session_id: state.session_id.clone(),
            execution_generation,
        }]
    } else {
        Vec::new()
    }
}

fn session(
    state: &mut SessionState,
    name: &str,
    payload: &Value,
    generation: Option<i64>,
    admission: Option<&str>,
    unknown: bool,
) {
    // The drafted tool name ends when the call starts (even while latched, so a stale name
    // never shows) or the turn ends. It is set only while no interrupt is latched.
    if matches!(
        name,
        "tool.start" | "message.start" | "message.complete" | "error"
    ) {
        state.drafting = None;
    }
    if name == "tool.generating" {
        if !state.interrupted {
            state.drafting = payload
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned);
        }
        return;
    }
    if state.interrupted
        && matches!(
            name,
            "message.delta"
                | "message.interim"
                | "reasoning.delta"
                | "reasoning.available"
                | "thinking.delta"
                | "tool.start"
                | "tool.progress"
                | "tool.complete"
        )
    {
        return;
    }
    match name {
        "btw.complete" => crate::side::reduce::btw_complete(state, payload),
        "message.start" => {
            state.begin_turn();
            state.execution_generation = generation;
            state.interrupted = false;
            state.activity = None;
            if let Some(text) = nonblank(payload, "display_text") {
                let kind = if payload["notification_category"].as_str() == Some("diagnostic") {
                    NoticeKind::Diagnostic
                } else {
                    NoticeKind::Automation
                };
                state.notice(kind, text.to_owned(), None);
            }
            crate::queue::flow::start_cell(state, admission);
        }
        "message.delta" => match payload.get("text") {
            Some(Value::Null) => state.seal_assistant(),
            Some(Value::String(text)) if !text.is_empty() => {
                match state.turn {
                    TurnState::Idle => state.turn = TurnState::Running,
                    TurnState::Finished(_) => state.begin_turn(),
                    TurnState::Running => {}
                }
                state.seal_reasoning();
                let text = if state.assistant.is_none() {
                    trim_leading_breaks(text)
                } else {
                    text
                };
                if !text.is_empty() {
                    if let Some(index) = state.assistant {
                        if let Cell::Assistant(cell) = &mut state.cells[index] {
                            cell.text.push_str(text);
                        }
                    } else {
                        add_assistant(state, text.to_owned(), false, false, false);
                    }
                }
            }
            _ => {}
        },
        "message.interim" => {
            if let Some(text) = payload["text"].as_str() {
                if payload["already_streamed"].as_bool() == Some(true) {
                    if let Some(index) = state.assistant.take() {
                        if let Cell::Assistant(cell) = &mut state.cells[index] {
                            if cell.text.trim() != text.trim() {
                                cell.text = text.to_owned();
                            }
                            cell.sealed = true;
                            cell.interim = true;
                        }
                    } else {
                        add_assistant(state, text.to_owned(), true, true, false);
                    }
                } else {
                    state.seal_assistant();
                    add_assistant(state, text.to_owned(), true, true, false);
                }
            }
        }
        "reasoning.delta" | "reasoning.available" => {
            if name == "reasoning.available"
                && (state.had_reasoning
                    || payload["text"]
                        .as_str()
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                        .and_then(|text| {
                            state.assistant.and_then(|idx| match &state.cells[idx] {
                                Cell::Assistant(cell) => Some(cell.text.trim().starts_with(text)),
                                _ => None,
                            })
                        })
                        .unwrap_or(false))
            {
                return;
            }
            state.seal_assistant();
            if let Some(text) = payload["text"].as_str().filter(|text| !text.is_empty()) {
                state.had_reasoning = true;
                if let Some(index) = state.reasoning {
                    if let Cell::Reasoning(cell) = &mut state.cells[index] {
                        cell.text.push_str(text);
                    }
                } else {
                    state.reasoning = Some(state.cells.len());
                    state.cells.push(Cell::Reasoning(ReasoningCell {
                        id: state.next_id(),
                        text: text.to_owned(),
                        sealed: false,
                    }));
                }
            }
        }
        "thinking.delta" => {
            if let Some(text) = payload["text"].as_str() {
                state.activity = (!text.is_empty()).then(|| text.to_owned());
            }
        }
        "message.complete" => complete(state, payload),
        "tool.start" | "tool.complete" => tool(state, name, payload),
        "tool.progress" => {
            if let Some(index) = payload["tool_id"]
                .as_str()
                .and_then(|id| state.tools.get(id))
                .copied()
                && let Cell::Tool(cell) = &mut state.cells[index]
                && cell.status == ToolStatus::Running
                && let Some(progress) = payload["progress"].as_f64()
            {
                cell.progress = Some(ToolProgress {
                    progress,
                    total: payload["total"].as_f64(),
                    percent: payload["percent"].as_i64(),
                    message: string(payload, "message"),
                });
            }
        }
        "todo.updated" => {
            if let (Some(revision), Some(todos)) =
                (payload["revision"].as_i64(), payload["todos"].as_array())
                && state
                    .todo
                    .as_ref()
                    .is_none_or(|todo| revision >= todo.revision)
            {
                state.todo = Some(TodoState {
                    revision,
                    todos: todos.clone(),
                });
            }
        }
        "status.update" => {
            if let Some(text) = payload["text"].as_str() {
                if text.is_empty() {
                    state.status = None;
                } else {
                    let note = StatusNote {
                        kind: payload["kind"].as_str().unwrap_or("").to_owned(),
                        text: text.to_owned(),
                    };
                    if state.status.as_ref() != Some(&note) {
                        state.status = Some(note.clone());
                        state.status_log.push(note);
                        if state.status_log.len() > 50 {
                            state.status_log.remove(0);
                        }
                    }
                }
            }
        }
        "session.usage" => super::usage::merge_usage(&mut state.usage, payload, true),
        "approval.request" | "clarify.request" | "approval.settled" | "clarify.settled" => {
            super::prompts::session(state, name, payload)
        }
        "error" => {
            if let Some(text) = payload["message"].as_str() {
                state.notice(NoticeKind::Error, text.to_owned(), None);
            }
        }
        _ if !unknown
            || name.starts_with("subagent.")
            || matches!(
                name,
                "agent.step"
                    | "session.updated"
                    | "session.tools_changed"
                    | "approval.cancelled"
                    | "request.cancel"
            ) => {}
        _ => {
            let safe = if !name.is_empty()
                && name.len() <= 64
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            {
                name
            } else {
                "an unnamed type"
            };
            if state.unknown.len() < 8 && !state.unknown.iter().any(|seen| seen == safe) {
                state.unknown.push(safe.to_owned());
                state.notice(NoticeKind::Diagnostic, format!("Hermes sent an event this version of Talaria does not recognise: {safe}. It is ignored."), None);
            }
        }
    }
}

fn add_assistant(
    state: &mut SessionState,
    text: String,
    sealed: bool,
    interim: bool,
    transformed: bool,
) {
    if !sealed {
        state.assistant = Some(state.cells.len());
    }
    state.cells.push(Cell::Assistant(AssistantCell {
        id: state.next_id(),
        text,
        sealed,
        interim,
        transformed,
    }));
}

fn complete(state: &mut SessionState, payload: &Value) {
    super::usage::merge_usage(&mut state.usage, payload, false);
    if matches!(state.turn, TurnState::Finished(_)) {
        return;
    }
    if state.interrupted {
        state.seal_all();
        interrupted_notice(state);
        state.turn = TurnState::Finished(TurnOutcome::Interrupted);
        state.activity = None;
        return;
    }
    let (outcome, unknown) = match payload.get("status") {
        None | Some(Value::Null) => (TurnOutcome::Completed, false),
        Some(Value::String(status)) if status == "complete" => (TurnOutcome::Completed, false),
        Some(Value::String(status)) if status == "interrupted" => (TurnOutcome::Interrupted, false),
        Some(Value::String(status)) if status == "error" => (TurnOutcome::Failed, false),
        _ => (TurnOutcome::Failed, true),
    };
    state.seal_reasoning();
    if !state.had_reasoning
        && let Some(text) = nonblank(payload, "reasoning")
    {
        state.cells.push(Cell::Reasoning(ReasoningCell {
            id: state.next_id(),
            text: text.to_owned(),
            sealed: true,
        }));
        state.had_reasoning = true;
    }
    match outcome {
        TurnOutcome::Completed => complete_text(state, payload),
        TurnOutcome::Interrupted => interrupted_notice(state),
        TurnOutcome::Failed => {
            let text = if unknown {
                "Hermes ended the turn with a status this version of Talaria does not recognise."
            } else {
                nonblank(payload, "error")
                    .or_else(|| nonblank(payload, "text"))
                    .or_else(|| nonblank(payload, "failure_reason"))
                    .unwrap_or("Hermes could not finish the turn.")
            };
            state.notice(
                NoticeKind::Error,
                text.to_owned(),
                payload["error_surface"]
                    .as_object()
                    .map(|value| Value::Object(value.clone())),
            );
        }
    }
    if let Some(text) = nonblank(payload, "warning") {
        state.notice(NoticeKind::Warning, text.to_owned(), None);
    }
    state.seal_all();
    state.turn = TurnState::Finished(outcome);
    state.activity = None;
}

fn interrupted_notice(state: &mut SessionState) {
    if !state.interrupt_notice {
        state.notice(NoticeKind::Interrupted, "Interrupted".to_owned(), None);
        state.interrupt_notice = true;
    }
}

fn complete_text(state: &mut SessionState, payload: &Value) {
    let text = payload["text"].as_str().unwrap_or("");
    if payload["response_transformed"].as_bool() == Some(true) && !text.trim().is_empty() {
        if let Some(index) = state.assistant.take() {
            if let Cell::Assistant(cell) = &mut state.cells[index] {
                cell.text = trim_leading_breaks(text).to_owned();
                cell.sealed = true;
                cell.transformed = true;
            }
        } else {
            add_assistant(
                state,
                trim_leading_breaks(text).to_owned(),
                true,
                false,
                true,
            );
        }
        return;
    }
    let final_text = remove_whitespace(text);
    if final_text.is_empty() {
        return;
    }
    let previewed = payload["response_previewed"].as_bool() == Some(true);
    let mut shown = String::new();
    for cell in &state.cells[state.turn_start..] {
        if let Cell::Assistant(cell) = cell
            && (!cell.interim || previewed)
        {
            shown.extend(cell.text.chars().filter(|ch| !ch.is_whitespace()));
        }
    }
    if !shown.is_empty() && shown.contains(&final_text) {
        return;
    }
    let addition = if !shown.is_empty() && final_text.starts_with(&shown) {
        text_after_non_whitespace(text, shown.chars().count())
    } else {
        trim_leading_breaks(text).to_owned()
    };
    state.seal_assistant();
    add_assistant(state, addition, true, false, false);
}

fn tool(state: &mut SessionState, name: &str, payload: &Value) {
    state.seal_assistant();
    state.seal_reasoning();
    if name == "tool.start" {
        state.activity = None;
    }
    let Some(id) = payload["tool_id"].as_str() else {
        return;
    };
    let index = if let Some(index) = state.tools.get(id).copied() {
        if name == "tool.start" {
            return;
        }
        index
    } else {
        let index = state.cells.len();
        state.tools.insert(id.to_owned(), index);
        state.cells.push(Cell::Tool(ToolCell {
            id: state.next_id(),
            tool_id: id.to_owned(),
            name: payload["name"].as_str().unwrap_or("").to_owned(),
            context: string(payload, "context").filter(|value| !value.is_empty()),
            args: payload["args"]
                .as_object()
                .filter(|value| !value.is_empty())
                .map(|value| Value::Object(value.clone())),
            labels: payload["labels"].as_array().cloned().unwrap_or_default(),
            status: ToolStatus::Running,
            progress: None,
            summary: None,
            result: None,
            diff: None,
            duration_s: None,
        }));
        index
    };
    if name != "tool.complete" {
        return;
    }
    if let Cell::Tool(cell) = &mut state.cells[index] {
        if cell.status != ToolStatus::Running {
            return;
        }
        cell.status = if payload["is_error"].as_bool() == Some(true) {
            ToolStatus::Failed
        } else {
            ToolStatus::Succeeded
        };
        if cell.context.is_none() {
            cell.context = string(payload, "context").filter(|value| !value.is_empty());
        }
        if cell.args.is_none() {
            cell.args = payload["args"]
                .as_object()
                .map(|value| Value::Object(value.clone()));
        }
        if cell.labels.is_empty() {
            cell.labels = payload["labels"].as_array().cloned().unwrap_or_default();
        }
        cell.summary = string(payload, "summary");
        cell.result = string(payload, "result");
        cell.duration_s = payload["duration_s"].as_f64();
        cell.diff = payload["inline_diff"]
            .as_str()
            .map(strip_terminal_escapes)
            .filter(|text| !text.trim().is_empty());
    }
}

fn string(payload: &Value, key: &str) -> Option<String> {
    payload[key].as_str().map(str::to_owned)
}
fn nonblank<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload[key].as_str().filter(|text| !text.trim().is_empty())
}
