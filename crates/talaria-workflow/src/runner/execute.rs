use std::{
    collections::BTreeMap,
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
    engine::{self, CallId, EngineEvent, OutcomeKind, ParsedScript, RunOutcome, RunOutcomeKind},
    hermes::{ExplicitLaunch, HermesPort, LaunchDefaults, SchemaTurnPolicy},
    journal::{
        AdmissionState, GatewayPort, JournalNotice, JournaledBackend, NewManifest, OnUnknown,
        RetryPolicy, RunDir, RunJournal,
    },
    progress::RunControl,
    schedule::{MoneySource, Scheduler, SchedulerConfig, Spend},
    staffing::{
        PriceBook, Roster, RosterError, RunBudget, StaffedLaunch, StaffingResolver, StaffingTable,
        check_tiers, estimate_run_cost, staffing_drift,
    },
    worktree::GitWorktrees,
};

pub use super::services::RunnerServices;
use super::{
    control::{self, PausingGate},
    files::{self, FailureView, LaunchFile, OutcomeFile, RunnerFile},
    resolver::FilesystemResolver,
    runs::{self, Fold, LockProbe},
    services,
};

pub struct RunRequest {
    pub script: PathBuf,
    pub args: Value,
    pub profile: String,
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
    pub tokens: u64,
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
    let snapshot = roster.snapshot_with(&table, &budget);
    Ok(Preflight {
        table,
        budget,
        roster,
        snapshot,
    })
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
        &services,
        output,
    )
    .await?;
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
        if fold.terminal.as_deref() == Some("completed") && !restarting {
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
                Ok(saved) if saved.v == 1 && saved.outcome == "completed" => saved.result,
                _ => {
                    (output.out)(&RunnerEvent::Log {
                        message: format!(
                            "stored result unavailable; completed summary {}",
                            fold.summary.unwrap_or(Value::Null)
                        ),
                    });
                    None
                }
            };
            (output.out)(&RunnerEvent::Finished {
                run_id: dir.run_id().as_str().into(),
                outcome: "completed".into(),
                result,
                spend: SpendView {
                    tokens: fold.tokens.unwrap_or_default(),
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
        let preflight = preflight(
            table,
            req.budget_tokens,
            req.budget_usd,
            &dir.manifest().profile,
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
        Ok(None) => output.exit(0),
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
    let port = services.port(&dir.manifest().profile).await?;
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
    drop(tx);
    let mut labels = BTreeMap::<CallId, String>::new();
    let mut engine_outcome = None;
    let mut stopping: Option<futures::future::BoxFuture<'static, Vec<String>>> = None;
    let mut stopped = None;
    let mut stop_requested = false;
    let mut last_heartbeat = tokio::time::Instant::now();
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
                        EngineEvent::AgentFinished{call_id,outcome,..}=>{
                            let state=match outcome {OutcomeKind::Completed=>"completed",OutcomeKind::Failed{reason}=>{output.diagnostic(reason);"failed"},OutcomeKind::Skipped=>"skipped",OutcomeKind::Aborted=>"aborted",OutcomeKind::Abandoned=>"abandoned"};
                            (output.out)(&RunnerEvent::AgentFinished{label:labels.remove(&call_id).unwrap_or_default(),state:state.into()});
                        }
                        _=>{},
                    }
                }
                Some(Input::Outcome(outcome))=>{if !stop_requested{break outcome;}engine_outcome=Some(outcome);}
                Some(Input::Scheduler(event))=>(output.out)(&RunnerEvent::Scheduler(event)),
                Some(Input::Notice(JournalNotice::NeedsOperator{key,label,attempt,episode,..}))=>(output.out)(&RunnerEvent::Parked{run_id:dir.run_id().as_str().into(),label,key,attempt,episode}),
                Some(Input::Notice(JournalNotice::Warning(warning)))=>output.diagnostic(format!("journal warning: {warning:?}")),
                Some(Input::Notice(_))=>{},
                Some(Input::Signal) if stop_requested=>{cancel.cancel();drop(durable_tx);forward.abort();return Ok(3);}
                Some(Input::Signal|Input::Poll)=>{
                    let signal=matches!(message,Some(Input::Signal));
                    let pending=RunControl::pending(&dir);gate.set_paused(stop_requested || pending.pause.is_some());
                    if last_heartbeat.elapsed() >= Duration::from_secs(1) {
                        journal.record_engine_event(&EngineEvent::Log {
                            message: if gate.is_paused() { "runner: admission gate paused" } else { "runner: admission gate open" }.into(),
                            workflow_path: vec![],
                        }).await.map_err(|e| e.to_string())?;
                        last_heartbeat = tokio::time::Instant::now();
                    }
                    if (signal||pending.stop.is_some()) && !stop_requested {
                        stop_requested = true;
                        gate.set_paused(true);
                        cancel.cancel();
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
    let mut file = match outcome {
        RunOutcome::Completed { result } => OutcomeFile {
            v: 1,
            outcome: "completed".into(),
            result: Some(result),
            error: None,
            reason: None,
            finished_at_ms: 0,
            summary: Value::Null,
        },
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
            }
        }
    };
    let kind = match file.outcome.as_str() {
        "completed" => RunOutcomeKind::Completed,
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
        "failed" => 1,
        _ => 3,
    })
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
                        .append(crate::journal::settled_record(
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
