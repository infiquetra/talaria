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
    },
    LocalCommand {
        name: "model",
        aliases: &[],
        summary: "Choose the model of this session",
        args_hint: "[<model> [--provider <id>] [--effort <level>] [--default]]",
        takes_argument: true,
        action: LocalAction::Model,
    },
    LocalCommand {
        name: "effort",
        aliases: &[],
        summary: "Choose the reasoning level of this session",
        args_hint: "[<level>|default] [--default]",
        takes_argument: true,
        action: LocalAction::Effort,
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
    },
    LocalCommand {
        name: "undo",
        aliases: &[],
        summary: "Remove your last N prompts and their replies (default 1)",
        args_hint: "[N]",
        takes_argument: true,
        action: LocalAction::Undo,
    },
    LocalCommand {
        name: "retry",
        aliases: &[],
        summary: "Remove the last prompt and its reply, then send the prompt again",
        args_hint: "",
        takes_argument: false,
        action: LocalAction::Retry,
    },
    LocalCommand {
        name: "rollback",
        aliases: &[],
        summary: "List, compare or restore file checkpoints",
        args_hint: "[list] | diff <n> | <n> [file] [--all]",
        takes_argument: true,
        action: LocalAction::Rollback,
    },
    LocalCommand {
        name: "diff",
        aliases: &[],
        summary: "Show what changed since Hermes's oldest kept checkpoint",
        args_hint: "[--stat]",
        takes_argument: true,
        action: LocalAction::Diff,
    },
];

/// The ordered local table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalCommandTable {
    commands: Vec<LocalCommand>,
}

impl LocalCommandTable {
    /// The built-in table. This unit's only entry is `/quit`.
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            commands: BUILTIN.to_vec(),
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
