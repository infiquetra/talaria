//! Catalogue built from a `commands.catalog` result. Issue #223.

use std::collections::HashMap;

/// Where a skill was loaded from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillOrigin {
    /// The skill hub.
    Hub,
    /// Shipped with Hermes.
    Bundled,
    /// On this machine.
    Local,
    /// Any other origin string.
    Other(String),
}

/// Who owns a palette row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandSource {
    /// A Talaria command. It wins over a Hermes command of the same name.
    Talaria,
    /// A Hermes registry command in this category.
    Hermes { category: String },
    /// A quick command from the "User commands" group.
    UserCommand,
    /// A plugin command.
    Plugin,
    /// A skill. `origin` is omitted from the tag when Hermes sent none.
    Skill { origin: SkillOrigin },
}

/// How a command takes its argument, when the catalogue says it takes one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentMode {
    /// A fixed set of options.
    Options,
    /// Free text.
    Text,
    /// Either.
    Mixed,
}

/// One command the palette can show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogEntry {
    /// Name with one leading slash, as Hermes sent it.
    pub name: String,
    /// Description from the catalogue pairs.
    pub description: String,
    /// Provenance, first match wins.
    pub source: CommandSource,
    /// `None` when the command takes no argument, or has no `commands` entry.
    pub argument_mode: Option<ArgumentMode>,
    /// Skill usage. Zero for everything else.
    pub usage: u64,
}

/// The decoded `commands.catalog` result.
pub type CatalogResult = hermes_contract::methods::legacy::commands_catalog::Result;

/// Browse order, aliases, and the warning strings. Issue #223.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandCatalog {
    entries: Vec<CatalogEntry>,
    categories: Vec<String>,
    canon: HashMap<String, String>,
    warning: Option<String>,
    plugin_error: Option<String>,
}

impl CommandCatalog {
    /// Build the catalogue from one result.
    #[must_use]
    pub fn from_result(result: &CatalogResult) -> Self {
        let _ = result;
        todo!()
    }

    /// Entries in browse order.
    #[must_use]
    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    /// Look up `/x` or `x`, case-insensitive, following aliases.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&CatalogEntry> {
        let _ = name;
        let _ = &self.entries;
        todo!()
    }

    /// The canonical name, when `name` is an alias or a known command.
    #[must_use]
    pub fn canonical(&self, name: &str) -> Option<&str> {
        let _ = name;
        let _ = &self.canon;
        todo!()
    }

    /// True when the name is in pairs, canon, commands, or skills.
    #[must_use]
    pub fn knows(&self, name: &str) -> bool {
        let _ = (name, &self.entries);
        todo!()
    }

    /// The text inside `(usage: ...)`, when the description has one.
    #[must_use]
    pub fn usage_hint(&self, name: &str) -> Option<&str> {
        let _ = (name, &self.entries);
        todo!()
    }

    /// The catalogue warning. `None` when it is empty.
    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    /// The plugin discovery error. `None` when it is empty.
    #[must_use]
    pub fn plugin_discovery_error(&self) -> Option<&str> {
        self.plugin_error.as_deref()
    }

    /// Category names in Hermes order, without `TUI`.
    #[must_use]
    pub fn categories(&self) -> &[String] {
        &self.categories
    }
}
