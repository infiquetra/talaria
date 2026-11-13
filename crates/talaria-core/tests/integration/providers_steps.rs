use crate::providers_support as support;
use support::*;
use talaria_core::providers::*;

fn press(state: &mut ProvidersState, key: ProvidersKey) -> Vec<ProvidersEffect> {
    step(state, ProvidersInput::Key(key))
}
fn title(state: &ProvidersState) -> Option<String> {
    screen(state).card.and_then(|c| c.step)
}

#[test]
fn key_steps_derive_from_the_existing_check_and_save_states() {
    let (mut state, _) = open(keys_tab(), true);
    step(
        &mut state,
        ProvidersInput::EnvLoaded(Ok(vec![env(
            "EXAMPLE_API_KEY",
            "example-00",
            "Example provider 00",
            false,
        )])),
    );
    press(&mut state, ProvidersKey::Enter);
    assert_eq!(title(&state).as_deref(), Some("Step 1 of 3: Paste key"));
    step(&mut state, ProvidersInput::SecretEdited { empty: false });
    let effects = step(&mut state, ProvidersInput::SecretSubmitted);
    assert!(has(&effects, |e| matches!(
        e,
        ProvidersEffect::CheckKey { .. }
    )));
    assert_eq!(title(&state).as_deref(), Some("Step 2 of 3: Check key"));
    let effects = step(
        &mut state,
        ProvidersInput::KeyChecked(Ok(KeyCheck {
            ok: true,
            reachable: true,
            message: String::new(),
        })),
    );
    assert!(has(&effects, |e| matches!(
        e,
        ProvidersEffect::SaveKey { .. }
    )));
    assert_eq!(
        title(&state).as_deref(),
        Some("Step 3 of 3: Save and read back")
    );
}

#[test]
fn account_steps_and_external_instructions() {
    let (mut state, _) = open(OpenTarget::Tab(ProvidersTab::Accounts), true);
    step(
        &mut state,
        ProvidersInput::OauthLoaded(Ok(vec![account(
            "example-00",
            "Example provider 00",
            true,
            AccountStatus::NotSignedIn,
        )])),
    );
    press(&mut state, ProvidersKey::Enter);
    assert_eq!(title(&state).as_deref(), Some("Step 1 of 3: Start sign-in"));
    step(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(SignInStart {
                session_id: "fixture-signin".into(),
                user_code: "ABCD-1234".into(),
                verification_url: "https://example.invalid/device".into(),
                expires_in: Some(90),
                poll_interval: Some(5),
            }),
        },
    );
    assert_eq!(
        title(&state).as_deref(),
        Some("Step 2 of 3: Approve in your browser")
    );
    step(
        &mut state,
        ProvidersInput::SignInPolled {
            session_id: "fixture-signin".into(),
            result: Ok(PollReply {
                status: PollStatus::Approved,
                error_message: None,
                account_email: None,
                model: None,
                retryable: None,
                retry_after: None,
            }),
        },
    );
    assert_eq!(
        title(&state).as_deref(),
        Some("Step 3 of 3: Confirm sign-in")
    );
    let (mut external, _) = open(OpenTarget::Tab(ProvidersTab::Accounts), true);
    let mut row = account(
        "example-external",
        "Example external",
        false,
        AccountStatus::NotSignedIn,
    );
    row.cli_command = Some("example-cli login".into());
    step(&mut external, ProvidersInput::OauthLoaded(Ok(vec![row])));
    press(&mut external, ProvidersKey::Enter);
    assert_eq!(screen(&external).card.unwrap().kind, CardKind::External);
    assert!(title(&external).is_none());
}

#[test]
fn endpoint_steps_preserve_test_and_save_effects() {
    let (mut state, _) = open(OpenTarget::Tab(ProvidersTab::Endpoints), true);
    step(
        &mut state,
        ProvidersInput::EndpointsLoaded(Ok(list_of(vec![endpoint(
            "fixture-endpoint",
            "Example endpoint",
            "https://example.invalid/v1",
            "example-model",
        )]))),
    );
    press(&mut state, ProvidersKey::Down);
    press(&mut state, ProvidersKey::Enter);
    assert_eq!(
        title(&state).as_deref(),
        Some("Step 1 of 3: Fill in the endpoint")
    );
    let effects = press(&mut state, ProvidersKey::Action(ProvidersAction::Test));
    assert!(has(&effects, |e| matches!(
        e,
        ProvidersEffect::CheckEndpoint(_)
    )));
    assert_eq!(title(&state).as_deref(), Some("Step 2 of 3: Test"));
    step(
        &mut state,
        ProvidersInput::EndpointChecked(Ok(EndpointCheck {
            ok: true,
            reachable: true,
            message: "checked".into(),
            models: vec![],
            model_details: vec![],
            resolved_base_url: None,
        })),
    );
    let effects = press(&mut state, ProvidersKey::Action(ProvidersAction::Save));
    assert!(has(&effects, |e| matches!(
        e,
        ProvidersEffect::SaveEndpoint(_)
    )));
    assert_eq!(
        title(&state).as_deref(),
        Some("Step 3 of 3: Save and read back")
    );
}
