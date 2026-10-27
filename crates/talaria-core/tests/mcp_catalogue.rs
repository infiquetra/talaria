use talaria_core::mcp::{TransportKind, transport_kind};

#[test]
fn rows_transport_kind_and_install() {
    for input in [
        "http",
        "TransportSpec(type='http', command=None, args=[], url='https://example.invalid/mcp', version=None, env={})",
    ] {
        assert!(matches!(transport_kind(input), TransportKind::Remote));
    }
    for input in ["stdio", "TransportSpec(type='stdio', ...)"] {
        assert!(matches!(transport_kind(input), TransportKind::Local));
    }
    assert!(matches!(transport_kind("weird"), TransportKind::Unknown));
    use std::time::Instant;
    use talaria_core::mcp::{McpEffect, McpInput, McpKey, McpOpen, McpState, McpTab, update, view};
    let now = Instant::now();
    let mut state = McpState::default();
    update(
        &mut state,
        McpInput::Open {
            open: McpOpen::Tab(McpTab::Catalogue),
            profile_name: "fixture".into(),
            connected: true,
            over_ssh: false,
            foreground: None,
        },
        now,
    );
    let fixture: hermes_contract::schemas::McpCatalogResult =
        serde_json::from_str(include_str!("fixtures/mcp/catalog.json")).unwrap();
    update(
        &mut state,
        McpInput::CatalogLoaded(Ok(fixture.servers)),
        now,
    );
    assert!(
        view(&state, now)
            .text_lines()
            .iter()
            .all(|line| !line.contains("TransportSpec") && !line.contains("example.invalid"))
    );
    update(&mut state, McpInput::Key(McpKey::Enter), now);
    let effects = update(&mut state, McpInput::Key(McpKey::Enter), now);
    assert!(effects.iter().any(|effect| matches!(effect, McpEffect::AddServer { name, preset:Some(preset), .. } if name == "deepwiki" && preset == "deepwiki")));
}

mod additional {
    use hermes_contract::schemas::{McpCatalogEntry, McpServerSummary};
    use std::time::Instant;
    use talaria_core::mcp::{
        Failure, McpAction, McpEffect, McpInput, McpKey, McpOpen, McpState, McpTab, update, view,
    };

    fn catalogue(now: Instant) -> McpState {
        let mut state = McpState::default();
        update(
            &mut state,
            McpInput::Open {
                open: McpOpen::Tab(McpTab::Catalogue),
                profile_name: "fixture".into(),
                connected: true,
                over_ssh: false,
                foreground: Some("chat".into()),
            },
            now,
        );
        let frame: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/mcp/catalog.json")).unwrap();
        update(
            &mut state,
            McpInput::CatalogLoaded(Ok(serde_json::from_value(frame["servers"].clone()).unwrap())),
            now,
        );
        state
    }

    fn server(name: &str, oauth: bool) -> McpServerSummary {
        let frame: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/mcp/list.json")).unwrap();
        let mut row = frame["servers"][0].clone();
        row["name"] = name.into();
        if oauth {
            row["auth"] = "oauth".into();
            row["oauth_tokens_present"] = false.into();
        }
        serde_json::from_value(row).unwrap()
    }

    fn key(state: &mut McpState, key: McpKey, now: Instant) -> Vec<McpEffect> {
        update(state, McpInput::Key(key), now)
    }

    fn contains(state: &McpState, now: Instant, expected: &str) {
        assert!(
            view(state, now)
                .text_lines()
                .iter()
                .any(|line| line.contains(expected)),
            "missing {expected}"
        );
    }

    fn assert_no_reload(effects: &[McpEffect]) {
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, McpEffect::Reload { .. }))
        );
    }

    fn begin_install(state: &mut McpState, now: Instant) {
        assert!(key(state, McpKey::Enter, now).is_empty());
        contains(state, now, "Install deepwiki?");
        contains(
            state,
            now,
            "After install it is on for new sessions of profile fixture.",
        );
        let effects = key(state, McpKey::Enter, now);
        assert_eq!(
            effects
                .iter()
                .filter(|effect| matches!(effect,
        McpEffect::AddServer { name, preset: Some(preset), config: None, token: false }
        if name == "deepwiki" && preset == "deepwiki"))
                .count(),
            1
        );
        assert_no_reload(&effects);
    }

    #[test]
    fn after_install_signs_in_or_tests() {
        let now = Instant::now();
        for oauth in [false, true] {
            let mut state = catalogue(now);
            key(&mut state, McpKey::Paste("deepwiki".into()), now);
            begin_install(&mut state, now);
            let effects = update(
                &mut state,
                McpInput::Added {
                    name: "deepwiki".into(),
                    result: Ok(server("deepwiki", oauth)),
                },
                now,
            );
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, McpEffect::LoadList))
            );
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, McpEffect::LoadStatus))
            );
            assert_no_reload(&effects);
            assert_eq!(view(&state, now).tab, McpTab::Servers);
            assert_eq!(view(&state, now).filter, "");
            assert!(view(&state, now).pending_line.is_some());
            let effects = update(
                &mut state,
                McpInput::ListLoaded(Ok(vec![
                    server("aaa", false),
                    server("deepwiki", oauth),
                    server("zzz", false),
                ])),
                now,
            );
            let screen = view(&state, now);
            assert_eq!(screen.rows[screen.selected.unwrap()].name, "deepwiki");
            assert_no_reload(&effects);
            if oauth {
                contains(
                    &state,
                    now,
                    "deepwiki needs you to sign in. Sign in now? y · n",
                );
                assert!(!effects.iter().any(|effect| matches!(
                    effect,
                    McpEffect::StartSignIn { .. } | McpEffect::Test { .. }
                )));
                let effects = key(&mut state, McpKey::Char('y'), now);
                assert!(effects.iter().any(
                    |effect| matches!(effect, McpEffect::StartSignIn { name } if name == "deepwiki")
                ));
            } else {
                assert!(effects.iter().any(
                    |effect| matches!(effect, McpEffect::Test { name } if name == "deepwiki")
                ));
                let failure = || {
                    serde_json::from_value(serde_json::json!({
                        "ok": false, "tools": [], "error": "missing setup", "oauth_needed": false,
                    }))
                    .unwrap()
                };
                update(
                    &mut state,
                    McpInput::Tested {
                        name: "deepwiki".into(),
                        result: Ok(failure()),
                    },
                    now,
                );
                contains(
                    &state,
                    now,
                    "Some catalogue servers need values Talaria cannot send yet.",
                );
                contains(
                    &state,
                    now,
                    "hermes -p fixture mcp install deepwiki asks for them.",
                );
                key(&mut state, McpKey::Action(McpAction::Test), now);
                update(
                    &mut state,
                    McpInput::Tested {
                        name: "deepwiki".into(),
                        result: Ok(failure()),
                    },
                    now,
                );
                assert!(
                    !view(&state, now)
                        .text_lines()
                        .iter()
                        .any(|line| line.contains("Some catalogue servers"))
                );
                let effects = key(&mut state, McpKey::Action(McpAction::NextTab), now);
                assert!(
                    effects
                        .iter()
                        .any(|effect| matches!(effect, McpEffect::LoadCatalog))
                );
            }
        }

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
        update(&mut state, McpInput::ListLoaded(Ok(vec![])), now);
        key(&mut state, McpKey::Action(McpAction::Add), now);
        key(&mut state, McpKey::Paste("custom-server".into()), now);
        key(&mut state, McpKey::Tab, now);
        key(&mut state, McpKey::Tab, now);
        key(
            &mut state,
            McpKey::Paste("https://example.invalid/custom".into()),
            now,
        );
        let effects = key(&mut state, McpKey::Action(McpAction::Save), now);
        assert!(effects.iter().any(|effect| matches!(effect, McpEffect::AddServer { name, preset: None, .. } if name == "custom-server")));
        update(
            &mut state,
            McpInput::Added {
                name: "custom-server".into(),
                result: Ok(server("custom-server", false)),
            },
            now,
        );
        update(
            &mut state,
            McpInput::ListLoaded(Ok(vec![server("custom-server", false)])),
            now,
        );
        update(
            &mut state,
            McpInput::Tested {
                name: "custom-server".into(),
                result: Ok(serde_json::from_value(serde_json::json!({
                    "ok": false, "tools": [], "error": "missing setup", "oauth_needed": false,
                }))
                .unwrap()),
            },
            now,
        );
        assert!(
            !view(&state, now)
                .text_lines()
                .iter()
                .any(|line| line.contains("Some catalogue servers"))
        );

        let mut state = catalogue(now);
        let mut installed: McpCatalogEntry = serde_json::from_value(
            serde_json::from_str::<serde_json::Value>(include_str!("fixtures/mcp/catalog.json"))
                .unwrap()["servers"][0]
                .clone(),
        )
        .unwrap();
        installed.installed = true;
        update(
            &mut state,
            McpInput::CatalogLoaded(Ok(vec![installed])),
            now,
        );
        update(
            &mut state,
            McpInput::ListLoaded(Ok(vec![server("deepwiki", false)])),
            now,
        );
        let effects = key(&mut state, McpKey::Enter, now);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, McpEffect::AddServer { .. }))
        );
        let screen = view(&state, now);
        assert_eq!(screen.tab, McpTab::Servers);
        assert_eq!(screen.rows[screen.selected.unwrap()].name, "deepwiki");

        let mut state = catalogue(now);
        begin_install(&mut state, now);
        update(
            &mut state,
            McpInput::Added {
                name: "deepwiki".into(),
                result: Ok(server("deepwiki", false)),
            },
            now,
        );
        update(&mut state, McpInput::ListLoaded(Ok(vec![])), now);
        contains(
            &state,
            now,
            "deepwiki is installed but not listed yet. ctrl+r reloads.",
        );
    }

    #[test]
    fn requires_blocks_install() {
        let now = Instant::now();
        let mut state = catalogue(now);
        let mut entry: McpCatalogEntry = serde_json::from_value(
            serde_json::from_str::<serde_json::Value>(include_str!("fixtures/mcp/catalog.json"))
                .unwrap()["servers"][0]
                .clone(),
        )
        .unwrap();
        entry.requires = vec!["SERVICE_URL".into(), "SERVICE_KEY".into()];
        update(&mut state, McpInput::CatalogLoaded(Ok(vec![entry])), now);
        let effects = key(&mut state, McpKey::Enter, now);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, McpEffect::AddServer { .. }))
        );
        contains(
            &state,
            now,
            "deepwiki needs values Hermes asks for at install:",
        );
        contains(&state, now, "SERVICE_URL");
        contains(&state, now, "SERVICE_KEY");
        contains(
            &state,
            now,
            "Run hermes -p fixture mcp install deepwiki in a terminal.",
        );
        let effects = key(&mut state, McpKey::Action(McpAction::Copy), now);
        assert!(effects.iter().any(|effect| matches!(effect, McpEffect::CopyText(command) if command == "hermes -p fixture mcp install deepwiki")));
        contains(
            &state,
            now,
            "Copied to the clipboard if your terminal allows it.",
        );
        for input in [McpKey::Enter, McpKey::Action(McpAction::Save)] {
            let effects = key(&mut state, input, now);
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, McpEffect::AddServer { .. }))
            );
        }
    }

    #[test]
    fn install_errors_map_to_text() {
        let now = Instant::now();
        for (code, message, expected, refresh) in [
            (
                4090,
                "already exists",
                "deepwiki is already installed.",
                true,
            ),
            (
                4063,
                "unknown preset",
                "Hermes no longer offers deepwiki. ctrl+r reloads the catalogue.",
                false,
            ),
            (
                5024,
                "catalogue unavailable",
                "Could not install deepwiki: catalogue unavailable",
                false,
            ),
            (
                -32601,
                "method not found",
                "Hermes did not answer mcp.servers.add on this connection.",
                false,
            ),
        ] {
            let mut state = catalogue(now);
            begin_install(&mut state, now);
            let effects = update(
                &mut state,
                McpInput::Added {
                    name: "deepwiki".into(),
                    result: Err(Failure::Rpc {
                        code,
                        message: message.into(),
                        reason: None,
                        detail: None,
                    }),
                },
                now,
            );
            contains(&state, now, expected);
            assert_no_reload(&effects);
            assert!(!effects.iter().any(|effect| matches!(
                effect,
                McpEffect::StartSignIn { .. } | McpEffect::Test { .. }
            )));
            if refresh {
                assert!(
                    effects.iter().any(|effect| matches!(
                        effect,
                        McpEffect::LoadList | McpEffect::LoadCatalog
                    ))
                );
            }
        }

        let mut state = catalogue(now);
        begin_install(&mut state, now);
        update(
            &mut state,
            McpInput::Added {
                name: "deepwiki".into(),
                result: Ok(server("deepwiki", true)),
            },
            now,
        );
        update(
            &mut state,
            McpInput::ListLoaded(Ok(vec![server("deepwiki", true)])),
            now,
        );
        key(&mut state, McpKey::Char('y'), now);
        update(
            &mut state,
            McpInput::SignInStarted {
                name: "deepwiki".into(),
                result: Err(Failure::Rpc {
                    code: 5024,
                    message: "missing setup".into(),
                    reason: None,
                    detail: None,
                }),
            },
            now,
        );
        contains(
            &state,
            now,
            "Some catalogue servers need values Talaria cannot send yet.",
        );
        contains(
            &state,
            now,
            "hermes -p fixture mcp install deepwiki asks for them.",
        );
    }
}
