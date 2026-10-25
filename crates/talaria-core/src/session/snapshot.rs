//! `SessionState::from_snapshot`.

use hermes_contract::schemas::{CanonicalPendingPromptKind, CanonicalSnapshot};
use serde_json::Value;

use super::{
    AssistantCell, Cell, NoticeKind, PromptCell, PromptKey, PromptKind, PromptState, ReasoningCell,
    ReplayCursor, SessionId, SessionState, TodoState, ToolCell, ToolStatus, TurnState,
};

pub(crate) fn from_snapshot(snapshot: CanonicalSnapshot) -> SessionState {
    let mut state = SessionState::empty(SessionId::new(snapshot.session_id));
    state.queue.apply_snapshot(
        &snapshot.pending,
        snapshot.authority_epoch,
        snapshot.execution_generation,
        &snapshot.replay_epoch,
        u64::try_from(snapshot.last_sequence).unwrap_or(0),
    );
    state.cursor = ReplayCursor {
        replay_epoch: Some(snapshot.replay_epoch),
        last_seq: u64::try_from(snapshot.last_sequence).unwrap_or(0),
    };
    state.execution_generation = Some(snapshot.execution_generation);
    state.subscription_id = Some(snapshot.subscription_id);
    state.stored_session_id = Some(snapshot.stored_session_id);
    state.queued_admissions_ignored = snapshot.pending.len();
    // Hermes sets the flag on the first row of `messages` only. The display rows never carry it.
    state.history_truncated = snapshot
        .messages
        .first()
        .and_then(|row| row.archived_truncated)
        .unwrap_or(false);
    state.turn = if snapshot.running {
        TurnState::Running
    } else {
        TurnState::Idle
    };
    state.todo = snapshot.todo_state.map(|todo| TodoState {
        revision: todo.revision,
        todos: todo.todos.into_iter().map(Value::Object).collect(),
    });
    for row in snapshot.display_messages.unwrap_or(snapshot.messages) {
        let text = row.text.or_else(|| {
            row.content
                .as_ref()
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        match row.role.as_str() {
            "user" => {
                let steer = row.display_kind.as_deref() == Some("steer");
                if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
                    if steer {
                        // A steer is a note to the agent, not something the person typed as a prompt.
                        state.notice(
                            NoticeKind::System,
                            crate::queue::flow::history_steer(&text),
                            None,
                        );
                    } else {
                        let text = crate::automation::fold_user_text(&text)
                            .map_or(text, |folded| folded.display);
                        state.user(text);
                    }
                }
            }
            "assistant" => {
                if let Some(text) = row.reasoning.filter(|text| !text.trim().is_empty()) {
                    state.cells.push(Cell::Reasoning(ReasoningCell {
                        id: state.next_id(),
                        text,
                        sealed: true,
                    }));
                }
                if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
                    state.cells.push(Cell::Assistant(AssistantCell {
                        id: state.next_id(),
                        text,
                        sealed: true,
                        interim: false,
                        transformed: false,
                    }));
                }
            }
            "tool" => state.cells.push(Cell::Tool(ToolCell {
                id: state.next_id(),
                tool_id: row.tool_call_id.unwrap_or_default(),
                name: row.name.unwrap_or_default(),
                context: row.context,
                args: row.args.map(Value::Object),
                labels: row
                    .labels
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|label| serde_json::to_value(label).ok())
                    .collect(),
                status: ToolStatus::Unknown,
                progress: None,
                summary: None,
                result: row
                    .content
                    .and_then(|value| value.as_str().map(str::to_owned)),
                diff: None,
                duration_s: None,
            })),
            "system" => {
                if let Some(text) = text {
                    state.notice(NoticeKind::System, text, None);
                }
            }
            _ => {}
        }
    }
    state.turn_start = state.cells.len();
    for prompt in snapshot.prompts {
        let cell = PromptCell {
            id: state.next_id(),
            key: PromptKey {
                session_id: state.session_id.clone(),
                prompt_id: prompt.prompt_id,
                execution_generation: prompt.execution_generation,
            },
            kind: match prompt.kind {
                CanonicalPendingPromptKind::Approval => PromptKind::Approval,
                CanonicalPendingPromptKind::Clarify => PromptKind::Clarify,
            },
            command: prompt.command,
            description: prompt.description,
            question: prompt.question,
            choices: prompt.choices,
            multi_select: prompt.multi_select.unwrap_or(false),
            edit: prompt.edit.map(Value::Object),
            timeout_s: prompt
                .timeout_s
                .and_then(|value| Value::from(value).as_f64()),
            state: PromptState::Open,
            sealed: false,
        };
        super::prompts::insert(&mut state, cell);
    }
    state.advance_prefix();
    state
}
