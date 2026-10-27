//! List profiles and explain which ones the running gateway serves.

use std::collections::HashSet;
use std::process::ExitCode;

use clap::{Arg, ArgAction, ColorChoice, Command};
use hermes_client::{Client, DiscoverOptions, Endpoint, EnsurePolicy};
use serde_json::json;
use talaria_app::profiles::{ProfilesService, gateway_view, load_topology};
use talaria_core::profiles::{
    HostTopology, ProfileRow, ProfilesEffect, ProfilesInput, RosterEntry, ServeState, explain,
    join_roster, normalize_profile_name,
};
use talaria_tui::history::sanitize;

/// The typed names retain their order; duplicate names are omitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfilesArgs {
    pub json: bool,
    pub names: Vec<String>,
}

fn command() -> Command {
    Command::new("talaria profiles")
        .bin_name("talaria profiles")
        .about("List Hermes profiles and explain why the gateway does not serve one")
        .color(ColorChoice::Never)
        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue))
        .arg(
            Arg::new("profile")
                .long("profile")
                .value_name("NAME")
                .default_value("default"),
        )
        .arg(Arg::new("names").value_name("NAME").num_args(0..))
}

fn parse_with_profile(args: &[String]) -> Result<(ProfilesArgs, String), clap::Error> {
    let input = args
        .strip_prefix(&["talaria".to_owned(), "profiles".to_owned()])
        .or_else(|| args.strip_prefix(&["profiles".to_owned()]))
        .unwrap_or(args);
    let matched = command().try_get_matches_from(
        std::iter::once("talaria profiles".to_owned()).chain(input.iter().cloned()),
    )?;
    let mut seen = HashSet::new();
    let names = matched
        .get_many::<String>("names")
        .into_iter()
        .flatten()
        .map(|name| normalize_profile_name(name))
        .filter(|name| seen.insert(name.clone()))
        .collect();
    Ok((
        ProfilesArgs {
            json: matched.get_flag("json"),
            names,
        },
        normalize_profile_name(
            matched
                .get_one::<String>("profile")
                .expect("default profile"),
        ),
    ))
}

/// Parse the arguments following the executable name, including `profiles`.
pub fn parse(args: &[String]) -> Result<ProfilesArgs, clap::Error> {
    parse_with_profile(args).map(|(args, _)| args)
}

fn model_cell(row: &ProfileRow) -> String {
    match &row.model {
        Some(model) => format!("{model} · {}", row.provider.as_deref().unwrap_or_default()),
        None => "-".to_owned(),
    }
}

/// Render an untruncated table, counting Unicode scalar values for column widths.
#[must_use]
pub fn render_text(rows: &[ProfileRow]) -> String {
    let mut lines = vec![[
        "NAME".to_owned(),
        "STATE".to_owned(),
        "MODEL".to_owned(),
        "NEXT STEP".to_owned(),
    ]];
    lines.extend(rows.iter().map(|row| {
        let text = explain(&row.state, &row.name);
        [
            row.name.clone(),
            text.label.to_owned(),
            model_cell(row),
            text.next_step,
        ]
    }));
    let widths: Vec<usize> = (0..3)
        .map(|column| {
            lines
                .iter()
                .map(|line| sanitize(&line[column]).chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut output = String::new();
    for line in lines {
        for column in 0..3 {
            let cell = sanitize(&line[column]);
            output.push_str(&cell);
            output.extend(std::iter::repeat_n(
                ' ',
                widths[column] - cell.chars().count() + 2,
            ));
        }
        output.push_str(&sanitize(&line[3]));
        output.push('\n');
    }
    output
}

/// Render the scrubbed domain values without truncating or sanitizing JSON reasons.
#[must_use]
pub fn render_json(rows: &[ProfileRow], warnings: &[String]) -> String {
    let profiles: Vec<_> = rows
        .iter()
        .map(|row| {
            let (state, reason) = match &row.state {
                ServeState::Served => ("served", None),
                ServeState::Parked { reason } => ("parked", Some(reason)),
                ServeState::Stopped => ("stopped", None),
                ServeState::OwnGateway => ("own_gateway", None),
                ServeState::HostSingle { reason } => ("not_served", Some(reason)),
                ServeState::Starting => ("starting", None),
                ServeState::NeverDiscovered => ("not_discovered", None),
                ServeState::Unknown => ("unknown", None),
            };
            let mut fields = vec![
                format!("\"name\":{}", json!(row.name)),
                format!("\"state\":{}", json!(state)),
            ];
            if let Some(reason) = reason {
                fields.push(format!("\"reason\":{}", json!(reason)));
            }
            fields.push(format!(
                "\"next_step\":{}",
                json!(explain(&row.state, &row.name).next_step)
            ));
            if let Some(model) = &row.model {
                fields.push(format!("\"model\":{}", json!(model)));
            }
            if let Some(provider) = &row.provider {
                fields.push(format!("\"provider\":{}", json!(provider)));
            }
            format!("{{{}}}", fields.join(","))
        })
        .collect();
    format!(
        "{{\"profiles\":[{}],\"warnings\":{}}}",
        profiles.join(","),
        json!(warnings)
    )
}

/// Run the read-only command without creating a session or ensuring a gateway.
pub fn run(args: &[String]) -> ExitCode {
    let (args, profile) = match parse_with_profile(args) {
        Ok(value) => value,
        Err(error) => {
            let code = if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                0
            } else {
                2
            };
            if code == 0 {
                print!("{error}");
            } else {
                eprint!("{error}");
            }
            return ExitCode::from(code);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("talaria profiles: could not start the async runtime");
            return ExitCode::from(1);
        }
    };
    ExitCode::from(runtime.block_on(execute(args, &profile)))
}

async fn roster(
    endpoint: Endpoint,
    home: &std::path::Path,
    profile: &str,
) -> Result<Vec<RosterEntry>, String> {
    let profile_id = endpoint
        .resolve_profile(profile)
        .map_err(|_| "could not connect".to_owned())?;
    let ticket = hermes_client::mint_ticket(
        home,
        &endpoint,
        &profile_id,
        hermes_client::Purpose::Interactive,
        hermes_client::Scope::profile(),
    )
    .await
    .map_err(|_| "could not connect".to_owned())?;
    let mut connection = hermes_client::connect(&endpoint, ticket)
        .await
        .map_err(|_| "could not connect".to_owned())?;
    hermes_client::handshake(&mut connection)
        .await
        .map_err(|_| "could not connect".to_owned())?;
    let (client, _inbound) = Client::start(connection.into_parts());
    let service = ProfilesService::new(client.clone(), home.to_owned(), profile.to_owned());
    let result = match service
        .execute(ProfilesEffect::List { generation: 1 })
        .await
    {
        Some(ProfilesInput::Listed(_, result)) => result,
        _ => Err("unreadable reply".to_owned()),
    };
    client.close().await;
    result
}

async fn execute(args: ProfilesArgs, profile: &str) -> u8 {
    let home = hermes_client::default_home();
    let options = DiscoverOptions {
        ensure: EnsurePolicy::Never,
        ..DiscoverOptions::default()
    };
    let endpoint = match hermes_client::discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(error) => {
            eprintln!(
                "talaria profiles: {error}\nNext step: {}",
                error.next_step()
            );
            return 1;
        }
    };
    let gateway = gateway_view(&endpoint, &home);
    let (roster, topology) = tokio::join!(
        roster(endpoint, &home, profile),
        load_topology(&home, profile)
    );
    let warnings = warnings(&roster, &topology);
    let all = join_roster(
        roster.as_ref().ok().map(Vec::as_slice),
        &gateway,
        topology.as_ref().ok(),
        &[],
        0,
    );
    let rows = if args.names.is_empty() {
        all
    } else {
        let mut rows = Vec::new();
        let mut missing = false;
        for name in args.names {
            match all.iter().find(|row| row.name == name) {
                Some(row) => rows.push(row.clone()),
                None => {
                    eprintln!("talaria profiles: no profile named {}", sanitize(&name));
                    missing = true;
                }
            }
        }
        if missing {
            return 2;
        }
        rows
    };
    for warning in &warnings {
        eprintln!("talaria profiles: {}", sanitize(warning));
    }
    if args.json {
        println!("{}", render_json(&rows, &warnings));
    } else {
        print!("{}", render_text(&rows));
    }
    0
}

fn warnings(
    roster: &Result<Vec<RosterEntry>, String>,
    topology: &Result<HostTopology, String>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Err(reason) = roster {
        warnings.push(format!("Hermes did not return the profile list ({reason}). Showing only what the gateway serves."));
    }
    match topology {
        Err(reason)=>warnings.push(format!("Some reasons are unavailable: the gateway did not answer /api/status ({reason}).")),
        Ok(topology) if topology.own_gateway.is_none()=>warnings.push("Some reasons are unavailable: the gateway did not say which profiles run their own gateway.".to_owned()),
        _=>{}
    }
    warnings
}
