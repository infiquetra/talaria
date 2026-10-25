//! Commands Talaria handles itself. Issue #223. Later units append entries.
//!
//! A local command wins over a Hermes command of the same name: it is listed first, tagged
//! `talaria`, and running it sends nothing to Hermes.

/// What a local command does.
///
/// Later units add one variant per command they own, so callers outside this crate keep a
/// fallback arm.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalAction {
    /// Leave Talaria. The Hermes session keeps running.
    Quit,
    /// Collapse or expand the agent's task checklist.
    Tasks,
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
}

/// One local command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
        name: "tasks",
        tag: "talaria",
        aliases: &[],
        summary: "Collapse or expand the agent's task checklist",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Tasks,
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
        name: "permissions",
        aliases: &[],
        summary: "Show or change the approval tiers",
        args_hint: "[manual | smart | off | session off | session default]",
        takes_argument: true,
        action: LocalAction::Permissions,
    },
    LocalCommand {
        name: "cd",
        aliases: &[],
        summary: "Show or move the session's working folder",
        args_hint: "[<path>]",
        takes_argument: true,
        action: LocalAction::Cd,
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
            commands: BUILTIN.iter().chain(LOCAL_COMMANDS).copied().collect(),
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
    BUILTIN.iter().find(|command| command.answers_to(&key))
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
        return Err(format!("/{} does not take an argument.", command.name));
    }
    Ok((command.action, argument.to_owned()))
}
