use std::time::Instant;
use talaria_core::mcp::{
    McpEffect, McpInput, McpOpen, McpState, McpTab, ServerRow, StateLabel, parse_mcp_args,
    server_state, update, view,
};

#[test]
fn parse_mcp_args_and_entries() {
    let entry = talaria_core::commands::local::LocalCommandTable::builtin()
        .find("mcp")
        .cloned()
        .unwrap();
    assert!(entry.takes_argument);
    assert!(
        talaria_core::settings::SETTINGS_LINKS
            .iter()
            .any(|link| link.label == "MCP servers"
                && link.action
                    == talaria_core::commands::local::LocalAction::Mcp(McpOpen::Tab(
                        McpTab::Servers
                    )))
    );
    for value in ["", "servers"] {
        assert_eq!(parse_mcp_args(value), Ok(McpOpen::Tab(McpTab::Servers)));
    }
    for value in ["catalogue", "catalog"] {
        assert_eq!(parse_mcp_args(value), Ok(McpOpen::Tab(McpTab::Catalogue)));
    }
    assert_eq!(parse_mcp_args("apply"), Ok(McpOpen::Apply));
    assert_eq!(
        parse_mcp_args("other"),
        Err("Usage: /mcp [servers|catalogue|apply]")
    );
}

#[test]
fn rows_join_list_and_status_by_name() {
    let mut state = McpState::default();
    let now = Instant::now();
    let effects = update(
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
    assert!(
        view(&state, now)
            .text_lines()
            .iter()
            .any(|line| line.contains("Loading…"))
    );
    let list: hermes_contract::schemas::McpServersListResult =
        serde_json::from_str(include_str!("fixtures/mcp/list.json")).unwrap();
    let status: hermes_contract::schemas::McpServersStatusResult =
        serde_json::from_str(include_str!("fixtures/mcp/status.json")).unwrap();
    update(&mut state, McpInput::StatusLoaded(Ok(status.servers)), now);
    update(&mut state, McpInput::ListLoaded(Ok(list.servers)), now);
    let screen = view(&state, now).text_lines();
    assert!(screen.iter().any(|line| line.contains("deepwiki")
        && line.contains("connected")
        && line.contains("3 tools")));
    let mut second = McpState::default();
    update(
        &mut second,
        McpInput::Open {
            open: McpOpen::Tab(McpTab::Servers),
            profile_name: "fixture".into(),
            connected: true,
            over_ssh: false,
            foreground: None,
        },
        now,
    );
    let list: hermes_contract::schemas::McpServersListResult =
        serde_json::from_str(include_str!("fixtures/mcp/list.json")).unwrap();
    update(&mut second, McpInput::ListLoaded(Ok(list.servers)), now);
    update(&mut second, McpInput::StatusLoaded(Ok(vec![])), now);
    assert!(
        view(&second, now)
            .text_lines()
            .iter()
            .any(|line| line.contains("deepwiki") && line.contains("status unknown"))
    );
    let status: hermes_contract::schemas::McpServersStatusResult =
        serde_json::from_str(include_str!("fixtures/mcp/status.json")).unwrap();
    update(&mut second, McpInput::StatusLoaded(Ok(status.servers)), now);
    assert!(
        view(&second, now)
            .text_lines()
            .iter()
            .any(|line| line.contains("deepwiki") && line.contains("connected"))
    );
    let mut listing: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mcp/list.json")).unwrap();
    let mut other = listing["servers"][0].clone();
    other["name"] = serde_json::json!("alpha");
    listing["servers"].as_array_mut().unwrap().push(other);
    let entries: hermes_contract::schemas::McpServersListResult =
        serde_json::from_value(listing.clone()).unwrap();
    update(&mut second, McpInput::ListLoaded(Ok(entries.servers)), now);
    update(
        &mut second,
        McpInput::Key(talaria_core::mcp::McpKey::Down),
        now,
    );
    let current = view(&second, now);
    assert_eq!(current.rows[current.selected.unwrap()].name, "deepwiki");
    listing["servers"].as_array_mut().unwrap().reverse();
    let entries: hermes_contract::schemas::McpServersListResult =
        serde_json::from_value(listing).unwrap();
    update(&mut second, McpInput::ListLoaded(Ok(entries.servers)), now);
    let current = view(&second, now);
    assert_eq!(current.rows[current.selected.unwrap()].name, "deepwiki");
}

#[test]
fn state_label_precedence() {
    let list: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mcp/list.json")).unwrap();
    let status: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mcp/status.json")).unwrap();
    let mut row = ServerRow {
        summary: serde_json::from_value(list["servers"][0].clone()).unwrap(),
        runtime: Some(serde_json::from_value(status["servers"][0].clone()).unwrap()),
    };
    assert!(matches!(server_state(&row, None), StateLabel::Connected(3)));
    row.summary.enabled = false;
    assert!(matches!(server_state(&row, None), StateLabel::Off));
    row.summary.enabled = true;
    row.summary.auth = Some("oauth".into());
    row.summary.oauth_tokens_present = Some(false);
    assert!(matches!(server_state(&row, None), StateLabel::NeedsSignIn));
    row.summary.oauth_tokens_present = Some(true);
    use hermes_contract::schemas::McpRuntimeStatus;
    for (status, matches_expected) in [
        (McpRuntimeStatus::Lazy, 0),
        (McpRuntimeStatus::Connecting, 1),
        (McpRuntimeStatus::Configured, 2),
        (McpRuntimeStatus::Disabled, 2),
    ] {
        row.runtime.as_mut().unwrap().status = status;
        let label = server_state(&row, None);
        assert!(match matches_expected {
            0 => matches!(label, StateLabel::Lazy(3)),
            1 => matches!(label, StateLabel::Connecting),
            _ => matches!(label, StateLabel::ConfiguredNotReported),
        });
    }
    row.runtime.as_mut().unwrap().status = hermes_contract::schemas::McpRuntimeStatus::Failed;
    assert!(matches!(server_state(&row, None), StateLabel::Failed));
    row.runtime = None;
    assert!(matches!(server_state(&row, None), StateLabel::Unknown));
    let mut unknown = status["servers"][0].clone();
    unknown["status"] = serde_json::json!("starting_up");
    row.runtime = serde_json::from_value(unknown).ok();
    assert!(row.runtime.is_none());
    assert!(matches!(server_state(&row, None), StateLabel::Unknown));
}

mod additional {
    use hermes_contract::schemas::{McpServerSummary, McpServersTestResult};
    use serde_json::{Value, json};
    use std::time::{Duration, Instant};
    use talaria_core::mcp::{
        Failure, McpAction, McpEffect, McpInput, McpKey, McpOpen, McpState, McpTab, update, view,
    };

    fn server(name: &str, extra: Value) -> McpServerSummary {
        let mut value = json!({"name":name,"transport":"http","url":"https://example.invalid/mcp",
        "command":null,"args":[],"env":[],"auth":null,"oauth_tokens_present":null,
        "enabled":true,"tools":null,"source":"config","plugin":null});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    }

    fn loaded(rows: Vec<McpServerSummary>, now: Instant) -> McpState {
        let mut state = McpState::default();
        update(
            &mut state,
            McpInput::Open {
                open: McpOpen::Tab(McpTab::Servers),
                profile_name: "fixture".into(),
                connected: true,
                over_ssh: false,
                foreground: Some("chat-1".into()),
            },
            now,
        );
        update(&mut state, McpInput::ListLoaded(Ok(rows)), now);
        update(&mut state, McpInput::StatusLoaded(Ok(vec![])), now);
        state
    }

    fn key(state: &mut McpState, key: McpKey, now: Instant) -> Vec<McpEffect> {
        update(state, McpInput::Key(key), now)
    }

    fn action(state: &mut McpState, action: McpAction, now: Instant) -> Vec<McpEffect> {
        key(state, McpKey::Action(action), now)
    }

    fn text(state: &McpState, now: Instant) -> String {
        view(state, now).text_lines().join("\n")
    }

    fn rpc(code: i64, message: &str) -> Failure {
        Failure::Rpc {
            code,
            message: message.into(),
            reason: None,
            detail: None,
        }
    }

    fn refresh(effects: &[McpEffect]) {
        assert!(effects.iter().any(|e| matches!(e, McpEffect::LoadList)));
        assert!(effects.iter().any(|e| matches!(e, McpEffect::LoadStatus)));
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, McpEffect::Reload { .. }))
        );
    }

    fn tested(
        state: &mut McpState,
        result: Result<McpServersTestResult, Failure>,
        now: Instant,
    ) -> Vec<McpEffect> {
        update(
            state,
            McpInput::Tested {
                name: "alpha".into(),
                result,
            },
            now,
        )
    }

    #[test]
    fn detail_card_summarises_config() {
        let now = Instant::now();
        for (tools, expected) in [
            (Value::Null, "all tools"),
            (json!({"include":["read","search"]}), "only: read"),
            (json!({"include":[]}), "none (the include list is empty)"),
            (json!({"exclude":["write"]}), "all except: write"),
            (
                json!({"include":["read"],"exclude":["write"]}),
                "only: read; except: write",
            ),
            (json!("not a map"), "all tools"),
        ] {
            let multiple_names = tools
                .get("include")
                .and_then(Value::as_array)
                .is_some_and(|names| names.len() > 1);
            let mut state = loaded(
                vec![server(
                    "alpha",
                    json!({"auth":"oauth", "oauth_tokens_present":false,
            "env":["PUBLIC_NAME","ANOTHER_NAME"],"tools":tools}),
                )],
                now,
            );
            key(&mut state, McpKey::Enter, now);
            let visible = text(&state, now);
            for expected in [
                "alpha",
                "remote",
                "from config",
                "URL: https://example.invalid/mcp",
                "PUBLIC_NAME",
                "ANOTHER_NAME",
                "values hidden",
                "OAuth, not signed in",
                expected,
                "hermes -p fixture mcp login alpha --flow device",
            ] {
                assert!(
                    visible.contains(expected),
                    "missing {expected:?} in {visible}"
                );
            }
            if multiple_names {
                assert!(visible.contains("search"));
            }
            let effects = action(&mut state, McpAction::Copy, now);
            assert!(matches!(effects.as_slice(), [McpEffect::CopyText(command)]
            if command == "hermes -p fixture mcp login alpha --flow device"));
            assert!(
                text(&state, now).contains("Copied to the clipboard if your terminal allows it.")
            );
            assert!(action(&mut state, McpAction::NextTab, now).is_empty());
            assert_eq!(view(&state, now).tab, McpTab::Servers);
        }

        let mut state = loaded(
            vec![server(
                "alpha",
                json!({"url":null,"command":"python3",
        "args":["-m","module"],"auth":"header","env":[]}),
            )],
            now,
        );
        action(&mut state, McpAction::Test, now);
        let tools: Vec<Value> = (0..23)
        .map(|n| json!({"name":format!("tool-{n:02}"),"description":format!("description-{n:02}")}))
        .collect();
        tested(
            &mut state,
            Ok(
                serde_json::from_value(json!({"ok":true,"oauth_needed":false,
        "tools":tools,"prompts":2,"resources":3}))
                .unwrap(),
            ),
            now,
        );
        key(&mut state, McpKey::Enter, now);
        let visible = text(&state, now);
        for expected in [
            "local",
            "Command: python3 -m module",
            "Environment variables: none",
            "API key header",
            "test ok, 23 tools",
            "tool-19 — description-19",
            "… and 3 more",
        ] {
            assert!(
                visible.contains(expected),
                "missing {expected:?} in {visible}"
            );
        }
        assert!(!visible.contains("tool-20"));
    }

    #[test]
    fn test_results_map_to_text() {
        let now = Instant::now();
        let cases = [
            (
                json!({"ok":true,"tools":[{"name":"read","description":"Read"}],"prompts":2,"resources":3,"oauth_needed":false}),
                "alpha works: 1 tools, 2 prompts, 3 resources.",
                " · test ok, 1 tools",
            ),
            (
                json!({"ok":false,"tools":[],"oauth_needed":true,"oauth_tokens_present":false}),
                "alpha needs sign-in. ctrl+o signs in.",
                " · test: needs sign-in",
            ),
            (
                json!({"ok":false,"tools":[],"oauth_needed":false,"error":"connection refused"}),
                "alpha did not connect: connection refused",
                " · test failed",
            ),
            (
                json!({"ok":false,"tools":[],"oauth_needed":true,"oauth_tokens_present":true,"error":"credentials rejected"}),
                "alpha did not connect: credentials rejected",
                " · test failed",
            ),
        ];
        for (reply, notice, suffix) in cases {
            let mut state = loaded(vec![server("alpha", json!({}))], now);
            assert!(
                matches!(action(&mut state, McpAction::Test, now).as_slice(),
            [McpEffect::Test { name }] if name == "alpha")
            );
            assert!(action(&mut state, McpAction::Test, now).is_empty());
            assert!(text(&state, now + Duration::from_secs(3)).contains("testing… 3s"));
            tested(
                &mut state,
                Ok(serde_json::from_value(reply).unwrap()),
                now + Duration::from_secs(3),
            );
            assert_eq!(view(&state, now).notice.as_deref(), Some(notice));
            assert!(view(&state, now).rows[0].label.ends_with(suffix));
            assert!(view(&state, now).pending_line.is_none());
        }
        for (failure, expected) in [
            (
                Failure::Unknown,
                "alpha did not answer the test in 60 s. ctrl+t tries again.",
            ),
            (
                rpc(8765, "unlisted failure"),
                "Hermes refused: unlisted failure",
            ),
        ] {
            let mut state = loaded(vec![server("alpha", json!({}))], now);
            action(&mut state, McpAction::Test, now);
            tested(&mut state, Err(failure), now + Duration::from_secs(60));
            assert_eq!(view(&state, now).notice.as_deref(), Some(expected));
        }
        let mut state = loaded(vec![server("alpha", json!({}))], now);
        action(&mut state, McpAction::Test, now);
        refresh(&tested(&mut state, Err(rpc(4064, "gone")), now));
        assert_eq!(
            view(&state, now).notice.as_deref(),
            Some("alpha is no longer configured.")
        );
    }

    #[test]
    fn toggle_and_remove_effects() {
        let now = Instant::now();
        for enabled in [true, false] {
            let mut state = loaded(vec![server("alpha", json!({"enabled":enabled}))], now);
            let before = view(&state, now).rows[0].label.clone();
            assert!(
                matches!(action(&mut state, McpAction::Toggle, now).as_slice(),
            [McpEffect::SetEnabled { name, enabled: target }] if name == "alpha" && *target != enabled)
            );
            assert_eq!(
                view(&state, now).rows[0].label,
                before,
                "toggle must wait for Hermes"
            );
            refresh(&update(
                &mut state,
                McpInput::Toggled {
                    name: "alpha".into(),
                    result: Ok(!enabled),
                },
                now,
            ));
            assert_eq!(
                view(&state, now).rows[0].label,
                if enabled { "off" } else { "status unknown" }
            );
            assert!(view(&state, now).pending_line.is_some());
        }
        for confirm in [McpKey::Char('y'), McpKey::Enter] {
            let mut state = loaded(vec![server("alpha", json!({}))], now);
            assert!(action(&mut state, McpAction::Remove, now).is_empty());
            assert!(text(&state, now).contains("Remove alpha from profile fixture? Its saved key and sign-in stay in the profile's files."));
            assert!(
                matches!(key(&mut state, confirm, now).as_slice(), [McpEffect::Remove { name }] if name == "alpha")
            );
            refresh(&update(
                &mut state,
                McpInput::Removed {
                    name: "alpha".into(),
                    result: Ok(()),
                },
                now,
            ));
            assert_eq!(view(&state, now).notice.as_deref(), Some("Removed alpha."));
            assert!(view(&state, now).pending_line.is_some());
        }
        for cancel in [McpKey::Char('n'), McpKey::Esc] {
            let mut state = loaded(vec![server("alpha", json!({}))], now);
            action(&mut state, McpAction::Remove, now);
            assert!(key(&mut state, cancel, now).is_empty());
            assert!(view(&state, now).card.is_none());
        }
        let mut state = loaded(vec![server("alpha", json!({}))], now);
        action(&mut state, McpAction::Remove, now);
        key(&mut state, McpKey::Char('y'), now);
        refresh(&update(
            &mut state,
            McpInput::Removed {
                name: "alpha".into(),
                result: Err(rpc(4064, "gone")),
            },
            now,
        ));
        assert_eq!(
            view(&state, now).notice.as_deref(),
            Some("alpha was already gone.")
        );
    }

    #[test]
    fn enable_404_tells_gone_from_missing_route() {
        let now = Instant::now();
        for still_present in [true, false] {
            let mut state = loaded(vec![server("alpha", json!({}))], now);
            action(&mut state, McpAction::Toggle, now);
            let effects = update(
                &mut state,
                McpInput::Toggled {
                    name: "alpha".into(),
                    result: Err(Failure::Http {
                        status: 404,
                        detail: None,
                    }),
                },
                now,
            );
            refresh(&effects);
            update(
                &mut state,
                McpInput::ListLoaded(Ok(if still_present {
                    vec![server("alpha", json!({}))]
                } else {
                    vec![]
                })),
                now,
            );
            assert_eq!(
                view(&state, now).notice.as_deref(),
                Some(if still_present {
                    "This Hermes cannot turn MCP servers on or off from Talaria (its web interface answered 404)."
                } else {
                    "alpha is no longer configured."
                })
            );
        }
        for (status, detail, expected) in [
            (
                409,
                None,
                "alpha comes from plugin demo; change it in the plugin's settings.",
            ),
            (400, None, "Hermes says alpha's configuration is malformed."),
            (
                401,
                None,
                "Hermes refused Talaria's sign-in to its web interface. Restart Talaria; if it persists, run the doctor.",
            ),
            (
                403,
                Some("profile mismatch"),
                "Hermes says this profile is not Talaria's to change (profile mismatch).",
            ),
            (503, None, "Hermes's web interface failed (HTTP 503)."),
        ] {
            let mut state = loaded(vec![server("alpha", json!({}))], now);
            action(&mut state, McpAction::Toggle, now);
            if status == 409 {
                update(
                    &mut state,
                    McpInput::ListLoaded(Ok(vec![server(
                        "alpha",
                        json!({"plugin":"demo","source":"plugin"}),
                    )])),
                    now,
                );
            }
            update(
                &mut state,
                McpInput::Toggled {
                    name: "alpha".into(),
                    result: Err(Failure::Http {
                        status,
                        detail: detail.map(str::to_owned),
                    }),
                },
                now,
            );
            assert_eq!(view(&state, now).notice.as_deref(), Some(expected));
        }
    }

    #[test]
    fn plugin_rows_refuse_writes_locally() {
        let now = Instant::now();
        for requested in [
            McpAction::Toggle,
            McpAction::SetKey,
            McpAction::SignIn,
            McpAction::Remove,
        ] {
            let mut state = loaded(
                vec![server(
                    "alpha",
                    json!({"source":"plugin","plugin":"demo","auth":"oauth"}),
                )],
                now,
            );
            assert!(action(&mut state, requested, now).is_empty());
            assert_eq!(
                view(&state, now).notice.as_deref(),
                Some("alpha comes from plugin demo; change it in the plugin's settings.")
            );
            assert!(view(&state, now).card.is_none());
            assert!(view(&state, now).rows[0].dim);
            assert!(view(&state, now).rows[0].label.starts_with("plugin demo ·"));
            assert!(
                matches!(action(&mut state, McpAction::Test, now).as_slice(), [McpEffect::Test { name }] if name == "alpha")
            );
        }
    }

    #[test]
    fn one_write_filter_and_esc() {
        let now = Instant::now();
        let mut state = loaded(
            vec![
                server("alpha", json!({})),
                server("other", json!({"url":"https://elsewhere.test/mcp"})),
            ],
            now,
        );
        action(&mut state, McpAction::Toggle, now);
        for requested in [
            McpAction::Toggle,
            McpAction::SetKey,
            McpAction::SignIn,
            McpAction::Remove,
            McpAction::Add,
            McpAction::Apply,
        ] {
            assert!(action(&mut state, requested, now).is_empty());
            assert!(text(&state, now).contains("Waiting for Hermes."));
        }
        update(
            &mut state,
            McpInput::Toggled {
                name: "alpha".into(),
                result: Ok(false),
            },
            now,
        );
        for c in "ALPHA example".chars() {
            key(&mut state, McpKey::Char(c), now);
        }
        assert_eq!(
            view(&state, now)
                .rows
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha"]
        );
        assert!(
            !key(&mut state, McpKey::Esc, now)
                .iter()
                .any(|e| matches!(e, McpEffect::Close))
        );
        assert_eq!(view(&state, now).filter, "");
        action(&mut state, McpAction::OpenLink, now);
        assert_eq!(view(&state, now).filter, "o");
        action(&mut state, McpAction::NextTab, now);
        assert_eq!(view(&state, now).tab, McpTab::Catalogue);
        assert_eq!(view(&state, now).filter, "");
        assert!(
            key(&mut state, McpKey::Esc, now)
                .iter()
                .any(|e| matches!(e, McpEffect::Close))
        );
    }

    #[test]
    fn not_connected_and_unknown_outcome() {
        let now = Instant::now();
        let mut state = loaded(vec![server("alpha", json!({}))], now);
        update(&mut state, McpInput::Connection { connected: false }, now);
        assert_eq!(
            view(&state, now).banner.as_deref(),
            Some("Not connected to Hermes. Changes are off until Talaria reconnects.")
        );
        for requested in [
            McpAction::Toggle,
            McpAction::Remove,
            McpAction::SetKey,
            McpAction::SignIn,
            McpAction::Add,
            McpAction::Apply,
            McpAction::Test,
        ] {
            assert!(action(&mut state, requested, now).is_empty());
        }
        update(&mut state, McpInput::Connection { connected: true }, now);
        action(&mut state, McpAction::Toggle, now);
        let effects = update(
            &mut state,
            McpInput::Toggled {
                name: "alpha".into(),
                result: Err(Failure::Unknown),
            },
            now,
        );
        assert!(effects.is_empty());
        assert_eq!(
            view(&state, now).notice.as_deref(),
            Some(
                "The change may or may not have been saved. Talaria will reload the list when it reconnects."
            )
        );
        update(&mut state, McpInput::Connection { connected: false }, now);
        let effects = update(&mut state, McpInput::Connection { connected: true }, now);
        refresh(&effects);
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, McpEffect::SetEnabled { .. }))
        );

        action(&mut state, McpAction::Test, now);
        update(&mut state, McpInput::Close, now);
        tested(
        &mut state,
        Ok(serde_json::from_value(
            json!({"ok":true,"tools":[{"name":"read","description":"Read"}],"oauth_needed":false}),
        )
        .unwrap()),
        now,
    );
        assert!(view(&state, now).notice.is_none());
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
        assert!(
            view(&state, now).rows[0]
                .label
                .ends_with(" · test ok, 1 tools")
        );
    }
}
