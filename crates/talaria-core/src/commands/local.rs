//! Commands Talaria handles itself. Issue #223. Later units append entries.
//!
//! A local command wins over a Hermes command of the same name: it is listed first, tagged
//! `talaria`, and running it sends nothing to Hermes.

use crate::connectors::{ConnectorsOpen, ConnectorsTab};

/// What a local command does.
///
/// Later units add one variant per command they own, so callers outside this crate keep a
/// fallback arm.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalAction {
    /// Open the gateway health check.
    Doctor,
    Providers(crate::providers::OpenTarget),
    /// Leave Talaria. The Hermes session keeps running.
    Quit,
    /// Choose the model of the foreground session.
    Model,
    /// Choose the reasoning level of the foreground session.
    Effort,
    /// Open the rewind overlay. Issue #233.
    Rewind,
    /// Remove the last N prompts. Issue #233.
    Undo,
    /// Remove the last prompt and send it again. Issue #233.
    Retry,
    /// List, compare, or restore file checkpoints. Issue #233.
    Rollback,
    /// Show the session diff. Issue #233.
    Diff,
    /// Show what fills the context window.
    Context,
    /// Compact the conversation to free the window.
    Compact,
    /// Show the session's tokens, calls and cost.
    Usage,
    /// Ask a side question with `prompt.btw`. Never reaches `slash.exec`.
    Btw,
    /// Show or change the Hermes skin.
    Skin,
    /// Show the colour decisions or override the background for this session.
    Theme,
    /// Show or change the approval tiers. Issue #237.
    Permissions,
    /// Show or move the session's working folder. Issue #237.
    Cd,
    /// Select, or queue a prompt with, the gateway's queue. Issue #232.
    Queue,
    /// Steer the running turn with typed text. Issue #232.
    Steer,
    /// Open the session picker, or switch to one session.
    Resume,
    /// Rename the current session.
    Rename,
    /// Branch the current session and switch into the child.
    Branch,
    /// Export a session to a file.
    Export,
    /// Open the fleet overlay.
    Fleet,
    /// Open the subagent panel, or pause and resume spawning. Issue #242.
    Agents,
    /// Open the background-process view.
    Processes,
    /// Run a prompt on a fresh agent in the background.
    Bg,
    /// Stop the running turn and this session's background processes.
    Stop,
    /// Review the foreground session's folder. Issue #245.
    Review,
    /// Open the settings screen.
    Config,
    /// Open the profiles overlay. Issue #253.
    Profiles,
    /// Open the profile's model roles page.
    ModelRoles,
    /// Open the MCP servers screen.
    Mcp(crate::mcp::McpOpen),
    /// Browse skills, toolsets and plugins.
    Capabilities,
    /// Open memory and curator settings.
    MemorySettings,
    /// Manage this profile's scheduled jobs.
    Cron,
    /// Open the connectors and messaging overlay.
    Connectors(ConnectorsOpen),
    /// Propose configuration changes for review.
    Configure,
}

/// One local command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalCommand {
    /// Name without a slash, lower-case.
    pub name: &'static str,
    /// Aliases without a slash, lower-case.
    pub aliases: &'static [&'static str],
    /// Row description.
    pub summary: &'static str,
    /// Argument hint. Empty when the command takes none.
    pub args_hint: &'static str,
    /// Whether an argument is accepted.
    pub takes_argument: bool,
    /// The action to run.
    pub action: LocalAction,
    /// The tag the palette shows beside the command.
    pub tag: &'static str,
}

impl LocalCommand {
    /// True when `name` (lower-case, no slash) is this command's name or one of its aliases.
    #[must_use]
    pub fn answers_to(&self, name: &str) -> bool {
        self.name == name || self.aliases.contains(&name)
    }
}

/// The one table, in the order the palette lists it.
const BUILTIN: &[LocalCommand] = &[
    LocalCommand {
        name: "quit",
        aliases: &["exit"],
        summary: "Leave Talaria; the session keeps running in Hermes",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Quit,
        tag: "talaria",
    },
    LocalCommand {
        name: "btw",
        aliases: &[],
        summary: "Ask a side question; it is not added to the conversation",
        args_hint: "<question>",
        takes_argument: true,
        action: LocalAction::Btw,
        tag: "talaria",
    },
    LocalCommand {
        name: "skin",
        aliases: &[],
        summary: "Show or change the Hermes skin",
        args_hint: "[name]",
        takes_argument: true,
        action: LocalAction::Skin,
        tag: "talaria",
    },
    LocalCommand {
        name: "theme",
        aliases: &[],
        summary: "Show colours, or set the background to light, dark or auto",
        args_hint: "[light|dark|auto]",
        takes_argument: true,
        action: LocalAction::Theme,
        tag: "talaria",
    },
    LocalCommand {
        name: "model",
        aliases: &[],
        summary: "Choose the model of this session",
        args_hint: "[<model> [--provider <id>] [--effort <level>] [--default]]",
        takes_argument: true,
        action: LocalAction::Model,
        tag: "talaria",
    },
    LocalCommand {
        name: "effort",
        aliases: &[],
        summary: "Choose the reasoning level of this session",
        args_hint: "[<level>|default] [--default]",
        takes_argument: true,
        action: LocalAction::Effort,
        tag: "talaria",
    },
    LocalCommand {
        name: "context",
        aliases: &[],
        summary: "Show what fills the model's context window",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Context,
        tag: "talaria",
    },
    LocalCommand {
        name: "compact",
        aliases: &["compress"],
        summary: "Summarize earlier conversation to free context",
        args_hint: "[focus] [--preview] [here N]",
        takes_argument: true,
        action: LocalAction::Compact,
        tag: "talaria",
    },
    LocalCommand {
        name: "usage",
        aliases: &["cost", "stats"],
        summary: "Show this session's tokens, calls and cost",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Usage,
        tag: "talaria",
    },
    LocalCommand {
        name: "permissions",
        tag: "talaria",
        aliases: &[],
        summary: "Show or change the approval tiers",
        args_hint: "[manual | smart | off | session off | session default]",
        takes_argument: true,
        action: LocalAction::Permissions,
    },
    LocalCommand {
        name: "cd",
        tag: "talaria",
        aliases: &[],
        summary: "Show or move the session's working folder",
        args_hint: "[<path>]",
        takes_argument: true,
        action: LocalAction::Cd,
    },
    LocalCommand {
        name: "queue",
        aliases: &["q"],
        summary: "Select a queued prompt, or queue one while Hermes works",
        args_hint: "[text]",
        takes_argument: true,
        action: LocalAction::Queue,
        tag: "talaria",
    },
    LocalCommand {
        name: "steer",
        aliases: &["s"],
        summary: "Send a note the running turn reads after its next tool call",
        args_hint: "<text>",
        takes_argument: true,
        action: LocalAction::Steer,
        tag: "talaria",
    },
    LocalCommand {
        name: "resume",
        aliases: &[],
        summary: "Open the session picker, or switch to one session",
        args_hint: "[<session id>]",
        takes_argument: true,
        action: LocalAction::Resume,
        tag: "talaria",
    },
    LocalCommand {
        name: "rename",
        aliases: &[],
        summary: "Rename the current session",
        args_hint: "[<title>]",
        takes_argument: true,
        action: LocalAction::Rename,
        tag: "talaria",
    },
    LocalCommand {
        name: "branch",
        aliases: &[],
        summary: "Branch the current session and switch into the child",
        args_hint: "[<title>]",
        takes_argument: true,
        action: LocalAction::Branch,
        tag: "talaria",
    },
    LocalCommand {
        name: "export",
        aliases: &[],
        summary: "Export a session to a file",
        args_hint: "[--no-redact] [file]",
        takes_argument: true,
        action: LocalAction::Export,
        tag: "talaria",
    },
    LocalCommand {
        name: "agents",
        aliases: &["tasks"],
        summary: "Show the session's subagents; `pause` and `resume` switch spawning",
        args_hint: "[pause|resume]",
        takes_argument: true,
        action: LocalAction::Agents,
        tag: "talaria",
    },
    LocalCommand {
        name: "changes",
        aliases: &[],
        summary: "Review the foreground session's changed files",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Review,
        tag: "talaria",
    },
    LocalCommand {
        name: "config",
        aliases: &[],
        summary: "Show and change Hermes settings for this profile",
        args_hint: "[filter | key=value …]",
        takes_argument: true,
        action: LocalAction::Config,
        tag: "talaria",
    },
    LocalCommand {
        name: "doctor",
        aliases: &[],
        summary: "Check the link to the Hermes gateway",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Doctor,
        tag: "talaria",
    },
    LocalCommand {
        name: "profiles",
        aliases: &[],
        summary: "List Hermes profiles, and why one is not served",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Profiles,
        tag: "talaria",
    },
    LocalCommand {
        name: "providers",
        aliases: &[],
        summary: "Set up provider keys, accounts and endpoints",
        args_hint: "[keys|accounts|endpoints]",
        takes_argument: true,
        action: LocalAction::Providers(crate::providers::OpenTarget::Tab(
            crate::providers::ProvidersTab::Keys,
        )),
        tag: "talaria",
    },
    LocalCommand {
        name: "models",
        aliases: &[],
        summary: "Show and change the profile's model roles",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::ModelRoles,
        tag: "talaria",
    },
    LocalCommand {
        name: "mcp",
        aliases: &[],
        summary: "Manage MCP servers for this profile",
        args_hint: "[servers | catalogue | apply]",
        takes_argument: true,
        action: LocalAction::Mcp(crate::mcp::McpOpen::Tab(crate::mcp::McpTab::Servers)),
        tag: "talaria",
    },
    LocalCommand {
        name: "capabilities",
        aliases: &[],
        summary: "Browse skills, toolsets and plugins for this profile",
        args_hint: "[skills|hub|toolsets|plugins]",
        takes_argument: true,
        action: LocalAction::Capabilities,
        tag: "talaria",
    },
    LocalCommand {
        name: "memory-settings",
        aliases: &[],
        summary: "Memory provider, built-in memory and the skill curator",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::MemorySettings,
        tag: "talaria",
    },
    LocalCommand {
        name: "cron",
        aliases: &[],
        summary: "Manage scheduled jobs for this profile",
        args_hint: "[list|new]",
        takes_argument: true,
        action: LocalAction::Cron,
        tag: "talaria",
    },
    LocalCommand {
        name: "connectors",
        aliases: &[],
        summary: "Show connector accounts and messaging platforms",
        args_hint: "",
        takes_argument: true,
        action: LocalAction::Connectors(ConnectorsOpen::Tab(ConnectorsTab::Connectors)),
        tag: "talaria",
    },
    LocalCommand {
        name: "messaging",
        aliases: &[],
        summary: "Show messaging platforms and connector accounts",
        args_hint: "",
        takes_argument: true,
        action: LocalAction::Connectors(ConnectorsOpen::Tab(ConnectorsTab::Messaging)),
        tag: "talaria",
    },
    LocalCommand {
        name: "configure",
        aliases: &[],
        summary: "Describe configuration changes, review them, then apply",
        args_hint: "<what you want to change>",
        takes_argument: true,
        action: LocalAction::Configure,
        tag: "talaria",
    },
];

/// The five rewind commands, in table order. Issue #233.
pub const LOCAL_COMMANDS: &[LocalCommand] = &[
    LocalCommand {
        name: "rewind",
        aliases: &["checkpoint"],
        summary: "Go back to an earlier prompt: rewind the conversation, and restore files when checkpoints are on",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Rewind,
        tag: "talaria",
    },
    LocalCommand {
        name: "undo",
        aliases: &[],
        summary: "Remove your last N prompts and their replies (default 1)",
        args_hint: "[N]",
        takes_argument: true,
        action: LocalAction::Undo,
        tag: "talaria",
    },
    LocalCommand {
        name: "retry",
        aliases: &[],
        summary: "Remove the last prompt and its reply, then send the prompt again",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Retry,
        tag: "talaria",
    },
    LocalCommand {
        name: "rollback",
        aliases: &[],
        summary: "List, compare or restore file checkpoints",
        args_hint: "[list] | diff <n> | <n> [file] [--all]",
        takes_argument: true,
        action: LocalAction::Rollback,
        tag: "talaria",
    },
    LocalCommand {
        name: "diff",
        aliases: &[],
        summary: "Show what changed since Hermes's oldest kept checkpoint",
        args_hint: "[--stat]",
        takes_argument: true,
        action: LocalAction::Diff,
        tag: "talaria",
    },
    LocalCommand {
        name: "fleet",
        aliases: &[],
        summary: "Show every live session and peek, reply, stop or open one",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Fleet,
        tag: "talaria",
    },
];

/// Background-process commands, after the rewind commands.
const BACKGROUND_COMMANDS: &[LocalCommand] = &[
    LocalCommand {
        name: "processes",
        aliases: &[],
        summary: "Show this session's background processes, with live output and kill",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Processes,
        tag: "talaria",
    },
    LocalCommand {
        name: "bg",
        aliases: &["background"],
        summary: "Run a prompt on a fresh agent in the background; the answer appears here",
        args_hint: "<prompt>",
        takes_argument: true,
        action: LocalAction::Bg,
        tag: "talaria",
    },
    LocalCommand {
        name: "stop",
        aliases: &[],
        summary: "Stop the running turn and this session's background processes",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Stop,
        tag: "talaria",
    },
];

/// The ordered local table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalCommandTable {
    commands: Vec<LocalCommand>,
}

impl LocalCommandTable {
    /// The built-in table, in the order the palette lists it.
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            commands: BUILTIN
                .iter()
                .chain(LOCAL_COMMANDS)
                .chain(BACKGROUND_COMMANDS)
                .cloned()
                .collect(),
        }
    }

    /// Find by name or alias. The slash is optional. Case does not matter.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&LocalCommand> {
        let key = name.strip_prefix('/').unwrap_or(name).to_lowercase();
        self.commands
            .iter()
            .find(|command| command.answers_to(&key))
    }

    /// Entries in table order.
    pub fn iter(&self) -> impl Iterator<Item = &LocalCommand> {
        self.commands.iter()
    }
}

/// Find a local command by name or alias, without the leading slash.
#[must_use]
pub fn lookup(name: &str) -> Option<&'static LocalCommand> {
    let key = name.strip_prefix('/').unwrap_or(name).to_lowercase();
    BUILTIN
        .iter()
        .chain(BACKGROUND_COMMANDS)
        .find(|command| command.answers_to(&key))
}

/// Parse a typed line into its action and argument text.
///
/// A command that takes no argument refuses one: the refusal reads
/// `<command> does not take an argument.`
pub fn parse(line: &str) -> Result<(LocalAction, String), String> {
    let trimmed = line.trim();
    let (name, argument) = match trimmed.split_once(char::is_whitespace) {
        Some((name, argument)) => (name, argument.trim()),
        None => (trimmed, ""),
    };
    let name = name.strip_prefix('/').unwrap_or(name);
    let Some(command) = lookup(name) else {
        return Err(format!("/{name} is not a Talaria command."));
    };
    if !command.takes_argument && !argument.is_empty() {
        // The review's command keeps the wording its own issue settled.
        if command.action == LocalAction::Review {
            return Err(format!("/{} takes no arguments.", command.name));
        }
        if command.action == LocalAction::ModelRoles {
            return Err("Usage: /models".to_owned());
        }
        return Err(format!("/{} does not take an argument.", command.name));
    }
    let action = if command.name == "providers" {
        LocalAction::Providers(
            crate::providers::parse_providers_args(argument).map_err(str::to_owned)?,
        )
    } else {
        command.action.clone()
    };
    Ok((action, argument.to_owned()))
}
