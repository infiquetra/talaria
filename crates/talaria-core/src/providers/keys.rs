use super::{allowed, load, load_all, model::*, open_card, text};
use hermes_contract::schemas::ModelOptionProvider;

pub(crate) fn tail(preview: &str) -> Option<String> {
    if preview.chars().count() == 4 {
        return Some(preview.into());
    }
    let body = preview.strip_prefix("«redacted:")?.strip_suffix('»')?;
    let chars: Vec<_> = body.chars().collect();
    if chars.len() == 11 && chars[4..7] == ['.', '.', '.'] {
        Some(chars[7..].iter().collect())
    } else {
        None
    }
}

pub(crate) fn option<'a>(state: &'a ProvidersState, slug: &str) -> Option<&'a ModelOptionProvider> {
    state
        .options
        .as_ref()?
        .providers
        .iter()
        .find(|p| p.slug == slug)
}
pub(crate) fn rebuild(state: &mut ProvidersState) {
    let selected = state.keys.get(state.selected).map(|r| r.env_var.clone());
    state.keys = state
        .env
        .iter()
        .filter(|r| {
            !r.slug.is_empty()
                && !r.env_var.is_empty()
                && r.primary
                && r.is_password
                && !r.channel_managed
                && !r.custom
        })
        .cloned()
        .collect();
    state.keys.sort_by(|a, b| {
        b.is_set
            .cmp(&a.is_set)
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });
    state.selected = selected
        .and_then(|v| state.keys.iter().position(|r| r.env_var == v))
        .unwrap_or_else(|| state.selected.min(state.keys.len().saturating_sub(1)));
}
pub(crate) fn status(state: &ProvidersState, row: &EnvRow) -> String {
    let key = row
        .tail
        .as_ref()
        .filter(|tail| tail.chars().count() == 4)
        .map(|t| format!("key …{t}"))
        .unwrap_or_else(|| "key saved".into());
    let saved = if row.tail.as_ref().is_some_and(|t| t.chars().count() == 4) {
        format!("{key} saved")
    } else {
        key.clone()
    };
    if state.loads[1].running || !state.loads[1].done {
        return format!(
            "{} · checking…",
            if row.is_set { saved.as_str() } else { "no key" }
        );
    }
    if state.loads[1].error.is_some() {
        return if row.is_set { saved } else { "no key".into() };
    }
    if let Some(p) = option(state, &row.slug).filter(|p| p.authenticated == Some(true)) {
        if !row.is_set {
            return "ready · credential not in this profile's key file".into();
        }
        let n = p.total_models.unwrap_or(p.models.len() as i64);
        return format!(
            "ready{} · {key}",
            if n > 0 {
                format!(" · {n} models")
            } else {
                String::new()
            }
        );
    }
    if row.is_set {
        format!("{saved} · not ready")
    } else {
        "no key".into()
    }
}
pub(crate) fn open(state: &mut ProvidersState, row: EnvRow, effects: &mut Vec<ProvidersEffect>) {
    open_card(
        state,
        Card::Key(KeyCard {
            row,
            empty: true,
            unchecked: false,
            message: None,
        }),
        effects,
    )
}
pub(crate) fn submit(state: &mut ProvidersState, effects: &mut Vec<ProvidersEffect>) {
    if !matches!(&state.card,Some(Card::Key(card)) if !card.empty) {
        return;
    }
    if !allowed(state) {
        return;
    }
    let Some(Card::Key(card)) = &state.card else {
        return;
    };
    if card.unchecked {
        state.busy = Some(Busy::SaveKey(card.row.clone()));
        effects.push(ProvidersEffect::SaveKey {
            slug: card.row.slug.clone(),
        })
    } else {
        state.busy = Some(Busy::CheckKey);
        effects.push(ProvidersEffect::CheckKey {
            env_var: card.row.env_var.clone(),
        })
    }
}
pub(crate) fn checked(
    state: &mut ProvidersState,
    result: Result<KeyCheck, Failure>,
    effects: &mut Vec<ProvidersEffect>,
) {
    if matches!(state.busy, Some(Busy::DiscardedKeyCheck)) {
        state.busy = None;
        return;
    }
    if !matches!(state.busy, Some(Busy::CheckKey)) {
        return;
    }
    state.busy = None;
    let Some(Card::Key(card)) = &mut state.card else {
        return;
    };
    match result {
        Ok(check) if check.ok => {
            if state.connected {
                state.busy = Some(Busy::SaveKey(card.row.clone()));
                effects.push(ProvidersEffect::SaveKey {
                    slug: card.row.slug.clone(),
                });
            }
        }
        Ok(check) if check.reachable => {
            card.message = Some(text::sanitize(&check.message));
            card.unchecked = false
        }
        Ok(_) => {
            card.message=Some("Could not reach the provider to check the key. Press enter again to save it anyway.".into());
            card.unchecked = true
        }
        Err(error) => {
            card.message = Some(format!(
                "Could not check the key ({}). Press enter again to save it anyway.",
                text::check_reason(&error, "/api/providers/validate").trim_end_matches('.')
            ));
            card.unchecked = true
        }
    }
}
pub(crate) fn saved(
    state: &mut ProvidersState,
    result: Result<ModelOptionProvider, Failure>,
    effects: &mut Vec<ProvidersEffect>,
) {
    let row = match state.busy.take() {
        Some(Busy::SaveKey(row)) => row,
        other => {
            state.busy = other;
            return;
        }
    };
    match result {
        Ok(provider) => {
            for key in state
                .keys
                .iter_mut()
                .filter(|key| key.env_var == row.env_var)
            {
                key.is_set = true;
            }
            for key in state
                .env
                .iter_mut()
                .filter(|key| key.env_var == row.env_var)
            {
                key.is_set = true;
            }
            if let Some(options) = &mut state.options {
                if let Some(p) = options
                    .providers
                    .iter_mut()
                    .find(|p| p.slug == provider.slug)
                {
                    *p = provider
                } else {
                    options.providers.push(provider)
                }
            }
            state.card = None;
            effects.push(ProvidersEffect::ForgetSecret);
            state.notice = Some(format!("Saved the key for {}.", text::sanitize(&row.label)));
            load(state, 0, effects)
        }
        Err(error) => {
            let message = save_error(&error, &row);
            if matches!(error, Failure::Unknown | Failure::BadReply) {
                state.card = None;
                effects.push(ProvidersEffect::ForgetSecret);
                load(state, 0, effects);
                load(state, 1, effects)
            }
            if let Some(Card::Key(card)) = &mut state.card {
                card.message = Some(message)
            } else {
                state.notice = Some(message)
            }
        }
    }
}
fn save_error(error: &Failure, row: &EnvRow) -> String {
    match error {
        Failure::Unknown | Failure::BadReply => text::unknown("key"),
        Failure::Refused { code, message } => match code {
            4001 => "Hermes needs both a provider and a key.".into(),
            4002 => format!("Hermes does not know the provider {}.", text::sanitize(&row.slug)),
            4003 => format!("{} signs in with an account, not a key. Use the Accounts tab.", text::sanitize(&row.label)),
            4004 => format!("Hermes has no key setting for {}.", text::sanitize(&row.label)),
            4006 => "This Hermes install is managed by a package manager; its keys are read-only.".into(),
            5098 => "The gateway runs older code than is installed. Restart the gateway (outside Talaria), then try again.".into(),
            -32601 => "This Hermes does not offer model.save_key on the gateway. Use hermes model in a terminal.".into(),
            _ => format!("Hermes could not save the key: {}", text::sentence(message)),
        },
        _ => text::failure(error, ""),
    }
}
pub(crate) fn remove(state: &mut ProvidersState, row: EnvRow, effects: &mut Vec<ProvidersEffect>) {
    if !allowed(state) {
        return;
    }
    let label = text::sanitize(&row.label);
    if !row.is_set {
        if state.loads[1].running || !state.loads[1].done {
            state.notice = Some(format!("Still checking {label}; try again in a moment."));
            return;
        }
        if state.loads[1].error.is_some() {
            state.notice = Some(format!(
                "Could not check {label}. Press ctrl+r, then try again."
            ));
            return;
        }
        if option(state, &row.slug).is_none_or(|p| p.authenticated != Some(true)) {
            state.notice = Some(format!("Nothing to remove for {label}."));
            return;
        }
    }
    let vars: Vec<_> = state
        .env
        .iter()
        .filter(|r| r.is_set && (r.slug == row.slug || r.profile_slugs.contains(&row.slug)))
        .collect();
    let mut title = format!(
        "Remove every saved credential for {label} from profile {}? ",
        text::sanitize(&state.profile)
    );
    if vars.is_empty() {
        title.push_str(
            "This deletes any API key Hermes holds for it and any account sign-in for it.",
        )
    } else {
        title.push_str(&format!(
            "This deletes its API keys (including {}) and any account sign-in for it.",
            vars.iter()
                .map(|r| text::sanitize(&r.env_var))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for var in vars {
        let others = var
            .shared_with
            .iter()
            .cloned()
            .chain(
                state
                    .env
                    .iter()
                    .filter(|r| r.env_var == var.env_var && r.slug != row.slug)
                    .map(|r| (r.slug.clone(), r.label.clone())),
            )
            .chain(var.profile_slugs.iter().map(|slug| {
                (
                    slug.clone(),
                    option(state, slug)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| slug.clone()),
                )
            }));
        for (slug, name) in others {
            if slug != row.slug && seen.insert((slug, var.env_var.clone())) {
                title.push_str(&format!(
                    " {} uses {} too and will lose it.",
                    text::sanitize(&name),
                    text::sanitize(&var.env_var)
                ))
            }
        }
    }
    open_card(
        state,
        Card::Confirm {
            title,
            effect: ProvidersEffect::Disconnect { slug: row.slug },
        },
        effects,
    )
}
pub(crate) fn disconnected(
    state: &mut ProvidersState,
    result: Result<(), Failure>,
    effects: &mut Vec<ProvidersEffect>,
) {
    let label = match state.busy.take() {
        Some(Busy::Disconnect(label)) => label,
        other => {
            state.busy = other;
            return;
        }
    };
    state.card = None;
    state.notice = Some(match result {
        Ok(()) => format!("Removed the credentials for {}.", text::sanitize(&label)),
        Err(Failure::Unknown | Failure::BadReply) => text::unknown("removal"),
        Err(Failure::Refused { code: 4005, .. }) => format!("There were no saved credentials for {}.", text::sanitize(&label)),
        Err(Failure::Refused { code: -32601, .. }) => "This Hermes does not offer model.disconnect on the gateway. Use hermes model in a terminal.".into(),
        Err(error) => text::failure(&error, ""),
    });
    load_all(state, effects)
}
