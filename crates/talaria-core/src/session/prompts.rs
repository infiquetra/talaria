//! Prompt registry transitions. [`super::update`] is the only caller.

use super::{
    AnswerRefusal, Effect, Event, NoticeKind, PromptCell, PromptKey, PromptKind, PromptState,
    SessionState, Settlement, TurnState,
};
use serde_json::Value;

pub(crate) fn apply(state: &mut SessionState, event: &Event) -> Vec<Effect> {
    let (key, kind, value) = match event {
        Event::ServerRequestAnswered { id } | Event::ServerRequestCancelled { id } => {
            let prompt = state.cells.iter_mut().find_map(|cell| match cell {
                super::Cell::Prompt(prompt)
                    if prompt.kind == PromptKind::ServerRequest
                        && prompt.key.prompt_id == *id
                        && prompt.state == PromptState::Open =>
                {
                    Some(prompt)
                }
                _ => None,
            });
            let settled = if let Some(prompt) = prompt {
                let settlement = if matches!(event, Event::ServerRequestAnswered { .. }) {
                    Settlement::Sent
                } else {
                    Settlement::Elsewhere
                };
                prompt.state = PromptState::Settled(settlement);
                prompt.sealed = true;
                Some(prompt.key.clone())
            } else {
                None
            };
            if let Some(key) = settled {
                note_left_open(state, &key);
            }
            return Vec::new();
        }
        Event::AnswerApproval { key, choice } => (key, PromptKind::Approval, choice),
        Event::AnswerClarify { key, answer } => (key, PromptKind::Clarify, answer),
        Event::AnswerAccepted { key } => {
            // Hermes took the answer, so no later reply can change it: seal the prompt now and
            // let what follows it reach scrollback while the turn runs.
            let mut left_open = false;
            if let Some(prompt) = prompt_mut(state, key) {
                match &prompt.state {
                    PromptState::Answering(value) => {
                        prompt.state = PromptState::Settled(Settlement::Answered(value.clone()));
                        prompt.sealed = true;
                        left_open = true;
                    }
                    PromptState::Settled(Settlement::Answered(_)) => prompt.sealed = true,
                    _ => {}
                }
            }
            if left_open {
                note_left_open(state, key);
            }
            return Vec::new();
        }
        Event::AnswerFailed { key } | Event::AnswerAlreadyResolved { key } => {
            let mut left_open = false;
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
                        left_open = matches!(prompt.state, PromptState::Answering(_));
                        prompt.state = PromptState::Settled(Settlement::Elsewhere);
                        prompt.sealed = true;
                    }
                    _ => {}
                }
            }
            if left_open {
                note_left_open(state, key);
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

/// Seal listed approvals, and when `cancelled_count` is larger than that list, withdraw
/// every approval still open in this session.
///
/// `ApprovalCancelledPayload` says `request_ids` omits entries that have no id, so the
/// count and the list can disagree. A payload whose `session_id` is not this session
/// changes nothing. `stored_session_id` does not select a different session.
pub(crate) fn cancel_approvals(state: &mut SessionState, payload: &Value) {
    let Some(session_id) = payload["session_id"]
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return;
    };
    if session_id != state.session_id.as_str() {
        return;
    }
    let ids: Vec<&str> = payload["request_ids"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let mut matched = 0i64;
    for key in open_approval_keys(state) {
        if ids.iter().any(|id| *id == key.prompt_id) {
            withdraw_approval(state, &key);
            matched += 1;
        }
    }
    let cancelled_count = payload["cancelled_count"].as_i64().unwrap_or(0);
    if cancelled_count > matched {
        for key in open_approval_keys(state) {
            withdraw_approval(state, &key);
        }
    }
}

fn open_approval_keys(state: &SessionState) -> Vec<super::PromptKey> {
    state
        .cells
        .iter()
        .filter_map(|cell| match cell {
            super::Cell::Prompt(prompt)
                if prompt.kind == PromptKind::Approval
                    && matches!(prompt.state, PromptState::Open | PromptState::Answering(_)) =>
            {
                Some(prompt.key.clone())
            }
            _ => None,
        })
        .collect()
}

fn withdraw_approval(state: &mut SessionState, key: &super::PromptKey) {
    let Some(prompt) = prompt_mut(state, key) else {
        return;
    };
    if prompt.kind != PromptKind::Approval || matches!(prompt.state, PromptState::Settled(_)) {
        return;
    }
    prompt.state = PromptState::Settled(Settlement::Elsewhere);
    prompt.sealed = true;
    note_left_open(state, key);
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
        let live = prompt_mut(state, &key).is_some_and(|prompt| {
            matches!(prompt.state, PromptState::Open | PromptState::Answering(_))
        });
        if let Some(prompt) = prompt_mut(state, &key) {
            match &prompt.state {
                PromptState::Open => {
                    prompt.state = PromptState::Settled(Settlement::Elsewhere);
                    prompt.sealed = true;
                }
                PromptState::Answering(value) => {
                    prompt.state = PromptState::Settled(Settlement::Answered(value.clone()));
                    // Until Hermes replies to the answer, a late `already_resolved` may still
                    // correct it; `AnswerAccepted` seals it as soon as Hermes accepts it.
                    prompt.sealed = !running;
                }
                PromptState::Settled(_) => {}
            }
        }
        if live {
            note_left_open(state, &key);
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
                choices: bounded_choices(payload.get("choices")),
                multi_select: payload["multi_select"].as_bool().unwrap_or(false),
                edit: payload["edit"]
                    .is_object()
                    .then(|| Value::Object(serde_json::Map::new())),
                timeout_s: payload["timeout_s"].as_f64(),
                state: PromptState::Open,
                sealed: false,
            },
        );
    }
}

pub(crate) fn insert(state: &mut SessionState, mut cell: PromptCell) {
    if state.prompts.contains_key(&cell.key) {
        return;
    }
    let live = matches!(cell.state, PromptState::Open | PromptState::Answering(_));
    if cell.key.prompt_id.len() > 256 {
        return;
    }
    if live && state.open_prompt_count >= MAX_OPEN_PROMPTS {
        return;
    }
    bound_prompt_fields(&mut cell);
    if live {
        state.open_prompt_count += 1;
    }
    state.prompts.insert(cell.key.clone(), state.cells.len());
    state.cells.push(super::Cell::Prompt(cell));
}

const MAX_OPEN_PROMPTS: usize = 64;
const MAX_SETTLED_PROMPTS: usize = 256;
const MAX_PROMPT_TEXT: usize = 4096;
const MAX_CHOICE_BYTES: usize = 16 * 1024;
const MAX_STORED_CHOICES: usize = 32;
const PROMPT_TEXT_SUFFIX: &str = "… (truncated)";

fn note_left_open(state: &mut SessionState, key: &PromptKey) {
    state.open_prompt_count = state.open_prompt_count.saturating_sub(1);
    state.settled_order.push_back(key.clone());
    prune_settled(state);
}

fn prune_settled(state: &mut SessionState) {
    if state.settled_order.len() <= MAX_SETTLED_PROMPTS {
        return;
    }
    let drop_count = state.settled_order.len() - MAX_SETTLED_PROMPTS;
    let mut remove = vec![false; state.cells.len()];
    for _ in 0..drop_count {
        let Some(key) = state.settled_order.pop_front() else {
            break;
        };
        if let Some(index) = state.prompts.get(&key).copied()
            && let Some(flag) = remove.get_mut(index)
        {
            *flag = true;
        }
    }
    if !remove.iter().any(|flag| *flag) {
        return;
    }
    let mut removed_keys = Vec::new();
    let mut new_index = vec![0; remove.len()];
    let mut kept = Vec::with_capacity(state.cells.len() - drop_count);
    let mut removed_before = 0usize;
    for (index, cell) in state.cells.drain(..).enumerate() {
        if remove[index] {
            if let super::Cell::Prompt(prompt) = cell {
                removed_keys.push(prompt.key);
            }
            removed_before += 1;
            continue;
        }
        new_index[index] = index - removed_before;
        kept.push(cell);
    }
    state.removed_cells = state.removed_cells.saturating_add(removed_before as u64);
    state.cells = kept;
    for key in removed_keys {
        state.prompts.remove(&key);
    }
    for index in state.prompts.values_mut() {
        *index = new_index[*index];
    }
    for index in state.tools.values_mut() {
        *index = new_index[*index];
    }
    if let Some(index) = state.assistant.as_mut() {
        *index = new_index[*index];
    }
    if let Some(index) = state.reasoning.as_mut() {
        *index = new_index[*index];
    }
    let removed_strictly_before = |position: usize| {
        remove[..position.min(remove.len())]
            .iter()
            .filter(|flag| **flag)
            .count()
    };
    state.final_prefix = state
        .final_prefix
        .saturating_sub(removed_strictly_before(state.final_prefix));
    state.turn_start = state
        .turn_start
        .saturating_sub(removed_strictly_before(state.turn_start));
    state.commit_cursor = state
        .commit_cursor
        .saturating_sub(removed_strictly_before(state.commit_cursor));
}

fn string(payload: &Value, key: &str) -> Option<String> {
    payload[key].as_str().map(bounded_text)
}

fn bounded_choices(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|choice| choice.len() <= MAX_CHOICE_BYTES)
                .take(MAX_STORED_CHOICES)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn bound_prompt_fields(cell: &mut PromptCell) {
    cell.command = cell.command.take().map(|value| bounded_text(&value));
    cell.description = cell.description.take().map(|value| bounded_text(&value));
    cell.question = cell.question.take().map(|value| bounded_text(&value));
    cell.choices = cell
        .choices
        .iter()
        .filter(|choice| choice.len() <= MAX_CHOICE_BYTES)
        .take(MAX_STORED_CHOICES)
        .cloned()
        .collect();
}

fn bounded_text(value: &str) -> String {
    if value.len() <= MAX_PROMPT_TEXT {
        return value.to_owned();
    }
    let mut end = MAX_PROMPT_TEXT - PROMPT_TEXT_SUFFIX.len();
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = String::with_capacity(end + PROMPT_TEXT_SUFFIX.len());
    out.push_str(&value[..end]);
    out.push_str(PROMPT_TEXT_SUFFIX);
    out
}
