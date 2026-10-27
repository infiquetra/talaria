use super::{allowed, load_all, model::*, open_card, text};
use std::time::{Duration, Instant};

pub(crate) fn status(account: &Account) -> String {
    match &account.status {
        AccountStatus::SignedIn {
            source_label,
            expires_in_s,
        } => format!(
            "signed in{}{}",
            if source_label.is_empty() {
                String::new()
            } else {
                format!(" · {source_label}")
            },
            expires_in_s
                .map(|n| format!(" · expires {}", text::relative(n)))
                .unwrap_or_default()
        ),
        AccountStatus::Unknown(error) => format!("status unknown: {error}"),
        AccountStatus::NotSignedIn => "not signed in".into(),
    }
}
pub(crate) fn enter(
    state: &mut ProvidersState,
    account: Account,
    effects: &mut Vec<ProvidersEffect>,
) {
    if !account.device_code {
        let command = account.cli_command.clone().unwrap_or_default();
        let mut lines = vec![format!(
            "Sign in to {} outside Talaria: run {command} in a terminal, then press ctrl+r here.",
            account.name
        )];
        if let Some(url) = &account.docs_url {
            lines.push(url.clone())
        }
        open_card(
            state,
            Card::External {
                title: format!("Sign in to {}", account.name),
                lines,
                copy: account.cli_command,
            },
            effects,
        );
        return;
    }
    if matches!(account.status, AccountStatus::SignedIn { .. }) {
        state.notice = Some(format!(
            "Already signed in to {}. ctrl+d signs out.",
            text::sanitize(&account.name)
        ));
        return;
    }
    start(state, account, effects)
}
pub(crate) fn start(
    state: &mut ProvidersState,
    account: Account,
    effects: &mut Vec<ProvidersEffect>,
) {
    if !allowed(state) {
        return;
    }
    state.attempt = state.attempt.wrapping_add(1);
    let attempt = state.attempt;
    state.busy = Some(Busy::Start(attempt));
    effects.push(ProvidersEffect::StartSignIn {
        id: account.id.clone(),
        attempt,
    });
    open_card(state, Card::Starting { account, attempt }, effects)
}
pub(crate) fn remove(
    state: &mut ProvidersState,
    account: Account,
    effects: &mut Vec<ProvidersEffect>,
) {
    if !allowed(state) {
        return;
    }
    if matches!(account.status, AccountStatus::NotSignedIn) {
        state.notice = Some(format!(
            "Not signed in to {}.",
            text::sanitize(&account.name)
        ));
        return;
    }
    if account.disconnectable {
        open_card(
            state,
            Card::Confirm {
                title: format!(
                    "Sign out of {} in profile {}?",
                    text::sanitize(&account.name),
                    text::sanitize(&state.profile)
                ),
                effect: ProvidersEffect::SignOut { id: account.id },
            },
            effects,
        )
    } else {
        let mut lines = Vec::new();
        if let Some(hint) = account.disconnect_hint {
            lines.push(hint)
        }
        if let Some(command) = &account.disconnect_command {
            lines.push(command.clone())
        }
        open_card(
            state,
            Card::External {
                title: format!("Sign out of {}", account.name),
                lines,
                copy: account.disconnect_command,
            },
            effects,
        )
    }
}
pub(crate) fn started(
    state: &mut ProvidersState,
    attempt: u64,
    result: Result<SignInStart, Failure>,
    now: Instant,
    effects: &mut Vec<ProvidersEffect>,
) {
    if matches!(state.busy,Some(Busy::Start(a)) if a==attempt) {
        state.busy = None
    }
    if !matches!(&state.card,Some(Card::Starting{attempt:a,..}) if *a==attempt) {
        if let Ok(start) = result {
            effects.push(ProvidersEffect::CancelSignIn {
                session_id: start.session_id,
            })
        }
        return;
    }
    let Some(Card::Starting { account, .. }) = state.card.take() else {
        return;
    };
    match result {
        Ok(start) => {
            let interval = Duration::from_secs(start.poll_interval.unwrap_or(2).max(2) as u64);
            let expires = start
                .expires_in
                .and_then(|s| now.checked_add(Duration::from_secs(s)));
            effects.push(ProvidersEffect::Poll {
                id: account.id.clone(),
                session_id: start.session_id.clone(),
                after: interval,
            });
            state.card = Some(Card::Waiting {
                account,
                start,
                expires,
                repaint: now + Duration::from_secs(1),
                failures: 0,
            })
        }
        Err(error) => {
            let message = format!(
                "Sign-in failed: {}",
                text::failure(&error, "/api/providers/oauth")
            );
            ended(state, account, message, true, None, now)
        }
    }
}
fn ended(
    state: &mut ProvidersState,
    account: Account,
    mut message: String,
    retryable: bool,
    retry_after: Option<u64>,
    now: Instant,
) {
    if let Some(seconds) = retry_after {
        message.push_str(&format!(" Try again in {seconds} s."))
    } else if retryable {
        message.push_str(" Press enter to try again.")
    }
    state.card = Some(Card::Ended {
        account,
        text: message,
        retryable,
        retry_at: retry_after.and_then(|s| now.checked_add(Duration::from_secs(s))),
    })
}
pub(crate) fn polled(
    state: &mut ProvidersState,
    session_id: String,
    result: Result<PollReply, Failure>,
    now: Instant,
    effects: &mut Vec<ProvidersEffect>,
) {
    if !matches!(&state.card,Some(Card::Waiting{start,..}) if start.session_id==session_id) {
        return;
    }
    let Some(Card::Waiting {
        account,
        start,
        expires,
        repaint,
        mut failures,
    }) = state.card.take()
    else {
        return;
    };
    let mut final_message = None;
    let mut retryable = true;
    let mut retry_after = None;
    match result {
        Ok(reply) => {
            retryable = reply.retryable != Some(false);
            retry_after = reply.retry_after;
            match reply.status {
                PollStatus::Approved => {
                    let mut message = format!("Signed in to {}.", account.name);
                    if let Some(email) = reply.account_email.filter(|s| !s.is_empty()) {
                        message.push_str(&format!(" Account: {email}."))
                    }
                    if let Some(model) = reply.model.filter(|s| !s.is_empty()) {
                        message.push_str(&format!(" Default model: {model}."))
                    }
                    state.card = Some(Card::Approved {
                        account,
                        text: message,
                    });
                    load_all(state, effects);
                    return;
                }
                PollStatus::Pending => failures = 0,
                PollStatus::Denied => final_message = Some("The sign-in was denied.".into()),
                PollStatus::Expired => final_message = Some("The code expired.".into()),
                PollStatus::Cancelled => final_message = Some("The sign-in was cancelled.".into()),
                PollStatus::Error => {
                    final_message = Some(format!(
                        "Sign-in failed: {}",
                        reply
                            .error_message
                            .as_deref()
                            .map(text::sentence)
                            .unwrap_or_else(|| "no reason given.".into())
                    ))
                }
                PollStatus::Other(_) => failures += 1,
            }
        }
        Err(Failure::Http { status: 404, .. }) => {
            final_message = Some(
                "Hermes no longer has this sign-in (it ended or the gateway restarted).".into(),
            )
        }
        Err(
            error @ Failure::Http {
                status: 400..=499, ..
            },
        ) if !matches!(error, Failure::Http { status: 429, .. }) => {
            final_message = Some(format!(
                "Sign-in stopped: {}",
                text::sentence(&text::failure(&error, "/api/providers/oauth"))
            ))
        }
        Err(_) => failures += 1,
    }
    if failures >= 3 {
        final_message=Some("Lost contact with Hermes while waiting. The sign-in may still finish; press ctrl+r on this tab to check.".into())
    }
    if let Some(message) = final_message {
        ended(state, account, message, retryable, retry_after, now);
        return;
    }
    effects.push(ProvidersEffect::Poll {
        id: account.id.clone(),
        session_id: start.session_id.clone(),
        after: Duration::from_secs(start.poll_interval.unwrap_or(2).max(2) as u64),
    });
    state.card = Some(Card::Waiting {
        account,
        start,
        expires,
        repaint,
        failures,
    })
}
pub(crate) fn tick(state: &mut ProvidersState, now: Instant, effects: &mut Vec<ProvidersEffect>) {
    if let Some(Card::Waiting {
        expires, repaint, ..
    }) = &mut state.card
    {
        *repaint = now + Duration::from_secs(1);
        if expires.is_some_and(|expiry| now >= expiry) {
            let Some(Card::Waiting { account, start, .. }) = state.card.take() else {
                return;
            };
            effects.push(ProvidersEffect::CancelSignIn {
                session_id: start.session_id,
            });
            ended(state, account, "The code expired.".into(), true, None, now)
        }
    }
}
pub(crate) fn signed_out(
    state: &mut ProvidersState,
    result: Result<(), Failure>,
    effects: &mut Vec<ProvidersEffect>,
) {
    let name = match state.busy.take() {
        Some(Busy::SignOut(name)) => name,
        other => {
            state.busy = other;
            return;
        }
    };
    state.card = None;
    state.notice = Some(match result {
        Ok(()) => format!("Signed out of {}.", text::sanitize(&name)),
        Err(Failure::Unknown | Failure::BadReply) => text::unknown("sign-out"),
        Err(Failure::Http { status: 409, .. }) => format!(
            "No stored credentials were removed for {}.",
            text::sanitize(&name)
        ),
        Err(error) => text::failure(&error, "/api/providers/oauth"),
    });
    load_all(state, effects)
}
