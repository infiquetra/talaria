//! `record-fixture record | replay | scan`. Thin wrapper over the library.
//!
//! Exit codes: 0 success; 1 failure (a scenario could not be produced, or the scan found
//! something); 2 a precondition is absent or the gateway is unreachable. Output never holds a
//! secret, an absolute path, a login name or a host name.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use hermes_client::{
    ConnectOptions, GatewayIdentity, Purpose, Recorder, RecorderOptions, ReplayItem, ReplaySource,
    Scope, Speed, connect_with, default_home, discover_with, handshake, local_forbidden_literals,
    mint_ticket, scan_for_secrets, scan_literals, ws_url,
};
use record_fixture::{
    AttemptResult, DiscoveryFacts, DriverConfig, EnvFacts, FLEET_REGISTRY, NO_TODO_TOOL, Preflight,
    PreflightInput, SCENARIOS, SCRIPTED_SCENARIOS, SESSION_PICKER, Scenario, ScratchDir,
    build_scenario_substitutions, check_evidence, never_ensure, preflight, run_scenario,
    scrub_recording, turn_status,
};

#[derive(Parser)]
#[command(about = "Record, replay and scan the live frame fixtures")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Record the scenarios against the local Hermes gateway.
    Record {
        #[arg(long, default_value = "fixtures/frames")]
        out: PathBuf,
        /// Record only these scenarios (repeatable). Default: every named scenario.
        #[arg(long)]
        scenario: Vec<String>,
        /// The Hermes home. Default: the one Talaria would use.
        #[arg(long)]
        home: Option<PathBuf>,
        #[arg(long, default_value = "fixture")]
        profile: String,
        #[arg(long, default_value = "minimax-m3.1-flash")]
        model: String,
        #[arg(long, default_value = "ollama-cloud")]
        provider: String,
        #[arg(long, default_value_t = 3)]
        attempts: usize,
        #[arg(long = "attempt-timeout-s", default_value_t = 120)]
        attempt_timeout_s: u64,
    },
    /// Replay one recording with no socket and print what it held.
    Replay {
        file: PathBuf,
        #[arg(long, default_value = "instant")]
        speed: String,
    },
    /// Scan the `*.jsonl` files of a directory for secrets and machine-specific text.
    Scan {
        #[arg(default_value = "fixtures/frames")]
        dir: PathBuf,
    },
}

/// How a run ended, before it becomes an exit code.
enum Outcome {
    Success,
    Failure,
    Precondition,
}

impl From<Outcome> for ExitCode {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Success => ExitCode::SUCCESS,
            Outcome::Failure => ExitCode::from(1),
            Outcome::Precondition => ExitCode::from(2),
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let outcome = match cli.command {
        Command::Record {
            out,
            scenario,
            home,
            profile,
            model,
            provider,
            attempts,
            attempt_timeout_s,
        } => {
            let settings = RecordSettings {
                out,
                scenarios: scenario,
                home: home.unwrap_or_else(default_home),
                profile,
                model,
                provider,
                attempts: attempts.max(1),
                attempt_timeout: Duration::from_secs(attempt_timeout_s),
            };
            // Ctrl-C ends the run through the same drop guards as an error does.
            tokio::select! {
                outcome = record(&settings) => outcome,
                _ = tokio::signal::ctrl_c() => {
                    eprintln!("interrupted; nothing was kept");
                    Outcome::Failure
                }
            }
        }
        Command::Replay { file, speed } => replay(&file, &speed).await,
        Command::Scan { dir } => scan(&dir),
    };
    outcome.into()
}

struct RecordSettings {
    out: PathBuf,
    scenarios: Vec<String>,
    home: PathBuf,
    profile: String,
    model: String,
    provider: String,
    attempts: usize,
    attempt_timeout: Duration,
}

fn login_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default()
}

async fn record(settings: &RecordSettings) -> Outcome {
    let mut scenarios = Vec::new();
    let names: Vec<&str> = if settings.scenarios.is_empty() {
        SCENARIOS.to_vec()
    } else {
        settings.scenarios.iter().map(String::as_str).collect()
    };
    for name in names {
        match Scenario::named(name) {
            Some(scenario) => scenarios.push(scenario),
            None => {
                // The picker scenario is recorded by name, so the operator is told of it too.
                let mut known = SCENARIOS.to_vec();
                known.push(SESSION_PICKER);
                known.push(FLEET_REGISTRY);
                known.extend_from_slice(&SCRIPTED_SCENARIOS);
                eprintln!(
                    "unknown scenario {name}; the scenarios are: {}",
                    known.join(", ")
                );
                return Outcome::Precondition;
            }
        }
    }
    let login = login_name();
    let input = PreflightInput {
        hermes_home: settings.home.clone(),
        profile: settings.profile.clone(),
        login_name: login.clone(),
        host_name: gethostname::gethostname().to_string_lossy().into_owned(),
    };
    let checked = match preflight(&input).await {
        Ok(checked) => checked,
        Err(error) => {
            eprintln!("{}", error.message);
            return Outcome::Precondition;
        }
    };
    let Ok(user_home) = std::env::var("HOME") else {
        eprintln!("HOME is not set; it is needed to keep the home directory out of recordings");
        return Outcome::Precondition;
    };
    let scratch = match ScratchDir::create() {
        Ok(scratch) => scratch,
        Err(_) => {
            eprintln!("the scratch project could not be created");
            return Outcome::Failure;
        }
    };
    if std::fs::create_dir_all(&settings.out).is_err() {
        eprintln!("the output directory could not be created");
        return Outcome::Failure;
    }
    let env = EnvFacts {
        home: user_home,
        login_name: login,
    };
    let mut forbidden = local_forbidden_literals();
    forbidden.extend(
        checked
            .other_profiles
            .iter()
            .filter(|name| name.chars().count() >= 4)
            .cloned(),
    );

    let mut outcome = Outcome::Success;
    for scenario in &scenarios {
        let mut last = String::new();
        let mut recorded = false;
        for number in 1..=settings.attempts {
            // A scripted scenario changes files and git state, so each attempt gets its own folders.
            let own = if SCRIPTED_SCENARIOS.contains(&scenario.name.as_str()) {
                match ScratchDir::create_for(&scenario.name) {
                    Ok(own) => Some(own),
                    Err(_) => {
                        eprintln!("the scratch project could not be created");
                        return Outcome::Failure;
                    }
                }
            } else {
                None
            };
            let attempt = Attempt {
                settings,
                checked: &checked,
                env: &env,
                scratch: own.as_ref().unwrap_or(&scratch),
                forbidden: &forbidden,
                scenario,
                number,
            };
            match attempt.run().await {
                Step::Recorded => {
                    println!("{}: recorded on attempt {number} ok", scenario.name);
                    recorded = true;
                    break;
                }
                Step::Retry(reason) => {
                    eprintln!("{}: attempt {number} failed: {reason}", scenario.name);
                    last = reason;
                }
                Step::Stop(reason) => {
                    eprintln!("{}: {reason}", scenario.name);
                    return Outcome::Failure;
                }
                Step::Precondition(reason) => {
                    eprintln!("{reason}");
                    return Outcome::Precondition;
                }
            }
        }
        if !recorded {
            if scenario.name == "todo-checklist" && last == NO_TODO_TOOL {
                eprintln!(
                    "{}: the model did not use the todo tool in {} attempts",
                    scenario.name, settings.attempts
                );
            } else {
                eprintln!(
                    "{}: not recorded in {} attempts; last failure: {last}",
                    scenario.name, settings.attempts
                );
            }
            outcome = Outcome::Failure;
        }
    }
    outcome
}

/// What one attempt decided.
enum Step {
    Recorded,
    Retry(String),
    /// A recording that would leak: the script stops, nothing is kept.
    Stop(String),
    Precondition(String),
}

struct Attempt<'a> {
    settings: &'a RecordSettings,
    checked: &'a Preflight,
    env: &'a EnvFacts,
    scratch: &'a ScratchDir,
    forbidden: &'a [String],
    scenario: &'a Scenario,
    number: usize,
}

/// Deletes the hidden temporary recording unless it was moved into place.
struct TempFile(Option<PathBuf>);

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Attempt<'_> {
    async fn run(&self) -> Step {
        let name = &self.scenario.name;
        let hidden = self
            .settings
            .out
            .join(format!(".{name}-{}.jsonl.tmp", self.number));
        let mut temp = TempFile(None);
        let recorded = self.record_to(&hidden, &mut temp).await;
        if let Err(step) = recorded {
            return step;
        }
        if scrub_file(&hidden, name).is_err() {
            return Step::Retry("the recording could not be scrubbed".to_owned());
        }
        let source = match ReplaySource::open(&hidden) {
            Ok(source) => source,
            Err(error) => return Step::Retry(format!("the recording did not open: {error}")),
        };
        if let Err(reason) = check_evidence(self.scenario, &source) {
            return Step::Retry(reason);
        }
        let Ok(text) = std::fs::read_to_string(&hidden) else {
            return Step::Retry("the recording could not be read back".to_owned());
        };
        let file_name = format!("{name}.jsonl");
        let mut hits: Vec<String> = scan_for_secrets(&text)
            .into_iter()
            .chain(scan_literals(&text, self.forbidden))
            .map(|hit| format!("{file_name}:{} {}", hit.line, hit.rule))
            .collect();
        if !hits.is_empty() {
            hits.sort();
            hits.dedup();
            for hit in &hits {
                println!("{hit}");
            }
            return Step::Stop("the scan found text that must not be committed".to_owned());
        }
        if std::fs::rename(&hidden, self.settings.out.join(&file_name)).is_err() {
            return Step::Retry("the recording could not be moved into place".to_owned());
        }
        temp.0 = None;
        Step::Recorded
    }

    async fn record_to(&self, hidden: &Path, temp: &mut TempFile) -> Result<(), Step> {
        let unreachable = |what: String| {
            Step::Precondition(format!(
                "the gateway could not be reached ({what}); check that it is running"
            ))
        };
        let home = &self.settings.home;
        let endpoint = discover_with(home, &never_ensure())
            .await
            .map_err(|error| unreachable(format!("{error}")))?;
        let profile = endpoint
            .resolve_profile(&self.settings.profile)
            .map_err(|error| unreachable(error.to_string()))?;
        let ticket = mint_ticket(
            home,
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .map_err(|error| unreachable(error.to_string()))?;
        let mut connection = connect_with(&endpoint, ticket, &ConnectOptions::default())
            .await
            .map_err(|error| unreachable(error.to_string()))?;
        let facts = DiscoveryFacts {
            profile_id: self.checked.profile_id.clone(),
            api_origin: self.checked.api_origin.clone(),
        };
        let options = RecorderOptions {
            endpoint: ws_url(endpoint.api_origin()),
            subprotocols: vec!["hermes-gateway-v1".to_owned()],
            gateway: Some(GatewayIdentity {
                code_version: endpoint.code_version().unwrap_or_default().to_owned(),
                code_sha: endpoint.code_sha().unwrap_or_default().to_owned(),
            }),
            scenario: Some(self.scenario.name.clone()),
            expected_statuses: self.scenario.expected_statuses.clone(),
            substitutions: build_scenario_substitutions(
                &self.scenario.name,
                &facts,
                self.env,
                self.scratch,
            ),
            queue_capacity: None,
        };
        let recorder = Recorder::create(hidden, options)
            .map_err(|error| Step::Retry(format!("the recording could not start: {error}")))?;
        temp.0 = Some(hidden.to_path_buf());
        connection.set_recorder(recorder.clone(), 1);
        if let Err(error) = handshake(&mut connection).await {
            let _ = recorder.finish();
            return Err(unreachable(error.to_string()));
        }
        let (client, mut inbound) = hermes_client::Client::start(connection.into_parts());
        let config = DriverConfig {
            model: self.settings.model.clone(),
            provider: self.settings.provider.clone(),
            scratch: self.scratch.path().to_path_buf(),
            second: self.scratch.second().map(Path::to_path_buf),
            attempt_timeout: self.settings.attempt_timeout,
        };
        let result = run_scenario(&client, &mut inbound, self.scenario, &config).await;
        client.close().await;
        drop(inbound);
        if let Err(error) = recorder.finish() {
            return Err(Step::Retry(format!("the recording failed: {error}")));
        }
        match result {
            AttemptResult::Ok => Ok(()),
            AttemptResult::Failed(reason) => Err(Step::Retry(reason)),
            AttemptResult::PreconditionAbsent(reason) => Err(Step::Precondition(reason)),
        }
    }
}

/// Apply the scenario's value-level scrub to a finished recording, line by line.
fn scrub_file(path: &Path, scenario: &str) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let scrubbed = scrub_recording(scenario, &text);
    if scrubbed != text {
        std::fs::write(path, scrubbed)?;
    }
    Ok(())
}

async fn replay(file: &Path, speed: &str) -> Outcome {
    let speed = match Speed::parse(speed) {
        Ok(speed) => speed,
        Err(error) => {
            eprintln!("{error}");
            return Outcome::Failure;
        }
    };
    let source = match ReplaySource::open(file) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{error}");
            return Outcome::Failure;
        }
    };
    let (sink, mut items) = tokio::sync::mpsc::channel::<ReplayItem>(256);
    let collector = tokio::spawn(async move {
        let mut statuses = Vec::new();
        while let Some(item) = items.recv().await {
            let Ok(frame) = serde_json::from_str::<serde_json::Value>(&item.text) else {
                continue;
            };
            if frame["method"] == "event" && frame["params"]["type"] == "message.complete" {
                statuses.push(turn_status(&frame["params"]["payload"]));
            }
        }
        statuses
    });
    let sent = match source.run(speed, sink).await {
        Ok(sent) => sent,
        Err(error) => {
            eprintln!("{error}");
            return Outcome::Failure;
        }
    };
    let statuses = collector.await.unwrap_or_default();
    println!(
        "replayed {sent} inbound frames; final statuses: {}",
        statuses.join(", ")
    );
    Outcome::Success
}

fn scan(dir: &Path) -> Outcome {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => {
            eprintln!("the directory to scan could not be read");
            return Outcome::Failure;
        }
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort();

    let long_enough = |value: Option<String>| value.is_some_and(|text| text.chars().count() >= 4);
    let labels = [
        ("home", long_enough(std::env::var("HOME").ok())),
        (
            "login",
            long_enough(
                std::env::var("USER")
                    .ok()
                    .or_else(|| std::env::var("LOGNAME").ok()),
            ),
        ),
        (
            "host",
            long_enough(Some(
                gethostname::gethostname().to_string_lossy().into_owned(),
            )),
        ),
    ];
    for (label, checked) in labels {
        if !checked {
            println!("note: {label} not checked (unset or under four characters)");
        }
    }
    let forbidden = local_forbidden_literals();

    let mut found = false;
    for path in files {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(text) = std::fs::read_to_string(&path) else {
            println!("{name}: could not be read");
            found = true;
            continue;
        };
        let mut hits: Vec<(usize, &str)> = scan_for_secrets(&text)
            .into_iter()
            .chain(scan_literals(&text, &forbidden))
            .map(|hit| (hit.line, hit.rule))
            .collect();
        hits.sort();
        hits.dedup();
        for (line, rule) in hits {
            println!("{name}:{line} {rule}");
            found = true;
        }
    }
    if found {
        Outcome::Failure
    } else {
        Outcome::Success
    }
}
