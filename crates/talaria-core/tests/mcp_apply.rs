use hermes_contract::schemas::ReloadMcpResult;
use std::time::Instant;
use talaria_core::mcp::{
    Failure, McpAction, McpEffect, McpInput, McpKey, McpOpen, McpState, McpTab, update, view,
};

fn open(
    state: &mut McpState,
    now: Instant,
    foreground: Option<&str>,
    apply: bool,
) -> Vec<McpEffect> {
    update(
        state,
        McpInput::Open {
            open: if apply {
                McpOpen::Apply
            } else {
                McpOpen::Tab(McpTab::Servers)
            },
            profile_name: "fixture".into(),
            connected: true,
            over_ssh: false,
            foreground: foreground.map(str::to_owned),
        },
        now,
    )
}

fn key(state: &mut McpState, key: McpKey, now: Instant) -> Vec<McpEffect> {
    update(state, McpInput::Key(key), now)
}

fn text(state: &McpState, now: Instant) -> String {
    view(state, now).text_lines().join("\n")
}

fn gate(state: &mut McpState, now: Instant) {
    let reply = serde_json::from_str(include_str!("fixtures/mcp/reload.json")).unwrap();
    let effects = update(state, McpInput::Reloaded(Ok(reply)), now);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, McpEffect::Reload { .. }))
    );
    assert!(text(state, now).contains("Reloading MCP refreshes every chat of this profile."));
    assert!(text(state, now).contains("y apply · n cancel"));
}

fn mark_pending(state: &mut McpState, now: Instant) {
    let frame: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mcp/list.json")).unwrap();
    update(
        state,
        McpInput::ListLoaded(Ok(serde_json::from_value(frame["servers"].clone()).unwrap())),
        now,
    );
    let effects = key(state, McpKey::Action(McpAction::Toggle), now);
    assert!(effects.iter().any(|effect| matches!(effect, McpEffect::SetEnabled { name, enabled: false } if name == "deepwiki")));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, McpEffect::Reload { .. }))
    );
    let effects = update(
        state,
        McpInput::Toggled {
            name: "deepwiki".into(),
            result: Ok(false),
        },
        now,
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, McpEffect::Reload { .. }))
    );
    assert!(text(state, now).contains(
        "Saved for new sessions; this session still uses the old servers · ctrl+y apply"
    ));
}

fn reload(session: Option<&str>, unavailable: bool) -> ReloadMcpResult {
    let mut value = serde_json::json!({"status": "reloaded", "message": "MCP servers reconciled"});
    if let Some(session) = session {
        value["session"] = session.into();
    }
    if unavailable {
        value["unavailable"] = serde_json::json!(["broken-server"]);
    }
    serde_json::from_value(value).unwrap()
}

#[test]
fn apply_asks_then_confirms() {
    let now = Instant::now();
    for foreground in [Some("session-example"), None] {
        for from_command in [false, true] {
            let mut state = McpState::default();
            let effects = open(&mut state, now, foreground, from_command);
            let effects = if from_command {
                effects
            } else {
                assert!(
                    !effects
                        .iter()
                        .any(|effect| matches!(effect, McpEffect::Reload { .. }))
                );
                key(&mut state, McpKey::Action(McpAction::Apply), now)
            };
            assert_eq!(effects.iter().filter(|effect| matches!(effect,
                McpEffect::Reload { session_id, confirm: false } if session_id.as_deref() == foreground)).count(), 1);
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, McpEffect::Reload { confirm: true, .. }))
            );
            assert!(text(&state, now).contains("Asking Hermes…"));
            let effects = key(&mut state, McpKey::Char('y'), now);
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, McpEffect::Reload { confirm: true, .. }))
            );
            gate(&mut state, now);
            update(
                &mut state,
                McpInput::Foreground(Some("new-foreground".into())),
                now,
            );
            let effects = key(&mut state, McpKey::Char('y'), now);
            assert_eq!(effects.iter().filter(|effect| matches!(effect,
                McpEffect::Reload { session_id, confirm: true } if session_id.as_deref() == foreground)).count(), 1);
            assert!(text(&state, now).contains("Reloading MCP servers… (up to 2 minutes)"));
            assert!(
                !key(&mut state, McpKey::Char('y'), now)
                    .iter()
                    .any(|effect| matches!(effect, McpEffect::Reload { .. }))
            );
        }
    }
    for cancel in [McpKey::Char('n'), McpKey::Esc] {
        let mut state = McpState::default();
        open(&mut state, now, Some("session-example"), false);
        mark_pending(&mut state, now);
        key(&mut state, McpKey::Action(McpAction::Apply), now);
        gate(&mut state, now);
        let effects = key(&mut state, cancel, now);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, McpEffect::Reload { .. }))
        );
        assert!(view(&state, now).card.is_none());
        assert!(view(&state, now).pending_line.is_some());
    }
}

#[test]
fn reload_outcomes_map_to_text() {
    let now = Instant::now();
    for (session, expected) in [
        (
            Some("applied"),
            "MCP servers reloaded. This session's tools change from its next turn.",
        ),
        (
            Some("unchanged"),
            "MCP servers reloaded. This session's tools are unchanged.",
        ),
        (
            Some("busy"),
            "MCP servers reloaded. This session is busy: its new tools apply after you apply again once the turn ends, or when it is reopened.",
        ),
        (None, "MCP servers reloaded for profile fixture."),
    ] {
        for skips_gate in [false, true] {
            let mut state = McpState::default();
            open(&mut state, now, Some("session-example"), false);
            mark_pending(&mut state, now);
            key(&mut state, McpKey::Action(McpAction::Apply), now);
            if !skips_gate {
                gate(&mut state, now);
                key(&mut state, McpKey::Char('y'), now);
            }
            let effects = update(
                &mut state,
                McpInput::Reloaded(Ok(reload(session, true))),
                now,
            );
            assert!(text(&state, now).contains(expected));
            assert!(text(&state, now).contains("Did not connect: broken-server."));
            assert!(view(&state, now).pending_line.is_none());
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, McpEffect::Reload { .. }))
            );
        }
    }
    for (reason, detail, expected) in [
        (
            "mcp_servers_unavailable",
            Some("did not connect: broken-server"),
            "Nothing changed: did not connect: broken-server.",
        ),
        (
            "permission_denied",
            None,
            "This connection may not reload MCP servers.",
        ),
        (
            "invalid_params",
            None,
            "Hermes refused Talaria's reload request (invalid_params).",
        ),
    ] {
        let mut state = McpState::default();
        open(&mut state, now, Some("session-example"), false);
        mark_pending(&mut state, now);
        key(&mut state, McpKey::Action(McpAction::Apply), now);
        gate(&mut state, now);
        key(&mut state, McpKey::Char('y'), now);
        let effects = update(
            &mut state,
            McpInput::Reloaded(Err(Failure::Rpc {
                code: 4001,
                message: reason.into(),
                reason: Some(reason.into()),
                detail: detail.map(str::to_owned),
            })),
            now,
        );
        assert!(text(&state, now).contains(expected));
        assert!(view(&state, now).pending_line.is_some());
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, McpEffect::Reload { .. }))
        );
    }
    for failure in [Failure::Unknown, Failure::BadReply] {
        let mut state = McpState::default();
        open(&mut state, now, Some("session-example"), false);
        mark_pending(&mut state, now);
        key(&mut state, McpKey::Action(McpAction::Apply), now);
        gate(&mut state, now);
        key(&mut state, McpKey::Char('y'), now);
        let effects = update(&mut state, McpInput::Reloaded(Err(failure)), now);
        assert!(text(&state, now).contains(
            "Hermes did not confirm the reload; it may or may not have run. ctrl+y asks again."
        ));
        assert!(view(&state, now).pending_line.is_some());
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, McpEffect::Reload { .. }))
        );
    }
    let mut state = McpState::default();
    open(&mut state, now, Some("session-example"), false);
    mark_pending(&mut state, now);
    update(&mut state, McpInput::Close, now);
    assert!(
        text(&state, now).contains("MCP changes are saved. /mcp apply uses them in this session.")
    );
    open(&mut state, now, Some("session-example"), false);
    assert!(view(&state, now).pending_line.is_some());
    key(&mut state, McpKey::Action(McpAction::Apply), now);
    gate(&mut state, now);
    key(&mut state, McpKey::Char('y'), now);
    update(
        &mut state,
        McpInput::Reloaded(Ok(reload(Some("applied"), false))),
        now,
    );
    assert!(view(&state, now).pending_line.is_none());
    update(&mut state, McpInput::Close, now);
    open(&mut state, now, Some("session-example"), false);
    assert!(view(&state, now).pending_line.is_none());
}
