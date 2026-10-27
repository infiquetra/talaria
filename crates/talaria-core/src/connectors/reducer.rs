//! Pure transitions for the two settings tabs.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use super::*;
use crate::attachments::sanitize;

const ACCOUNTS: u8 = 1;
const LIST: u8 = 2;
const POLICY: u8 = 4;
const PLATFORMS: u8 = 8;
const PAIRING: u8 = 16;

pub(super) fn name(state: &ConnectorsState, slug: &str) -> String {
    state
        .catalog
        .iter()
        .find(|row| row.slug == slug)
        .map_or_else(|| slug.to_owned(), |row| row.name.clone())
}

pub(super) fn allows(body: &PolicyBody, slug: &str) -> bool {
    match body.mode.as_str() {
        "unrestricted" => true,
        "deny-all" => false,
        "allow" => body.connectors.iter().any(|entry| entry == slug),
        "deny" => !body.disabled_connectors.iter().any(|entry| entry == slug),
        _ => false,
    }
}

pub(super) fn member(state: &ConnectorsState) -> Option<&PolicyLayer> {
    state
        .policy
        .as_ref()?
        .layers
        .iter()
        .find(|layer| layer.kind == "member")
}

pub(super) fn member_on(state: &ConnectorsState, slug: &str) -> bool {
    member(state).is_none_or(|layer| allows(&layer.body, slug))
}

pub(super) fn org_locked(state: &ConnectorsState, slug: &str) -> bool {
    state.policy.as_ref().is_some_and(|policy| {
        policy.layers.iter().any(|layer| {
            matches!(layer.kind.as_str(), "org" | "role") && !allows(&layer.body, slug)
        })
    })
}

pub(super) fn tool_locked(state: &ConnectorsState, slug: &str, tool: &ToolSpec) -> bool {
    state.policy.as_ref().is_some_and(|policy| {
        policy.layers.iter().any(|layer| {
            if !matches!(layer.kind.as_str(), "org" | "role") {
                return false;
            }
            layer
                .body
                .tools
                .get(slug)
                .is_some_and(|tools| tools.contains(&tool.slug))
                || layer.body.tags.as_ref().is_some_and(|tags| {
                    tags.disable.iter().any(|tag| tool.hints.contains(tag))
                        || (!tags.enable.is_empty()
                            && !tags.enable.iter().any(|tag| tool.hints.contains(tag)))
                })
        })
    })
}

pub(super) fn disabled(state: &ConnectorsState, slug: &str) -> BTreeSet<String> {
    if let Some(policy) = &state.policy {
        policy
            .layers
            .iter()
            .find(|layer| layer.kind == "member")
            .and_then(|layer| layer.body.tools.get(slug))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect()
    } else {
        state
            .connectors
            .iter()
            .find(|row| row.connector == slug)
            .map(|row| row.gateway_disabled_tools.clone())
            .unwrap_or_default()
            .into_iter()
            .collect()
    }
}

pub(super) fn joined(state: &ConnectorsState) -> Vec<ConnectorRow> {
    let mut rows = state.connectors.clone();
    for catalog in &state.catalog {
        if !rows.iter().any(|row| row.connector == catalog.slug) {
            rows.push(ConnectorRow {
                connector: catalog.slug.clone(),
                enabled: true,
                connected: false,
                connection_status: None,
                status_reason: None,
                gateway_disabled_tools: Vec::new(),
                org_locked: false,
                listed: false,
            });
        }
    }
    for row in &mut rows {
        row.org_locked = org_locked(state, &row.connector);
        row.enabled &= member_on(state, &row.connector);
    }
    let words: Vec<_> = state
        .filter
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    rows.retain(|row| {
        let catalog = state.catalog.iter().find(|item| item.slug == row.connector);
        let haystack = format!(
            "{} {} {} {}",
            name(state, &row.connector),
            row.connector,
            catalog
                .and_then(|item| item.category.as_deref())
                .unwrap_or_default(),
            catalog
                .and_then(|item| item.description.as_deref())
                .unwrap_or_default()
        )
        .to_lowercase();
        words.iter().all(|word| haystack.contains(word))
    });
    rows.sort_by_key(|row| {
        (
            name(state, &row.connector).to_lowercase(),
            row.connector.clone(),
        )
    });
    rows
}

pub(super) fn platform_rows(state: &ConnectorsState) -> Vec<&PlatformRow> {
    let words: Vec<_> = state
        .filter
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    state
        .platforms
        .iter()
        .filter(|row| {
            let haystack = format!("{} {} {}", row.name, row.id, row.description).to_lowercase();
            words.iter().all(|word| haystack.contains(word))
        })
        .collect()
}

pub(super) fn platform(state: &ConnectorsState) -> Option<&PlatformRow> {
    if let Pane::Platform(id) = &state.pane {
        state.platforms.iter().find(|row| &row.id == id)
    } else {
        platform_rows(state).get(state.selected).copied()
    }
}

fn slug(state: &ConnectorsState) -> Option<String> {
    match &state.pane {
        Pane::Connector(slug) | Pane::Tools(slug) => Some(slug.clone()),
        _ => joined(state)
            .get(state.selected)
            .map(|row| row.connector.clone()),
    }
}

pub(super) fn account_rows(state: &ConnectorsState, slug: &str) -> AccountView {
    account_view(
        &state
            .accounts
            .iter()
            .filter(|row| row.connector == slug)
            .cloned()
            .collect::<Vec<_>>(),
        state.show_retired,
    )
}

fn account(state: &ConnectorsState) -> Option<&AccountRow> {
    let slug = slug(state)?;
    let rows = account_rows(state, &slug);
    let selected = rows.rows.get(state.card_selected)?;
    state
        .accounts
        .iter()
        .find(|row| row.connection_id == selected.connection_id)
}

pub(super) fn fields<'a>(state: &ConnectorsState, platform: &'a PlatformRow) -> Vec<&'a EnvVar> {
    platform
        .env_vars
        .iter()
        .filter(|field| !field.advanced || state.advanced)
        .collect()
}

fn notice(state: &mut ConnectorsState, text: impl Into<String>) {
    let mut text = text.into();
    let mut sensitive = Vec::new();
    sensitive.extend(
        state
            .accounts
            .iter()
            .filter(|account| account.alias.as_deref() != Some(&account.label))
            .map(|account| account.label.as_str()),
    );
    for pair in &state.pairing.pending {
        sensitive.push(&pair.user_id);
        if let Some(name) = &pair.user_name {
            sensitive.push(name);
        }
    }
    for pair in &state.pairing.approved {
        sensitive.push(&pair.user_id);
        if let Some(name) = &pair.user_name {
            sensitive.push(name);
        }
    }
    if let Some(link) = &state.link {
        sensitive.push(link);
    }
    for value in sensitive {
        if !value.is_empty() {
            text = text.replace(value, "[redacted]");
        }
    }
    state.notice = Some(Line {
        text: sanitize(&text),
        tone: Tone::Normal,
    });
}

fn command(state: &ConnectorsState) -> String {
    format!("hermes -p {} auth upgrade", state.profile)
}

fn error_text(
    state: &ConnectorsState,
    failure: &ConnectorsFailure,
    method: &str,
    id: &str,
) -> String {
    match failure {
        ConnectorsFailure::NotSent(message) => message.clone(),
        ConnectorsFailure::Unknown | ConnectorsFailure::BadReply => {
            "No answer from Hermes; the change may or may not have been made. Reloading.".into()
        }
        ConnectorsFailure::Http { status: 401, .. } => {
            "Hermes refused Talaria's ticket. Run /doctor.".into()
        }
        ConnectorsFailure::Http { status: 403, .. } => {
            "Hermes refused: the request named another profile.".into()
        }
        ConnectorsFailure::Http { status: 404, .. } => {
            format!("Hermes does not know {id}. ctrl+r reloads.")
        }
        ConnectorsFailure::Http { status, .. } if *status >= 500 => {
            format!("Hermes reported an error (HTTP {status}). Reloading to show what was saved.")
        }
        ConnectorsFailure::Http { status, detail } => detail
            .clone()
            .unwrap_or_else(|| format!("Hermes refused (HTTP {status}).")),
        ConnectorsFailure::Refused { code: -32601, .. } => {
            format!("This Hermes does not offer {method}. Check the version with /doctor.")
        }
        ConnectorsFailure::Refused { code: 4032, .. } => {
            format!("Sign in to use connectors: {}", command(state))
        }
        ConnectorsFailure::Refused {
            reason,
            message,
            code,
        } => match reason.as_deref() {
            Some("ALIAS_TAKEN") => "That name is already used.".into(),
            Some("CONNECTION_NOT_FOUND") => "That account no longer exists. Reloaded.".into(),
            Some("POLICY_CONFLICT") => {
                "Connector rules changed elsewhere. Reloaded; try again.".into()
            }
            Some("ORG_REQUIRED") => "Select an organization to manage connector rules.".into(),
            Some("LINK_STILL_VALID") => {
                "The last link still works. Open it from the card, or wait until it expires.".into()
            }
            _ if *code == 5034 => {
                format!("Hermes could not reach the connector service: {message}")
            }
            _ => message.clone(),
        },
    }
}

fn read_error(
    state: &ConnectorsState,
    failure: &ConnectorsFailure,
    method: &str,
    id: &str,
) -> String {
    match failure {
        ConnectorsFailure::Unknown => "No answer from Hermes.".into(),
        ConnectorsFailure::BadReply => "The reply from Hermes was invalid.".into(),
        _ => error_text(state, failure, method, id),
    }
}

fn reloads(state: &ConnectorsState, mask: u8) -> Vec<ConnectorsEffect> {
    let mut effects = Vec::new();
    if mask & ACCOUNTS != 0 && state.consented {
        effects.push(ConnectorsEffect::LoadAccounts);
    }
    if mask & POLICY != 0 && state.consented {
        effects.push(ConnectorsEffect::LoadPolicy);
    }
    if mask & LIST != 0 && state.consented {
        effects.push(ConnectorsEffect::ListConnectors);
    }
    if mask & PAIRING != 0 {
        effects.push(ConnectorsEffect::LoadPairing);
    }
    if mask & PLATFORMS != 0 {
        effects.push(ConnectorsEffect::LoadPlatforms);
    }
    effects
}

fn hold_reload(state: &mut ConnectorsState, mask: u8) -> Vec<ConnectorsEffect> {
    state.awaiting = mask;
    if mask == 0 {
        state.write = None;
    }
    reloads(state, mask)
}

fn loaded(state: &mut ConnectorsState, bit: u8) {
    if state.awaiting & bit == 0 {
        return;
    }
    state.awaiting &= !bit;
    if state.awaiting == 0 && state.write.is_some() {
        let write = state.write.take();
        if write == Some(WriteKind::SetConnector)
            && let Some(slug) = state.write_slug.clone()
        {
            let app = name(state, &slug);
            let on = member_on(state, &slug);
            if on
                && state
                    .connectors
                    .iter()
                    .any(|row| row.connector == slug && !row.enabled)
            {
                notice(
                    state,
                    format!("Saved, but Hermes still lists {app} as off."),
                );
            } else {
                let status = if on { "on" } else { "off" };
                notice(
                    state,
                    format!("{app} is {status} for profile {}.", state.profile),
                );
            }
        }
    }
}

fn failed_write(
    state: &mut ConnectorsState,
    failure: ConnectorsFailure,
    method: &str,
    id: &str,
    mask: u8,
) -> Vec<ConnectorsEffect> {
    notice(state, error_text(state, &failure, method, id));
    let reload = match &failure {
        ConnectorsFailure::Unknown | ConnectorsFailure::BadReply => mask,
        ConnectorsFailure::Refused { reason, .. }
            if reason.as_deref() == Some("POLICY_CONFLICT") =>
        {
            POLICY
        }
        ConnectorsFailure::Refused { reason, .. }
            if reason.as_deref() == Some("CONNECTION_NOT_FOUND") =>
        {
            ACCOUNTS
        }
        ConnectorsFailure::Http { status, .. } if *status >= 500 => PLATFORMS,
        _ => 0,
    };
    // Failed writes never produce a saved-state notice during their readback.
    if reload != 0 && state.write == Some(WriteKind::SetConnector) {
        state.write = Some(WriteKind::SetTools);
    }
    hold_reload(state, reload)
}

fn can_write(state: &mut ConnectorsState) -> bool {
    if state.write.is_some() {
        notice(state, "Waiting for Hermes.");
        false
    } else {
        true
    }
}

fn policy_revision(
    state: &mut ConnectorsState,
    slug: &str,
    connector_switch: bool,
) -> Option<String> {
    if state.policy.is_none() {
        notice(
            state,
            format!(
                "Connector rules are not available right now: {}",
                state.policy_error.as_deref().unwrap_or("not loaded")
            ),
        );
        return None;
    }
    let Some(revision) = member(state).and_then(|layer| layer.revision.clone()) else {
        notice(state, "Connector rules are read-only for this account.");
        return None;
    };
    if connector_switch && org_locked(state, slug) {
        notice(
            state,
            format!(
                "Your organization turned {} off; your switch cannot change that.",
                name(state, slug)
            ),
        );
        return None;
    }
    Some(revision)
}

fn start_connect(
    state: &mut ConnectorsState,
    slug: String,
    address: AccountAddress,
) -> Vec<ConnectorsEffect> {
    if state.operation.is_some() {
        notice(
            state,
            format!("Already waiting for {}.", name(state, &slug)),
        );
        return Vec::new();
    }
    if !can_write(state) || !state.consented {
        return Vec::new();
    }
    state.connect_alias = match &address {
        AccountAddress::Alias(alias) => Some(alias.clone()),
        AccountAddress::Repair { connection_id } => state
            .accounts
            .iter()
            .find(|row| &row.connection_id == connection_id)
            .and_then(|row| row.alias.clone()),
    };
    state.connect_slug = Some(slug.clone());
    state.write = Some(WriteKind::Connect);
    state.pane = Pane::Link;
    vec![ConnectorsEffect::Connect { slug, address }]
}

fn stop_link(state: &mut ConnectorsState, text: Option<String>) -> Vec<ConnectorsEffect> {
    if let Some(text) = text {
        notice(state, text);
    }
    state.operation = None;
    state.link = None;
    state.next_poll = None;
    state.next_frame = None;
    state.poll_in_flight = false;
    state.wake_in_flight = false;
    state.wake_poll_pending = false;
    if state.pane == Pane::Link {
        state.pane = state
            .connect_slug
            .clone()
            .map_or(Pane::List, Pane::Connector);
    }
    reloads(state, ACCOUNTS | LIST)
}

fn operation_reply(
    state: &mut ConnectorsState,
    operation: Operation,
    now: Instant,
) -> Vec<ConnectorsEffect> {
    let target = operation.targets.first();
    if let Some(link) = target.and_then(|target| target.connect_url.clone()) {
        state.link = Some(link);
    }
    let finished = operation.settled
        || target.is_some_and(|target| {
            matches!(
                target.state.as_str(),
                "connected" | "expired" | "failed" | "not_connected" | "skipped"
            )
        });
    if finished {
        let app = name(
            state,
            state
                .connect_slug
                .as_deref()
                .unwrap_or_else(|| target.map_or("", |target| target.name.as_str())),
        );
        let text = if target.is_some_and(|target| target.state == "connected") {
            state.connect_alias.as_ref().map_or_else(
                || format!("An unnamed {app} account is connected."),
                |alias| format!("{app} account {alias} is connected."),
            )
        } else {
            target
                .and_then(|target| target.detail.clone())
                .unwrap_or_else(|| format!("{app} did not connect. Try again."))
        };
        return stop_link(state, Some(text));
    }
    state.operation = Some(operation);
    state.poll_in_flight = false;
    state.next_poll = Some(now + Duration::from_secs(2));
    state.next_frame = Some(now + Duration::from_secs(1));
    if std::mem::take(&mut state.wake_poll_pending) {
        poll(state)
    } else {
        Vec::new()
    }
}

fn poll(state: &mut ConnectorsState) -> Vec<ConnectorsEffect> {
    if state.poll_in_flight || state.wake_in_flight || !state.consented {
        return Vec::new();
    }
    let Some(op) = &state.operation else {
        return Vec::new();
    };
    state.poll_in_flight = true;
    vec![ConnectorsEffect::PollOperation {
        op_id: op.op_id.clone(),
    }]
}

pub(super) fn update(
    state: &mut ConnectorsState,
    input: ConnectorsInput,
    now: Instant,
) -> Vec<ConnectorsEffect> {
    match input {
        ConnectorsInput::Open {
            open: ConnectorsOpen::Tab(tab),
            profile,
            consented,
        } => {
            if state.write.is_none() || state.profile != profile {
                *state = ConnectorsState::new();
            }
            state.profile = profile;
            state.tab = tab;
            state.opened_tab = tab;
            state.consented = consented;
            state.closed = false;
            state.list_followups = tab == ConnectorsTab::Connectors && consented;
            if tab == ConnectorsTab::Messaging {
                vec![
                    ConnectorsEffect::LoadPlatforms,
                    ConnectorsEffect::LoadPairing,
                ]
            } else if consented {
                vec![ConnectorsEffect::ListConnectors]
            } else {
                Vec::new()
            }
        }
        ConnectorsInput::Key(key) => key_input(state, key, now),
        ConnectorsInput::Paste(text) => {
            edit_text(state, &text.replace('\n', ","));
            Vec::new()
        }
        ConnectorsInput::ListReply(reply) => {
            let result = match reply {
                Ok(reply) => {
                    state.loaded = true;
                    state.unavailable = None;
                    state.connectors = reply.connectors;
                    if reply.available {
                        if state.consented && std::mem::take(&mut state.list_followups) {
                            vec![
                                ConnectorsEffect::LoadCatalog,
                                ConnectorsEffect::LoadAccounts,
                                ConnectorsEffect::LoadPolicy,
                            ]
                        } else {
                            Vec::new()
                        }
                    } else {
                        state.unavailable = Some((
                            format!(
                                "Connectors are not available for profile {}. Hermes offers them when tools.connectors is on and the profile is signed in to Nous or may use a guest identity (nous.guest). To sign in: {}",
                                state.profile,
                                command(state)
                            ),
                            true,
                        ));
                        Vec::new()
                    }
                }
                Err(failure) => {
                    let text = if matches!(failure, ConnectorsFailure::Refused { code: 4031, .. }) {
                        format!(
                            "Connectors are not available for profile {}. To sign in: {}",
                            state.profile,
                            command(state)
                        )
                    } else {
                        read_error(state, &failure, "connectors.list", "")
                    };
                    let copy = matches!(
                        failure,
                        ConnectorsFailure::Refused {
                            code: 4031 | 4032,
                            ..
                        }
                    );
                    state.unavailable = Some((
                        if copy {
                            text
                        } else {
                            format!("Could not load connectors: {text}. ctrl+r tries again.")
                        },
                        copy,
                    ));
                    Vec::new()
                }
            };
            loaded(state, LIST);
            result
        }
        ConnectorsInput::CatalogReply(reply) => {
            if let Ok(rows) = reply {
                state.catalog = rows;
            } else {
                state.catalog.clear();
            }
            Vec::new()
        }
        ConnectorsInput::AccountsReply(reply) => {
            match reply {
                Ok(rows) => {
                    state.accounts = rows;
                    state.accounts_error = None;
                }
                Err(failure) => {
                    let text = read_error(state, &failure, "connectors.accounts", "");
                    if matches!(failure, ConnectorsFailure::Refused { code: 4032, .. }) {
                        state.unavailable = Some((text.clone(), true));
                    }
                    state.accounts.clear();
                    state.accounts_error = Some(text);
                }
            }
            loaded(state, ACCOUNTS);
            Vec::new()
        }
        ConnectorsInput::PolicyReply(reply) => {
            match reply {
                Ok(policy) => {
                    state.policy = Some(policy);
                    state.policy_error = None;
                }
                Err(failure) => {
                    state.policy = None;
                    state.policy_error =
                        Some(read_error(state, &failure, "connectors.policy.get", ""));
                }
            }
            loaded(state, POLICY);
            Vec::new()
        }
        ConnectorsInput::ToolsReply { slug, reply } => {
            match reply {
                Ok(tools) => {
                    state.tools = Some(tools);
                    state.pane = Pane::Tools(slug);
                    state.card_selected = 0;
                }
                Err(failure) => notice(
                    state,
                    read_error(state, &failure, "connectors.tools", &slug),
                ),
            }
            Vec::new()
        }
        ConnectorsInput::ConnectReply(reply) => match reply {
            Ok(operation) => {
                state.write = None;
                state.expired_polled = false;
                operation_reply(state, operation, now)
            }
            Err(failure) => {
                state.pane = state
                    .connect_slug
                    .clone()
                    .map_or(Pane::List, Pane::Connector);
                failed_write(state, failure, "connectors.connect", "", ACCOUNTS | LIST)
            }
        },
        ConnectorsInput::OperationReply { op_id, reply } => {
            if state.operation.as_ref().is_none_or(|op| op.op_id != op_id) {
                return Vec::new();
            }
            state.poll_in_flight = false;
            match reply {
                Ok(operation) => operation_reply(state, operation, now),
                Err(
                    ConnectorsFailure::Unknown
                    | ConnectorsFailure::NotSent(_)
                    | ConnectorsFailure::BadReply,
                ) => {
                    state.next_poll = Some(now + Duration::from_secs(2));
                    Vec::new()
                }
                Err(ConnectorsFailure::Refused { code, .. }) if code >= 5000 => {
                    state.next_poll = Some(now + Duration::from_secs(2));
                    Vec::new()
                }
                Err(ConnectorsFailure::Refused { code: 4004, .. }) => stop_link(state, None),
                Err(failure) => {
                    let text = error_text(state, &failure, "connectors.operation.status", "");
                    stop_link(state, Some(text))
                }
            }
        }
        ConnectorsInput::WakeReply(reply) => {
            state.wake_in_flight = false;
            if let Err(failure) = reply {
                notice(
                    state,
                    error_text(state, &failure, "connectors.operation.wake", ""),
                );
            }
            if state.poll_in_flight {
                state.wake_poll_pending = true;
                Vec::new()
            } else {
                poll(state)
            }
        }
        ConnectorsInput::ConnectionUpdate {
            op_id,
            owner_is_account,
        } => {
            if owner_is_account
                && state
                    .operation
                    .as_ref()
                    .is_some_and(|operation| operation.op_id == op_id)
            {
                poll(state)
            } else {
                Vec::new()
            }
        }
        ConnectorsInput::RenameReply(reply) => match reply {
            Ok(account) => {
                notice(
                    state,
                    format!(
                        "Renamed to {} ({}).",
                        account.alias.as_deref().unwrap_or_default(),
                        name(state, &account.connector)
                    ),
                );
                state.prompt = None;
                hold_reload(state, ACCOUNTS)
            }
            Err(failure) => {
                failed_write(state, failure, "connectors.accounts.rename", "", ACCOUNTS)
            }
        },
        ConnectorsInput::RemoveReply(reply) => match reply {
            Ok(_) => {
                if let Some(account) = state.write_account.clone() {
                    let app = name(state, &account.connector);
                    notice(
                        state,
                        account.alias.map_or_else(
                            || format!("Account removed from {app}."),
                            |alias| format!("Removed {app} account {alias}."),
                        ),
                    );
                }
                hold_reload(state, ACCOUNTS | LIST)
            }
            Err(failure) => failed_write(
                state,
                failure,
                "connectors.accounts.remove",
                "",
                ACCOUNTS | LIST,
            ),
        },
        ConnectorsInput::PolicySetReply(reply) => {
            let mask = if state.write == Some(WriteKind::SetConnector) {
                POLICY | LIST
            } else {
                POLICY
            };
            match reply {
                Ok(_) => {
                    state.tool_drafts.clear();
                    hold_reload(state, mask)
                }
                Err(failure) => failed_write(state, failure, "connectors.policy.set", "", mask),
            }
        }
        ConnectorsInput::PlatformsLoaded(reply) => {
            let reply = reply.map(|reply| {
                state.gateway_start_command = reply.gateway_start_command;
                reply.platforms
            });
            update(state, ConnectorsInput::PlatformsReply(reply), now)
        }
        ConnectorsInput::PlatformsReply(reply) => {
            match reply {
                Ok(mut rows) => {
                    for row in &mut rows {
                        for field in &mut row.env_vars {
                            if !field.is_list {
                                field.value = None;
                            }
                        }
                    }
                    for row in &rows {
                        for field in &row.env_vars {
                            if state.drafts.get(&row.id).is_some_and(|draft| {
                                draft.typed.contains(&field.key)
                                    || draft.clear.contains(&field.key)
                                    || draft.lists.contains_key(&field.key)
                            }) && state
                                .platforms
                                .iter()
                                .find(|old| old.id == row.id)
                                .and_then(|old| {
                                    old.env_vars.iter().find(|old| old.key == field.key)
                                })
                                .is_some_and(|old| old.is_set != field.is_set)
                            {
                                state.changed_fields.insert(field.key.clone());
                            }
                        }
                    }
                    state.platforms = rows;
                    state.platforms_loaded = true;
                }
                Err(failure) => notice(
                    state,
                    format!(
                        "Could not load messaging platforms: {}. ctrl+r tries again.",
                        read_error(state, &failure, "", "")
                    ),
                ),
            }
            loaded(state, PLATFORMS);
            Vec::new()
        }
        ConnectorsInput::PairingReply(reply) => {
            match reply {
                Ok(pairing) => {
                    state.pairing = pairing;
                    state.pairing_error = None;
                }
                Err(failure) => state.pairing_error = Some(read_error(state, &failure, "", "")),
            }
            loaded(state, PAIRING);
            Vec::new()
        }
        ConnectorsInput::UpdateReply { id, reply } => match reply {
            Ok(reply) => {
                let was_toggle = state.toggle_enabled.is_some();
                let prefix = state.toggle_enabled.take().map_or_else(
                    || "Saved.".into(),
                    |enabled| {
                        format!(
                            "{} is {}.",
                            platform_name(state, &id),
                            if enabled { "on" } else { "off" }
                        )
                    },
                );
                state.restart_notice = !reply.hot_served;
                notice(
                    state,
                    format!(
                        "{prefix} {}",
                        if reply.hot_served {
                            "The running gateway is connecting with the new settings."
                        } else {
                            "This takes effect after a gateway restart. R opens the doctor."
                        }
                    ),
                );
                if !was_toggle {
                    let submitted = std::mem::take(&mut state.submitted_versions);
                    for (key, version) in submitted {
                        if state.draft_versions.get(&(id.clone(), key.clone())) == Some(&version) {
                            if let Some(draft) = state.drafts.get_mut(&id) {
                                draft.typed.remove(&key);
                                draft.lists.remove(&key);
                                draft.clear.remove(&key);
                            }
                            state.draft_versions.remove(&(id.clone(), key.clone()));
                            state.changed_fields.remove(&key);
                        }
                    }
                }
                let mut effects = if was_toggle
                    || state.secret_editor.is_some()
                    || state.drafts.values().any(|draft| !draft.typed.is_empty())
                {
                    Vec::new()
                } else {
                    vec![ConnectorsEffect::ForgetSecrets]
                };
                effects.extend(hold_reload(state, PLATFORMS));
                effects
            }
            Err(failure) => failed_write(state, failure, "", &id, PLATFORMS),
        },
        ConnectorsInput::TestReply { id, reply } => {
            state.testing.remove(&id);
            match reply {
                Ok(reply) => {
                    notice(
                        state,
                        format!(
                            "Test: {}{}",
                            reply.message,
                            if reply.auth_posture.as_deref() == Some("open") {
                                " (anyone can message the bot)"
                            } else {
                                ""
                            }
                        ),
                    );
                    if let Some(line) = &mut state.notice {
                        line.tone = if !reply.ok {
                            Tone::Error
                        } else if reply.auth_posture.as_deref() == Some("deny_all") {
                            Tone::Warning
                        } else {
                            Tone::Normal
                        };
                    }
                }
                Err(failure) => {
                    notice(state, read_error(state, &failure, "", &id));
                    if matches!(failure, ConnectorsFailure::Http { status, .. } if status >= 500) {
                        return vec![ConnectorsEffect::LoadPlatforms];
                    }
                }
            }
            Vec::new()
        }
        ConnectorsInput::ApproveReply(reply) => pairing_reply(state, reply, true),
        ConnectorsInput::RevokeReply(reply) => pairing_reply(state, reply, false),
        ConnectorsInput::PlatformsChanged => change_signal(state, false, now),
        ConnectorsInput::PairingChanged => change_signal(state, true, now),
        ConnectorsInput::SecretEdited { key, empty } => {
            if let Some(editor) = &state.secret_editor {
                if editor == &key {
                    state.secret_edit_empty = empty;
                }
            } else if !empty {
                commit_secret_draft(state, &key);
            }
            Vec::new()
        }
        ConnectorsInput::Tick => {
            state.next_frame = state
                .operation
                .as_ref()
                .map(|_| now + Duration::from_secs(1));
            let Some(operation) = &state.operation else {
                return Vec::new();
            };
            if now >= operation.deadline {
                if state.expired_polled {
                    return stop_link(state, Some("Stopped waiting: the link expired. The list shows where the account stands.".into()));
                }
                state.expired_polled = true;
                return poll(state);
            }
            state.next_frame = Some(now + Duration::from_secs(1));
            if state.next_poll.is_some_and(|deadline| now >= deadline) {
                poll(state)
            } else {
                Vec::new()
            }
        }
        ConnectorsInput::Reconnected => {
            if state.closed {
                return Vec::new();
            }
            if state.tab == ConnectorsTab::Messaging {
                vec![
                    ConnectorsEffect::LoadPlatforms,
                    ConnectorsEffect::LoadPairing,
                ]
            } else if state.consented {
                state.list_followups = true;
                vec![ConnectorsEffect::ListConnectors]
            } else {
                Vec::new()
            }
        }
    }
}

fn platform_name(state: &ConnectorsState, id: &str) -> String {
    state
        .platforms
        .iter()
        .find(|row| row.id == id)
        .map_or_else(|| id.to_owned(), |row| row.name.clone())
}

fn pairing_reply(
    state: &mut ConnectorsState,
    reply: Result<(), ConnectorsFailure>,
    approve: bool,
) -> Vec<ConnectorsEffect> {
    match reply {
        Ok(()) => {
            if approve {
                notice(
                    state,
                    format!(
                        "Pairing approved on {}.",
                        platform_name(state, state.write_platform.as_deref().unwrap_or_default())
                    ),
                );
            } else {
                notice(
                    state,
                    "Pairing approval removed. Other access rules may still let this person message the bot.",
                );
            }
            hold_reload(state, PAIRING | PLATFORMS)
        }
        Err(ConnectorsFailure::Http { status: 404, .. }) => {
            notice(
                state,
                if approve {
                    "That pairing request is gone or expired. Reloaded."
                } else {
                    "That pairing approval was already removed. Reloaded."
                },
            );
            hold_reload(state, PAIRING)
        }
        Err(failure) => failed_write(state, failure, "", "", PAIRING | PLATFORMS),
    }
}

fn change_signal(
    state: &mut ConnectorsState,
    pairing: bool,
    now: Instant,
) -> Vec<ConnectorsEffect> {
    if state.tab != ConnectorsTab::Messaging || state.write.is_some() || state.closed {
        return Vec::new();
    }
    let due = |last: Option<Instant>| {
        last.is_none_or(|last| now.saturating_duration_since(last) >= Duration::from_secs(2))
    };
    let mut effects = Vec::new();
    if pairing && due(state.last_pairing_signal) {
        state.last_pairing_signal = Some(now);
        effects.push(ConnectorsEffect::LoadPairing);
    }
    if due(state.last_platform_signal) {
        state.last_platform_signal = Some(now);
        effects.push(ConnectorsEffect::LoadPlatforms);
    }
    effects
}

fn edit_text(state: &mut ConnectorsState, text: &str) {
    match state.prompt.as_mut() {
        Some(
            Prompt::Alias { text: value, .. }
            | Prompt::Rename { text: value, .. }
            | Prompt::List { text: value, .. },
        ) => value.push_str(text),
        None if state.filtering => {
            state.filter.push_str(text);
            state.selected = 0;
        }
        _ => {}
    }
}

fn key_input(
    state: &mut ConnectorsState,
    key: ConnectorsKey,
    now: Instant,
) -> Vec<ConnectorsEffect> {
    if state.secret_editor.is_some() {
        if key == ConnectorsKey::Open && !state.secret_edit_empty {
            if let Some(key) = state.secret_editor.take() {
                commit_secret_draft(state, &key);
            }
            state.secret_edit_empty = true;
        }
        if matches!(key, ConnectorsKey::Close | ConnectorsKey::Cancel) {
            state.secret_editor = None;
            state.secret_edit_empty = true;
        }
        return Vec::new();
    }
    if state.prompt.is_some() {
        return prompt_key(state, key);
    }
    if state.filtering {
        match key {
            ConnectorsKey::Char(ch) => edit_text(state, &ch.to_string()),
            ConnectorsKey::Backspace => {
                state.filter.pop();
                state.selected = 0;
            }
            ConnectorsKey::Close | ConnectorsKey::Cancel => {
                state.filter.clear();
                state.filtering = false;
            }
            ConnectorsKey::Open => {
                state.filtering = false;
                return open_selected(state);
            }
            _ => {}
        }
        return Vec::new();
    }
    if state.tab == ConnectorsTab::Connectors && !state.consented {
        return match key {
            ConnectorsKey::Open => {
                state.consented = true;
                state.list_followups = true;
                vec![
                    ConnectorsEffect::GrantConsent,
                    ConnectorsEffect::ListConnectors,
                ]
            }
            ConnectorsKey::NextTab | ConnectorsKey::PrevTab => switch_tab(state),
            ConnectorsKey::Close | ConnectorsKey::Cancel => {
                if state.opened_tab == ConnectorsTab::Messaging {
                    switch_tab(state)
                } else {
                    state.closed = true;
                    vec![ConnectorsEffect::Close]
                }
            }
            _ => Vec::new(),
        };
    }
    if matches!(
        key,
        ConnectorsKey::Add
            | ConnectorsKey::Rename
            | ConnectorsKey::Remove
            | ConnectorsKey::Toggle
            | ConnectorsKey::Save
            | ConnectorsKey::Approve
    ) && !can_write(state)
    {
        return Vec::new();
    }
    match key {
        ConnectorsKey::NextTab | ConnectorsKey::PrevTab => switch_tab(state),
        ConnectorsKey::Up | ConnectorsKey::Down => {
            let count = match &state.pane {
                Pane::List if state.tab == ConnectorsTab::Connectors => joined(state).len(),
                Pane::List => platform_rows(state).len(),
                Pane::Connector(slug) => account_rows(state, slug).rows.len(),
                Pane::Tools(_) => state.tools.as_ref().map_or(0, |tools| tools.tools.len()),
                Pane::Platform(id) => {
                    state
                        .platforms
                        .iter()
                        .find(|row| &row.id == id)
                        .map_or(0, |platform| {
                            fields(state, platform).len()
                                + state
                                    .pairing
                                    .pending
                                    .iter()
                                    .filter(|pair| &pair.platform == id)
                                    .count()
                                + state
                                    .pairing
                                    .approved
                                    .iter()
                                    .filter(|pair| &pair.platform == id)
                                    .count()
                        })
                }
                Pane::Link => 0,
            };
            let selected = if state.pane == Pane::List {
                &mut state.selected
            } else {
                &mut state.card_selected
            };
            *selected = if key == ConnectorsKey::Up {
                selected.saturating_sub(1)
            } else {
                (*selected + 1).min(count.saturating_sub(1))
            };
            Vec::new()
        }
        ConnectorsKey::Filter if state.pane == Pane::List => {
            state.filtering = true;
            Vec::new()
        }
        ConnectorsKey::Open => open_selected(state),
        ConnectorsKey::Reload => match &state.pane {
            Pane::Tools(slug) => vec![ConnectorsEffect::LoadTools {
                slug: slug.clone(),
                refresh: true,
            }],
            _ if state.tab == ConnectorsTab::Messaging => {
                vec![
                    ConnectorsEffect::LoadPlatforms,
                    ConnectorsEffect::LoadPairing,
                ]
            }
            _ => {
                state.list_followups = true;
                vec![ConnectorsEffect::ListConnectors]
            }
        },
        ConnectorsKey::Toggle if matches!(state.pane, Pane::Tools(_)) => toggle_tool(state),
        ConnectorsKey::Toggle if state.tab == ConnectorsTab::Connectors => {
            let Some(slug) = slug(state) else {
                return Vec::new();
            };
            let Some(revision) = policy_revision(state, &slug, true) else {
                return Vec::new();
            };
            let enabled = !member_on(state, &slug);
            state.write = Some(WriteKind::SetConnector);
            state.write_slug = Some(slug.clone());
            vec![ConnectorsEffect::SetConnector {
                slug,
                enabled,
                expected_revision: revision,
            }]
        }
        ConnectorsKey::Toggle if platform(state).is_some() => {
            state.prompt = Some(Prompt::Confirm(ConfirmKind::TogglePlatform));
            Vec::new()
        }
        ConnectorsKey::Add if matches!(state.pane, Pane::Connector(_)) => {
            let Some(slug) = slug(state) else {
                return Vec::new();
            };
            if !state
                .connectors
                .iter()
                .any(|row| row.connector == slug && row.listed)
            {
                notice(
                    state,
                    format!("{} is not offered to this profile.", name(state, &slug)),
                );
            } else if state.operation.is_some() {
                notice(
                    state,
                    format!("Already waiting for {}.", name(state, &slug)),
                );
            } else {
                state.prompt = Some(Prompt::Alias {
                    slug,
                    text: String::new(),
                });
            }
            Vec::new()
        }
        ConnectorsKey::Rename if matches!(state.pane, Pane::Connector(_)) => {
            if let Some(account) = account(state).cloned() {
                state.prompt = Some(Prompt::Rename {
                    connection_id: account.connection_id,
                    text: account.alias.unwrap_or_default(),
                });
            }
            Vec::new()
        }
        ConnectorsKey::Remove if matches!(state.pane, Pane::Connector(_)) => {
            if account(state).is_some() {
                state.prompt = Some(Prompt::Confirm(ConfirmKind::RemoveAccount));
            }
            Vec::new()
        }
        ConnectorsKey::ReconnectOrCopy => reconnect_copy(state),
        ConnectorsKey::Retired => {
            state.show_retired = !state.show_retired;
            state.card_selected = 0;
            Vec::new()
        }
        ConnectorsKey::ToolsOrTest if state.tab == ConnectorsTab::Connectors => {
            let Some(slug) = slug(state) else {
                return Vec::new();
            };
            if !member_on(state, &slug) {
                notice(
                    state,
                    format!("Turn {} on before choosing its tools.", name(state, &slug)),
                );
                Vec::new()
            } else {
                vec![ConnectorsEffect::LoadTools {
                    slug,
                    refresh: false,
                }]
            }
        }
        ConnectorsKey::ToolsOrTest => {
            if let Some(platform) = platform(state).cloned()
                && state.testing.insert(platform.id.clone())
            {
                return vec![ConnectorsEffect::TestPlatform { id: platform.id }];
            }
            Vec::new()
        }
        ConnectorsKey::OpenLink => open_link(state),
        ConnectorsKey::Wake if state.pane == Pane::Link => {
            if state.wake_in_flight {
                return Vec::new();
            }
            if let Some(operation) = &state.operation {
                state.wake_in_flight = true;
                vec![ConnectorsEffect::WakeOperation {
                    op_id: operation.op_id.clone(),
                }]
            } else {
                Vec::new()
            }
        }
        ConnectorsKey::Save => save_review(state),
        ConnectorsKey::ClearField if matches!(state.pane, Pane::Platform(_)) => {
            if let Some(platform) = platform(state).cloned()
                && let Some(field) = fields(state, &platform).get(state.card_selected)
                && field.is_set
            {
                let draft = state.drafts.entry(platform.id.clone()).or_default();
                if !draft.clear.remove(&field.key) {
                    draft.clear.insert(field.key.clone());
                }
                changed_draft(state, &platform.id, &field.key);
            }
            Vec::new()
        }
        ConnectorsKey::Advanced => {
            state.advanced = !state.advanced;
            state.card_selected = 0;
            Vec::new()
        }
        ConnectorsKey::Approve if matches!(state.pane, Pane::Platform(_)) => {
            pairing_confirm(state, true)
        }
        ConnectorsKey::Remove if matches!(state.pane, Pane::Platform(_)) => {
            pairing_confirm(state, false)
        }
        ConnectorsKey::Doctor
            if state.tab == ConnectorsTab::Messaging
                && matches!(state.pane, Pane::List | Pane::Platform(_))
                && (state.restart_notice
                    || state
                        .platforms
                        .iter()
                        .any(|row| row.state.as_deref() == Some("pending_restart"))) =>
        {
            vec![ConnectorsEffect::OpenDoctor]
        }
        ConnectorsKey::Close | ConnectorsKey::Cancel => close(state),
        _ => {
            let _ = now;
            Vec::new()
        }
    }
}

fn switch_tab(state: &mut ConnectorsState) -> Vec<ConnectorsEffect> {
    state.tab = if state.tab == ConnectorsTab::Connectors {
        ConnectorsTab::Messaging
    } else {
        ConnectorsTab::Connectors
    };
    state.pane = if state.tab == ConnectorsTab::Connectors && state.operation.is_some() {
        Pane::Link
    } else {
        Pane::List
    };
    state.selected = 0;
    state.filter.clear();
    if state.tab == ConnectorsTab::Messaging {
        if matches!(
            state.write,
            Some(WriteKind::UpdatePlatform | WriteKind::Approve | WriteKind::Revoke)
        ) {
            Vec::new()
        } else {
            vec![
                ConnectorsEffect::LoadPlatforms,
                ConnectorsEffect::LoadPairing,
            ]
        }
    } else if state.consented && !state.loaded {
        state.list_followups = true;
        vec![ConnectorsEffect::ListConnectors]
    } else {
        Vec::new()
    }
}

fn open_selected(state: &mut ConnectorsState) -> Vec<ConnectorsEffect> {
    match state.pane {
        Pane::List if state.tab == ConnectorsTab::Connectors => {
            if let Some(slug) = slug(state) {
                state.pane = Pane::Connector(slug);
                state.card_selected = 0;
            }
        }
        Pane::List => {
            if let Some(platform) = platform(state) {
                state.pane = Pane::Platform(platform.id.clone());
                state.card_selected = 0;
            }
        }
        Pane::Platform(_) => {
            if let Some(platform) = platform(state).cloned()
                && let Some(field) = fields(state, &platform).get(state.card_selected)
            {
                if field.is_list {
                    let entries = state
                        .drafts
                        .get(&platform.id)
                        .and_then(|draft| draft.lists.get(&field.key))
                        .cloned()
                        .unwrap_or_else(|| parse_list(field.value.as_deref().unwrap_or_default()));
                    state.prompt = Some(Prompt::List {
                        key: field.key.clone(),
                        text: entries.join(", "),
                    });
                } else {
                    state.secret_editor = Some(field.key.clone());
                    state.secret_edit_empty = true;
                    return vec![ConnectorsEffect::EditSecret {
                        key: field.key.clone(),
                    }];
                }
            }
        }
        _ => {}
    }
    Vec::new()
}

fn toggle_tool(state: &mut ConnectorsState) -> Vec<ConnectorsEffect> {
    let Some(slug) = slug(state) else {
        return Vec::new();
    };
    if policy_revision(state, &slug, false).is_none() {
        return Vec::new();
    }
    if let Some(tool) = state
        .tools
        .as_ref()
        .and_then(|tools| tools.tools.get(state.card_selected))
    {
        if tool_locked(state, &slug, tool) {
            notice(state, "Your organization turned this tool off.");
        } else {
            let original = !disabled(state, &slug).contains(&tool.slug);
            let current = state
                .tool_drafts
                .get(&tool.slug)
                .copied()
                .unwrap_or(original);
            let tool = tool.slug.clone();
            if !current == original {
                state.tool_drafts.remove(&tool);
            } else {
                state.tool_drafts.insert(tool, !current);
            }
        }
    }
    Vec::new()
}

fn reconnect_copy(state: &mut ConnectorsState) -> Vec<ConnectorsEffect> {
    if state.pane == Pane::Link {
        return match &state.link {
            Some(link) if https(link) => vec![ConnectorsEffect::CopyText(link.clone())],
            Some(_) => {
                notice(state, "Hermes sent a link Talaria will not open.");
                Vec::new()
            }
            None => Vec::new(),
        };
    }
    if state.unavailable.as_ref().is_some_and(|(_, copy)| *copy) {
        return vec![ConnectorsEffect::CopyText(command(state))];
    }
    if let Some(command) = &state.copy_command {
        return vec![ConnectorsEffect::CopyText(command.clone())];
    }
    if state.tab != ConnectorsTab::Connectors || !matches!(state.pane, Pane::Connector(_)) {
        return Vec::new();
    }
    let Some(account) = account(state).cloned() else {
        return Vec::new();
    };
    if account.disabled {
        notice(
            state,
            "A retired account cannot be reconnected. Add another account instead.",
        );
        Vec::new()
    } else if account.status == "active" {
        let app = name(state, &account.connector);
        notice(
            state,
            account.alias.map_or_else(
                || format!("An unnamed {app} account is connected."),
                |alias| format!("{app} account {alias} is connected."),
            ),
        );
        Vec::new()
    } else {
        start_connect(
            state,
            account.connector,
            AccountAddress::Repair {
                connection_id: account.connection_id,
            },
        )
    }
}

pub(super) fn https(link: &str) -> bool {
    link.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty() && !rest.chars().any(|ch| ch.is_control() || ch.is_whitespace())
    })
}

fn open_link(state: &mut ConnectorsState) -> Vec<ConnectorsEffect> {
    let link = if state.pane == Pane::Link {
        state.link.clone()
    } else if let Some(platform) = platform(state) {
        fields(state, platform)
            .get(state.card_selected)
            .and_then(|field| field.url.clone())
            .filter(|url| https(url))
            .or_else(|| platform.docs_url.clone())
    } else {
        None
    };
    match link {
        Some(link) if https(&link) => vec![ConnectorsEffect::OpenUrl(link)],
        Some(_) => {
            notice(state, "Hermes sent a link Talaria will not open.");
            Vec::new()
        }
        None => Vec::new(),
    }
}

fn save_review(state: &mut ConnectorsState) -> Vec<ConnectorsEffect> {
    match &state.pane {
        Pane::Tools(slug) => {
            let slug = slug.clone();
            if policy_revision(state, &slug, false).is_none() {
                return Vec::new();
            }
            if !member_on(state, &slug) {
                notice(
                    state,
                    format!("Turn {} on before choosing its tools.", name(state, &slug)),
                );
            } else if !state.tool_drafts.is_empty() {
                state.prompt = Some(Prompt::Confirm(ConfirmKind::SaveTools));
            }
        }
        Pane::Platform(id) if has_drafts(state, id) => {
            state.prompt = Some(Prompt::Confirm(ConfirmKind::SavePlatform));
        }
        _ => {}
    }
    Vec::new()
}

pub(super) fn has_drafts(state: &ConnectorsState, id: &str) -> bool {
    state.drafts.get(id).is_some_and(|draft| {
        !draft.typed.is_empty() || !draft.lists.is_empty() || !draft.clear.is_empty()
    })
}

fn changed_draft(state: &mut ConnectorsState, id: &str, key: &str) {
    state.draft_sequence = state.draft_sequence.saturating_add(1);
    state
        .draft_versions
        .insert((id.into(), key.into()), state.draft_sequence);
}

fn commit_secret_draft(state: &mut ConnectorsState, key: &str) {
    if let Some(platform) = platform(state).cloned()
        && platform
            .env_vars
            .iter()
            .any(|field| field.key == key && !field.is_list)
    {
        let draft = state.drafts.entry(platform.id.clone()).or_default();
        draft.clear.remove(key);
        draft.typed.insert(key.into());
        changed_draft(state, &platform.id, key);
    }
}

fn close(state: &mut ConnectorsState) -> Vec<ConnectorsEffect> {
    if state.pane == Pane::Link {
        return stop_link(
            state,
            Some("Stopped waiting. The link still works until it expires.".into()),
        );
    }
    if let Pane::Tools(_) = &state.pane
        && !state.tool_drafts.is_empty()
    {
        state.prompt = Some(Prompt::Confirm(ConfirmKind::DiscardTools));
        return Vec::new();
    }
    if let Pane::Platform(id) = &state.pane
        && has_drafts(state, id)
    {
        state.prompt = Some(Prompt::Confirm(ConfirmKind::DiscardPlatform));
        return Vec::new();
    }
    if state.pane != Pane::List {
        state.pane = Pane::List;
        state.card_selected = 0;
        return Vec::new();
    }
    if !state.filter.is_empty() {
        state.filter.clear();
        return Vec::new();
    }
    state.closed = true;
    let mut effects = if state.operation.is_some() {
        stop_link(
            state,
            Some("Stopped waiting. The link still works until it expires.".into()),
        )
    } else {
        Vec::new()
    };
    effects.push(ConnectorsEffect::ForgetSecrets);
    effects.push(ConnectorsEffect::Close);
    effects
}

fn pairing_confirm(state: &mut ConnectorsState, approve: bool) -> Vec<ConnectorsEffect> {
    let Some(platform) = platform(state).cloned() else {
        return Vec::new();
    };
    let offset = state
        .card_selected
        .checked_sub(fields(state, &platform).len());
    let pending: Vec<_> = state
        .pairing
        .pending
        .iter()
        .filter(|row| row.platform == platform.id)
        .collect();
    if approve {
        if let Some(pair) = offset.and_then(|offset| pending.get(offset)) {
            if pair.request_id.is_empty() {
                let command = format!(
                    "hermes -p {} pairing approve {} <code>",
                    state.profile, pair.platform
                );
                notice(state, format!("This request needs its code: {command}"));
                state.copy_command = Some(command);
            } else {
                state.prompt = Some(Prompt::Confirm(ConfirmKind::ApprovePairing));
            }
        }
    } else {
        let approved: Vec<_> = state
            .pairing
            .approved
            .iter()
            .filter(|row| row.platform == platform.id)
            .collect();
        if offset
            .and_then(|offset| offset.checked_sub(pending.len()))
            .and_then(|offset| approved.get(offset))
            .is_some()
        {
            state.prompt = Some(Prompt::Confirm(ConfirmKind::RevokePairing));
        }
    }
    Vec::new()
}

fn prompt_key(state: &mut ConnectorsState, key: ConnectorsKey) -> Vec<ConnectorsEffect> {
    if matches!(key, ConnectorsKey::Close | ConnectorsKey::Cancel) {
        state.prompt = None;
        return Vec::new();
    }
    if matches!(state.prompt, Some(Prompt::Confirm(_))) {
        if key == ConnectorsKey::Confirm {
            return confirm(state);
        }
        return Vec::new();
    }
    match key {
        ConnectorsKey::Char(ch) => edit_text(state, &ch.to_string()),
        ConnectorsKey::Backspace => {
            if let Some(
                Prompt::Alias { text, .. }
                | Prompt::Rename { text, .. }
                | Prompt::List { text, .. },
            ) = &mut state.prompt
            {
                text.pop();
            }
        }
        ConnectorsKey::ClearField => {
            if let Some(
                Prompt::Alias { text, .. }
                | Prompt::Rename { text, .. }
                | Prompt::List { text, .. },
            ) = &mut state.prompt
            {
                text.clear();
            }
        }
        ConnectorsKey::Open => match state.prompt.clone() {
            Some(Prompt::Alias { slug, text }) => {
                if alias_problem(&text, &slug, &state.accounts, None).is_none() {
                    state.prompt = None;
                    return start_connect(state, slug, AccountAddress::Alias(text));
                }
            }
            Some(Prompt::Rename {
                connection_id,
                text,
            }) => {
                if let Some(account) = state
                    .accounts
                    .iter()
                    .find(|row| row.connection_id == connection_id)
                    .cloned()
                    && alias_problem(
                        &text,
                        &account.connector,
                        &state.accounts,
                        Some(&connection_id),
                    )
                    .is_none()
                    && can_write(state)
                {
                    state.write = Some(WriteKind::Rename);
                    state.write_account = Some(account);
                    state.prompt = None;
                    return vec![ConnectorsEffect::RenameAccount {
                        connection_id,
                        alias: text,
                    }];
                }
            }
            Some(Prompt::List { key, text }) => {
                if let Some(platform) = platform(state).cloned() {
                    let entries = parse_list(&text);
                    let original = platform
                        .env_vars
                        .iter()
                        .find(|field| field.key == key)
                        .map(|field| parse_list(field.value.as_deref().unwrap_or_default()))
                        .unwrap_or_default();
                    let draft = state.drafts.entry(platform.id.clone()).or_default();
                    if entries == original {
                        draft.clear.remove(&key);
                        draft.lists.remove(&key);
                    } else if entries.is_empty() {
                        draft.clear.insert(key.clone());
                        draft.lists.remove(&key);
                    } else {
                        draft.clear.remove(&key);
                        draft.lists.insert(key.clone(), entries);
                    }
                    changed_draft(state, &platform.id, &key);
                }
                state.prompt = None;
            }
            _ => {}
        },
        _ => {}
    }
    Vec::new()
}

fn confirm(state: &mut ConnectorsState) -> Vec<ConnectorsEffect> {
    let Some(Prompt::Confirm(kind)) = state.prompt.take() else {
        return Vec::new();
    };
    if !matches!(
        kind,
        ConfirmKind::DiscardPlatform | ConfirmKind::DiscardTools
    ) && !can_write(state)
    {
        return Vec::new();
    }
    match kind {
        ConfirmKind::RemoveAccount => {
            if let Some(account) = account(state).cloned() {
                state.write = Some(WriteKind::Remove);
                state.write_account = Some(account.clone());
                return vec![ConnectorsEffect::RemoveAccount {
                    connection_id: account.connection_id,
                }];
            }
        }
        ConfirmKind::SaveTools => {
            let Some(slug) = slug(state) else {
                return Vec::new();
            };
            let Some(revision) = policy_revision(state, &slug, false) else {
                return Vec::new();
            };
            if !member_on(state, &slug) {
                notice(
                    state,
                    format!("Turn {} on before choosing its tools.", name(state, &slug)),
                );
                return Vec::new();
            }
            let mut disabled = disabled(state, &slug);
            for (tool, enabled) in &state.tool_drafts {
                if state
                    .tools
                    .as_ref()
                    .and_then(|tools| tools.tools.iter().find(|item| &item.slug == tool))
                    .is_some_and(|tool| tool_locked(state, &slug, tool))
                {
                    continue;
                }
                if *enabled {
                    disabled.remove(tool);
                } else {
                    disabled.insert(tool.clone());
                }
            }
            state.write = Some(WriteKind::SetTools);
            return vec![ConnectorsEffect::SetTools {
                slug,
                disabled_tools: disabled.into_iter().collect(),
                expected_revision: revision,
            }];
        }
        ConfirmKind::SavePlatform | ConfirmKind::TogglePlatform => {
            let Some(platform) = platform(state).cloned() else {
                return Vec::new();
            };
            let enabled = (kind == ConfirmKind::TogglePlatform).then_some(!platform.enabled);
            let draft = if enabled.is_none() {
                state.drafts.get(&platform.id).cloned().unwrap_or_default()
            } else {
                Drafts::default()
            };
            let lists = draft
                .lists
                .into_iter()
                .filter(|(key, values)| !values.is_empty() && !draft.clear.contains(key))
                .collect();
            state.write = Some(WriteKind::UpdatePlatform);
            state.write_platform = Some(platform.id.clone());
            state.toggle_enabled = enabled;
            state.submitted_versions = if enabled.is_none() {
                state
                    .draft_versions
                    .iter()
                    .filter(|((id, _), _)| id == &platform.id)
                    .map(|((_, key), version)| (key.clone(), *version))
                    .collect()
            } else {
                Default::default()
            };
            return vec![ConnectorsEffect::UpdatePlatform {
                id: platform.id,
                enabled,
                typed_keys: draft
                    .typed
                    .into_iter()
                    .filter(|key| !draft.clear.contains(key))
                    .collect(),
                lists,
                clear_env: draft.clear.into_iter().collect(),
            }];
        }
        ConfirmKind::ApprovePairing | ConfirmKind::RevokePairing => {
            let Some(platform) = platform(state).cloned() else {
                return Vec::new();
            };
            let Some(offset) = state
                .card_selected
                .checked_sub(fields(state, &platform).len())
            else {
                return Vec::new();
            };
            let pending: Vec<_> = state
                .pairing
                .pending
                .iter()
                .filter(|row| row.platform == platform.id)
                .collect();
            if kind == ConfirmKind::ApprovePairing {
                if let Some(pair) = pending.get(offset) {
                    if pair.request_id.is_empty() {
                        return Vec::new();
                    }
                    let effect = ConnectorsEffect::ApprovePairing {
                        platform: platform.id.clone(),
                        request_id: pair.request_id.clone(),
                    };
                    state.write = Some(WriteKind::Approve);
                    state.write_platform = Some(platform.id);
                    return vec![effect];
                }
            } else if let Some(pair) = offset.checked_sub(pending.len()).and_then(|offset| {
                state
                    .pairing
                    .approved
                    .iter()
                    .filter(|row| row.platform == platform.id)
                    .nth(offset)
            }) {
                let effect = ConnectorsEffect::RevokePairing {
                    platform: platform.id.clone(),
                    user_id: pair.user_id.clone(),
                };
                state.write = Some(WriteKind::Revoke);
                state.write_platform = Some(platform.id);
                return vec![effect];
            }
        }
        ConfirmKind::DiscardTools => {
            state.tool_drafts.clear();
            state.pane = slug(state).map_or(Pane::List, Pane::Connector);
        }
        ConfirmKind::DiscardPlatform => {
            if let Some(platform) = platform(state) {
                let id = platform.id.clone();
                state.drafts.remove(&id);
            }
            state.pane = Pane::List;
            return if state.drafts.values().any(|draft| !draft.typed.is_empty()) {
                Vec::new()
            } else {
                vec![ConnectorsEffect::ForgetSecrets]
            };
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform_fixture() -> PlatformRow {
        let field = |key: &str| EnvVar {
            key: key.into(),
            prompt: key.into(),
            required: false,
            is_set: false,
            is_list: false,
            is_password: false,
            advanced: false,
            value: None,
            help: None,
            description: None,
            url: None,
        };
        PlatformRow {
            id: "signal".into(),
            name: "Signal".into(),
            description: String::new(),
            docs_url: None,
            enabled: false,
            state: Some("disabled".into()),
            error_message: None,
            home_channel: None,
            ingress_url: None,
            env_vars: vec![field("BRIDGE"), field("ACCESS")],
        }
    }

    fn platform_card(now: Instant) -> ConnectorsState {
        let mut state = ConnectorsState::new();
        let _ = update(
            &mut state,
            ConnectorsInput::Open {
                open: ConnectorsOpen::Tab(ConnectorsTab::Messaging),
                profile: "fixture".into(),
                consented: false,
            },
            now,
        );
        let _ = update(
            &mut state,
            ConnectorsInput::PlatformsReply(Ok(vec![platform_fixture()])),
            now,
        );
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), now);
        state
    }

    #[test]
    fn secret_editor_presence_waits_for_acceptance() {
        let now = Instant::now();
        let mut state = platform_card(now);
        let edit = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), now);
        assert_eq!(
            edit,
            [ConnectorsEffect::EditSecret {
                key: "BRIDGE".into()
            }]
        );
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), now);
        assert!(matches!(
            view(&state, now).screen,
            Screen::SecretEditor { .. }
        ));
        for _ in 0..3 {
            let _ = update(
                &mut state,
                ConnectorsInput::SecretEdited {
                    key: "BRIDGE".into(),
                    empty: false,
                },
                now,
            );
            assert!(matches!(
                view(&state, now).screen,
                Screen::SecretEditor { .. }
            ));
            assert!(!has_drafts(&state, "signal"));
        }
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Cancel), now);
        assert!(!has_drafts(&state, "signal"));

        let _ = update(
            &mut state,
            ConnectorsInput::SecretEdited {
                key: "BRIDGE".into(),
                empty: false,
            },
            now,
        );
        let original = state.draft_sequence;
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), now);
        let _ = update(
            &mut state,
            ConnectorsInput::SecretEdited {
                key: "BRIDGE".into(),
                empty: false,
            },
            now,
        );
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Close), now);
        assert_eq!(state.draft_sequence, original);
        assert!(has_drafts(&state, "signal"));

        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), now);
        let _ = update(
            &mut state,
            ConnectorsInput::SecretEdited {
                key: "BRIDGE".into(),
                empty: false,
            },
            now,
        );
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), now);
        assert!(matches!(
            view(&state, now).screen,
            Screen::PlatformCard { .. }
        ));
        assert!(state.draft_sequence > original);
        assert!(!format!("{:?}", ConnectorsInput::Key(ConnectorsKey::Char('$'))).contains('$'));
    }

    #[test]
    fn confirmed_save_preserves_an_open_secret_editor() {
        let now = Instant::now();
        let mut state = platform_card(now);
        let _ = update(
            &mut state,
            ConnectorsInput::SecretEdited {
                key: "BRIDGE".into(),
                empty: false,
            },
            now,
        );
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Save), now);
        let _ = update(
            &mut state,
            ConnectorsInput::Key(ConnectorsKey::Confirm),
            now,
        );
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), now);
        let _ = update(
            &mut state,
            ConnectorsInput::SecretEdited {
                key: "BRIDGE".into(),
                empty: false,
            },
            now,
        );
        let effects = update(
            &mut state,
            ConnectorsInput::UpdateReply {
                id: "signal".into(),
                reply: Ok(UpdateReply { hot_served: false }),
            },
            now,
        );
        assert_eq!(effects, [ConnectorsEffect::LoadPlatforms]);
        assert!(matches!(
            view(&state, now).screen,
            Screen::SecretEditor { .. }
        ));
        assert!(!has_drafts(&state, "signal"));
    }

    #[test]
    fn edits_after_submission_survive_confirmed_save() {
        let now = Instant::now();
        let mut state = ConnectorsState::new();
        let _ = update(
            &mut state,
            ConnectorsInput::Open {
                open: ConnectorsOpen::Tab(ConnectorsTab::Messaging),
                profile: "fixture".into(),
                consented: false,
            },
            now,
        );
        let _ = update(
            &mut state,
            ConnectorsInput::PlatformsReply(Ok(vec![platform_fixture()])),
            now,
        );
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), now);
        for key in ["BRIDGE", "ACCESS"] {
            let _ = update(
                &mut state,
                ConnectorsInput::SecretEdited {
                    key: key.into(),
                    empty: false,
                },
                now,
            );
        }
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Save), now);
        let sent = update(
            &mut state,
            ConnectorsInput::Key(ConnectorsKey::Confirm),
            now,
        );
        assert!(
            matches!(&sent[..], [ConnectorsEffect::UpdatePlatform { typed_keys, .. }] if typed_keys.len() == 2)
        );

        let _ = update(
            &mut state,
            ConnectorsInput::SecretEdited {
                key: "BRIDGE".into(),
                empty: false,
            },
            now,
        );
        let effects = update(
            &mut state,
            ConnectorsInput::UpdateReply {
                id: "signal".into(),
                reply: Ok(UpdateReply { hot_served: false }),
            },
            now,
        );
        assert_eq!(effects, [ConnectorsEffect::LoadPlatforms]);
        assert!(state.is_busy());
        let Screen::PlatformCard { fields, .. } = view(&state, now).screen else {
            panic!("platform card");
        };
        assert_eq!(fields[0].value, FieldValue::Draft);
        assert_eq!(fields[1].value, FieldValue::NotSet);

        let _ = update(
            &mut state,
            ConnectorsInput::PlatformsReply(Ok(vec![platform_fixture()])),
            now,
        );
        assert!(!state.is_busy());
        let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Save), now);
        let sent = update(
            &mut state,
            ConnectorsInput::Key(ConnectorsKey::Confirm),
            now,
        );
        assert!(
            matches!(&sent[..], [ConnectorsEffect::UpdatePlatform { typed_keys, .. }] if typed_keys == &["BRIDGE"])
        );
    }
}
