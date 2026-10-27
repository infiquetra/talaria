use super::*;
use hermes_contract::schemas::McpRuntimeStatus;
pub fn server_state(row: &ServerRow, _test: Option<&TestOutcome>) -> StateLabel {
    if !row.summary.enabled {
        return StateLabel::Off;
    }
    if row.summary.auth.as_deref() == Some("oauth")
        && row.summary.oauth_tokens_present == Some(false)
    {
        return StateLabel::NeedsSignIn;
    }
    let Some(runtime) = &row.runtime else {
        return StateLabel::Unknown;
    };
    let tools = runtime.tools.max(0) as u64;
    match runtime.status {
        McpRuntimeStatus::Connected => StateLabel::Connected(tools),
        McpRuntimeStatus::Lazy => StateLabel::Lazy(tools),
        McpRuntimeStatus::Connecting => StateLabel::Connecting,
        McpRuntimeStatus::Failed => StateLabel::Failed,
        McpRuntimeStatus::Configured | McpRuntimeStatus::Disabled => {
            StateLabel::ConfiguredNotReported
        }
    }
}
pub(super) fn label(row: &ServerRow, test: Option<&TestOutcome>, now: Instant) -> String {
    let mut label = match server_state(row, test) {
        StateLabel::Off => "off".into(),
        StateLabel::NeedsSignIn => "needs sign-in".into(),
        StateLabel::Connected(n) => format!("connected · {n} tools"),
        StateLabel::Lazy(n) => format!("ready · {n} tools · starts on first use"),
        StateLabel::Connecting => "connecting…".into(),
        StateLabel::Failed => "failed to connect · ctrl+t shows why".into(),
        StateLabel::ConfiguredNotReported => "configured · not reported running".into(),
        StateLabel::Unknown => "status unknown".into(),
    };
    if row.summary.source == hermes_contract::schemas::McpServerSource::Plugin {
        label = format!(
            "plugin {} · {label}",
            short(row.summary.plugin.as_deref().unwrap_or("unknown"))
        );
    }
    if let Some(test) = test {
        label.push_str(&test_suffix(test, now));
    }
    label
}
pub(super) fn test_suffix(test: &TestOutcome, now: Instant) -> String {
    match test {
        TestOutcome::Running { since } => format!(
            " · testing… {}s",
            now.saturating_duration_since(*since).as_secs()
        ),
        TestOutcome::Ok { tools, .. } => format!(" · test ok, {} tools", tools.len()),
        TestOutcome::NeedsSignIn => " · test: needs sign-in".into(),
        TestOutcome::Failed { .. } => " · test failed".into(),
    }
}
pub(super) fn kind(row: &McpServerSummary) -> &'static str {
    if row.url.is_some() {
        "remote"
    } else if row.command.is_some() {
        "local"
    } else {
        ""
    }
}
pub(super) fn matched(text: &str, filter: &str) -> bool {
    let text = text.to_lowercase();
    filter
        .to_lowercase()
        .split_whitespace()
        .all(|part| text.contains(part))
}
pub(super) fn visible_servers(state: &McpState) -> Vec<&ServerRow> {
    state
        .servers
        .iter()
        .filter(|r| {
            matched(
                &format!(
                    "{} {} {} {}",
                    r.summary.name,
                    r.summary.transport,
                    r.summary.url.as_deref().unwrap_or(""),
                    r.summary.command.as_deref().unwrap_or("")
                ),
                &state.filter,
            )
        })
        .collect()
}
pub(super) fn visible_catalogue(state: &McpState) -> Vec<&McpCatalogEntry> {
    state
        .catalogue
        .iter()
        .filter(|r| matched(&format!("{} {}", r.name, r.description), &state.filter))
        .collect()
}
pub(super) fn selected_name(state: &McpState) -> Option<String> {
    match state.tab {
        McpTab::Servers => visible_servers(state)
            .get(state.selected)
            .map(|r| r.summary.name.clone()),
        McpTab::Catalogue => visible_catalogue(state)
            .get(state.selected)
            .map(|r| r.name.clone()),
    }
}
pub(super) fn select_name(state: &mut McpState, name: &str) {
    let position = match state.tab {
        McpTab::Servers => visible_servers(state)
            .iter()
            .position(|r| r.summary.name == name),
        McpTab::Catalogue => visible_catalogue(state).iter().position(|r| r.name == name),
    };
    let len = match state.tab {
        McpTab::Servers => visible_servers(state).len(),
        McpTab::Catalogue => visible_catalogue(state).len(),
    };
    state.selected = position.unwrap_or(state.selected.min(len.saturating_sub(1)));
}
pub(super) fn tested(
    state: &mut McpState,
    name: String,
    result: Result<McpServersTestResult, Failure>,
    now: Instant,
) -> Vec<McpEffect> {
    let mut effects = vec![];
    let (outcome, mut notice, failed) = match result {
        Ok(reply) if reply.ok => {
            let tools: Vec<_> = reply
                .tools
                .into_iter()
                .map(|t| (short(&t.name), short(&t.description)))
                .collect();
            let prompts = reply.prompts.unwrap_or(0).max(0) as u64;
            let resources = reply.resources.unwrap_or(0).max(0) as u64;
            let text = format!(
                "{name} works: {} tools, {prompts} prompts, {resources} resources.",
                tools.len()
            );
            (
                TestOutcome::Ok {
                    tools,
                    prompts,
                    resources,
                },
                text,
                false,
            )
        }
        Ok(reply) if reply.oauth_needed && reply.oauth_tokens_present == Some(false) => (
            TestOutcome::NeedsSignIn,
            format!("{name} needs sign-in. ctrl+o signs in."),
            true,
        ),
        Ok(reply) => {
            let error = short(reply.error.as_deref().unwrap_or("unknown error"));
            (
                TestOutcome::Failed {
                    error: error.clone(),
                },
                format!("{name} did not connect: {error}"),
                true,
            )
        }
        Err(e) => {
            let notice = if unknown(&e) {
                format!("{name} did not answer the test in 60 s. ctrl+t tries again.")
            } else {
                error_text(state, &name, "mcp.servers.test", &e, false)
            };
            if rpc_code(&e, 4064) {
                effects.extend(refresh());
            }
            (
                TestOutcome::Failed {
                    error: notice.clone(),
                },
                notice,
                true,
            )
        }
    };
    state.tests.insert(name.clone(), outcome);
    if failed {
        append_hint(state, &name, &mut notice);
    } else {
        state.installed_hint.remove(&name);
    }
    if state.open && !state.preserve_notice {
        state.notice = Some(notice);
    }
    state.clock = Some(now);
    effects
}
pub(super) fn append_hint(state: &mut McpState, name: &str, text: &mut String) {
    if state.installed_hint.remove(name) {
        let command = format!("hermes -p {} mcp install {name}", state.profile);
        text.push_str(&format!(
            " Some catalogue servers need values Talaria cannot send yet. {command} asks for them."
        ));
        state.hint_command = Some(command);
    } else {
        state.hint_command = None;
    }
}
