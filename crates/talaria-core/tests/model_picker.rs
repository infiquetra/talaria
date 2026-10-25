//! Issue 230 picker scenarios. Each test reaches `ModelsState::update` first.

#[path = "support/catalogue.rs"]
mod catalogue;
#[path = "support/model_drive.rs"]
mod drive;

use drive::{
    PROFILE, SID, has_close, move_down_to, notices, one_mutate, open_loaded, press, saved_files,
    selected_key, type_filter, view_parts,
};
use hermes_contract::schemas::ModelOptionsResult;
use serde_json::json;
use talaria_core::models::{
    Effort, EffortCommand, ModelCommand, ModelCommandOrEffort, ModelsEffect, ModelsInput,
    ModelsState, PickerKey, RowKey, SavedDefault, SavedModels,
};
use talaria_core::mutation::MutationOutcome;

fn picker_catalog() -> ModelOptionsResult {
    catalogue::parse(json!({
        "model": "minimax-m3.1-flash",
        "provider": "ollama-cloud",
        "providers": [
            {
                "slug": "ollama-cloud",
                "name": "Ollama Cloud",
                "models": ["minimax-m3.1-flash", "glm-5", "gone"],
                "total_models": 3,
                "is_current": true,
                "authenticated": true,
                "capabilities": {
                    "minimax-m3.1-flash": {"fast": false, "reasoning": true},
                    "glm-5": {"fast": true, "reasoning": true},
                    "gone": {"fast": false, "reasoning": true}
                },
                "unavailable_models": ["gone"]
            },
            {
                "slug": "anthropic",
                "name": "Anthropic",
                "models": [],
                "authenticated": false,
                "key_env": "ANTHROPIC_API_KEY"
            }
        ]
    }))
}

fn model_key(model: &str) -> RowKey {
    RowKey::Model {
        provider: "ollama-cloud".to_owned(),
        model: model.to_owned(),
    }
}

fn favourite_key(model: &str) -> RowKey {
    RowKey::Favourite {
        provider: "ollama-cloud".to_owned(),
        model: model.to_owned(),
    }
}

fn with_favourites() -> SavedModels {
    let mut saved = SavedModels::empty();
    saved.favourites = vec![
        "ollama-cloud::minimax-m3.1-flash".to_owned(),
        "ollama-cloud::missing-model".to_owned(),
    ];
    saved
}

fn current_model() -> RowKey {
    model_key("minimax-m3.1-flash")
}

#[test]
fn filter_terms_and_selection() {
    let mut state = open_loaded(picker_catalog(), Some("high"), with_favourites());
    assert_eq!(selected_key(&state), Some(current_model()));
    type_filter(&mut state, "oll glm");
    assert_eq!(selected_key(&state), Some(model_key("glm-5")));
    let (title, _effort, filter, banner) = view_parts(&state);
    assert_eq!(title, "Switch model · this session");
    assert_eq!(filter, "oll glm");
    assert!(banner.is_none());
    press(&mut state, PickerKey::ClearFilter);
    type_filter(&mut state, "zzz");
    let (_title, _effort, filter, banner) = view_parts(&state);
    assert_eq!(filter, "zzz");
    assert_eq!(banner.as_deref(), Some("No model matches \"zzz\"."));
}

#[test]
fn keys_navigate_and_skip() {
    let mut state = open_loaded(picker_catalog(), Some("high"), with_favourites());
    state.set_list_height(3);
    assert_eq!(selected_key(&state), Some(current_model()));
    press(&mut state, PickerKey::Down);
    assert_eq!(selected_key(&state), Some(model_key("glm-5")));
    press(&mut state, PickerKey::Down);
    assert_eq!(selected_key(&state), Some(model_key("gone")));
    press(&mut state, PickerKey::Down);
    assert_eq!(
        selected_key(&state),
        Some(RowKey::NotSetUp {
            provider: "anthropic".to_owned()
        })
    );
    press(&mut state, PickerKey::Down);
    assert_eq!(
        selected_key(&state),
        Some(RowKey::NotSetUp {
            provider: "anthropic".to_owned()
        })
    );
    press(&mut state, PickerKey::Home);
    assert_eq!(
        selected_key(&state),
        Some(favourite_key("minimax-m3.1-flash"))
    );
    press(&mut state, PickerKey::Down);
    assert_eq!(selected_key(&state), Some(favourite_key("missing-model")));
    press(&mut state, PickerKey::End);
    assert_eq!(
        selected_key(&state),
        Some(RowKey::NotSetUp {
            provider: "anthropic".to_owned()
        })
    );
    press(&mut state, PickerKey::Home);
    press(&mut state, PickerKey::PageDown);
    assert_eq!(selected_key(&state), Some(model_key("glm-5")));
    press(&mut state, PickerKey::PageUp);
    assert_eq!(
        selected_key(&state),
        Some(favourite_key("minimax-m3.1-flash"))
    );
}

#[test]
fn effort_control() {
    let mut state = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    let (_title, effort, _filter, _banner) = view_parts(&state);
    assert_eq!(effort, "effort: high");
    press(&mut state, PickerKey::Right);
    press(&mut state, PickerKey::Right);
    let (_title, effort, _filter, _banner) = view_parts(&state);
    assert_eq!(effort, "effort: high → max");
    press(&mut state, PickerKey::Right);
    press(&mut state, PickerKey::Right);
    let (_title, effort, _filter, _banner) = view_parts(&state);
    assert_eq!(effort, "effort: high → ultra");

    let mut hidden = open_loaded(
        catalogue::reasoning_false_model(),
        Some("high"),
        SavedModels::empty(),
    );
    press(&mut hidden, PickerKey::Right);
    press(&mut hidden, PickerKey::Right);
    move_down_to(&mut hidden, &model_key("no-dial"));
    let (_title, effort, _filter, _banner) = view_parts(&hidden);
    assert_eq!(effort, "effort: not offered by this model");
    press(&mut hidden, PickerKey::Right);
    let (_title, effort, _filter, _banner) = view_parts(&hidden);
    assert_eq!(effort, "effort: not offered by this model");
    press(&mut hidden, PickerKey::Up);
    let (_title, effort, _filter, _banner) = view_parts(&hidden);
    assert_eq!(effort, "effort: high → max");

    let mut unread = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    let _ = unread.update(ModelsInput::EffortReadFailed {
        session_id: SID.to_owned(),
        reason: "timed out".to_owned(),
        unknown_method: false,
    });
    let (_title, effort, _filter, _banner) = view_parts(&unread);
    assert_eq!(effort, "effort: could not be read");
    move_down_to(&mut unread, &model_key("glm-5"));
    let effects = press(&mut unread, PickerKey::Enter);
    let request = one_mutate(&effects);
    assert_eq!(request.operation, "model");
    assert!(request.payload.get("reasoning").is_none());

    let mut again = open_loaded(picker_catalog(), None, SavedModels::empty());
    let (_title, effort, _filter, _banner) = view_parts(&again);
    assert_eq!(effort, "effort: medium");
    press(&mut again, PickerKey::ToggleScope);
    let (title, _effort, _filter, _banner) = view_parts(&again);
    assert_eq!(
        title,
        "Switch model · this session and new sessions on fixture"
    );
    press(&mut again, PickerKey::Close);
    let _ = again.update(ModelsInput::OpenPicker {
        session_id: Some(SID.to_owned()),
        profile: PROFILE.to_owned(),
    });
    let (title, _effort, _filter, _banner) = view_parts(&again);
    assert_eq!(title, "Switch model · this session");
}

#[test]
fn enter_builds_the_right_request() {
    let mut state = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    let effects = press(&mut state, PickerKey::Enter);
    assert!(mutates_empty(&effects));
    assert!(has_close(&effects));
    assert_eq!(
        notices(&effects),
        vec!["Already using minimax-m3.1-flash at effort high; nothing changed.".to_owned()]
    );

    let mut state = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    press(&mut state, PickerKey::Right);
    let request = one_mutate(&press(&mut state, PickerKey::Enter));
    assert_eq!(request.operation, "reasoning");
    assert_eq!(request.payload, drive::payload(json!({"effort": "xhigh"})));
    assert_eq!(request.request_id, "L-m1");
    assert_eq!(request.expected_revision, 12);
    assert_eq!(request.expected_generation, Some(4));

    let mut state = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    move_down_to(&mut state, &model_key("glm-5"));
    let request = one_mutate(&press(&mut state, PickerKey::Enter));
    assert_eq!(request.operation, "model");
    assert_eq!(
        request.payload,
        drive::payload(json!({"model": "glm-5", "provider": "ollama-cloud"}))
    );

    let mut state = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    press(&mut state, PickerKey::Right);
    move_down_to(&mut state, &model_key("glm-5"));
    let request = one_mutate(&press(&mut state, PickerKey::Enter));
    assert_eq!(
        request.payload,
        drive::payload(json!({
            "model": "glm-5",
            "provider": "ollama-cloud",
            "reasoning": "xhigh"
        }))
    );

    let mut quiet = open_loaded(
        catalogue::reasoning_false_model(),
        Some("high"),
        SavedModels::empty(),
    );
    press(&mut quiet, PickerKey::Right);
    move_down_to(&mut quiet, &model_key("no-dial"));
    let request = one_mutate(&press(&mut quiet, PickerKey::Enter));
    assert!(request.payload.get("reasoning").is_none());
    assert_eq!(
        request
            .payload
            .get("model")
            .and_then(|value| value.as_str()),
        Some("no-dial")
    );

    let mut deferred = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    move_down_to(&mut deferred, &model_key("glm-5"));
    let request = one_mutate(&press(&mut deferred, PickerKey::Enter));
    let _ = deferred.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Deferred(json!({
            "session_id": SID,
            "operation": "model",
            "status": "deferred",
            "model": "glm-5"
        })),
    });
    let _ = deferred.update(ModelsInput::OpenPicker {
        session_id: Some(SID.to_owned()),
        profile: PROFILE.to_owned(),
    });
    let request = one_mutate(&press(&mut deferred, PickerKey::Enter));
    assert_eq!(request.operation, "model");
    assert_eq!(
        request
            .payload
            .get("model")
            .and_then(|value| value.as_str()),
        Some("minimax-m3.1-flash")
    );
}

fn mutates_empty(effects: &[ModelsEffect]) -> bool {
    drive::mutates(effects).is_empty()
}

#[test]
fn not_set_up_and_unavailable_rows() {
    let mut state = open_loaded(picker_catalog(), Some("high"), with_favourites());
    move_down_to(&mut state, &model_key("gone"));
    let effects = press(&mut state, PickerKey::Enter);
    assert!(drive::mutates(&effects).is_empty());
    assert!(!has_close(&effects));
    assert_eq!(
        notices(&effects),
        vec!["gone is not available on your Ollama Cloud plan.".to_owned()]
    );
    assert!(state.view().is_some());

    move_down_to(
        &mut state,
        &RowKey::NotSetUp {
            provider: "anthropic".to_owned(),
        },
    );
    let effects = press(&mut state, PickerKey::Enter);
    assert!(drive::mutates(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec!["Anthropic is not set up. Set it up with hermes model in a terminal, then press ctrl+r here.".to_owned()]
    );

    press(&mut state, PickerKey::Home);
    press(&mut state, PickerKey::Down);
    let effects = press(&mut state, PickerKey::Enter);
    assert_eq!(
        notices(&effects),
        vec!["missing-model is no longer listed by Ollama Cloud. Press ctrl+f to remove it from favourites.".to_owned()]
    );
    assert!(drive::mutates(&effects).is_empty());
}

#[test]
fn escape_and_close() {
    let mut state = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    type_filter(&mut state, "glm");
    let effects = press(&mut state, PickerKey::Escape);
    assert!(!has_close(&effects));
    let (_title, _effort, filter, _banner) = view_parts(&state);
    assert_eq!(filter, "");
    let effects = press(&mut state, PickerKey::Escape);
    assert_eq!(effects, vec![ModelsEffect::CloseOverlay]);
    let mut state = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    type_filter(&mut state, "glm");
    let effects = press(&mut state, PickerKey::Close);
    assert_eq!(effects, vec![ModelsEffect::CloseOverlay]);
}

#[test]
fn scope_toggle_and_save() {
    let mut state = open_loaded(
        picker_catalog(),
        Some("high"),
        saved_effort(Some(Effort::Low)),
    );
    press(&mut state, PickerKey::ToggleScope);
    let (title, _effort, _filter, _banner) = view_parts(&state);
    assert_eq!(
        title,
        "Switch model · this session and new sessions on fixture"
    );
    move_down_to(&mut state, &model_key("glm-5"));
    let request = one_mutate(&press(&mut state, PickerKey::Enter));
    let effects = state.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Committed(json!({
            "session_id": SID,
            "operation": "model",
            "model": "glm-5",
            "provider": "ollama-cloud",
            "revision": 13,
            "execution_generation": 5
        })),
    });
    let saved = only_save(&effects);
    let default = saved.defaults.get(PROFILE).expect("profile default");
    assert_eq!(default.model.as_deref(), Some("glm-5"));
    assert_eq!(default.provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(default.effort, Some(Effort::Low));

    let mut moved = open_loaded(
        picker_catalog(),
        Some("high"),
        saved_effort(Some(Effort::Low)),
    );
    press(&mut moved, PickerKey::ToggleScope);
    press(&mut moved, PickerKey::Right);
    move_down_to(&mut moved, &model_key("glm-5"));
    let request = one_mutate(&press(&mut moved, PickerKey::Enter));
    let effects = moved.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Committed(json!({
            "operation": "model",
            "model": "glm-5",
            "provider": "ollama-cloud",
            "effort": "xhigh",
            "revision": 13,
            "execution_generation": 5
        })),
    });
    assert_eq!(
        only_save(&effects).defaults[PROFILE].effort,
        Some(Effort::Xhigh)
    );

    let mut deferred = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    press(&mut deferred, PickerKey::ToggleScope);
    move_down_to(&mut deferred, &model_key("glm-5"));
    let request = one_mutate(&press(&mut deferred, PickerKey::Enter));
    let effects = deferred.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Deferred(json!({"status": "deferred", "model": "glm-5"})),
    });
    let default = &only_save(&effects).defaults[PROFILE];
    assert_eq!(default.model.as_deref(), Some("glm-5"));
    assert_eq!(default.provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(default.effort, None);

    let mut noop = open_loaded(picker_catalog(), None, SavedModels::empty());
    press(&mut noop, PickerKey::ToggleScope);
    let effects = press(&mut noop, PickerKey::Enter);
    let default = &only_save(&effects).defaults[PROFILE];
    assert_eq!(default.model.as_deref(), Some("minimax-m3.1-flash"));
    assert_eq!(default.provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(default.effort, None);
    assert_eq!(
        notices(&effects),
        vec!["Saved for new Talaria sessions on profile fixture: minimax-m3.1-flash on Ollama Cloud.".to_owned()]
    );

    let mut effort_only = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    press(&mut effort_only, PickerKey::ToggleScope);
    press(&mut effort_only, PickerKey::Right);
    let request = one_mutate(&press(&mut effort_only, PickerKey::Enter));
    let effects = effort_only.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Committed(json!({
            "operation": "reasoning",
            "effort": "xhigh",
            "revision": 13,
            "execution_generation": 5
        })),
    });
    let default = &only_save(&effects).defaults[PROFILE];
    assert_eq!(default.model.as_deref(), Some("minimax-m3.1-flash"));
    assert_eq!(default.provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(default.effort, Some(Effort::Xhigh));

    let mut waiting = ModelsState::new("L", Ok(SavedModels::empty()));
    let _ = waiting.update(ModelsInput::SessionOpened {
        session_id: SID.to_owned(),
        revision: Some(12),
        execution_generation: Some(4),
    });
    let effects = waiting.update(ModelsInput::Command {
        session_id: Some(SID.to_owned()),
        profile: PROFILE.to_owned(),
        command: Ok(ModelCommandOrEffort::Model(ModelCommand::Switch {
            model: "glm-5".to_owned(),
            provider: None,
            effort: None,
            save_default: true,
        })),
    });
    assert!(saved_files(&effects).is_empty());
    let request = one_mutate(&effects);
    let effects = waiting.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Deferred(json!({"status": "deferred", "model": "glm-5"})),
    });
    assert!(saved_files(&effects).is_empty());
    let effects = waiting.update(ModelsInput::SessionEvent {
        session_id: SID.to_owned(),
        type_name: "session.updated".to_owned(),
        payload: json!({
            "session_id": SID,
            "operation": "model",
            "model": "glm-5",
            "provider": "ollama-cloud",
            "revision": 13,
            "execution_generation": 5
        }),
    });
    let default = &only_save(&effects).defaults[PROFILE];
    assert_eq!(default.model.as_deref(), Some("glm-5"));
    assert_eq!(default.provider.as_deref(), Some("ollama-cloud"));

    let mut refused = ModelsState::new("L", Ok(SavedModels::empty()));
    let _ = refused.update(ModelsInput::SessionOpened {
        session_id: SID.to_owned(),
        revision: Some(12),
        execution_generation: Some(4),
    });
    let effects = refused.update(ModelsInput::Command {
        session_id: Some(SID.to_owned()),
        profile: PROFILE.to_owned(),
        command: Ok(ModelCommandOrEffort::Model(ModelCommand::Switch {
            model: "glm-5".to_owned(),
            provider: None,
            effort: None,
            save_default: true,
        })),
    });
    let request = one_mutate(&effects);
    let _ = refused.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Deferred(json!({"status": "deferred", "model": "glm-5"})),
    });
    let effects = refused.update(ModelsInput::SessionEvent {
        session_id: SID.to_owned(),
        type_name: "session.updated".to_owned(),
        payload: json!({
            "status": "refused",
            "operation": "model",
            "reason": "model_resolution_failed",
            "target_model": "glm-5",
            "model": "minimax-m3.1-flash"
        }),
    });
    assert!(saved_files(&effects).is_empty());

    let mut effort_default = ModelsState::new("L", Ok(SavedModels::empty()));
    let _ = effort_default.update(ModelsInput::SessionOpened {
        session_id: SID.to_owned(),
        revision: Some(12),
        execution_generation: Some(4),
    });
    let effects = effort_default.update(ModelsInput::Command {
        session_id: Some(SID.to_owned()),
        profile: PROFILE.to_owned(),
        command: Ok(ModelCommandOrEffort::Effort(EffortCommand::Set {
            level: Some(Effort::High),
            save_default: true,
        })),
    });
    let request = one_mutate(&effects);
    let effects = effort_default.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Committed(json!({
            "operation": "reasoning",
            "effort": "high",
            "revision": 13,
            "execution_generation": 5
        })),
    });
    let default = &only_save(&effects).defaults[PROFILE];
    assert_eq!(default.effort, Some(Effort::High));
    assert_eq!(default.model, None);
    assert_eq!(default.provider, None);

    let mut busy = ModelsState::new("L", Ok(SavedModels::empty()));
    let _ = busy.update(ModelsInput::SessionOpened {
        session_id: SID.to_owned(),
        revision: Some(12),
        execution_generation: Some(4),
    });
    let effects = busy.update(ModelsInput::Command {
        session_id: Some(SID.to_owned()),
        profile: PROFILE.to_owned(),
        command: Ok(ModelCommandOrEffort::Effort(EffortCommand::Set {
            level: Some(Effort::High),
            save_default: true,
        })),
    });
    let request = one_mutate(&effects);
    let effects = busy.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Refused {
            reason: "session_busy".to_owned(),
            field: None,
            detail: None,
            rpc_code: 4001,
        },
    });
    assert!(saved_files(&effects).is_empty());
    let effects = busy.update(ModelsInput::EffortSetReturned {
        session_id: SID.to_owned(),
        level: Some(Effort::High),
        outcome: Ok(()),
    });
    assert_eq!(
        only_save(&effects).defaults[PROFILE].effort,
        Some(Effort::High)
    );

    let mut cleared = ModelsState::new("L", Ok(saved_effort(Some(Effort::High))));
    let _ = cleared.update(ModelsInput::SessionOpened {
        session_id: SID.to_owned(),
        revision: Some(12),
        execution_generation: Some(4),
    });
    let effects = cleared.update(ModelsInput::Command {
        session_id: Some(SID.to_owned()),
        profile: PROFILE.to_owned(),
        command: Ok(ModelCommandOrEffort::Effort(EffortCommand::Set {
            level: None,
            save_default: true,
        })),
    });
    let request = one_mutate(&effects);
    let effects = cleared.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Committed(json!({
            "operation": "reasoning",
            "effort": null,
            "revision": 13,
            "execution_generation": 5
        })),
    });
    assert_eq!(only_save(&effects).defaults[PROFILE].effort, None);
}

fn saved_effort(effort: Option<Effort>) -> SavedModels {
    let mut saved = SavedModels::empty();
    saved.defaults.insert(
        PROFILE.to_owned(),
        SavedDefault {
            model: None,
            provider: None,
            effort,
        },
    );
    saved
}

fn only_save(effects: &[ModelsEffect]) -> SavedModels {
    let found = saved_files(effects);
    assert_eq!(found.len(), 1, "expected one save");
    found.into_iter().next().expect("one save")
}

#[test]
fn favourite_toggle() {
    let mut state = open_loaded(picker_catalog(), Some("high"), SavedModels::empty());
    move_down_to(&mut state, &model_key("glm-5"));
    let effects = press(&mut state, PickerKey::ToggleFavourite);
    assert_eq!(
        only_save(&effects).favourites,
        vec!["ollama-cloud::glm-5".to_owned()]
    );
    let effects = press(&mut state, PickerKey::ToggleFavourite);
    assert!(only_save(&effects).favourites.is_empty());

    move_down_to(&mut state, &model_key("gone"));
    let effects = press(&mut state, PickerKey::ToggleFavourite);
    assert_eq!(
        only_save(&effects).favourites,
        vec!["ollama-cloud::gone".to_owned()]
    );

    move_down_to(
        &mut state,
        &RowKey::NotSetUp {
            provider: "anthropic".to_owned(),
        },
    );
    let effects = press(&mut state, PickerKey::ToggleFavourite);
    assert!(saved_files(&effects).is_empty());

    let mut stale = open_loaded(picker_catalog(), Some("high"), with_favourites());
    press(&mut stale, PickerKey::Home);
    press(&mut stale, PickerKey::Down);
    assert_eq!(selected_key(&stale), Some(favourite_key("missing-model")));
    let effects = press(&mut stale, PickerKey::Enter);
    assert_eq!(
        notices(&effects),
        vec!["missing-model is no longer listed by Ollama Cloud. Press ctrl+f to remove it from favourites.".to_owned()]
    );
    let effects = press(&mut stale, PickerKey::ToggleFavourite);
    let favourites = only_save(&effects).favourites;
    assert_eq!(
        favourites,
        vec!["ollama-cloud::minimax-m3.1-flash".to_owned()]
    );
}
