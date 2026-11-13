//! Issue 230, scenario "parse and ladder".

use serde_json::json;
use talaria_core::models::Effort;
use talaria_core::models::{
    EffortCommand, ModelCommandOrEffort, ModelsEffect, ModelsInput, ModelsState, SavedModels,
};
use talaria_core::mutation::MutationOutcome;

#[test]
fn parse_and_ladder() {
    assert_eq!(Effort::parse("High"), Some(Effort::High));
    assert_eq!(Effort::parse(" xhigh "), Some(Effort::Xhigh));
    assert_eq!(Effort::parse("ULTRA"), Some(Effort::Ultra));
    assert_eq!(Effort::parse("none"), Some(Effort::None));
    assert_eq!(Effort::parse("NONE"), Some(Effort::None));
    assert_eq!(Effort::parse("false"), None);
    assert_eq!(Effort::parse("disabled"), None);
    assert_eq!(Effort::parse("hgih"), None);
    assert_eq!(Effort::parse(""), None);
    assert_eq!(Effort::parse("  "), None);

    assert_eq!(Effort::Medium.up(), Effort::High);
    assert_eq!(Effort::None.down(), Effort::None);
    assert_eq!(Effort::Ultra.up(), Effort::Ultra);
    assert_eq!(Effort::High.as_wire(), "high");
    assert_eq!(Effort::None.as_wire(), "none");
}

#[test]
fn session_info_loads_catalogue_once_and_looks_up_the_route() {
    let mut state = ModelsState::new("L", Ok(SavedModels::empty()));
    state.update(ModelsInput::ForegroundChanged {
        session_id: Some("s".into()),
    });
    let event = || ModelsInput::SessionEvent {
        session_id: "s".into(),
        type_name: "session.info".into(),
        payload: json!({"model":"same", "provider":"a", "reasoning_effort":"max"}),
    };
    let effects = state.update(event());
    assert!(effects.iter().any(|e| matches!(e, ModelsEffect::LoadOptions { session_id, refresh: false } if session_id == "s")));
    assert!(
        !state
            .update(event())
            .iter()
            .any(|e| matches!(e, ModelsEffect::LoadOptions { .. }))
    );
    let result = serde_json::from_value(json!({"model":"same", "provider":"a", "providers":[
        {"slug":"a", "name":"A", "models":["same"], "authenticated":true, "capabilities":{"same":{"fast":false,"reasoning":false}}},
        {"slug":"b", "name":"B", "models":["same"], "authenticated":true, "capabilities":{"same":{"fast":false,"reasoning":true}}}
    ]})).unwrap();
    state.update(ModelsInput::OptionsLoaded {
        session_id: "s".into(),
        result,
    });
    assert_eq!(state.effort_offered("s", "a", "same"), Some(false));
    assert_eq!(state.effort_offered("s", "b", "same"), Some(true));
    assert_eq!(state.effort_offered("s", "a", "missing"), None);
}

#[test]
fn effort_notice_reflects_model_capability() {
    for (offered, expected) in [
        (
            false,
            "Effort: max for this session; this model does not offer effort levels.",
        ),
        (true, "Effort: max for this session."),
    ] {
        let mut state = ModelsState::new("L", Ok(SavedModels::empty()));
        state.update(ModelsInput::SessionOpened {
            session_id: "s".into(),
            revision: Some(1),
            execution_generation: Some(1),
        });
        state.update(ModelsInput::ForegroundChanged {
            session_id: Some("s".into()),
        });
        let result = serde_json::from_value(json!({"model":"m", "provider":"p", "providers":[{"slug":"p", "name":"P", "models":["m"], "authenticated":true, "capabilities":{"m":{"fast":false,"reasoning":offered}}}]})).unwrap();
        state.update(ModelsInput::OptionsLoaded {
            session_id: "s".into(),
            result,
        });
        let effects = state.update(ModelsInput::Command {
            session_id: Some("s".into()),
            profile: "profile".into(),
            command: Ok(ModelCommandOrEffort::Effort(EffortCommand::Set {
                level: Some(Effort::Max),
                save_default: false,
            })),
        });
        let request = effects
            .into_iter()
            .find_map(|e| {
                if let ModelsEffect::Mutate(r) = e {
                    Some(r)
                } else {
                    None
                }
            })
            .unwrap();
        let effects = state.update(ModelsInput::MutationReturned {
            request,
            outcome: MutationOutcome::Committed(
                json!({"effort":"max", "revision":2, "execution_generation":1}),
            ),
        });
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, ModelsEffect::Notice { text, .. } if text == expected)),
            "{effects:?}"
        );
    }
}

#[test]
fn failed_catalogue_read_is_not_retried_and_remains_unknown() {
    let mut state = ModelsState::new("L", Ok(SavedModels::empty()));
    state.update(ModelsInput::ForegroundChanged {
        session_id: Some("s".into()),
    });
    let event = || ModelsInput::SessionEvent {
        session_id: "s".into(),
        type_name: "session.info".into(),
        payload: json!({"model":"m", "provider":"p", "reasoning_effort":"max"}),
    };
    state.update(event());
    state.update(ModelsInput::OptionsFailed {
        session_id: "s".into(),
        reason: "offline".into(),
        unknown_method: false,
    });
    assert_eq!(state.effort_offered("s", "p", "m"), None);
    assert!(
        !state
            .update(event())
            .iter()
            .any(|e| matches!(e, ModelsEffect::LoadOptions { .. }))
    );
}
