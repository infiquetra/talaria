//! Which step a routed turn is on while the agent builds a workflow, read from the turn's
//! cells. Pure: the caller passes the cells after the routed prompt.
use crate::session::{Cell, PromptKind, PromptState, ToolCell, ToolStatus};
use serde_json::Value;

/// How many steps the block counts before the run dock takes over.
pub const STEPS: u8 = 5;

/// One step of building a workflow, in the order the agent takes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringStage {
    /// Reading the roster and the guide, deciding what to build.
    Planning,
    /// Still the first step, while the agent writes project files before any workflow script.
    ProjectFiles,
    /// Writing or changing the workflow script.
    Writing,
    /// Reading the written script back or running a check on it.
    Checking,
    /// A question is open: the approval to start, or an answer the plan needs.
    Waiting,
    /// The person answered the approval question an earlier turn ended with; the agent reads
    /// the answer.
    Answered,
    /// The run was started.
    Running,
}

/// What the block shows: the step, and the script once one is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringProgress {
    pub stage: AuthoringStage,
    /// The workflow script's file name.
    pub script: Option<String>,
}

impl AuthoringProgress {
    /// The step in plain words.
    pub fn label(&self) -> &'static str {
        match self.stage {
            AuthoringStage::Planning | AuthoringStage::ProjectFiles => "planning the workflow",
            AuthoringStage::Writing => "writing the workflow script",
            AuthoringStage::Checking => "checking the script",
            AuthoringStage::Waiting if self.script.is_some() => "waiting for your approval",
            AuthoringStage::Waiting => "waiting for your answer",
            AuthoringStage::Answered => "reading your answer",
            AuthoringStage::Running => "running the workflow",
        }
    }

    /// The step's number, from 1 to [`STEPS`].
    pub fn step(&self) -> u8 {
        match self.stage {
            AuthoringStage::Planning | AuthoringStage::ProjectFiles => 1,
            AuthoringStage::Writing => 2,
            AuthoringStage::Checking => 3,
            AuthoringStage::Waiting | AuthoringStage::Answered => 4,
            AuthoringStage::Running => 5,
        }
    }
}

/// What the screen draws for a routed turn: its progress and how long it has run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoringView {
    pub progress: AuthoringProgress,
    pub elapsed_ms: u64,
}

/// How the routed turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringEnd {
    Completed,
    Interrupted,
    Failed,
}

/// The run the turn started, when Talaria found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunFacts<'a> {
    pub name: &'a str,
    /// The run's status word: running, parked, completed, failed and so on.
    pub status: &'a str,
    pub agents: u32,
}

/// `12s` under a minute, `1m 05s` after.
pub fn elapsed_text(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms / 1_000;
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

/// The step the turn is on, from the cells after the routed prompt. Each tool call moves the
/// block to the step it belongs to; an open question shows while it waits.
pub fn progress(cells: &[Cell]) -> AuthoringProgress {
    resume(None, cells)
}

/// [`progress`] for a turn that answers an earlier turn's approval question: that turn wrote
/// `script` and ended without starting it, so this one starts at the answer, not at planning.
pub fn resume(script: Option<&str>, cells: &[Cell]) -> AuthoringProgress {
    let mut progress = AuthoringProgress {
        stage: if script.is_some() {
            AuthoringStage::Answered
        } else {
            AuthoringStage::Planning
        },
        script: script.map(str::to_owned),
    };
    let mut waiting = false;
    for cell in cells {
        match cell {
            Cell::Tool(tool) => {
                waiting = tool.name == "clarify" && tool.status == ToolStatus::Running;
                follow(&mut progress, tool);
            }
            Cell::Prompt(prompt) if prompt.kind == PromptKind::Clarify => {
                waiting = prompt.state == PromptState::Open;
            }
            _ => {}
        }
    }
    if waiting {
        progress.stage = AuthoringStage::Waiting;
    }
    progress
}

fn follow(progress: &mut AuthoringProgress, tool: &ToolCell) {
    let args = tool.args.as_ref();
    let text = |key: &str| args.and_then(|args| args.get(key)).and_then(Value::as_str);
    match tool.name.as_str() {
        "write_file" | "patch" => {
            let target = text("path").and_then(script_name).or_else(|| {
                text("patch").and_then(|patch| patch.split_whitespace().find_map(script_name))
            });
            if let Some(name) = target {
                progress.script = Some(name);
                progress.stage = AuthoringStage::Writing;
            } else if progress.stage == AuthoringStage::Planning
                && !text("path")
                    .or_else(|| text("patch"))
                    .unwrap_or_default()
                    .contains("/drafts/")
            {
                // A file outside the drafts folder, before any script: the project itself.
                progress.stage = AuthoringStage::ProjectFiles;
            }
        }
        "terminal" => {
            let command = text("command").unwrap_or_default();
            if command.contains("workflow run") || command.contains("workflow resume") {
                progress.stage = AuthoringStage::Running;
            } else if names_script(progress, command) {
                progress.stage = AuthoringStage::Checking;
            }
        }
        "read_file" if names_script(progress, text("path").unwrap_or_default()) => {
            progress.stage = AuthoringStage::Checking;
        }
        _ => {}
    }
}

/// True when `text` names the script written so far, while the run has not started.
fn names_script(progress: &AuthoringProgress, text: &str) -> bool {
    progress.stage != AuthoringStage::Running
        && progress
            .script
            .as_deref()
            .is_some_and(|script| text.contains(script))
}

/// The file name of a workflow script path: a `.workflow` file, or a script in a drafts folder.
fn script_name(path: &str) -> Option<String> {
    let path = path.trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
    let name = path.rsplit(['/', '\\']).next()?;
    let draft = path.contains("/drafts/") && name.ends_with(".js");
    (!name.is_empty() && (name.ends_with(".workflow") || draft)).then(|| name.to_owned())
}
