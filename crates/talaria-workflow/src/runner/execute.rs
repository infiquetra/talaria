use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, mpsc};

use crate::{
    engine::{
        self, CallId, CutShort, EngineEvent, OutcomeKind, ParsedScript, RunOutcome, RunOutcomeKind,
    },
    hermes::{
        AgentFeedItem, AgentFeedReceiver, ExplicitLaunch, HermesPort, LaunchDefaults,
        SchemaTurnPolicy, StopResult,
    },
    journal::{
        AdmissionGate, AdmissionState, GatewayPort, JournalNotice, JournaledBackend, NewManifest,
        OnUnknown, RetryPolicy, RunDir, RunJournal,
    },
    progress::RunControl,
    schedule::{MoneySource, Scheduler, SchedulerConfig, Spend},
    staffing::{
        PriceBook, Roster, RosterError, RunBudget, StaffedLaunch, StaffingResolver, StaffingTable,
        check_script_routes, check_tiers, estimate_run_cost, script_routes, staffing_drift,
    },
    tokens::Reading,
    worktree::GitWorktrees,
};

pub use super::services::RunnerServices;
use super::{
    control::{self, PausingGate},
    files::{self, AgentNote, FailureView, LaunchFile, OutcomeFile, RunnerFile},
    resolver::FilesystemResolver,
    runs::{self, Fold, LockProbe},
    services,
};

pub struct RunRequest {
    pub script: PathBuf,
    pub args: Value,
    pub profile: String,
    /// The model and provider the calling session runs on, as `(model, provider)`; it stands in
    /// for the profile's configured model wherever a call names no model or tier.
    pub session_model: Option<(String, String)>,
    pub staffing: Option<PathBuf>,
    pub cwd: PathBuf,
    pub budget_tokens: Option<u64>,
    pub budget_usd: Option<f64>,
    pub on_unknown: OnUnknown,
    pub runs_dir: PathBuf,
    pub workflows_dir: PathBuf,
    pub talaria_version: String,
    pub json: bool,
    pub record: Option<PathBuf>,
    pub settle_bound: Duration,
    pub receipt_poll: Duration,
}
pub struct ResumeRequest {
    pub target: String,
    pub script: Option<PathBuf>,
    pub staffing: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub budget_tokens: Option<u64>,
    pub budget_usd: Option<f64>,
    pub record: Option<PathBuf>,
    pub settle_bound: Duration,
    pub receipt_poll: Duration,
    pub runs_dir: PathBuf,
    pub workflows_dir: PathBuf,
    pub json: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpendView {
    /// Tokens the budget counts: everything except cache reads. A turn whose cache reads Hermes
    /// did not report is counted whole here.
    pub tokens: u64,
    /// Cache reads Hermes reported. They are not in `tokens` and not in the budget.
    #[serde(default)]
    pub cache_read_tokens: u64,
    /// Turns whose cache reads Hermes did not report; their tokens are all in `tokens`.
    #[serde(default)]
    pub cache_unknown_turns: u32,
    pub money_usd: Option<f64>,
    pub money_source: String,
}
impl SpendView {
    fn from_spend(spend: Spend) -> Self {
        let money_source = serde_json::to_value(&spend.money_source)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "none".into());
        Self {
            tokens: spend.tokens,
            cache_read_tokens: spend.cache_read_tokens,
            cache_unknown_turns: spend.cache_unknown_turns,
            money_usd: if matches!(spend.money_source, MoneySource::None) {
                None
            } else {
                Some(spend.money_usd)
            },
            money_source,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum RunnerEvent {
    Diagnostic {
        text: String,
    },
    RunStarted {
        run_id: String,
        run_dir: PathBuf,
    },
    Phase {
        title: String,
    },
    Log {
        message: String,
    },
    AgentStarted {
        label: String,
        model: String,
        provider: String,
    },
    AgentFinished {
        label: String,
        state: String,
    },
    Scheduler(Value),
    Parked {
        run_id: String,
        label: String,
        key: String,
        attempt: u32,
        episode: u32,
        /// Why the call is parked: `unknown`, `lost`, `identity_conflict` or `stopped`.
        cause: String,
        /// The reason in plain words.
        reason: String,
        /// The answers `talaria workflow resolve --resolution` accepts for it.
        choices: Vec<String>,
    },
    Finished {
        run_id: String,
        outcome: String,
        result: Option<Value>,
        spend: SpendView,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunExit {
    pub code: i32,
    pub run_id: Option<String>,
    pub problems: Vec<String>,
}

struct Output<'a> {
    out: &'a mut dyn FnMut(&RunnerEvent),
    problems: Vec<String>,
    run_id: Option<String>,
}
impl Output<'_> {
    fn diagnostic(&mut self, text: impl Into<String>) {
        let text = text.into();
        (self.out)(&RunnerEvent::Diagnostic { text: text.clone() });
        self.problems.push(text);
    }
    fn exit(&self, code: i32) -> RunExit {
        RunExit {
            code,
            run_id: self.run_id.clone(),
            problems: self.problems.clone(),
        }
    }
    fn started(&mut self, dir: &RunDir) {
        self.run_id = Some(dir.run_id().as_str().into());
        (self.out)(&RunnerEvent::RunStarted {
            run_id: dir.run_id().as_str().into(),
            run_dir: dir.path().into(),
        });
    }
}

pub struct PreparedRun {
    pub dir: RunDir,
    parsed: ParsedScript,
    source: String,
    launch: LaunchFile,
    services: RunnerServices,
    preflight: Preflight,
    restart_keys: Vec<String>,
}
struct Preflight {
    table: StaffingTable,
    budget: RunBudget,
    roster: Arc<Roster>,
    snapshot: Value,
}
fn staffing(path: Option<&Path>, output: &mut Output<'_>) -> Result<StaffingTable, i32> {
    let path = path.map(Path::to_owned).unwrap_or_else(|| {
        StaffingTable::default_path(
            &crate::staffing::talaria_home().unwrap_or_else(runs::configuration_dir),
        )
    });
    StaffingTable::load(&path).map_err(|e| {
        output.diagnostic(e.to_string());
        2
    })
}
async fn preflight(
    table: StaffingTable,
    tokens: Option<u64>,
    usd: Option<f64>,
    profile: &str,
    session_model: Option<&(String, String)>,
    services: &RunnerServices,
    output: &mut Output<'_>,
) -> Result<Preflight, i32> {
    let budget = RunBudget::resolve(tokens, usd, &table);
    let roster = services.rosters.load(profile).await.map_err(|e| {
        output.diagnostic(match e {
            RosterError::NotServed(_) => format!("gateway not found: {e}"),
            _ => e.to_string(),
        });
        1
    })?;
    let roster = session_roster(roster, session_model);
    let check = check_tiers(&table, &roster, &budget, services.clock.now());
    for warning in check.warnings {
        output.diagnostic(warning);
    }
    let refused = !check.problems.is_empty();
    for problem in check.problems {
        output.diagnostic(problem);
    }
    if refused {
        return Err(2);
    }
    output.diagnostic(estimate_run_cost(&table, &roster, budget.tokens).to_string());
    warn_harmful_settings(services, profile, output).await;
    let snapshot = roster.snapshot_with(&table, &budget);
    Ok(Preflight {
        table,
        budget,
        roster,
        snapshot,
    })
}
/// Warns, before any agent starts, about a Hermes setting that makes workflow agents fail late.
/// It is a warning only: a configuration that cannot be read, or reads fine, adds nothing, and
/// the run starts either way.
async fn warn_harmful_settings(services: &RunnerServices, profile: &str, output: &mut Output<'_>) {
    let Ok(effective) = services.config.effective(profile).await else {
        return;
    };
    for finding in talaria_core::config_check::findings(&effective) {
        if let Some(warning) = finding.workflow_warning(profile) {
            output.diagnostic(warning);
        }
    }
}
/// The roster with the session's model as its current pair, when the run has one.
fn session_roster(roster: Arc<Roster>, session_model: Option<&(String, String)>) -> Arc<Roster> {
    match session_model {
        Some((model, provider)) => {
            Arc::new(Roster::clone(&roster).with_session_model(model, provider))
        }
        None => roster,
    }
}
/// The token and money budget the run started with, as its manifest's roster snapshot records
/// them; a resume keeps them unless its own flags replace them.
fn stored_budget(snapshot: &Value) -> (Option<u64>, Option<f64>) {
    (
        snapshot["budget"]["tokens"].as_u64(),
        snapshot["budget"]["money_usd"].as_f64(),
    )
}
fn read_script(path: &Path, output: &mut Output<'_>) -> Result<(String, ParsedScript), i32> {
    let source = fs::read_to_string(path).map_err(|e| {
        output.diagnostic(format!("cannot read workflow script: {e}"));
        2
    })?;
    let parsed = engine::parse_script(&source).map_err(|e| {
        output.diagnostic(e.to_string());
        2
    })?;
    Ok((source, parsed))
}
fn seed(started_at_ms: i64, source: &str) -> u64 {
    let hash = hex::encode(Sha256::digest(source.as_bytes()));
    let bytes = Sha256::digest(format!("{started_at_ms}{hash}").as_bytes());
    let mut first = [0u8; 8];
    first.copy_from_slice(&bytes[..8]);
    u64::from_be_bytes(first)
}
async fn prepare(
    req: &RunRequest,
    args: Option<Value>,
    services: RunnerServices,
    output: &mut Output<'_>,
) -> Result<PreparedRun, i32> {
    let (source, parsed) = read_script(&req.script, output)?;
    let table = staffing(req.staffing.as_deref(), output)?;
    let preflight = preflight(
        table,
        req.budget_tokens,
        req.budget_usd,
        &req.profile,
        req.session_model.as_ref(),
        &services,
        output,
    )
    .await?;
    // A misspelt model would otherwise fail only when its agent starts, after earlier agents
    // have spent.
    let unknown = check_script_routes(&preflight.roster, &script_routes(&parsed.body));
    for problem in &unknown {
        output.diagnostic(problem);
    }
    if !unknown.is_empty() {
        return Err(2);
    }
    let wire = services.wires.open(&req.profile).await.map_err(|e| {
        output.diagnostic(e.to_string());
        1
    })?;
    let describe = wire
        .call(
            "runtime.describe",
            serde_json::Map::new(),
            Duration::from_secs(10),
        )
        .await
        .map_err(|e| {
            output.diagnostic(e.to_string());
            1
        })?;
    let script_path = files::absolute(&req.script).map_err(|e| {
        output.diagnostic(e.to_string());
        2
    })?;
    let cwd = fs::canonicalize(&req.cwd).map_err(|e| {
        output.diagnostic(e.to_string());
        2
    })?;
    let workflows_dir = files::absolute(&req.workflows_dir).map_err(|e| {
        output.diagnostic(e.to_string());
        2
    })?;
    let launch = LaunchFile {
        v: 1,
        script_dir: script_path.parent().unwrap_or(Path::new(".")).into(),
        script_path,
        cwd,
        workflows_dir,
    };
    let started_at_ms = files::now_ms();
    let manifest = NewManifest {
        workflow_name: parsed.meta.name.clone(),
        args,
        started_at_ms,
        seed: seed(started_at_ms, &source),
        profile: req.profile.clone(),
        on_unknown: req.on_unknown,
        talaria_version: req.talaria_version.clone(),
        hermes: json!({"describe":describe}),
        roster: preflight.snapshot.clone(),
        approval_policy: json!({"mode":"finite-only","source":"operator decision on #218 (`PH5`)"}),
    };
    files::private_dir(&req.runs_dir).map_err(|e| {
        output.diagnostic(e.to_string());
        1
    })?;
    let dir = RunDir::create(&req.runs_dir, manifest, &source).map_err(|e| {
        output.diagnostic(e.to_string());
        1
    })?;
    files::write_json(&dir.path().join("launch.json"), &launch).map_err(|e| {
        output.diagnostic(e.to_string());
        1
    })?;
    Ok(PreparedRun {
        dir,
        parsed,
        source,
        launch,
        services,
        preflight,
        restart_keys: Vec::new(),
    })
}

pub async fn prepare_new(
    req: &RunRequest,
    out: &mut dyn FnMut(&RunnerEvent),
) -> Result<PreparedRun, RunExit> {
    prepare_new_with_args(req, Some(req.args.clone()), out).await
}
pub async fn prepare_new_with_args(
    req: &RunRequest,
    args: Option<Value>,
    out: &mut dyn FnMut(&RunnerEvent),
) -> Result<PreparedRun, RunExit> {
    let mut output = Output {
        out,
        problems: Vec::new(),
        run_id: None,
    };
    if let Err(code) = read_script(&req.script, &mut output) {
        return Err(output.exit(code));
    }
    let services = services::production(req.record.as_deref(), &req.profile).map_err(|e| {
        output.diagnostic(e);
        output.exit(1)
    })?;
    prepare(req, args, services, &mut output)
        .await
        .map_err(|code| output.exit(code))
}
pub async fn run_new(req: RunRequest, out: &mut dyn FnMut(&RunnerEvent)) -> RunExit {
    let args = Some(req.args.clone());
    run_new_with_args(req, args, out).await
}
pub async fn run_new_with_args(
    req: RunRequest,
    args: Option<Value>,
    out: &mut dyn FnMut(&RunnerEvent),
) -> RunExit {
    let mut output = Output {
        out,
        problems: Vec::new(),
        run_id: None,
    };
    if let Err(code) = read_script(&req.script, &mut output) {
        return output.exit(code);
    }
    let services = match services::production(req.record.as_deref(), &req.profile) {
        Ok(services) => services,
        Err(e) => {
            output.diagnostic(e);
            return output.exit(1);
        }
    };
    run_new_with(req, args, services, output).await
}
pub async fn run_new_with_services(
    req: RunRequest,
    services: RunnerServices,
    out: &mut dyn FnMut(&RunnerEvent),
) -> RunExit {
    let args = Some(req.args.clone());
    run_new_with(
        req,
        args,
        services,
        Output {
            out,
            problems: Vec::new(),
            run_id: None,
        },
    )
    .await
}
async fn run_new_with(
    req: RunRequest,
    args: Option<Value>,
    services: RunnerServices,
    mut output: Output<'_>,
) -> RunExit {
    match prepare(&req, args, services, &mut output).await {
        Ok(prepared) => {
            execute(
                prepared,
                req.settle_bound,
                req.receipt_poll,
                false,
                &mut output,
            )
            .await
        }
        Err(code) => output.exit(code),
    }
}

pub async fn run_resume(req: ResumeRequest, out: &mut dyn FnMut(&RunnerEvent)) -> RunExit {
    resume(req, None, out).await
}
pub async fn run_resume_with_services(
    req: ResumeRequest,
    services: RunnerServices,
    out: &mut dyn FnMut(&RunnerEvent),
) -> RunExit {
    resume(req, Some(services), out).await
}
async fn resume(
    req: ResumeRequest,
    injected: Option<RunnerServices>,
    out: &mut dyn FnMut(&RunnerEvent),
) -> RunExit {
    let mut output = Output {
        out,
        problems: Vec::new(),
        run_id: None,
    };
    let mut replayed_code = 0;
    let result = async {
        let path = runs::find_run(&req.runs_dir, &req.target).map_err(|e| {
            output.diagnostic(e.to_string());
            2
        })?;
        let dir = RunDir::open(&path).map_err(|e| {
            output.diagnostic(e.to_string());
            2
        })?;
        output.run_id = Some(dir.run_id().as_str().into());
        let mut probe = runs::probe_lock(&path).map_err(|e| {
            output.diagnostic(format!("lock probe failed: {e}"));
            1
        })?;
        // A cancelled worker releases its engine thread and journal asynchronously.
        for _ in 0..5 {
            if !matches!(probe, LockProbe::Held) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            probe = runs::probe_lock(&path).map_err(|e| {
                output.diagnostic(format!("lock probe failed: {e}"));
                1
            })?;
        }
        let lease = match probe {
            LockProbe::Held => {
                output.diagnostic(format!(
                    "run {} is already open in another runner",
                    dir.run_id().as_str()
                ));
                return Err(1);
            }
            LockProbe::Free(file) => file,
        };
        let records = control::read_records(&path).map_err(|e| {
            output.diagnostic(e.to_string());
            2
        })?;
        let fold = Fold::records(&records).map_err(|e| {
            output.diagnostic(e.to_string());
            2
        })?;
        let pending_restarts = control::restart_keys(&dir);
        let restarting =
            control::restart_needs_execution(&records, &pending_restarts).map_err(|e| {
                output.diagnostic(e);
                2
            })?;
        // A run its budget ended replays like a completed one unless a new budget is given:
        // with its own budget kept, a new execution could only be refused again.
        let replay = match fold.terminal.as_deref() {
            Some("completed") => Some(("completed", 0)),
            Some("completed_with_failures") => Some(("completed_with_failures", 5)),
            Some("budget_exhausted") if req.budget_tokens.is_none() && req.budget_usd.is_none() => {
                Some(("budget_exhausted", 4))
            }
            _ => None,
        };
        if let Some((outcome, code)) = replay.filter(|_| !restarting) {
            if let Some(path) = &req.record {
                super::recording::open(path, false)
                    .and_then(|recording| recording.finish().map(|_| ()).map_err(|e| e.to_string()))
                    .map_err(|e| {
                        output.diagnostic(e);
                        1
                    })?;
            }
            files::consume_restarts(
                dir.path(),
                &pending_restarts.iter().cloned().collect::<Vec<_>>(),
            )
            .map_err(|e| {
                output.diagnostic(e.to_string());
                1
            })?;
            output.started(&dir);
            let result = match files::read_json::<OutcomeFile>(&path.join("outcome.json")) {
                Ok(saved) if saved.v == 1 && saved.outcome == outcome => saved.result,
                _ => {
                    (output.out)(&RunnerEvent::Log {
                        message: format!(
                            "stored result unavailable; {outcome} summary {}",
                            fold.summary.unwrap_or(Value::Null)
                        ),
                    });
                    None
                }
            };
            if outcome == "budget_exhausted" {
                output.diagnostic(
                    "this run's budget ran out; to continue it, resume with a larger --budget-tokens or --budget-usd",
                );
            }
            replayed_code = code;
            (output.out)(&RunnerEvent::Finished {
                run_id: dir.run_id().as_str().into(),
                outcome: outcome.into(),
                result,
                spend: SpendView {
                    tokens: fold.tokens.unwrap_or_default(),
                    cache_read_tokens: fold.cache_read_tokens,
                    cache_unknown_turns: fold.cache_unknown_turns,
                    money_usd: fold.money,
                    money_source: if fold.money.is_some() {
                        "hermes"
                    } else {
                        "none"
                    }
                    .into(),
                },
            });
            drop(lease);
            return Ok(None);
        }
        let (source, parsed) = if let Some(script) = &req.script {
            let result = read_script(script, &mut output)?;
            dir.store_script(&result.0).map_err(|e| {
                output.diagnostic(e.to_string());
                2
            })?;
            result
        } else {
            let hash = fold
                .latest_script
                .unwrap_or_else(|| dir.manifest().workflow.script_sha256.clone());
            let path = path.join("scripts").join(format!("{hash}.js"));
            if !path.is_file() {
                output.diagnostic(format!(
                    "stored script for run {} is missing: scripts/{hash}.js",
                    dir.run_id().as_str()
                ));
                return Err(2);
            }
            read_script(&path, &mut output)?
        };
        let mut launch: LaunchFile =
            files::read_json(&dir.path().join("launch.json")).map_err(|e| {
                output.diagnostic(format!("cannot read launch.json: {e}"));
                2
            })?;
        if let Some(cwd) = &req.cwd {
            launch.cwd = files::absolute(cwd).map_err(|e| {
                output.diagnostic(e.to_string());
                2
            })?;
        }
        launch.workflows_dir = files::absolute(&req.workflows_dir).map_err(|e| {
            output.diagnostic(e.to_string());
            2
        })?;
        let table = staffing(req.staffing.as_deref(), &mut output)?;
        let services = match injected {
            Some(services) => services,
            None => services::production(req.record.as_deref(), &dir.manifest().profile).map_err(
                |e| {
                    output.diagnostic(e);
                    1
                },
            )?,
        };
        let (stored_tokens, stored_usd) = stored_budget(&dir.manifest().roster);
        let preflight = preflight(
            table,
            req.budget_tokens.or(stored_tokens),
            req.budget_usd.or(stored_usd),
            &dir.manifest().profile,
            Roster::session_model_in(&dir.manifest().roster).as_ref(),
            &services,
            &mut output,
        )
        .await?;
        for drift in staffing_drift(&dir.manifest().roster, &preflight.table) {
            output.diagnostic(format!(
                "{}: {} -> {}",
                drift.tier, drift.before, drift.after
            ));
        }
        files::clear_transient_controls(dir.path()).map_err(|e| {
            output.diagnostic(e.to_string());
            1
        })?;
        control::remove_stale_runner(dir.path()).map_err(|e| {
            output.diagnostic(e.to_string());
            1
        })?;
        drop(lease);
        Ok(Some(PreparedRun {
            dir,
            parsed,
            source,
            launch,
            services,
            preflight,
            restart_keys: pending_restarts.into_iter().collect(),
        }))
    }
    .await;
    match result {
        Ok(Some(prepared)) => {
            execute(
                prepared,
                req.settle_bound,
                req.receipt_poll,
                false,
                &mut output,
            )
            .await
        }
        Ok(None) => output.exit(replayed_code),
        Err(code) => output.exit(code),
    }
}

pub async fn run_child(run_dir: PathBuf, out: &mut dyn FnMut(&RunnerEvent)) -> RunExit {
    run_child_with_options(run_dir, ChildOptions::default(), out).await
}
#[derive(Default)]
pub struct ChildOptions {
    pub staffing: Option<PathBuf>,
    pub budget_tokens: Option<u64>,
    pub budget_usd: Option<f64>,
    pub workflows_dir: Option<PathBuf>,
    pub record: Option<PathBuf>,
    pub record_append: bool,
}
pub async fn run_child_with_options(
    run_dir: PathBuf,
    options: ChildOptions,
    out: &mut dyn FnMut(&RunnerEvent),
) -> RunExit {
    let mut output = Output {
        out,
        problems: Vec::new(),
        run_id: None,
    };
    let result = async {
        let dir = RunDir::open(&run_dir).map_err(|e| {
            output.diagnostic(e.to_string());
            1
        })?;
        output.run_id = Some(dir.run_id().as_str().into());
        let mut launch =
            files::read_json::<LaunchFile>(&dir.path().join("launch.json")).map_err(|e| {
                output.diagnostic(e.to_string());
                1
            })?;
        if let Some(workflows_dir) = options.workflows_dir {
            launch.workflows_dir = files::absolute(&workflows_dir).map_err(|e| {
                output.diagnostic(e.to_string());
                2
            })?;
        }
        let (source, parsed) = read_script(
            &dir.path()
                .join("scripts")
                .join(format!("{}.js", dir.manifest().workflow.script_sha256)),
            &mut output,
        )?;
        let services = services::production_recording(
            options.record.as_deref(),
            &dir.manifest().profile,
            options.record_append,
        )
        .map_err(|e| {
            output.diagnostic(e);
            1
        })?;
        let table = staffing(options.staffing.as_deref(), &mut output)?;
        let budget = RunBudget::resolve(options.budget_tokens, options.budget_usd, &table);
        let roster = services
            .rosters
            .load(&dir.manifest().profile)
            .await
            .map_err(|e| {
                output.diagnostic(e.to_string());
                1
            })?;
        let snapshot = dir.manifest().roster.clone();
        let roster = session_roster(roster, Roster::session_model_in(&snapshot).as_ref());
        Ok(PreparedRun {
            dir,
            parsed,
            source,
            launch,
            services,
            preflight: Preflight {
                table,
                budget,
                roster,
                snapshot,
            },
            restart_keys: Vec::new(),
        })
    }
    .await;
    match result {
        Ok(prepared) => {
            execute(
                prepared,
                Duration::from_secs(30),
                Duration::from_millis(250),
                true,
                &mut output,
            )
            .await
        }
        Err(code) => output.exit(code),
    }
}

pub fn spawn_detached(
    prepared: PreparedRun,
    executable: &Path,
    extra_args: &[String],
) -> std::io::Result<u32> {
    use std::process::{Command, Stdio};
    prepared
        .services
        .finish_recording()
        .map_err(std::io::Error::other)?;
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(prepared.dir.path().join("runner.log"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        log.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    let mut command = Command::new(executable);
    command
        .args(["workflow", "run", "--foreground-child", "--run-dir"])
        .arg(prepared.dir.path())
        .args(extra_args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x00000008 | 0x00000200);
    }
    let mut child = command.spawn()?;
    let pid = child.id();
    files::write_json(
        &prepared.dir.path().join("runner.json"),
        &RunnerFile {
            pid,
            started_at_ms: files::now_ms(),
        },
    )?;
    if child.try_wait()?.is_some()
        && files::read_json::<RunnerFile>(&prepared.dir.path().join("runner.json"))
            .is_ok_and(|owner| owner.pid == pid)
    {
        files::remove(&prepared.dir.path().join("runner.json"))?;
    }
    Ok(pid)
}

enum Input {
    Engine(Box<EngineEvent>),
    Outcome(RunOutcome),
    Scheduler(Value),
    Notice(JournalNotice),
    Poll,
    Signal,
    Failure(String),
}
struct Tasks {
    handles: Vec<tokio::task::JoinHandle<()>>,
    cancel: tokio_util::sync::CancellationToken,
    forward: tokio::task::AbortHandle,
}
impl Drop for Tasks {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.forward.abort();
        for task in &self.handles {
            task.abort();
        }
    }
}

async fn execute(
    prepared: PreparedRun,
    settle_bound: Duration,
    receipt_poll: Duration,
    child: bool,
    output: &mut Output<'_>,
) -> RunExit {
    let result = execute_inner(prepared, settle_bound, receipt_poll, child, output).await;
    match result {
        Ok(code) => output.exit(code),
        Err(error) => {
            output.diagnostic(error);
            output.exit(1)
        }
    }
}
async fn execute_inner(
    prepared: PreparedRun,
    settle_bound: Duration,
    receipt_poll: Duration,
    child: bool,
    output: &mut Output<'_>,
) -> Result<i32, String> {
    let PreparedRun {
        dir,
        parsed,
        source,
        launch,
        services,
        preflight,
        restart_keys,
    } = prepared;
    let (journal, _, warnings) = RunJournal::open(dir, &source)
        .await
        .map_err(|e| e.to_string())?;
    let journal = Arc::new(journal);
    let dir = journal.dir().clone();
    if child {
        files::remove_other_runner(dir.path()).map_err(|e| e.to_string())?;
    }
    for warning in warnings {
        output.diagnostic(format!("journal warning: {warning:?}"));
    }
    output.started(&dir);
    let (port, feed) = services.port(&dir.manifest().profile).await?;
    let records = control::read_records(dir.path()).map_err(|e| e.to_string())?;
    for target in control::stop_targets(&records).map_err(|e| e.to_string())? {
        // The durable backend handles a failed attachment through its recovery policy.
        let _ = port.attach(target.profile, target.session_id).await;
    }
    if !restart_keys.is_empty() {
        verify_restarts(&port, dir.path(), &restart_keys, settle_bound).await?;
        journal
            .request_restart_keys(&restart_keys)
            .await
            .map_err(|e| e.to_string())?;
        files::consume_restarts(dir.path(), &restart_keys).map_err(|e| e.to_string())?;
    }
    let prices = PriceBook::from_snapshot(&dir.manifest().roster);
    let (scheduler, mut scheduler_events) = Scheduler::start(
        SchedulerConfig::from_table(
            &preflight.table,
            preflight.budget,
            &dir.manifest().profile,
            prices,
        ),
        services.rosters.clone(),
        dir.path().join("journal.jsonl"),
        services.clock.clone(),
    )
    .await
    .map_err(|e| e.to_string())?;
    let gate = PausingGate::new(scheduler.clone());
    gate.set_paused(RunControl::pending(&dir).pause.is_some());
    let original_launch: LaunchFile = files::read_json(&dir.path().join("launch.json"))
        .map_err(|e| format!("cannot read launch.json: {e}"))?;
    let worktrees = GitWorktrees::new(
        dir.path().to_path_buf(),
        original_launch.cwd,
        dir.run_id().as_str().into(),
    );
    let defaults = LaunchDefaults {
        profile: dir.manifest().profile.clone(),
        cwd: launch.cwd.clone(),
        toolsets: Some(Vec::new()),
        worktrees: Some(Arc::new(worktrees)),
    };
    let for_config = StaffedLaunch::new(
        StaffingResolver::new(Arc::new(preflight.table.clone()), preflight.roster.clone()),
        ExplicitLaunch::new(defaults.clone()),
    );
    let config = journal
        .engine_config(&for_config)
        .map_err(|e| e.to_string())?;
    let launch_resolver = StaffedLaunch::new(
        StaffingResolver::new(Arc::new(preflight.table), preflight.roster),
        ExplicitLaunch::new(defaults),
    );
    let backend = Arc::new(JournaledBackend::new(
        journal.clone(),
        port.clone(),
        SchemaTurnPolicy,
        launch_resolver,
        gate.clone(),
        RetryPolicy::default(),
    ));
    let (durable_tx, durable_rx) = mpsc::unbounded_channel();
    let forward = journal.forward_events(durable_rx);
    let mut notices = journal.notices();
    let mut engine = engine::spawn(
        parsed,
        config,
        backend.clone(),
        Arc::new(
            FilesystemResolver::new(launch.workflows_dir, launch.script_dir).with_cwd(launch.cwd),
        ),
    );
    let cancel = engine.cancel.clone();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut tasks = Tasks {
        handles: Vec::new(),
        cancel: cancel.clone(),
        forward: forward.abort_handle(),
    };
    let input = tx.clone();
    tasks.handles.push(tokio::spawn(async move {
        while let Some(event) = engine.events.recv().await {
            if input.send(Input::Engine(Box::new(event))).is_err() {
                return;
            }
        }
        let message = match engine.outcome.await {
            Ok(outcome) => Input::Outcome(outcome),
            Err(e) => Input::Failure(e.to_string()),
        };
        let _ = input.send(message);
    }));
    let input = tx.clone();
    tasks.handles.push(tokio::spawn(async move {
        while let Some(event) = scheduler_events.0.recv().await {
            match serde_json::to_value(event) {
                Ok(event) => {
                    if input.send(Input::Scheduler(event)).is_err() {
                        return;
                    }
                }
                Err(e) => {
                    let _ = input.send(Input::Failure(e.to_string()));
                    return;
                }
            }
        }
    }));
    let input = tx.clone();
    tasks.handles.push(tokio::spawn(async move {
        loop {
            match notices.recv().await {
                Ok(notice) => {
                    if input.send(Input::Notice(notice)).is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            }
        }
    }));
    let input = tx.clone();
    tasks.handles.push(tokio::spawn(async move {
        let mut ticks = tokio::time::interval(Duration::from_millis(250));
        loop {
            ticks.tick().await;
            if input.send(Input::Poll).is_err() {
                return;
            }
        }
    }));
    tasks.handles.push(signal_task(tx.clone()));
    tasks
        .handles
        .push(tokio::spawn(count_live_usage(feed, scheduler.clone())));
    drop(tx);
    let mut labels = BTreeMap::<CallId, String>::new();
    let mut engine_outcome = None;
    let mut stopping: Option<futures::future::BoxFuture<'static, Vec<String>>> = None;
    let mut stopped = None;
    let mut stop_requested = false;
    // The gate starts open; only a change is journaled, so script logs stay in view.
    let mut journaled_paused = false;
    // Calls the budget refused or stopped, the latest reason, and turns whose stop is in
    // flight or was sent.
    let (mut refused, mut budget_stopped) = (0_usize, 0_usize);
    let mut budget_reason: Option<String> = None;
    // Agents that failed or were cut short, for the final result.
    let (mut failed_agents, mut cut_short_agents) =
        (Vec::<AgentNote>::new(), Vec::<AgentNote>::new());
    let stop_sent = Arc::new(std::sync::Mutex::new(BTreeSet::<String>::new()));
    let outcome = loop {
        tokio::select! {
            unsettled=async {match stopping.as_mut(){Some(f)=>f.await,None=>std::future::pending().await}},if stopping.is_some()=>{stopped=Some(unsettled);stopping=None;},
            message=rx.recv()=>match message {
                Some(Input::Engine(event))=>{
                    let event=*event;
                    durable_tx.send(event.clone()).map_err(|_|"journal event forwarder closed".to_owned())?;
                    match event {
                        EngineEvent::Phase{title,..}=>(output.out)(&RunnerEvent::Phase{title}),
                        EngineEvent::Log{message,..}=>(output.out)(&RunnerEvent::Log{message}),
                        EngineEvent::AgentStarted{request,..}=>{
                            labels.insert(request.call_id,request.label.clone());
                            (output.out)(&RunnerEvent::AgentStarted{label:request.label,model:request.model.unwrap_or_else(||request.tier.map_or_else(||"profile default".into(),|tier|format!("tier:{tier}"))),provider:request.provider.unwrap_or_else(||"profile default".into())});
                        }
                        EngineEvent::AgentRefused{label,reason,..}=>{
                            refused+=1;
                            output.diagnostic(format!("agent {label} refused: {reason}"));
                            budget_reason=Some(reason);
                            (output.out)(&RunnerEvent::AgentFinished{label,state:"refused".into()});
                        }
                        EngineEvent::AgentFinished{call_id,outcome,..}=>{
                            let label=labels.remove(&call_id).unwrap_or_default();
                            let state=match outcome {OutcomeKind::Completed=>"completed",OutcomeKind::CutShort{steps,limit}=>{let reason=CutShort{steps,limit}.describe();output.diagnostic(format!("agent {label}: {reason}; its reply was kept"));cut_short_agents.push(AgentNote{label:label.clone(),reason:reason.clone()});"cut short"},OutcomeKind::Failed{reason}=>{output.diagnostic(reason.clone());failed_agents.push(AgentNote{label:label.clone(),reason});"failed"},OutcomeKind::Skipped=>"skipped",OutcomeKind::Stopped{reason}=>{budget_stopped+=1;output.diagnostic(format!("agent {label} stopped: {reason}"));budget_reason=Some(reason);"stopped"},OutcomeKind::Aborted=>"aborted",OutcomeKind::Abandoned=>"abandoned"};
                            (output.out)(&RunnerEvent::AgentFinished{label,state:state.into()});
                        }
                        _=>{},
                    }
                }
                Some(Input::Outcome(outcome))=>{if !stop_requested{break outcome;}engine_outcome=Some(outcome);}
                Some(Input::Scheduler(event))=>{
                    if let Some(message)=budget_note(&event,||gate.budget_stop()) {
                        journal.record_engine_event(&EngineEvent::Log{message,workflow_path:vec![]}).await.map_err(|e| e.to_string())?;
                    }
                    if event["event"]=="budget_reached" && !stop_requested {
                        stop_for_budget(&port,dir.path(),&stop_sent,&mut tasks.handles);
                    }
                    (output.out)(&RunnerEvent::Scheduler(event));
                }
                Some(Input::Notice(JournalNotice::NeedsOperator{key,label,attempt,episode,cause,detail,..}))=>(output.out)(&RunnerEvent::Parked{run_id:dir.run_id().as_str().into(),label,key,attempt,episode,cause:cause.as_str().into(),reason:detail,choices:cause.choices().iter().map(|c|(*c).to_owned()).collect()}),
                Some(Input::Notice(JournalNotice::Warning(warning)))=>output.diagnostic(format!("journal warning: {warning:?}")),
                Some(Input::Notice(JournalNotice::RouteNote{message}))=>(output.out)(&RunnerEvent::Log{message}),
                Some(Input::Notice(_))=>{},
                Some(Input::Signal) if stop_requested=>{cancel.cancel();drop(durable_tx);forward.abort();return Ok(3);}
                Some(Input::Signal|Input::Poll)=>{
                    let signal=matches!(message,Some(Input::Signal));
                    let pending=RunControl::pending(&dir);gate.set_paused(stop_requested || pending.pause.is_some());
                    if gate.is_paused() != journaled_paused {
                        journal.record_engine_event(&EngineEvent::Log {
                            message: if gate.is_paused() { "runner: admission gate paused" } else { "runner: admission gate open" }.into(),
                            workflow_path: vec![],
                        }).await.map_err(|e| e.to_string())?;
                        journaled_paused = gate.is_paused();
                    }
                    if (signal||pending.stop.is_some()) && !stop_requested {
                        stop_requested = true;
                        gate.set_stopping();
                        gate.set_paused(true);
                        cancel.cancel();
                    }
                    // A turn admitted after the budget ran out is stopped on the next tick, and a
                    // stop the gateway did not confirm is sent again.
                    if !stop_requested && gate.budget_stop().is_some() {
                        stop_for_budget(&port,dir.path(),&stop_sent,&mut tasks.handles);
                    }
                }
                Some(Input::Failure(error))=>{cancel.cancel();return Err(error);}
                None=>return Err("runner input channel closed".into()),
            },
        }
        if stop_requested && engine_outcome.is_some() && stopping.is_none() && stopped.is_none() {
            backend.cancel_queued().await.map_err(|e| e.to_string())?;
            let port = port.clone();
            let journal = journal.clone();
            let path = dir.path().to_owned();
            stopping = Some(Box::pin(async move {
                settle_stop(port, journal, path, settle_bound, receipt_poll).await
            }));
        }
        if stopped.is_some() && engine_outcome.is_some() {
            break RunOutcome::Aborted {
                reason: "cancelled".into(),
            };
        }
    };
    cancel.cancel();
    drop(durable_tx);
    forward
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let exhausted = refused + budget_stopped > 0;
    let mut file = match outcome {
        // The script returned, but not every agent it asked for ran to the end.
        RunOutcome::Completed { result } if exhausted => OutcomeFile {
            v: 1,
            outcome: "budget_exhausted".into(),
            result: Some(result),
            error: None,
            reason: gate.budget_stop().or(budget_reason),
            finished_at_ms: 0,
            summary: Value::Null,
            failed_agents: Vec::new(),
            cut_short_agents: Vec::new(),
            leftover_edits: Vec::new(),
            untracked_work: Vec::new(),
        },
        // The script returned normally, but at least one agent failed.
        RunOutcome::Completed { result } if !failed_agents.is_empty() => OutcomeFile {
            v: 1,
            outcome: "completed_with_failures".into(),
            result: Some(result),
            error: None,
            reason: Some(match failed_agents.len() {
                1 => "1 agent failed".to_owned(),
                n => format!("{n} agents failed"),
            }),
            finished_at_ms: 0,
            summary: Value::Null,
            failed_agents: Vec::new(),
            cut_short_agents: Vec::new(),
            leftover_edits: Vec::new(),
            untracked_work: Vec::new(),
        },
        RunOutcome::Completed { result } => OutcomeFile {
            v: 1,
            outcome: "completed".into(),
            result: Some(result),
            error: None,
            reason: None,
            finished_at_ms: 0,
            summary: Value::Null,
            failed_agents: Vec::new(),
            cut_short_agents: Vec::new(),
            leftover_edits: Vec::new(),
            untracked_work: Vec::new(),
        },
        // The script let the budget error end it: the run ended on its budget, with no result.
        RunOutcome::Failed { error } if exhausted && error.name == "BudgetExhaustedError" => {
            OutcomeFile {
                v: 1,
                outcome: "budget_exhausted".into(),
                result: None,
                error: Some(FailureView {
                    name: error.name,
                    message: error.message,
                }),
                reason: gate.budget_stop().or(budget_reason),
                finished_at_ms: 0,
                summary: Value::Null,
                failed_agents: Vec::new(),
                cut_short_agents: Vec::new(),
                leftover_edits: Vec::new(),
                untracked_work: Vec::new(),
            }
        }
        RunOutcome::Failed { error } => OutcomeFile {
            v: 1,
            outcome: "failed".into(),
            result: None,
            error: Some(FailureView {
                name: error.name,
                message: error.message,
            }),
            reason: None,
            finished_at_ms: 0,
            summary: Value::Null,
            failed_agents: Vec::new(),
            cut_short_agents: Vec::new(),
            leftover_edits: Vec::new(),
            untracked_work: Vec::new(),
        },
        RunOutcome::Aborted { reason } => {
            let reason = if let Some(unsettled) = stopped {
                for session in &unsettled {
                    output.diagnostic(format!(
                        "stop unconfirmed: session {session} still unsettled"
                    ));
                }
                if unsettled.is_empty() {
                    "stopped"
                } else {
                    "stopped-unconfirmed"
                }
                .to_owned()
            } else if reason.contains("operator") || reason.contains("abort") {
                "operator-abort".into()
            } else {
                reason
            };
            OutcomeFile {
                v: 1,
                outcome: "aborted".into(),
                result: None,
                error: None,
                reason: Some(reason),
                finished_at_ms: 0,
                summary: Value::Null,
                failed_agents: Vec::new(),
                cut_short_agents: Vec::new(),
                leftover_edits: Vec::new(),
                untracked_work: Vec::new(),
            }
        }
    };
    file.failed_agents = failed_agents;
    file.cut_short_agents = cut_short_agents;
    // Edits an agent left behind stay visible: they are listed here, in the journal and in the
    // result file. Nothing is deleted.
    let records = control::read_records(dir.path()).map_err(|e| e.to_string())?;
    file.untracked_work = super::leftover::untracked_work(&records);
    let worktrees = dir.path().join("worktrees");
    file.leftover_edits = tokio::task::spawn_blocking(move || {
        super::leftover::leftover_folders(&records, &worktrees)
    })
    .await
    .map_err(|e| e.to_string())?;
    for folder in &file.leftover_edits {
        let message = super::leftover::describe(folder);
        journal
            .record_engine_event(&EngineEvent::Log {
                message: message.clone(),
                workflow_path: vec![],
            })
            .await
            .map_err(|e| e.to_string())?;
        (output.out)(&RunnerEvent::Log { message });
    }
    let kind = match file.outcome.as_str() {
        "completed" => RunOutcomeKind::Completed,
        "completed_with_failures" => RunOutcomeKind::CompletedWithFailures,
        "budget_exhausted" => RunOutcomeKind::BudgetExhausted,
        "failed" => RunOutcomeKind::Failed,
        _ => RunOutcomeKind::Aborted,
    };
    journal.finish(kind).await.map_err(|e| e.to_string())?;
    let _final_summary = journal.summary();
    file.summary =
        control::latest_summary(&control::read_records(dir.path()).map_err(|e| e.to_string())?);
    file.finished_at_ms = files::now_ms();
    files::write_json(&dir.path().join("outcome.json"), &file).map_err(|e| e.to_string())?;
    files::clear_transient_controls(dir.path()).map_err(|e| e.to_string())?;
    files::remove_owned_runner(dir.path()).map_err(|e| e.to_string())?;
    let spend = SpendView::from_spend(scheduler.status().spend);
    services.finish_recording()?;
    (output.out)(&RunnerEvent::Finished {
        run_id: dir.run_id().as_str().into(),
        outcome: file.outcome.clone(),
        result: file.result,
        spend,
    });
    Ok(match file.outcome.as_str() {
        "completed" => 0,
        "budget_exhausted" => 4,
        // The script returned, so the result is printed, but an agent failed: a caller that
        // reads only the exit code must not take the run for a clean one.
        "completed_with_failures" => 5,
        "failed" => 1,
        _ => 3,
    })
}

/// Count each agent session's streamed usage toward the run's budget as it arrives.
async fn count_live_usage(mut feed: AgentFeedReceiver, scheduler: Scheduler) {
    while let Some(item) = feed.recv().await {
        if let AgentFeedItem::Event {
            session_id, frame, ..
        } = item
            && let hermes_contract::events::Notification::SessionUsage(p) = frame.notification
            && let Some(reading) = Reading::from_payload(&p)
        {
            scheduler.observe_usage(&session_id, reading);
        }
    }
}
/// The journal line for a budget event the scheduler reported, if it needs one.
fn budget_note(event: &Value, stop: impl FnOnce() -> Option<String>) -> Option<String> {
    match event["event"].as_str()? {
        "budget_reached" => Some(format!(
            "budget: {}; no new agents start and running agents are being stopped",
            stop().unwrap_or_else(|| "budget reached".into())
        )),
        "usage_unmetered" => Some(format!(
            "budget: agent {} streamed no token usage before its turn settled; its tokens count only when its turns settle",
            event["label"].as_str().unwrap_or_default()
        )),
        _ => None,
    }
}
/// Interrupt every admitted turn through the same stop the operator's stop uses. The
/// interrupted calls settle and end as stopped for the budget. `sent` holds the admissions
/// with a stop in flight or confirmed; a stop the gateway did not confirm leaves the set, so
/// the next tick sends it again while the turn is still admitted.
fn stop_for_budget(
    port: &HermesPort,
    dir: &Path,
    sent: &Arc<std::sync::Mutex<BTreeSet<String>>>,
    handles: &mut Vec<tokio::task::JoinHandle<()>>,
) {
    let Ok(targets) = control::read_records(dir)
        .map_err(|e| e.to_string())
        .and_then(|records| control::stop_targets(&records).map_err(|e| e.to_string()))
    else {
        return;
    };
    handles.retain(|handle| !handle.is_finished());
    for target in targets {
        let fresh = sent
            .lock()
            .is_ok_and(|mut set| set.insert(target.admission_id.clone()));
        if fresh {
            let (port, sent) = (port.clone(), sent.clone());
            handles.push(tokio::spawn(async move {
                // `NotRunning` can also mean the port has no receipt yet for a reattached turn.
                if port.stop(&target.profile, &target.session_id).await != StopResult::Sent
                    && let Ok(mut set) = sent.lock()
                {
                    set.remove(&target.admission_id);
                }
            }));
        }
    }
}

async fn verify_restarts(
    port: &HermesPort,
    path: &Path,
    keys: &[String],
    bound: Duration,
) -> Result<(), String> {
    let records = control::read_records(path).map_err(|e| e.to_string())?;
    for target in control::restart_targets(&records, keys)? {
        let receipt = tokio::time::timeout(bound, async {
            port.attach(target.profile.clone(), target.session_id.clone())
                .await?;
            port.receipt(target.profile, target.session_id, target.admission_id)
                .await
        })
        .await
        .map_err(|_| format!("restart for {} could not confirm settlement", target.key))?
        .map_err(|e| e.to_string())?;
        if receipt.status != AdmissionState::Terminal {
            return Err(format!(
                "restart for {} requires stopping and settling its running admission first",
                target.key
            ));
        }
    }
    Ok(())
}

async fn settle_stop(
    port: HermesPort,
    journal: Arc<RunJournal>,
    path: PathBuf,
    bound: Duration,
    poll: Duration,
) -> Vec<String> {
    let (targets, mut uncertain) = match control::read_records(&path).and_then(|r| {
        control::stop_targets(&r)
            .and_then(|targets| control::uncertain_calls(&r).map(|uncertain| (targets, uncertain)))
            .map_err(|e| crate::journal::JournalError::Io(std::io::Error::other(e)))
    }) {
        Ok(t) => t,
        Err(e) => return vec![format!("journal unavailable: {e}")],
    };
    let deadline = tokio::time::Instant::now() + bound;
    let mut pending = Vec::new();
    for target in targets {
        let attached = tokio::time::timeout_at(
            deadline,
            port.attach(target.profile.clone(), target.session_id.clone()),
        )
        .await;
        if !matches!(attached, Ok(Ok(()))) {
            pending.push(target);
            continue;
        }
        let _ =
            tokio::time::timeout_at(deadline, port.stop(&target.profile, &target.session_id)).await;
        pending.push(target);
    }
    while !pending.is_empty() && tokio::time::Instant::now() < deadline {
        let mut remaining = Vec::new();
        for target in pending {
            let poll_deadline = (tokio::time::Instant::now() + poll).min(deadline);
            let result = tokio::time::timeout_at(
                poll_deadline,
                port.receipt(
                    target.profile.clone(),
                    target.session_id.clone(),
                    target.admission_id.clone(),
                ),
            )
            .await;
            match result {
                Ok(Ok(receipt)) if receipt.status == AdmissionState::Terminal => {
                    let _ = journal
                        .append(run_stop_settlement(
                            &target.key,
                            target.attempt,
                            target.turn,
                            receipt,
                        ))
                        .await;
                }
                _ => remaining.push(target),
            }
        }
        pending = remaining;
        if !pending.is_empty() {
            tokio::time::sleep_until((tokio::time::Instant::now() + poll).min(deadline)).await;
        }
    }
    uncertain.extend(pending.into_iter().map(|t| t.session_id));
    uncertain
}
/// A turn the run's stop settled, marked as stopped by the runner when it ended stopped, so a
/// later resume fails it as part of the stop instead of parking it for the operator.
fn run_stop_settlement(
    key: &str,
    attempt: u32,
    turn: u32,
    receipt: crate::journal::Receipt,
) -> crate::journal::Record {
    let mut record = crate::journal::settled_record(key, attempt, turn, receipt);
    if let crate::journal::Record::TurnSettled {
        outcome,
        result_meta,
        ..
    } = &mut record
        && crate::journal::stopped_outcome(outcome)
        && let Some(meta) = result_meta.as_object_mut()
    {
        meta.insert(
            crate::journal::STOPPED_BY.into(),
            crate::journal::StopSource::RunStop.as_str().into(),
        );
    }
    record
}
fn signal_task(sender: mpsc::UnboundedSender<Input>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            let (Ok(mut interrupt), Ok(mut terminate), Ok(mut hangup)) = (
                signal(SignalKind::interrupt()),
                signal(SignalKind::terminate()),
                signal(SignalKind::hangup()),
            ) else {
                return;
            };
            loop {
                tokio::select! {
                    value = interrupt.recv() => if value.is_none() { return; },
                    value = terminate.recv() => if value.is_none() { return; },
                    _ = hangup.recv() => continue,
                }
                if sender.send(Input::Signal).is_err() {
                    return;
                }
            }
        }
        #[cfg(not(unix))]
        loop {
            if tokio::signal::ctrl_c().await.is_err() || sender.send(Input::Signal).is_err() {
                return;
            }
        }
    })
}
