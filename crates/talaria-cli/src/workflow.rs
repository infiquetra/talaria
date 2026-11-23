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
    /// Answer an agent call a run parked because its outcome is unknown or because it was
    /// stopped: fail it, run it again, abort the run, or use the stopped agent's next reply.
    Resolve(ResolveArgs),
    /// Ask a finished run to run a failed agent again; `workflow resume` then runs it while
    /// finished agents stay kept.
    Restart(RestartArgs),
    /// Print a finished run's result, or with --agent the value one agent returned, read from
    /// the run's folder. It never starts or resumes anything.
    Result(ResultArgs),
    /// Turn a workflow's agent calls into kanban board cards. Kanban boards only; it does not
    /// validate a script, so use `check` for that.
    Compile(CompileArgs),
    /// Check a script for problems before a run: meta block, syntax, agent options and tier
    /// names. Starts no agent and does not contact the gateway.
    Check(crate::workflow_check::CheckArgs),
    /// List or remove the git worktrees and branches that runs keep for review.
    Worktrees(crate::workflow_worktrees::WorktreesArgs),
    /// Show the lessons earlier agents recorded about workflow runs, or record one.
    Lessons(crate::workflow_lessons::LessonsArgs),
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
    /// For an agent that was stopped: its session's next reply becomes its result.
    UseReply,
}
#[derive(Clone, Debug, Args)]
pub struct RestartArgs {
    #[arg(value_name = "RUN", value_parser = required_text)]
    pub target: String,
    /// The failed agent's label or key.
    #[arg(value_name = "AGENT", value_parser = required_text, required_unless_present = "all_failed", conflicts_with = "all_failed")]
    pub agent: Option<String>,
    /// Ask for every failed agent of the run.
    #[arg(long)]
    pub all_failed: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long, value_parser = required_path)]
    pub runs_dir: Option<PathBuf>,
}
#[derive(Clone, Debug, Args)]
pub struct ResultArgs {
    #[arg(value_name = "RUN", value_parser = required_text)]
    pub target: String,
    /// The agent's label or key. Without it, the run's own result is printed as a resume of the
    /// finished run prints it, without starting anything.
    #[arg(long, value_parser = required_text)]
    pub agent: Option<String>,
    #[arg(long)]
    pub json: bool,
    #[arg(long, value_parser = required_path)]
    pub runs_dir: Option<PathBuf>,
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
    let list = leftover.as_array().map(Vec::as_slice).unwrap_or_default();
    let (in_place, worktrees): (Vec<&Value>, Vec<&Value>) =
        list.iter().partition(|folder| folder["in_place"] == true);
    if !worktrees.is_empty() {
        text.push_str("Uncommitted changes left in agent working folders:\n");
        for folder in worktrees {
            let count = folder["changed_files"].as_u64().unwrap_or(0);
            let saved = folder["saved_branch"]
                .as_str()
                .map(|b| format!("; a copy is saved on branch {}", safe(b)))
                .unwrap_or_default();
            text.push_str(&format!(
                "  {}: {count} changed file{}{saved}\n",
                safe(folder["path"].as_str().unwrap_or("unknown folder")),
                if count == 1 { "" } else { "s" }
            ));
        }
    }
    if !in_place.is_empty() {
        text.push_str("Folders agents ran in without worktree isolation (anything they changed is already there; nothing was saved):\n");
        for folder in in_place {
            let agents: Vec<String> = folder["agents"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).map(safe).collect())
                .unwrap_or_default();
            text.push_str(&format!(
                "  {}: {}\n",
                safe(folder["path"].as_str().unwrap_or("unknown folder")),
                if agents.is_empty() {
                    "agents".to_owned()
                } else {
                    agents.join(", ")
                }
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
            "{}  {}  {}  agents {}/{}{}{}  {}  money {}  {}  {}\n",
            safe(&run.run_id),
            safe(&run.name),
            run.status.label(),
            run.agents_finished,
            run.agents_total_known,
            parked,
            attention_marks(&run.attention),
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
                cause,
                reason,
                choices,
                saved_branch,
            } => {
                let runs_dir = custom_runs_dir_flag(self.dir.as_deref().and_then(Path::parent));
                let answer = format!(
                    "talaria workflow resolve {}{runs_dir} --key {} --attempt {attempt} --episode {episode} --resolution",
                    safe(run_id),
                    safe(key)
                );
                let use_reply = choices.iter().any(|c| c == "use-reply");
                if self.json {
                    #[derive(Serialize)]
                    struct Parked<'a> {
                        event: &'static str,
                        run_id: &'a str,
                        label: &'a str,
                        key: &'a str,
                        attempt: u32,
                        episode: u32,
                        cause: &'a str,
                        reason: &'a str,
                        waiting_for: &'static str,
                        choices: &'a [String],
                        answer: String,
                        #[serde(skip_serializing_if = "Option::is_none")]
                        use_reply: Option<&'static str>,
                        #[serde(skip_serializing_if = "Option::is_none")]
                        saved_branch: Option<&'a str>,
                    }
                    self.json(&Parked {
                        event: "parked",
                        run_id,
                        label,
                        key,
                        attempt: *attempt,
                        episode: *episode,
                        cause,
                        reason,
                        waiting_for: "operator",
                        choices,
                        answer: format!("{answer} <choice>"),
                        use_reply: use_reply.then_some(USE_REPLY_HELP),
                        saved_branch: saved_branch.as_deref(),
                    });
                } else {
                    self.text(&format!(
                        "needs operator: run {} agent {} key {} attempt {attempt} episode {episode} - answer: {answer} {}\n",
                        safe(run_id),
                        safe(label),
                        safe(key),
                        choices.join("|")
                    ));
                    self.text(&format!(
                        "  agent {} is parked ({}): {}\n  the run waits for this answer and nothing ends the wait for you\n",
                        safe(label),
                        safe(cause),
                        safe(reason)
                    ));
                    if use_reply {
                        self.text(&format!("  use-reply: {USE_REPLY_HELP}\n"));
                    }
                    if let Some(branch) = saved_branch {
                        self.text(&format!(
                            "  its uncommitted work was saved to branch {}\n",
                            safe(branch)
                        ));
                    }
                }
            }
            RunnerEvent::WaitingForReply {
                run_id,
                label,
                key,
                session_id,
            } => {
                if self.json {
                    self.json(&waiting_for_reply_json(run_id, label, key, session_id));
                } else {
                    self.text(&waiting_for_reply_text(run_id, label, key, session_id));
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
            let attention = self
                .dir
                .as_deref()
                .map(runner::run_attention)
                .unwrap_or_default();
            self.text(&format!(
                "{}\n",
                closing_line(run_id, outcome, spend, runs_dir, &attention)
            ));
        }
    }
}
/// Why a run's money is unknown, in what the code that built the spend knows. A live run's money
/// is unknown when no finished turn had a cost from Hermes or could be priced from the price list
/// (a price needs Hermes's input and output token split). A replayed run's spend comes from the
/// journal, which keeps only the costs Hermes reported, so it says that instead. Neither suggests
/// that setting tiers would fix it.
fn unknown_money_text(spend: &SpendView) -> String {
    if spend.tokens == 0 && spend.cache_read_tokens == 0 {
        "unknown (no agent turn has finished)".to_owned()
    } else if spend.money_source == "unrecorded" {
        "unknown (the stored run keeps only costs Hermes reported, and at least one turn had none)"
            .to_owned()
    } else {
        "unknown (Hermes reported no cost and no turn could be priced from the price list)"
            .to_owned()
    }
}
#[cfg(test)]
mod money_text_tests {
    use super::*;

    fn spend(tokens: u64, money_usd: Option<f64>) -> SpendView {
        SpendView {
            tokens,
            cache_read_tokens: 0,
            cache_unknown_turns: 0,
            money_usd,
            money_source: "none".into(),
        }
    }

    #[test]
    fn unknown_money_names_the_price_list_not_the_tiers() {
        let line = closing_line(
            "r1",
            "completed",
            &spend(500, None),
            None,
            &Default::default(),
        );
        assert!(
            line.contains(
                "money unknown (Hermes reported no cost and no turn could be priced from the price list)"
            ),
            "{line}"
        );
        assert!(!line.contains("tier"), "{line}");
        let mut replayed = spend(500, None);
        replayed.money_source = "unrecorded".into();
        let replay = closing_line("r1", "completed", &replayed, None, &Default::default());
        assert!(
            replay.contains(
                "money unknown (the stored run keeps only costs Hermes reported, and at least one turn had none)"
            ),
            "{replay}"
        );
        assert!(!replay.contains("no price"), "{replay}");
        let idle = closing_line("r1", "failed", &spend(0, None), None, &Default::default());
        assert!(
            idle.contains("money unknown (no agent turn has finished)"),
            "{idle}"
        );
        let known = closing_line(
            "r1",
            "completed",
            &spend(500, Some(0.5)),
            None,
            &Default::default(),
        );
        assert!(known.contains("money $0.5000 (none)"), "{known}");
    }
}
/// What `use-reply` does, printed with a parked stopped agent.
const USE_REPLY_HELP: &str = "continue the agent's Hermes session (for example type \"continue\" into it from a Talaria window); its next reply becomes the agent's result and is checked like any reply. A reply that came after the stop while a runner had the run open is used at once; one that came while no runner had it open cannot be seen, so type it again after answering.";
/// How to give the reply a use-reply answer waits for.
const WAITING_FOR_REPLY_HELP: &str = "continue the agent's Hermes session (for example type \"continue\" into it from a Talaria window); its reply becomes the agent's result. Nothing else ends this wait; talaria workflow stop ends the run.";
/// The JSON line for a use-reply answer that now waits for the agent's next reply.
fn waiting_for_reply_json(run_id: &str, label: &str, key: &str, session_id: &str) -> Value {
    json!({
        "event": "waiting_for_reply",
        "run_id": run_id,
        "label": label,
        "key": key,
        "session_id": session_id,
        "waiting_for": "operator",
        "how": WAITING_FOR_REPLY_HELP,
    })
}
/// The text for a use-reply answer that now waits for the agent's next reply.
fn waiting_for_reply_text(run_id: &str, label: &str, key: &str, session_id: &str) -> String {
    format!(
        "waiting for a reply: run {} agent {} key {} waits for the next reply on Hermes session {}\n  {WAITING_FOR_REPLY_HELP}\n",
        safe(run_id),
        safe(label),
        safe(key),
        safe(session_id)
    )
}
/// The short plain-text line printed after the `run_finished` object in JSON mode. Hermes keeps the
/// last 2,000 characters of a background command's output and then cuts back to the first newline,
/// so a single very long final line reached the lead as nothing; this line is always short (under
/// about 600 characters plus a custom runs folder's path), so it arrives whole.
///
/// `runs_dir` is the folder holding the run's own folder. When it is not the default one, the
/// printed `result` command names it, so the command works as printed.
fn closing_line(
    run_id: &str,
    outcome: &str,
    spend: &SpendView,
    runs_dir: Option<&std::path::Path>,
    attention: &runner::RunAttention,
) -> String {
    let money = match spend.money_usd {
        Some(usd) => format!("${usd:.4} ({})", short(&spend.money_source, 40)),
        None => unknown_money_text(spend),
    };
    format!(
        "run {} {}: {}, money {}.{}{} Full result: talaria workflow result --json{} {}",
        safe(run_id),
        safe(outcome),
        tokens_text(
            spend.tokens,
            spend.cache_read_tokens,
            spend.cache_unknown_turns,
            |n| n.to_string()
        ),
        money,
        script_sentence(attention),
        attention_sentence(attention),
        custom_runs_dir_flag(runs_dir),
        safe(run_id)
    )
}
/// The script's own `status` word, made safe and short for one line.
fn script_status(status: &str) -> String {
    short(status, 30)
}
/// `text` made safe, cut to `limit` characters with `...` when longer.
fn short(text: &str, limit: usize) -> String {
    let text = safe(text);
    if text.chars().count() > limit {
        format!("{}...", text.chars().take(limit).collect::<String>())
    } else {
        text
    }
}
/// How many phases that did not start are named on one line; the rest are counted.
const PHASES_NAMED: usize = 3;
/// The phases that did not start, named (the first few, each cut short), or counted when the
/// names are not known.
fn phases_not_started(attention: &runner::RunAttention) -> Option<String> {
    let n = attention.phases_not_started;
    if n == 0 {
        return None;
    }
    let names = &attention.phases_not_started_names;
    if names.is_empty() {
        return Some(format!(
            "{n} phase{} not started",
            if n == 1 { "" } else { "s" }
        ));
    }
    let mut listed: Vec<String> = names
        .iter()
        .take(PHASES_NAMED)
        .map(|name| short(name, 20))
        .collect();
    let rest = (n as usize).saturating_sub(listed.len());
    if rest > 0 {
        listed.push(format!("and {rest} more"));
    }
    Some(format!("phases not started: {}", listed.join(", ")))
}
/// What the script itself reported, stated neutrally: its own `status` word and the declared
/// phases that did not start. Neither is a problem by itself (a review with no findings can
/// end after its first phase), so they stay out of the attention sentence.
fn script_sentence(attention: &runner::RunAttention) -> String {
    let mut parts = Vec::new();
    if let Some(status) = &attention.script_status {
        parts.push(format!("Script status \"{}\"", script_status(status)));
    }
    if let Some(phases) = phases_not_started(attention) {
        parts.push(if parts.is_empty() {
            let mut chars = phases.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        } else {
            phases
        });
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}.", parts.join("; "))
    }
}
/// The problems a finished run left, as one short sentence for the closing line, or nothing:
/// agents that failed, were cut short or left uncommitted edits. The most important come last,
/// nearest the end a reader that keeps only the tail still sees.
fn attention_sentence(attention: &runner::RunAttention) -> String {
    let agents = |n: u32| format!("{n} agent{}", if n == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    let saved = match attention.saved_branches {
        0 => String::new(),
        n => format!(" ({n} saved to a branch)"),
    };
    if attention.uncommitted_edits > 0 {
        parts.push(format!(
            "{} left uncommitted edits{saved}",
            agents(attention.uncommitted_edits)
        ));
    } else if !saved.is_empty() {
        parts.push(format!("uncommitted work{saved}"));
    }
    if attention.cut_short_agents > 0 {
        parts.push(format!("{} cut short", agents(attention.cut_short_agents)));
    }
    if attention.failed_agents > 0 {
        parts.push(format!("{} failed", agents(attention.failed_agents)));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" Attention: {}.", parts.join("; "))
    }
}
/// Compact marks for a `workflow list` text row, or nothing when there is nothing to say: the
/// problem counts first, then what the script itself reported, stated neutrally.
fn attention_marks(attention: &runner::RunAttention) -> String {
    let mut marks = String::new();
    for (count, word) in [
        (attention.failed_agents, "failed"),
        (attention.cut_short_agents, "cut short"),
        (attention.uncommitted_edits, "uncommitted edits"),
        (attention.saved_branches, "saved branches"),
    ] {
        if count > 0 {
            marks.push_str(&format!("  {word} {count}"));
        }
    }
    if let Some(status) = &attention.script_status {
        marks.push_str(&format!("  script status \"{}\"", script_status(status)));
    }
    if let Some(phases) = phases_not_started(attention) {
        marks.push_str(&format!("  {phases}"));
    }
    marks
}
/// The commit and tree state this binary was built from, as the build script recorded them.
fn build_stamp() -> talaria_workflow::journal::BuildStamp {
    talaria_workflow::journal::BuildStamp {
        talaria_commit: env!("TALARIA_BUILD_COMMIT").into(),
        talaria_tree: env!("TALARIA_BUILD_TREE").into(),
    }
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
                ResolutionArg::UseReply => Resolution::UseReply,
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
                Err(e @ runner::AnswerError::NotStopped { .. }) => fail(e, 2),
                Err(e) => fail(e, 1),
            }
        }
        WorkflowCommand::Restart(args) => restart_command(args),
        WorkflowCommand::Result(args) => result_command(args),
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
        WorkflowCommand::Lessons(args) => crate::workflow_lessons::execute(args),
    }
}
fn agent_error(e: runner::agents::AgentError) -> u8 {
    use runner::agents::AgentError;
    let code = match e {
        AgentError::Journal(_) | AgentError::Io(_) => 1,
        _ => 2,
    };
    fail(e, code)
}
fn print(text: &str) -> u8 {
    match io::stdout().write_all(text.as_bytes()) {
        Ok(()) => 0,
        Err(e) => fail(e, 1),
    }
}
fn restart_command(args: RestartArgs) -> u8 {
    use runner::agents::{RestartTarget, request_restart};
    let runs = runner::runs_dir(args.runs_dir.as_deref());
    let which = match args.agent {
        Some(agent) => RestartTarget::Agent(agent),
        None => RestartTarget::AllFailed,
    };
    let report = match request_restart(&runs, &args.target, &which) {
        Ok(report) => report,
        Err(e) => return agent_error(e),
    };
    let flag = custom_runs_dir_flag(absolute(runs).ok().as_deref());
    let next = format!("talaria workflow resume{flag} {}", safe(&report.run_id));
    if args.json {
        let line = json!({
            "event": "restart_requested",
            "run_id": report.run_id,
            "requested": report.requested,
            "also_run_again": report.also_run_again,
            "next": next,
        });
        return print(&format!("{line}\n"));
    }
    let mut text = String::new();
    for agent in &report.requested {
        text.push_str(&format!(
            "{} for agent {} (key {})\n",
            if agent.written {
                "restart requested"
            } else {
                "restart already requested"
            },
            safe(&agent.label),
            safe(&agent.key)
        ));
    }
    if !report.also_run_again.is_empty() {
        text.push_str(&format!(
            "note: the resume also runs these failed agents again when the script reaches them: {}\n",
            report
                .also_run_again
                .iter()
                .map(|l| safe(l))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    text.push_str(&format!("finished agents stay kept. Next: {next}\n"));
    print(&text)
}
fn result_command(args: ResultArgs) -> u8 {
    let runs = runner::runs_dir(args.runs_dir.as_deref());
    let Some(agent) = &args.agent else {
        // The run's result, printed by the same renderer a resume replay uses.
        let mut renderer = Renderer::new(args.json);
        let exit = runner::replay_result(&runs, &args.target, &mut |event| renderer.emit(event));
        return if renderer.failed { 1 } else { exit_code(exit) };
    };
    let result = match runner::agents::agent_result(&runs, &args.target, agent) {
        Ok(result) => result,
        Err(e) => return agent_error(e),
    };
    let kept = result.value.is_some();
    let code = if kept { 0 } else { 1 };
    if args.json {
        let mut line = serde_json::to_value(&result).unwrap_or(Value::Null);
        if let Some(object) = line.as_object_mut() {
            object.insert("event".into(), json!("agent_result"));
        }
        return match print(&format!("{line}\n")) {
            0 => code,
            failed => failed,
        };
    }
    let text = match &result.value {
        Some(value) => format!(
            "{}{}\n",
            result
                .cut_short
                .as_ref()
                .map(|c| format!(
                    "agent {}: {}; its reply was kept\n",
                    safe(&result.label),
                    safe(c)
                ))
                .unwrap_or_default(),
            serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".into())
        ),
        None => format!(
            "agent {} is {} and has no kept value{}\n",
            safe(&result.label),
            result.state,
            result
                .reason
                .as_ref()
                .map(|r| format!(": {}", safe(r)))
                .unwrap_or_default()
        ),
    };
    match print(&text) {
        0 => code,
        failed => failed,
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
        build: build_stamp(),
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
    fn the_build_stamp_comes_from_the_build_script() {
        let stamp = build_stamp();
        assert!(!stamp.talaria_commit.is_empty());
        assert!(
            ["clean", "modified", "unknown"].contains(&stamp.talaria_tree.as_str()),
            "{stamp:?}"
        );
        // A commit is known exactly when the tree state is.
        assert_eq!(
            stamp.talaria_commit == "unknown",
            stamp.talaria_tree == "unknown"
        );
    }

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

    #[test]
    fn the_closing_line_puts_problems_before_the_hint_and_adds_nothing_without_them() {
        let spend = SpendView {
            tokens: 10,
            cache_read_tokens: 0,
            cache_unknown_turns: 0,
            money_usd: None,
            money_source: "unknown".into(),
        };
        let quiet = closing_line("r1", "completed", &spend, None, &Default::default());
        assert!(!quiet.contains("Attention"), "{quiet}");
        assert!(!quiet.contains("Script status"), "{quiet}");
        assert!(
            quiet.ends_with(" Full result: talaria workflow result --json r1"),
            "{quiet}"
        );
        let attention = runner::RunAttention {
            failed_agents: 2,
            cut_short_agents: 1,
            uncommitted_edits: 1,
            saved_branches: 1,
            phases_not_started: 2,
            phases_not_started_names: vec!["Verify".into(), "Synthesize".into()],
            script_status: Some("partial".into()),
        };
        let line = closing_line("r1", "completed", &spend, None, &attention);
        let hint = line.find(" Full result: ").unwrap();
        let tail = &line[..hint];
        assert!(
            tail.ends_with(
                " Script status \"partial\"; phases not started: Verify, Synthesize. Attention: 1 agent left uncommitted edits (1 saved to a branch); 1 agent cut short; 2 agents failed."
            ),
            "{line}"
        );
        assert_eq!(
            &line[hint..],
            " Full result: talaria workflow result --json r1",
            "{line}"
        );
        // The outcome word itself is unchanged.
        assert!(line.starts_with("run r1 completed: "), "{line}");
        let marks = attention_marks(&attention);
        assert_eq!(
            marks,
            "  failed 2  cut short 1  uncommitted edits 1  saved branches 1  script status \"partial\"  phases not started: Verify, Synthesize"
        );
        assert_eq!(attention_marks(&Default::default()), "");
    }

    #[test]
    fn a_clean_run_that_ended_early_by_its_own_choice_shows_no_attention() {
        // A review with no findings returns "reviewed" after its first phase.
        let spend = SpendView {
            tokens: 10,
            cache_read_tokens: 0,
            cache_unknown_turns: 0,
            money_usd: Some(0.01),
            money_source: "estimated".into(),
        };
        let attention = runner::RunAttention {
            phases_not_started: 2,
            phases_not_started_names: vec!["Verify".into(), "Synthesize".into()],
            script_status: Some("reviewed".into()),
            ..Default::default()
        };
        let line = closing_line("r1", "completed", &spend, None, &attention);
        assert!(!line.contains("Attention"), "{line}");
        assert!(
            line.contains(
                ". Script status \"reviewed\"; phases not started: Verify, Synthesize. Full result: "
            ),
            "{line}"
        );
        // Phases alone, many of them: the first few are named and the rest counted.
        let many = runner::RunAttention {
            phases_not_started: 5,
            phases_not_started_names: ["A", "B", "C", "D", "E"].map(String::from).to_vec(),
            ..Default::default()
        };
        assert!(
            closing_line("r1", "completed", &spend, None, &many)
                .contains(". Phases not started: A, B, C, and 2 more. Full result: "),
            "{many:?}"
        );
        // A count without names still says how many.
        let counted = runner::RunAttention {
            phases_not_started: 1,
            ..Default::default()
        };
        assert!(attention_marks(&counted).contains("1 phase not started"));
    }

    #[test]
    fn the_closing_line_stays_short_in_the_worst_case() {
        // Every text field at its longest (a long status, many long phase names, a long money
        // source, the longest money and token wording), with `big` tokens and `count` agents.
        let line = |big: u64, count: u32, money_usd: Option<f64>| {
            let spend = SpendView {
                tokens: big,
                cache_read_tokens: big,
                cache_unknown_turns: count,
                money_usd,
                money_source: "x".repeat(500),
            };
            let attention = runner::RunAttention {
                failed_agents: count,
                cut_short_agents: count,
                uncommitted_edits: count,
                saved_branches: count,
                phases_not_started: count.max(50),
                phases_not_started_names: (0..50)
                    .map(|n| format!("{n}{}", "p".repeat(200)))
                    .collect(),
                script_status: Some("s".repeat(500)),
            };
            closing_line(
                &"0".repeat(26),
                "completed_with_failures",
                &spend,
                None,
                &attention,
            )
        };
        // Realistic extremes: a trillion tokens and up to 999,999 agents or turns.
        for money in [None, Some(999_999.0)] {
            let realistic = line(999_999_999_999, 999_999, money);
            assert!(
                realistic.chars().count() < 600,
                "{} {realistic}",
                realistic.chars().count()
            );
        }
        // Every count at its type's maximum still leaves the line far below what Hermes keeps.
        let extreme = line(u64::MAX, u32::MAX, None);
        assert!(
            extreme.chars().count() < 640,
            "{} {extreme}",
            extreme.chars().count()
        );
        assert!(!extreme.contains('\n'), "{extreme}");
        assert!(
            extreme.ends_with(
                " Full result: talaria workflow result --json 00000000000000000000000000"
            )
        );
    }

    #[test]
    fn leftovers_name_the_saved_branch_and_say_plainly_when_nothing_is_missing() {
        let text = leftovers_text(
            &Value::Null,
            &json!([
                {"path": "/w/one", "changed_files": 2, "saved_branch": "talaria/wf/r/c-0-saved"},
                {"path": "/project", "changed_files": 0, "in_place": true,
                 "agents": ["writer", "checker"]}
            ]),
        );
        assert!(
            text.contains(
                "  /w/one: 2 changed files; a copy is saved on branch talaria/wf/r/c-0-saved\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("Folders agents ran in without worktree isolation (anything they changed is already there; nothing was saved):\n  /project: writer, checker\n"),
            "{text}"
        );
        assert!(!text.contains("edited"), "{text}");
        assert!(!text.contains("/project: 0 changed"), "{text}");
    }
}

#[cfg(test)]
mod waiting_for_reply_tests {
    use super::*;

    #[test]
    fn a_use_reply_wait_is_printed_as_waiting_for_the_operator_with_how_to_end_it() {
        let line = waiting_for_reply_json("RUN", "review", "KEY", "session-1");
        assert_eq!(line["event"], "waiting_for_reply");
        assert_eq!(line["waiting_for"], "operator");
        assert_eq!(
            (
                &line["run_id"],
                &line["label"],
                &line["key"],
                &line["session_id"]
            ),
            (
                &json!("RUN"),
                &json!("review"),
                &json!("KEY"),
                &json!("session-1")
            )
        );
        assert!(line["how"].as_str().unwrap().contains("continue"), "{line}");
        let text = waiting_for_reply_text("RUN", "review", "KEY", "session-1");
        assert!(
            text.starts_with(
                "waiting for a reply: run RUN agent review key KEY waits for the next reply on Hermes session session-1\n"
            ),
            "{text}"
        );
        assert!(text.contains("Nothing else ends this wait"), "{text}");
    }
}
