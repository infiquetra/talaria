use super::reply::{cut, sanitize_text, scrub_change};
use super::{
    CapKind, Change, Context, McpSource, Proposal, Screen, SecretDetector, Section, WITHHELD,
};
use crate::settings::{
    FieldView, ReadOnlyReason, ValueKind, apply_effect, contains_secret_key, env_reference_names,
    is_secret_key, value_kind,
};
use serde_json::Value;
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq)]
pub enum RowStatus {
    Ready,
    NoChange,
    Invalid(String),
    Refused(String),
    HandOff,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub change: Change,
    pub status: RowStatus,
    pub destructive: bool,
    pub text: String,
    pub detail: Vec<String>,
    pub effect_label: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalError {
    WrongProfile { named: String, connected: String },
    TooMany(usize),
}
pub fn validate(
    proposal: &Proposal,
    ctx: &Context,
    detect: SecretDetector,
) -> Result<Vec<Row>, ProposalError> {
    if let Some(profile) = &proposal.profile
        && profile != &ctx.profile
    {
        return Err(ProposalError::WrongProfile {
            named: sanitize_text(profile, detect),
            connected: sanitize_text(&ctx.profile, detect),
        });
    }
    if proposal.changes.len() > super::MAX_CHANGES {
        return Err(ProposalError::TooMany(proposal.changes.len()));
    }
    let mut identities = std::collections::BTreeMap::new();
    let mut rows = Vec::new();
    for original in &proposal.changes {
        let mut change = original.clone();
        let hit = scrub_change(&mut change, detect);
        if !hit
            && let Change::McpAdd { name, .. } = &mut change
            && let Ok(normalized) = crate::mcp::validate_server_name(name, &[])
        {
            *name = normalized;
        }
        let sensitive = hit
            && matches!(
                change,
                Change::Set { .. }
                    | Change::CronAdd { .. }
                    | Change::McpAdd { .. }
                    | Change::McpEnable { .. }
                    | Change::McpRemove { .. }
            );
        let mut row = Row {
            text: row_text(&change, ctx, detect),
            detail: vec![],
            effect_label: None,
            destructive: destructive(&change, ctx),
            change,
            status: RowStatus::Ready,
        };
        if let Change::Set { key, .. } = &row.change {
            row.effect_label = Some(apply_effect(key).0.label().into());
        }
        if matches!(
            row.change,
            Change::McpAdd { .. } | Change::McpEnable { .. } | Change::McpRemove { .. }
        ) {
            row.effect_label = Some("new sessions; /mcp apply reloads this one".into());
        }
        row.status = if sensitive {
            refused()
        } else if let Some(section) = missing_section(&row.change, ctx) {
            invalid(format!("could not read {section} before the proposal"))
        } else if let Some(identity) = identity(&row.change) {
            if let Some(index) = identities.get(&identity) {
                invalid(format!("duplicate of row {index}"))
            } else {
                identities.insert(identity, rows.len() + 1);
                validate_one(&mut row, ctx, detect)
            }
        } else {
            validate_one(&mut row, ctx, detect)
        };
        if let RowStatus::Refused(_) = row.status {
            let screen = match row.change {
                Change::McpAdd { .. } | Change::McpEnable { .. } | Change::McpRemove { .. } => {
                    "/mcp"
                }
                Change::CronAdd { .. } => "/cron",
                _ => "/providers",
            };
            row.status = RowStatus::Refused(format!(
                "refused: carries a secret; add it on {screen} instead"
            ));
            // Refused changes never retain strings, including values the detector does not know.
            scrub_change(&mut row.change, |_| true);
        }
        row.detail.extend(change_detail(&row.change, detect));
        rows.push(row);
    }
    Ok(rows)
}
fn refused() -> RowStatus {
    RowStatus::Refused(
        "Talaria will not put a key or secret in a change. Use the screen that owns it.".into(),
    )
}
fn invalid(reason: impl Into<String>) -> RowStatus {
    RowStatus::Invalid(reason.into())
}
fn change_detail(change: &Change, detect: SecretDetector) -> Vec<String> {
    let (why, owner) = match change {
        Change::Set { key, why, .. } if key == "memory.provider" => (why, "/memory-settings"),
        Change::Set { why, .. } => (why, "/config"),
        Change::McpAdd { why, .. }
        | Change::McpEnable { why, .. }
        | Change::McpRemove { why, .. } => (why, "/mcp"),
        Change::Toggle { why, .. } => (why, "/capabilities"),
        Change::CronAdd { why, .. }
        | Change::CronPause { why, .. }
        | Change::CronResume { why, .. }
        | Change::CronRemove { why, .. } => (why, "/cron"),
        Change::MainModel { why, .. } => (why, "/model"),
        Change::Open { why, screen, .. } => (
            why,
            match screen {
                Screen::Providers => "/providers",
                Screen::Mcp => "/mcp",
                Screen::Capabilities => "/capabilities",
                Screen::Cron => "/cron",
                Screen::Profiles => "/profiles",
                Screen::Connectors => "/connectors",
                Screen::Settings => "/config",
            },
        ),
        _ => return Vec::new(),
    };
    let mut detail = Vec::new();
    if let Some(why) = why {
        detail.push(sanitize_text(why, detect));
    }
    if let Change::Set { key, value, .. } = change {
        detail.push(if is_secret_key(key) || contains_secret_key(value) {
            "<secret>".into()
        } else {
            sanitize_text(&value.to_string(), detect)
        });
    }
    detail.push(format!("owner screen: {owner}"));
    detail
}
pub(crate) fn lookup<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.').try_fold(value, |v, key| v.get(key))
}
fn available<T>(section: &Section<T>) -> &[T] {
    match section {
        Section::Available(items) => items,
        Section::Unavailable(_) => &[],
    }
}
fn missing_section(change: &Change, ctx: &Context) -> Option<&'static str> {
    let i = &ctx.inventory;
    let mut needs = Vec::new();
    match change {
        Change::McpAdd { source, .. } => {
            needs.push((
                "mcp_servers",
                matches!(i.mcp_servers, Section::Available(_)),
            ));
            if matches!(source, McpSource::Preset(_)) {
                needs.push((
                    "mcp_catalogue",
                    matches!(i.mcp_catalogue, Section::Available(_)),
                ));
            }
        }
        Change::McpEnable { .. } | Change::McpRemove { .. } => needs.push((
            "mcp_servers",
            matches!(i.mcp_servers, Section::Available(_)),
        )),
        Change::Toggle { kind, .. } => needs.push(match kind {
            CapKind::Skill => ("skills", matches!(i.skills, Section::Available(_))),
            CapKind::Toolset => ("toolsets", matches!(i.toolsets, Section::Available(_))),
            CapKind::Plugin => ("plugins", matches!(i.plugins, Section::Available(_))),
        }),
        Change::CronAdd { .. }
        | Change::CronPause { .. }
        | Change::CronResume { .. }
        | Change::CronRemove { .. } => {
            needs.push(("cron_jobs", matches!(i.cron_jobs, Section::Available(_))))
        }
        Change::MainModel { .. } => {
            needs.push(("main_model", matches!(i.main_model, Section::Available(_))));
            needs.push(("routes", matches!(i.routes, Section::Available(_))));
        }
        _ => {}
    }
    needs
        .into_iter()
        .find_map(|(name, ok)| (!ok).then_some(name))
}
fn identity(change: &Change) -> Option<String> {
    Some(match change {
        Change::Set { key, .. } => format!("set:{key}"),
        Change::McpAdd { name, .. }
        | Change::McpEnable { name, .. }
        | Change::McpRemove { name, .. } => format!("mcp:{name}"),
        Change::Toggle { kind, name, .. } => format!("toggle:{kind:?}:{name}"),
        Change::CronAdd { name, .. } => format!("cron_add:{name}"),
        Change::CronPause { job_id, .. }
        | Change::CronResume { job_id, .. }
        | Change::CronRemove { job_id, .. } => format!("cron:{job_id}"),
        Change::MainModel { .. } => "main_model".into(),
        Change::Open { screen, target, .. } => format!("open:{screen:?}:{target:?}"),
        _ => return None,
    })
}
fn validate_one(row: &mut Row, ctx: &Context, detect: SecretDetector) -> RowStatus {
    let i = &ctx.inventory;
    match &mut row.change {
        Change::Set { key, value, .. } => {
            if contains_secret_key(value) {
                return refused();
            }
            if key == "memory.provider" {
                return RowStatus::HandOff;
            }
            if key == "model" || key.starts_with("model.") {
                return invalid("use main_model");
            }
            let Some(field) = ctx.schema.fields.iter().find(|f| f.key == *key) else {
                // Unknown scalar switches are not settings, even when their names end in
                // "_key". Text and compound payloads still cross the credential boundary.
                if is_secret_key(key)
                    && (value.is_string() || value.is_array() || value.is_object())
                {
                    return refused();
                }
                return invalid("not a Hermes setting");
            };
            if is_secret_key(key) {
                return refused();
            }
            let current = lookup(&ctx.effective, key);
            let kind = value_kind(
                field,
                current,
                lookup(&ctx.defaults, key),
                lookup(&ctx.raw, key),
            );
            match &kind {
                ValueKind::ReadOnly(ReadOnlyReason::OwnedElsewhere) => {
                    return invalid("set on its own screen: models");
                }
                ValueKind::ReadOnly(ReadOnlyReason::EnvReference(names)) => {
                    return invalid(format!(
                        "comes from {} in the key file",
                        sanitize_text(&names.join(", "), detect)
                    ));
                }
                ValueKind::ReadOnly(ReadOnlyReason::HoldsSecret) => return refused(),
                ValueKind::ReadOnly(_) => return invalid("this setting cannot be edited here"),
                _ => {}
            }
            match check_value(&kind, field, value) {
                Ok(v) => *value = v,
                Err(reason) => return invalid(reason),
            }
            if current == Some(value) {
                RowStatus::NoChange
            } else {
                RowStatus::Ready
            }
        }
        Change::McpAdd { name, source, .. } => {
            if matches!(source, McpSource::Forbidden(_)) {
                return refused();
            }
            if available(&i.mcp_servers).iter().any(|s| s.name == *name) {
                return invalid("already exists; use mcp_enable");
            }
            match crate::mcp::validate_server_name(name, &[]) {
                Ok(clean) => *name = clean,
                Err(_) => return invalid("use 1 to 64 letters, digits, dot, dash or underscore"),
            }
            match source {
                McpSource::Preset(preset) => match available(&i.mcp_catalogue)
                    .iter()
                    .find(|p| p.name == *preset)
                {
                    None => invalid("not in Hermes's catalogue"),
                    Some(p) if !p.requires.is_empty() => {
                        row.text = format!("next: sign in to {preset} (/mcp)");
                        RowStatus::HandOff
                    }
                    Some(_) => RowStatus::Ready,
                },
                McpSource::Command { command, .. } if command.trim().is_empty() => {
                    invalid("command must not be empty")
                }
                McpSource::Url { url, .. } if !allowed_url(url) => {
                    invalid("URL must use HTTPS or HTTP on loopback")
                }
                _ => RowStatus::Ready,
            }
        }
        Change::McpEnable { name, enabled, .. } => {
            match available(&i.mcp_servers).iter().find(|s| s.name == *name) {
                None => invalid("no MCP server with that name"),
                Some(s) if s.plugin => invalid(format!(
                    "provided by plugin {}",
                    sanitize_text(&s.name, detect)
                )),
                Some(s) if s.enabled == *enabled => RowStatus::NoChange,
                Some(_) => RowStatus::Ready,
            }
        }
        Change::McpRemove { name, .. } => {
            match available(&i.mcp_servers).iter().find(|s| s.name == *name) {
                None => invalid("no MCP server with that name"),
                Some(s) if s.plugin => invalid(format!(
                    "provided by plugin {}",
                    sanitize_text(&s.name, detect)
                )),
                Some(_) => RowStatus::Ready,
            }
        }
        Change::Toggle {
            kind,
            name,
            enabled,
            ..
        } => {
            let list = match kind {
                CapKind::Skill => &i.skills,
                CapKind::Toolset => &i.toolsets,
                CapKind::Plugin => &i.plugins,
            };
            match available(list).iter().find(|s| s.name == *name) {
                None => invalid("no item with that name"),
                Some(s) if s.enabled == *enabled => RowStatus::NoChange,
                Some(_) => RowStatus::Ready,
            }
        }
        Change::CronAdd {
            name,
            schedule,
            prompt,
            repeat,
            deliver,
            ..
        } => {
            row.detail.extend([
                cut(prompt, 300),
                format!(
                    "repeat: {}",
                    repeat.map_or_else(|| "unlimited".into(), |n| n.to_string())
                ),
                "delivers to local".into(),
            ]);
            if !(1..=64).contains(&name.chars().count()) {
                invalid("name must be 1 to 64 characters")
            } else if schedule.trim().is_empty() {
                invalid("schedule must not be empty")
            } else if !(1..=4_000).contains(&prompt.chars().count()) {
                invalid("prompt must be 1 to 4,000 characters")
            } else if repeat.is_some_and(|r| !(1..=9_999).contains(&r)) {
                invalid("repeat must be 1 to 9,999")
            } else if deliver
                .as_ref()
                .is_some_and(|d| !d.is_empty() && d != "local")
            {
                row.text = format!("next: set delivery for cron job \"{name}\" (/cron)");
                RowStatus::HandOff
            } else {
                RowStatus::Ready
            }
        }
        Change::CronPause { job_id, .. }
        | Change::CronResume { job_id, .. }
        | Change::CronRemove { job_id, .. } => {
            let Some(job) = available(&i.cron_jobs).iter().find(|j| j.job_id == *job_id) else {
                return invalid(format!("no cron job {job_id}"));
            };
            if matches!(&row.change, Change::CronPause { .. }) && job.state == "paused"
                || matches!(&row.change, Change::CronResume { .. }) && job.state == "active"
            {
                RowStatus::NoChange
            } else {
                RowStatus::Ready
            }
        }
        Change::MainModel {
            provider, model, ..
        } => {
            let route = available(&i.routes)
                .iter()
                .find(|r| r.slug == *provider && r.authenticated);
            let Some(route) = route else {
                row.text = format!("next: add a key for {provider} (/providers)");
                return RowStatus::HandOff;
            };
            if model.trim().is_empty() {
                return invalid("model must not be empty");
            }
            if !route.models.is_empty() && !route.models.contains(model) {
                row.detail.push("not in the model list".into());
            }
            if available(&i.main_model)
                .iter()
                .any(|m| m.provider == *provider && m.model == *model)
            {
                RowStatus::NoChange
            } else {
                RowStatus::Ready
            }
        }
        Change::Open { .. } => RowStatus::HandOff,
        Change::Malformed { op, field } => invalid(if *field == "source" {
            format!("{op}: needs exactly one of preset, command, url")
        } else {
            format!("{op}: {field} is missing or has the wrong type")
        }),
        Change::Unknown { op } => invalid(format!("Talaria cannot apply {op}")),
    }
}
fn allowed_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.chars().any(char::is_whitespace)
    {
        return false;
    }
    if scheme == "https" {
        return true;
    }
    let host = if let Some(after) = authority.strip_prefix('[') {
        after.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    scheme == "http" && matches!(host, "localhost" | "127.0.0.1" | "::1")
}
fn destructive(change: &Change, ctx: &Context) -> bool {
    match change {
        Change::McpRemove { .. }
        | Change::CronRemove { .. }
        | Change::CronPause { .. }
        | Change::McpEnable { enabled: false, .. }
        | Change::Toggle { enabled: false, .. } => true,
        Change::Set { key, value, .. } => {
            value.as_array().is_some_and(Vec::is_empty)
                && lookup(&ctx.effective, key)
                    .and_then(Value::as_array)
                    .is_some_and(|v| !v.is_empty())
        }
        _ => false,
    }
}
fn shown(value: Option<&Value>, detect: SecretDetector) -> String {
    value.map_or_else(
        || "unset".into(),
        |v| cut(&sanitize_text(&v.to_string(), detect), 60),
    )
}
fn row_text(change: &Change, ctx: &Context, detect: SecretDetector) -> String {
    let text = match change {
        Change::Set { key, .. } if key == "memory.provider" => {
            "next: choose the memory provider (/memory-settings)".into()
        }
        Change::Set { key, value, .. } => {
            let raw = lookup(&ctx.raw, key);
            let old = raw
                .filter(|v| !env_reference_names(v).is_empty())
                .or_else(|| lookup(&ctx.effective, key));
            let old = if is_secret_key(key) || old.is_some_and(contains_secret_key) {
                "<secret>".into()
            } else {
                shown(old, detect)
            };
            let new = if is_secret_key(key) || contains_secret_key(value) {
                "<secret>".into()
            } else {
                shown(Some(value), detect)
            };
            format!("set {key}: {} → {}", old, new)
        }
        Change::McpAdd { name, source, .. } => format!(
            "add MCP server {name} ({})",
            match source {
                McpSource::Preset(_) => "from Hermes's catalogue".into(),
                McpSource::Command { command, .. } => format!("runs {command}"),
                McpSource::Url { url, .. } => format!("at {url}"),
                McpSource::Forbidden(_) => "secret fields refused".into(),
            }
        ),
        Change::McpEnable { name, enabled, .. } => {
            format!("turn MCP server {name} {}", on_off(*enabled))
        }
        Change::McpRemove { name, .. } => format!("remove MCP server {name}"),
        Change::Toggle {
            kind,
            name,
            enabled,
            ..
        } => format!(
            "turn {} {name} {}",
            match kind {
                CapKind::Skill => "skill",
                CapKind::Toolset => "toolset",
                CapKind::Plugin => "plugin",
            },
            on_off(*enabled)
        ),
        Change::CronAdd { name, schedule, .. } => {
            format!("add cron job \"{name}\" at \"{schedule}\"")
        }
        Change::CronPause { job_id, .. }
        | Change::CronResume { job_id, .. }
        | Change::CronRemove { job_id, .. } => {
            let name = available(&ctx.inventory.cron_jobs)
                .iter()
                .find(|j| j.job_id == *job_id)
                .map_or("", |j| j.name.as_str());
            let action = match change {
                Change::CronPause { .. } => "pause",
                Change::CronResume { .. } => "resume",
                _ => "remove",
            };
            format!(
                "{action} cron job \"{}\" ({job_id})",
                sanitize_text(name, detect)
            )
        }
        Change::MainModel {
            provider, model, ..
        } => format!("use {provider}/{model} as the main model for new sessions"),
        Change::Open { screen, target, .. } => {
            let target = target.as_deref().unwrap_or("");
            match screen {
                Screen::Providers => format!("next: add a key for {target} (/providers)"),
                Screen::Mcp => format!("next: sign in to {target} (/mcp)"),
                Screen::Capabilities => format!("next: install {target} (/capabilities)"),
                Screen::Cron => "next: open cron jobs (/cron)".into(),
                Screen::Profiles => format!("next: create profile {target} (/profiles)"),
                Screen::Connectors => format!("next: connect {target} (/connectors)"),
                Screen::Settings => format!("next: open settings {target} (/config)"),
            }
        }
        Change::Malformed { op, .. } | Change::Unknown { op } => op.clone(),
    };
    sanitize_text(&text, detect)
}
fn on_off(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}
pub fn check_value(kind: &ValueKind, field: &FieldView, value: &Value) -> Result<Value, String> {
    let ok = match kind {
        ValueKind::Bool => value.is_boolean(),
        ValueKind::Integer => {
            if value.is_i64() || value.is_u64() {
                return Ok(value.clone());
            }
            if let Some(n) = value
                .as_f64()
                .filter(|n| n.fract() == 0.0 && *n >= i64::MIN as f64 && *n < i64::MAX as f64)
            {
                return Ok(Value::from(n as i64));
            }
            if let Some(n) = value
                .as_f64()
                .filter(|n| n.fract() == 0.0 && *n >= 0.0 && *n < u64::MAX as f64)
            {
                return Ok(Value::from(n as u64));
            }
            false
        }
        ValueKind::Float => value.is_number(),
        ValueKind::Text => value
            .as_str()
            .is_some_and(|s| !s.chars().any(|c| c.is_control() && c != '\t')),
        ValueKind::StringList => value
            .as_array()
            .is_some_and(|v| v.iter().all(Value::is_string)),
        ValueKind::JsonList => value.is_array(),
        ValueKind::Nullable => !value.is_array() && !value.is_object(),
        ValueKind::Choice => {
            value.is_string()
                && field
                    .options
                    .as_ref()
                    .is_some_and(|options| options.contains(value))
        }
        ValueKind::ReadOnly(_) => false,
    };
    if ok {
        Ok(value.clone())
    } else {
        Err(match kind {
            ValueKind::Bool => "expected a boolean",
            ValueKind::Integer => "expected a whole number",
            ValueKind::Float => "expected a number",
            ValueKind::Text => "expected text without control characters",
            ValueKind::StringList => "expected an array of strings",
            ValueKind::JsonList => "expected an array",
            ValueKind::Nullable => "expected null or a scalar",
            ValueKind::Choice => "expected a listed choice",
            ValueKind::ReadOnly(_) => "this setting cannot be edited here",
        }
        .into())
    }
}
pub fn withhold<'a>(text: &'a str, detect: SecretDetector) -> Cow<'a, str> {
    if detect(text) {
        Cow::Borrowed(WITHHELD)
    } else {
        Cow::Borrowed(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::{
        ConfigureEffect, ConfigureInput, ConfigureKey, ConfigureState, Inventory, plan_apply,
    };
    use crate::settings::ConfigSchemaView;
    use serde_json::json;
    use std::time::Instant;

    fn unavailable<T>() -> Section<T> {
        Section::Unavailable("not needed by this setting".into())
    }

    #[test]
    fn secret_text_remains_refused_after_reducer_scrubbing() {
        let schema = ConfigSchemaView::from_json_str(
            r#"{"fields":{"custom.label":{"type":"string","category":"custom"}},"category_order":["custom"]}"#,
        )
        .unwrap();
        let context = Context {
            profile: "test".into(),
            hermes_version: None,
            schema,
            effective: json!({"custom":{"label":"before"}}),
            raw: json!({"custom":{"label":"before"}}),
            defaults: json!({"custom":{"label":"before"}}),
            inventory: Inventory {
                mcp_servers: unavailable(),
                mcp_catalogue: unavailable(),
                skills: unavailable(),
                toolsets: unavailable(),
                plugins: unavailable(),
                cron_jobs: unavailable(),
                providers: unavailable(),
                main_model: unavailable(),
                routes: unavailable(),
                profiles: unavailable(),
                connectors: unavailable(),
            },
        };
        let reply = "```talaria-configure\n{\"kind\":\"proposal\",\"summary\":\"Update label\",\"changes\":[{\"op\":\"set\",\"key\":\"custom.label\",\"value\":\"sensitive-test-value\"}]}\n```";
        let detector: SecretDetector = |text| text.contains("sensitive-test-value");
        assert!(!detector(WITHHELD));
        let mut state = ConfigureState::from_reply(context.clone(), reply, detector);
        assert!(matches!(state.rows()[0].status, RowStatus::Refused(_)));
        assert!(plan_apply(state.rows(), &[true], &context).is_empty());
        assert!(!format!("{:?}", state.rows()).contains("sensitive-test-value"));
        let now = Instant::now();
        for key in [ConfigureKey::Char('a'), ConfigureKey::Char('y')] {
            let effects = state.update(ConfigureInput::Key(key), now);
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, ConfigureEffect::Apply { .. }))
            );
        }
    }
}
