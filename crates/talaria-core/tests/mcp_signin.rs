use hermes_contract::schemas::McpOauthPollResult;
use std::time::{Duration, Instant};
use talaria_core::mcp::{
    Failure, McpAction, McpEffect, McpInput, McpKey, McpOpen, McpState, McpTab, RedirectError,
    SecretSlot, next_deadline, over_ssh, parse_redirect, update, view,
};

const LINK: &str = "https://example.invalid/authorize?redirect_uri=http%3A%2F%2F127.0.0.1%3A43123%2Fcallback&state=public-flow-marker";

fn key(state: &mut McpState, key: McpKey, now: Instant) -> Vec<McpEffect> {
    update(state, McpInput::Key(key), now)
}

fn text(state: &McpState, now: Instant) -> String {
    view(state, now).text_lines().join("\n")
}

fn start(now: Instant, ssh: bool) -> McpState {
    let mut state = McpState::default();
    update(
        &mut state,
        McpInput::Open {
            open: McpOpen::Tab(McpTab::Servers),
            profile_name: "fixture".into(),
            connected: true,
            over_ssh: ssh,
            foreground: Some("foreground-chat".into()),
        },
        now,
    );
    let frame: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mcp/list.json")).unwrap();
    let mut row = frame["servers"][0].clone();
    row["auth"] = "oauth".into();
    row["oauth_tokens_present"] = false.into();
    update(
        &mut state,
        McpInput::ListLoaded(Ok(vec![serde_json::from_value(row).unwrap()])),
        now,
    );
    let effects = key(&mut state, McpKey::Action(McpAction::SignIn), now);
    assert_eq!(
        effects
            .iter()
            .filter(
                |effect| matches!(effect, McpEffect::StartSignIn { name } if name == "deepwiki")
            )
            .count(),
        1
    );
    state
}

fn started(state: &mut McpState, now: Instant, link: &str) -> Vec<McpEffect> {
    update(
        state,
        McpInput::SignInStarted {
            name: "deepwiki".into(),
            result: Ok(("flow-example".into(), link.into())),
        },
        now,
    )
}

fn poll_reply(status: &str, ok: bool) -> McpOauthPollResult {
    let mut reply: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mcp/oauth-poll.json")).unwrap();
    reply["status"] = status.into();
    reply["ok"] = ok.into();
    reply["error_message"] = "approval refused".into();
    if status == "approved" {
        reply["tools"] =
            serde_json::json!([{"name": "lookup", "description": "Look up documentation"}]);
    }
    serde_json::from_value(reply).unwrap()
}

fn polled(
    state: &mut McpState,
    now: Instant,
    result: Result<McpOauthPollResult, Failure>,
) -> Vec<McpEffect> {
    update(
        state,
        McpInput::Polled {
            name: "deepwiki".into(),
            result,
        },
        now,
    )
}

fn polls(effects: &[McpEffect]) -> usize {
    effects.iter().filter(|effect| matches!(effect, McpEffect::Poll { name, flow } if name == "deepwiki" && flow == "flow-example")).count()
}

fn cancels(effects: &[McpEffect]) -> usize {
    effects.iter().filter(|effect| matches!(effect, McpEffect::CancelSignIn { name, flow } if name == "deepwiki" && flow == "flow-example")).count()
}

#[test]
fn happy_path() {
    let now = Instant::now();
    let mut state = start(now, false);
    let arrival = now + Duration::from_secs(7);
    assert_eq!(polls(&started(&mut state, arrival, LINK)), 0);
    assert!(text(&state, arrival).contains("Waiting for approval… 5:00 left"));
    assert!(
        next_deadline(&state).is_some_and(|deadline| deadline <= arrival + Duration::from_secs(1))
    );
    assert_eq!(
        polls(&update(
            &mut state,
            McpInput::Tick,
            arrival + Duration::from_millis(1999)
        )),
        0
    );
    assert_eq!(
        polls(&update(
            &mut state,
            McpInput::Tick,
            arrival + Duration::from_secs(2)
        )),
        1
    );
    assert_eq!(
        polls(&update(
            &mut state,
            McpInput::Tick,
            arrival + Duration::from_secs(4)
        )),
        0
    );
    let answer = arrival + Duration::from_secs(5);
    assert_eq!(
        polls(&polled(&mut state, answer, Ok(poll_reply("pending", true)))),
        0
    );
    assert_eq!(
        polls(&update(
            &mut state,
            McpInput::Tick,
            answer + Duration::from_millis(1999)
        )),
        0
    );
    assert_eq!(
        polls(&update(
            &mut state,
            McpInput::Tick,
            answer + Duration::from_secs(2)
        )),
        1
    );
    let action_time = answer + Duration::from_secs(2);
    let effects = key(&mut state, McpKey::Action(McpAction::OpenLink), action_time);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, McpEffect::OpenUrl(url) if url == LINK))
    );
    update(&mut state, McpInput::OpenFailed, action_time);
    assert!(text(&state, action_time).contains("Could not open a browser. Copy the link with c."));
    let effects = key(&mut state, McpKey::Action(McpAction::Copy), action_time);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, McpEffect::CopyText(url) if url == LINK))
    );
    assert!(text(&state, answer).contains("Copied to the clipboard if your terminal allows it."));
    let effects = polled(
        &mut state,
        answer + Duration::from_secs(3),
        Ok(poll_reply("approved", true)),
    );
    assert!(
        text(&state, answer + Duration::from_secs(3)).contains("Signed in to deepwiki: 1 tools.")
    );
    assert!(view(&state, answer).pending_line.is_some());
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
        !effects
            .iter()
            .any(|effect| matches!(effect, McpEffect::Reload { .. }))
    );
    assert_eq!(
        polls(&update(
            &mut state,
            McpInput::Tick,
            answer + Duration::from_secs(8)
        )),
        0
    );
    assert_eq!(next_deadline(&state), None);
}

#[test]
fn ended_states() {
    let now = Instant::now();
    for (elapsed, expected) in [
        (100, "Sign-in failed: approval refused"),
        (295, "The sign-in timed out."),
    ] {
        let mut state = start(now, false);
        started(&mut state, now, LINK);
        update(&mut state, McpInput::Tick, now + Duration::from_secs(2));
        polled(
            &mut state,
            now + Duration::from_secs(elapsed),
            Ok(poll_reply("error", true)),
        );
        assert!(text(&state, now + Duration::from_secs(elapsed)).contains(expected));
        assert_eq!(
            polls(&update(
                &mut state,
                McpInput::Tick,
                now + Duration::from_secs(359)
            )),
            0
        );
    }

    let mut state = start(now, false);
    started(&mut state, now, LINK);
    for (index, failure) in [
        Err(Failure::Unknown),
        Err(Failure::Rpc {
            code: 5024,
            message: "temporary refusal".into(),
            reason: None,
            detail: None,
        }),
        Ok(poll_reply("pending", false)),
    ]
    .into_iter()
    .enumerate()
    {
        let due = now + Duration::from_secs(2 + index as u64 * 3);
        assert_eq!(polls(&update(&mut state, McpInput::Tick, due)), 1);
        let effects = polled(&mut state, due + Duration::from_secs(1), failure);
        if index < 2 {
            assert_eq!(cancels(&effects), 0);
        } else {
            assert_eq!(cancels(&effects), 1);
            assert!(
                text(&state, due + Duration::from_secs(1)).contains("Lost track of the sign-in:")
            );
        }
    }
    assert_eq!(
        cancels(&update(
            &mut state,
            McpInput::Tick,
            now + Duration::from_secs(30)
        )),
        0
    );
    assert_eq!(next_deadline(&state), None);

    let mut state = start(now, false);
    started(&mut state, now, LINK);
    for (index, failed) in [true, true, false, true, true].into_iter().enumerate() {
        let due = now + Duration::from_secs(2 + index as u64 * 3);
        update(&mut state, McpInput::Tick, due);
        let result = if failed {
            Err(Failure::Unknown)
        } else {
            Ok(poll_reply("pending", true))
        };
        assert_eq!(
            cancels(&polled(&mut state, due + Duration::from_secs(1), result)),
            0
        );
    }
    assert!(!text(&state, now + Duration::from_secs(16)).contains("Lost track"));

    let mut state = start(now, false);
    started(&mut state, now, LINK);
    update(&mut state, McpInput::Tick, now + Duration::from_secs(300));
    assert!(
        text(&state, now + Duration::from_secs(300)).contains("Waiting for Hermes to give up…")
    );
    assert_eq!(
        cancels(&update(
            &mut state,
            McpInput::Tick,
            now + Duration::from_secs(360)
        )),
        1
    );
    assert!(text(&state, now + Duration::from_secs(360)).contains("The sign-in timed out."));
    assert_eq!(
        cancels(&update(
            &mut state,
            McpInput::Tick,
            now + Duration::from_secs(361)
        )),
        0
    );

    for (code, message, expected) in [
        (4001, "unsupported OAuth server", "unsupported OAuth server"),
        (
            5024,
            "already in progress",
            "A sign-in for deepwiki is already running. Try again in a few minutes.",
        ),
        (
            5024,
            "Too many active flows",
            "Hermes has too many sign-ins running. Try again in a few minutes.",
        ),
        (
            5024,
            "metadata unavailable",
            "Could not start sign-in: metadata unavailable",
        ),
    ] {
        let mut state = start(now, false);
        update(
            &mut state,
            McpInput::SignInStarted {
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
        assert!(text(&state, now).contains(expected));
        assert_eq!(next_deadline(&state), None);
    }
}

#[test]
fn cancel_is_sent_once() {
    let now = Instant::now();
    for closed in [false, true] {
        for late in [false, true] {
            let mut state = start(now, false);
            if !late {
                started(&mut state, now, LINK);
            }
            let screen = text(&state, now);
            for tab in [
                McpKey::Tab,
                McpKey::BackTab,
                McpKey::Action(McpAction::NextTab),
                McpKey::Action(McpAction::PrevTab),
            ] {
                assert!(key(&mut state, tab, now).is_empty());
                assert_eq!(text(&state, now), screen);
            }
            let effects = if closed {
                update(&mut state, McpInput::Close, now)
            } else {
                key(&mut state, McpKey::Esc, now)
            };
            assert_eq!(cancels(&effects), usize::from(!late));
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, McpEffect::ForgetSecrets))
            );
            if late {
                assert_eq!(
                    cancels(&started(&mut state, now + Duration::from_secs(1), LINK)),
                    1
                );
                assert!(view(&state, now).card.is_none());
            } else if !closed {
                assert!(text(&state, now).contains("Sign-in cancelled."));
            }
            assert_eq!(
                cancels(&update(
                    &mut state,
                    McpInput::Close,
                    now + Duration::from_secs(2)
                )),
                0
            );
            assert_eq!(
                polls(&update(
                    &mut state,
                    McpInput::Tick,
                    now + Duration::from_secs(20)
                )),
                0
            );
            assert_eq!(next_deadline(&state), None);
        }
    }
}

#[test]
fn paste_and_ssh_rules() {
    for variable in ["SSH_CLIENT", "SSH_CONNECTION", "SSH_TTY"] {
        assert!(over_ssh(
            |name| (name == variable).then(|| "set".to_string())
        ));
        assert!(!over_ssh(|name| (name == variable).then(String::new)));
    }
    assert!(!over_ssh(|_| None));
    let parsed = parse_redirect("http://127.0.0.1:43123/callback?code=test-code&state=test-state&iss=issuer&extra=ignored#fragment").unwrap();
    assert_eq!(parsed.code, Some("test-code"));
    assert_eq!(parsed.state, "test-state");
    assert_eq!(parsed.iss, Some("issuer"));
    assert_eq!(parsed.error, None);
    let parsed =
        parse_redirect("http://localhost:43123/callback?error=access_denied&state=test-state")
            .unwrap();
    assert_eq!(parsed.code, None);
    assert_eq!(parsed.error, Some("access_denied"));
    assert!(parse_redirect("http://[::1]:43123/callback?code=test&state=test").is_ok());
    assert!(matches!(
        parse_redirect("garbage"),
        Err(RedirectError::NotUrl)
    ));
    assert!(matches!(
        parse_redirect("https://127.0.0.1/callback?code=a&state=b"),
        Err(RedirectError::NotHttp)
    ));
    assert!(matches!(
        parse_redirect("http://example.invalid/callback?code=a&state=b"),
        Err(RedirectError::WrongHost)
    ));
    assert!(matches!(
        parse_redirect("http://127.0.0.1/other?code=a&state=b"),
        Err(RedirectError::WrongPath)
    ));
    assert!(matches!(
        parse_redirect("http://127.0.0.1/callback?code=a"),
        Err(RedirectError::NoState)
    ));
    assert!(matches!(
        parse_redirect("http://127.0.0.1/callback?state=b"),
        Err(RedirectError::NoCodeOrError)
    ));

    let now = Instant::now();
    for ssh in [false, true] {
        let mut state = start(now, ssh);
        started(&mut state, now, LINK);
        if ssh {
            assert!(text(&state, now).contains("http://127.0.0.1:43123/callback"));
        }
        key(&mut state, McpKey::Action(McpAction::PasteRedirect), now);
        assert_eq!(view(&state, now).secret_focus, Some(SecretSlot::Paste));
        update(
            &mut state,
            McpInput::SecretEdited {
                slot: SecretSlot::Paste,
                empty: false,
            },
            now,
        );
        let effects = update(
            &mut state,
            McpInput::SecretSubmitted {
                slot: SecretSlot::Paste,
            },
            now,
        );
        assert!(effects.iter().any(|effect| matches!(effect, McpEffect::RelayCallback { name, flow } if name == "deepwiki" && flow == "flow-example")));
        let effects = update(
            &mut state,
            McpInput::CallbackRelayed {
                name: "deepwiki".into(),
                result: Err(Failure::BadPaste(RedirectError::WrongHost)),
            },
            now,
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, McpEffect::ForgetSecrets))
        );
        assert!(text(&state, now).contains("That is not the sign-in address:"));
        assert_eq!(
            polls(&update(
                &mut state,
                McpInput::Tick,
                now + Duration::from_secs(2)
            )),
            1
        );
    }
    for unsafe_link in [
        "http://example.invalid/authorize",
        "javascript:alert(1)",
        "file:///tmp/authorize",
    ] {
        let mut state = start(now, false);
        started(&mut state, now, unsafe_link);
        assert!(
            !key(&mut state, McpKey::Action(McpAction::OpenLink), now)
                .iter()
                .any(|effect| matches!(effect, McpEffect::OpenUrl(_)))
        );
    }
    let mut state = start(now, true);
    started(&mut state, now, "https://example.invalid/authorize");
    assert!(text(&state, now).contains("http://127.0.0.1/callback"));
}
