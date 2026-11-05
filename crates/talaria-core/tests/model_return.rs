#[path = "support/model_drive.rs"]
mod drive;
use serde_json::json;
use talaria_core::models::ListScope;
use talaria_core::models::{
    ModelsEffect, ModelsInput, ModelsState, PickerKey, RowKey, SavedModels,
};

fn catalogue(ready: bool) -> hermes_contract::schemas::ModelOptionsResult {
    serde_json::from_value(json!({"provider":"example-current","model":"example-model","providers":[
        {"slug":"example-current","name":"Example current","authenticated":true,"models":["example-model"],"capabilities":{"example-model":{"fast":false,"reasoning":true}}},
        {"slug":"example-setup","name":"Example setup","authenticated":ready,"models":if ready {vec!["first-new-model","second-new-model"]}else{vec![]}}
    ]})).unwrap()
}
fn no_mutation(effects: &[ModelsEffect]) {
    assert!(!effects.iter().any(|e| matches!(
        e,
        ModelsEffect::Mutate(_) | ModelsEffect::SetEffort { .. } | ModelsEffect::SaveFile(_)
    )));
}

fn at_stub(scope: ListScope) -> ModelsState {
    let mut state = drive::open_loaded(catalogue(false), Some("high"), SavedModels::empty());
    drive::press(&mut state, PickerKey::Right);
    if scope == ListScope::SetUp {
        drive::press(&mut state, PickerKey::ChipsPrev);
    }
    drive::type_filter(&mut state, "Example setup");
    assert_eq!(
        drive::selected_key(&state),
        Some(RowKey::NotSetUp {
            provider: "example-setup".into()
        })
    );
    state
}

#[test]
fn stub_enter_still_closes_then_opens_provider_setup() {
    let mut state = at_stub(ListScope::All);
    let effects = drive::press(&mut state, PickerKey::Enter);
    no_mutation(&effects);
    let close = effects
        .iter()
        .position(|e| matches!(e, ModelsEffect::CloseOverlay))
        .unwrap();
    let open = effects
        .iter()
        .position(|e| matches!(e,ModelsEffect::OpenProviders{slug} if slug=="example-setup"))
        .unwrap();
    assert!(close < open);
    assert!(state.view().is_none());
}

#[test]
fn resume_restores_filter_scope_effort_and_route_then_reloads() {
    let mut state = at_stub(ListScope::SetUp);
    let before = state.view().unwrap();
    let filter = before.filter.to_owned();
    let effort = before.effort.to_owned();
    let selected = drive::selected_key(&state);
    drive::press(&mut state, PickerKey::Enter);
    let effects = state.update(ModelsInput::ResumePicker {
        provider: "example-setup".into(),
    });
    no_mutation(&effects);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, ModelsEffect::OpenOverlay))
    );
    assert!(effects.iter().any(
        |e| matches!(e,ModelsEffect::LoadOptions{session_id,refresh:true} if session_id==drive::SID)
    ));
    let view = state.view().expect("return to remembered picker");
    assert_eq!(view.filter, filter);
    assert_eq!(view.browse.unwrap().scope, ListScope::SetUp);
    assert_eq!(view.effort, effort);
    assert_eq!(drive::selected_key(&state), selected);
    let effects = state.update(ModelsInput::OptionsLoaded {
        session_id: drive::SID.into(),
        result: catalogue(false),
    });
    no_mutation(&effects);
    assert_eq!(state.view().unwrap().filter, filter);
    assert_eq!(drive::selected_key(&state), selected);
}

#[test]
fn first_new_model_is_selected_only_after_readback_without_switching() {
    let mut state = at_stub(ListScope::All);
    let mut all = drive::press(&mut state, PickerKey::Enter);
    all.extend(state.update(ModelsInput::ResumePicker {
        provider: "example-setup".into(),
    }));
    assert!(
        state.view().is_some(),
        "resume must restore the picker before the catalogue reply"
    );
    all.extend(state.update(ModelsInput::OptionsLoaded {
        session_id: drive::SID.into(),
        result: catalogue(true),
    }));
    no_mutation(&all);
    let selected = drive::selected_key(&state).unwrap();
    assert!(
        matches!(selected,RowKey::Model{provider,model}|RowKey::Favourite{provider,model} if provider=="example-setup"&&model=="first-new-model")
    );
    let view = state.view().unwrap();
    assert_eq!(view.filter, "Example setup");
    assert_eq!(view.browse.unwrap().scope, ListScope::All);
    assert!(
        view.effort.contains("xhigh"),
        "the requested dial survives setup"
    );
}

#[test]
fn resume_with_no_open_session_has_the_existing_notice() {
    let mut state = ModelsState::new("fixture", Ok(SavedModels::empty()));
    let effects = state.update(ModelsInput::ResumePicker {
        provider: "example-setup".into(),
    });
    no_mutation(&effects);
    assert_eq!(drive::notices(&effects), ["No session is open yet."]);
    assert!(state.view().is_none());
}
