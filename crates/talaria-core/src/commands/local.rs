//! Commands Talaria handles itself. Issue #223. Later units append entries.

/// What a local command does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalAction {
    /// Leave Talaria. The Hermes session keeps running.
    Quit,
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

/// The ordered local table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalCommandTable {
    commands: Vec<LocalCommand>,
}

impl LocalCommandTable {
    /// The built-in table. This unit's only entry is `/quit`.
    #[must_use]
    pub fn builtin() -> Self {
        todo!()
    }

    /// Find by name or alias. The slash is optional. Case does not matter.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&LocalCommand> {
        let _ = (name, &self.commands);
        todo!()
    }

    /// Entries in table order.
    pub fn iter(&self) -> impl Iterator<Item = &LocalCommand> {
        let commands = &self.commands;
        std::iter::from_fn(move || -> Option<&LocalCommand> {
            let _ = commands;
            todo!()
        })
    }
}
