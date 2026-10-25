//! Issue 230 pick-state scenarios. Each test reaches `ModelsState::update` first.

#[path = "support/catalogue.rs"]
mod catalogue;
#[path = "support/model_drive.rs"]
mod drive;

use drive::{PROFILE, SID, confirmation, mutates, notices, one_mutate, open_loaded, press};
use serde_json::json;
use talaria_core::models::{
    ConfirmKey, Effort, EffortCommand, ModelCommand, ModelCommandOrEffort, ModelsEffect,
    ModelsInput, ModelsState, NoticeLevel, PickerKey, SavedModels,
};
use talaria_core::mutation::MutationOutcome;

fn session() -> ModelsState {
    let mut state = ModelsState::new("L", Ok(SavedModels::empty()));
    let _ = state.update(ModelsInput::SessionOpened {
        session_id: SID.to_owned(),
        revision: Some(12),
        execution_generation: Some(4),
    });
    let _ = state.update(ModelsInput::ForegroundChanged {
        session_id: Some(SID.to_owned()),
    });
    let _ = state.update(ModelsInput::OptionsLoaded {
        session_id: SID.to_owned(),
        result: catalogue::section_five_plus_absent(),
    });
    state
}

fn command(state: &mut ModelsState, command: ModelCommandOrEffort) -> Vec<ModelsEffect> {
    state.update(ModelsInput::Command {
        session_id: Some(SID.to_owned()),
        profile: PROFILE.to_owned(),
        command: Ok(command),
    })
}

fn switch(model: &str, effort: Option<Effort>) -> ModelCommandOrEffort {
    ModelCommandOrEffort::Model(ModelCommand::Switch {
        model: model.to_owned(),
        provider: Some("ollama-cloud".to_owned()),
        effort,
        save_default: false,
    })
}

fn event(state: &mut ModelsState, type_name: &str, body: serde_json::Value) -> Vec<ModelsEffect> {
    state.update(ModelsInput::SessionEvent {
        session_id: SID.to_owned(),
        type_name: type_name.to_owned(),
        payload: body,
    })
}

fn notice_text(effects: &[ModelsEffect]) -> Vec<String> {
    notices(effects)
}

#[test]
fn committed_and_deferred_notices() {
    let mut state = session();
    let effects = command(&mut state, switch("glm-5", Some(Effort::Xhigh)));
    let request = one_mutate(&effects);
    let effects = state.update(ModelsInput::MutationReturned {
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
        notice_text(&effects),
        vec!["Model: glm-5 on Ollama Cloud. Effort: xhigh.".to_owned()]
    );

    let mut plain = session();
    let request = one_mutate(&command(&mut plain, switch("glm-5", None)));
    let effects = plain.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Committed(json!({
            "operation": "model",
            "model": "glm-5",
            "provider": "ollama-cloud",
            "revision": 13,
            "execution_generation": 5
        })),
    });
    assert_eq!(
        notice_text(&effects),
        vec!["Model: glm-5 on Ollama Cloud.".to_owned()]
    );

    let mut deferred = session();
    let request = one_mutate(&command(&mut deferred, switch("glm-5", None)));
    let effects = deferred.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Deferred(json!({
            "operation": "model",
            "status": "deferred",
            "model": "glm-5",
            "revision": 12,
            "execution_generation": 4
        })),
    });
    assert_eq!(
        notice_text(&effects),
        vec!["Model will change to glm-5 when the current turn ends.".to_owned()]
    );
    assert!(deferred.is_deferred(SID));
    let effects = event(
        &mut deferred,
        "session.info",
        json!({"model": "minimax-m3.1-flash", "provider": "ollama-cloud", "revision": 12}),
    );
    assert!(notice_text(&effects).is_empty());
    assert!(deferred.is_deferred(SID));

    let mut picker = open_loaded(
        catalogue::section_five_plus_absent(),
        Some("high"),
        SavedModels::empty(),
    );
    let request = one_mutate(&{
        let _ = drive::press(&mut picker, PickerKey::Down);
        drive::press(&mut picker, PickerKey::Enter)
    });
    let _ = picker.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Deferred(json!({"status": "deferred", "model": "glm-5"})),
    });
    let _ = event(
        &mut picker,
        "session.info",
        json!({"model": "minimax-m3.1-flash", "provider": "ollama-cloud"}),
    );
    assert!(picker.is_deferred(SID));
    let _ = picker.update(ModelsInput::OpenPicker {
        session_id: Some(SID.to_owned()),
        profile: PROFILE.to_owned(),
    });
    let request = one_mutate(&press(&mut picker, PickerKey::Enter));
    assert_eq!(request.operation, "model");
    assert_eq!(
        request
            .payload
            .get("model")
            .and_then(|value| value.as_str()),
        Some("minimax-m3.1-flash")
    );

    let mut renamed = session();
    let request = one_mutate(&command(&mut renamed, switch("glm-5", None)));
    let _ = renamed.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Deferred(json!({"status": "deferred", "model": "glm-5"})),
    });
    let _ = event(
        &mut renamed,
        "session.info",
        json!({"model": "GLM-5", "provider": "ollama-cloud"}),
    );
    assert!(!renamed.is_deferred(SID));

    let mut receipt = session();
    let request = one_mutate(&command(&mut receipt, switch("glm-5", None)));
    let _ = receipt.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Deferred(json!({"status": "deferred", "model": "glm-5"})),
    });
    let _ = event(
        &mut receipt,
        "session.updated",
        json!({
            "operation": "model",
            "model": "minimax-m3.1-flash",
            "provider": "ollama-cloud",
            "revision": 13,
            "execution_generation": 5
        }),
    );
    assert!(!receipt.is_deferred(SID));
}

#[test]
fn confirmation_flow() {
    let mut state = session();
    let request = one_mutate(&command(&mut state, switch("glm-5", Some(Effort::Xhigh))));
    let _ = state.update(ModelsInput::OpenPicker {
        session_id: Some(SID.to_owned()),
        profile: PROFILE.to_owned(),
    });
    let effects = state.update(ModelsInput::MutationReturned {
        request: request.clone(),
        outcome: MutationOutcome::NeedsConfirmation(confirmation(
            "tok-1",
            "This model costs more.",
        )),
    });
    assert!(
        state.confirmation().is_none(),
        "the card waits until the overlay closes"
    );
    assert!(notice_text(&effects).is_empty());
    let _ = state.update(ModelsInput::Picker(PickerKey::Close));
    let card = state.confirmation().expect("the card is waiting").clone();
    assert_eq!(card.token, "tok-1");
    assert_eq!(card.message, "This model costs more.");

    let effects = state.update(ModelsInput::Confirm(ConfirmKey::Accept));
    assert!(state.confirmation().is_none());
    let resent = one_mutate(&effects);
    assert_ne!(resent.request_id, request.request_id);
    assert_eq!(
        resent
            .payload
            .get("reasoning")
            .and_then(|value| value.as_str()),
        Some("xhigh")
    );
    assert_eq!(
        resent
            .payload
            .get("confirm")
            .and_then(|value| value.as_str()),
        Some("tok-1")
    );
    assert_eq!(
        resent.payload.get("model").and_then(|value| value.as_str()),
        Some("glm-5")
    );

    let effects = state.update(ModelsInput::MutationReturned {
        request: resent,
        outcome: MutationOutcome::NeedsConfirmation(confirmation(
            "tok-1",
            "This model costs more.",
        )),
    });
    assert!(state.confirmation().is_none());
    assert_eq!(
        notice_text(&effects),
        vec!["Hermes still asks for confirmation, so the model was not changed: This model costs more.".to_owned()]
    );

    let mut decline = session();
    let request = one_mutate(&command(&mut decline, switch("glm-5", Some(Effort::Xhigh))));
    let _ = decline.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::NeedsConfirmation(confirmation(
            "tok-1",
            "This model costs more.",
        )),
    });
    let effects = decline.update(ModelsInput::Confirm(ConfirmKey::Decline));
    assert!(mutates(&effects).is_empty());
    assert_eq!(
        notice_text(&effects),
        vec!["Kept minimax-m3.1-flash.".to_owned()]
    );

    let mut background = session();
    let _ = background.update(ModelsInput::ForegroundChanged { session_id: None });
    let request = one_mutate(&command(
        &mut background,
        switch("glm-5", Some(Effort::Xhigh)),
    ));
    let _ = background.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::NeedsConfirmation(confirmation(
            "tok-a",
            "This model costs more.",
        )),
    });
    assert!(background.confirmation().is_none());
    let _ = background.update(ModelsInput::ForegroundChanged {
        session_id: Some(SID.to_owned()),
    });
    assert_eq!(
        background.confirmation().map(|card| card.token.as_str()),
        Some("tok-a")
    );

    let _ = background.update(ModelsInput::SessionEvent {
        session_id: SID.to_owned(),
        type_name: "session.updated".to_owned(),
        payload: json!({
            "status": "confirmation_required",
            "confirm": "tok-b",
            "confirm_message": "A newer ask.",
            "target_model": "glm-5",
            "target_provider": "ollama-cloud",
            "payload": {"model": "glm-5", "provider": "ollama-cloud", "reasoning": "xhigh"}
        }),
    });
    assert_eq!(
        background.confirmation().map(|card| card.token.as_str()),
        Some("tok-b")
    );

    let effects = command(&mut background, switch("minimax-m3", None));
    assert!(background.confirmation().is_none());
    assert!(notice_text(&effects).is_empty());
    assert_eq!(one_mutate(&effects).operation, "model");

    let mut idle = session();
    let effects = idle.update(ModelsInput::Confirm(ConfirmKey::Accept));
    assert!(effects.is_empty());
}

#[test]
fn deferred_outcome_events() {
    let mut plain = session();
    let effects = event(
        &mut plain,
        "session.updated",
        json!({
            "operation": "model",
            "status": "refused",
            "reason": "model_resolution_failed",
            "target_model": "glm-5",
            "model": "minimax-m3.1-flash",
            "provider": "ollama-cloud"
        }),
    );
    assert_eq!(
        notice_text(&effects),
        vec!["Hermes did not switch to glm-5 when the turn ended (model_resolution_failed). Still using minimax-m3.1-flash.".to_owned()]
    );
    assert_eq!(drive::levels(&effects), vec![NoticeLevel::Error]);

    let mut held = session();
    let request = one_mutate(&command(&mut held, switch("glm-5", None)));
    let _ = held.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Deferred(json!({"status": "deferred", "model": "glm-5"})),
    });
    assert!(held.is_deferred(SID));
    let effects = event(
        &mut held,
        "session.updated",
        json!({
            "operation": "model",
            "status": "refused",
            "reason": "model_resolution_failed",
            "target_model": "glm-5",
            "model": "minimax-m3.1-flash"
        }),
    );
    assert!(!held.is_deferred(SID));
    assert_eq!(
        notice_text(&effects),
        vec!["Hermes did not switch to glm-5 when the turn ended (model_resolution_failed). Still using minimax-m3.1-flash.".to_owned()]
    );

    let mut card = session();
    let _ = event(
        &mut card,
        "session.updated",
        json!({
            "operation": "model",
            "status": "confirmation_required",
            "confirm": "tok-1",
            "confirm_message": "This model costs more.",
            "target_model": "glm-5",
            "target_provider": "ollama-cloud",
            "committed_model": "minimax-m3.1-flash",
            "committed_provider": "ollama-cloud",
            "payload": {"model": "glm-5", "provider": "ollama-cloud"}
        }),
    );
    assert_eq!(
        card.confirmation().map(|item| item.token.as_str()),
        Some("tok-1")
    );
    assert_eq!(card.confirm_keep(), Some("minimax-m3.1-flash"));
    let effects = card.update(ModelsInput::Confirm(ConfirmKey::Decline));
    assert_eq!(
        notice_text(&effects),
        vec!["Kept minimax-m3.1-flash. Hermes keeps the switch pending and will ask again the next time Talaria attaches to this session.".to_owned()]
    );
}

#[test]
fn effort_while_busy_goes_to_config_set() {
    let mut state = session();
    let request = one_mutate(&command(
        &mut state,
        ModelCommandOrEffort::Effort(EffortCommand::Set {
            level: Some(Effort::High),
            save_default: false,
        }),
    ));
    assert_eq!(request.operation, "reasoning");
    let effects = state.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Refused {
            reason: "session_busy".to_owned(),
            field: None,
            detail: None,
            rpc_code: 4001,
        },
    });
    assert!(notice_text(&effects).is_empty());
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, ModelsEffect::SetEffort { .. }))
            .count(),
        1
    );
    assert!(matches!(
        effects
            .iter()
            .find(|effect| matches!(effect, ModelsEffect::SetEffort { .. })),
        Some(ModelsEffect::SetEffort {
            level: Some(Effort::High),
            ..
        })
    ));
    let effects = state.update(ModelsInput::EffortSetReturned {
        session_id: SID.to_owned(),
        level: Some(Effort::High),
        outcome: Ok(()),
    });
    assert_eq!(
        notice_text(&effects),
        vec!["Effort will change to high when the current turn ends.".to_owned()]
    );
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, ModelsEffect::SetEffort { .. }))
    );

    let mut profile = session();
    let request = one_mutate(&command(
        &mut profile,
        ModelCommandOrEffort::Effort(EffortCommand::Set {
            level: None,
            save_default: false,
        }),
    ));
    let effects = profile.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Refused {
            reason: "session_busy".to_owned(),
            field: None,
            detail: None,
            rpc_code: 4001,
        },
    });
    assert!(matches!(
        effects
            .iter()
            .find(|effect| matches!(effect, ModelsEffect::SetEffort { .. })),
        Some(ModelsEffect::SetEffort { level: None, .. })
    ));
    let effects = profile.update(ModelsInput::EffortSetReturned {
        session_id: SID.to_owned(),
        level: None,
        outcome: Ok(()),
    });
    assert_eq!(
        notice_text(&effects),
        vec!["Effort will go back to the profile's level when the current turn ends.".to_owned()]
    );

    let mut denied = session();
    let request = one_mutate(&command(
        &mut denied,
        ModelCommandOrEffort::Effort(EffortCommand::Set {
            level: Some(Effort::Low),
            save_default: false,
        }),
    ));
    let _ = denied.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Refused {
            reason: "session_busy".to_owned(),
            field: None,
            detail: None,
            rpc_code: 4001,
        },
    });
    let effects = denied.update(ModelsInput::EffortSetReturned {
        session_id: SID.to_owned(),
        level: Some(Effort::Low),
        outcome: Err(MutationOutcome::Refused {
            reason: "permission_denied".to_owned(),
            field: None,
            detail: None,
            rpc_code: 4001,
        }),
    });
    assert_eq!(
        notice_text(&effects),
        vec![
            "Hermes did not allow this window to change the session (permission_denied)."
                .to_owned()
        ]
    );

    for outcome in [MutationOutcome::Unknown, MutationOutcome::Unreadable] {
        let mut lost = session();
        let request = one_mutate(&command(
            &mut lost,
            ModelCommandOrEffort::Effort(EffortCommand::Set {
                level: Some(Effort::Low),
                save_default: false,
            }),
        ));
        let _ = lost.update(ModelsInput::MutationReturned {
            request,
            outcome: MutationOutcome::Refused {
                reason: "session_busy".to_owned(),
                field: None,
                detail: None,
                rpc_code: 4001,
            },
        });
        let effects = lost.update(ModelsInput::EffortSetReturned {
            session_id: SID.to_owned(),
            level: Some(Effort::Low),
            outcome: Err(outcome),
        });
        assert_eq!(
            notice_text(&effects),
            vec!["The connection dropped before Hermes answered, so the effort may or may not change when the current turn ends. The status line shows what Hermes reports; setting the same effort again is safe.".to_owned()]
        );
        assert!(mutates(&effects).is_empty());
    }

    let mut missed = session();
    let request = one_mutate(&command(
        &mut missed,
        ModelCommandOrEffort::Effort(EffortCommand::Set {
            level: Some(Effort::Low),
            save_default: false,
        }),
    ));
    let _ = missed.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Refused {
            reason: "session_busy".to_owned(),
            field: None,
            detail: None,
            rpc_code: 4001,
        },
    });
    let effects = missed.update(ModelsInput::EffortSetReturned {
        session_id: SID.to_owned(),
        level: Some(Effort::Low),
        outcome: Err(MutationOutcome::NotSent {
            reason: "not connected".to_owned(),
        }),
    });
    assert_eq!(
        notice_text(&effects),
        vec!["Not sent: not connected.".to_owned()]
    );

    let mut unknown = session();
    let request = one_mutate(&command(
        &mut unknown,
        ModelCommandOrEffort::Effort(EffortCommand::Set {
            level: Some(Effort::Low),
            save_default: false,
        }),
    ));
    let effects = unknown.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Refused {
            reason: "unknown_execution".to_owned(),
            field: None,
            detail: None,
            rpc_code: 4001,
        },
    });
    assert!(matches!(
        effects
            .iter()
            .find(|effect| matches!(effect, ModelsEffect::SetEffort { .. })),
        Some(ModelsEffect::SetEffort {
            level: Some(Effort::Low),
            ..
        })
    ));
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, ModelsEffect::SetEffort { .. }))
            .count(),
        1
    );
}

#[test]
fn refusal_texts_exact() {
    let detail = "d".repeat(130);
    let cases = [
        (
            "model_resolution_failed",
            4001,
            None,
            "Hermes could not switch to glm-5: it does not know that model on ollama-cloud, or the provider is not set up. Nothing changed.",
        ),
        (
            "permission_denied",
            4001,
            None,
            "Hermes did not allow this window to change the session (permission_denied).",
        ),
        (
            "not_found",
            4001,
            None,
            "Hermes no longer has this session (not_found).",
        ),
        (
            "session_busy",
            4001,
            None,
            "Hermes refused the change (session_busy).",
        ),
        (
            "storage_unavailable",
            5001,
            None,
            "Hermes refused the change (storage_unavailable).",
        ),
    ];
    for (reason, code, extra, text) in cases {
        let mut state = ModelsState::new("L", Ok(SavedModels::empty()));
        let _ = state.update(ModelsInput::SessionOpened {
            session_id: SID.to_owned(),
            revision: Some(12),
            execution_generation: Some(4),
        });
        let request = one_mutate(&command(&mut state, switch("glm-5", None)));
        let effects = state.update(ModelsInput::MutationReturned {
            request,
            outcome: MutationOutcome::Refused {
                reason: reason.to_owned(),
                field: None,
                detail: extra.map(str::to_owned),
                rpc_code: code,
            },
        });
        assert_eq!(notice_text(&effects), vec![text.to_owned()], "{reason}");
    }

    let mut unnamed = ModelsState::new("L", Ok(SavedModels::empty()));
    let _ = unnamed.update(ModelsInput::SessionOpened {
        session_id: SID.to_owned(),
        revision: Some(12),
        execution_generation: Some(4),
    });
    let request = one_mutate(&command(&mut unnamed, switch("glm-5", None)));
    let effects = unnamed.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Refused {
            reason: "Nope!".to_owned(),
            field: None,
            detail: Some(detail.clone()),
            rpc_code: 4001,
        },
    });
    let text = notice_text(&effects).into_iter().next().expect("a notice");
    assert!(text.contains("an unnamed reason"), "{text}");
    assert!(text.contains(&detail[..120]), "{text}");
    assert!(!text.contains(&detail[..121]), "{text}");

    let mut missing = ModelsState::new("L", Ok(SavedModels::empty()));
    let _ = missing.update(ModelsInput::SessionOpened {
        session_id: SID.to_owned(),
        revision: Some(12),
        execution_generation: Some(4),
    });
    let request = one_mutate(&command(
        &mut missing,
        ModelCommandOrEffort::Model(ModelCommand::Switch {
            model: "glm-5".to_owned(),
            provider: None,
            effort: None,
            save_default: false,
        }),
    ));
    let effects = missing.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Refused {
            reason: "model_resolution_failed".to_owned(),
            field: None,
            detail: None,
            rpc_code: 4001,
        },
    });
    assert_eq!(
        notice_text(&effects),
        vec!["Hermes could not switch to glm-5: it does not know that model on the current provider, or the provider is not set up. Nothing changed.".to_owned()]
    );

    let mut method = ModelsState::new("L", Ok(SavedModels::empty()));
    let _ = method.update(ModelsInput::SessionOpened {
        session_id: SID.to_owned(),
        revision: Some(12),
        execution_generation: Some(4),
    });
    let request = one_mutate(&command(&mut method, switch("glm-5", None)));
    let effects = method.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Refused {
            reason: "unknown_method".to_owned(),
            field: None,
            detail: None,
            rpc_code: -32601,
        },
    });
    assert_eq!(
        notice_text(&effects),
        vec!["This Hermes gateway cannot change the model (unknown method). Update Hermes to the pinned release.".to_owned()]
    );

    let mut fenced = ModelsState::new("L", Ok(SavedModels::empty()));
    let _ = fenced.update(ModelsInput::SessionOpened {
        session_id: SID.to_owned(),
        revision: Some(12),
        execution_generation: Some(4),
    });
    let request = one_mutate(&command(&mut fenced, switch("glm-5", None)));
    let effects = fenced.update(ModelsInput::MutationReturned {
        request: request.clone(),
        outcome: MutationOutcome::FenceRefused {
            reason: "revision_conflict".to_owned(),
            revision: Some(15),
            execution_generation: Some(6),
        },
    });
    assert_eq!(mutates(&effects).len(), 1);
    assert!(notice_text(&effects).is_empty());
    let retry = one_mutate(&effects);
    let effects = fenced.update(ModelsInput::MutationReturned {
        request: retry,
        outcome: MutationOutcome::FenceRefused {
            reason: "stale_generation".to_owned(),
            revision: Some(16),
            execution_generation: Some(7),
        },
    });
    assert!(mutates(&effects).is_empty());
    assert_eq!(
        notice_text(&effects),
        vec!["Another window changed this session at the same moment, so nothing was changed. Try again.".to_owned()]
    );
}

#[test]
fn one_request_in_flight_latest_wins() {
    let mut state = session();
    let first = command(&mut state, switch("glm-5", None));
    let first_request = one_mutate(&first);
    let second = command(&mut state, switch("minimax-m3", None));
    assert!(mutates(&second).is_empty());
    let third = command(&mut state, switch("gone", None));
    assert!(mutates(&third).is_empty());
    let settled = state.update(ModelsInput::MutationReturned {
        request: first_request,
        outcome: MutationOutcome::Committed(json!({
            "operation": "model",
            "model": "glm-5",
            "provider": "ollama-cloud",
            "revision": 13,
            "execution_generation": 5
        })),
    });
    let follow = one_mutate(&settled);
    assert_eq!(
        follow.payload.get("model").and_then(|value| value.as_str()),
        Some("gone")
    );
    assert_ne!(follow.request_id, "L-m1");
}

#[test]
fn receipt_broadcast_is_silent() {
    let mut state = session();
    let effects = event(
        &mut state,
        "session.updated",
        json!({
            "operation": "model",
            "model": "glm-5",
            "provider": "ollama-cloud",
            "revision": 13,
            "execution_generation": 5
        }),
    );
    assert!(notice_text(&effects).is_empty());
    assert!(mutates(&effects).is_empty());
    let fences = state.fences(SID);
    assert_eq!(fences.revision, Some(13));
    assert_eq!(fences.execution_generation, Some(5));
}
