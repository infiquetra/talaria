use super::*;
pub fn view(state: &McpState, now: Instant) -> McpView {
    let (rows, placeholder, status_line) = match state.tab {
        McpTab::Servers => {
            let rows: Vec<_> = servers::visible_servers(state)
                .into_iter()
                .map(|r| RowView {
                    name: short(&r.summary.name),
                    kind: servers::kind(&r.summary).into(),
                    label: servers::label(r, state.tests.get(&r.summary.name), now),
                    dim: r.summary.source == hermes_contract::schemas::McpServerSource::Plugin,
                })
                .collect();
            let placeholder = if !state.list_loaded {
                Some("Loading…".into())
            } else if let Some(e) = &state.list_error {
                Some(format!(
                    "Could not load MCP servers: {e}. ctrl+r tries again."
                ))
            } else if rows.is_empty() && !state.filter.is_empty() {
                Some(format!("No server matches \"{}\".", short(&state.filter)))
            } else if rows.is_empty() {
                Some(format!(
                    "No MCP servers in profile {} yet. ctrl+n adds one; tab opens Hermes's catalogue.",
                    short(&state.profile)
                ))
            } else {
                None
            };
            (rows, placeholder, state.status_line.clone())
        }
        McpTab::Catalogue => {
            let rows: Vec<_> = servers::visible_catalogue(state)
                .into_iter()
                .map(|r| {
                    let mut label = if r.installed {
                        format!("installed · {}   ", if r.enabled { "on" } else { "off" })
                    } else {
                        String::new()
                    };
                    label.push_str(&short(&r.description));
                    if r.connector_slug.is_some() {
                        label.push_str(" · also a connector");
                    }
                    RowView {
                        name: short(&r.name),
                        kind: kind_text(transport_kind(&r.transport)).into(),
                        label,
                        dim: false,
                    }
                })
                .collect();
            let placeholder = if !state.catalog_loaded {
                Some("Loading…".into())
            } else if let Some(e) = &state.catalog_error {
                Some(format!(
                    "Could not load catalogue: {e}. ctrl+r tries again."
                ))
            } else if rows.is_empty() && !state.filter.is_empty() {
                Some(format!("No server matches \"{}\".", short(&state.filter)))
            } else if rows.is_empty() {
                Some("Hermes's catalogue is empty in this install.".into())
            } else {
                None
            };
            (
                rows,
                placeholder,
                Some(format!(
                    "{} servers in Hermes's catalogue",
                    state.catalogue.len()
                )),
            )
        }
    };
    let selected = (!rows.is_empty()).then_some(state.selected.min(rows.len().saturating_sub(1)));
    let card = state.card.as_ref().map(|c| card_view(state, c, now));
    let secret_focus = card
        .as_ref()
        .and_then(|c| c.fields.iter().find(|f| f.focused).and_then(|f| f.secret));
    McpView {
        title:format!("MCP servers · profile {}",short(&state.profile)),tab:state.tab,
        banner:(!state.connected).then(||"Not connected to Hermes. Changes are off until Talaria reconnects.".into()),
        filter:state.filter.clone(),placeholder,rows,selected,status_line,card,
        pending_line:state.pending.then(||"Saved for new sessions; this session still uses the old servers · ctrl+y apply".into()),
        notice:state.notice.clone(),
        footer:match state.tab { McpTab::Servers => "enter details · ctrl+t test · ctrl+e on/off · ctrl+k key · ctrl+o sign in · ctrl+d remove · ctrl+n add · ctrl+y apply", McpTab::Catalogue => "enter install · tab servers · ctrl+r reload · esc close" }.into(),
        secret_focus,
    }
}
fn kind_text(kind: TransportKind) -> &'static str {
    match kind {
        TransportKind::Remote => "remote",
        TransportKind::Local => "local",
        TransportKind::Unknown => "",
    }
}
fn card(title: String, lines: Vec<String>, fields: Vec<FieldView>, footer: &str) -> CardView {
    CardView {
        title,
        lines,
        fields,
        footer: footer.into(),
    }
}
fn card_view(state: &McpState, c: &Card, now: Instant) -> CardView {
    match c {
        Card::Form(form) => {
            let mut lines = vec![];
            if form.discard {
                lines.push("Discard this server? y · n".into());
            }
            if form.local && form.env.iter().all(|n| !n.trim().is_empty()) {
                lines.push("Up to 8 variables here; add more later with ctrl+k.".into());
            }
            card(
                "Add an MCP server".into(),
                lines,
                form.fields(state),
                "tab next field · ←→ choose · ctrl+s save · esc discard",
            )
        }
        Card::Key {
            name,
            variable,
            focus,
            error,
        } => {
            let mut fields = vec![];
            if let Some(var) = variable {
                let mut field = form::field("Variable", var, None);
                field.focused = *focus == 0;
                if *focus == 0 {
                    field.error = error.clone();
                }
                fields.push(field);
            }
            let mut value = form::field("Value", "", Some(SecretSlot::KeyValue));
            value.focused = variable.is_none() || *focus == 1;
            if value.focused {
                value.error = error.clone();
            }
            fields.push(value);
            let mut lines = vec!["This replaces any value already saved under that name.".into()];
            if variable.is_none() {
                lines.insert(
                    0,
                    format!(
                        "Saved to profile {}'s key file; the server sends it as a Bearer token.",
                        short(&state.profile)
                    ),
                );
            }
            card(
                format!("API key for {}", short(name)),
                lines,
                fields,
                "enter save · esc cancel",
            )
        }
        Card::Remove(name) => card(
            "Remove server".into(),
            vec![format!(
                "Remove {} from profile {}? Its saved key and sign-in stay in the profile's files.",
                short(name),
                short(&state.profile)
            )],
            vec![],
            "y remove · n cancel",
        ),
        Card::Install(entry) => {
            let mut lines = vec![
                short(&entry.description),
                kind_text(transport_kind(&entry.transport)).into(),
            ];
            let footer = if entry.requires.is_empty() {
                lines.push(format!(
                    "After install it is on for new sessions of profile {}.",
                    short(&state.profile)
                ));
                "enter install · esc cancel"
            } else {
                lines.push(format!(
                    "{} needs values Hermes asks for at install: {}. Run {} in a terminal.",
                    short(&entry.name),
                    entry
                        .requires
                        .iter()
                        .map(|n| short(n))
                        .collect::<Vec<_>>()
                        .join(", "),
                    install_command(state, &entry.name)
                ));
                "c copy command · esc back"
            };
            card(
                format!("Install {}?", short(&entry.name)),
                lines,
                vec![],
                footer,
            )
        }
        Card::OfferSignIn(name) => card(
            "Sign in".into(),
            vec![format!(
                "{} needs you to sign in. Sign in now? y · n",
                short(name)
            )],
            vec![],
            "y sign in · n cancel",
        ),
        Card::SignIn(signin) => {
            let mut lines = if let Some(since) = signin.since {
                let left = 300u64.saturating_sub(now.saturating_duration_since(since).as_secs());
                vec![
                    "1. Open this link in a browser on this machine:".into(),
                    signin.url.clone(),
                    "2. Approve access. Hermes receives the answer itself.".into(),
                    if left == 0 {
                        "Waiting for Hermes to give up…".into()
                    } else {
                        format!("Waiting for approval… {}:{:02} left", left / 60, left % 60)
                    },
                ]
            } else {
                vec!["Starting sign-in…".into()]
            };
            if state.ssh && signin.since.is_some() {
                lines.push(format!("You are connected over SSH. If your browser is on another computer, the redirect to {} cannot reach this machine. After approving, copy the address your browser shows (even if the page fails) and press p.",signin::redirect_hint(&signin.url)));
            }
            let fields = if signin.paste {
                let mut field = form::field("Paste", "", Some(SecretSlot::Paste));
                field.focused = true;
                vec![field]
            } else {
                vec![]
            };
            card(
                format!("Sign in to {}", short(&signin.name)),
                lines,
                fields,
                if signin.paste {
                    "enter send · esc back"
                } else {
                    "o open link · c copy link · p paste the browser's address · esc cancel"
                },
            )
        }
        Card::Done { title, message } => {
            card(title.clone(), vec![message.clone()], vec![], "esc back")
        }
        Card::Apply { stage, .. } => card(
            "Apply MCP servers".into(),
            vec![match stage {
                ApplyStage::Asking => "Asking Hermes…".into(),
                ApplyStage::Applying => "Reloading MCP servers… (up to 2 minutes)".into(),
                ApplyStage::Confirm(message) => message.clone(),
            }],
            vec![],
            match stage {
                ApplyStage::Confirm(_) => "y apply · n cancel",
                _ => "",
            },
        ),
        Card::Detail(name) => detail(state, name, now),
    }
}
pub(super) fn install_command(state: &McpState, name: &str) -> String {
    format!("hermes -p {} mcp install {name}", state.profile)
}
pub(super) fn login_command(state: &McpState, name: &str) -> String {
    format!("hermes -p {} mcp login {name} --flow device", state.profile)
}
fn tool_filter(value: Option<&serde_json::Value>) -> String {
    let Some(value) = value.and_then(serde_json::Value::as_object) else {
        return "all tools".into();
    };
    let names = |key| {
        value
            .get(key)
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(short)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
    };
    let include = names("include");
    let exclude = names("exclude");
    match (include, exclude) {
        (Some(include), exclude) => {
            let mut text = if include.is_empty() {
                "none (the include list is empty)".into()
            } else {
                format!("only: {include}")
            };
            if let Some(exclude) = exclude {
                text.push_str(&format!("; except: {exclude}"));
            }
            text
        }
        (None, Some(exclude)) => format!("all except: {exclude}"),
        _ => "all tools".into(),
    }
}
fn detail(state: &McpState, name: &str, now: Instant) -> CardView {
    let Some(row) = state.servers.iter().find(|r| r.summary.name == name) else {
        return card(
            short(name),
            vec![format!("{} is no longer configured.", short(name))],
            vec![],
            "esc back",
        );
    };
    let s = &row.summary;
    let mut lines = vec![format!(
        "{} · {}",
        servers::kind(s),
        if s.source == hermes_contract::schemas::McpServerSource::Plugin {
            format!(
                "from plugin {}",
                short(s.plugin.as_deref().unwrap_or("unknown"))
            )
        } else {
            "from config".into()
        }
    )];
    if let Some(url) = &s.url {
        lines.push(format!("URL: {}", short(url)));
    } else if let Some(command) = &s.command {
        lines.push(format!(
            "Command: {} {}",
            short(command),
            s.args
                .iter()
                .map(|a| short(a))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    lines.push(if s.env.is_empty() {
        "Environment variables: none".into()
    } else {
        format!(
            "Environment variables: {} (values hidden)",
            s.env
                .iter()
                .map(|n| short(n))
                .collect::<Vec<_>>()
                .join(", ")
        )
    });
    lines.push(format!(
        "Authentication: {}",
        match s.auth.as_deref() {
            Some("oauth") if s.oauth_tokens_present == Some(true) => "OAuth, signed in",
            Some("oauth") => "OAuth, not signed in",
            Some("header") => "API key header",
            _ => "none",
        }
    ));
    lines.push(format!("Tool filter: {}", tool_filter(s.tools.as_ref())));
    lines.push(servers::label(row, state.tests.get(name), now));
    if let Some(outcome) = state.tests.get(name) {
        match outcome {
            TestOutcome::Ok { tools, .. } => {
                lines.extend(
                    tools
                        .iter()
                        .take(20)
                        .map(|(name, description)| format!("{name} — {description}")),
                );
                if tools.len() > 20 {
                    lines.push(format!("… and {} more", tools.len() - 20));
                }
            }
            TestOutcome::Failed { error } => lines.push(error.clone()),
            _ => {}
        }
    }
    let oauth = s.auth.as_deref() == Some("oauth");
    if oauth {
        lines.push(format!(
            "No browser on this machine? Run {} on the gateway's machine, then press ctrl+r.",
            login_command(state, name)
        ));
    }
    card(
        short(name),
        lines,
        vec![],
        if oauth {
            "c copy command · esc back"
        } else {
            "esc back"
        },
    )
}
