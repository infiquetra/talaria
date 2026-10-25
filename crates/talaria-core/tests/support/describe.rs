//! Golden text for a [`talaria_core::session::SessionState`].
//!
//! The `reduce_frames` example includes this file with
//! `#[path = "../tests/support/describe.rs"]`.
//!
//! Each integration test is its own crate, so a helper used by one binary is dead in the others.

#![allow(dead_code)]

use talaria_core::session::{
    AnswerRefusal, AssistantCell, Cell, Effect, NoticeCell, NoticeKind, PromptCell, PromptKind,
    PromptState, ReasoningCell, SessionState, Settlement, ToolCell, ToolStatus, TurnOutcome,
    TurnState, UserCell,
};
use talaria_core::side::{ShellCell, SideCell};

/// The golden body: one cell per line, then turn, final prefix, prompts and unknown types.
pub fn describe(state: &SessionState) -> String {
    let mut lines = Vec::new();
    for (index, cell) in state.cells().iter().enumerate() {
        lines.push(format!("cell {} {}", index + 1, cell_line(cell)));
    }
    lines.push(format!("turn: {}", turn_line(state.turn())));
    let total = state.cells().len();
    lines.push(format!(
        "final_prefix: {} of {total}",
        state.final_prefix_len()
    ));
    let open = state.open_prompts().count();
    lines.push(if open == 0 {
        "prompts: none".to_string()
    } else {
        format!("prompts: {open} open")
    });
    let unknown = state.unknown_event_types();
    lines.push(if unknown.is_empty() {
        "unknown_event_types: none".to_string()
    } else {
        format!("unknown_event_types: {}", unknown.join(", "))
    });
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// One effect, in the example's layout. Callers print `effects: <n>` themselves.
pub fn effect_line(effect: &Effect) -> String {
    match effect {
        Effect::SendPrompt {
            session_id,
            submission_id,
            ..
        } => format!(
            " SendPrompt session={} submission={submission_id}",
            session_id.as_str()
        ),
        Effect::Interrupt {
            session_id,
            execution_generation,
        } => format!(
            " Interrupt session={} generation={execution_generation}",
            session_id.as_str()
        ),
        Effect::RespondApproval { key, choice } => format!(
            " RespondApproval session={} generation={} prompt={} choice={choice}",
            key.session_id.as_str(),
            key.execution_generation,
            key.prompt_id
        ),
        Effect::RespondClarify { key, .. } => format!(
            " RespondClarify session={} generation={} prompt={}",
            key.session_id.as_str(),
            key.execution_generation,
            key.prompt_id
        ),
        Effect::RefuseAnswer { key, reason } => {
            format!("refused: {} {}", refusal_word(*reason), key.prompt_id)
        }
        Effect::RunShell {
            session_id,
            request_id,
            command,
        } => format!(
            " RunShell session={} request={} command={}",
            session_id.as_str(),
            request_id.0,
            quote(command)
        ),
        Effect::AskSide {
            session_id,
            request_id,
            question,
        } => format!(
            " AskSide session={} request={} question={}",
            session_id.as_str(),
            request_id.0,
            quote(question)
        ),
        Effect::CancelQueued {
            session_id,
            admission_id,
        } => format!(
            " CancelQueued session={} admission={}",
            session_id.as_str(),
            admission_id.0
        ),
        Effect::ReplaceQueued {
            session_id,
            admission_id,
            submission_id,
            ..
        } => format!(
            " ReplaceQueued session={} admission={} submission={}",
            session_id.as_str(),
            admission_id.0,
            submission_id.0
        ),
        Effect::SteerQueued {
            session_id,
            admission_id,
        } => format!(
            " SteerQueued session={} admission={}",
            session_id.as_str(),
            admission_id.0
        ),
        Effect::Steer { session_id, .. } => {
            format!(" Steer session={}", session_id.as_str())
        }
        Effect::ResolveUnknown {
            session_id,
            admission_id,
            execution_generation,
        } => format!(
            " ResolveUnknown session={} admission={} generation={execution_generation}",
            session_id.as_str(),
            admission_id.0
        ),
    }
}

fn refusal_word(reason: AnswerRefusal) -> &'static str {
    match reason {
        AnswerRefusal::NoSuchPrompt => "NoSuchPrompt",
        AnswerRefusal::WrongKind => "WrongKind",
        AnswerRefusal::AlreadyAnswered => "AlreadyAnswered",
        AnswerRefusal::ChoiceNotOffered => "ChoiceNotOffered",
    }
}

fn cell_line(cell: &Cell) -> String {
    match cell {
        Cell::User(cell) => user_line(cell),
        Cell::Assistant(cell) => assistant_line(cell),
        Cell::Reasoning(cell) => reasoning_line(cell),
        Cell::Tool(cell) => tool_line(cell),
        Cell::Notice(cell) => notice_line(cell),
        Cell::Prompt(cell) => prompt_line(cell),
        Cell::Shell(cell) => shell_line(cell),
        Cell::Side(cell) => side_line(cell),
        Cell::HermesCommand(cell) => format!("hermes-command {}", quote(&cell.typed)),
    }
}

fn shell_line(cell: &ShellCell) -> String {
    format!("shell {} {:?}", quote(&cell.command), cell.outcome)
}

fn side_line(cell: &SideCell) -> String {
    format!("side {} {:?}", quote(&cell.question), cell.outcome)
}

fn user_line(cell: &UserCell) -> String {
    format!("user {}", quote(&cell.text))
}

fn assistant_line(cell: &AssistantCell) -> String {
    let mut flags = Vec::new();
    if cell.interim {
        flags.push("interim");
    }
    if cell.transformed {
        flags.push("transformed");
    }
    if flags.is_empty() {
        format!("assistant {}", quote(&cell.text))
    } else {
        format!("assistant[{}] {}", flags.join(","), quote(&cell.text))
    }
}

fn reasoning_line(cell: &ReasoningCell) -> String {
    format!("reasoning {}", quote(&cell.text))
}

fn turn_line(turn: &TurnState) -> String {
    match turn {
        TurnState::Idle => "Idle".to_string(),
        TurnState::Running => "Running".to_string(),
        TurnState::Finished(TurnOutcome::Completed) => "Finished(Completed)".to_string(),
        TurnState::Finished(TurnOutcome::Failed) => "Finished(Failed)".to_string(),
        TurnState::Finished(TurnOutcome::Interrupted) => "Finished(Interrupted)".to_string(),
    }
}

fn tool_line(cell: &ToolCell) -> String {
    let context = match &cell.context {
        Some(context) => format!("context={}", quote(context)),
        None => "context=none".to_string(),
    };
    format!(
        "tool[{}] {} {context} id={}",
        tool_word(cell.status),
        cell.name,
        cell.tool_id
    )
}

fn tool_word(status: ToolStatus) -> &'static str {
    match status {
        ToolStatus::Running => "running",
        ToolStatus::Succeeded => "ok",
        ToolStatus::Failed => "failed",
        ToolStatus::Unfinished => "unfinished",
        ToolStatus::Unknown => "unknown",
    }
}

fn notice_line(cell: &NoticeCell) -> String {
    format!("notice[{}] {}", notice_word(cell.kind), quote(&cell.text))
}

fn notice_word(kind: NoticeKind) -> &'static str {
    match kind {
        NoticeKind::Interrupted => "interrupted",
        NoticeKind::Error => "error",
        NoticeKind::Warning => "warning",
        NoticeKind::Automation => "automation",
        NoticeKind::Diagnostic => "diagnostic",
        NoticeKind::System => "system",
    }
}

fn prompt_line(cell: &PromptCell) -> String {
    let kind = match cell.kind {
        PromptKind::Approval => "approval",
        PromptKind::Clarify => "clarify",
    };
    let subject = if cell.kind == PromptKind::Approval {
        match &cell.command {
            Some(command) => format!("command={}", quote(command)),
            None => "command=\"\"".to_string(),
        }
    } else {
        match &cell.question {
            Some(question) => format!("question={}", quote(question)),
            None => "question=\"\"".to_string(),
        }
    };
    format!(
        "prompt[{kind} {}] {subject}",
        prompt_state(&cell.state, cell.kind)
    )
}

fn prompt_state(state: &PromptState, kind: PromptKind) -> String {
    match state {
        PromptState::Open => "Open".to_string(),
        PromptState::Answering(value) => format!("Answering({})", prompt_value(value, kind)),
        PromptState::Settled(Settlement::Answered(value)) => {
            format!("Settled(Answered({}))", prompt_value(value, kind))
        }
        PromptState::Settled(Settlement::Elsewhere) => "Settled(Elsewhere)".to_string(),
    }
}

fn prompt_value(value: &str, kind: PromptKind) -> String {
    match kind {
        PromptKind::Approval => value.to_string(),
        PromptKind::Clarify => quote(value),
    }
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if (other as u32) < 0x20 || other == '\u{007F}' => {
                out.push_str(&format!("\\u{{{:x}}}", other as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}
