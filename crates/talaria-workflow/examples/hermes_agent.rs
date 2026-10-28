//! Run an explicitly configured workflow agent through the durable journal.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use talaria_workflow::{
    engine::*,
    hermes::{
        self, ExplicitLaunch, HermesPort, LaunchDefaults, LinkWireFactory, PortConfig, SetupError,
        WireFactory,
    },
    journal::*,
};
use tokio::sync::watch;

fn output(value: Value) {
    println!("{value}");
}
fn options() -> Result<BTreeMap<String, String>, String> {
    let mut args = std::env::args().skip(1);
    let mut options = BTreeMap::new();
    while let Some(flag) = args.next() {
        if ![
            "--profile",
            "--model",
            "--provider",
            "--effort",
            "--toolsets",
            "--skills",
            "--max-turns",
            "--cwd",
            "--prompt",
            "--script",
            "--schema",
            "--runs",
            "--resume",
            "--record",
            "--scripted",
        ]
        .contains(&flag.as_str())
        {
            return Err("unknown option".into());
        }
        let value = args.next().ok_or("option needs a value")?;
        if options.insert(flag, value).is_some() {
            return Err("duplicate option".into());
        }
    }
    Ok(options)
}
fn text_file(path: &str) -> Result<String, (u8, String)> {
    std::fs::read_to_string(path).map_err(|_| (64, "input file could not be read".into()))
}
fn source(options: &BTreeMap<String, String>) -> Result<String, (u8, String)> {
    if let Some(script) = options.get("--script") {
        if options.contains_key("--prompt") {
            return Err((64, "choose prompt or script".into()));
        }
        return text_file(script);
    }
    let Some(prompt) = options.get("--prompt") else {
        if let Some(resume) = options.get("--resume") {
            let dir = RunDir::open(Path::new(resume))
                .map_err(|_| (1, "run could not be opened".into()))?;
            return text_file(
                &dir.path()
                    .join("scripts")
                    .join(format!("{}.js", dir.manifest().workflow.script_sha256))
                    .to_string_lossy(),
            );
        }
        return Err((64, "provide prompt or script".into()));
    };
    let mut agent = serde_json::Map::new();
    for (flag, key) in [
        ("--model", "model"),
        ("--provider", "provider"),
        ("--effort", "effort"),
        ("--profile", "profile"),
    ] {
        if let Some(value) = options.get(flag) {
            agent.insert(key.into(), json!(value));
        }
    }
    for (flag, key) in [("--toolsets", "toolsets"), ("--skills", "skills")] {
        if let Some(value) = options.get(flag) {
            agent.insert(
                key.into(),
                json!(
                    value
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                ),
            );
        }
    }
    if let Some(turns) = options.get("--max-turns") {
        agent.insert(
            "maxTurns".into(),
            json!(
                turns
                    .parse::<u32>()
                    .map_err(|_| (64, "invalid max-turns".into()))?
            ),
        );
    }
    if let Some(path) = options.get("--schema") {
        agent.insert(
            "schema".into(),
            serde_json::from_str(&text_file(path)?)
                .map_err(|_| (64, "schema is not JSON".into()))?,
        );
    }
    Ok(format!(
        "export const meta = {{name: 'hermes-agent', description: 'Run an explicit Hermes agent', phases: []}};\nreturn await agent({}, {});",
        json!(prompt),
        Value::Object(agent)
    ))
}
fn public_path(path: &Path) -> String {
    if std::env::var_os("HOME").is_some_and(|home| path.starts_with(PathBuf::from(home))) {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "run".into())
    } else {
        path.to_string_lossy().into_owned()
    }
}
fn tail(
    path: PathBuf,
    mut offset: u64,
    mut submissions: BTreeMap<(String, u32, u32), String>,
    mut stop: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Ok((lines, next)) = JournalReader::read_from(&path, offset) {
                offset = next;
                for line in lines {
                    match line.record {
                        Record::SessionBound { session_id, .. } => {
                            output(json!({"event":"session_created","session_id":session_id}))
                        }
                        Record::TurnAdmitted {
                            key,
                            attempt,
                            turn,
                            admission_id,
                            ..
                        } => output(
                            json!({"event":"admitted","turn":turn,"submission_id":submissions.get(&(key,attempt,turn)),"admission_id":admission_id}),
                        ),
                        Record::TurnSettled {
                            turn,
                            outcome,
                            usage,
                            ..
                        } => output(
                            json!({"event":"settled","turn":turn,"outcome":outcome,"usage":usage}),
                        ),
                        Record::TurnIntent {
                            key,
                            attempt,
                            turn,
                            submission_id,
                            text,
                            ..
                        } => {
                            submissions.insert((key, attempt, turn), submission_id);
                            if turn == 1
                                && let Some(text) = text
                            {
                                output(
                                    json!({"event":"correction","errors":text.lines().filter_map(|s|s.strip_prefix("- ")).collect::<Vec<_>>() }),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            if *stop.borrow() {
                break;
            }
            tokio::select! {_=tokio::time::sleep(std::time::Duration::from_millis(10))=>{},_=stop.changed()=>{}}
        }
    })
}
#[cfg(feature = "mock")]
fn scripted(path: &str) -> Result<Arc<dyn WireFactory>, (u8, String)> {
    Ok(Arc::new(
        hermes::mock_wire::ScriptedWire::load(Path::new(path))
            .map_err(|_| (64, "scripted wire is invalid".into()))?,
    ))
}
#[cfg(not(feature = "mock"))]
fn scripted(_: &str) -> Result<Arc<dyn WireFactory>, (u8, String)> {
    Err((64, "built without the mock feature".into()))
}
async fn run(options: BTreeMap<String, String>) -> Result<u8, (u8, String)> {
    let source = source(&options)?;
    let parsed = parse_script(&source).map_err(|_| (64, "workflow script is invalid".into()))?;
    let resumed_dir = options
        .get("--resume")
        .map(|path| RunDir::open(Path::new(path)))
        .transpose()
        .map_err(|_| (1, "run could not be opened".into()))?;
    let saved = resumed_dir.as_ref().map(RunDir::manifest);
    let profile = options
        .get("--profile")
        .cloned()
        .or_else(|| saved.map(|manifest| manifest.profile.clone()))
        .or_else(|| options.contains_key("--scripted").then(|| "fixture".into()))
        .ok_or((64, "profile is required".into()))?;
    let cwd = options
        .get("--cwd")
        .map(PathBuf::from)
        .or_else(|| {
            saved
                .and_then(|manifest| manifest.hermes["launch_defaults"]["cwd"].as_str())
                .map(PathBuf::from)
        })
        .unwrap_or(
            std::env::current_dir().map_err(|_| (64, "working directory unavailable".into()))?,
        );
    let cwd = cwd
        .canonicalize()
        .map_err(|_| (64, "working directory unavailable".into()))?;
    let toolsets = options
        .get("--toolsets")
        .map(|s| {
            s.split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .or_else(|| {
            saved
                .and_then(|manifest| manifest.hermes["launch_defaults"]["toolsets"].as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
        });
    let defaults = LaunchDefaults {
        profile: profile.clone(),
        cwd,
        toolsets,
        worktrees: None,
    };
    let mut recorder = None;
    let cfg = PortConfig::default();
    let factory: Arc<dyn WireFactory> = if let Some(path) = options.get("--scripted") {
        scripted(path)?
    } else {
        let mut factory = LinkWireFactory::new(cfg.home.clone());
        if let Some(path) = options.get("--record") {
            let recording = hermes_client::Recorder::create(
                Path::new(path),
                hermes_client::RecorderOptions::default(),
            )
            .map_err(|_| (64, "recording could not be created".into()))?;
            let tap = recording.clone();
            factory = factory.on_connect(Arc::new(move |connection, generation| {
                connection.set_recorder(tap.clone(), generation)
            }));
            recorder = Some(recording);
        }
        Arc::new(factory)
    };
    let (port, feed) = HermesPort::with_wires(cfg, factory, &profile)
        .await
        .map_err(|e| match e {
            SetupError::NotFound(_) => (2, "Hermes gateway not found".into()),
            SetupError::Unsupported { missing } => (
                2,
                format!("gateway unsupported: missing {}", missing.join(", ")),
            ),
            SetupError::Connect(_) => (2, "Hermes gateway connection failed".into()),
        })?;
    drop(feed);
    let resumed = resumed_dir.is_some();
    let dir = if let Some(dir) = resumed_dir {
        dir
    } else {
        let parent = if let Some(path) = options.get("--runs") {
            PathBuf::from(path)
        } else {
            tempfile::Builder::new()
                .prefix("hermes-agent-")
                .tempdir()
                .map_err(|_| (1, "temporary run folder unavailable".into()))?
                .keep()
        };
        std::fs::create_dir_all(&parent)
            .map_err(|_| (1, "run parent could not be created".into()))?;
        let started_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| (1, "system clock unavailable".into()))?
            .as_millis() as i64;
        RunDir::create(
            &parent,
            NewManifest {
                workflow_name: parsed.meta.name.clone(),
                args: None,
                started_at_ms,
                seed: 0,
                profile: profile.clone(),
                on_unknown: OnUnknown::Wait,
                talaria_version: env!("CARGO_PKG_VERSION").into(),
                hermes: json!({"launch_defaults":{"cwd":defaults.cwd,"toolsets":defaults.toolsets}}),
                roster: json!({}),
                approval_policy: json!({"finite":true}),
            },
            &source,
        )
        .map_err(|_| (1, "run could not be created".into()))?
    };
    let (journal, summary, _) = RunJournal::open(dir.clone(), &source)
        .await
        .map_err(|_| (1, "journal could not be opened".into()))?;
    let journal = Arc::new(journal);
    output(json!({"event":"run","run_dir":public_path(dir.path())}));
    if resumed {
        output(json!({"event":"resumed","in_flight":summary.in_flight}));
    }
    let config = journal
        .engine_config(&ExplicitLaunch::new(defaults.clone()))
        .map_err(|_| (1, "journal replay configuration failed".into()))?;
    let backend = Arc::new(hermes::backend(
        journal.clone(),
        port.clone(),
        defaults,
        NoGate,
        RetryPolicy::default(),
    ));
    let mut offset = 0;
    let mut submissions = BTreeMap::new();
    if resumed {
        let (lines, end) = JournalReader::read_from(dir.path(), 0)
            .map_err(|_| (1, "journal could not be read".into()))?;
        offset = end;
        for line in lines {
            if let Record::TurnIntent {
                key,
                attempt,
                turn,
                submission_id,
                ..
            } = line.record
            {
                submissions.insert((key, attempt, turn), submission_id);
            }
        }
    }
    let handle = spawn(parsed, config, backend, Arc::new(MapResolver::new()));
    let events = journal.forward_events(handle.events);
    let mut outcome = handle.outcome;
    let (stop, signal) = watch::channel(false);
    let tail = tail(dir.path().into(), offset, submissions, signal);
    let result = tokio::select! {result=&mut outcome=>result.map_err(|_|(1,"workflow engine stopped".into()))?,_=tokio::signal::ctrl_c()=>{handle.cancel.cancel();RunOutcome::Aborted{reason:"detached".into()}}};
    events
        .await
        .map_err(|_| (1, "journal event task stopped".into()))?
        .map_err(|_| (1, "journal event write failed".into()))?;
    let kind = match &result {
        RunOutcome::Completed { .. } => RunOutcomeKind::Completed,
        RunOutcome::Failed { .. } => RunOutcomeKind::Failed,
        RunOutcome::Aborted { .. } => RunOutcomeKind::Aborted,
    };
    journal
        .finish(kind)
        .await
        .map_err(|_| (1, "journal finish failed".into()))?;
    stop.send_replace(true);
    let _ = tail.await;
    let mut ids = Vec::new();
    let mut failure = None;
    if let Ok((lines, _)) = JournalReader::read_from(dir.path(), 0) {
        for line in lines {
            match line.record {
                Record::SessionBound { session_id, .. } => {
                    if !ids.contains(&session_id) {
                        ids.push(session_id);
                    }
                }
                Record::CallFinished {
                    outcome: CallOutcome::Failed { reason },
                    ..
                } => failure = Some(reason),
                _ => {}
            }
        }
    }
    let listed = if ids.is_empty() || matches!(result, RunOutcome::Aborted { .. }) {
        None
    } else {
        Some(
            port.live_session_ids(&profile)
                .await
                .map_err(|_| (1, "session listing failed".into()))?,
        )
    };
    for id in ids {
        if matches!(result, RunOutcome::Aborted { .. }) {
            output(json!({"event":"detached","session_id":id}));
        }
        if let Some(info) = port.session_info(&id) {
            output(
                json!({"event":"session","session_id":id,"source":info.source,"model":info.model,"provider":info.provider}),
            );
        }
        if let Some(listed) = &listed {
            output(json!({"event":"listed_in_pickers","value":listed.contains(&id)}));
        }
    }
    let stats = port.stats();
    output(
        json!({"event":"stats","session.create":stats.session_create,"prompt.submit":stats.prompt_submit,"prompt.receipt":stats.prompt_receipt}),
    );
    if let Some(recording) = recorder {
        recording
            .finish()
            .map_err(|_| (1, "recording failed".into()))?;
    }
    match result {
        RunOutcome::Completed { result } => {
            if result.is_null() {
                output(
                    json!({"outcome":"failed","reason":failure.unwrap_or_else(||"agent returned no result".into())}),
                );
                Ok(1)
            } else {
                output(json!({"outcome":"completed","value":result}));
                Ok(0)
            }
        }
        RunOutcome::Failed { .. } => Err((1, "workflow failed".into())),
        RunOutcome::Aborted { .. } => Ok(3),
    }
}
#[tokio::main]
async fn main() -> std::process::ExitCode {
    if std::env::args().any(|arg| arg == "--help") {
        println!(
            "hermes_agent --profile NAME --model MODEL --provider PROVIDER --toolsets LIST [--effort LEVEL] [--skills LIST] [--max-turns N] [--cwd DIR] (--prompt TEXT | --script FILE) [--schema FILE] [--runs DIR] [--resume DIR] [--record FILE] [--scripted FILE]"
        );
        return std::process::ExitCode::SUCCESS;
    }
    let result = match options() {
        Ok(options) => run(options).await,
        Err(reason) => Err((64, reason)),
    };
    let code = match result {
        Ok(code) => code,
        Err((code, reason)) => {
            output(json!({"outcome":"failed","reason":reason}));
            code
        }
    };
    std::process::ExitCode::from(code)
}
