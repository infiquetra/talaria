//! Read-only command-line configuration proposals.

use std::{collections::VecDeque, path::PathBuf, process::ExitCode, sync::Arc, time::Instant};

use hermes_client::{
    channel::Inbound,
    discovery::{DiscoverOptions, EnsurePolicy, default_home, discover_with},
    liveness::{LinkEvent, LinkState},
    rest::{NativeHttpMinter, TicketMinter},
};
use serde_json::{Value, json};
use talaria_app::{
    config::load_user_config,
    configure::{ConfigureService, SiblingBackends, service::detect_secret},
    connection::ConnectionService,
    connectors::ConnectorsConsent,
    launch::{Launch, StartMode},
    settings::{MinterFactory, SettingsService},
};
use talaria_core::{
    configure::{
        Change, ConfigureEffect, ConfigureInput, ConfigureState, Phase, RowStatus, WITHHELD,
    },
    session::{Event, PromptKind, SessionId},
};

pub fn is_command(args: &[String]) -> bool {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "configure" => return true,
            "--profile" | "--provider" | "--model" => {
                if args.next().is_none() {
                    return false;
                }
            }
            value
                if ["--profile=", "--provider=", "--model="]
                    .iter()
                    .any(|prefix| value.starts_with(prefix)) => {}
            _ => return false,
        }
    }
    false
}

struct Options {
    profile: String,
    model: Option<(String, String)>,
    request: String,
    json: bool,
}

fn parse(args: &[String]) -> Result<Options, &'static str> {
    let mut profile = None;
    let mut provider = None;
    let mut model = None;
    let mut i = 0;
    while i < args.len() && args[i] != "configure" {
        let arg = &args[i];
        let (name, value) = if let Some((name, value)) = arg.split_once('=') {
            (name, value.to_owned())
        } else {
            i += 1;
            (
                arg.as_str(),
                args.get(i).ok_or("Missing option value.")?.clone(),
            )
        };
        if value.trim().is_empty() {
            return Err("Option values must not be empty.");
        }
        match name {
            "--profile" if profile.is_none() => profile = Some(value),
            "--provider" if provider.is_none() => provider = Some(value),
            "--model" if model.is_none() => model = Some(value),
            _ => return Err("Invalid or repeated option."),
        }
        i += 1;
    }
    if i == args.len() {
        return Err("Missing configure command.");
    }
    i += 1;
    let mut dry_run = false;
    let mut json = false;
    let mut request = Vec::new();
    for arg in &args[i..] {
        match arg.as_str() {
            "--dry-run" if !dry_run => dry_run = true,
            "--json" if !json => json = true,
            option if option.starts_with('-') => return Err("Unknown configure option."),
            _ => request.push(arg.as_str()),
        }
    }
    if !dry_run {
        return Err("Applying needs the terminal UI: run talaria and type /configure.");
    }
    let request = request.join(" ");
    if request.trim().is_empty() {
        return Err("A configuration request is required.");
    }
    if request.chars().count() > 4_000 {
        return Err("The request is too long (at most 4,000 characters).");
    }
    let model = match (provider, model) {
        (Some(provider), Some(model)) => Some((provider, model)),
        (None, None) => None,
        _ => return Err("--provider and --model must be given together."),
    };
    Ok(Options {
        profile: profile.unwrap_or_else(|| "default".into()),
        model,
        request,
        json,
    })
}

pub fn run(args: &[String]) -> ExitCode {
    let options = match parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            output_error(options.json, "Could not start the runtime.", 1);
            return ExitCode::from(1);
        }
    };
    ExitCode::from(runtime.block_on(run_async(options)))
}

fn safe(text: &str) -> String {
    if detect_secret(text) {
        WITHHELD.into()
    } else {
        talaria_tui::history::sanitize(text).into_owned()
    }
}

fn output_error(as_json: bool, error: &str, exit: u8) {
    if as_json {
        println!(
            "{}",
            json!({"version": 1, "error": safe(error), "exit": exit})
        );
    } else if exit == 130 {
        println!("Interrupted.");
    } else {
        println!("{}", safe(error));
    }
}

async fn run_async(mut options: Options) -> u8 {
    if detect_secret(&options.request) {
        output_error(
            options.json,
            "That request contains something that looks like a key or token. Talaria never sends secrets to a model. Add keys with /providers.",
            1,
        );
        return 1;
    }
    if options.model.is_none() {
        options.model = std::env::var_os("HOME")
            .map(PathBuf::from)
            .and_then(|home| load_user_config(&home).config.configure);
    }
    let home = default_home();
    let launch = Launch {
        mode: StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: options.profile.clone(),
        folder: std::env::current_dir().unwrap_or_default(),
        record: None,
        bench_replay: None,
        no_ensure: true,
    };
    let opened = tokio::select! {
        result = ConnectionService::open(&launch, &home, None) => match result {
            Ok(opened) => opened,
            Err(error) => { output_error(options.json, &error.what, 1); return 1; }
        },
        _ = tokio::signal::ctrl_c() => { output_error(options.json, "interrupted", 130); return 130; }
    };
    options.profile = opened.profile_name.clone();
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
        Err(_) => {
            output_error(options.json, "Could not discover the gateway.", 1);
            return 1;
        }
    };
    let origin = Some(endpoint.api_origin().to_owned());
    let profiles: Vec<String> = endpoint
        .profiles()
        .iter()
        .map(|profile| profile.name.clone())
        .collect();
    let factory: MinterFactory = Arc::new(move |profile: &str| -> Arc<dyn TicketMinter> {
        let profile = endpoint
            .resolve_profile(profile)
            .expect("served profile checked before minting");
        Arc::new(NativeHttpMinter::new(
            home.clone(),
            endpoint.clone(),
            profile,
        ))
    });
    let settings = Arc::new(SettingsService::new(
        origin.clone(),
        profiles.clone(),
        factory.clone(),
    ));
    let client = opened.link.client();
    let backends = SiblingBackends::new(
        settings,
        client.clone(),
        ConnectorsConsent::default(),
        origin,
        profiles,
        factory,
    );
    let service = Arc::new(ConfigureService::new(
        Some(client),
        backends,
        options.profile.clone(),
    ));
    let mut state =
        ConfigureState::new_with_prefix(detect_secret, talaria_app::launch::new_launch_id());
    let mut queue: VecDeque<ConfigureEffect> = state
        .update(
            ConfigureInput::Open {
                request: options.request.clone(),
                profile: options.profile.clone(),
                connected: true,
                model: options.model.clone(),
            },
            Instant::now(),
        )
        .into();
    let mut inbound = opened.inbound;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut busy = false;
    loop {
        if !busy && let Some(effect) = queue.pop_front() {
            if matches!(effect, ConfigureEffect::Apply { .. }) {
                service.close().await;
                output_error(options.json, "The dry run cannot apply changes.", 1);
                return 1;
            }
            let service = Arc::clone(&service);
            let tx = tx.clone();
            busy = true;
            tokio::spawn(async move {
                let _ = tx.send(service.execute(effect).await);
            });
        }
        let view = state.view(Instant::now());
        if !busy
            && queue.is_empty()
            && matches!(
                view.phase,
                Phase::Review
                    | Phase::Answer
                    | Phase::Cannot
                    | Phase::NoProposal
                    | Phase::Failed
                    | Phase::Cancelled
            )
        {
            let exit = print_result(&options, &state);
            service.close().await;
            return exit;
        }
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                let _ = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                    service.close().await;
                    if busy {
                        let _ = rx.recv().await;
                    }
                }).await;
                output_error(options.json, "interrupted", 130);
                return 130;
            }
            Some(inputs) = rx.recv(), if busy => {
                busy = false;
                for input in inputs { queue.extend(state.update(input, Instant::now())); }
            }
            item = inbound.recv() => match item {
                Some(LinkEvent::Inbound { item: Inbound::Event(inbound), .. }) => {
                    if let Some(id) = inbound.raw.get("session_id").and_then(Value::as_str)
                        && Some(id) == state.hidden_session_id() {
                        let event = Event::Session { session_id: SessionId::new(id.to_owned()), frame: inbound.frame, raw: inbound.raw };
                        service.observe(&event);
                        queue.extend(state.update(ConfigureInput::Session(event), Instant::now()));
                        if state.view(Instant::now()).phase == Phase::Question
                            && let Some(key) = state.session().and_then(|session| session.open_prompts().find(|prompt| prompt.kind == PromptKind::Clarify).map(|prompt| prompt.key.clone())) {
                            queue.extend(state.update(ConfigureInput::ClarifyAnswered { key, answer: String::new() }, Instant::now()));
                        }
                    }
                }
                Some(LinkEvent::Inbound { item: Inbound::ServerRequest(request), .. }) => {
                    let id = request.id.clone();
                    service.hold_request(request);
                    queue.extend(state.update(ConfigureInput::ServerRequest { request_id: id, method: "server request".into() }, Instant::now()));
                }
                Some(LinkEvent::State(LinkState::Reconnecting { .. } | LinkState::Failed { .. } | LinkState::Closed)) | None => {
                    queue.extend(state.update(ConfigureInput::LinkLost, Instant::now()));
                }
                _ => {}
            },
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(state.next_deadline().unwrap_or_else(|| Instant::now() + std::time::Duration::from_secs(3600)))) => {
                queue.extend(state.update(ConfigureInput::Tick, Instant::now()));
            }
        }
    }
}

fn operation(change: &Change) -> &str {
    match change {
        Change::Set { .. } => "set",
        Change::McpAdd { .. } => "mcp_add",
        Change::McpEnable { .. } => "mcp_enable",
        Change::McpRemove { .. } => "mcp_remove",
        Change::Toggle { .. } => "toggle",
        Change::CronAdd { .. } => "cron_add",
        Change::CronPause { .. } => "cron_pause",
        Change::CronResume { .. } => "cron_resume",
        Change::CronRemove { .. } => "cron_remove",
        Change::MainModel { .. } => "main_model",
        Change::Open { .. } => "open",
        Change::Malformed { op, .. } | Change::Unknown { op } => op,
    }
}

fn print_result(options: &Options, state: &ConfigureState) -> u8 {
    let view = state.view(Instant::now());
    if matches!(view.phase, Phase::Failed | Phase::Cancelled) {
        let message = view
            .lines
            .first()
            .map_or("The configuration agent failed.", |line| line.text.as_str());
        output_error(options.json, message, 1);
        return 1;
    }
    let kind = state.result_kind().unwrap_or("unreadable");
    let exit = match kind {
        "proposal"
            if state
                .rows()
                .iter()
                .any(|row| row.status == RowStatus::Ready) =>
        {
            0
        }
        "proposal" | "answer" | "cannot" => 3,
        _ => 4,
    };
    let rows: Vec<Value> = state.rows().iter().map(|row| {
        let (status, reason) = match &row.status {
            RowStatus::Ready => ("ready", None), RowStatus::NoChange => ("no_change", None),
            RowStatus::Invalid(reason) => ("invalid", Some(safe(reason))), RowStatus::Refused(reason) => ("refused", Some(safe(reason))),
            RowStatus::HandOff => ("hand_off", None),
        };
        json!({"op": safe(operation(&row.change)), "status": status, "text": safe(&row.text), "reason": reason, "destructive": row.destructive})
    }).collect();
    if options.json {
        let model = options
            .model
            .as_ref()
            .map(|(provider, model)| format!("{}/{}", safe(provider), safe(model)))
            .or_else(|| {
                view.title
                    .strip_prefix(&format!(
                        "Configure · profile {} · ",
                        safe(&options.profile)
                    ))
                    .map(safe)
            });
        println!(
            "{}",
            json!({"version": 1, "profile": safe(&options.profile), "model": model, "kind": kind,
            "summary": safe(state.summary()), "rows": rows, "context_rounds": state.context_rounds()})
        );
    } else if rows.is_empty() {
        println!("{}", safe(state.summary()));
    } else {
        for row in state.rows() {
            let mark = match &row.status {
                RowStatus::Ready if row.destructive => "[ ] !",
                RowStatus::Ready => "[x]",
                RowStatus::NoChange => "=",
                RowStatus::Invalid(_) | RowStatus::Refused(_) => "✗",
                RowStatus::HandOff => "→",
            };
            println!("{mark} {}", safe(&row.text));
        }
    }
    exit
}
