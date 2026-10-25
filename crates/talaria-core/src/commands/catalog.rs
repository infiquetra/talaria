//! Catalogue built from a `commands.catalog` result. Issue #223.
//!
//! The catalogue is Hermes's view only. Talaria's own commands come from the local table,
//! which the palette lists first; this module never consults it.

use std::collections::{HashMap, HashSet};

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

impl SkillOrigin {
    fn parse(origin: &str) -> Self {
        match origin {
            "hub" => Self::Hub,
            "bundled" => Self::Bundled,
            "local" => Self::Local,
            other => Self::Other(other.to_owned()),
        }
    }
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

impl CommandSource {
    /// The tag drawn at the right of a row.
    #[must_use]
    pub fn tag(&self) -> &str {
        match self {
            Self::Talaria => "talaria",
            Self::Hermes { .. } => "hermes",
            Self::UserCommand => "your command",
            Self::Plugin => "plugin",
            Self::Skill { origin } => match origin {
                SkillOrigin::Hub => "skill \u{b7} hub",
                SkillOrigin::Bundled => "skill \u{b7} bundled",
                SkillOrigin::Local => "skill \u{b7} local",
                SkillOrigin::Other(_) => "skill",
            },
        }
    }
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

impl CatalogEntry {
    /// True when choosing this row should stop for an argument: any argument mode, or a
    /// skill, which always takes an instruction.
    #[must_use]
    pub fn takes_argument(&self) -> bool {
        self.argument_mode.is_some() || matches!(self.source, CommandSource::Skill { .. })
    }
}

/// The decoded `commands.catalog` result.
pub type CatalogResult = hermes_contract::methods::legacy::commands_catalog::Result;

/// Category names that do not name a registry group.
const USER_COMMANDS: &str = "User commands";
const PLUGIN_COMMANDS: &str = "Plugin commands";
const TUI: &str = "TUI";

/// Browse order, aliases, and the warning strings. Issue #223.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandCatalog {
    entries: Vec<CatalogEntry>,
    categories: Vec<String>,
    canon: HashMap<String, String>,
    warning: Option<String>,
    plugin_error: Option<String>,
    /// Lower-case name of each entry to its index.
    index: HashMap<String, usize>,
    /// Every name in `pairs`, `canon`, `commands` or `skills`, lower-case with a slash.
    known: HashSet<String>,
}

/// `/x` for `x` or `/x`, lower-case.
fn key(name: &str) -> String {
    let bare = name.trim().trim_start_matches('/');
    format!("/{}", bare.to_lowercase())
}

fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}

fn argument_mode(mode: Option<&hermes_contract::schemas::ArgumentMode>) -> Option<ArgumentMode> {
    use hermes_contract::schemas::ArgumentMode as Wire;
    mode.map(|mode| match mode {
        Wire::Options => ArgumentMode::Options,
        Wire::Text => ArgumentMode::Text,
        Wire::Mixed => ArgumentMode::Mixed,
    })
}

/// Provenance for one name by the run order Hermes uses: a registry category, a quick
/// command, a plugin, a skill. `None` hides the name (the terminal-only extras).
fn source_of(
    name: &str,
    homes: &[&str],
    result: &CatalogResult,
    in_pairs: bool,
) -> Option<(CommandSource, Option<String>)> {
    if let Some(category) = homes
        .iter()
        .find(|category| ![USER_COMMANDS, PLUGIN_COMMANDS, TUI].contains(category))
    {
        let category = (*category).to_owned();
        return Some((
            CommandSource::Hermes {
                category: category.clone(),
            },
            Some(category),
        ));
    }
    if homes.contains(&USER_COMMANDS) {
        return Some((CommandSource::UserCommand, Some(USER_COMMANDS.to_owned())));
    }
    let bare = name.trim_start_matches('/');
    if result.plugin_commands.contains_key(bare) {
        let home = homes
            .contains(&PLUGIN_COMMANDS)
            .then(|| PLUGIN_COMMANDS.to_owned());
        return Some((CommandSource::Plugin, home));
    }
    if let Some(skill) = result.skills.get(name) {
        return Some((
            CommandSource::Skill {
                origin: SkillOrigin::parse(&skill.origin),
            },
            None,
        ));
    }
    if homes.contains(&TUI) {
        return None;
    }
    // Listed without a category: Hermes lists every registry command in one, so this is a
    // newer or unusual entry. It is shown as a Hermes command without a group.
    in_pairs.then(|| {
        (
            CommandSource::Hermes {
                category: String::new(),
            },
            None,
        )
    })
}

impl CommandCatalog {
    /// Build the catalogue from one result.
    #[must_use]
    pub fn from_result(result: &CatalogResult) -> Self {
        // Every listed name with its description, in the order Hermes listed it: categories
        // first (their order is the browse order), then the flat pairs.
        let mut names: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut homes: HashMap<String, Vec<&str>> = HashMap::new();
        let mut text_of: HashMap<String, String> = HashMap::new();
        for category in &result.categories {
            for pair in &category.pairs {
                let Some(name) = pair.first() else { continue };
                homes
                    .entry(name.clone())
                    .or_default()
                    .push(category.name.as_str());
                if let Some(description) = pair.get(1) {
                    text_of
                        .entry(name.clone())
                        .or_insert_with(|| description.clone());
                }
                if seen.insert(name.clone()) {
                    names.push(name.clone());
                }
            }
        }
        let mut in_pairs: HashSet<&str> = HashSet::new();
        for pair in &result.pairs {
            let Some(name) = pair.first() else { continue };
            in_pairs.insert(name.as_str());
            if let Some(description) = pair.get(1) {
                text_of
                    .entry(name.clone())
                    .or_insert_with(|| description.clone());
            }
            if seen.insert(name.clone()) {
                names.push(name.clone());
            }
        }
        let mut skill_names: Vec<&String> = result.skills.keys().collect();
        skill_names.sort();
        for name in skill_names {
            if seen.insert(name.clone()) {
                names.push(name.clone());
            }
        }
        // Place each visible name in the group of its source: a category in Hermes's order,
        // then the uncategorised names, then the skills by usage and name.
        let categories: Vec<String> = result
            .categories
            .iter()
            .map(|category| category.name.clone())
            .filter(|name| name != TUI)
            .collect();
        let mut grouped: Vec<(Option<String>, CatalogEntry)> = Vec::new();
        for name in &names {
            let held = homes.get(name).map(Vec::as_slice).unwrap_or_default();
            let Some((source, home)) =
                source_of(name, held, result, in_pairs.contains(name.as_str()))
            else {
                continue;
            };
            let usage = match &source {
                CommandSource::Skill { .. } => result
                    .skills
                    .get(name)
                    .map_or(0, |skill| u64::try_from(skill.usage).unwrap_or(0)),
                _ => 0,
            };
            let argument_mode = result
                .commands
                .get(name)
                .and_then(|meta| argument_mode(meta.argument_mode.as_ref()));
            grouped.push((
                home,
                CatalogEntry {
                    name: name.clone(),
                    description: text_of.get(name).cloned().unwrap_or_default(),
                    source,
                    argument_mode,
                    usage,
                },
            ));
        }
        let mut entries: Vec<CatalogEntry> = Vec::with_capacity(grouped.len());
        for category in &categories {
            for (home, entry) in &grouped {
                if home.as_deref() == Some(category.as_str()) {
                    entries.push(entry.clone());
                }
            }
        }
        for (home, entry) in &grouped {
            let placed = home
                .as_deref()
                .is_some_and(|home| categories.iter().any(|category| category == home));
            if !placed && !matches!(entry.source, CommandSource::Skill { .. }) {
                entries.push(entry.clone());
            }
        }
        let mut skills: Vec<&CatalogEntry> = grouped
            .iter()
            .map(|(_, entry)| entry)
            .filter(|entry| matches!(entry.source, CommandSource::Skill { .. }))
            .collect();
        skills.sort_by(|left, right| {
            right
                .usage
                .cmp(&left.usage)
                .then_with(|| left.name.cmp(&right.name))
        });
        entries.extend(skills.into_iter().cloned());

        let index = entries
            .iter()
            .enumerate()
            .map(|(position, entry)| (key(&entry.name), position))
            .collect();
        let mut known: HashSet<String> = HashSet::new();
        known.extend(
            result
                .pairs
                .iter()
                .filter_map(|pair| pair.first())
                .map(|name| key(name)),
        );
        known.extend(result.canon.keys().map(|name| key(name)));
        known.extend(result.canon.values().map(|name| key(name)));
        known.extend(result.commands.keys().map(|name| key(name)));
        known.extend(result.skills.keys().map(|name| key(name)));
        known.extend(names.iter().map(|name| key(name)));
        let canon = result
            .canon
            .iter()
            .map(|(alias, target)| (key(alias), target.clone()))
            .collect();
        Self {
            entries,
            categories,
            canon,
            warning: non_empty(&result.warning),
            plugin_error: non_empty(&result.plugin_discovery_error),
            index,
            known,
        }
    }

    /// Entries in browse order.
    #[must_use]
    pub fn entries(&self) -> &[CatalogEntry] {
        &self.entries
    }

    /// Look up `/x` or `x`, case-insensitive, following aliases.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&CatalogEntry> {
        let wanted = key(name);
        if let Some(position) = self.index.get(&wanted) {
            return self.entries.get(*position);
        }
        let target = self.canon.get(&wanted)?;
        self.index
            .get(&key(target))
            .and_then(|position| self.entries.get(*position))
    }

    /// The canonical name, when `name` is an alias or a known command.
    #[must_use]
    pub fn canonical(&self, name: &str) -> Option<&str> {
        let wanted = key(name);
        if let Some(target) = self.canon.get(&wanted) {
            return Some(target.as_str());
        }
        self.index
            .get(&wanted)
            .and_then(|position| self.entries.get(*position))
            .map(|entry| entry.name.as_str())
    }

    /// True when `name` is an alias in `canon` (not a canonical name itself).
    #[must_use]
    pub fn is_alias(&self, name: &str) -> bool {
        let wanted = key(name);
        self.canon
            .get(&wanted)
            .is_some_and(|target| key(target) != wanted)
    }

    /// True when the name is in pairs, canon, commands, or skills.
    #[must_use]
    pub fn knows(&self, name: &str) -> bool {
        self.known.contains(&key(name))
    }

    /// The text inside `(usage: ...)`, when the description has one.
    #[must_use]
    pub fn usage_hint(&self, name: &str) -> Option<&str> {
        let entry = self.lookup(name)?;
        usage_inside(&entry.description)
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

/// The text of a trailing `(usage: ...)` suffix, as Hermes writes registry descriptions.
fn usage_inside(description: &str) -> Option<&str> {
    let start = description.rfind("(usage: ")?;
    let inner = &description[start + "(usage: ".len()..];
    let end = inner.rfind(')')?;
    let hint = inner[..end].trim();
    (!hint.is_empty()).then_some(hint)
}
