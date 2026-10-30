//! Workflow commands run before terminal initialization and use the runner's file protocol.

use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};

use clap::{Args, ColorChoice, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{Value, json};
use talaria_workflow::{
    journal::{OnUnknown, Resolution},
    runner::{self, ResumeRequest, RunExit, RunRequest, RunnerEvent, SpendView, StopExit, execute},
};

use crate::args::{neutralise, required_path, required_text};

#[path = "workflow_builtins.rs"]
pub mod builtins;

#[derive(Clone, Debug, Parser)]
#[command(name="talaria workflow",bin_name="talaria workflow",color=ColorChoice::Never,about="Run, resume, list, stop, and resolve durable workflow runs")]
pub struct WorkflowArgs {
    #[command(subcommand)]
    pub command: WorkflowCommand,
}
#[derive(Clone, Debug, Subcommand)]
pub enum WorkflowCommand {
    Skill(crate::workflow_skill::SkillArgs),
    Roster(crate::workflow_roster::RosterArgs),
    Builtins(builtins::BuiltinArgs),
    Run(RunArgs),
    Resume(ResumeArgs),
    List(ListArgs),
    Stop(StopArgs),
    Resolve(ResolveArgs),
    Compile(CompileArgs),
}
#[derive(Clone, Debug, Args)]
pub struct CompileArgs {
    #[arg(value_name = "SCRIPT", value_parser = required_path)]
    pub script: PathBuf,
    #[arg(long, default_value = "default", value_parser = required_text)]
    pub board: String,
    #[arg(long, value_parser = required_text)]
    pub tenant: Option<String>,
    #[arg(long)]
    pub args: Option<String>,
    #[arg(long, value_parser = required_text)]
    pub profile: Option<String>,
    #[arg(long, value_parser = required_path)]
    pub repo: Option<PathBuf>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub no_dispatch: bool,
}
#[derive(Clone, Debug, Args)]
pub struct RunArgs {
    #[arg(value_name="SCRIPT",value_parser=required_path,required_unless_present="foreground_child",conflicts_with="foreground_child")]
    pub script: Option<PathBuf>,
    #[arg(long, conflicts_with = "foreground_child")]
    pub detach: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long, conflicts_with = "args_file")]
    pub args: Option<String>,
    #[arg(long,value_parser=required_path)]
    pub args_file: Option<PathBuf>,
    #[arg(long,default_value="default",value_parser=required_text)]
    pub profile: String,
    #[arg(long,value_parser=required_path)]
    pub staffing: Option<PathBuf>,
    #[arg(long,value_parser=required_path)]
    pub cwd: Option<PathBuf>,
    #[arg(long,value_parser=clap::value_parser!(u64).range(1..))]
    pub budget_tokens: Option<u64>,
    #[arg(long,value_parser=positive_money)]
    pub budget_usd: Option<f64>,
    #[arg(long,default_value="wait",value_parser=["wait","fail"])]
    pub on_unknown: String,
    #[arg(long,value_parser=required_path)]
    pub record: Option<PathBuf>,
    #[arg(long,value_parser=required_path)]
    pub runs_dir: Option<PathBuf>,
    #[arg(long,value_parser=required_path)]
    pub workflows_dir: Option<PathBuf>,
    #[arg(long, hide = true, requires = "run_dir")]
    pub foreground_child: bool,
    #[arg(long, hide = true, requires_all = ["foreground_child", "record"])]
    pub record_append: bool,
    #[arg(long,hide=true,value_parser=required_path,requires="foreground_child")]
    pub run_dir: Option<PathBuf>,
}
#[derive(Clone, Debug, Args)]
pub struct ResumeArgs {
    #[arg(value_name="RUN",value_parser=required_text)]
    pub target: String,
    #[arg(long,value_parser=required_path)]
    pub script: Option<PathBuf>,
    #[arg(long,value_parser=required_path)]
    pub staffing: Option<PathBuf>,
    #[arg(long,value_parser=required_path)]
    pub cwd: Option<PathBuf>,
    #[arg(long,value_parser=clap::value_parser!(u64).range(1..))]
    pub budget_tokens: Option<u64>,
    #[arg(long,value_parser=positive_money)]
    pub budget_usd: Option<f64>,
    #[arg(long,value_parser=required_path)]
    pub record: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
    #[arg(long,value_parser=required_path)]
    pub runs_dir: Option<PathBuf>,
    #[arg(long,value_parser=required_path)]
    pub workflows_dir: Option<PathBuf>,
}
#[derive(Clone, Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub json: bool,
    #[arg(long,value_parser=required_path)]
    pub runs_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 20)]
    pub limit: u64,
}
#[derive(Clone, Debug, Args)]
pub struct StopArgs {
    #[arg(value_name="RUN",value_parser=required_text)]
    pub target: String,
    #[arg(long,value_parser=required_path)]
    pub runs_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 35)]
    pub timeout: u64,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ResolutionArg {
    Fail,
    Rerun,
    Abort,
}
#[derive(Clone, Debug, Args)]
pub struct ResolveArgs {
    #[arg(value_name="RUN",value_parser=required_text)]
    pub target: String,
    #[arg(long,value_parser=required_path)]
    pub runs_dir: Option<PathBuf>,
    #[arg(long,value_parser=required_text)]
    pub key: String,
    #[arg(long)]
    pub attempt: u32,
    #[arg(long)]
    pub episode: u32,
    #[arg(long, value_enum)]
    pub resolution: ResolutionArg,
}
fn positive_money(text: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| "expected a finite amount above zero".into())
}
pub fn parse(args: &[String]) -> Result<WorkflowArgs, clap::Error> {
    let input = args
        .strip_prefix(&["talaria".into(), "workflow".into()])
        .or_else(|| args.strip_prefix(&["workflow".into()]))
        .unwrap_or(args);
    WorkflowArgs::try_parse_from(
        std::iter::once("talaria workflow".into()).chain(input.iter().cloned()),
    )
}
fn absolute(path: PathBuf) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
fn safe(text: &str) -> String {
    text.chars().flat_map(|c|match c {'\n'=>"\\n".chars().collect::<Vec<_>>(),'\r'=>"\\r".chars().collect(),c if c.is_control()||matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')=>vec!['?'],c=>vec![c]}).collect()
}
fn fail(text: impl std::fmt::Display, code: u8) -> u8 {
    eprintln!("{}", safe(&text.to_string()));
    code
}
fn exit_code(exit: RunExit) -> u8 {
    u8::try_from(exit.code).unwrap_or(1)
}

pub fn render_list(runs: &[runner::RunSummary], json_mode: bool) -> String {
    if json_mode {
        return format!(
            "{}\n",
            serde_json::to_string(runs).unwrap_or_else(|_| "[]".into())
        );
    }
    let mut text = String::new();
    for run in runs {
        let parked = if run.parked > 0 {
            format!("  parked {}", run.parked)
        } else {
            String::new()
        };
        let tokens = run
            .tokens_spent
            .map_or_else(|| "-".into(), |v| v.to_string());
        let money = run
            .money_spent_usd
            .map_or_else(|| "-".into(), |v| format!("${v:.2}"));
        let timestamp =
            time::OffsetDateTime::from_unix_timestamp(run.started_at_ms.div_euclid(1000))
                .ok()
                .and_then(|time| {
                    time.format(&time::format_description::well_known::Rfc3339)
                        .ok()
                })
                .unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
        text.push_str(&format!(
            "{}  {}  {}  agents {}/{}{}  tokens {}  money {}  {}  {}\n",
            safe(&run.run_id),
            safe(&run.name),
            run.status.as_str(),
            run.agents_finished,
            run.agents_total_known,
            parked,
            tokens,
            money,
            timestamp,
            safe(&run.run_dir)
        ));
        if let Some(warning) = &run.warning {
            text.push_str(&format!("warning: {}\n", safe(warning)));
        }
    }
    text
}
struct Renderer {
    json: bool,
    dir: Option<PathBuf>,
    failed: bool,
}
impl Renderer {
    fn new(json: bool) -> Self {
        Self {
            json,
            dir: None,
            failed: false,
        }
    }
    fn json(&mut self, value: &impl Serialize) {
        let mut out = io::stdout().lock();
        if serde_json::to_writer(&mut out, value)
            .and_then(|_| out.write_all(b"\n").map_err(serde_json::Error::io))
            .is_err()
            || out.flush().is_err()
        {
            self.failed = true;
        }
    }
    fn text(&mut self, text: &str) {
        let mut out = io::stdout().lock();
        if out
            .write_all(text.as_bytes())
            .and_then(|_| out.flush())
            .is_err()
        {
            self.failed = true;
        }
    }
    fn emit(&mut self, event: &RunnerEvent) {
        if let RunnerEvent::Diagnostic { text } = event {
            eprintln!("{}", safe(text));
            return;
        }
        match event {
            RunnerEvent::RunStarted { run_id, run_dir } => {
                self.dir = Some(run_dir.clone());
                if self.json {
                    #[derive(Serialize)]
                    struct Started<'a> {
                        event: &'static str,
                        run_id: &'a str,
                        run_dir: &'a std::path::Path,
                    }
                    self.json(&Started {
                        event: "run_started",
                        run_id,
                        run_dir,
                    });
                } else {
                    self.text(&format!(
                        "run {} in {}\n",
                        safe(run_id),
                        safe(&run_dir.to_string_lossy())
                    ));
                }
            }
            RunnerEvent::Phase { title } => {
                if self.json {
                    self.json(&json!({"event":"phase","title":title}));
                } else {
                    self.text(&format!("phase: {}\n", safe(title)));
                }
            }
            RunnerEvent::Log { message } => {
                if self.json {
                    self.json(&json!({"event":"log","message":message}));
                } else {
                    self.text(&format!("log: {}\n", safe(message)));
                }
            }
            RunnerEvent::AgentStarted {
                label,
                model,
                provider,
            } => {
                if self.json {
                    self.json(&json!({"event":"agent_started","label":label,"model":model,"provider":provider}));
                } else {
                    self.text(&format!(
                        "agent {}: started ({} on {})\n",
                        safe(label),
                        safe(model),
                        safe(provider)
                    ));
                }
            }
            RunnerEvent::AgentFinished { label, state } => {
                if self.json {
                    self.json(&json!({"event":"agent_finished","label":label,"state":state}));
                } else {
                    self.text(&format!("agent {}: {}\n", safe(label), safe(state)));
                }
            }
            RunnerEvent::Scheduler(event) => {
                if self.json {
                    self.json(&json!({"event":"scheduler","value":event}));
                } else {
                    self.text(&format!("scheduler: {event}\n"));
                }
            }
            RunnerEvent::Parked {
                run_id,
                label,
                key,
                attempt,
                episode,
            } => {
                if self.json {
                    #[derive(Serialize)]
                    struct Parked<'a> {
                        event: &'static str,
                        run_id: &'a str,
                        label: &'a str,
                        key: &'a str,
                        attempt: u32,
                        episode: u32,
                    }
                    self.json(&Parked {
                        event: "parked",
                        run_id,
                        label,
                        key,
                        attempt: *attempt,
                        episode: *episode,
                    });
                } else {
                    self.text(&format!("needs operator: run {} agent {} key {} attempt {} episode {} - answer: talaria workflow resolve {} --key {} --attempt {} --episode {} --resolution fail|rerun|abort\n",safe(run_id),safe(label),safe(key),attempt,episode,safe(run_id),safe(key),attempt,episode));
                }
            }
            RunnerEvent::Finished {
                run_id,
                outcome,
                result,
                spend,
            } => self.finished(run_id, outcome, result.as_ref(), spend),
            RunnerEvent::Diagnostic { .. } => {}
        }
    }
    fn finished(&mut self, run_id: &str, outcome: &str, result: Option<&Value>, spend: &SpendView) {
        let saved = self
            .dir
            .as_ref()
            .and_then(|dir| fs::read(dir.join("outcome.json")).ok())
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .unwrap_or(Value::Null);
        #[derive(Serialize)]
        struct Completed<'a> {
            event: &'static str,
            run_id: &'a str,
            outcome: &'a str,
            result: &'a Value,
            spend: &'a SpendView,
        }
        #[derive(Serialize)]
        struct Failed<'a> {
            event: &'static str,
            run_id: &'a str,
            outcome: &'a str,
            error: &'a Value,
            spend: &'a SpendView,
        }
        #[derive(Serialize)]
        struct Aborted<'a> {
            event: &'static str,
            run_id: &'a str,
            outcome: &'a str,
            reason: &'a str,
            spend: &'a SpendView,
        }
        match outcome {
            "completed" => {
                let result = result.unwrap_or(&Value::Null);
                if self.json {
                    self.json(&Completed {
                        event: "run_finished",
                        run_id,
                        outcome,
                        result,
                        spend,
                    });
                } else {
                    self.text(&format!(
                        "Result:\n{}\n",
                        serde_json::to_string_pretty(result).unwrap_or_else(|_| "null".into())
                    ));
                }
            }
            "failed" => {
                if self.json {
                    self.json(&Failed {
                        event: "run_finished",
                        run_id,
                        outcome,
                        error: &saved["error"],
                        spend,
                    });
                } else {
                    self.text(&format!(
                        "Failed: {}: {}\n",
                        safe(saved["error"]["name"].as_str().unwrap_or("Error")),
                        safe(
                            saved["error"]["message"]
                                .as_str()
                                .unwrap_or("workflow failed")
                        )
                    ));
                }
            }
            _ => {
                let reason = saved["reason"].as_str().unwrap_or("interrupted");
                if self.json {
                    self.json(&Aborted {
                        event: "run_finished",
                        run_id,
                        outcome,
                        reason,
                        spend,
                    });
                } else {
                    self.text(&format!("Aborted: {}\n", safe(reason)));
                }
            }
        }
    }
}

pub async fn run(args: WorkflowArgs) -> u8 {
    match args.command {
        WorkflowCommand::Skill(args) => crate::workflow_skill::execute(args).await,
        WorkflowCommand::Roster(args) => crate::workflow_roster::execute(args).await,
        WorkflowCommand::List(args) => {
            let dir = match absolute(runner::runs_dir(args.runs_dir.as_deref())) {
                Ok(dir) => dir,
                Err(e) => return fail(e, 1),
            };
            if let Err(e) = fs::read_dir(&dir)
                && e.kind() != io::ErrorKind::NotFound
            {
                return fail(
                    format!("cannot read runs directory {}: {e}", dir.display()),
                    1,
                );
            }
            let mut runs = runner::discover_runs(&dir);
            runs.truncate(args.limit.try_into().unwrap_or(usize::MAX));
            let text = if runs.is_empty() && !args.json && args.limit > 0 {
                "No workflow runs found.\n".into()
            } else {
                render_list(&runs, args.json)
            };
            if let Err(e) = io::stdout().write_all(text.as_bytes()) {
                return fail(e, 1);
            }
            0
        }
        WorkflowCommand::Stop(args) => {
            let runs = runner::runs_dir(args.runs_dir.as_deref());
            let dir = match runner::find_run(&runs, &args.target) {
                Ok(dir) => dir,
                Err(e) => return fail(e, 2),
            };
            let id = dir.file_name().unwrap_or_default().to_string_lossy();
            match runner::request_stop(&runs, &args.target, Duration::from_secs(args.timeout)) {
                StopExit::Stopped => {
                    println!("stopped {}", safe(&id));
                    0
                }
                StopExit::AlreadyFinished { outcome } => {
                    println!("already finished ({})", safe(&outcome));
                    0
                }
                StopExit::Unconfirmed => {
                    println!(
                        "stop requested; run {} is still finishing — stop not confirmed",
                        safe(&id)
                    );
                    1
                }
                StopExit::UnknownId => fail(
                    format!(
                        "unknown run '{}' (searched {})",
                        args.target,
                        runs.display()
                    ),
                    2,
                ),
            }
        }
        WorkflowCommand::Resolve(args) => {
            let resolution = match args.resolution {
                ResolutionArg::Fail => Resolution::Fail,
                ResolutionArg::Rerun => Resolution::Rerun,
                ResolutionArg::Abort => Resolution::Abort,
            };
            match runner::answer_parked(
                &runner::runs_dir(args.runs_dir.as_deref()),
                &args.target,
                &args.key,
                args.attempt,
                args.episode,
                resolution,
            ) {
                Ok(()) => 0,
                Err(runner::AnswerError::Target(e)) => fail(e, 2),
                Err(e) => fail(e, 1),
            }
        }
        WorkflowCommand::Resume(args) => {
            let mut renderer = Renderer::new(args.json);
            let req = ResumeRequest {
                target: args.target,
                script: args.script,
                staffing: args.staffing,
                cwd: args.cwd,
                budget_tokens: args.budget_tokens,
                budget_usd: args.budget_usd,
                record: args.record,
                settle_bound: Duration::from_secs(30),
                receipt_poll: Duration::from_millis(250),
                runs_dir: runner::runs_dir(args.runs_dir.as_deref()),
                workflows_dir: runner::workflows_dir(args.workflows_dir.as_deref()),
                json: args.json,
            };
            let exit = runner::run_resume(req, &mut |event| renderer.emit(event)).await;
            if renderer.failed { 1 } else { exit_code(exit) }
        }
        WorkflowCommand::Run(args) => run_command(args).await,
        WorkflowCommand::Builtins(args) => builtins::execute(args),
        WorkflowCommand::Compile(args) => {
            let _ = args;
            eprintln!("compile is not available");
            1
        }
    }
}
async fn run_command(args: RunArgs) -> u8 {
    let mut renderer = Renderer::new(args.json && !args.foreground_child);
    if let Some(dir) = args.run_dir {
        let exit = execute::run_child_with_options(
            dir,
            execute::ChildOptions {
                staffing: args.staffing,
                budget_tokens: args.budget_tokens,
                budget_usd: args.budget_usd,
                workflows_dir: args.workflows_dir,
                record: args.record,
                record_append: args.record_append,
            },
            &mut |e| renderer.emit(e),
        )
        .await;
        return if renderer.failed { 1 } else { exit_code(exit) };
    }
    let args_present = args.args.is_some() || args.args_file.is_some();
    let input = match (args.args, args.args_file) {
        (Some(text), _) => serde_json::from_str(&text).map_err(|e| e.to_string()),
        (_, Some(path)) => fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string())),
        _ => Ok(Value::Null),
    };
    let input = match input {
        Ok(input) => input,
        Err(e) => return fail(format!("invalid workflow arguments JSON: {e}"), 2),
    };
    let Some(script) = args.script else {
        return fail("a workflow script is required", 2);
    };
    let cwd = match args.cwd.map_or_else(std::env::current_dir, Ok) {
        Ok(cwd) => cwd,
        Err(e) => return fail(e, 2),
    };
    let runs_dir = match absolute(runner::runs_dir(args.runs_dir.as_deref())) {
        Ok(dir) => dir,
        Err(e) => return fail(e, 2),
    };
    let workflows_dir = match absolute(runner::workflows_dir(args.workflows_dir.as_deref())) {
        Ok(dir) => dir,
        Err(e) => return fail(e, 2),
    };
    let script = match resolve_run_script(&script.to_string_lossy(), &workflows_dir) {
        Ok(path) => path,
        Err((message, code)) => return fail(message, code),
    };
    let mut extra = Vec::new();
    for (flag, value) in [
        ("--staffing", args.staffing.as_ref()),
        ("--workflows-dir", Some(&workflows_dir)),
        ("--runs-dir", Some(&runs_dir)),
    ] {
        if let Some(value) = value {
            extra.push(flag.to_owned());
            extra.push(value.to_string_lossy().into_owned());
        }
    }
    if let Some(tokens) = args.budget_tokens {
        extra.extend(["--budget-tokens".into(), tokens.to_string()]);
    }
    if let Some(usd) = args.budget_usd {
        extra.extend(["--budget-usd".into(), usd.to_string()]);
    }
    if let Some(record) = &args.record {
        extra.extend([
            "--record".into(),
            record.to_string_lossy().into_owned(),
            "--record-append".into(),
        ]);
    }
    let req = RunRequest {
        script,
        args: input,
        profile: args.profile,
        staffing: args.staffing,
        cwd,
        budget_tokens: args.budget_tokens,
        budget_usd: args.budget_usd,
        on_unknown: if args.on_unknown == "fail" {
            OnUnknown::Fail
        } else {
            OnUnknown::Wait
        },
        runs_dir,
        workflows_dir,
        talaria_version: env!("CARGO_PKG_VERSION").into(),
        json: args.json,
        record: args.record,
        settle_bound: Duration::from_secs(30),
        receipt_poll: Duration::from_millis(250),
    };
    if args.detach {
        let prepared = match execute::prepare_new_with_args(
            &req,
            args_present.then(|| req.args.clone()),
            &mut |e| renderer.emit(e),
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(exit) => return exit_code(exit),
        };
        let run_id = prepared.dir.run_id().as_str().to_owned();
        let run_dir = prepared.dir.path().to_owned();
        let executable = match std::env::current_exe() {
            Ok(path) => path,
            Err(e) => return fail(e, 1),
        };
        match execute::spawn_detached(prepared, &executable, &extra) {
            Ok(_) => {
                if args.json {
                    renderer.json(&json!({"run_id":run_id,"run_dir":run_dir,"detached":true}));
                } else {
                    renderer.text(&format!(
                        "run {} detached in {}\n",
                        safe(&run_id),
                        safe(&run_dir.to_string_lossy())
                    ));
                }
                if renderer.failed { 1 } else { 0 }
            }
            Err(e) => fail(format!("could not detach workflow: {e}"), 1),
        }
    } else {
        let input = args_present.then(|| req.args.clone());
        let exit = execute::run_new_with_args(req, input, &mut |e| renderer.emit(e)).await;
        if renderer.failed { 1 } else { exit_code(exit) }
    }
}
pub fn entry(args: &[String]) -> u8 {
    let args = match parse(args) {
        Ok(args) => args,
        Err(error) => {
            let code = if error.use_stderr() { 2 } else { 0 };
            if code == 0 {
                print!("{error}");
            } else {
                eprint!("{}", neutralise(&error.to_string()));
            }
            return code;
        }
    };
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(run(args)),
        Err(e) => fail(e, 1),
    }
}

/// Resolve names before runner preflight; an edited installed script always wins.
pub fn resolve_run_script(arg: &str, dir: &std::path::Path) -> Result<PathBuf, (String, u8)> {
    use talaria_workflow::builtins::{InstallMode, ScriptArg, install, resolve_script_arg};
    match resolve_script_arg(arg, dir).map_err(|e| (e, 2))? {
        ScriptArg::Path(path) | ScriptArg::Named { path, .. } => Ok(path),
        ScriptArg::MissingBuiltin { name, path } => {
            install(dir, InstallMode::MissingOnly).map_err(|e| {
                (
                    format!(
                        "could not install built-in workflows into {}: {e}",
                        dir.display()
                    ),
                    1,
                )
            })?;
            eprintln!("installed built-in workflows into {}", dir.display());
            match resolve_script_arg(arg, dir) {
                Ok(ScriptArg::Named { path, .. }) => Ok(path),
                _ => Err((
                    format!(
                        "built-in workflow '{name}' is still missing from {}: {} is not a regular file",
                        dir.display(),
                        path.display()
                    ),
                    1,
                )),
            }
        }
    }
}
