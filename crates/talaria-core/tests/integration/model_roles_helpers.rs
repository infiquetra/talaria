//! Helper staleness and endpoint rows.

#![allow(clippy::too_many_lines)]

use talaria_core::model_roles::{
    ApplyStep, AssignScopeView, Assignment, AuxSlot, EffortChange, Loaded, ModelRolesState,
    RolesEffect, RolesInput, RolesKey, RouteCatalog, RouteProvider, Section, stale_helpers,
};

fn slot(task: &str, provider: &str, local_endpoint: bool) -> AuxSlot {
    AuxSlot {
        task: task.to_owned(),
        provider: provider.to_owned(),
        model: if provider == "auto" || provider.is_empty() {
            String::new()
        } else {
            "m".to_owned()
        },
        base_url: if local_endpoint {
            "http://127.0.0.1:9".to_owned()
        } else {
            String::new()
        },
        reasoning_effort: None,
        local_endpoint,
        label: None,
        hint: None,
        plugin: None,
        inherit_from: None,
        effective: None,
    }
}

#[test]
fn stale_rule() {
    let slots = vec![
        slot("vision", "auto", false),
        slot("compression", "", false),
        slot("skills_hub", "main", false),
        slot("approval", "other", false),
        slot("mcp", "Other", false),
        slot("title_generation", "remote", true),
        slot("review", "REMOTE", false),
    ];
    let stale = stale_helpers(&slots, "other");
    let tasks: Vec<&str> = stale.iter().map(|item| item.task.as_str()).collect();
    assert_eq!(tasks, vec!["review"]);
    assert!(
        stale_helpers(&slots, "").is_empty(),
        "an empty main model makes no helper stale"
    );
    assert!(
        stale_helpers(&slots, "OTHER")
            .iter()
            .all(|item| item.task != "mcp")
    );
}

fn catalog() -> RouteCatalog {
    RouteCatalog {
        providers: vec![
            RouteProvider {
                slug: "lab".to_owned(),
                name: "Lab".to_owned(),
                models: vec!["m".to_owned()],
                authenticated: true,
                is_user_defined: true,
                api_url: "https://endpoint.invalid".to_owned(),
            },
            RouteProvider {
                slug: "public".to_owned(),
                name: "Public".to_owned(),
                models: vec!["a".to_owned()],
                authenticated: true,
                is_user_defined: false,
                api_url: String::new(),
            },
        ],
    }
}

fn choose(state: &mut ModelRolesState) {
    state.update(RolesInput::Key(RolesKey::Enter));
    state.update(RolesInput::Key(RolesKey::Enter));
    state.update(RolesInput::Key(RolesKey::Enter));
}

#[test]
fn api_url_rows_send_base_url() {
    let (mut state, _) = ModelRolesState::new("fixture".to_owned());
    state.update(RolesInput::CatalogLoaded(catalog()));
    state.update(RolesInput::Loaded(Loaded::Auxiliary(
        talaria_core::model_roles::AuxView {
            slots: vec![slot("vision", "auto", false)],
            main: talaria_core::model_roles::RouteTriple {
                provider: "public".to_owned(),
                model: "a".to_owned(),
                base_url: String::new(),
            },
        },
    )));
    state.update(RolesInput::Key(RolesKey::Section(Section::Helpers)));
    choose(&mut state);
    state.update(RolesInput::Key(RolesKey::Review));
    state.update(RolesInput::Fresh {
        section: Section::Helpers,
        reads: Vec::new(),
    });
    let effects = state.update(RolesInput::Key(RolesKey::Enter));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RolesEffect::Assign { assignment, .. } if assignment.base_url == "https://endpoint.invalid" && assignment.task == "vision"
    )));

    let (mut main, _) = ModelRolesState::new("fixture".to_owned());
    main.update(RolesInput::CatalogLoaded(catalog()));
    main.update(RolesInput::Key(RolesKey::Section(Section::Main)));
    choose(&mut main);
    main.update(RolesInput::Key(RolesKey::Review));
    main.update(RolesInput::Fresh {
        section: Section::Main,
        reads: Vec::new(),
    });
    let effects = main.update(RolesInput::Key(RolesKey::Enter));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RolesEffect::Assign { step: ApplyStep::MainModel, assignment } if assignment.base_url == "https://endpoint.invalid"
    )));

    let (mut fallbacks, _) = ModelRolesState::new("fixture".to_owned());
    fallbacks.update(RolesInput::CatalogLoaded(catalog()));
    fallbacks.update(RolesInput::Key(RolesKey::Section(Section::Fallbacks)));
    fallbacks.update(RolesInput::Key(RolesKey::Add));
    choose(&mut fallbacks);
    fallbacks.update(RolesInput::Key(RolesKey::Review));
    fallbacks.update(RolesInput::Fresh {
        section: Section::Fallbacks,
        reads: Vec::new(),
    });
    let effects = fallbacks.update(RolesInput::Key(RolesKey::Enter));
    let patch = effects.iter().find_map(|effect| match effect {
        RolesEffect::PutConfig { patch, .. } => Some(patch),
        _ => None,
    });
    let patch = patch.expect("fallbacks write the chain");
    assert!(patch["fallback_providers"][0].get("base_url").is_none());
    assert_eq!(patch["fallback_providers"][0]["provider"], "lab");
}

#[test]
fn stale_reset_is_per_slot() {
    let (mut state, _) = ModelRolesState::new("fixture".to_owned());
    state.update(RolesInput::Loaded(Loaded::Auxiliary(
        talaria_core::model_roles::AuxView {
            slots: vec![
                slot("vision", "other", false),
                slot("approval", "other", false),
                slot("mcp", "auto", false),
            ],
            main: talaria_core::model_roles::RouteTriple {
                provider: "ollama-cloud".to_owned(),
                model: "m".to_owned(),
                base_url: String::new(),
            },
        },
    )));
    state.update(RolesInput::Key(RolesKey::Section(Section::Helpers)));
    state.update(RolesInput::Key(RolesKey::ResetStale));
    state.update(RolesInput::Key(RolesKey::Review));
    state.update(RolesInput::Fresh {
        section: Section::Helpers,
        reads: Vec::new(),
    });
    let effects = state.update(RolesInput::Key(RolesKey::Enter));
    let assigns: Vec<&Assignment> = effects
        .iter()
        .filter_map(|effect| match effect {
            RolesEffect::Assign { assignment, .. } => Some(assignment),
            _ => None,
        })
        .collect();
    assert_eq!(assigns.len(), 2, "only the stale slots are reset");
    assert!(
        assigns
            .iter()
            .all(|item| item.scope == AssignScopeView::Auxiliary)
    );
    assert!(
        assigns
            .iter()
            .all(|item| item.provider == "auto" && item.model.is_empty())
    );
    assert!(
        assigns
            .iter()
            .all(|item| item.reasoning_effort == EffortChange::Inherit)
    );
    assert!(assigns.iter().all(|item| item.task != "__reset__"));
    let tasks: Vec<&str> = assigns.iter().map(|item| item.task.as_str()).collect();
    assert_eq!(tasks, vec!["vision", "approval"]);
}
