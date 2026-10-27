//! Read profile capability lists without creating a session.

use std::{process::ExitCode, sync::Arc};

use hermes_client::rest::{NativeHttpMinter, TicketMinter};
use hermes_client::{DiscoverOptions, EnsurePolicy, discover_with};
use serde_json::{Value, json};
use talaria_app::{capabilities::CapabilitiesService, connection::ConnectionService};
use talaria_core::capabilities::{
    CapData, CapEffect, CapError, CapInput, CapTab, CapText, PluginSource, PluginStatus,
    Provenance, TextArgs, format_text, redact_paths,
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilitiesArgs {
    pub list: String,
    pub json: bool,
    pub profile: String,
}

fn tab(list: &str) -> Option<CapTab> {
    match list {
        "skills" => Some(CapTab::Skills),
        "toolsets" => Some(CapTab::Toolsets),
        "plugins" => Some(CapTab::Plugins),
        _ => None,
    }
}

fn unknown_list(list: &str) -> String {
    format!(
        "talaria capabilities: unknown list {}; use skills, toolsets or plugins",
        public_value(list, "", "")
    )
}

pub fn parse(args: &[String]) -> Result<CapabilitiesArgs, String> {
    let mut tokens = args.iter();
    if tokens.next().map(String::as_str) != Some("capabilities") {
        return Err(
            "Usage: talaria capabilities <skills|toolsets|plugins> [--json] [--profile <name>]"
                .into(),
        );
    }
    let Some(list) = tokens.next() else {
        return Err(
            "Usage: talaria capabilities <skills|toolsets|plugins> [--json] [--profile <name>]"
                .into(),
        );
    };
    if tab(list).is_none() {
        return Err(unknown_list(list));
    }
    let mut parsed = CapabilitiesArgs {
        list: list.clone(),
        json: false,
        profile: "default".into(),
    };
    while let Some(option) = tokens.next() {
        match option.as_str() {
            "--json" => parsed.json = true,
            "--profile" => {
                parsed.profile = tokens
                    .next()
                    .filter(|name| !name.trim().is_empty())
                    .ok_or_else(|| "talaria capabilities: --profile needs a name".to_owned())?
                    .trim()
                    .to_owned();
            }
            _ => {
                return Err(format!(
                    "talaria capabilities: unexpected argument {}",
                    public_value(option, "", "")
                ));
            }
        }
    }
    Ok(parsed)
}

fn public_text(text: &str, hermes_home: &str, user_home: &str) -> String {
    redact_paths(
        &talaria_tui::history::sanitize(text),
        hermes_home,
        user_home,
    )
}

fn public_value(text: &str, hermes_home: &str, user_home: &str) -> String {
    let text = public_text(text, hermes_home, user_home);
    if text.chars().count() <= 300 {
        text
    } else {
        text.chars().take(299).chain(std::iter::once('…')).collect()
    }
}

fn provenance(source: &Provenance) -> &str {
    match source {
        Provenance::Hub => "hub",
        Provenance::Bundled => "bundled",
        Provenance::External => "external",
        Provenance::Local => "local",
        Provenance::Other(source) if source.is_empty() => "-",
        Provenance::Other(source) => source,
    }
}

fn plugin_source(source: &PluginSource) -> &str {
    match source {
        PluginSource::Bundled => "bundled",
        PluginSource::User => "user",
        PluginSource::Git => "git",
        PluginSource::Entrypoint => "entrypoint",
        PluginSource::Other(source) => source,
    }
}

fn plugin_status(status: &PluginStatus) -> &str {
    match status {
        PluginStatus::Enabled => "enabled",
        PluginStatus::Disabled => "disabled",
        PluginStatus::NotEnabled => "not enabled",
        PluginStatus::Other(status) => status,
    }
}

fn columns(rows: Vec<Vec<String>>) -> String {
    let width = rows.first().map_or(0, Vec::len);
    let mut widths = vec![0; width];
    for row in &rows {
        for (index, value) in row.iter().enumerate() {
            widths[index] = widths[index].max(UnicodeWidthStr::width(value.as_str()));
        }
    }
    let mut output = String::new();
    for row in rows {
        for (index, value) in row.iter().enumerate() {
            output.push_str(value);
            if index + 1 < width {
                output.extend(std::iter::repeat_n(
                    ' ',
                    widths[index] + 2 - UnicodeWidthStr::width(value.as_str()),
                ));
            }
        }
        output.push('\n');
    }
    output
}

pub fn format_list(
    args: &CapabilitiesArgs,
    data: &CapData,
    hermes_home: &str,
    user_home: &str,
) -> String {
    let clean = |text: &str| public_value(text, hermes_home, user_home);
    let (header, text_rows, json_rows): (Vec<&str>, Vec<Vec<String>>, Vec<Value>) = match data {
        CapData::Skills(skills) => {
            let mut skills: Vec<_> = skills.iter().collect();
            skills.sort_by(|left, right| match (&left.category, &right.category) {
                (Some(left_category), Some(right_category)) => left_category
                    .cmp(right_category)
                    .then_with(|| left.name.cmp(&right.name)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left.name.cmp(&right.name),
            });
            let text = skills
                .iter()
                .map(|row| {
                    vec![
                        clean(&row.name),
                        if row.enabled { "on" } else { "off" }.into(),
                        clean(provenance(&row.provenance)),
                        row.category.as_deref().map_or_else(|| "-".into(), clean),
                    ]
                })
                .collect();
            let json = skills
                .iter()
                .map(|row| {
                    json!({
                        "name": clean(&row.name), "enabled": row.enabled,
                        "provenance": clean(provenance(&row.provenance)),
                        "category": row.category.as_deref().map(clean),
                        "usage": row.usage, "description": clean(&row.description),
                    })
                })
                .collect();
            (vec!["NAME", "STATE", "SOURCE", "CATEGORY"], text, json)
        }
        CapData::Toolsets(toolsets) => {
            let mut toolsets: Vec<_> = toolsets.iter().collect();
            toolsets.sort_by(|left, right| left.name.cmp(&right.name));
            let text = toolsets
                .iter()
                .map(|row| {
                    vec![
                        clean(&row.name),
                        if row.enabled { "on" } else { "off" }.into(),
                        row.tools.len().to_string(),
                        if row.configured { "ok" } else { "needed" }.into(),
                    ]
                })
                .collect();
            let json = toolsets.iter().map(|row| json!({
                "name": clean(&row.name), "label": clean(&row.label), "enabled": row.enabled,
                "configured": row.configured, "platform": clean(&row.platform),
                "tools": row.tools.iter().map(|name| clean(name)).collect::<Vec<_>>(),
            })).collect();
            (vec!["NAME", "STATE", "TOOLS", "SETUP"], text, json)
        }
        CapData::Plugins(plugins) => {
            let mut plugins: Vec<_> = plugins.iter().collect();
            plugins.sort_by(|left, right| left.name.cmp(&right.name));
            let text = plugins
                .iter()
                .map(|row| {
                    let source = match &row.source {
                        PluginSource::User | PluginSource::Git => "yours",
                        PluginSource::Entrypoint => "package",
                        other => plugin_source(other),
                    };
                    vec![
                        clean(&row.name),
                        clean(&plugin_status(&row.status).replace(' ', "-")),
                        if row.version.is_empty() {
                            "-".into()
                        } else {
                            clean(&row.version)
                        },
                        clean(source),
                    ]
                })
                .collect();
            let json = plugins.iter().map(|row| json!({
                "name": clean(&row.name), "key": row.key.as_deref().map(clean),
                "version": clean(&row.version), "source": clean(plugin_source(&row.source)),
                "status": clean(plugin_status(&row.status)), "description": clean(&row.description),
            })).collect();
            (vec!["NAME", "STATE", "VERSION", "SOURCE"], text, json)
        }
        CapData::Hub(_) => return String::new(),
    };
    if args.json {
        format!(
            "{}\n",
            json!({ "profile": clean(&args.profile), args.list.clone(): json_rows })
        )
    } else {
        let mut rows = vec![header.into_iter().map(str::to_owned).collect()];
        rows.extend(text_rows);
        columns(rows)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    pub status: u8,
    pub stdout: String,
    pub stderr: String,
}

pub async fn collect(
    args: &CapabilitiesArgs,
    service: &CapabilitiesService,
    hermes_home: &str,
    user_home: &str,
) -> CommandOutput {
    let Some(tab) = tab(&args.list) else {
        return CommandOutput {
            status: 2,
            stdout: String::new(),
            stderr: format!("{}\n", unknown_list(&args.list)),
        };
    };
    let inputs = service
        .execute(CapEffect::Fetch {
            profile: args.profile.clone(),
            tab,
            generation: 1,
        })
        .await;
    for input in inputs {
        if let CapInput::Loaded { result, .. } = input {
            return match result {
                Ok(data) => CommandOutput {
                    status: 0,
                    stdout: format_list(args, &data, hermes_home, user_home),
                    stderr: String::new(),
                },
                Err(error) => CommandOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: format!("{}\n", service.read_error_text(&error, tab, &args.profile)),
                },
            };
        }
    }
    CommandOutput {
        status: 1,
        stdout: String::new(),
        stderr: format!(
            "{}\n",
            service.read_error_text(&CapError::BadResult, tab, &args.profile)
        ),
    }
}

pub async fn run(mut args: CapabilitiesArgs) -> ExitCode {
    if tab(&args.list).is_none() {
        eprintln!("{}", unknown_list(&args.list));
        return ExitCode::from(2);
    }
    let home = hermes_client::default_home();
    let user_home = std::env::var("HOME").unwrap_or_default();
    let hermes_home = home.to_string_lossy().into_owned();
    let endpoint = match discover_with(
        &home,
        &DiscoverOptions {
            ensure: EnsurePolicy::Never,
            ..DiscoverOptions::default()
        },
    )
    .await
    {
        Ok(endpoint) => Arc::new(endpoint),
        Err(error) => {
            let text = talaria_app::failure::LaunchFailure::from(error).render();
            eprint!("{}", public_text(&text, &hermes_home, &user_home));
            return ExitCode::from(1);
        }
    };
    if endpoint.resolve_profile(&args.profile).is_err() {
        eprintln!(
            "{}",
            format_text(
                CapText::ProfileUnavailable,
                &TextArgs {
                    profile: public_value(&args.profile, &hermes_home, &user_home),
                    ..TextArgs::default()
                }
            )
        );
        return ExitCode::from(1);
    }
    let launch = talaria_app::launch::Launch {
        mode: talaria_app::launch::StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: args.profile.clone(),
        folder: std::env::current_dir().unwrap_or_default(),
        record: None,
        bench_replay: None,
        no_ensure: true,
    };
    let connection = match ConnectionService::open(&launch, &home, None).await {
        Ok(connection) => connection,
        Err(error) => {
            eprint!("{}", public_text(&error.render(), &hermes_home, &user_home));
            return ExitCode::from(1);
        }
    };
    args.profile = connection.profile_name.clone();
    let origin = endpoint.api_origin().to_owned();
    let profiles = endpoint
        .profiles()
        .iter()
        .map(|profile| profile.name.clone())
        .collect();
    let make_minter = Arc::new(move |name: &str| -> Arc<dyn TicketMinter> {
        // The service checks the immutable served-profile list before invoking this factory.
        let profile = endpoint
            .resolve_profile(name)
            .expect("served profile was validated");
        Arc::new(NativeHttpMinter::new(
            home.clone(),
            endpoint.clone(),
            profile,
        ))
    });
    let service = CapabilitiesService::new(
        Some(origin),
        profiles,
        make_minter,
        connection.link.client(),
        hermes_home.clone(),
        user_home.clone(),
    );
    let result = collect(&args, &service, &hermes_home, &user_home).await;
    connection.link.close().await;
    print!("{}", result.stdout);
    eprint!("{}", result.stderr);
    ExitCode::from(result.status)
}
