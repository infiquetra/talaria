mod accounts;
mod endpoints;
mod keys;
mod model;
pub mod text;
pub use model::*;
use std::time::Instant;

pub fn parse_providers_args(args: &str) -> Result<OpenTarget, &'static str> {
    match args.trim().to_ascii_lowercase().as_str() {
        "" | "keys" => Ok(OpenTarget::Tab(ProvidersTab::Keys)),
        "accounts" => Ok(OpenTarget::Tab(ProvidersTab::Accounts)),
        "endpoints" => Ok(OpenTarget::Tab(ProvidersTab::Endpoints)),
        _ => Err(text::USAGE),
    }
}
pub fn update(
    state: &mut ProvidersState,
    input: ProvidersInput,
    now: Instant,
) -> Vec<ProvidersEffect> {
    let mut effects = Vec::new();
    match input {
        ProvidersInput::Open {
            target,
            profile_name,
            connected,
        } => {
            if state.card.is_some() {
                close_card(state, &mut effects);
            }
            state.open = true;
            state.profile = profile_name;
            state.connected = connected;
            state.filter.clear();
            state.selected = 0;
            state.notice = None;
            match target {
                OpenTarget::Tab(tab) => {
                    state.tab = tab;
                    state.target = None
                }
                OpenTarget::Provider { slug } => {
                    state.tab = ProvidersTab::Keys;
                    state.target = Some(slug)
                }
            }
            for entry in &mut state.loads {
                entry.done = false;
                entry.error = None
            }
            if state.target.is_some() {
                load(state, 0, &mut effects);
                load(state, 1, &mut effects);
                load(state, 2, &mut effects);
            } else {
                match state.tab {
                    ProvidersTab::Keys => {
                        load(state, 0, &mut effects);
                        load(state, 1, &mut effects);
                    }
                    ProvidersTab::Accounts => load(state, 2, &mut effects),
                    ProvidersTab::Endpoints => load(state, 3, &mut effects),
                }
            }
        }
        ProvidersInput::Close => close(state, &mut effects),
        ProvidersInput::Key(key) if state.open => key_input(state, key, now, &mut effects),
        ProvidersInput::Connection { connected } => {
            let was = state.connected;
            state.connected = connected;
            if connected && !was {
                load_all(state, &mut effects)
            }
        }
        ProvidersInput::Tick => accounts::tick(state, now, &mut effects),
        ProvidersInput::OpenFailed => {
            state.notice = Some("Could not open a browser. Copy the link instead.".into())
        }
        ProvidersInput::SecretEdited { empty } if state.busy.is_none() => match &mut state.card {
            Some(Card::Key(card)) => {
                card.empty = empty;
                card.unchecked = false;
                card.message = None
            }
            Some(Card::Form(form)) if form.field == 2 => endpoints::secret(form, empty),
            _ => {}
        },
        ProvidersInput::SecretCleared if state.busy.is_none() => {
            match &mut state.card {
                Some(Card::Key(card)) => {
                    card.empty = true;
                    card.unchecked = false;
                    card.message = None
                }
                Some(Card::Form(form)) if form.field == 2 => endpoints::secret(form, true),
                _ => {}
            }
            effects.push(ProvidersEffect::ForgetSecret)
        }
        ProvidersInput::SecretSubmitted => {
            if matches!(state.card, Some(Card::Form(_))) {
                if state.busy.is_none() {
                    if let Some(Card::Form(form)) = &mut state.card {
                        endpoints::edit(form, ProvidersKey::Enter)
                    }
                } else {
                    state.notice = Some("Waiting for Hermes.".into())
                }
            } else {
                if let Some(Card::Key(card)) = &mut state.card {
                    card.empty = false;
                }
                keys::submit(state, &mut effects)
            }
        }
        ProvidersInput::EnvLoaded(result) => {
            let error = result.as_ref().err().cloned();
            if let Ok(rows) = result {
                state.env = rows
                    .into_iter()
                    .map(|mut row| {
                        row.tail = row.tail.as_deref().and_then(keys::tail);
                        row
                    })
                    .collect();
                keys::rebuild(state)
            }
            loaded(state, 0, error, &mut effects)
        }
        ProvidersInput::OptionsLoaded(result) => {
            let error = result.as_ref().err().cloned();
            if let Ok(options) = result {
                state.options = Some(options)
            }
            loaded(state, 1, error, &mut effects)
        }
        ProvidersInput::OauthLoaded(result) => {
            let error = result.as_ref().err().cloned();
            if let Ok(accounts) = result {
                let selected = state.accounts.get(state.selected).map(|a| a.id.clone());
                state.accounts = accounts;
                if let Some(index) =
                    selected.and_then(|s| state.accounts.iter().position(|a| a.id == s))
                {
                    state.selected = index
                } else {
                    state.selected = state.selected.min(state.accounts.len().saturating_sub(1));
                }
            }
            loaded(state, 2, error, &mut effects)
        }
        ProvidersInput::EndpointsLoaded(result) => {
            let error = result.as_ref().err().cloned();
            if let Ok(list) = result {
                let selected = state
                    .selected
                    .checked_sub(1)
                    .and_then(|i| state.endpoints.endpoints.get(i))
                    .map(|e| e.id.clone());
                state.endpoints = list;
                if let Some(index) =
                    selected.and_then(|s| state.endpoints.endpoints.iter().position(|e| e.id == s))
                {
                    state.selected = index + 1
                } else {
                    state.selected = state.selected.min(state.endpoints.endpoints.len());
                }
            }
            loaded(state, 3, error, &mut effects)
        }
        ProvidersInput::KeyChecked(result) => keys::checked(state, result, &mut effects),
        ProvidersInput::KeySaved(result) => keys::saved(state, result, &mut effects),
        ProvidersInput::Disconnected(result) => keys::disconnected(state, result, &mut effects),
        ProvidersInput::SignInStarted { attempt, result } => {
            accounts::started(state, attempt, result, now, &mut effects)
        }
        ProvidersInput::SignInPolled { session_id, result } => {
            accounts::polled(state, session_id, result, now, &mut effects)
        }
        ProvidersInput::SignedOut(result) => accounts::signed_out(state, result, &mut effects),
        ProvidersInput::EndpointChecked(result) => endpoints::checked(state, result),
        ProvidersInput::EndpointSaved(result) => {
            endpoints::saved(state, result, false, &mut effects)
        }
        ProvidersInput::EndpointDeleted(result) => {
            endpoints::saved(state, result, true, &mut effects)
        }
        _ => {}
    }
    target(state, &mut effects);
    effects
}
fn load(state: &mut ProvidersState, index: usize, effects: &mut Vec<ProvidersEffect>) {
    let load = &mut state.loads[index];
    if load.running {
        load.again = true;
        return;
    }
    load.running = true;
    effects.push(match index {
        0 => ProvidersEffect::LoadEnv,
        1 => ProvidersEffect::LoadOptions,
        2 => ProvidersEffect::LoadOauth,
        _ => ProvidersEffect::LoadEndpoints,
    })
}
fn load_all(state: &mut ProvidersState, effects: &mut Vec<ProvidersEffect>) {
    for index in 0..4 {
        load(state, index, effects)
    }
}
fn loaded(
    state: &mut ProvidersState,
    index: usize,
    error: Option<Failure>,
    effects: &mut Vec<ProvidersEffect>,
) {
    let entry = &mut state.loads[index];
    entry.running = false;
    entry.done = true;
    entry.error = error;
    let again = std::mem::take(&mut entry.again);
    if again {
        load(state, index, effects)
    }
}
fn open_card(state: &mut ProvidersState, card: Card, effects: &mut Vec<ProvidersEffect>) {
    state.notice = None;
    state.card = Some(card);
    effects.push(ProvidersEffect::ForgetSecret)
}
fn close_card(state: &mut ProvidersState, effects: &mut Vec<ProvidersEffect>) {
    match state.busy {
        Some(Busy::CheckKey) => state.busy = Some(Busy::DiscardedKeyCheck),
        Some(Busy::CheckEndpoint) => state.busy = Some(Busy::DiscardedEndpointCheck),
        _ => {}
    }
    match state.card.take() {
        Some(Card::Waiting { start, .. }) => {
            effects.push(ProvidersEffect::CancelSignIn {
                session_id: start.session_id,
            });
            state.notice = Some("Sign-in cancelled.".into())
        }
        Some(Card::Starting { attempt, .. }) => {
            if matches!(state.busy,Some(Busy::Start(a)) if a==attempt) {
                state.busy = None
            }
            state.notice = Some("Sign-in cancelled.".into())
        }
        _ => {}
    }
    effects.push(ProvidersEffect::ForgetSecret)
}
fn close(state: &mut ProvidersState, effects: &mut Vec<ProvidersEffect>) {
    close_card(state, effects);
    state.open = false;
    state.target = None;
    state.notice = None;
    effects.push(ProvidersEffect::Close)
}
fn allowed(state: &mut ProvidersState) -> bool {
    if !state.connected {
        return false;
    }
    if state.busy.is_some() {
        state.notice = Some("Waiting for Hermes.".into());
        return false;
    }
    true
}
fn target(state: &mut ProvidersState, effects: &mut Vec<ProvidersEffect>) {
    if !state.open || state.target.is_none() || !state.loads[..3].iter().all(|l| l.done) {
        return;
    }
    let Some(slug) = state.target.take() else {
        return;
    };
    if let Some(row) = state.keys.iter().find(|r| r.slug == slug).cloned() {
        state.tab = ProvidersTab::Keys;
        state.selected = state
            .keys
            .iter()
            .position(|r| r.env_var == row.env_var)
            .unwrap_or(0);
        keys::open(state, row, effects);
        return;
    }
    if let Some(index) = state.accounts.iter().position(|a| a.id == slug) {
        state.tab = ProvidersTab::Accounts;
        state.selected = index;
        return;
    }
    if let Some(p) = keys::option(state, &slug) {
        if p.auth_type.as_deref() == Some("api_key")
            && let Some(env) = p.key_env.as_ref().filter(|e| !e.is_empty())
        {
            let row = EnvRow {
                env_var: env.clone(),
                slug: slug.clone(),
                label: if p.name.is_empty() {
                    slug.clone()
                } else {
                    p.name.clone()
                },
                ..EnvRow::default()
            };
            keys::open(state, row, effects);
            return;
        }
        let name = if p.name.is_empty() {
            slug.clone()
        } else {
            p.name.clone()
        };
        let warning = p
            .warning
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| format!(": {s}"))
            .unwrap_or_default();
        state.notice = Some(text::sanitize(&format!(
            "{name} is set up outside Talaria{warning}."
        )))
    } else {
        state.notice = Some(format!(
            "{} is set up outside Talaria.",
            text::sanitize(&slug)
        ))
    }
}
fn indices(state: &ProvidersState) -> Vec<usize> {
    match state.tab {
        ProvidersTab::Keys => state
            .keys
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                text::matches(
                    &state.filter,
                    &format!("{} {} {}", r.label, r.slug, r.env_var),
                )
            })
            .map(|(i, _)| i)
            .collect(),
        ProvidersTab::Accounts => state
            .accounts
            .iter()
            .enumerate()
            .filter(|(_, a)| text::matches(&state.filter, &format!("{} {}", a.name, a.id)))
            .map(|(i, _)| i)
            .collect(),
        ProvidersTab::Endpoints => (0..=state.endpoints.endpoints.len())
            .filter(|i| {
                if *i == 0 {
                    text::matches(&state.filter, "+ Add an endpoint")
                } else {
                    let e = &state.endpoints.endpoints[i - 1];
                    text::matches(&state.filter, &format!("{} {}", e.name, e.id))
                }
            })
            .collect(),
    }
}
fn key_input(
    state: &mut ProvidersState,
    key: ProvidersKey,
    now: Instant,
    effects: &mut Vec<ProvidersEffect>,
) {
    if key == ProvidersKey::Esc {
        if let Some(Card::Form(form)) = &state.card
            && state.busy.is_none()
            && form.dirty
        {
            state.card = Some(Card::Discard(form.clone()));
            return;
        }
        if state.card.is_some() {
            close_card(state, effects)
        } else if !state.filter.is_empty() {
            state.filter.clear();
            state.selected = 0
        } else {
            close(state, effects)
        }
        return;
    }
    if let ProvidersKey::Action(ProvidersAction::Reload) = key {
        match state.tab {
            ProvidersTab::Keys => {
                load(state, 0, effects);
                load(state, 1, effects)
            }
            ProvidersTab::Accounts => load(state, 2, effects),
            ProvidersTab::Endpoints => load(state, 3, effects),
        }
        return;
    }
    if state.card.is_some() {
        card_input(state, key, now, effects);
        return;
    }
    match key {
        ProvidersKey::Tab | ProvidersKey::Action(ProvidersAction::NextTab) => {
            change_tab(state, true, effects)
        }
        ProvidersKey::BackTab | ProvidersKey::Action(ProvidersAction::PrevTab) => {
            change_tab(state, false, effects)
        }
        ProvidersKey::Char(c) => {
            state.filter.push(c);
            state.selected = indices(state).first().copied().unwrap_or(0)
        }
        ProvidersKey::Paste(text) => {
            if text::clean(&text) {
                state.filter.push_str(&text);
                state.selected = indices(state).first().copied().unwrap_or(0)
            }
        }
        ProvidersKey::Backspace => {
            state.filter.pop();
            state.selected = indices(state).first().copied().unwrap_or(0)
        }
        ProvidersKey::Clear | ProvidersKey::CtrlU => {
            state.filter.clear();
            state.selected = 0
        }
        ProvidersKey::Up
        | ProvidersKey::Down
        | ProvidersKey::PageUp
        | ProvidersKey::PageDown
        | ProvidersKey::Home
        | ProvidersKey::End => {
            let rows = indices(state);
            if rows.is_empty() {
                return;
            }
            let current = rows.iter().position(|i| *i == state.selected).unwrap_or(0);
            let next = match key {
                ProvidersKey::Up => current.saturating_sub(1),
                ProvidersKey::Down => (current + 1).min(rows.len() - 1),
                ProvidersKey::PageUp => current.saturating_sub(10),
                ProvidersKey::PageDown => (current + 10).min(rows.len() - 1),
                ProvidersKey::Home => 0,
                _ => rows.len() - 1,
            };
            state.selected = rows[next]
        }
        ProvidersKey::Enter => {
            if !indices(state).contains(&state.selected) {
                return;
            }
            match state.tab {
                ProvidersTab::Keys => {
                    if let Some(row) = state.keys.get(state.selected).cloned() {
                        keys::open(state, row, effects)
                    }
                }
                ProvidersTab::Accounts => {
                    if let Some(account) = state.accounts.get(state.selected).cloned() {
                        accounts::enter(state, account, effects)
                    }
                }
                ProvidersTab::Endpoints => {
                    let endpoint = state
                        .selected
                        .checked_sub(1)
                        .and_then(|i| state.endpoints.endpoints.get(i))
                        .cloned();
                    endpoints::open(state, endpoint, effects)
                }
            }
        }
        ProvidersKey::Action(ProvidersAction::Remove) => {
            if !indices(state).contains(&state.selected) {
                return;
            }
            match state.tab {
                ProvidersTab::Keys => {
                    if let Some(row) = state.keys.get(state.selected).cloned() {
                        keys::remove(state, row, effects)
                    }
                }
                ProvidersTab::Accounts => {
                    if let Some(account) = state.accounts.get(state.selected).cloned() {
                        accounts::remove(state, account, effects)
                    }
                }
                ProvidersTab::Endpoints => {
                    if let Some(endpoint) = state
                        .selected
                        .checked_sub(1)
                        .and_then(|i| state.endpoints.endpoints.get(i))
                        .cloned()
                    {
                        endpoints::remove(state, endpoint, effects)
                    }
                }
            }
        }
        _ => {}
    }
}
fn change_tab(state: &mut ProvidersState, next: bool, effects: &mut Vec<ProvidersEffect>) {
    state.tab = match (state.tab, next) {
        (ProvidersTab::Keys, true) | (ProvidersTab::Endpoints, false) => ProvidersTab::Accounts,
        (ProvidersTab::Accounts, true) | (ProvidersTab::Keys, false) => ProvidersTab::Endpoints,
        _ => ProvidersTab::Keys,
    };
    state.filter.clear();
    state.selected = 0;
    state.notice = None;
    let index = match state.tab {
        ProvidersTab::Keys => 0,
        ProvidersTab::Accounts => 2,
        ProvidersTab::Endpoints => 3,
    };
    if !state.loads[index].done && !state.loads[index].running {
        load(state, index, effects)
    }
    if state.tab == ProvidersTab::Keys && !state.loads[1].done && !state.loads[1].running {
        load(state, 1, effects);
    }
}
fn card_input(
    state: &mut ProvidersState,
    key: ProvidersKey,
    now: Instant,
    effects: &mut Vec<ProvidersEffect>,
) {
    if let Some(Card::Form(_)) = &state.card {
        match key {
            ProvidersKey::Action(ProvidersAction::Test) => {
                endpoints::request(state, false, effects)
            }
            ProvidersKey::Action(ProvidersAction::Save) => endpoints::request(state, true, effects),
            ProvidersKey::Action(ProvidersAction::ClearKey) => {
                if !allowed(state) {
                    return;
                }
                if let Some(Card::Form(form)) = &mut state.card {
                    form.cleared = true;
                    endpoints::secret(form, true);
                    effects.push(ProvidersEffect::ForgetSecret)
                }
            }
            _ => {
                if state.busy.is_none()
                    && let Some(Card::Form(form)) = &mut state.card
                {
                    endpoints::edit(form, key)
                }
            }
        }
        return;
    }
    if matches!(state.card, Some(Card::Key(_))) {
        if key == ProvidersKey::Enter {
            keys::submit(state, effects)
        }
        return;
    }
    if matches!(state.card, Some(Card::Discard(_))) {
        match key {
            ProvidersKey::Char('y') | ProvidersKey::Enter => close_card(state, effects),
            ProvidersKey::Char('n') => {
                if let Some(Card::Discard(form)) = state.card.take() {
                    state.card = Some(Card::Form(form))
                }
            }
            _ => {}
        }
        return;
    }
    if let Some(Card::Confirm { effect, .. }) = &state.card {
        if matches!(key, ProvidersKey::Char('n')) {
            close_card(state, effects);
            return;
        }
        if matches!(key, ProvidersKey::Char('y') | ProvidersKey::Enter) {
            let effect = effect.clone();
            if !allowed(state) {
                return;
            }
            state.busy = Some(match &effect {
                ProvidersEffect::Disconnect { slug } => Busy::Disconnect(
                    state
                        .keys
                        .iter()
                        .find(|r| r.slug == *slug)
                        .map(|r| r.label.clone())
                        .unwrap_or_else(|| slug.clone()),
                ),
                ProvidersEffect::SignOut { id } => Busy::SignOut(
                    state
                        .accounts
                        .iter()
                        .find(|a| a.id == *id)
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| id.clone()),
                ),
                ProvidersEffect::DeleteEndpoint { id } => Busy::DeleteEndpoint(
                    state
                        .endpoints
                        .endpoints
                        .iter()
                        .find(|e| e.id == *id)
                        .map(|e| e.name.clone())
                        .unwrap_or_else(|| id.clone()),
                ),
                _ => return,
            });
            state.card = None;
            effects.push(effect)
        }
        return;
    }
    if matches!(key, ProvidersKey::Enter) {
        if let Some(Card::Ended {
            account,
            retryable,
            retry_at,
            ..
        }) = &state.card
            && *retryable
            && retry_at.is_none_or(|t| now >= t)
        {
            accounts::start(state, account.clone(), effects)
        }
        return;
    }
    let copy = matches!(
        key,
        ProvidersKey::Char('c') | ProvidersKey::Action(ProvidersAction::Copy)
    );
    let open = matches!(
        key,
        ProvidersKey::Char('o') | ProvidersKey::Action(ProvidersAction::OpenLink)
    );
    let text = match &state.card {
        Some(Card::Waiting { start, .. }) if copy => Some(start.user_code.clone()),
        Some(Card::Waiting { start, .. }) if open => Some(start.verification_url.clone()),
        Some(Card::External { copy: command, .. }) if copy => command.clone(),
        _ => None,
    };
    if let Some(value) = text {
        if !text::clean(&value) {
            state.notice =
                Some("Talaria will not copy or open text that contained control characters.".into())
        } else if open {
            if value.starts_with("https://") {
                effects.push(ProvidersEffect::OpenUrl(value))
            } else {
                state.notice = Some("Talaria opens only https links. Copy the link instead.".into())
            }
        } else {
            effects.push(ProvidersEffect::CopyText(value));
            state.notice = Some("Copied to the clipboard if your terminal allows it.".into())
        }
    }
}
pub fn view(state: &ProvidersState, now: Instant) -> ProvidersView {
    let index = match state.tab {
        ProvidersTab::Keys => 0,
        ProvidersTab::Accounts => 2,
        ProvidersTab::Endpoints => 3,
    };
    let load = &state.loads[index];
    let visible = indices(state);
    let mut rows = Vec::new();
    if !load.done {
        rows.push("Loading…".into())
    } else if let Some(error) = &load.error {
        let (tab, route) = match state.tab {
            ProvidersTab::Keys => ("keys", "/api/env"),
            ProvidersTab::Accounts => ("accounts", "/api/providers/oauth"),
            ProvidersTab::Endpoints => ("endpoints", "/api/providers/custom-endpoints"),
        };
        rows.push(format!(
            "Could not load {tab}: {} ctrl+r tries again.",
            text::failure(error, route)
        ))
    } else {
        for i in &visible {
            rows.push(match state.tab {
                ProvidersTab::Keys => {
                    let row = &state.keys[*i];
                    let shared = if row.shared_with.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " · also used by {}",
                            row.shared_with
                                .iter()
                                .map(|(_, name)| name.clone())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    format!(
                        "{} · {} · {}{shared}",
                        row.label,
                        row.env_var,
                        keys::status(state, row)
                    )
                }
                ProvidersTab::Accounts => {
                    let account = &state.accounts[*i];
                    format!(
                        "{} · {} · {}",
                        account.name,
                        accounts::status(account),
                        if account.device_code {
                            "sign in here".into()
                        } else {
                            format!(
                                "sign in with {}",
                                account.cli_command.as_deref().unwrap_or("")
                            )
                        }
                    )
                }
                ProvidersTab::Endpoints => {
                    if *i == 0 {
                        "+ Add an endpoint".into()
                    } else {
                        let e = &state.endpoints.endpoints[*i - 1];
                        format!(
                            "{} · {} · {}{}{}{}",
                            e.name,
                            e.base_url,
                            e.model,
                            if e.is_current { " · default" } else { "" },
                            if e.has_api_key { " · key saved" } else { "" },
                            if e.source == "custom_providers" {
                                " · older format"
                            } else {
                                ""
                            }
                        )
                    }
                }
            })
        }
        if rows.is_empty() {
            rows.push(format!("No provider matches \"{}\".", state.filter))
        }
    }
    let mut footer = match state.tab {
        ProvidersTab::Keys => {
            "enter set key · ctrl+d remove · ctrl+r reload · tab next tab · type to filter".into()
        }
        ProvidersTab::Accounts => {
            "enter sign in · ctrl+d sign out · ctrl+r reload · tab next tab".into()
        }
        ProvidersTab::Endpoints => {
            "enter edit · ctrl+d delete · ctrl+r reload · tab next tab".into()
        }
    };
    if state.tab == ProvidersTab::Keys
        && let Some(error) = &state.loads[1].error
    {
        footer = format!(
            "Could not check readiness: {} ctrl+r tries again.",
            text::sentence(&text::check_reason(error, "model.options"))
        )
    }
    if state.tab == ProvidersTab::Endpoints && load.done && load.error.is_none() {
        if !state.endpoints.current_provider.is_empty() {
            footer = format!(
                "Default: {} on {} · {footer}",
                state.endpoints.current_model, state.endpoints.current_provider
            )
        } else if state.endpoints.endpoints.is_empty() {
            footer = format!("No custom endpoints in this profile. · {footer}")
        }
    }
    let card = state.card.as_ref().map(|card| card_view(state, card, now));
    let secret_focus = state.busy.is_none()
        && matches!(
            &state.card,
            Some(Card::Key(_)) | Some(Card::Form(Form { field: 2, .. }))
        );
    ProvidersView {
        title: format!("Providers · profile {}", text::sanitize(&state.profile)),
        tab: state.tab,
        rows: rows.into_iter().map(|s| text::sanitize(&s)).collect(),
        selected: if load.done && load.error.is_none() {
            visible.iter().position(|i| *i == state.selected)
        } else {
            None
        },
        filter: text::sanitize(&state.filter),
        card,
        notice: state.notice.as_deref().map(text::sanitize),
        banner: (!state.connected)
            .then(|| "Not connected to Hermes. Changes are off until Talaria reconnects.".into()),
        secret_focus,
        footer: text::sanitize(&footer),
    }
}
fn card_view(state: &ProvidersState, card: &Card, now: Instant) -> ProvidersCard {
    let (kind, title, mut lines, footer, selected_field) = match card {
        Card::Key(card) => {
            let mut lines = vec![format!(
                "Saved to profile {}'s key file as {}.",
                state.profile, card.row.env_var
            )];
            if let Some(message) = &card.message {
                lines.push(message.clone())
            }
            match state.busy {
                Some(Busy::CheckKey) => lines.push("Checking the key…".into()),
                Some(Busy::SaveKey(_)) => lines.push("Saving the key…".into()),
                _ => {}
            }
            (
                CardKind::Key,
                format!("API key for {}", card.row.label),
                lines,
                "enter save · ctrl+u clear · esc cancel".into(),
                None,
            )
        }
        Card::Confirm { title, effect } => (
            match effect {
                ProvidersEffect::SignOut { .. } => CardKind::SignOutConfirm,
                ProvidersEffect::DeleteEndpoint { .. } => CardKind::DeleteConfirm,
                _ => CardKind::RemoveConfirm,
            },
            title.clone(),
            Vec::new(),
            match effect {
                ProvidersEffect::SignOut { .. } => "y sign out · n keep",
                ProvidersEffect::DeleteEndpoint { .. } => "y delete · n keep",
                _ => "y remove · n keep",
            }
            .into(),
            None,
        ),
        Card::External { title, lines, copy } => (
            if title.starts_with("Sign out of ") {
                CardKind::DisconnectHint
            } else {
                CardKind::External
            },
            title.clone(),
            lines.clone(),
            if copy.is_some() {
                "c copy command · esc close"
            } else {
                "esc close"
            }
            .into(),
            None,
        ),
        Card::Starting { account, .. } => (
            CardKind::Waiting,
            format!("Sign in to {}", account.name),
            vec!["Starting sign-in…".into()],
            "esc cancel".into(),
            None,
        ),
        Card::Waiting {
            account,
            start,
            expires,
            repaint,
            ..
        } => {
            let waiting = if let Some(expiry) = expires {
                let painted_at = repaint
                    .checked_sub(std::time::Duration::from_secs(1))
                    .unwrap_or(now)
                    .max(now);
                let remaining = expiry.saturating_duration_since(painted_at);
                let seconds = remaining.as_secs() + u64::from(remaining.subsec_nanos() != 0);
                format!(
                    "Waiting for you to approve in the browser… expires in {:02}:{:02}",
                    seconds / 60,
                    seconds % 60
                )
            } else {
                "Waiting for you to approve in the browser…".into()
            };
            (
                CardKind::Waiting,
                format!("Sign in to {}", account.name),
                vec![
                    format!("1. Open {}", start.verification_url),
                    format!("2. Enter the code {}", start.user_code),
                    waiting,
                ],
                "o open link · c copy code · esc cancel".into(),
                None,
            )
        }
        Card::Approved { account, text } => (
            CardKind::Ended,
            format!("Sign in to {}", account.name),
            vec![text.clone()],
            "esc close".into(),
            None,
        ),
        Card::Ended { account, text, .. } => (
            CardKind::Ended,
            format!("Sign in to {}", account.name),
            vec![text.clone()],
            "enter try again · esc close".into(),
            None,
        ),
        Card::Form(form) => (
            CardKind::EndpointForm,
            if form.original.is_some() {
                "Edit endpoint"
            } else {
                "Add an endpoint"
            }
            .into(),
            endpoints::lines(form),
            "ctrl+t test · ctrl+s save · tab next field · esc close".into(),
            Some(form.field),
        ),
        Card::Discard(_) => (
            CardKind::RemoveConfirm,
            "Discard your changes?".into(),
            Vec::new(),
            "y discard · n keep editing".into(),
            None,
        ),
    };
    if matches!(card, Card::Form(_)) {
        match state.busy {
            Some(Busy::CheckEndpoint) => lines.push("Testing…".into()),
            Some(Busy::SaveEndpoint(_)) => lines.push("Saving…".into()),
            _ => {}
        }
    }
    if let Some(index) = selected_field {
        if let Some(line) = lines.get_mut(index) {
            line.insert_str(0, "› ");
        }
    } else {
        lines.insert(0, title);
    }
    ProvidersCard {
        kind,
        lines: lines.into_iter().map(|s| text::sanitize(&s)).collect(),
        footer,
    }
}
pub fn next_deadline(state: &ProvidersState) -> Option<Instant> {
    if !state.open {
        return None;
    }
    match &state.card {
        Some(Card::Waiting {
            expires: Some(expiry),
            repaint,
            ..
        }) => Some((*expiry).min(*repaint)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn key_state(now: Instant) -> ProvidersState {
        let mut state = ProvidersState::new();
        update(
            &mut state,
            ProvidersInput::Open {
                target: OpenTarget::Tab(ProvidersTab::Keys),
                profile_name: "fixture".into(),
                connected: true,
            },
            now,
        );
        update(
            &mut state,
            ProvidersInput::EnvLoaded(Ok(vec![EnvRow {
                env_var: "EXAMPLE_API_KEY".into(),
                slug: "example".into(),
                label: "Example".into(),
                primary: true,
                is_password: true,
                ..EnvRow::default()
            }])),
            now,
        );
        update(&mut state, ProvidersInput::Key(ProvidersKey::Enter), now);
        state
    }

    #[test]
    fn stale_key_check_cannot_save_reopened_card() {
        let now = Instant::now();
        let mut state = key_state(now);
        update(
            &mut state,
            ProvidersInput::SecretEdited { empty: false },
            now,
        );
        assert!(
            update(&mut state, ProvidersInput::SecretSubmitted, now)
                .iter()
                .any(|effect| matches!(effect, ProvidersEffect::CheckKey { .. }))
        );
        update(&mut state, ProvidersInput::Key(ProvidersKey::Esc), now);
        update(&mut state, ProvidersInput::Key(ProvidersKey::Enter), now);
        let effects = update(
            &mut state,
            ProvidersInput::KeyChecked(Ok(KeyCheck {
                ok: true,
                reachable: true,
                message: String::new(),
            })),
            now,
        );
        assert!(effects.is_empty());
        assert!(view(&state, now).secret_focus);
        assert!(update(&mut state, ProvidersInput::Key(ProvidersKey::Enter), now).is_empty());
    }

    #[test]
    fn stale_endpoint_check_cannot_fill_replacement_form() {
        let now = Instant::now();
        let mut state = ProvidersState {
            open: true,
            connected: true,
            tab: ProvidersTab::Endpoints,
            ..ProvidersState::default()
        };
        endpoints::open(&mut state, None, &mut Vec::new());
        if let Some(Card::Form(form)) = &mut state.card {
            form.draft.name = "Example".into();
            form.draft.base_url = "https://example.invalid".into();
        }
        endpoints::request(&mut state, false, &mut Vec::new());
        update(&mut state, ProvidersInput::Key(ProvidersKey::Esc), now);
        update(&mut state, ProvidersInput::Key(ProvidersKey::Enter), now);
        let effects = update(
            &mut state,
            ProvidersInput::EndpointChecked(Ok(EndpointCheck {
                ok: true,
                reachable: true,
                message: String::new(),
                models: vec!["old-model".into()],
                model_details: Vec::new(),
                resolved_base_url: None,
            })),
            now,
        );
        assert!(effects.is_empty());
        assert!(
            !view(&state, now)
                .card
                .unwrap()
                .lines
                .iter()
                .any(|line| line.contains("old-model"))
        );
    }

    #[test]
    fn successful_key_check_does_not_write_after_disconnect() {
        let now = Instant::now();
        let mut state = key_state(now);
        update(
            &mut state,
            ProvidersInput::SecretEdited { empty: false },
            now,
        );
        update(&mut state, ProvidersInput::SecretSubmitted, now);
        update(
            &mut state,
            ProvidersInput::Connection { connected: false },
            now,
        );
        assert!(
            update(
                &mut state,
                ProvidersInput::KeyChecked(Ok(KeyCheck {
                    ok: true,
                    reachable: true,
                    message: String::new()
                })),
                now
            )
            .is_empty()
        );
        assert!(view(&state, now).banner.is_some());
    }

    #[test]
    fn load_requests_collapse_to_one_followup() {
        let now = Instant::now();
        let mut state = key_state(now);
        for _ in 0..3 {
            update(
                &mut state,
                ProvidersInput::Key(ProvidersKey::Action(ProvidersAction::Reload)),
                now,
            );
        }
        let effects = update(
            &mut state,
            ProvidersInput::OptionsLoaded(Ok(hermes_contract::schemas::ModelOptionsResult {
                model: String::new(),
                provider: String::new(),
                providers: Vec::new(),
            })),
            now,
        );
        assert_eq!(effects, vec![ProvidersEffect::LoadOptions]);
    }

    #[test]
    fn countdown_expires_and_cancels_once() {
        let now = Instant::now();
        let account = Account {
            id: "example".into(),
            name: "Example".into(),
            device_code: true,
            cli_command: None,
            docs_url: None,
            disconnect_hint: None,
            disconnect_command: None,
            disconnectable: true,
            status: AccountStatus::NotSignedIn,
        };
        let mut state = ProvidersState {
            open: true,
            connected: true,
            ..ProvidersState::default()
        };
        let mut effects = Vec::new();
        accounts::start(&mut state, account, &mut effects);
        let attempt = state.attempt;
        update(
            &mut state,
            ProvidersInput::SignInStarted {
                attempt,
                result: Ok(SignInStart {
                    session_id: "one".into(),
                    user_code: "CODE".into(),
                    verification_url: "https://example.invalid".into(),
                    expires_in: Some(2),
                    poll_interval: Some(0),
                }),
            },
            now,
        );
        assert_eq!(next_deadline(&state), Some(now + Duration::from_secs(1)));
        let effects = update(
            &mut state,
            ProvidersInput::Tick,
            now + Duration::from_secs(2),
        );
        assert_eq!(
            effects,
            vec![ProvidersEffect::CancelSignIn {
                session_id: "one".into()
            }]
        );
        assert_eq!(next_deadline(&state), None);
        assert!(
            update(
                &mut state,
                ProvidersInput::Tick,
                now + Duration::from_secs(3)
            )
            .is_empty()
        );
    }
}
