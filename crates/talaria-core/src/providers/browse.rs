//! Provider identity, setup methods and presentation facts for the shared picker.
use super::{AccountStatus, EnvRow, ProvidersState, ProvidersTab, keys, text};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowseKey {
    Provider(String),
    Endpoint(String),
    AddEndpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupMethod {
    Key,
    Account,
    Endpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderStatus {
    ListedReady,
    KeySaved,
    NeedsSetup,
    Checking,
    Unknown,
}

impl ProviderStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::ListedReady => "listed ready",
            Self::KeySaved => "key saved",
            Self::NeedsSetup => "needs setup",
            Self::Checking => "checking",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowseRow {
    pub key: Option<BrowseKey>,
    pub label: String,
    pub status: Option<ProviderStatus>,
    pub methods: Vec<SetupMethod>,
    pub details: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrowseCounts {
    pub providers: usize,
    pub complete: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrowseView {
    pub rows: Vec<BrowseRow>,
    pub selected: Option<usize>,
    pub counts: BrowseCounts,
    pub details_open: bool,
    pub details_scroll: usize,
}

pub(super) fn key(state: &ProvidersState, slug: &str) -> Option<EnvRow> {
    state
        .keys
        .iter()
        .find(|r| r.slug == slug)
        .cloned()
        .or_else(|| {
            let p = keys::option(state, slug)?;
            if p.auth_type.as_deref() != Some("api_key") {
                return None;
            }
            Some(EnvRow {
                env_var: p.key_env.as_ref().filter(|v| !v.is_empty())?.clone(),
                slug: slug.into(),
                label: name(state, slug),
                primary: true,
                is_password: true,
                ..EnvRow::default()
            })
        })
}

fn name(state: &ProvidersState, slug: &str) -> String {
    keys::option(state, slug)
        .filter(|p| !p.name.is_empty())
        .map(|p| p.name.clone())
        .or_else(|| {
            state
                .keys
                .iter()
                .find(|r| r.slug == slug)
                .map(|r| r.label.clone())
        })
        .or_else(|| {
            state
                .accounts
                .iter()
                .find(|a| a.id == slug)
                .map(|a| a.name.clone())
        })
        .unwrap_or_else(|| slug.into())
}

fn loaded(state: &ProvidersState, index: usize) -> bool {
    state.loads[index].done && state.loads[index].error.is_none()
}

fn provider(state: &ProvidersState, slug: &str, tab: ProvidersTab) -> BrowseRow {
    let key_row = key(state, slug);
    let account = state.accounts.iter().find(|a| a.id == slug);
    let status = if !state.connected
        && let Some(status) = state
            .known_statuses
            .get(&(state.profile.clone(), slug.into()))
    {
        *status
    } else if loaded(state, 1)
        && keys::option(state, slug).is_some_and(|p| p.authenticated == Some(true))
    {
        ProviderStatus::ListedReady
    } else if loaded(state, 0) && key_row.as_ref().is_some_and(|r| r.is_set) {
        ProviderStatus::KeySaved
    } else if loaded(state, 0) && loaded(state, 1) {
        ProviderStatus::NeedsSetup
    } else if [0, 1].into_iter().any(|i| state.loads[i].running) {
        ProviderStatus::Checking
    } else {
        ProviderStatus::Unknown
    };
    let label = match tab {
        ProvidersTab::Keys => key_row
            .as_ref()
            .map(|r| r.label.clone())
            .unwrap_or_else(|| name(state, slug)),
        ProvidersTab::Accounts => account
            .map(|a| a.name.clone())
            .unwrap_or_else(|| name(state, slug)),
        _ => name(state, slug),
    };
    let mut methods = Vec::new();
    if key_row.is_some() && tab != ProvidersTab::Accounts {
        methods.push(SetupMethod::Key);
    }
    if account.is_some() && tab != ProvidersTab::Keys {
        methods.push(SetupMethod::Account);
    }
    let mut details = vec![
        format!("Provider: {label}"),
        format!("Status: {}", status.label()),
    ];
    if !state.connected {
        details.push("Offline; changes disabled until reconnect".into());
    }
    if status == ProviderStatus::ListedReady {
        details.push("Catalogue authentication; reachability unknown".into());
    }
    let offered = methods
        .iter()
        .map(|method| match method {
            SetupMethod::Key => "API key",
            SetupMethod::Account => "account sign-in",
            SetupMethod::Endpoint => "endpoint",
        })
        .collect::<Vec<_>>();
    details.push(format!(
        "Methods: {}",
        if offered.is_empty() {
            "set up outside Talaria".into()
        } else {
            offered.join(", ")
        }
    ));
    if !loaded(state, 2) {
        details.push("Account methods and sign-in state: unknown".into());
    } else if let Some(account) = account {
        details.push(format!(
            "Account: {}",
            match &account.status {
                AccountStatus::SignedIn { .. } => "signed in",
                AccountStatus::NotSignedIn => "not signed in",
                AccountStatus::Unknown(_) => "unknown",
            }
        ));
        details.push(format!(
            "{} · {} · {}",
            account.name,
            super::accounts::status(account),
            if account.device_code {
                "sign in here".into()
            } else {
                format!(
                    "sign in with {}",
                    account.cli_command.as_deref().unwrap_or("")
                )
            }
        ));
    }
    if let Some(row) = key_row {
        details.push(format!("Key setting: {}", row.env_var));
        details.push(format!(
            "{} · {} · {}",
            row.label,
            row.env_var,
            keys::status(state, &row)
        ));
    }
    if let Some(p) = keys::option(state, slug) {
        if let Some(warning) = &p.warning {
            details.push(format!("Warning: {warning}"));
        }
        if let Some(total) = p.total_models.filter(|n| *n > p.models.len() as i64) {
            details.push(format!("Catalogue: showing {} of {total}", p.models.len()));
        }
    }
    if state.uncertain {
        details.push("Outcome unknown; reloading without repeating the write".into());
    }
    details.push("F2/Ctrl+E full details".into());
    BrowseRow {
        key: Some(BrowseKey::Provider(slug.into())),
        label: text::sanitize(&label),
        status: Some(status),
        methods,
        details: details.iter().map(|line| text::sanitize(line)).collect(),
    }
}

fn header(label: &str) -> BrowseRow {
    BrowseRow {
        key: None,
        label: label.into(),
        status: None,
        methods: Vec::new(),
        details: Vec::new(),
    }
}

pub fn view(state: &ProvidersState) -> BrowseView {
    let mut rows = Vec::new();
    let mut slugs = Vec::new();
    match state.tab {
        ProvidersTab::All => {
            if let Some(options) = &state.options {
                slugs.extend(options.providers.iter().map(|p| p.slug.clone()));
            }
            // Unlisted inventory rows follow source inventory order, never the key tab's sort.
            slugs.extend(
                state
                    .env
                    .iter()
                    .filter(|r| {
                        state
                            .keys
                            .iter()
                            .any(|k| k.env_var == r.env_var && k.slug == r.slug)
                    })
                    .map(|r| r.slug.clone()),
            );
            slugs.extend(state.accounts.iter().map(|a| a.id.clone()));
        }
        ProvidersTab::Keys => slugs.extend(state.keys.iter().map(|r| r.slug.clone())),
        ProvidersTab::Accounts => slugs.extend(state.accounts.iter().map(|a| a.id.clone())),
        ProvidersTab::Endpoints => {}
    }
    let mut seen = std::collections::BTreeSet::new();
    let providers: Vec<_> = slugs
        .into_iter()
        .filter(|slug| seen.insert(slug.clone()))
        .map(|slug| provider(state, &slug, state.tab))
        .filter(|r| {
            text::matches(
                &state.filter,
                &format!(
                    "{} {} {}",
                    r.label,
                    match &r.key {
                        Some(BrowseKey::Provider(slug)) => slug.as_str(),
                        _ => "",
                    },
                    if state.tab == ProvidersTab::Accounts {
                        String::new()
                    } else {
                        r.key
                            .as_ref()
                            .and_then(|key| match key {
                                BrowseKey::Provider(slug) => {
                                    super::browse::key(state, slug).map(|r| r.env_var)
                                }
                                _ => None,
                            })
                            .unwrap_or_default()
                    }
                ),
            )
        })
        .collect();
    let count = providers.len();
    if state.tab == ProvidersTab::All {
        for (label, ready) in [("Configured", true), ("Needs setup", false)] {
            let group: Vec<_> = providers
                .iter()
                .filter(|row| (row.status == Some(ProviderStatus::ListedReady)) == ready)
                .cloned()
                .collect();
            if !group.is_empty() {
                rows.push(header(label));
                rows.extend(group);
            }
        }
    } else {
        rows.extend(providers);
    }
    if matches!(state.tab, ProvidersTab::All | ProvidersTab::Endpoints) {
        let mut endpoints = Vec::new();
        if text::matches(&state.filter, "+ Add an endpoint") {
            endpoints.push(BrowseRow {
                key: Some(BrowseKey::AddEndpoint),
                label: "+ Add an endpoint".into(),
                status: None,
                methods: vec![SetupMethod::Endpoint],
                details: vec!["Add a user-defined endpoint; Enter opens its form".into()],
            });
        }
        for endpoint in &state.endpoints.endpoints {
            if !text::matches(&state.filter, &format!("{} {}", endpoint.name, endpoint.id)) {
                continue;
            }
            let mut details = vec![
                format!("Endpoint: {}", endpoint.name),
                format!("URL: {}", endpoint.base_url),
                format!("Model: {}", endpoint.model),
            ];
            details.push(format!(
                "{} · {} · {}{}{}{}",
                endpoint.name,
                endpoint.base_url,
                endpoint.model,
                if endpoint.is_current {
                    " · default"
                } else {
                    ""
                },
                if endpoint.has_api_key {
                    " · key saved"
                } else {
                    ""
                },
                if endpoint.source == "custom_providers" {
                    " · older format"
                } else {
                    ""
                }
            ));
            if !state.connected {
                details.push("Offline; changes disabled".into());
            }
            endpoints.push(BrowseRow {
                key: Some(BrowseKey::Endpoint(endpoint.id.clone())),
                label: text::sanitize(&endpoint.name),
                status: endpoint.has_api_key.then_some(ProviderStatus::KeySaved),
                methods: vec![SetupMethod::Endpoint],
                details: details.iter().map(|s| text::sanitize(s)).collect(),
            });
        }
        if !endpoints.is_empty() {
            if state.tab == ProvidersTab::All {
                rows.push(header("Endpoints"));
            }
            rows.extend(endpoints);
        }
    }
    let selected = if state.tab == ProvidersTab::All {
        state
            .all_selected
            .as_ref()
            .and_then(|key| rows.iter().position(|r| r.key.as_ref() == Some(key)))
    } else {
        let key = match state.tab {
            ProvidersTab::Keys => state
                .keys
                .get(state.selected)
                .map(|r| BrowseKey::Provider(r.slug.clone())),
            ProvidersTab::Accounts => state
                .accounts
                .get(state.selected)
                .map(|a| BrowseKey::Provider(a.id.clone())),
            ProvidersTab::Endpoints => {
                if state.selected == 0 {
                    Some(BrowseKey::AddEndpoint)
                } else {
                    state
                        .endpoints
                        .endpoints
                        .get(state.selected - 1)
                        .map(|e| BrowseKey::Endpoint(e.id.clone()))
                }
            }
            ProvidersTab::All => None,
        };
        key.and_then(|key| rows.iter().position(|r| r.key.as_ref() == Some(&key)))
    }
    .or_else(|| rows.iter().position(|r| r.key.is_some()));
    let needed: &[usize] = match state.tab {
        ProvidersTab::Keys => &[0],
        ProvidersTab::Accounts => &[2],
        ProvidersTab::All => &[0, 1, 2],
        ProvidersTab::Endpoints => &[3],
    };
    BrowseView {
        rows,
        selected,
        counts: BrowseCounts {
            providers: count,
            complete: needed.iter().all(|i| loaded(state, *i)),
        },
        details_open: state.details_open,
        details_scroll: state.details_scroll,
    }
}
