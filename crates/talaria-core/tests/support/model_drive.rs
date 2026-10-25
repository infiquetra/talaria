//! Shared picker steps for issue 230. The first call in each test is `ModelsState::update`.
//!
//! This file is separate from the session reducer's helper so the two test crates do not share one module.

#![allow(dead_code)]

use hermes_contract::schemas::ModelOptionsResult;
use serde_json::{Map, Value, json};
use talaria_core::models::{
    ModelsEffect, ModelsInput, ModelsState, NoticeLevel, PickerKey, RowKey, SavedModels,
};
use talaria_core::mutation::{Confirmation, MutationOutcome, MutationRequest};

pub const SID: &str = "sess-model-1";
pub const PROFILE: &str = "fixture";

pub fn payload(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("payload is not an object: {other}"),
    }
}

pub fn notices(effects: &[ModelsEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            ModelsEffect::Notice { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

pub fn mutates(effects: &[ModelsEffect]) -> Vec<MutationRequest> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            ModelsEffect::Mutate(request) => Some(request.clone()),
            _ => None,
        })
        .collect()
}

pub fn saved_files(effects: &[ModelsEffect]) -> Vec<SavedModels> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            ModelsEffect::SaveFile(saved) => Some(saved.clone()),
            _ => None,
        })
        .collect()
}

pub fn has_close(effects: &[ModelsEffect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, ModelsEffect::CloseOverlay))
}

pub fn selected_key(state: &ModelsState) -> Option<RowKey> {
    let view = state.view().expect("the picker is open");
    let index = view.selected.expect("a row is selected");
    view.rows.get(index).and_then(|row| row.key.clone())
}

pub fn view_parts(state: &ModelsState) -> (String, String, String, Option<String>) {
    let view = state.view().expect("the picker is open");
    (
        view.title.to_owned(),
        view.effort.to_owned(),
        view.filter.to_owned(),
        view.banner.map(str::to_owned),
    )
}

pub fn open_loaded(
    catalog: ModelOptionsResult,
    effort: Option<&str>,
    saved: SavedModels,
) -> ModelsState {
    let mut state = ModelsState::new("L", Ok(saved));
    state.set_list_height(10);
    let _ = state.update(ModelsInput::SessionOpened {
        session_id: SID.to_owned(),
        revision: Some(12),
        execution_generation: Some(4),
    });
    let _ = state.update(ModelsInput::ForegroundChanged {
        session_id: Some(SID.to_owned()),
    });
    let _ = state.update(ModelsInput::OpenPicker {
        session_id: Some(SID.to_owned()),
        profile: PROFILE.to_owned(),
    });
    let _ = state.update(ModelsInput::OptionsLoaded {
        session_id: SID.to_owned(),
        result: catalog,
    });
    let _ = state.update(ModelsInput::EffortRead {
        session_id: SID.to_owned(),
        value: effort.map(str::to_owned),
    });
    state
}

pub fn press(state: &mut ModelsState, key: PickerKey) -> Vec<ModelsEffect> {
    state.update(ModelsInput::Picker(key))
}

pub fn type_filter(state: &mut ModelsState, text: &str) -> Vec<ModelsEffect> {
    let mut effects = Vec::new();
    for ch in text.chars() {
        effects.extend(press(state, PickerKey::Char(ch)));
    }
    effects
}

pub fn move_down_to(state: &mut ModelsState, key: &RowKey) {
    for _ in 0..40 {
        if selected_key(state).as_ref() == Some(key) {
            return;
        }
        let _ = press(state, PickerKey::Down);
    }
    panic!("the row was not reached by moving down");
}

pub fn levels(effects: &[ModelsEffect]) -> Vec<NoticeLevel> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            ModelsEffect::Notice { level, .. } => Some(*level),
            _ => None,
        })
        .collect()
}

pub fn confirmation(token: &str, message: &str) -> Confirmation {
    Confirmation {
        token: token.to_owned(),
        message: message.to_owned(),
        target_model: "glm-5".to_owned(),
        target_provider: Some("ollama-cloud".to_owned()),
        pick: payload(json!({
            "model": "glm-5",
            "provider": "ollama-cloud",
            "reasoning": "xhigh"
        })),
    }
}

pub fn committed(value: Value) -> MutationOutcome {
    MutationOutcome::Committed(value)
}

pub fn one_mutate(effects: &[ModelsEffect]) -> MutationRequest {
    let found = mutates(effects);
    assert_eq!(found.len(), 1, "expected one mutate request");
    found.into_iter().next().expect("one request")
}
