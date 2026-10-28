//! Run against a persistent offline authority, kill the runner, and resume it.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use talaria_workflow::{
    engine::*,
    journal::{fake_gateway::*, *},
};
#[derive(Clone)]
struct Resolver {
    profile: String,
}
impl LaunchResolver for Resolver {
    fn resolve(&self, r: &AgentRequest) -> Result<ResolvedLaunch, String> {
        Ok(ResolvedLaunch {
            profile: r.profile.clone().unwrap_or_else(|| self.profile.clone()),
            model: r
                .model
                .clone()
                .or_else(|| r.tier.clone())
                .unwrap_or_else(|| "fake".into()),
            provider: r.provider.clone(),
            effort: r.effort,
            toolsets: r.toolsets.clone().unwrap_or_default(),
            skills: r.skills.clone().unwrap_or_default(),
            max_turns: r.max_turns,
            isolation: r.isolation,
            cwd: None,
        })
    }
}
struct Policy;
impl TurnPolicy for Policy {
    fn first_turn(&self, r: &AgentRequest, _: &ResolvedLaunch) -> String {
        r.prompt.clone()
    }
    fn after_turn(&self, r: &AgentRequest, turns: &[SettledTurn]) -> NextStep {
        let Some(turn) = turns.last() else {
            return NextStep::Finish(AgentOutcome::Failed {
                reason: "no settled turn".into(),
            });
        };
        if turn.outcome != "completed" {
            return NextStep::Finish(AgentOutcome::Failed {
                reason: turn.outcome.clone(),
            });
        }
        let text = turn.final_text.clone().unwrap_or_default();
        if r.schema.is_some() {
            match serde_json::from_str::<Value>(&text) {
                Ok(value) => NextStep::Finish(AgentOutcome::Completed(value)),
                Err(_) if turns.len() == 1 => NextStep::Turn {
                    text: "Return the answer again as valid JSON.".into(),
                },
                Err(e) => NextStep::Finish(AgentOutcome::Failed {
                    reason: e.to_string(),
                }),
            }
        } else {
            NextStep::Finish(AgentOutcome::Completed(json!(text)))
        }
    }
}
fn output(value: Value) {
    println!("{value}");
}
fn arguments() -> Result<(String, BTreeMap<String, String>), String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or("missing command")?;
    let mut options = BTreeMap::new();
    while let Some(key) = args.next() {
        if !key.starts_with("--") {
            return Err(format!("expected option, found {key}"));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {key}"))?;
        if options.insert(key.clone(), value).is_some() {
            return Err(format!("duplicate option {key}"));
        }
    }
    Ok((command, options))
}
fn required<'a>(options: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    options
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing {key}"))
}
fn resolution(s: &str) -> Result<Resolution, String> {
    match s {
        "fail" => Ok(Resolution::Fail),
        "rerun" => Ok(Resolution::Rerun),
        "abort" => Ok(Resolution::Abort),
        _ => Err("--as must be fail, rerun or abort".into()),
    }
}
fn warning(w: JournalWarning) {
    match w {
        JournalWarning::TornTail { line, bytes } => {
            eprintln!("warning: dropped a torn last line (line {line}, {bytes} bytes)")
        }
        other => eprintln!("warning: {other:?}"),
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let code = match arguments() {
        Ok((command, options)) => match command.as_str() {
            "run" | "resume" => workflow(&command, &options).await,
            "stats" | "restart-gateway" | "resolve" => utility(&command, &options),
            _ => Err((64, "unknown command".into())),
        },
        Err(e) => Err((64, e)),
    };
    match code {
        Ok(code) => std::process::exit(code),
        Err((code, message)) => {
            eprintln!("{message}");
            std::process::exit(code);
        }
    }
}
fn journal_error(e: JournalError) -> (i32, String) {
    let code = if matches!(e, JournalError::Locked { .. }) {
        5
    } else {
        1
    };
    (code, e.to_string())
}
fn utility(command: &str, o: &BTreeMap<String, String>) -> Result<i32, (i32, String)> {
    let arg = |key| required(o, key).map_err(|e| (64, e));
    if command == "resolve" {
        let dir =
            RunDir::open(&Path::new(arg("--runs")?).join(arg("--run")?)).map_err(journal_error)?;
        let attempt = arg("--attempt")?
            .parse()
            .map_err(|_| (64, "invalid attempt".into()))?;
        let episode = o
            .get("--episode")
            .map(String::as_str)
            .unwrap_or("0")
            .parse()
            .map_err(|_| (64, "invalid episode".into()))?;
        dir.request_resolution(
            arg("--key")?,
            attempt,
            episode,
            resolution(arg("--as")?).map_err(|e| (64, e))?,
        )
        .map_err(journal_error)?;
    } else {
        let path = Path::new(arg("--gateway-state")?);
        let fake = FakeGateway::load_state(path).map_err(|e| (1, e.to_string()))?;
        if command == "restart-gateway" {
            fake.restart();
            fake.save_state(path).map_err(|e| (1, e.to_string()))?;
        } else {
            let s = fake.stats();
            output(
                json!({"sessions":s.sessions,"admissions":s.admissions,"receipts":s.receipts,"resolves":s.resolves}),
            );
        }
    }
    Ok(0)
}
async fn workflow(command: &str, o: &BTreeMap<String, String>) -> Result<i32, (i32, String)> {
    let arg = |key| required(o, key).map_err(|e| (64, e));
    let script = std::fs::read_to_string(arg("--script")?).map_err(|e| (1, e.to_string()))?;
    let parsed = parse_script(&script).map_err(|e| (1, e.to_string()))?;
    let runs = Path::new(arg("--runs")?);
    std::fs::create_dir_all(runs).map_err(|e| (1, e.to_string()))?;
    let dir = if command == "resume" {
        RunDir::open(&runs.join(arg("--run")?)).map_err(journal_error)?
    } else {
        let on_unknown = match o.get("--on-unknown").map(String::as_str).unwrap_or("wait") {
            "wait" => OnUnknown::Wait,
            "fail" => OnUnknown::Fail,
            _ => return Err((64, "--on-unknown must be wait or fail".into())),
        };
        let started_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| (1, e.to_string()))?
            .as_millis() as i64;
        RunDir::create(
            runs,
            NewManifest {
                workflow_name: parsed.meta.name.clone(),
                args: None,
                started_at_ms,
                seed: 42,
                profile: "fixture".into(),
                on_unknown,
                talaria_version: env!("CARGO_PKG_VERSION").into(),
                hermes: json!({}),
                roster: json!({}),
                approval_policy: json!({}),
            },
            &script,
        )
        .map_err(journal_error)?
    };
    let state_path = PathBuf::from(arg("--gateway-state")?);
    let mut fake = if state_path.exists() {
        FakeGateway::load_state(&state_path).map_err(|e| (1, e.to_string()))?
    } else {
        FakeGateway::new()
    };
    if let Some(label) = o.get("--hold") {
        fake = fake.with_script(vec![ScriptRule {
            label: Some(label.clone()),
            hold: Some(label.clone()),
            ..Default::default()
        }]);
    }
    let fake = fake.autosave(state_path);
    let (journal, summary, warnings) = RunJournal::open(dir.clone(), &script)
        .await
        .map_err(journal_error)?;
    for w in warnings {
        warning(w);
    }
    let journal = Arc::new(journal);
    output(
        json!({"event":"run","run_id":dir.run_id().as_str(),"resume_count":summary.resume_count}),
    );
    let resolver = Resolver {
        profile: dir.manifest().profile.clone(),
    };
    let config = journal.engine_config(&resolver).map_err(journal_error)?;
    let backend = JournaledBackend::new(
        journal.clone(),
        fake,
        Policy,
        resolver.clone(),
        NoGate,
        RetryPolicy::default(),
    );
    let mut notices = journal.notices();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();
    let forward_j = journal.clone();
    let forward_dir = dir.clone();
    let forward = tokio::spawn(async move {
        let mut calls = BTreeMap::new();
        while let Some(e) = rx.recv().await {
            if let EngineEvent::AgentStarted { request, replayed } = &e {
                let launch = resolver.resolve(request).map_err(JournalError::Manifest)?;
                let key = journal_key(&request.call_id, &launch);
                let records = JournalReader::read_from(forward_dir.path(), 0)?.0;
                let latest = records
                    .iter()
                    .rev()
                    .find(|l| l.record.identity().is_some_and(|(k, _)| k == key));
                let attempt = latest
                    .and_then(|l| l.record.identity())
                    .map_or(0, |(_, a)| a);
                let source = if *replayed {
                    "cache"
                } else {
                    match latest.map(|l| &l.record) {
                        Some(Record::CallFinished {
                            outcome: CallOutcome::Completed { .. } | CallOutcome::Skipped,
                            ..
                        }) => "cache",
                        Some(Record::CallIntent { .. } | Record::TurnIntent { .. }) => "retry",
                        Some(_) => "reattach",
                        None => "fresh",
                    }
                };
                output(
                    json!({"event":"call","label":request.label,"key":key,"attempt":attempt,"source":source}),
                );
                calls.insert(request.call_id.clone(), (request.label.clone(), key));
            }
            forward_j.record_engine_event(&e).await?;
            if let EngineEvent::AgentFinished {
                call_id,
                outcome,
                completion_seq: Some(_),
            } = e
                && let Some((label, key)) = calls.get(&call_id)
            {
                let kind = match outcome {
                    OutcomeKind::Completed => "completed",
                    OutcomeKind::Skipped => "skipped",
                    _ => "failed",
                };
                output(json!({"event":"call_finished","label":label,"key":key,"outcome":kind}));
            }
        }
        Ok::<_, JournalError>(())
    });
    let running = run(
        parsed,
        config,
        Arc::new(backend),
        Arc::new(MapResolver::new()),
        tx,
        tokio_util::sync::CancellationToken::new(),
    );
    tokio::pin!(running);
    let outcome = loop {
        tokio::select! {
         o=&mut running=>break o,
         notice=notices.recv()=>match notice{
          Ok(JournalNotice::NeedsOperator{key,label,attempt,episode,cause,..})=>{output(json!({"event":"needs_operator","key":key,"label":label,"attempt":attempt,"episode":episode,"cause":cause}));eprintln!("waiting for an operator resolution file");},
          Ok(JournalNotice::Warning(w))=>warning(w),_=>{},
         },
         signal=tokio::signal::ctrl_c()=>{signal.map_err(|e|(1,e.to_string()))?;forward.abort();return Ok(if journal.summary().parked.is_empty(){3}else{4});},
        }
    };
    forward
        .await
        .map_err(|e| (1, e.to_string()))?
        .map_err(journal_error)?;
    let (kind, code) = match outcome {
        RunOutcome::Completed { .. } => (RunOutcomeKind::Completed, 0),
        RunOutcome::Failed { error } => {
            eprintln!("{}: {}", error.name, error.message);
            (RunOutcomeKind::Failed, 1)
        }
        RunOutcome::Aborted { reason } => {
            eprintln!("{reason}");
            (RunOutcomeKind::Aborted, 3)
        }
    };
    journal.finish(kind.clone()).await.map_err(journal_error)?;
    output(json!({"event":"finished","outcome":kind}));
    Ok(code)
}
