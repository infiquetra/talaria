//! Workflow commands run before terminal initialization and use the runner's file protocol.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use clap::{Args, ColorChoice, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{Value, json};
use talaria_workflow::{
    journal::{OnUnknown, Resolution},
    runner::{self, ResumeRequest, RunExit, RunRequest, RunnerEvent, SpendView, StopExit, execute},
    tokens::tokens_text,
};

use crate::args::{neutralise, required_path, required_text};

#[path = "workflow_builtins.rs"]
pub mod builtins;
#[path = "workflow_compile.rs"]
mod compile_command;

#[derive(Clone, Debug, Parser)]
#[command(name="talaria workflow",bin_name="talaria workflow",color=ColorChoice::Never,about="Run, resume, list, stop, and resolve durable workflow runs")]
pub struct WorkflowArgs {
    #[command(subcommand)]
    pub command: WorkflowCommand,
}
#[derive(Clone, Debug, Subcommand)]
pub enum WorkflowCommand {
    /// Install, inspect or remove the workflow skill that teaches Hermes to use this command.
    Skill(crate::workflow_skill::SkillArgs),
    /// Show the models and providers a workflow run could use, and what is left of their limits.
    Roster(crate::workflow_roster::RosterArgs),
    /// List, show, install or check the workflows that ship with Talaria.
    Builtins(builtins::BuiltinArgs),
    /// Start a workflow run from a script or a named workflow.
    Run(RunArgs),
    /// Continue a run that stopped, using its saved journal.
    Resume(ResumeArgs),
    /// List recent runs with their status, progress and spend.
    List(ListArgs),
    /// Stop a running workflow.
    Stop(StopArgs),
    /// Answer an agent call a run parked because its outcome is unknown: fail it, run it again,
    /// or abort the run.
    Resolve(ResolveArgs),
    /// Turn a workflow's agent calls into kanban board cards. Kanban boards only; it does not
    /// validate a script, so use `check` for that.
    Compile(CompileArgs),
    /// Check a script for problems before a run: meta block, syntax, agent options and tier
    /// names. Starts no agent and does not contact the gateway.
    Check(crate::workflow_check::CheckArgs),
    /// List or remove the git worktrees and branches that runs keep for review.
    Worktrees(crate::workflow_worktrees::WorktreesArgs),
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
    /// The model the session runs on; calls that name no model or tier use it.
    #[arg(long, value_parser = required_text, requires = "session_provider", conflicts_with = "foreground_child")]
    pub session_model: Option<String>,
    /// The provider of `--session-model`.
    #[arg(long, value_parser = required_text, requires = "session_model", conflicts_with = "foreground_child")]
    pub session_provider: Option<String>,
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
/// True for a missing or empty list of agents, which the final object leaves out.
fn is_unlisted(list: &&Value) -> bool {
    list.as_array().is_none_or(Vec::is_empty)
}
/// The failed and cut short agents as plain lines, or nothing when there are none.
fn agent_attention_text(failed: &Value, cut_short: &Value) -> String {
    let mut text = String::new();
    for (title, list) in [("Failed agents", failed), ("Cut short agents", cut_short)] {
        let Some(list) = list.as_array().filter(|list| !list.is_empty()) else {
            continue;
        };
        text.push_str(&format!("{title}:\n"));
        for note in list {
            text.push_str(&format!(
                "  {}: {}\n",
                safe(note["label"].as_str().unwrap_or("agent")),
                safe(note["reason"].as_str().unwrap_or("no reason given"))
            ));
        }
    }
    text
}
/// What the run left behind in agent folders, as plain lines, or nothing when there is none:
/// untracked work found on an agent's route, and folders with uncommitted changes.
fn leftovers_text(untracked: &Value, leftover: &Value) -> String {
    let mut text = String::new();
    if let Some(list) = untracked.as_array().filter(|list| !list.is_empty()) {
        text.push_str("Untracked work found on agent routes:\n");
        for note in list {
            text.push_str(&format!("  {}\n", safe(note.as_str().unwrap_or_default())));
        }
    }
    if let Some(list) = leftover.as_array().filter(|list| !list.is_empty()) {
        text.push_str("Uncommitted changes left in agent working folders:\n");
        for folder in list {
            let count = folder["changed_files"].as_u64().unwrap_or(0);
            text.push_str(&format!(
                "  {}: {count} changed file{}\n",
                safe(folder["path"].as_str().unwrap_or("unknown folder")),
                if count == 1 { "" } else { "s" }
            ));
        }
    }
    text
}
pub fn safe(text: &str) -> String {
    let mut prepared = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\n' => prepared.push_str("\\n"),
            '\r' => prepared.push_str("\\r"),
            other => prepared.push(other),
        }
    }
    talaria_core::text::sanitize_with_options(
        &prepared,
        talaria_core::text::SanitizeOptions::CLI,
        talaria_core::text::Replacement::Char('?'),
    )
    .into_owned()
}
fn fail(text: impl std::fmt::Display, code: u8) -> u8 {
    eprintln!("{}", safe(&text.to_string()));
    code
}
/// True when `path` or one of its ancestors holds a `.git` entry (a folder, or the file a
/// linked worktree uses).
fn inside_git_repository(path: &Path) -> bool {
    path.ancestors().any(|dir| dir.join(".git").exists())
}
/// A warning, not a refusal: the bootstrap pattern runs its first agent here without isolation.
fn outside_git_warning(cwd: &Path) -> Option<String> {
    if inside_git_repository(cwd) {
        return None;
    }
    Some(format!(
        "warning: the working folder {} is not inside a git repository. Agents with isolation: 'worktree' will fail here. To start a new project, make the script's first agent run without isolation (the run's --cwd is that folder) and have it do only `git init` and an initial commit; give every later agent worktree isolation.",
        cwd.display()
    ))
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
        let tokens = run.tokens_spent.map_or_else(
            || "tokens -".into(),
            |v| {
                tokens_text(
                    v,
                    run.cache_read_tokens.unwrap_or_default(),
                    run.cache_unknown_turns,
                    |n| n.to_string(),
                )
            },
        );
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
            "{}  {}  {}  agents {}/{}{}  {}  money {}  {}  {}\n",
            safe(&run.run_id),
            safe(&run.name),
            run.status.label(),
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
        // Agents that failed or were cut short come first, before the result they qualify.
        let failed_agents = &saved["failed_agents"];
        let cut_short_agents = &saved["cut_short_agents"];
        let untracked_work = &saved["untracked_work"];
        let leftover_edits = &saved["leftover_edits"];
        let attention = format!(
            "{}{}",
            agent_attention_text(failed_agents, cut_short_agents),
            leftovers_text(untracked_work, leftover_edits)
        );
        #[derive(Serialize)]
        struct Completed<'a> {
            event: &'static str,
            run_id: &'a str,
            outcome: &'a str,
            #[serde(skip_serializing_if = "is_unlisted")]
            cut_short_agents: &'a Value,
            #[serde(skip_serializing_if = "is_unlisted")]
            untracked_work: &'a Value,
            #[serde(skip_serializing_if = "is_unlisted")]
            leftover_edits: &'a Value,
            result: &'a Value,
            spend: &'a SpendView,
        }
        #[derive(Serialize)]
        struct CompletedWithFailures<'a> {
            event: &'static str,
            run_id: &'a str,
            outcome: &'a str,
            reason: &'a str,
            failed_agents: &'a Value,
            #[serde(skip_serializing_if = "is_unlisted")]
            cut_short_agents: &'a Value,
            #[serde(skip_serializing_if = "is_unlisted")]
            untracked_work: &'a Value,
            #[serde(skip_serializing_if = "is_unlisted")]
            leftover_edits: &'a Value,
            result: &'a Value,
            summary: &'a Value,
            spend: &'a SpendView,
        }
        #[derive(Serialize)]
        struct Failed<'a> {
            event: &'static str,
            run_id: &'a str,
            outcome: &'a str,
            error: &'a Value,
            #[serde(skip_serializing_if = "is_unlisted")]
            untracked_work: &'a Value,
            #[serde(skip_serializing_if = "is_unlisted")]
            leftover_edits: &'a Value,
            spend: &'a SpendView,
        }
        #[derive(Serialize)]
        struct Aborted<'a> {
            event: &'static str,
            run_id: &'a str,
            outcome: &'a str,
            reason: &'a str,
            #[serde(skip_serializing_if = "is_unlisted")]
            untracked_work: &'a Value,
            #[serde(skip_serializing_if = "is_unlisted")]
            leftover_edits: &'a Value,
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
                        cut_short_agents,
                        untracked_work,
                        leftover_edits,
                        result,
                        spend,
                    });
                } else {
                    self.text(&format!(
                        "{attention}Result:\n{}\n",
                        serde_json::to_string_pretty(result).unwrap_or_else(|_| "null".into())
                    ));
                }
            }
            "completed_with_failures" => {
                // The script returned, but at least one agent failed; the result is what the script made of that.
                let result = result.unwrap_or(&Value::Null);
                let reason = saved["reason"].as_str().unwrap_or("agents failed");
                if self.json {
                    self.json(&CompletedWithFailures {
                        event: "run_finished",
                        run_id,
                        outcome,
                        reason,
                        failed_agents,
                        cut_short_agents,
                        untracked_work,
                        leftover_edits,
                        result,
                        summary: &saved["summary"],
                        spend,
                    });
                } else {
                    self.text(&format!(
                        "Completed with failures: {}.\n{attention}Result:\n{}\n",
                        safe(reason),
                        serde_json::to_string_pretty(result).unwrap_or_else(|_| "null".into())
                    ));
                }
            }
            "budget_exhausted" => {
                // The script returned, but the budget refused or stopped some of its agents.
                #[derive(Serialize)]
                struct Exhausted<'a> {
                    event: &'static str,
                    run_id: &'a str,
                    outcome: &'a str,
                    #[serde(skip_serializing_if = "is_unlisted")]
                    failed_agents: &'a Value,
                    #[serde(skip_serializing_if = "is_unlisted")]
                    cut_short_agents: &'a Value,
                    #[serde(skip_serializing_if = "is_unlisted")]
                    untracked_work: &'a Value,
                    #[serde(skip_serializing_if = "is_unlisted")]
                    leftover_edits: &'a Value,
                    result: &'a Value,
                    reason: &'a str,
                    summary: &'a Value,
                    spend: &'a SpendView,
                }
                let result = result.unwrap_or(&Value::Null);
                let reason = saved["reason"].as_str().unwrap_or("budget exhausted");
                if self.json {
                    self.json(&Exhausted {
                        event: "run_finished",
                        run_id,
                        outcome,
                        failed_agents,
                        cut_short_agents,
                        untracked_work,
                        leftover_edits,
                        result,
                        reason,
                        summary: &saved["summary"],
                        spend,
                    });
                } else {
                    self.text(&format!(
                        "Budget exhausted: {}. Agents refused: {}, stopped: {}.\n{attention}Result:\n{}\n",
                        safe(reason),
                        saved["summary"]["refused"].as_u64().unwrap_or(0),
                        saved["summary"]["stopped"].as_u64().unwrap_or(0),
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
                        untracked_work,
                        leftover_edits,
                        spend,
                    });
                } else {
                    self.text(&format!(
                        "Failed: {}: {}\n{}",
                        safe(saved["error"]["name"].as_str().unwrap_or("Error")),
                        safe(
                            saved["error"]["message"]
                                .as_str()
                                .unwrap_or("workflow failed")
                        ),
                        leftovers_text(untracked_work, leftover_edits)
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
                        untracked_work,
                        leftover_edits,
                        spend,
                    });
                } else {
                    self.text(&format!(
                        "Aborted: {}\n{}",
                        safe(reason),
                        leftovers_text(untracked_work, leftover_edits)
                    ));
                }
            }
        }
        if self.json {
            let runs_dir = self.dir.as_deref().and_then(std::path::Path::parent);
            self.text(&format!(
                "{}\n",
                closing_line(run_id, outcome, spend, runs_dir)
            ));
        }
    }
}
/// The short plain-text line printed after the `run_finished` object in JSON mode. Hermes keeps the
/// last 2,000 characters of a background command's output and then cuts back to the first newline,
/// so a single very long final line reached the lead as nothing; this line is always short.
///
/// `runs_dir` is the folder holding the run's own folder. When it is not the default one, the
/// printed `resume` command names it, so the command works as printed.
fn closing_line(
    run_id: &str,
    outcome: &str,
    spend: &SpendView,
    runs_dir: Option<&std::path::Path>,
) -> String {
    let money = match spend.money_usd {
        Some(usd) => format!("${usd:.4} ({})", safe(&spend.money_source)),
        None => "unknown".to_owned(),
    };
    format!(
        "run {} {}: {}, money {}. Full result: talaria workflow resume --json{} {}",
        safe(run_id),
        safe(outcome),
        tokens_text(
            spend.tokens,
            spend.cache_read_tokens,
            spend.cache_unknown_turns,
            |n| n.to_string()
        ),
        money,
        custom_runs_dir_flag(runs_dir),
        safe(run_id)
    )
}
/// ` --runs-dir <path>` when `runs_dir` is not the default runs folder, else nothing.
fn custom_runs_dir_flag(runs_dir: Option<&std::path::Path>) -> String {
    let Some(used) = runs_dir.and_then(|dir| absolute(dir.to_path_buf()).ok()) else {
        return String::new();
    };
    // The run's folder is reported in its resolved form, so compare resolved forms.
    let resolved = |path: PathBuf| std::fs::canonicalize(&path).unwrap_or(path);
    let default = absolute(runner::runs_dir(None)).map(resolved);
    if default.is_ok_and(|default| default == resolved(used.clone())) {
        return String::new();
    }
    let text = safe(&used.to_string_lossy());
    if text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-+=:@%".contains(c))
    {
        format!(" --runs-dir {text}")
    } else {
        format!(" --runs-dir '{}'", text.replace('\'', "'\\''"))
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
        WorkflowCommand::Compile(args) => compile_command::execute(args).await,
        WorkflowCommand::Check(args) => crate::workflow_check::execute(args).await,
        WorkflowCommand::Worktrees(args) => crate::workflow_worktrees::execute(args),
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
    if let Some(warning) = outside_git_warning(&cwd) {
        eprintln!("{}", safe(&warning));
    }
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
        session_model: args.session_model.zip(args.session_provider),
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

#[cfg(test)]
mod leftover_text_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn leftovers_are_plain_lines_and_absent_when_empty() {
        assert_eq!(leftovers_text(&json!([]), &Value::Null), "");
        let text = leftovers_text(
            &json!(["agent a, session s, working folder /w: moved"]),
            &json!([
                {"path": "/w/one", "changed_files": 1},
                {"path": "/w/two", "changed_files": 3}
            ]),
        );
        assert_eq!(
            text,
            "Untracked work found on agent routes:\n  agent a, session s, working folder /w: moved\nUncommitted changes left in agent working folders:\n  /w/one: 1 changed file\n  /w/two: 3 changed files\n"
        );
    }
}
