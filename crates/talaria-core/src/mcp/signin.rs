use super::*;
use hermes_contract::schemas::McpOauthPollStatus;

#[derive(Debug)]
pub(super) struct SignIn {
    pub name: String,
    pub flow: Option<String>,
    pub url: String,
    pub since: Option<Instant>,
    pub due: Option<Instant>,
    pub polling: bool,
    pub failures: u8,
    pub paste: bool,
    pub callback: bool,
}
pub fn over_ssh(var: impl Fn(&str) -> Option<String>) -> bool {
    ["SSH_CLIENT", "SSH_CONNECTION", "SSH_TTY"]
        .iter()
        .any(|key| var(key).is_some_and(|value| !value.is_empty()))
}
/// The returned query components borrow the input and remain percent encoded.
/// The transport decodes them immediately before creating the callback request.
pub fn parse_redirect(pasted: &str) -> Result<RedirectParts<'_>, RedirectError> {
    let raw = pasted.trim();
    let parts = form::address_parts(raw).ok_or(RedirectError::NotUrl)?;
    if parts.scheme != "http" {
        return Err(RedirectError::NotHttp);
    }
    if !form::loopback(parts.host) || parts.authority.contains('@') {
        return Err(RedirectError::WrongHost);
    }
    if parts.path != "/callback" {
        return Err(RedirectError::WrongPath);
    }
    let query = raw
        .split('#')
        .next()
        .unwrap_or(raw)
        .split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("");
    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut iss = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "code" => code = Some(value),
            "state" => state = Some(value),
            "error" => error = Some(value),
            "iss" => iss = Some(value),
            _ => {}
        }
    }
    let state = state
        .filter(|s| !s.is_empty())
        .ok_or(RedirectError::NoState)?;
    if code.is_none_or(str::is_empty) && error.is_none_or(str::is_empty) {
        return Err(RedirectError::NoCodeOrError);
    }
    Ok(RedirectParts {
        code,
        state,
        error,
        iss,
    })
}
pub(super) fn start(state: &mut McpState, name: String) -> Vec<McpEffect> {
    if !state.connected || state.cancelled_starts.contains(&name) {
        return vec![];
    }
    state.write = Some(Write::SignIn { name: name.clone() });
    state.card = Some(Card::SignIn(SignIn {
        name: name.clone(),
        flow: None,
        url: String::new(),
        since: None,
        due: None,
        polling: false,
        failures: 0,
        paste: false,
        callback: false,
    }));
    state.notice = None;
    vec![McpEffect::StartSignIn { name }]
}
pub(super) fn cancel(state: &mut McpState) -> Vec<McpEffect> {
    let Some(Card::SignIn(signin)) = state.card.take() else {
        return vec![];
    };
    state.write = None;
    state.notice = Some("Sign-in cancelled.".into());
    match signin.flow {
        Some(flow) => cancel_flow(state, signin.name, flow),
        None => {
            state.cancelled_starts.insert(signin.name);
            vec![]
        }
    }
}
fn cancel_flow(state: &mut McpState, name: String, flow: String) -> Vec<McpEffect> {
    if state.cancelled_flows.insert(flow.clone()) {
        vec![McpEffect::CancelSignIn { name, flow }]
    } else {
        vec![]
    }
}
pub(super) fn started(
    state: &mut McpState,
    name: String,
    result: Result<(String, String), Failure>,
    now: Instant,
) -> Vec<McpEffect> {
    let cancelled = state.cancelled_starts.remove(&name);
    let active = matches!(&state.card, Some(Card::SignIn(s)) if s.name == name && s.flow.is_none())
        && matches!(&state.write, Some(Write::SignIn { name: expected }) if expected == &name);
    if cancelled || !state.open || !active {
        return match result {
            Ok((flow, _)) => cancel_flow(state, name, flow),
            Err(_) => vec![],
        };
    }
    match result {
        Ok((flow, url)) => {
            if let Some(Card::SignIn(s)) = &mut state.card {
                s.flow = Some(flow);
                s.url = url;
                s.since = Some(now);
                s.due = Some(now + Duration::from_secs(2));
            }
            vec![]
        }
        Err(e) => {
            state.write = None;
            let mut text = match &e {
                Failure::Rpc {
                    code: 4001,
                    message,
                    ..
                } => short(message),
                Failure::Rpc {
                    code: 5024,
                    message,
                    ..
                } if message.contains("Too many") => {
                    "Hermes has too many sign-ins running. Try again in a few minutes.".into()
                }
                Failure::Rpc {
                    code: 5024,
                    message,
                    ..
                } if message.contains("already in progress") => {
                    format!("A sign-in for {name} is already running. Try again in a few minutes.")
                }
                Failure::Rpc {
                    code: 5024,
                    message,
                    ..
                } => format!("Could not start sign-in: {}", short(message)),
                _ => error_text(state, &name, "mcp.servers.oauth.start", &e, true),
            };
            servers::append_hint(state, &name, &mut text);
            state.card = Some(Card::Done {
                title: format!("Sign in to {}", short(&name)),
                message: text,
            });
            if rpc_code(&e, 4064) {
                refresh()
            } else {
                vec![]
            }
        }
    }
}
pub(super) fn tick(state: &mut McpState, now: Instant) -> Vec<McpEffect> {
    if !matches!(&state.card, Some(Card::SignIn(_))) {
        return vec![];
    }
    let Some(Card::SignIn(mut signin)) = state.card.take() else {
        return vec![];
    };
    let Some(since) = signin.since else {
        state.card = Some(Card::SignIn(signin));
        return vec![];
    };
    if now.saturating_duration_since(since) >= Duration::from_secs(360) {
        state.write = None;
        let effects = signin.flow.take().map_or_else(Vec::new, |flow| {
            cancel_flow(state, signin.name.clone(), flow)
        });
        state.card = Some(Card::Done {
            title: format!("Sign in to {}", short(&signin.name)),
            message: "The sign-in timed out.".into(),
        });
        state.secrets.clear();
        let mut effects = effects;
        effects.push(McpEffect::ForgetSecrets);
        return effects;
    }
    let effects = if !signin.polling && state.connected && signin.due.is_some_and(|due| due <= now)
    {
        signin.polling = true;
        signin.due = None;
        signin
            .flow
            .as_ref()
            .map(|flow| McpEffect::Poll {
                name: signin.name.clone(),
                flow: flow.clone(),
            })
            .into_iter()
            .collect()
    } else {
        vec![]
    };
    state.card = Some(Card::SignIn(signin));
    effects
}
pub(super) fn polled(
    state: &mut McpState,
    name: String,
    result: Result<McpOauthPollResult, Failure>,
    now: Instant,
) -> Vec<McpEffect> {
    if !matches!(&state.card, Some(Card::SignIn(_))) {
        return vec![];
    }
    let Some(Card::SignIn(mut signin)) = state.card.take() else {
        return vec![];
    };
    if signin.name != name || signin.since.is_none() || !signin.polling {
        state.card = Some(Card::SignIn(signin));
        return vec![];
    }
    signin.polling = false;
    let failure = match &result {
        Err(e) => Some(error_text(state, &name, "mcp.servers.oauth.poll", e, false)),
        Ok(r) if !r.ok => Some(short(
            r.error_message
                .as_deref()
                .unwrap_or("no answer from Hermes"),
        )),
        _ => None,
    };
    let mut effects = vec![];
    let done = if let Some(error) = failure {
        signin.failures += 1;
        if signin.failures >= 3 {
            if let Some(flow) = signin.flow.take() {
                effects.extend(cancel_flow(state, name.clone(), flow));
            }
            Some(format!("Lost track of the sign-in: {error}."))
        } else {
            None
        }
    } else if let Ok(reply) = result {
        signin.failures = 0;
        match reply.status {
            McpOauthPollStatus::Pending => None,
            McpOauthPollStatus::Approved => {
                changed(state);
                state.installed_hint.remove(&name);
                effects.extend(refresh());
                Some(format!(
                    "Signed in to {name}: {} tools.",
                    reply.tools.as_ref().map_or(0, Vec::len)
                ))
            }
            McpOauthPollStatus::Error => {
                let elapsed = now.saturating_duration_since(signin.since.unwrap_or(now));
                let mut text = if elapsed >= Duration::from_secs(290) {
                    "The sign-in timed out.".into()
                } else {
                    format!(
                        "Sign-in failed: {}",
                        short(reply.error_message.as_deref().unwrap_or("unknown error"))
                    )
                };
                servers::append_hint(state, &name, &mut text);
                Some(text)
            }
        }
    } else {
        None
    };
    if let Some(message) = done {
        state.write = None;
        effects.push(forget(state));
        state.card = Some(Card::Done {
            title: format!("Sign in to {}", short(&name)),
            message,
        });
    } else {
        signin.due = Some(now + Duration::from_secs(2));
        state.card = Some(Card::SignIn(signin));
    }
    effects
}
pub(super) fn redirect_hint(url: &str) -> String {
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or("");
    let encoded = query
        .split('&')
        .find_map(|p| p.strip_prefix("redirect_uri="));
    if let Some(encoded) = encoded {
        let mut decoded = Vec::new();
        let mut bytes = encoded.bytes();
        while let Some(byte) = bytes.next() {
            if byte == b'%' {
                let hex: String = bytes.by_ref().take(2).map(char::from).collect();
                if let Ok(b) = u8::from_str_radix(&hex, 16) {
                    decoded.push(b);
                }
            } else {
                decoded.push(byte);
            }
        }
        if let Ok(raw) = String::from_utf8(decoded)
            && let Some(parts) = form::address_parts(&raw)
            && parts.scheme == "http"
            && form::loopback(parts.host)
            && parts.path == "/callback"
        {
            return raw;
        }
    }
    "http://127.0.0.1/callback".into()
}
