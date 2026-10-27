use super::{allowed, load, model::*, open_card, text};

const MODES: [&str; 4] = [
    "",
    "chat_completions",
    "codex_responses",
    "anthropic_messages",
];
pub(crate) fn open(
    state: &mut ProvidersState,
    endpoint: Option<Endpoint>,
    effects: &mut Vec<ProvidersEffect>,
) {
    if endpoint
        .as_ref()
        .is_some_and(|e| e.source == "direct-config")
    {
        state.notice=Some("This endpoint is set directly in the profile's model settings. Change it in Model roles.".into());
        return;
    }
    let draft = endpoint
        .as_ref()
        .map(|e| EndpointDraft {
            id: e.id.clone(),
            name: e.name.clone(),
            base_url: e.base_url.clone(),
            model: e.model.clone(),
            api_key: ApiKeyChange::Keep,
            api_mode: None,
            context_length: e.context_length,
            discover_models: e.discover_models,
            models: None,
            model_details: None,
        })
        .unwrap_or_else(|| EndpointDraft {
            api_mode: Some(String::new()),
            discover_models: true,
            ..EndpointDraft::default()
        });
    let context = draft
        .context_length
        .map(|n| n.to_string())
        .unwrap_or_default();
    open_card(
        state,
        Card::Form(Form {
            draft,
            context,
            field: 0,
            dirty: false,
            cleared: false,
            result: None,
            original: endpoint,
            mode_changed: false,
        }),
        effects,
    )
}
fn stale(form: &mut Form) {
    form.result = None;
    form.draft.models = None;
    form.draft.model_details = None
}
pub(crate) fn secret(form: &mut Form, empty: bool) {
    form.draft.api_key = if empty {
        if form.cleared {
            ApiKeyChange::Clear
        } else {
            ApiKeyChange::Keep
        }
    } else {
        ApiKeyChange::Typed
    };
    form.dirty = true;
    stale(form)
}
pub(crate) fn edit(form: &mut Form, key: ProvidersKey) {
    match key {
        ProvidersKey::Tab | ProvidersKey::Down | ProvidersKey::Enter => {
            form.field = (form.field + 1).min(5)
        }
        ProvidersKey::BackTab | ProvidersKey::Up => form.field = form.field.saturating_sub(1),
        ProvidersKey::Left | ProvidersKey::Right => {
            let forward = key == ProvidersKey::Right;
            match form.field {
                3 => {
                    let value = form
                        .draft
                        .api_mode
                        .as_deref()
                        .or_else(|| form.original.as_ref().and_then(|e| e.api_mode.as_deref()))
                        .unwrap_or("");
                    let value = if value == "auto" { "" } else { value };
                    if let Some(index) = MODES.iter().position(|m| *m == value) {
                        let next = if forward {
                            (index + 1) % MODES.len()
                        } else {
                            (index + MODES.len() - 1) % MODES.len()
                        };
                        form.draft.api_mode = Some(MODES[next].into());
                        form.mode_changed = true;
                        form.dirty = true;
                        stale(form)
                    }
                }
                4 => {
                    if let Some(models) = &form.draft.models
                        && !models.is_empty()
                    {
                        let index = models
                            .iter()
                            .position(|m| m == &form.draft.model)
                            .unwrap_or(0);
                        let next = if forward {
                            (index + 1) % models.len()
                        } else {
                            (index + models.len() - 1) % models.len()
                        };
                        form.draft.model = models[next].clone();
                        form.dirty = true
                    }
                }
                _ => {}
            }
        }
        ProvidersKey::Char(c) => append(form, &c.to_string()),
        ProvidersKey::Paste(s) => append(form, &s),
        ProvidersKey::Backspace => {
            let target = match form.field {
                0 => Some(&mut form.draft.name),
                1 => Some(&mut form.draft.base_url),
                4 => Some(&mut form.draft.model),
                5 => Some(&mut form.context),
                _ => None,
            };
            if let Some(value) = target {
                value.pop();
                form.dirty = true;
                if form.field == 1 {
                    stale(form)
                }
            }
        }
        _ => {}
    }
}
fn append(form: &mut Form, text: &str) {
    if text.chars().any(char::is_control) {
        return;
    }
    match form.field {
        0 => form.draft.name.push_str(text),
        1 => form.draft.base_url.push_str(text),
        4 => form.draft.model.push_str(text),
        5 if text.bytes().all(|c| c.is_ascii_digit()) => form.context.push_str(text),
        _ => return,
    }
    form.dirty = true;
    if form.field == 1 {
        stale(form)
    }
}
pub(crate) fn valid_url(url: &str) -> bool {
    let rest = if let Some(v) = url.strip_prefix("https://") {
        v
    } else if let Some(v) = url.strip_prefix("http://") {
        v
    } else {
        return false;
    };
    if rest.chars().any(|c| c.is_whitespace() || c.is_control()) || rest.contains(['\\', '#']) {
        return false;
    }
    let host = rest.split(['/', '?']).next().unwrap_or("");
    !host.is_empty()
        && !host.contains('@')
        && !host.starts_with(':')
        && if host.starts_with('[') {
            host.find(']').is_some_and(|i| {
                i > 1
                    && (host.len() == i + 1
                        || host[i + 1..]
                            .strip_prefix(':')
                            .is_some_and(|p| p.parse::<u16>().is_ok_and(|n| n > 0)))
            })
        } else {
            let mut parts = host.split(':');
            let first = parts.next().unwrap_or("");
            !first.is_empty()
                && parts
                    .next()
                    .is_none_or(|p| p.parse::<u16>().is_ok_and(|n| n > 0))
                && parts.next().is_none()
        }
}
fn validation(
    form: &mut Form,
    endpoints: &EndpointList,
    save: bool,
) -> Result<EndpointDraft, String> {
    let mut draft = form.draft.clone();
    draft.name = draft.name.trim().into();
    draft.base_url = draft.base_url.trim().into();
    draft.model = draft.model.trim().into();
    if draft.name.is_empty() {
        return Err("Enter a name for this endpoint.".into());
    }
    if form.original.is_none()
        && endpoints.endpoints.iter().any(|e| {
            e.id.to_lowercase() == draft.name.to_lowercase()
                || e.name.to_lowercase() == draft.name.to_lowercase()
        })
    {
        return Err(format!(
            "An endpoint named {} already exists; edit it instead.",
            text::sanitize(&draft.name)
        ));
    }
    if !valid_url(&draft.base_url) {
        return Err("Enter an absolute http or https Base URL with a host.".into());
    }
    if save && draft.model.is_empty() {
        return Err("Enter a model for this endpoint.".into());
    }
    draft.context_length = if form.context.is_empty() {
        None
    } else {
        match form.context.parse::<u64>() {
            Ok(n) if n > 0 => Some(n),
            _ => return Err("Context length must be an integer above 0.".into()),
        }
    };
    if !save {
        let mode = draft
            .api_mode
            .clone()
            .or_else(|| form.original.as_ref().and_then(|e| e.api_mode.clone()))
            .unwrap_or_default();
        let mode = if mode == "auto" { String::new() } else { mode };
        draft.api_mode = MODES.contains(&mode.as_str()).then_some(mode);
    } else if let Some(url) = form
        .result
        .as_ref()
        .and_then(|r| r.resolved_base_url.as_ref())
    {
        draft.base_url = url.clone()
    }
    Ok(draft)
}
pub(crate) fn request(state: &mut ProvidersState, save: bool, effects: &mut Vec<ProvidersEffect>) {
    if !allowed(state) {
        return;
    }
    let Some(Card::Form(form)) = &mut state.card else {
        return;
    };
    match validation(form, &state.endpoints, save) {
        Ok(draft) => {
            if save {
                state.busy = Some(Busy::SaveEndpoint(draft.name.clone()));
                effects.push(ProvidersEffect::SaveEndpoint(draft))
            } else {
                state.busy = Some(Busy::CheckEndpoint);
                effects.push(ProvidersEffect::CheckEndpoint(draft))
            }
        }
        Err(message) => state.notice = Some(message),
    }
}
pub(crate) fn checked(state: &mut ProvidersState, result: Result<EndpointCheck, Failure>) {
    if matches!(state.busy, Some(Busy::DiscardedEndpointCheck)) {
        state.busy = None;
        return;
    }
    if !matches!(state.busy, Some(Busy::CheckEndpoint)) {
        return;
    }
    state.busy = None;
    let Some(Card::Form(form)) = &mut state.card else {
        return;
    };
    match result {
        Ok(check) => {
            if !check.models.is_empty() {
                if form.draft.model.is_empty() {
                    form.draft.model = check.models[0].clone()
                }
                form.draft.models = Some(check.models.clone())
            }
            form.draft.model_details = Some(check.model_details.clone());
            form.result = Some(check)
        }
        Err(error) => {
            state.notice = Some(text::failure(
                &error,
                "/api/providers/custom-endpoints/validate",
            ))
        }
    }
}
pub(crate) fn remove(
    state: &mut ProvidersState,
    endpoint: Endpoint,
    effects: &mut Vec<ProvidersEffect>,
) {
    if !allowed(state) {
        return;
    }
    if endpoint.source == "direct-config" {
        state.notice=Some("This endpoint is set directly in the profile's model settings. Change it in Model roles.".into());
        return;
    }
    let mut title = format!(
        "Delete the endpoint {} ({}) from profile {}? Its saved key is removed too.",
        text::sanitize(&endpoint.name),
        text::sanitize(&endpoint.base_url),
        text::sanitize(&state.profile)
    );
    if endpoint.is_current {
        title.push_str(" It is the profile's default provider, so the default provider is cleared.")
    }
    open_card(
        state,
        Card::Confirm {
            title,
            effect: ProvidersEffect::DeleteEndpoint { id: endpoint.id },
        },
        effects,
    )
}
pub(crate) fn saved(
    state: &mut ProvidersState,
    result: Result<EndpointList, Failure>,
    delete: bool,
    effects: &mut Vec<ProvidersEffect>,
) {
    let name = match state.busy.take() {
        Some(Busy::SaveEndpoint(name)) if !delete => name,
        Some(Busy::DeleteEndpoint(name)) if delete => name,
        other => {
            state.busy = other;
            return;
        }
    };
    match result {
        Ok(list) => {
            state.endpoints = list;
            state.card = None;
            effects.push(ProvidersEffect::ForgetSecret);
            state.notice = Some(format!(
                "{} {}.",
                if delete { "Deleted" } else { "Saved" },
                text::sanitize(&name)
            ))
        }
        Err(error) => {
            state.notice = Some(match &error {
                Failure::Unknown | Failure::BadReply => text::unknown("endpoint change"),
                Failure::Http { status: 404, .. } if delete => {
                    format!("Hermes no longer has {}.", text::sanitize(&name))
                }
                _ => text::failure(&error, "/api/providers/custom-endpoints"),
            });
            if matches!(error, Failure::Unknown | Failure::BadReply)
                | matches!(error, Failure::Http { status: 404, .. })
            {
                state.card = None;
                effects.push(ProvidersEffect::ForgetSecret);
                load(state, 3, effects)
            }
        }
    }
}
pub(crate) fn lines(form: &Form) -> Vec<String> {
    let mode = form
        .draft
        .api_mode
        .as_deref()
        .or_else(|| form.original.as_ref().and_then(|e| e.api_mode.as_deref()))
        .unwrap_or("");
    let mode = if mode.is_empty() || mode == "auto" {
        "auto".into()
    } else if MODES.contains(&mode) {
        mode.into()
    } else {
        format!("{mode} (set outside Talaria)")
    };
    let key = match form.draft.api_key {
        ApiKeyChange::Keep if form.original.as_ref().is_some_and(|e| e.has_api_key) => {
            "saved · type to replace · ctrl+x clears"
        }
        ApiKeyChange::Clear => "cleared",
        _ => "",
    };
    let mut lines = vec![
        format!("Name: {}", form.draft.name),
        format!("Base URL: {}", form.draft.base_url),
        format!("API key: {key}"),
        format!("API mode: {mode}"),
        format!("Model: {}", form.draft.model),
        format!("Context length: {}", form.context),
    ];
    if form.context.is_empty()
        && form
            .original
            .as_ref()
            .is_some_and(|e| e.context_length.is_some())
    {
        lines.push("Hermes keeps the old context length when this is empty.".into())
    }
    if form.original.as_ref().is_some_and(|e| e.has_api_key)
        && form.draft.api_key != ApiKeyChange::Typed
    {
        lines.push("Testing without the saved key; type the key to test with it.".into())
    }
    if let Some(check) = &form.result {
        let url = check
            .resolved_base_url
            .as_deref()
            .unwrap_or(&form.draft.base_url);
        if check.ok {
            lines.push(format!(
                "Reached {url}: {} model{}.",
                check.models.len(),
                if check.models.len() == 1 { "" } else { "s" }
            ));
        } else {
            lines.push(check.message.clone())
        }
        if check
            .resolved_base_url
            .as_ref()
            .is_some_and(|u| u != &form.draft.base_url)
        {
            lines.push(format!(
                "Talaria will save {url}, the address that answered."
            ));
        }
    }
    lines
}
