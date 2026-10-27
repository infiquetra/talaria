//! Read-only projection of the connector and messaging state.

use super::reducer::{
    account_rows, disabled, fields, https, joined, name, platform, platform_rows, tool_locked,
};
use super::*;
use crate::attachments::sanitize;

fn line(text: impl Into<String>) -> Line {
    Line {
        text: sanitize(&text.into()),
        tone: Tone::Normal,
    }
}

fn footer(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(key, label)| ((*key).into(), (*label).into()))
        .collect()
}

pub(super) fn view(state: &ConnectorsState, now: Instant) -> ConnectorsView {
    let mut result = ConnectorsView {
        title: sanitize(&format!(
            "Connectors and messaging · profile {}",
            state.profile
        )),
        tab: state.tab,
        screen: Screen::Loading,
        notice: state.notice.clone(),
        footer: Vec::new(),
    };
    if let Some(prompt) = &state.prompt {
        result.screen = prompt_screen(state, prompt);
        result.footer = if matches!(prompt, Prompt::Confirm(_)) {
            footer(&[("y", "confirm"), ("n", "back")])
        } else {
            footer(&[("enter", "keep"), ("esc", "cancel")])
        };
        return result;
    }
    if let Some(key) = &state.secret_editor
        && let Some(field) = platform(state)
            .and_then(|platform| platform.env_vars.iter().find(|field| &field.key == key))
    {
        result.screen = Screen::SecretEditor {
            key: key.clone(),
            title: sanitize(&format!("{} ({key})", field.prompt)),
            help: field
                .help
                .as_ref()
                .or(field.description.as_ref())
                .map(|text| sanitize(text)),
        };
        result.footer = footer(&[
            ("enter", "keep"),
            ("ctrl+u", "clear text"),
            ("esc", "cancel"),
        ]);
        return result;
    }
    if state.tab == ConnectorsTab::Connectors && !state.consented {
        result.screen = Screen::Consent {
            text: sanitize(&format!(
                "Connectors run through Nous Research's hosted tool gateway. If profile {} has no Nous sign-in, Hermes creates a free guest identity for it when you continue.",
                state.profile
            )),
        };
        result.footer = footer(&[("enter", "continue"), ("esc", "back")]);
        return result;
    }
    if state.tab == ConnectorsTab::Connectors
        && let Some((text, copy)) = &state.unavailable
    {
        result.screen = Screen::Unavailable {
            text: sanitize(text),
            command: copy.then(|| format!("hermes -p {} auth upgrade", state.profile)),
        };
        result.footer = footer(&[("ctrl+r", "try again"), ("esc", "close")]);
        if *copy {
            result.footer.insert(0, ("c".into(), "copy command".into()));
        }
        return result;
    }
    match &state.pane {
        Pane::Link if state.tab == ConnectorsTab::Connectors => {
            let app = name(state, state.connect_slug.as_deref().unwrap_or_default());
            let left = state.operation.as_ref().map(|operation| {
                let seconds = operation.deadline.saturating_duration_since(now).as_secs();
                format!("{:02}:{:02} left", seconds / 60, seconds % 60)
            });
            let link = state.link.as_ref().filter(|link| https(link)).cloned();
            result.screen = Screen::LinkCard {
                app: sanitize(&app),
                link,
                left,
                ssh: false,
                starting: state.link.is_none(),
            };
            result.footer = footer(&[("w", "check now"), ("esc", "stop waiting")]);
            if state.link.as_ref().is_some_and(|link| https(link)) {
                result
                    .footer
                    .splice(0..0, footer(&[("o", "open link"), ("c", "copy link")]));
            } else if state.link.is_some() {
                result.notice = Some(line("Hermes sent a link Talaria will not open."));
            }
        }
        Pane::Connector(slug) if state.tab == ConnectorsTab::Connectors => {
            let title = name(state, slug);
            let catalog = state.catalog.iter().find(|row| &row.slug == slug);
            let mut lines = vec![line(slug.clone())];
            if let Some(category) = catalog.and_then(|row| row.category.clone()) {
                lines.push(line(category));
            }
            if let Some(description) = catalog.and_then(|row| row.description.clone()) {
                lines.push(line(description));
            }
            if let Some(row) = joined(state).iter().find(|row| &row.connector == slug) {
                lines.push(line(connector_label(row, &state.accounts)));
                if let Some(reason) = &row.status_reason {
                    lines.push(line(reason.clone()));
                }
            }
            if let Some(message) = &state.accounts_error {
                lines.push(line(format!("Accounts unavailable: {message}")));
            }
            let accounts = account_rows(state, slug);
            result.screen = Screen::ConnectorCard {
                title: sanitize(&title),
                lines,
                accounts: accounts
                    .rows
                    .iter()
                    .map(|account| Row {
                        label: sanitize(&account.name),
                        state: sanitize(&account.status),
                        marker: None,
                    })
                    .collect(),
                retired: accounts.retired,
                selected: state
                    .card_selected
                    .min(accounts.rows.len().saturating_sub(1)),
            };
            result.footer = footer(&[
                ("a", "add account"),
                ("t", "tools"),
                ("h", "retired"),
                ("esc", "back"),
            ]);
            if let Some(selected) = accounts.rows.get(state.card_selected).and_then(|selected| {
                state
                    .accounts
                    .iter()
                    .find(|account| account.connection_id == selected.connection_id)
            }) {
                result.footer.insert(1, ("r".into(), "rename".into()));
                result.footer.insert(2, ("x".into(), "remove".into()));
                if !selected.disabled && selected.status != "active" {
                    result.footer.insert(2, ("c".into(), "reconnect".into()));
                }
            }
        }
        Pane::Tools(slug) if state.tab == ConnectorsTab::Connectors => {
            let tools = state.tools.as_ref();
            let disabled = disabled(state, slug);
            let rows: Vec<_> = tools
                .into_iter()
                .flat_map(|tools| &tools.tools)
                .map(|tool| {
                    let enabled = state
                        .tool_drafts
                        .get(&tool.slug)
                        .copied()
                        .unwrap_or_else(|| !disabled.contains(&tool.slug));
                    ToolRow {
                        name: sanitize(&tool.name),
                        facet: sanitize(&tool.facet),
                        state: if tool_locked(state, slug, tool) {
                            ToolState::Locked
                        } else if enabled {
                            ToolState::On
                        } else {
                            ToolState::Off
                        },
                        draft: state.tool_drafts.contains_key(&tool.slug),
                        deprecated: tool.deprecated,
                    }
                })
                .collect();
            let on = rows.iter().filter(|row| row.state == ToolState::On).count();
            let header = if rows.is_empty() {
                format!("{} has no tools to choose.", name(state, slug))
            } else {
                format!(
                    "{} tools · {on} on · {} off",
                    name(state, slug),
                    rows.len() - on
                )
            };
            result.screen = Screen::Tools {
                header: sanitize(&header),
                cached: tools.is_some_and(|tools| tools.stale),
                selected: state.card_selected.min(rows.len().saturating_sub(1)),
                rows,
            };
            result.footer = footer(&[
                ("space", "toggle"),
                ("ctrl+r", "refresh"),
                ("ctrl+s", "review"),
                ("esc", "back"),
            ]);
        }
        Pane::Platform(id) if state.tab == ConnectorsTab::Messaging => {
            if let Some(platform) = state.platforms.iter().find(|row| &row.id == id) {
                platform_card(state, platform, &mut result);
            }
        }
        _ if state.tab == ConnectorsTab::Messaging => {
            if state.platforms_loaded {
                let platforms = platform_rows(state);
                let rows = platforms
                    .iter()
                    .map(|platform| {
                        let pending = state
                            .pairing
                            .pending
                            .iter()
                            .filter(|row| row.platform == platform.id)
                            .count();
                        let mut status = platform_label(platform);
                        if pending != 0 {
                            status.push_str(&format!(" · {pending} waiting to pair"));
                        }
                        Row {
                            label: sanitize(&platform.name),
                            state: sanitize(&status),
                            marker: None,
                        }
                    })
                    .collect::<Vec<_>>();
                let mut other = std::collections::BTreeMap::<String, usize>::new();
                for pair in &state.pairing.pending {
                    if !state.platforms.iter().any(|row| row.id == pair.platform) {
                        *other.entry(pair.platform.clone()).or_default() += 1;
                    }
                }
                result.screen = Screen::PlatformList {
                    selected: state.selected.min(rows.len().saturating_sub(1)),
                    rows,
                    restart_line: state
                        .platforms
                        .iter()
                        .any(|row| row.state.as_deref() == Some("pending_restart")),
                    other_pairing: other
                        .into_iter()
                        .map(|(platform, count)| {
                            sanitize(&format!("{count} waiting to pair on {platform}"))
                        })
                        .collect(),
                };
            }
            result.footer = footer(&[
                ("tab", "connectors"),
                ("enter", "open"),
                ("space", "on/off"),
                ("t", "test"),
                ("/", "filter"),
                ("ctrl+r", "refresh"),
                ("esc", "close"),
            ]);
            if state.restart_notice
                || state
                    .platforms
                    .iter()
                    .any(|row| row.state.as_deref() == Some("pending_restart"))
            {
                result.footer.push(("R".into(), "doctor".into()));
            }
        }
        _ => {
            if state.loaded || !state.catalog.is_empty() {
                let joined = joined(state);
                let rows = joined
                    .iter()
                    .map(|row| Row {
                        label: sanitize(&name(state, &row.connector)),
                        state: sanitize(&connector_label(row, &state.accounts)),
                        marker: None,
                    })
                    .collect::<Vec<_>>();
                result.screen = Screen::ConnectorList {
                    selected: state.selected.min(rows.len().saturating_sub(1)),
                    rows,
                    filter: (!state.filter.is_empty() || state.filtering)
                        .then(|| sanitize(&state.filter)),
                };
                if joined.is_empty() && state.filter.is_empty() {
                    result.notice = Some(line(format!(
                        "No connectors are offered to profile {}.",
                        state.profile
                    )));
                }
            }
            result.footer = footer(&[
                ("tab", "messaging"),
                ("enter", "open"),
                ("space", "on/off"),
                ("t", "tools"),
                ("/", "filter"),
                ("ctrl+r", "refresh"),
                ("esc", "close"),
            ]);
        }
    }
    result
}

fn platform_card(state: &ConnectorsState, platform: &PlatformRow, result: &mut ConnectorsView) {
    let mut lines = vec![line(platform.description.clone())];
    if let Some(url) = &platform.docs_url {
        lines.push(line(format!("Docs: {url}")));
    }
    lines.push(line(format!(
        "On: {}",
        if platform.enabled { "yes" } else { "no" }
    )));
    lines.push(line(format!("State: {}", platform_label(platform))));
    if let Some(error) = &platform.error_message {
        lines.push(line(error.clone()));
    }
    if let Some(name) = platform
        .home_channel
        .as_ref()
        .and_then(|channel| channel.name.as_ref())
    {
        lines.push(line(format!("Home channel: {name}")));
    }
    if let Some(url) = &platform.ingress_url {
        lines.push(line(format!("Callback address: {url}")));
    }
    if platform.state.as_deref() == Some("gateway_stopped")
        && let Some(command) = &state.gateway_start_command
    {
        lines.push(line(format!("Start the gateway with: {command}")));
    }
    if let Some(error) = &state.pairing_error {
        lines.push(line(format!("Pairing unavailable: {error}")));
    }
    let advanced = platform
        .env_vars
        .iter()
        .filter(|field| field.advanced)
        .count();
    if advanced != 0 {
        lines.push(line(format!(
            "A {} {advanced} more fields",
            if state.advanced { "hides" } else { "shows" }
        )));
    }
    let draft = state.drafts.get(&platform.id);
    let fields = fields(state, platform)
        .into_iter()
        .map(|field| {
            let value = if draft.is_some_and(|draft| draft.clear.contains(&field.key)) {
                FieldValue::WillClear
            } else if let Some(values) = draft.and_then(|draft| draft.lists.get(&field.key)) {
                FieldValue::List(values.iter().map(|value| sanitize(value)).collect())
            } else if draft.is_some_and(|draft| draft.typed.contains(&field.key)) {
                FieldValue::Draft
            } else if field.is_list {
                FieldValue::List(
                    parse_list(field.value.as_deref().unwrap_or_default())
                        .into_iter()
                        .map(|value| sanitize(&value))
                        .collect(),
                )
            } else if field.is_set {
                FieldValue::Set
            } else {
                FieldValue::NotSet
            };
            let mut prompt = sanitize(&field.prompt);
            if state.changed_fields.contains(&field.key) {
                prompt.push_str(" (changed in Hermes)");
            }
            FieldRow {
                key: field.key.clone(),
                prompt,
                required: field.required,
                value,
            }
        })
        .collect::<Vec<_>>();
    let pending: Vec<_> = state
        .pairing
        .pending
        .iter()
        .filter(|pair| pair.platform == platform.id)
        .collect();
    let approved: Vec<_> = state
        .pairing
        .approved
        .iter()
        .filter(|pair| pair.platform == platform.id)
        .collect();
    lines.push(line(format!("Waiting to pair ({})", pending.len())));
    lines.push(line(format!("Paired ({})", approved.len())));
    let pairing = pending
        .into_iter()
        .map(|pair| Row {
            label: sanitize(&format!(
                "{} · {}",
                pair.user_name.as_deref().unwrap_or(&pair.user_id),
                pair.user_id
            )),
            state: format!("{} min", pair.age_minutes),
            marker: Some('p'),
        })
        .chain(approved.into_iter().map(|pair| Row {
            label: sanitize(&format!(
                "{} · {}",
                pair.user_name.as_deref().unwrap_or(&pair.user_id),
                pair.user_id
            )),
            state: "paired".into(),
            marker: Some('x'),
        }))
        .collect::<Vec<_>>();
    result.screen = Screen::PlatformCard {
        lines,
        selected: state
            .card_selected
            .min((fields.len() + pairing.len()).saturating_sub(1)),
        fields,
        pairing,
    };
    result.footer = footer(&[
        ("enter", "edit field"),
        ("space", "on/off"),
        ("d", "clear field"),
        ("ctrl+s", "review"),
        ("t", "test"),
        ("A", "advanced"),
        ("esc", "back"),
    ]);
    let field_link = platform
        .env_vars
        .iter()
        .filter(|field| !field.advanced || state.advanced)
        .nth(state.card_selected)
        .and_then(|field| field.url.as_deref())
        .is_some_and(https);
    if field_link {
        result.footer.push(("o".into(), "help link".into()));
    } else if platform.docs_url.as_ref().is_some_and(|url| https(url)) {
        result.footer.push(("o".into(), "docs".into()));
    }
    if state.restart_notice
        || state
            .platforms
            .iter()
            .any(|row| row.state.as_deref() == Some("pending_restart"))
    {
        result.footer.push(("R".into(), "doctor".into()));
    }
    if let Some(command) = &state.copy_command {
        let _ = command;
        result.footer.push(("c".into(), "copy command".into()));
    }
}

fn prompt_screen(state: &ConnectorsState, prompt: &Prompt) -> Screen {
    match prompt {
        Prompt::Alias { slug, text } => Screen::Prompt {
            title: sanitize(&format!("Name the new {} account", name(state, slug))),
            text: sanitize(text),
            help: None,
            problem: alias_error(alias_problem(text, slug, &state.accounts, None)),
        },
        Prompt::Rename {
            connection_id,
            text,
        } => {
            let account = state
                .accounts
                .iter()
                .find(|row| &row.connection_id == connection_id);
            let slug = account.map_or("", |row| row.connector.as_str());
            Screen::Prompt {
                title: sanitize(&format!("Rename {} account", name(state, slug))),
                text: sanitize(text),
                help: None,
                problem: alias_error(alias_problem(
                    text,
                    slug,
                    &state.accounts,
                    Some(connection_id),
                )),
            }
        }
        Prompt::List { key, text } => Screen::Prompt {
            title: sanitize(key),
            text: sanitize(text),
            help: Some("Separate entries with commas.".into()),
            problem: None,
        },
        Prompt::Confirm(ConfirmKind::SaveTools) => {
            let slug = if let Pane::Tools(slug) = &state.pane {
                slug.as_str()
            } else {
                ""
            };
            let disabled = disabled(state, slug);
            let rows = state
                .tool_drafts
                .iter()
                .map(|(slug, enabled)| crate::settings::ReviewRow {
                    key: state
                        .tools
                        .as_ref()
                        .and_then(|tools| tools.tools.iter().find(|tool| &tool.slug == slug))
                        .map_or_else(|| slug.clone(), |tool| sanitize(&tool.name)),
                    old: if disabled.contains(slug) { "off" } else { "on" }.into(),
                    new: if *enabled { "on" } else { "off" }.into(),
                    effect_label: String::new(),
                    warnings: Vec::new(),
                })
                .collect();
            Screen::Review {
                title: sanitize(&format!("Save tool changes for {}?", name(state, slug))),
                rows,
            }
        }
        Prompt::Confirm(ConfirmKind::SavePlatform) => {
            let Some(platform) = platform(state) else {
                return Screen::Loading;
            };
            let rows = state
                .drafts
                .get(&platform.id)
                .map(|draft| review_rows(platform, draft))
                .unwrap_or_default();
            Screen::Review {
                title: sanitize(&format!(
                    "Save {} change(s) to {} for profile {}?",
                    rows.len(),
                    platform.name,
                    state.profile
                )),
                rows,
            }
        }
        Prompt::Confirm(kind) => Screen::Confirm {
            text: confirmation(state, *kind),
        },
    }
}

fn alias_error(problem: Option<AliasProblem>) -> Option<String> {
    problem.map(|problem| match problem {
        AliasProblem::Invalid => {
            "Use 1-32 lowercase letters, digits or -, starting with a letter or digit.".into()
        }
        AliasProblem::Taken => "That name is already used.".into(),
    })
}

fn confirmation(state: &ConnectorsState, kind: ConfirmKind) -> String {
    let platform = platform(state);
    let text = match kind {
        ConfirmKind::DiscardTools => "Discard tool changes?".into(),
        ConfirmKind::DiscardPlatform => format!(
            "Discard changes to {}?",
            platform.map_or("", |row| row.name.as_str())
        ),
        ConfirmKind::TogglePlatform => platform.map_or_else(String::new, |platform| {
            format!(
                "Turn {} {} for profile {}?{}",
                platform.name,
                if platform.enabled { "off" } else { "on" },
                state.profile,
                if platform.enabled {
                    " Hermes stops answering there."
                } else {
                    ""
                }
            )
        }),
        ConfirmKind::RemoveAccount => {
            let slug = if let Pane::Connector(slug) = &state.pane {
                slug.as_str()
            } else {
                ""
            };
            let account = account_rows(state, slug);
            format!(
                "Remove {} account {}? This signs Hermes out of it.",
                name(state, slug),
                account
                    .rows
                    .get(state.card_selected)
                    .map_or("", |row| row.name.as_str())
            )
        }
        ConfirmKind::ApprovePairing | ConfirmKind::RevokePairing => {
            let Some(platform) = platform else {
                return String::new();
            };
            let offset = state
                .card_selected
                .saturating_sub(fields(state, platform).len());
            let pending: Vec<_> = state
                .pairing
                .pending
                .iter()
                .filter(|pair| pair.platform == platform.id)
                .collect();
            if kind == ConfirmKind::ApprovePairing {
                pending.get(offset).map_or_else(String::new, |pair| {
                    format!(
                        "Let {} ({}) message the bot on {}?",
                        pair.user_name.as_deref().unwrap_or(&pair.user_id),
                        pair.user_id,
                        platform.name
                    )
                })
            } else {
                offset
                    .checked_sub(pending.len())
                    .and_then(|offset| {
                        state
                            .pairing
                            .approved
                            .iter()
                            .filter(|pair| pair.platform == platform.id)
                            .nth(offset)
                    })
                    .map_or_else(String::new, |pair| {
                        format!(
                            "Remove the pairing approval for {} ({}) on {}?",
                            pair.user_name.as_deref().unwrap_or(&pair.user_id),
                            pair.user_id,
                            platform.name
                        )
                    })
            }
        }
        _ => String::new(),
    };
    sanitize(&text)
}
