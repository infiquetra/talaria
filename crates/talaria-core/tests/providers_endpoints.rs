//! Providers endpoint form.

#[path = "support/providers.rs"]
mod support;

use support::{draft, endpoint, has, joined, list_of, open, screen, step};
use talaria_core::providers::{
    ApiKeyChange, EndpointCheck, Failure, OpenTarget, ProvidersAction, ProvidersEffect,
    ProvidersInput, ProvidersKey, ProvidersTab,
};

fn endpoints_tab() -> OpenTarget {
    OpenTarget::Tab(ProvidersTab::Endpoints)
}

fn loaded(rows: Vec<talaria_core::providers::Endpoint>) -> talaria_core::providers::ProvidersState {
    let (mut state, _) = open(endpoints_tab(), true);
    let _ = step(
        &mut state,
        ProvidersInput::EndpointsLoaded(Ok(list_of(rows))),
    );
    state
}

#[test]
fn form_checks_mirror_hermes() {
    let mut state = loaded(vec![endpoint(
        "demo",
        "Demo",
        "https://example.invalid/v1",
        "example-model",
    )]);
    let text = joined(&state);
    assert!(text.contains("+ Add an endpoint"), "{text}");
    assert!(text.contains("Demo"), "{text}");
    assert!(text.contains("https://example.invalid/v1"), "{text}");
    assert!(text.contains("example-model"), "{text}");
    assert!(!text.contains("api_key_preview"), "{text}");

    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let effects = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Save)),
    );
    assert!(
        !has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::SaveEndpoint(_)
        )),
        "an empty add is not saved: {effects:?}"
    );
    assert!(
        joined(&state).contains("An endpoint named Demo already exists; edit it instead.")
            || joined(&state).contains("name"),
        "a blank or duplicate name is refused: {}",
        joined(&state)
    );

    for ch in ['n', 'e', 'w'] {
        let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char(ch)));
    }
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Tab));
    for ch in ['f', 't', 'p', ':', '/', '/', 'x'] {
        let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char(ch)));
    }
    let effects = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Test)),
    );
    assert!(
        !has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::CheckEndpoint(_)
        )),
        "a base URL needs http or https and a host: {effects:?}"
    );

    let mut older = endpoint("old", "Old", "https://example.invalid/old", "old-model");
    older.source = "custom_providers".into();
    older.has_api_key = true;
    older.is_current = true;
    let mut current = list_of(vec![older]);
    current.current_provider = "old".into();
    current.current_model = "old-model".into();
    let (mut state, _) = open(endpoints_tab(), true);
    let _ = step(&mut state, ProvidersInput::EndpointsLoaded(Ok(current)));
    let text = joined(&state);
    assert!(text.contains("Default: old-model on old"), "{text}");
    assert!(text.contains("· default"), "{text}");
    assert!(text.contains("· key saved"), "{text}");
    assert!(text.contains("· older format"), "{text}");

    let mut direct = endpoint("custom", "Custom", "https://example.invalid/c", "m");
    direct.source = "direct-config".into();
    let mut state = loaded(vec![direct]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Down));
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    assert!(
        joined(&state).contains(
            "This endpoint is set directly in the profile's model settings. Change it in Model roles."
        ),
        "{}",
        joined(&state)
    );
    assert_eq!(screen(&state).tab, ProvidersTab::Endpoints);
}

#[test]
fn save_body_rules() {
    let mut state = loaded(vec![]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    for ch in ['n', 'e', 'w'] {
        let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char(ch)));
    }
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Tab));
    for ch in "https://example.invalid/v1".chars() {
        let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char(ch)));
    }
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Tab));
    let _ = step(&mut state, ProvidersInput::SecretEdited { empty: false });
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Tab));
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Right));
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Tab));
    for ch in "example-model".chars() {
        let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char(ch)));
    }
    let effects = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Save)),
    );
    let sent = effects.iter().find_map(|effect| match effect {
        ProvidersEffect::SaveEndpoint(body) => Some(body.clone()),
        _ => None,
    });
    let sent = sent.expect("save sends the form");
    assert_eq!(sent.id, "");
    assert_eq!(sent.name, "new");
    assert_eq!(sent.base_url, "https://example.invalid/v1");
    assert_eq!(sent.model, "example-model");
    assert_eq!(sent.api_key, ApiKeyChange::Typed);
    assert_eq!(sent.api_mode.as_deref(), Some("chat_completions"));
    assert_eq!(sent.context_length, None);
    assert!(sent.discover_models);
    assert!(sent.models.is_none());

    let mut state = loaded(vec![endpoint(
        "demo",
        "Demo",
        "https://example.invalid/v1",
        "example-model",
    )]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Down));
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let cleared = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::ClearKey)),
    );
    assert!(
        has(&cleared, |effect| matches!(
            effect,
            ProvidersEffect::ForgetSecret
        )),
        "clearing the key forgets the typed secret: {cleared:?}"
    );
    let effects = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Save)),
    );
    let sent = effects.iter().find_map(|effect| match effect {
        ProvidersEffect::SaveEndpoint(body) => Some(body.clone()),
        _ => None,
    });
    let sent = sent.expect("clear then save");
    assert_eq!(sent.id, "demo");
    assert_eq!(sent.api_key, ApiKeyChange::Clear);

    let mut editing = loaded(vec![]);
    let _ = step(&mut editing, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(&mut editing, ProvidersInput::Key(ProvidersKey::Char('a')));
    let effects = step(&mut editing, ProvidersInput::Key(ProvidersKey::Esc));
    assert!(
        joined(&editing).contains("Discard your changes?"),
        "{}",
        joined(&editing)
    );
    assert!(!has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::Close
    )));
    let _ = draft("new", "https://example.invalid/v1", "example-model");
}

#[test]
fn test_results_and_delete() {
    let mut state = loaded(vec![]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    for ch in ['n', 'e', 'w'] {
        let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char(ch)));
    }
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Tab));
    for ch in "https://example.invalid/v1".chars() {
        let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char(ch)));
    }
    let effects = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Test)),
    );
    assert!(
        has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::CheckEndpoint(_)
        )),
        "test is allowed with an empty model: {effects:?}"
    );
    let _ = step(
        &mut state,
        ProvidersInput::EndpointChecked(Ok(EndpointCheck {
            ok: false,
            reachable: true,
            message: "chat route missing".into(),
            models: vec!["first-model".into(), "second-model".into()],
            model_details: vec![],
            resolved_base_url: Some("https://example.invalid/resolved".into()),
        })),
    );
    let text = joined(&state);
    assert!(text.contains("chat route missing"), "{text}");
    assert!(
        text.contains("first-model"),
        "an empty model takes the first: {text}"
    );
    assert!(
        text.contains(
            "Talaria will save https://example.invalid/resolved, the address that answered."
        ),
        "{text}"
    );
    let mut edited = state.clone();
    let effects = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Save)),
    );
    let sent = effects.iter().find_map(|effect| match effect {
        ProvidersEffect::SaveEndpoint(body) => Some(body.clone()),
        _ => None,
    });
    let sent = sent.expect("save after a test");
    assert_eq!(sent.base_url, "https://example.invalid/resolved");
    assert_eq!(sent.model, "first-model");
    assert_eq!(
        sent.models.as_deref(),
        Some(["first-model".to_owned(), "second-model".to_owned()].as_slice())
    );
    let _ = step(
        &mut state,
        ProvidersInput::EndpointSaved(Ok(list_of(vec![]))),
    );

    let _ = step(&mut edited, ProvidersInput::Key(ProvidersKey::Char('x')));
    let effects = step(
        &mut edited,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Save)),
    );
    let sent = effects.iter().find_map(|effect| match effect {
        ProvidersEffect::SaveEndpoint(body) => Some(body.clone()),
        _ => None,
    });
    let sent = sent.expect("editing the base URL clears the test");
    assert_eq!(sent.models, None);
    assert!(sent.base_url.contains('x') || sent.base_url.ends_with('x'));

    let mut row = endpoint(
        "demo",
        "Demo",
        "https://example.invalid/v1",
        "example-model",
    );
    row.is_current = true;
    row.has_api_key = true;
    let mut state = loaded(vec![row]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Down));
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let effects = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Test)),
    );
    assert!(
        joined(&state).contains("Testing without the saved key; type the key to test with it."),
        "{}",
        joined(&state)
    );
    let _ = effects;
    let _ = step(
        &mut state,
        ProvidersInput::EndpointChecked(Ok(EndpointCheck {
            ok: false,
            reachable: true,
            message: String::new(),
            models: vec![],
            model_details: vec![],
            resolved_base_url: None,
        })),
    );
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Esc));
    let _ = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Remove)),
    );
    let text = joined(&state);
    assert!(
        text.contains("Delete the endpoint Demo (https://example.invalid/v1) from profile fixture? Its saved key is removed too."),
        "{text}"
    );
    assert!(
        text.contains("It is the profile's default provider, so the default provider is cleared."),
        "{text}"
    );
    let effects = step(&mut state, ProvidersInput::Key(ProvidersKey::Char('y')));
    assert!(
        has(
            &effects,
            |effect| matches!(effect, ProvidersEffect::DeleteEndpoint { id } if id == "demo")
        ),
        "{effects:?}"
    );
    let _ = step(
        &mut state,
        ProvidersInput::EndpointDeleted(Err(Failure::Http {
            status: 404,
            detail: None,
        })),
    );
    assert!(
        joined(&state).contains("Hermes no longer has Demo."),
        "{}",
        joined(&state)
    );

    let (mut state, _) = open(endpoints_tab(), true);
    let _ = step(
        &mut state,
        ProvidersInput::EndpointsLoaded(Ok(list_of(vec![]))),
    );
    let text = joined(&state);
    assert!(text.contains("+ Add an endpoint"), "{text}");
    assert!(
        text.contains("No custom endpoints in this profile."),
        "{text}"
    );
}
