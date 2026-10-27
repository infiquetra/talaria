use talaria_core::mcp::{NameError, env_key_for_server, validate_server_name};

#[test]
fn name_rules() {
    let existing = vec!["taken".to_string()];
    assert!(matches!(
        validate_server_name("", &existing),
        Err(NameError::Empty)
    ));
    assert!(matches!(
        validate_server_name(&"a".repeat(65), &existing),
        Err(NameError::Invalid)
    ));
    for invalid in ["a b", "a:b"] {
        assert!(matches!(
            validate_server_name(invalid, &existing),
            Err(NameError::Invalid)
        ));
    }
    assert!(matches!(
        validate_server_name("taken", &existing),
        Err(NameError::Taken)
    ));
    assert_eq!(
        validate_server_name("a.b-c_1", &existing).ok().as_deref(),
        Some("a.b-c_1")
    );
}

#[test]
fn set_key_rules() {
    use std::time::Instant;
    use talaria_core::mcp::{
        McpAction, McpEffect, McpInput, McpKey, McpOpen, McpState, McpTab, SecretSlot, update, view,
    };
    for (name, expected) in [
        ("deepwiki", "MCP_DEEPWIKI_API_KEY"),
        ("my.srv", "MCP_MY_SRV_API_KEY"),
        ("my-srv.1", "MCP_MY_SRV_1_API_KEY"),
        ("__x__", "MCP_X_API_KEY"),
        ("a b", "MCP_A_B_API_KEY"),
    ] {
        assert_eq!(env_key_for_server(name), expected);
    }
    let now = Instant::now();
    for (url, command, expected_env) in [
        (Some("https://example.invalid/mcp"), None, None),
        (None, Some("server"), Some("MCP_EXAMPLE_API_KEY")),
    ] {
        let row = serde_json::from_value(serde_json::json!({"name":"example", "transport":"http", "url":url, "command":command, "args":[], "env":[], "auth":null, "oauth_tokens_present":null, "enabled":true, "tools":null, "source":"config", "plugin":null})).unwrap();
        let mut state = McpState::default();
        update(
            &mut state,
            McpInput::Open {
                open: McpOpen::Tab(McpTab::Servers),
                profile_name: "fixture".into(),
                connected: true,
                over_ssh: false,
                foreground: None,
            },
            now,
        );
        update(&mut state, McpInput::ListLoaded(Ok(vec![row])), now);
        update(&mut state, McpInput::StatusLoaded(Ok(vec![])), now);
        update(
            &mut state,
            McpInput::Key(McpKey::Action(McpAction::SetKey)),
            now,
        );
        if expected_env.is_some() {
            update(&mut state, McpInput::Key(McpKey::Enter), now);
        }
        assert!(
            update(
                &mut state,
                McpInput::SecretSubmitted {
                    slot: SecretSlot::KeyValue
                },
                now
            )
            .iter()
            .all(|effect| !matches!(effect, McpEffect::SetKey { .. }))
        );
        update(
            &mut state,
            McpInput::SecretEdited {
                slot: SecretSlot::KeyValue,
                empty: false,
            },
            now,
        );
        let effects = update(
            &mut state,
            McpInput::SecretSubmitted {
                slot: SecretSlot::KeyValue,
            },
            now,
        );
        assert!(effects.iter().any(|effect| matches!(effect, McpEffect::SetKey { name, env_var, slot: SecretSlot::KeyValue } if name == "example" && env_var.as_deref() == expected_env)));
        assert!(
            view(&state, now)
                .text_lines()
                .iter()
                .all(|line| !line.contains("sentinel"))
        );
    }
}

mod additional {
    use hermes_contract::schemas::{McpServerSummary, McpServersTestResult};
    use serde_json::{Value, json};
    use std::time::Instant;
    use talaria_core::mcp::{
        Failure, McpAction, McpEffect, McpInput, McpKey, McpOpen, McpState, McpTab, SecretSlot,
        update, view,
    };

    fn server(name: &str, oauth: bool) -> McpServerSummary {
        serde_json::from_value(
            json!({"name":name,"transport":"http","url":"https://example.invalid/mcp",
        "args":[],"env":[],"auth":if oauth {Some("oauth")}else{None},
        "oauth_tokens_present":false,"enabled":true,"source":"config"}),
        )
        .unwrap()
    }

    fn opened(now: Instant) -> McpState {
        let mut state = McpState::default();
        update(
            &mut state,
            McpInput::Open {
                open: McpOpen::Tab(McpTab::Servers),
                profile_name: "fixture".into(),
                connected: true,
                over_ssh: false,
                foreground: None,
            },
            now,
        );
        update(
            &mut state,
            McpInput::ListLoaded(Ok(vec![server("taken", false)])),
            now,
        );
        update(&mut state, McpInput::StatusLoaded(Ok(vec![])), now);
        action(&mut state, McpAction::Add, now);
        state
    }

    fn key(state: &mut McpState, key: McpKey, now: Instant) -> Vec<McpEffect> {
        update(state, McpInput::Key(key), now)
    }

    fn action(state: &mut McpState, action: McpAction, now: Instant) -> Vec<McpEffect> {
        key(state, McpKey::Action(action), now)
    }

    fn type_text(state: &mut McpState, text: &str, now: Instant) {
        for c in text.chars() {
            key(state, McpKey::Char(c), now);
        }
    }

    fn tab(state: &mut McpState, now: Instant) {
        key(state, McpKey::Tab, now);
    }

    fn secret(state: &mut McpState, slot: SecretSlot, empty: bool, now: Instant) {
        update(state, McpInput::SecretEdited { slot, empty }, now);
    }

    fn text(state: &McpState, now: Instant) -> String {
        view(state, now).text_lines().join("\n")
    }

    fn remote(name: &str, url: &str, auth_right: usize, now: Instant) -> McpState {
        let mut state = opened(now);
        type_text(&mut state, name, now);
        tab(&mut state, now); // Kind.
        tab(&mut state, now); // URL.
        type_text(&mut state, url, now);
        tab(&mut state, now); // Authentication.
        for _ in 0..auth_right {
            key(&mut state, McpKey::Right, now);
        }
        state
    }

    fn local(name: &str, command: &str, args: &str, now: Instant) -> McpState {
        let mut state = opened(now);
        type_text(&mut state, name, now);
        tab(&mut state, now); // Kind.
        key(&mut state, McpKey::Right, now);
        tab(&mut state, now); // Command.
        type_text(&mut state, command, now);
        tab(&mut state, now); // Arguments.
        type_text(&mut state, args, now);
        tab(&mut state, now); // First environment name.
        state
    }

    fn env_row(state: &mut McpState, index: u8, name: &str, empty: bool, now: Instant) {
        type_text(state, name, now);
        tab(state, now);
        secret(state, SecretSlot::EnvValue(index), empty, now);
        tab(state, now);
    }

    fn add_effect(
        effects: &[McpEffect],
        expected_name: &str,
        expected_config: Value,
        token_expected: bool,
    ) {
        let adds: Vec<_> = effects
            .iter()
            .filter_map(|effect| match effect {
                McpEffect::AddServer {
                    name,
                    preset,
                    config,
                    token,
                } => Some((name, preset, config, token)),
                _ => None,
            })
            .collect();
        assert_eq!(adds.len(), 1, "exactly one add request");
        let (name, preset, config, token) = adds[0];
        assert_eq!(name, expected_name);
        assert!(preset.is_none());
        assert_eq!(
            config.as_ref().unwrap(),
            expected_config.as_object().unwrap()
        );
        assert_eq!(*token, token_expected);
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, McpEffect::Reload { .. } | McpEffect::SetKey { .. }))
        );
    }

    fn assert_blocked(state: &mut McpState, expected: &str, now: Instant) {
        assert!(
            action(state, McpAction::Save, now).is_empty(),
            "invalid form must send nothing"
        );
        assert!(
            text(state, now).contains(expected),
            "missing {expected:?} in {}",
            text(state, now)
        );
        assert!(
            view(state, now)
                .card
                .as_ref()
                .unwrap()
                .fields
                .iter()
                .any(|f| f.focused && f.error.is_some())
        );
    }

    #[test]
    fn field_rules_and_texts() {
        let now = Instant::now();
        let mut defaults = opened(now);
        let fields = &view(&defaults, now).card.unwrap().fields;
        assert_eq!(
            fields.iter().find(|f| f.label == "Kind").unwrap().text,
            "remote"
        );
        assert_eq!(
            fields
                .iter()
                .find(|f| f.label == "Authentication")
                .unwrap()
                .text,
            "none"
        );
        assert!(fields.iter().find(|f| f.label == "Name").unwrap().focused);
        action(&mut defaults, McpAction::OpenLink, now);
        assert_eq!(view(&defaults, now).card.unwrap().fields[0].text, "o");
        key(&mut defaults, McpKey::Enter, now);
        assert!(
            view(&defaults, now)
                .card
                .unwrap()
                .fields
                .iter()
                .any(|f| f.label == "Kind" && f.focused)
        );
        key(&mut defaults, McpKey::Down, now);
        assert!(
            view(&defaults, now)
                .card
                .unwrap()
                .fields
                .iter()
                .any(|f| f.label == "URL" && f.focused)
        );

        for (name, expected) in [
            ("", "Name is required."),
            (
                "bad name!",
                "Use 1 to 64 letters, digits, dot, dash or underscore.",
            ),
            ("taken", "A server named taken already exists."),
        ] {
            let mut state = remote(name, "https://example.invalid/mcp", 0, now);
            assert_blocked(&mut state, expected, now);
        }
        for (url, expected) in [
            ("", "URL is required."),
            (
                "ftp://example.com",
                "Enter an http:// or https:// address with a host.",
            ),
            (
                "https:///",
                "Enter an http:// or https:// address with a host.",
            ),
            (
                "https://user:password@example.com/mcp",
                "Put credentials in the API key field, not the address.",
            ),
        ] {
            let mut state = remote("fresh", url, 0, now);
            assert_blocked(&mut state, expected, now);
        }
        for (url, warning) in [
            (
                "https://example.invalid/mcp?query=yes",
                "The address is saved in config.yaml as typed; keep keys out of it.",
            ),
            ("http://example.com/mcp", "This address is not encrypted."),
        ] {
            let mut state = remote("fresh", url, 0, now);
            assert!(text(&state, now).contains(warning));
            add_effect(
                &action(&mut state, McpAction::Save, now),
                "fresh",
                json!({"url":url}),
                false,
            );
        }
        for url in [
            "http://127.0.0.1:9/mcp",
            "http://localhost:9/mcp",
            "http://[::1]:9/mcp",
        ] {
            let mut state = remote("fresh", url, 0, now);
            assert!(!text(&state, now).contains("This address is not encrypted."));
            add_effect(
                &action(&mut state, McpAction::Save, now),
                "fresh",
                json!({"url":url}),
                false,
            );
        }
        let mut state = remote("fresh", "https://example.invalid/mcp", 1, now);
        assert_blocked(&mut state, "Enter the token, or choose none.", now);
        let mut state = local("fresh", "", "", now);
        assert_blocked(&mut state, "Command is required.", now);
        let mut state = local("fresh", "python3", "'unclosed", now);
        assert_blocked(&mut state, "Arguments have an unclosed quote.", now);
        let mut state = local("fresh", "python3", "", now);
        env_row(&mut state, 0, "9INVALID", false, now);
        assert_blocked(
            &mut state,
            "Use letters, digits and underscore; do not start with a digit.",
            now,
        );
        let mut state = local("fresh", "python3", "", now);
        env_row(&mut state, 0, "DUPLICATE", false, now);
        env_row(&mut state, 1, "DUPLICATE", false, now);
        assert_blocked(&mut state, "DUPLICATE is already listed.", now);
        let mut state = local("fresh", "python3", "", now);
        env_row(&mut state, 0, "EMPTY_VALUE", true, now);
        assert_blocked(&mut state, "Enter a value, or clear the name.", now);
        let mut state = local("fresh", "python3", "", now);
        assert!(text(&state, now).contains(
        "This runs a program on this machine whenever Hermes connects. Add it only if you trust it."
    ));
        add_effect(
            &action(&mut state, McpAction::Save, now),
            "fresh",
            json!({"command":"python3","args":[]}),
            false,
        );
        let mut state = local("fresh", "python3", "", now);
        for i in 0..8 {
            env_row(&mut state, i, &format!("VAR_{i}"), false, now);
        }
        assert!(text(&state, now).contains("Up to 8 variables here; add more later with ctrl+k."));
        assert!(
            !view(&state, now)
                .card
                .unwrap()
                .fields
                .iter()
                .any(|f| f.secret == Some(SecretSlot::EnvValue(8)))
        );

        let mut state = remote("fresh", "https://example.invalid/mcp", 1, now);
        secret(&mut state, SecretSlot::Token, false, now);
        let effects = key(&mut state, McpKey::Left, now);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, McpEffect::ForgetSecrets))
        );
        let mut state = remote("fresh", "https://example.invalid/mcp", 1, now);
        secret(&mut state, SecretSlot::Token, false, now);
        key(&mut state, McpKey::BackTab, now);
        key(&mut state, McpKey::BackTab, now); // Kind.
        assert!(
            key(&mut state, McpKey::Right, now)
                .iter()
                .any(|e| matches!(e, McpEffect::ForgetSecrets))
        );
        let changed = view(&state, now).card.unwrap();
        assert_eq!(
            changed
                .fields
                .iter()
                .find(|field| field.label == "Name")
                .unwrap()
                .text,
            "fresh"
        );
        assert!(
            !changed
                .fields
                .iter()
                .any(|field| field.text.contains("https://example.invalid/mcp"))
        );
        assert_eq!(
            changed
                .fields
                .iter()
                .find(|field| field.label == "Command")
                .unwrap()
                .text,
            ""
        );
        key(&mut state, McpKey::Esc, now);
        assert!(text(&state, now).contains("Discard this server? y · n"));
    }

    #[test]
    fn remote_add_effects() {
        let now = Instant::now();
        for (choice, expected, token) in [
            (0, json!({"url":"https://example.invalid/mcp"}), false),
            (1, json!({"url":"https://example.invalid/mcp"}), true),
            (
                2,
                json!({"url":"https://example.invalid/mcp","auth":"oauth"}),
                false,
            ),
        ] {
            let mut state = remote(" fresh ", " https://example.invalid/mcp ", choice, now);
            if token {
                secret(&mut state, SecretSlot::Token, false, now);
            }
            let effects = action(&mut state, McpAction::Save, now);
            add_effect(&effects, "fresh", expected, token);
            for effect in &effects {
                if matches!(effect, McpEffect::AddServer { .. }) {
                    assert_eq!(
                        effect.secret_slot(),
                        if token { Some(SecretSlot::Token) } else { None }
                    );
                }
            }
        }
        let mut empty = remote("fresh", "https://example.invalid/mcp", 1, now);
        secret(&mut empty, SecretSlot::Token, true, now);
        assert_blocked(&mut empty, "Enter the token, or choose none.", now);
    }

    #[test]
    fn local_add_then_keys_in_order() {
        let now = Instant::now();
        let mut state = local("fresh", " python3 ", "-y \"some server\" --flag", now);
        env_row(&mut state, 0, "FIRST", false, now);
        env_row(&mut state, 1, "SECOND", false, now);
        add_effect(
            &action(&mut state, McpAction::Save, now),
            "fresh",
            json!({"command":"python3","args":["-y","some server","--flag"]}),
            false,
        );
        let effects = update(
            &mut state,
            McpInput::Added {
                name: "fresh".into(),
                result: Ok(server("fresh", false)),
            },
            now,
        );
        assert!(
            matches!(effects.as_slice(), [McpEffect::SetKey { name,env_var:Some(env),slot:SecretSlot::EnvValue(0) }]
        if name == "fresh" && env == "FIRST")
        );
        assert_eq!(effects[0].secret_slot(), Some(SecretSlot::EnvValue(0)));
        let effects = update(
            &mut state,
            McpInput::KeySaved {
                name: "fresh".into(),
                env_var: Some("FIRST".into()),
                result: Ok("FIRST".into()),
            },
            now,
        );
        assert!(
            matches!(effects.as_slice(), [McpEffect::SetKey { name,env_var:Some(env),slot:SecretSlot::EnvValue(1) }]
        if name == "fresh" && env == "SECOND")
        );
        let effects = update(
            &mut state,
            McpInput::KeySaved {
                name: "fresh".into(),
                env_var: Some("SECOND".into()),
                result: Ok("SECOND".into()),
            },
            now,
        );
        for expected in [0, 1, 2] {
            assert!(effects.iter().any(|e| match expected {
                0 => matches!(e, McpEffect::ForgetSecrets),
                1 => matches!(e, McpEffect::LoadList),
                _ => matches!(e, McpEffect::LoadStatus),
            }));
        }
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, McpEffect::SetKey { .. } | McpEffect::Reload { .. }))
        );
        assert!(view(&state, now).card.is_none());
        assert!(view(&state, now).pending_line.is_some());
    }

    #[test]
    fn partial_failure_keeps_server() {
        let now = Instant::now();
        let mut state = local("fresh", "python3", "", now);
        for (i, name) in ["FIRST", "SECOND", "THIRD"].into_iter().enumerate() {
            env_row(&mut state, i as u8, name, false, now);
        }
        action(&mut state, McpAction::Save, now);
        update(
            &mut state,
            McpInput::Added {
                name: "fresh".into(),
                result: Ok(server("fresh", false)),
            },
            now,
        );
        update(
            &mut state,
            McpInput::KeySaved {
                name: "fresh".into(),
                env_var: Some("FIRST".into()),
                result: Ok("FIRST".into()),
            },
            now,
        );
        let effects = update(
            &mut state,
            McpInput::KeySaved {
                name: "fresh".into(),
                env_var: Some("SECOND".into()),
                result: Err(Failure::NotSent("fixture failure".into())),
            },
            now,
        );
        assert!(!effects.iter().any(|e| matches!(
            e,
            McpEffect::SetKey { .. } | McpEffect::Remove { .. } | McpEffect::Reload { .. }
        )));
        assert!(effects.iter().any(|e| matches!(e, McpEffect::LoadList)));
        assert!(effects.iter().any(|e| matches!(e, McpEffect::LoadStatus)));
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, McpEffect::ForgetSecrets))
        );
        let expected = view(&state, now).notice.unwrap();
        assert!(expected.starts_with("Added fresh, but saving SECOND failed:"));
        assert!(expected.ends_with("ctrl+k on the server sets it."));
        let effects = update(
            &mut state,
            McpInput::ListLoaded(Ok(vec![server("fresh", false), server("taken", false)])),
            now,
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e,McpEffect::Test { name } if name == "fresh"))
        );
        let reply: McpServersTestResult = serde_json::from_value(
            json!({"ok":false,"tools":[],"error":"unreachable","oauth_needed":false}),
        )
        .unwrap();
        update(
            &mut state,
            McpInput::Tested {
                name: "fresh".into(),
                result: Ok(reply),
            },
            now,
        );
        assert_eq!(view(&state, now).notice.as_deref(), Some(expected.as_str()));
        assert!(
            view(&state, now)
                .rows
                .iter()
                .any(|r| r.name == "fresh" && r.label.ends_with(" · test failed"))
        );
    }

    #[test]
    fn add_selects_the_new_row() {
        let now = Instant::now();
        for oauth in [false, true] {
            let mut state = remote(
                "middle",
                "https://example.invalid/mcp",
                if oauth { 2 } else { 0 },
                now,
            );
            action(&mut state, McpAction::Save, now);
            let effects = update(
                &mut state,
                McpInput::Added {
                    name: "middle".into(),
                    result: Ok(server("middle", oauth)),
                },
                now,
            );
            assert!(effects.iter().any(|e| matches!(e, McpEffect::LoadList)));
            assert!(effects.iter().any(|e| matches!(e, McpEffect::LoadStatus)));
            assert!(!effects.iter().any(|e| matches!(
                e,
                McpEffect::Reload { .. } | McpEffect::Test { .. } | McpEffect::StartSignIn { .. }
            )));
            let effects = update(
                &mut state,
                McpInput::ListLoaded(Ok(vec![
                    server("zulu", false),
                    server("middle", oauth),
                    server("alpha", false),
                ])),
                now,
            );
            let visible = view(&state, now);
            assert_eq!(visible.rows[visible.selected.unwrap()].name, "middle");
            assert_eq!(visible.selected, Some(1));
            assert!(effects.iter().any(|e| if oauth {
                matches!(e,McpEffect::StartSignIn { name } if name == "middle")
            } else {
                matches!(e,McpEffect::Test { name } if name == "middle")
            }));
            assert!(view(&state, now).pending_line.is_some());
        }
    }
}
