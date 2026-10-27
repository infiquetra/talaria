//! Providers account sign-in.

#[path = "support/providers.rs"]
mod support;

use std::time::{Duration, Instant};

use support::{account, has, joined, open, screen, step};
use talaria_core::providers::{
    AccountStatus, Failure, OpenTarget, PollReply, PollStatus, ProvidersAction, ProvidersEffect,
    ProvidersInput, ProvidersKey, ProvidersState, ProvidersTab, SignInStart, next_deadline, update,
};

fn accounts() -> OpenTarget {
    OpenTarget::Tab(ProvidersTab::Accounts)
}

fn nous(status: AccountStatus) -> talaria_core::providers::Account {
    let mut row = account("nous", "Nous", true, status);
    row.disconnectable = true;
    row
}

fn start() -> SignInStart {
    SignInStart {
        session_id: "sess-1".into(),
        user_code: "ABCD-1234".into(),
        verification_url: "https://example.invalid/device".into(),
        expires_in: Some(90),
        poll_interval: Some(5),
    }
}

fn feed_accounts(state: &mut ProvidersState, rows: Vec<talaria_core::providers::Account>) {
    let _ = step(state, ProvidersInput::OauthLoaded(Ok(rows)));
}

#[test]
fn device_code_happy_path() {
    let (mut state, effects) = open(accounts(), true);
    assert!(has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::LoadOauth
    )));
    feed_accounts(&mut state, vec![nous(AccountStatus::NotSignedIn)]);
    let effects = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let start_effect = effects.iter().find_map(|effect| match effect {
        ProvidersEffect::StartSignIn { id, attempt } => Some((id.clone(), *attempt)),
        _ => None,
    });
    assert_eq!(start_effect, Some(("nous".into(), 1)), "{effects:?}");

    let began = Instant::now();
    let effects = update(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(start()),
        },
        began,
    );
    let poll = effects.iter().find_map(|effect| match effect {
        ProvidersEffect::Poll {
            id,
            session_id,
            after,
        } => Some((id.clone(), session_id.clone(), *after)),
        _ => None,
    });
    assert_eq!(
        poll,
        Some(("nous".into(), "sess-1".into(), Duration::from_secs(5))),
        "the first poll waits the interval: {effects:?}"
    );
    let text = joined(&state);
    assert!(
        text.contains("1. Open https://example.invalid/device"),
        "{text}"
    );
    assert!(text.contains("2. Enter the code ABCD-1234"), "{text}");
    assert!(text.contains("expires in 01:30"), "{text}");
    assert!(
        text.contains("o open link · c copy code · esc cancel"),
        "{text}"
    );

    let effects = step(
        &mut state,
        ProvidersInput::SignInPolled {
            session_id: "sess-1".into(),
            result: Ok(PollReply {
                status: PollStatus::Pending,
                error_message: None,
                account_email: None,
                model: None,
                retryable: None,
                retry_after: None,
            }),
        },
    );
    assert!(
        has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::Poll { .. }
        )),
        "pending schedules another poll: {effects:?}"
    );
    let effects = step(
        &mut state,
        ProvidersInput::SignInPolled {
            session_id: "sess-1".into(),
            result: Ok(PollReply {
                status: PollStatus::Approved,
                error_message: None,
                account_email: Some("person@example.invalid".into()),
                model: Some("example-model".into()),
                retryable: None,
                retry_after: None,
            }),
        },
    );
    let text = joined(&state);
    assert!(text.contains("Signed in to Nous."), "{text}");
    assert!(text.contains("Account: person@example.invalid."), "{text}");
    assert!(text.contains("Default model: example-model."), "{text}");
    assert!(has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::LoadOauth
    )));
    assert!(has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::LoadEnv
    )));
    assert!(has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::LoadOptions
    )));
}

#[test]
fn ended_states_and_retry() {
    let cases = [
        (PollStatus::Denied, None, "The sign-in was denied.", true),
        (PollStatus::Expired, None, "The code expired.", true),
        (
            PollStatus::Error,
            None,
            "Sign-in failed: no reason given.",
            true,
        ),
        (
            PollStatus::Cancelled,
            None,
            "The sign-in was cancelled.",
            true,
        ),
        (
            PollStatus::Other("weird".into()),
            None,
            "Lost contact with Hermes while waiting. The sign-in may still finish; press ctrl+r on this tab to check.",
            true,
        ),
    ];
    for (status, message, expect, retry) in cases {
        let (mut state, _) = open(accounts(), true);
        feed_accounts(&mut state, vec![nous(AccountStatus::NotSignedIn)]);
        let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
        let _ = step(
            &mut state,
            ProvidersInput::SignInStarted {
                attempt: 1,
                result: Ok(start()),
            },
        );
        if matches!(status, PollStatus::Other(_)) {
            for _ in 0..3 {
                let _ = step(
                    &mut state,
                    ProvidersInput::SignInPolled {
                        session_id: "sess-1".into(),
                        result: Ok(PollReply {
                            status: status.clone(),
                            error_message: message.clone(),
                            account_email: None,
                            model: None,
                            retryable: None,
                            retry_after: None,
                        }),
                    },
                );
            }
        } else {
            let _ = step(
                &mut state,
                ProvidersInput::SignInPolled {
                    session_id: "sess-1".into(),
                    result: Ok(PollReply {
                        status,
                        error_message: message,
                        account_email: None,
                        model: None,
                        retryable: None,
                        retry_after: None,
                    }),
                },
            );
        }
        let text = joined(&state);
        assert!(text.contains(expect), "{text}");
        if retry {
            assert!(text.contains("Press enter to try again."), "{text}");
        }
    }

    let (mut state, _) = open(accounts(), true);
    feed_accounts(&mut state, vec![nous(AccountStatus::NotSignedIn)]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(start()),
        },
    );
    let _ = step(
        &mut state,
        ProvidersInput::SignInPolled {
            session_id: "sess-1".into(),
            result: Err(Failure::Http {
                status: 404,
                detail: None,
            }),
        },
    );
    assert!(
        joined(&state)
            .contains("Hermes no longer has this sign-in (it ended or the gateway restarted)."),
        "{}",
        joined(&state)
    );

    let (mut state, _) = open(accounts(), true);
    feed_accounts(&mut state, vec![nous(AccountStatus::NotSignedIn)]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(start()),
        },
    );
    let _ = step(
        &mut state,
        ProvidersInput::SignInPolled {
            session_id: "sess-1".into(),
            result: Err(Failure::Http {
                status: 400,
                detail: Some("bad session".into()),
            }),
        },
    );
    assert!(
        joined(&state).contains("Sign-in stopped: bad session."),
        "{}",
        joined(&state)
    );

    let (mut state, _) = open(accounts(), true);
    feed_accounts(&mut state, vec![nous(AccountStatus::NotSignedIn)]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(start()),
        },
    );
    let _ = step(
        &mut state,
        ProvidersInput::SignInPolled {
            session_id: "sess-1".into(),
            result: Ok(PollReply {
                status: PollStatus::Denied,
                error_message: None,
                account_email: None,
                model: None,
                retryable: Some(false),
                retry_after: Some(30),
            }),
        },
    );
    let text = joined(&state);
    assert!(!text.contains("Press enter to try again."), "{text}");
    assert!(text.contains("Try again in 30 s."), "{text}");
}

#[test]
fn cancel_once_and_stale_answers_ignored() {
    let (mut state, _) = open(accounts(), true);
    feed_accounts(&mut state, vec![nous(AccountStatus::NotSignedIn)]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(start()),
        },
    );
    let effects = step(&mut state, ProvidersInput::Key(ProvidersKey::Esc));
    let cancels = effects
        .iter()
        .filter(|effect| {
            matches!(effect, ProvidersEffect::CancelSignIn { session_id } if session_id == "sess-1")
        })
        .count();
    assert_eq!(cancels, 1, "{effects:?}");
    assert!(
        joined(&state).contains("Sign-in cancelled."),
        "{}",
        joined(&state)
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, ProvidersEffect::Close)),
        "one escape cancels the card and leaves the overlay open: {effects:?}"
    );

    let effects = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ProvidersEffect::StartSignIn { attempt: 2, .. })),
        "the next enter is attempt 2: {effects:?}"
    );
    let mut second = start();
    second.session_id = "sess-2".into();
    second.user_code = "WXYZ-9999".into();
    let _ = step(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 2,
            result: Ok(second),
        },
    );
    let text_before = joined(&state);
    let effects = step(
        &mut state,
        ProvidersInput::SignInPolled {
            session_id: "sess-1".into(),
            result: Ok(PollReply {
                status: PollStatus::Cancelled,
                error_message: None,
                account_email: None,
                model: None,
                retryable: None,
                retry_after: None,
            }),
        },
    );
    assert!(
        !has(
            &effects,
            |effect| matches!(effect, ProvidersEffect::Poll { session_id, .. } if session_id == "sess-1")
        ),
        "an old poll is ignored: {effects:?}"
    );
    assert_eq!(joined(&state), text_before);
    let effects = step(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(start()),
        },
    );
    assert!(
        has(&effects, |effect| {
            matches!(effect, ProvidersEffect::CancelSignIn { session_id } if session_id == "sess-1")
        }),
        "a stale start is cancelled: {effects:?}"
    );
    assert!(
        joined(&state).contains("WXYZ-9999"),
        "the new card stays: {}",
        joined(&state)
    );
    assert!(!joined(&state).contains("ABCD-1234"), "{}", joined(&state));

    let (mut closed, _) = open(accounts(), true);
    feed_accounts(&mut closed, vec![nous(AccountStatus::NotSignedIn)]);
    let _ = step(&mut closed, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(
        &mut closed,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(start()),
        },
    );
    let _ = step(&mut closed, ProvidersInput::Key(ProvidersKey::Esc));
    let effects = step(&mut closed, ProvidersInput::Key(ProvidersKey::Esc));
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, ProvidersEffect::CancelSignIn { .. }))
            .count(),
        0,
        "a second escape does not cancel again: {effects:?}"
    );
}

#[test]
fn local_expiry_and_countdown() {
    let (mut state, _) = open(accounts(), true);
    feed_accounts(&mut state, vec![nous(AccountStatus::NotSignedIn)]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let began = Instant::now();
    let _ = update(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(start()),
        },
        began,
    );
    let deadline = next_deadline(&state).expect("a waiting card has a deadline");
    assert!(deadline <= began + Duration::from_secs(1));
    assert!(deadline > began);
    let later = began + Duration::from_secs(1);
    let _ = update(&mut state, ProvidersInput::Tick, later);
    let moved = next_deadline(&state).expect("the countdown moves");
    assert!(moved > deadline);
    assert!(moved <= later + Duration::from_secs(1));
    assert!(
        joined(&state).contains("expires in 01:29"),
        "{}",
        joined(&state)
    );

    let expired_at = began + Duration::from_secs(90);
    let effects = update(&mut state, ProvidersInput::Tick, expired_at);
    assert!(
        has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::CancelSignIn { .. }
        )),
        "local expiry cancels: {effects:?}"
    );
    assert!(
        joined(&state).contains("The code expired."),
        "{}",
        joined(&state)
    );
    assert!(next_deadline(&state).is_none());

    let (mut state, _) = open(accounts(), true);
    feed_accounts(&mut state, vec![nous(AccountStatus::NotSignedIn)]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let mut endless = start();
    endless.expires_in = None;
    let _ = update(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(endless),
        },
        Instant::now(),
    );
    assert!(
        next_deadline(&state).is_none(),
        "no expires_in means no countdown"
    );
    assert!(!joined(&state).contains("expires in"), "{}", joined(&state));

    let (mut state, _) = open(accounts(), true);
    feed_accounts(&mut state, vec![nous(AccountStatus::NotSignedIn)]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let mut http = start();
    http.verification_url = "http://example.invalid/device".into();
    let _ = step(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(http),
        },
    );
    let effects = step(&mut state, ProvidersInput::Key(ProvidersKey::Char('o')));
    assert!(
        !has(&effects, |effect| matches!(
            effect,
            ProvidersEffect::OpenUrl(_)
        )),
        "a non-https link is not opened: {effects:?}"
    );
    assert!(
        joined(&state).contains("Talaria opens only https links. Copy the link instead."),
        "{}",
        joined(&state)
    );
}

#[test]
fn account_rows_and_sign_out_rules() {
    let (mut state, _) = open(accounts(), true);
    let mut signed = nous(AccountStatus::SignedIn {
        source_label: "device".into(),
        expires_in_s: Some(120),
    });
    signed.cli_command = Some("hermes auth".into());
    let mut unknown = account(
        "xai",
        "xAI",
        true,
        AccountStatus::Unknown("clock skew".into()),
    );
    unknown.disconnectable = true;
    let mut external = account("copilot", "Copilot", false, AccountStatus::NotSignedIn);
    external.cli_command = Some("gh auth login".into());
    external.docs_url = Some("https://example.invalid/docs".into());
    external.disconnectable = false;
    external.disconnect_hint = Some("Remove it in the vendor tool.".into());
    external.disconnect_command = Some("gh auth logout".into());
    feed_accounts(&mut state, vec![signed, unknown, external]);
    let text = joined(&state);
    assert!(
        text.contains("signed in · device · expires in 2 min"),
        "{text}"
    );
    assert!(text.contains("sign in here"), "{text}");
    assert!(text.contains("status unknown: clock skew"), "{text}");
    assert!(text.contains("not signed in"), "{text}");
    assert!(text.contains("sign in with gh auth login"), "{text}");
    assert!(!text.contains("token_preview"), "{text}");

    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    assert!(
        joined(&state).contains("Already signed in to Nous. ctrl+d signs out."),
        "{}",
        joined(&state)
    );

    let (mut state, _) = open(accounts(), true);
    let mut external = account("copilot", "Copilot", false, AccountStatus::NotSignedIn);
    external.cli_command = Some("gh auth login".into());
    external.docs_url = Some("https://example.invalid/docs".into());
    feed_accounts(&mut state, vec![external]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let text = joined(&state);
    assert!(
        text.contains(
            "Sign in to Copilot outside Talaria: run gh auth login in a terminal, then press ctrl+r here."
        ),
        "{text}"
    );
    assert!(text.contains("https://example.invalid/docs"), "{text}");
    assert!(text.contains("c copy command · esc close"), "{text}");
    let effects = step(&mut state, ProvidersInput::Key(ProvidersKey::Char('c')));
    assert!(
        has(&effects, |effect| {
            matches!(effect, ProvidersEffect::CopyText(text) if text == "gh auth login")
        }),
        "{effects:?}"
    );

    let (mut state, _) = open(accounts(), true);
    feed_accounts(&mut state, vec![nous(AccountStatus::NotSignedIn)]);
    let effects = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Remove)),
    );
    assert!(!has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::SignOut { .. }
    )));
    assert!(
        joined(&state).contains("Not signed in to Nous."),
        "{}",
        joined(&state)
    );

    let (mut state, _) = open(accounts(), true);
    feed_accounts(
        &mut state,
        vec![nous(AccountStatus::SignedIn {
            source_label: "device".into(),
            expires_in_s: None,
        })],
    );
    let _ = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Remove)),
    );
    assert!(
        joined(&state).contains("Sign out of Nous in profile fixture?"),
        "{}",
        joined(&state)
    );
    let effects = step(&mut state, ProvidersInput::Key(ProvidersKey::Char('y')));
    assert!(
        has(
            &effects,
            |effect| matches!(effect, ProvidersEffect::SignOut { id } if id == "nous")
        ),
        "{effects:?}"
    );
    let effects = step(&mut state, ProvidersInput::SignedOut(Ok(())));
    assert!(
        joined(&state).contains("Signed out of Nous."),
        "{}",
        joined(&state)
    );
    assert!(has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::LoadOauth
    )));
    assert!(has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::LoadEnv
    )));
    assert!(has(&effects, |effect| matches!(
        effect,
        ProvidersEffect::LoadOptions
    )));

    let (mut state, _) = open(accounts(), true);
    let mut stuck = account(
        "copilot",
        "Copilot",
        false,
        AccountStatus::SignedIn {
            source_label: String::new(),
            expires_in_s: None,
        },
    );
    stuck.disconnectable = false;
    stuck.disconnect_hint = Some("Remove it in the vendor tool.".into());
    stuck.disconnect_command = Some("gh auth logout".into());
    feed_accounts(
        &mut state,
        vec![account(
            "nous",
            "Nous",
            true,
            AccountStatus::SignedIn {
                source_label: String::new(),
                expires_in_s: None,
            },
        )],
    );
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Down));
    let _ = step(&mut state, ProvidersInput::OauthLoaded(Ok(vec![stuck])));
    let _ = step(
        &mut state,
        ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Remove)),
    );
    let text = joined(&state);
    assert!(text.contains("Remove it in the vendor tool."), "{text}");
    assert!(text.contains("gh auth logout"), "{text}");

    let (mut state, _) = open(accounts(), true);
    feed_accounts(&mut state, vec![nous(AccountStatus::NotSignedIn)]);
    let _ = step(&mut state, ProvidersInput::Key(ProvidersKey::Enter));
    let _ = step(
        &mut state,
        ProvidersInput::SignInStarted {
            attempt: 1,
            result: Ok(start()),
        },
    );
    let before = joined(&state);
    let effects = step(&mut state, ProvidersInput::OpenFailed);
    assert!(effects.is_empty(), "{effects:?}");
    assert!(
        joined(&state).contains("Could not open a browser. Copy the link instead."),
        "{}",
        joined(&state)
    );
    assert!(
        joined(&state).contains("ABCD-1234"),
        "the card stays: {}",
        joined(&state)
    );
    let _ = (before, screen(&state));
    assert_eq!(screen(&state).tab, ProvidersTab::Accounts);
}
