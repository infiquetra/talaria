//! Shared steps for the providers overlay tests.

#![allow(dead_code)]

use std::time::Instant;

use hermes_contract::schemas::ModelOptionsResult;
use serde_json::{Value, json};
use talaria_core::providers::{
    Account, AccountStatus, Endpoint, EndpointDraft, EndpointList, EnvRow, OpenTarget,
    ProvidersEffect, ProvidersInput, ProvidersState, ProvidersTab, ProvidersView, update, view,
};

pub fn open(target: OpenTarget, connected: bool) -> (ProvidersState, Vec<ProvidersEffect>) {
    let mut state = ProvidersState::new();
    let effects = update(
        &mut state,
        ProvidersInput::Open {
            target,
            profile_name: "fixture".into(),
            connected,
        },
        Instant::now(),
    );
    (state, effects)
}

pub fn keys_tab() -> OpenTarget {
    OpenTarget::Tab(ProvidersTab::Keys)
}

pub fn step(state: &mut ProvidersState, input: ProvidersInput) -> Vec<ProvidersEffect> {
    update(state, input, Instant::now())
}

pub fn screen(state: &ProvidersState) -> ProvidersView {
    view(state, Instant::now())
}

pub fn joined(state: &ProvidersState) -> String {
    let shown = screen(state);
    let mut text = String::new();
    text.push_str(&shown.title);
    text.push('\n');
    text.push_str(&shown.rows.join("\n"));
    text.push('\n');
    text.push_str(&shown.footer);
    text.push('\n');
    text.push_str(&shown.filter);
    if let Some(card) = &shown.card {
        text.push('\n');
        text.push_str(&card.lines.join("\n"));
        text.push('\n');
        text.push_str(&card.footer);
    }
    if let Some(notice) = &shown.notice {
        text.push('\n');
        text.push_str(notice);
    }
    if let Some(banner) = &shown.banner {
        text.push('\n');
        text.push_str(banner);
    }
    text
}

pub fn env(var: &str, slug: &str, label: &str, set: bool) -> EnvRow {
    EnvRow {
        env_var: var.into(),
        slug: slug.into(),
        label: label.into(),
        is_set: set,
        tail: None,
        shared_with: Vec::new(),
        primary: true,
        is_password: true,
        channel_managed: false,
        custom: false,
        profile_slugs: Vec::new(),
    }
}

pub fn catalogue(providers: Vec<Value>) -> ModelOptionsResult {
    serde_json::from_value(json!({
        "model": "",
        "provider": "",
        "providers": providers,
    }))
    .expect("model options")
}

pub fn provider(slug: &str, name: &str, authenticated: bool, total: i64) -> Value {
    json!({
        "slug": slug,
        "name": name,
        "models": [],
        "authenticated": authenticated,
        "total_models": total,
        "auth_type": "api_key",
    })
}

pub fn account(id: &str, name: &str, device_code: bool, status: AccountStatus) -> Account {
    Account {
        id: id.into(),
        name: name.into(),
        device_code,
        cli_command: None,
        docs_url: None,
        disconnect_hint: None,
        disconnect_command: None,
        disconnectable: true,
        status,
    }
}

pub fn endpoint(id: &str, name: &str, url: &str, model: &str) -> Endpoint {
    Endpoint {
        id: id.into(),
        name: name.into(),
        base_url: url.into(),
        model: model.into(),
        models: Vec::new(),
        api_mode: Some("auto".into()),
        context_length: None,
        discover_models: true,
        has_api_key: false,
        is_current: false,
        source: "providers".into(),
    }
}

pub fn list_of(endpoints: Vec<Endpoint>) -> EndpointList {
    EndpointList {
        endpoints,
        current_provider: String::new(),
        current_model: String::new(),
    }
}

pub fn draft(name: &str, url: &str, model: &str) -> EndpointDraft {
    EndpointDraft {
        id: String::new(),
        name: name.into(),
        base_url: url.into(),
        model: model.into(),
        api_key: talaria_core::providers::ApiKeyChange::Keep,
        api_mode: Some(String::new()),
        context_length: None,
        discover_models: true,
        models: None,
        model_details: None,
    }
}

pub fn has(effects: &[ProvidersEffect], pred: impl Fn(&ProvidersEffect) -> bool) -> bool {
    effects.iter().any(pred)
}
