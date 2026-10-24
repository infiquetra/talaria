//! Prompt registry transitions. [`super::update`] is the only caller.

use super::{
    AnswerRefusal, Effect, Event, NoticeKind, PromptCell, PromptKey, PromptKind, PromptState,
    SessionState, Settlement, TurnState,
};
use serde_json::Value;

pub(crate) fn apply(state: &mut SessionState, event: &Event) -> Vec<Effect> {
    let (key, kind, value) = match event {
        Event::AnswerApproval { key, choice } => (key, PromptKind::Approval, choice),
        Event::AnswerClarify { key, answer } => (key, PromptKind::Clarify, answer),
        Event::AnswerFailed { key } | Event::AnswerAlreadyResolved { key } => {
            if let Some(prompt) = prompt_mut(state, key)
                && !prompt.sealed
            {
                match event {
                    Event::AnswerFailed { .. }
                        if matches!(prompt.state, PromptState::Answering(_)) =>
                    {
                        prompt.state = PromptState::Open
                    }
                    Event::AnswerAlreadyResolved { .. }
                        if matches!(
                            prompt.state,
                            PromptState::Answering(_)
                                | PromptState::Settled(Settlement::Answered(_))
                        ) =>
                    {
                        prompt.state = PromptState::Settled(Settlement::Elsewhere);
                        prompt.sealed = true;
                    }
                    _ => {}
                }
            }
            return Vec::new();
        }
        _ => return Vec::new(),
    };
    let refusal = match prompt_mut(state, key) {
        None => Some(AnswerRefusal::NoSuchPrompt),
        Some(prompt) if prompt.kind != kind => Some(AnswerRefusal::WrongKind),
        Some(prompt) if prompt.state != PromptState::Open => Some(AnswerRefusal::AlreadyAnswered),
        Some(prompt) if kind == PromptKind::Approval && !prompt.choices.contains(value) => {
            Some(AnswerRefusal::ChoiceNotOffered)
        }
        Some(prompt) => {
            prompt.state = PromptState::Answering(value.clone());
            None
        }
    };
    vec![if let Some(reason) = refusal {
        Effect::RefuseAnswer {
            key: key.clone(),
            reason,
        }
    } else if kind == PromptKind::Approval {
        Effect::RespondApproval {
            key: key.clone(),
            choice: value.clone(),
        }
    } else {
        Effect::RespondClarify {
            key: key.clone(),
            answer: value.clone(),
        }
    }]
}

fn prompt_mut<'a>(state: &'a mut SessionState, key: &PromptKey) -> Option<&'a mut PromptCell> {
    let index = *state.prompts.get(key)?;
    match state.cells.get_mut(index)? {
        super::Cell::Prompt(prompt) => Some(prompt),
        _ => None,
    }
}

pub(crate) fn session(state: &mut SessionState, name: &str, payload: &Value) {
    let key = payload["prompt_id"]
        .as_str()
        .zip(payload["execution_generation"].as_i64())
        .map(|(id, generation)| PromptKey {
            session_id: state.session_id.clone(),
            prompt_id: id.to_owned(),
            execution_generation: generation,
        });
    let Some(key) = key else {
        if name.ends_with(".request") {
            state.notice(
                NoticeKind::Diagnostic,
                "Hermes asked a question this client cannot answer.".to_owned(),
                None,
            );
        }
        return;
    };
    if name.ends_with(".settled") {
        let running = state.turn == TurnState::Running;
        if let Some(prompt) = prompt_mut(state, &key) {
            match &prompt.state {
                PromptState::Open => {
                    prompt.state = PromptState::Settled(Settlement::Elsewhere);
                    prompt.sealed = true;
                }
                PromptState::Answering(value) => {
                    prompt.state = PromptState::Settled(Settlement::Answered(value.clone()));
                    prompt.sealed = !running;
                }
                PromptState::Settled(_) => {}
            }
        }
    } else if !state.prompts.contains_key(&key) {
        insert(
            state,
            PromptCell {
                id: state.next_id(),
                key,
                kind: if name == "approval.request" {
                    PromptKind::Approval
                } else {
                    PromptKind::Clarify
                },
                command: string(payload, "command"),
                description: string(payload, "description"),
                question: string(payload, "question"),
                choices: payload["choices"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
                multi_select: payload["multi_select"].as_bool().unwrap_or(false),
                edit: payload["edit"]
                    .as_object()
                    .map(|value| Value::Object(value.clone())),
                timeout_s: payload["timeout_s"].as_f64(),
                state: PromptState::Open,
                sealed: false,
            },
        );
    }
}

pub(crate) fn insert(state: &mut SessionState, cell: PromptCell) {
    if !state.prompts.contains_key(&cell.key) {
        state.prompts.insert(cell.key.clone(), state.cells.len());
        state.cells.push(super::Cell::Prompt(cell));
    }
}

fn string(payload: &Value, key: &str) -> Option<String> {
    payload[key].as_str().map(str::to_owned)
}
