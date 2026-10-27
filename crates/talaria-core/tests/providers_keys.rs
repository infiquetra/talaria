//! Providers keys tab.

#[path = "support/providers.rs"]
mod support;

use std::time::Instant;

use support::{catalogue, env, has, joined, keys_tab, open, provider, screen, step};
use talaria_core::providers::{
    ApiKeyChange, Failure, KeyCheck, OpenTarget, ProvidersAction, ProvidersEffect, ProvidersInput,
    ProvidersKey, ProvidersTab,
};

fn load_env(
    state: &mut talaria_core::providers::ProvidersState,
    rows: Vec<talaria_core::providers::EnvRow>,
) -> Vec<ProvidersEffect> {
    step(state, ProvidersInput::EnvLoaded(Ok(rows)))
}

#[test]
fn rows_are_primary_provider_password_entries() {
    let (mut state, effects) = open(keys_tab(), true);
    assert!(
        has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::LoadEnv
        )) && has(&effects, |effect| {
            matches!(effect, ProvidersEffect::LoadOptions)
        }),
        "open should load the env list and model options together, got {effects:?}"
    );
    assert_eq!(
        effects.len(),
        2,
        "open loads only those two calls: {effects:?}"
    );

    let mut skipped = env("", "openai", "OpenAI", true);
    skipped.primary = true;
    let mut secondary = env("OPENAI_ORG", "openai", "OpenAI", true);
    secondary.primary = false;
    secondary.slug = "openai".into();
    let mut plain = env("OPENAI_BASE", "openai", "OpenAI", true);
    plain.is_password = false;
    let mut channel = env("SLACK_TOKEN", "slack", "Slack", true);
    channel.channel_managed = true;
    let mut custom = env("MY_KEY", "openai", "OpenAI", true);
    custom.custom = true;
    let mut shared = env("DASHSCOPE_API_KEY", "qwen", "Qwen Cloud", true);
    shared.tail = Some("«redacted:abcd...wxyz»".into());
    shared.shared_with = vec![("alibaba".into(), "Alibaba Cloud".into())];
    let mut beta = env("BETA_API_KEY", "beta", "beta cloud", true);
    beta.tail = Some("«redacted:zzzz...abcd»".into());
    let mut alpha = env("ALPHA_API_KEY", "alpha", "Alpha", false);

    let _ = load_env(
        &mut state,
        vec![
            skipped,
            secondary,
            plain,
            channel,
            custom,
            alpha,
            beta.clone(),
            shared.clone(),
            env("OLLAMA_API_KEY", "ollama-cloud", "Ollama Cloud", true),
        ],
    );
    let shown = screen(&state);
    let text = joined(&state);
    assert!(text.contains("Ollama Cloud"), "{text}");
    assert!(text.contains("Qwen Cloud"), "{text}");
    assert!(text.contains("beta cloud"), "{text}");
    assert!(text.contains("Alpha"), "{text}");
    assert!(
        !text.contains("OPENAI_ORG"),
        "secondary rows stay off the list: {text}"
    );
    assert!(!text.contains("OPENAI_BASE"), "{text}");
    assert!(!text.contains("SLACK_TOKEN"), "{text}");
    assert!(!text.contains("MY_KEY"), "{text}");
    assert!(
        text.contains("also used by Alibaba Cloud"),
        "shared variables name the other provider: {text}"
    );
    let positions = ["beta cloud", "Ollama Cloud", "Qwen Cloud", "Alpha"]
        .map(|label| text.find(label).expect(label));
    assert!(
        positions[0] < positions[1] && positions[1] < positions[2] && positions[2] < positions[3],
        "set rows come first, then labels ignoring case: {text}"
    );
    let _ = (shown, beta, shared);
}

#[test]
fn status_joins_model_options_by_slug() {
    let (mut state, _) = open(keys_tab(), true);
    let mut ready = env("OLLAMA_API_KEY", "ollama-cloud", "Ollama Cloud", true);
    ready.tail = Some("«redacted:abcd...wxyz»".into());
    let mut saved = env("DASHSCOPE_API_KEY", "qwen", "Qwen Cloud", true);
    saved.tail = Some("«redacted:zzzz...abcd»".into());
    let mut odd = env("ODD_API_KEY", "odd", "Odd", true);
    odd.tail = Some("not-a-preview".into());
    let missing = env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false);
    let outside = env("EXTERNAL_API_KEY", "external", "External", false);
    let _ = load_env(&mut state, vec![ready, saved, odd, missing, outside]);
    let loading = joined(&state);
    assert!(
        loading.contains("key …wxyz saved · checking…"),
        "a set row checks while options load: {loading}"
    );
    assert!(
        loading.contains("no key · checking…"),
        "an unset row checks while options load: {loading}"
    );
    assert!(!loading.contains("abcd...wxyz"), "{loading}");

    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![
            provider("ollama-cloud", "Ollama Cloud", true, 12),
            provider("qwen", "Qwen Cloud", false, 3),
            provider("openrouter", "OpenRouter", true, 4),
            {
                let mut row = provider("odd", "Odd", true, 0);
                row["models"] = serde_json::json!([]);
                row["total_models"] = serde_json::json!(0);
                row
            },
        ]))),
    );
    let text = joined(&state);
    assert!(text.contains("ready · 12 models · key …wxyz"), "{text}");
    assert!(text.contains("key …abcd saved · not ready"), "{text}");
    assert!(
        text.contains("ready · key saved") && !text.contains("ready · 0 models"),
        "a zero model count is omitted: {text}"
    );
    assert!(
        text.contains("ready · credential not in this profile's key file"),
        "{text}"
    );
    assert!(text.contains("no key"), "{text}");
    assert!(
        !text.contains("paste "),
        "an option warning is not a key status: {text}"
    );

    let (mut failed, _) = open(keys_tab(), true);
    let mut set = env("OLLAMA_API_KEY", "ollama-cloud", "Ollama Cloud", true);
    set.tail = Some("«redacted:abcd...wxyz»".into());
    let _ = load_env(
        &mut failed,
        vec![
            set,
            env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false),
        ],
    );
    let _ = step(
        &mut failed,
        ProvidersInput::OptionsLoaded(Err(Failure::NotSent("socket down".into()))),
    );
    let text = joined(&failed);
    assert!(text.contains("key …wxyz saved"), "{text}");
    assert!(text.contains("no key"), "{text}");
    assert!(
        text.contains("Could not check readiness: socket down. ctrl+r tries again."),
        "{text}"
    );
}

#[test]
fn preview_shows_only_the_tail() {
    let (mut state, _) = open(keys_tab(), true);
    let mut matched = env("OLLAMA_API_KEY", "ollama-cloud", "Ollama Cloud", true);
    matched.tail = Some("«redacted:abcd...wxyz»".into());
    let mut short = env("SHORT_API_KEY", "short", "Short", true);
    short.tail = Some("«redacted:***»".into());
    let mut raw = env("RAW_API_KEY", "raw", "Raw", true);
    raw.tail = Some("sk-test-SENTINEL-0123456789abcdef".into());
    let mut empty = env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false);
    empty.tail = None;
    let _ = load_env(&mut state, vec![matched, short, raw, empty]);
    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Err(Failure::NotSent("down".into()))),
    );
    let text = joined(&state);
    assert!(text.contains("key …wxyz"), "{text}");
    assert!(!text.contains("abcd"), "the first four stay hidden: {text}");
    assert!(text.contains("key saved"), "{text}");
    assert!(!text.contains("«redacted:"), "{text}");
    assert!(
        !text.contains("sk-test-SENTINEL-0123456789abcdef"),
        "{text}"
    );
    assert!(!text.contains("0123456789abcdef"), "{text}");
}

#[test]
fn check_then_save_sequence() {
    let (mut state, _) = open(keys_tab(), true);
    let mut row = env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false);
    row.label = "OpenRouter".into();
    let _ = load_env(&mut state, vec![row]);
    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![provider(
            "openrouter",
            "OpenRouter",
            false,
            0,
        )]))),
    );
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let shown = screen(&state);
    let text = joined(&state);
    assert!(text.contains("API key for OpenRouter"), "{text}");
    assert!(
        text.contains("Saved to profile fixture's key file as OPENROUTER_API_KEY."),
        "{text}"
    );
    assert!(
        text.contains("enter save · ctrl+u clear · esc cancel"),
        "{text}"
    );
    assert!(shown.secret_focus, "the key card takes input");

    let effects = step(&mut state, ProvidersInput::SecretSubmitted);
    assert!(
        effects.iter().any(|effect| {
            matches!(effect, ProvidersEffect::CheckKey { env_var } if env_var == "OPENROUTER_API_KEY")
        }),
        "submit checks the key: {effects:?}"
    );
    let shown = screen(&state);
    assert!(!shown.secret_focus, "a check locks the field");
    assert!(
        joined(&state).contains("Checking the key…"),
        "{}",
        joined(&state)
    );

    let effects = step(
        &mut state,
        ProvidersInput::KeyChecked(Ok(KeyCheck {
            ok: true,
            reachable: true,
            message: String::new(),
        })),
    );
    assert!(
        effects.iter().any(
            |effect| matches!(effect, ProvidersEffect::SaveKey { slug } if slug == "openrouter")
        ),
        "an accepted key is saved: {effects:?}"
    );

    let saved_row = catalogue(vec![provider("openrouter", "OpenRouter", true, 2)])
        .providers
        .remove(0);
    let effects = step(&mut state, ProvidersInput::KeySaved(Ok(saved_row)));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ProvidersEffect::ForgetSecret)),
        "a save forgets the secret: {effects:?}"
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ProvidersEffect::LoadEnv)),
        "a save reloads the env list: {effects:?}"
    );
    let text = joined(&state);
    assert!(text.contains("Saved the key for OpenRouter."), "{text}");
    assert!(screen(&state).card.is_none(), "the card closes");

    let (mut blocked, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut blocked,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let _ = step(&mut blocked, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(&mut blocked, ProvidersInput::SecretSubmitted);
    let _ = step(
        &mut blocked,
        ProvidersInput::KeyChecked(Ok(KeyCheck {
            ok: false,
            reachable: true,
            message: "Provider returned HTTP 500 for this key.".into(),
        })),
    );
    let text = joined(&blocked);
    assert!(
        text.contains("Provider returned HTTP 500 for this key."),
        "{text}"
    );
    let effects = step(&mut blocked, ProvidersInput::SecretSubmitted);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ProvidersEffect::CheckKey { .. })),
        "a rejected key is checked again: {effects:?}"
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, ProvidersEffect::SaveKey { .. })),
        "a rejected key is never saved: {effects:?}"
    );

    let (mut offline, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut offline,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let _ = step(&mut offline, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(&mut offline, ProvidersInput::SecretSubmitted);
    let _ = step(
        &mut offline,
        ProvidersInput::KeyChecked(Ok(KeyCheck {
            ok: false,
            reachable: false,
            message: "no route".into(),
        })),
    );
    assert!(
        joined(&offline).contains(
            "Could not reach the provider to check the key. Press enter again to save it anyway."
        ),
        "{}",
        joined(&offline)
    );
    let effects = step(&mut offline, ProvidersInput::SecretSubmitted);
    assert!(
        effects.iter().any(
            |effect| matches!(effect, ProvidersEffect::SaveKey { slug } if slug == "openrouter")
        ),
        "the next submit saves unchecked: {effects:?}"
    );

    let (mut dropped, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut dropped,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let _ = step(&mut dropped, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(&mut dropped, ProvidersInput::SecretSubmitted);
    let _ = step(
        &mut dropped,
        ProvidersInput::KeyChecked(Err(Failure::NotSent("socket down".into()))),
    );
    assert!(
        joined(&dropped).contains(
            "Could not check the key (socket down). Press enter again to save it anyway."
        ),
        "{}",
        joined(&dropped)
    );
    let _ = step(&mut dropped, ProvidersInput::SecretEdited { empty: false });
    assert!(
        !joined(&dropped).contains("save it anyway"),
        "an edit clears the warning: {}",
        joined(&dropped)
    );

    let (mut late, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut late,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let _ = step(&mut late, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(&mut late, ProvidersInput::SecretSubmitted);
    let effects = step(&mut late, ProvidersInput::Key(ProvidersKey::Esc));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ProvidersEffect::ForgetSecret)),
        "escape forgets the secret: {effects:?}"
    );
    assert!(screen(&late).card.is_none());
    let effects = step(
        &mut late,
        ProvidersInput::KeyChecked(Ok(KeyCheck {
            ok: true,
            reachable: true,
            message: String::new(),
        })),
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, ProvidersEffect::SaveKey { .. })),
        "a check that answers after escape is dropped: {effects:?}"
    );

    let (mut saved_late, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut saved_late,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let _ = step(&mut saved_late, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(&mut saved_late, ProvidersInput::SecretSubmitted);
    let _ = step(
        &mut saved_late,
        ProvidersInput::KeyChecked(Ok(KeyCheck {
            ok: true,
            reachable: true,
            message: String::new(),
        })),
    );
    let _ = step(&mut saved_late, ProvidersInput::Key(ProvidersKey::Esc));
    let saved = catalogue(vec![provider("openrouter", "OpenRouter", true, 1)])
        .providers
        .remove(0);
    let effects = step(&mut saved_late, ProvidersInput::KeySaved(Ok(saved)));
    assert!(
        joined(&saved_late).contains("Saved the key for OpenRouter."),
        "a save that answers after escape still applies: {}",
        joined(&saved_late)
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ProvidersEffect::LoadEnv)),
        "{effects:?}"
    );
    let _ = ApiKeyChange::Keep;
}

#[test]
fn save_errors_map_to_text() {
    let cases = [
        (4001, "nope", "Hermes needs both a provider and a key."),
        (
            4002,
            "unknown provider",
            "Hermes does not know the provider openrouter.",
        ),
        (
            4003,
            "oauth",
            "OpenRouter signs in with an account, not a key. Use the Accounts tab.",
        ),
        (4004, "none", "Hermes has no key setting for OpenRouter."),
        (
            4006,
            "managed",
            "This Hermes install is managed by a package manager; its keys are read-only.",
        ),
        (
            5098,
            "old",
            "The gateway runs older code than is installed. Restart the gateway (outside Talaria), then try again.",
        ),
        (
            -32601,
            "missing",
            "This Hermes does not offer model.save_key on the gateway. Use hermes model in a terminal.",
        ),
        (4000, "boom", "Hermes could not save the key: boom."),
    ];
    for (code, message, expect) in cases {
        let (mut state, _) = open(keys_tab(), true);
        let _ = load_env(
            &mut state,
            vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
        );
        let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
        let _ = step(&mut state, ProvidersInput::SecretSubmitted);
        let _ = step(
            &mut state,
            ProvidersInput::KeyChecked(Ok(KeyCheck {
                ok: true,
                reachable: true,
                message: String::new(),
            })),
        );
        let _ = step(
            &mut state,
            ProvidersInput::KeySaved(Err(Failure::Refused {
                code,
                message: message.into(),
            })),
        );
        let text = joined(&state);
        assert!(text.contains(expect), "code {code}: {text}");
        if code != -32601 && code != 4000 {
            assert!(screen(&state).card.is_some(), "code {code} keeps the card");
        }
    }

    let (mut state, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut state,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(&mut state, ProvidersInput::KeySaved(Err(Failure::Unknown)));
    let text = joined(&state);
    assert!(
        text.contains("The connection dropped before Hermes answered. The key may or may not have happened; the list will reload."),
        "{text}"
    );
    assert!(
        screen(&state).card.is_none(),
        "an unknown save closes the card"
    );

    let (mut state, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut state,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", true)],
    );
    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![provider(
            "openrouter",
            "OpenRouter",
            true,
            1,
        )]))),
    );
    let _ = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Remove)),
    );
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char('y')));
    let _ = step(
        &mut state,
        ProvidersInput::Disconnected(Err(Failure::Refused {
            code: 4005,
            message: "no credentials found for openrouter".into(),
        })),
    );
    assert!(
        joined(&state).contains("There were no saved credentials for OpenRouter."),
        "{}",
        joined(&state)
    );
}

#[test]
fn remove_needs_confirmation() {
    let (mut loading, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut loading,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let effects = step(
        &mut loading,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Remove)),
    );
    assert!(
        effects.is_empty()
            || !has(&effects, |effect| matches!(
                effect,
                ProvidersEffect::Disconnect { .. }
            ))
    );
    assert!(
        joined(&loading).contains("Still checking OpenRouter; try again in a moment."),
        "{}",
        joined(&loading)
    );

    let (mut failed, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut failed,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let _ = step(
        &mut failed,
        ProvidersInput::OptionsLoaded(Err(Failure::NotSent("down".into()))),
    );
    let _ = step(
        &mut failed,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Remove)),
    );
    assert!(
        joined(&failed).contains("Could not check OpenRouter. Press ctrl+r, then try again."),
        "{}",
        joined(&failed)
    );

    let (mut empty, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut empty,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let _ = step(
        &mut empty,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![provider(
            "openrouter",
            "OpenRouter",
            false,
            0,
        )]))),
    );
    let effects = step(
        &mut empty,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Remove)),
    );
    assert!(!has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::Disconnect { .. }
    )));
    assert!(
        joined(&empty).contains("Nothing to remove for OpenRouter."),
        "{}",
        joined(&empty)
    );

    let (mut state, _) = open(keys_tab(), true);
    let mut row = env("DASHSCOPE_API_KEY", "qwen", "Qwen Cloud", true);
    row.shared_with = vec![("alibaba".into(), "Alibaba Cloud".into())];
    let _ = load_env(&mut state, vec![row]);
    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![provider("qwen", "Qwen Cloud", true, 2)]))),
    );
    let _ = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Remove)),
    );
    let text = joined(&state);
    assert!(
        text.contains("Remove every saved credential for Qwen Cloud from profile fixture? This deletes its API keys (including DASHSCOPE_API_KEY) and any account sign-in for it."),
        "{text}"
    );
    assert!(
        text.contains("Alibaba Cloud uses DASHSCOPE_API_KEY too and will lose it."),
        "{text}"
    );
    assert!(text.contains("y remove · n keep"), "{text}");
    let effects = step(&mut state, ProvidersInput::Key(ProvidersKey::Char('y')));
    assert!(
        has(
            &effects,
            |effect| matches!(effect, ProvidersEffect::Disconnect { slug } if slug == "qwen")
        ),
        "{effects:?}"
    );
    let effects = step(&mut state, ProvidersInput::Disconnected(Ok(())));
    let text = joined(&state);
    assert!(
        text.contains("Removed the credentials for Qwen Cloud."),
        "{text}"
    );
    assert!(
        has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::LoadEnv
        )),
        "{effects:?}"
    );
    assert!(
        has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::LoadOptions
        )),
        "{effects:?}"
    );
    assert!(
        has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::LoadOauth
        )),
        "{effects:?}"
    );
}

#[test]
fn picker_entry_resolves_in_order() {
    let (mut state, effects) = open(
        OpenTarget::Provider {
            slug: "openrouter".into(),
        },
        true,
    );
    assert!(has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::LoadEnv
    )));
    assert!(has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::LoadOptions
    )));
    assert!(has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::LoadOauth
    )));

    let _ = load_env(
        &mut state,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![provider(
            "openrouter",
            "OpenRouter",
            false,
            0,
        )]))),
    );
    let _ = step(&mut state, ProvidersInput::OauthLoaded(Ok(vec![])));
    let text = joined(&state);
    assert!(
        text.contains("API key for OpenRouter"),
        "a keys row opens key entry: {text}"
    );

    let (mut state, _) = open(
        OpenTarget::Provider {
            slug: "nous".into(),
        },
        true,
    );
    let _ = load_env(&mut state, vec![]);
    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![]))),
    );
    let _ = step(
        &mut state,
        ProvidersInput::OauthLoaded(Ok(vec![support::account(
            "nous",
            "Nous",
            true,
            talaria_core::providers::AccountStatus::NotSignedIn,
        )])),
    );
    assert_eq!(screen(&state).tab, ProvidersTab::Accounts);

    let (mut state, _) = open(
        OpenTarget::Provider {
            slug: "anthropic".into(),
        },
        true,
    );
    let _ = load_env(&mut state, vec![]);
    let mut row = provider("anthropic", "Anthropic", false, 0);
    row["key_env"] = serde_json::json!("ANTHROPIC_API_KEY");
    row["auth_type"] = serde_json::json!("api_key");
    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![row]))),
    );
    let _ = step(&mut state, ProvidersInput::OauthLoaded(Ok(vec![])));
    assert!(
        joined(&state).contains("API key for Anthropic"),
        "{}",
        joined(&state)
    );
    assert!(
        joined(&state).contains("ANTHROPIC_API_KEY"),
        "{}",
        joined(&state)
    );

    let (mut state, _) = open(
        OpenTarget::Provider {
            slug: "copilot".into(),
        },
        true,
    );
    let _ = load_env(&mut state, vec![]);
    let mut row = provider("copilot", "Copilot", false, 0);
    row["auth_type"] = serde_json::json!("external");
    row["warning"] = serde_json::json!("run the vendor tool");
    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![row]))),
    );
    let _ = step(&mut state, ProvidersInput::OauthLoaded(Ok(vec![])));
    assert!(
        joined(&state).contains("Copilot is set up outside Talaria: run the vendor tool."),
        "{}",
        joined(&state)
    );
}

#[test]
fn one_write_at_a_time_and_key_scope() {
    let (mut state, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut state,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![provider(
            "openrouter",
            "OpenRouter",
            false,
            0,
        )]))),
    );
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let first = step(&mut state, ProvidersInput::SecretSubmitted);
    assert!(has(&first, |effect| matches!(
        effect,
        ProvidersEffect::CheckKey { .. }
    )));
    let second = step(&mut state, ProvidersInput::SecretSubmitted);
    assert!(
        second.is_empty(),
        "a second submit while a check is in flight sends nothing: {second:?}"
    );
    assert!(
        joined(&state).contains("Waiting for Hermes."),
        "{}",
        joined(&state)
    );
    let dumped = format!(
        "{:?} {:?}",
        ProvidersInput::SecretSubmitted,
        ProvidersEffect::CheckKey {
            env_var: "OPENROUTER_API_KEY".into()
        }
    );
    assert!(
        !dumped.contains("sk-test-SENTINEL-0123456789abcdef"),
        "core values carry no key text: {dumped}"
    );
    let _ = Instant::now();
}

#[test]
fn open_args_esc_order_and_filter() {
    assert!(matches!(
        talaria_core::providers::parse_providers_args(""),
        Ok(OpenTarget::Tab(ProvidersTab::Keys))
    ));
    assert!(matches!(
        talaria_core::providers::parse_providers_args("keys"),
        Ok(OpenTarget::Tab(ProvidersTab::Keys))
    ));
    assert!(matches!(
        talaria_core::providers::parse_providers_args(" Accounts "),
        Ok(OpenTarget::Tab(ProvidersTab::Accounts))
    ));
    assert!(matches!(
        talaria_core::providers::parse_providers_args("ENDPOINTS"),
        Ok(OpenTarget::Tab(ProvidersTab::Endpoints))
    ));
    for bad in ["keys x", "foo"] {
        assert_eq!(
            talaria_core::providers::parse_providers_args(bad),
            Err("Usage: /providers [keys|accounts|endpoints]")
        );
    }

    let (mut state, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut state,
        vec![
            env("OLLAMA_API_KEY", "ollama-cloud", "Ollama Cloud", true),
            env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false),
        ],
    );
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    assert!(screen(&state).card.is_some(), "enter opens the key card");
    let effects = step(&mut state, ProvidersInput::Key(ProvidersKey::Esc));
    assert!(
        screen(&state).card.is_none(),
        "escape closes the card first"
    );
    assert!(!has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::Close
    )));
    for ch in ['z', 'z', 'z'] {
        let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char(ch)));
    }
    assert_eq!(screen(&state).filter, "zzz");
    assert!(
        joined(&state).contains("No provider matches \"zzz\"."),
        "{}",
        joined(&state)
    );
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Esc));
    assert_eq!(
        screen(&state).filter,
        "",
        "the next escape clears the filter"
    );
    assert!(screen(&state).card.is_none());
    let effects = step(&mut state, ProvidersInput::Key(ProvidersKey::Esc));
    assert!(
        has(&effects, |effect| matches!(effect, ProvidersEffect::Close)),
        "the third escape closes the overlay: {effects:?}"
    );

    let (mut state, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut state,
        vec![env("OLLAMA_API_KEY", "ollama-cloud", "Ollama Cloud", true)],
    );
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char('o')));
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char('l')));
    let text = joined(&state);
    assert!(
        text.contains("Ollama Cloud"),
        "every word is a substring: {text}"
    );
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char(' ')));
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Char('z')));
    assert!(
        joined(&state).contains("No provider matches \"ol z\"."),
        "{}",
        joined(&state)
    );
}

#[test]
fn notices_banner_and_reloads() {
    let (mut state, _) = open(keys_tab(), true);
    let _ = load_env(
        &mut state,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", false)],
    );
    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Ok(catalogue(vec![provider(
            "openrouter",
            "OpenRouter",
            false,
            0,
        )]))),
    );
    let _ = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Remove)),
    );
    let notice = screen(&state).notice.clone();
    let _ = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Reload)),
    );
    assert_eq!(screen(&state).notice, notice, "a reload keeps the notice");
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Tab));
    assert!(
        screen(&state).notice.is_none(),
        "a tab change clears the notice"
    );

    let (mut state, _) = open(keys_tab(), false);
    let text = joined(&state);
    assert!(
        text.contains("Not connected to Hermes. Changes are off until Talaria reconnects."),
        "{text}"
    );
    let _ = load_env(
        &mut state,
        vec![env("OPENROUTER_API_KEY", "openrouter", "OpenRouter", true)],
    );
    let effects = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    assert!(
        !has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::CheckKey { .. }
        )),
        "writes and checks do nothing under the banner: {effects:?}"
    );
    let _ = step(
        &mut state,
        ProvidersInput::OptionsLoaded(Err(Failure::NotSent("not connected".into()))),
    );
    let effects = step(&mut state, ProvidersInput::Connection { connected: true });
    assert!(
        has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::LoadEnv
        )),
        "{effects:?}"
    );
    assert!(
        has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::LoadOptions
        )),
        "{effects:?}"
    );
    assert!(
        has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::LoadOauth
        )),
        "{effects:?}"
    );
    assert!(
        has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::LoadEndpoints
        )),
        "{effects:?}"
    );
    assert!(screen(&state).banner.is_none());

    let (mut state, _) = open(keys_tab(), true);
    let _ = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Reload)),
    );
    let effects = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Reload)),
    );
    let extra = effects
        .iter()
        .filter(|effect| matches!(effect, ProvidersEffect::LoadEnv))
        .count();
    assert_eq!(
        extra, 0,
        "a reload during a load does not start a second one"
    );
    let effects = step(&mut state, ProvidersInput::EnvLoaded(Ok(vec![])));
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, ProvidersEffect::LoadEnv))
            .count(),
        1,
        "the queued reload runs once when the first load answers: {effects:?}"
    );

    let (mut state, _) = open(keys_tab(), true);
    let _ = step(
        &mut state,
        ProvidersInput::EnvLoaded(Err(Failure::Http {
            status: 404,
            detail: None,
        })),
    );
    let text = joined(&state);
    assert!(
        text.contains(
            "Could not load keys: This Hermes has no /api/env. It may be older or newer than Talaria expects."
        ),
        "{text}"
    );
    assert!(text.contains("ctrl+r tries again."), "{text}");
}
