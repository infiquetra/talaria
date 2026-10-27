//! The rows of each section, before they are cut down to a screenful.

use serde_json::Value;

use super::fallbacks::{duplicates, notes, summary};
use super::helpers::{follows_main, label_and_hint, stale_helpers};
use super::main_role::effort_words;
use super::moa::{effort_words as slot_effort, fanout_words, number_words, slot_words};
use super::state::{CatalogState, Load, ModelRolesState, PAGE};
use super::{AuxSlot, ChainMode, MoaPresetView, RoleRoute, RolesKey, RowView, Section, text};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    None,
    MainModel,
    MainContext,
    MainEffort,
    Fallback(usize),
    Helper(usize),
    SubRoute,
    SubEffort,
    SubChainMode,
    SubChain(usize),
    Preset(usize),
    MoaEnabled,
    MoaReference(usize),
    MoaAggregator,
    MoaFanout,
    MoaPolicy,
    MoaTimeout,
    MoaReferenceTemperature,
    MoaAggregatorTemperature,
}

#[derive(Clone, Debug)]
pub(super) struct Item {
    pub text: String,
    pub kind: Kind,
    pub dim: bool,
    pub marker: Option<char>,
}

impl Item {
    pub(super) fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: Kind::None,
            dim: false,
            marker: None,
        }
    }

    pub(super) fn dim(text: impl Into<String>) -> Self {
        Self {
            dim: true,
            ..Self::plain(text)
        }
    }

    pub(super) fn pick(kind: Kind, text: impl Into<String>) -> Self {
        Self {
            kind,
            ..Self::plain(text)
        }
    }

    pub(super) fn selectable(&self) -> bool {
        self.kind != Kind::None
    }
}

pub(super) enum Status {
    Loading,
    Ready,
    Problem(Vec<String>),
    Gone(String),
}

fn load_status<T>(load: &Load<T>) -> Status {
    match load {
        Load::Pending => Status::Loading,
        Load::Ready(_) => Status::Ready,
        Load::Failed { last, text } => {
            if last.is_some() {
                Status::Ready
            } else {
                Status::Problem(text.clone())
            }
        }
        Load::Unavailable { text, .. } => Status::Gone(text.clone()),
    }
}

fn bad_lines<T>(load: &Load<T>, out: &mut Vec<String>) {
    match load {
        Load::Failed { text, .. } => out.extend(text.iter().cloned()),
        Load::Unavailable { text, .. } => out.push(text.clone()),
        _ => {}
    }
}

impl ModelRolesState {
    pub(super) fn status(&self, section: Section) -> Status {
        match section {
            Section::Main => {
                if self.info.value().is_some()
                    || self.aux.value().is_some()
                    || self.raw_value().is_some()
                {
                    return Status::Ready;
                }
                for status in [
                    load_status(&self.info),
                    load_status(&self.aux),
                    load_status(&self.raw),
                ] {
                    if !matches!(status, Status::Loading) {
                        return status;
                    }
                }
                Status::Loading
            }
            Section::Fallbacks | Section::Subagents => load_status(&self.raw),
            Section::Helpers => load_status(&self.aux),
            Section::Moa => load_status(&self.moa),
        }
    }

    /// A section whose last read failed still shows what it knew, with its tab marked.
    pub(super) fn tab_stale(&self, section: Section) -> bool {
        match section {
            Section::Main => self.info.is_bad() || self.aux.is_bad() || self.raw.is_bad(),
            Section::Fallbacks | Section::Subagents => self.raw.is_bad(),
            Section::Helpers => self.aux.is_bad(),
            Section::Moa => self.moa.is_bad(),
        }
    }

    pub(super) fn problem_lines(&self, section: Section) -> Vec<String> {
        let mut out = Vec::new();
        match section {
            Section::Main => {
                bad_lines(&self.info, &mut out);
                bad_lines(&self.aux, &mut out);
                bad_lines(&self.raw, &mut out);
            }
            Section::Fallbacks | Section::Subagents => bad_lines(&self.raw, &mut out),
            Section::Helpers => bad_lines(&self.aux, &mut out),
            Section::Moa => bad_lines(&self.moa, &mut out),
        }
        out.dedup();
        out
    }

    pub(super) fn unlisted(&self, provider: &str, model: &str) -> bool {
        match &self.catalog {
            CatalogState::Ready(catalog) => {
                !provider.trim().is_empty()
                    && !model.trim().is_empty()
                    && !catalog.lists(provider, model)
            }
            _ => false,
        }
    }

    pub(super) fn items(&self, section: Section) -> Vec<Item> {
        match self.status(section) {
            Status::Loading => return vec![Item::plain(text::LOADING)],
            Status::Problem(lines) => return lines.into_iter().map(Item::plain).collect(),
            Status::Gone(line) => return vec![Item::plain(line)],
            Status::Ready => {}
        }
        match section {
            Section::Main => self.main_items(),
            Section::Fallbacks => self.fallback_items(),
            Section::Helpers => self.helper_items(),
            Section::Subagents => self.subagent_items(),
            Section::Moa => self.preset_items(),
        }
    }

    fn main_items(&self) -> Vec<Item> {
        let mut out = Vec::new();
        let route = self
            .edits
            .main_route
            .as_ref()
            .map(|route| (route.provider.clone(), route.model.clone()))
            .or_else(|| self.main_route());
        let mut model = match &route {
            Some((provider, model)) => format!("Model: {model} (via {provider})"),
            None => format!("Model: {}", text::NOT_SET),
        };
        if let Some((provider, name)) = &route
            && self.unlisted(provider, name)
        {
            model.push(' ');
            model.push_str(text::NOT_IN_LIST);
        }
        if self.edits.main_route.is_some() {
            model.push(' ');
            model.push_str(text::CHANGED_MARK);
        }
        out.push(Item::pick(Kind::MainModel, model));
        out.push(Item::pick(Kind::MainContext, self.context_line()));
        match self.info.value() {
            Some(info) => {
                let caps = &info.capabilities;
                if caps.is_empty() {
                    out.push(Item::plain("Capabilities: unknown"));
                } else {
                    let mut can = Vec::new();
                    for (key, word) in [
                        ("supports_tools", "tools"),
                        ("supports_vision", "images"),
                        ("supports_reasoning", "reasoning"),
                    ] {
                        if caps.get(key).and_then(Value::as_bool) == Some(true) {
                            can.push(word);
                        }
                    }
                    if can.is_empty() {
                        out.push(Item::plain("Can use: nothing listed"));
                    } else {
                        out.push(Item::plain(format!("Can use: {}", can.join(", "))));
                    }
                    if let Some(max) = caps.get("max_output_tokens").and_then(Value::as_u64)
                        && max > 0
                    {
                        out.push(Item::plain(format!(
                            "Max output: {} tokens",
                            text::count(max)
                        )));
                    }
                    if let Some(family) = caps.get("model_family").and_then(Value::as_str)
                        && !family.trim().is_empty()
                    {
                        out.push(Item::plain(format!("Family: {family}")));
                    }
                }
            }
            None if matches!(self.info, Load::Pending) => {
                out.push(Item::plain("Capabilities: checking..."));
            }
            None => out.push(Item::plain("Capabilities: unknown")),
        }
        out.push(Item::pick(Kind::MainEffort, self.main_effort_line()));
        out.push(Item::plain(text::APPLIES_MAIN));
        if let Some((provider, model)) = &self.saved_default {
            out.push(Item::plain(
                text::SAVED_DEFAULT
                    .replace("{provider}", provider)
                    .replace("{model}", model),
            ));
        }
        out
    }

    fn context_line(&self) -> String {
        if let Some(value) = self.edits.main_context {
            return if value == 0 {
                format!("Context window: auto-detect {}", text::CHANGED_MARK)
            } else {
                format!(
                    "Context window: {} tokens (set) {}",
                    text::count(value),
                    text::CHANGED_MARK
                )
            };
        }
        match self.info.value() {
            None if matches!(self.info, Load::Pending) => "Context window: checking...".to_owned(),
            None => "Context window: unknown".to_owned(),
            Some(info) => {
                let auto = info.auto_context_length;
                let set = info.config_context_length;
                if set > 0 {
                    let found = if auto > 0 {
                        text::count(auto)
                    } else {
                        "unknown".to_owned()
                    };
                    format!(
                        "Context window: {} tokens (set; auto-detected {found})",
                        text::count(set)
                    )
                } else if auto > 0 {
                    format!(
                        "Context window: {} tokens (auto-detected)",
                        text::count(auto)
                    )
                } else {
                    "Context window: unknown".to_owned()
                }
            }
        }
    }

    fn main_effort_line(&self) -> String {
        if let Some(effort) = self.edits.main_effort {
            let word = effort.map_or(text::INHERITED, |level| level.as_wire());
            return format!("Default effort: {word} {}", text::CHANGED_MARK);
        }
        match self.raw_value() {
            Some(raw) => format!(
                "Default effort: {}",
                effort_words(&raw.agent_reasoning_effort)
            ),
            None if matches!(self.raw, Load::Pending) => "Default effort: checking...".to_owned(),
            None => "Default effort: unknown".to_owned(),
        }
    }

    fn fallback_items(&self) -> Vec<Item> {
        let chain = self.fallback_work();
        let legacy = self.legacy_entries();
        let (dup_chain, dup_legacy) = duplicates(&chain, &legacy);
        let main = self.main_route();
        let mut out = Vec::new();
        if chain.is_empty() && legacy.is_empty() {
            out.push(Item::plain(text::NO_FALLBACKS));
        }
        for (index, entry) in chain.iter().enumerate() {
            let mut line = format!("{}. {}", index + 1, summary(entry));
            if self.unlisted(&entry.provider, &entry.model) {
                line.push(' ');
                line.push_str(text::NOT_IN_LIST);
            }
            line.push_str(&notes(entry, dup_chain[index], main.as_ref()));
            out.push(Item::pick(Kind::Fallback(index), line));
        }
        if !legacy.is_empty() {
            out.push(Item::dim(text::LEGACY_RULE));
            for (index, entry) in legacy.iter().enumerate() {
                let line = format!(
                    "   {}{}",
                    summary(entry),
                    notes(entry, dup_legacy[index], main.as_ref())
                );
                out.push(Item::dim(line));
            }
        }
        out
    }

    pub(super) fn helper_slots(&self) -> Vec<AuxSlot> {
        self.aux
            .value()
            .map(|aux| aux.slots.clone())
            .unwrap_or_default()
    }

    fn helper_items(&self) -> Vec<Item> {
        let slots = self.helper_slots();
        let main = self.main_route();
        let stale: Vec<String> = stale_helpers(&slots, &self.main_provider())
            .into_iter()
            .map(|slot| slot.task)
            .collect();
        let mut out = Vec::new();
        for (index, slot) in slots.iter().enumerate() {
            let edit = self
                .edits
                .helpers
                .iter()
                .find(|edit| edit.task == slot.task);
            let (label, hint) = label_and_hint(slot);
            let mut line = match hint {
                Some(hint) => format!("{label} - {hint}"),
                None => label,
            };
            if let Some(plugin) = &slot.plugin {
                line.push_str(&format!("  from plugin {plugin}"));
            }
            let (provider, model, base_url) = match edit.and_then(|edit| edit.route.as_ref()) {
                Some(super::state::HelperRoute::Inherit) => {
                    ("auto".to_owned(), String::new(), String::new())
                }
                Some(super::state::HelperRoute::Pin(route)) => (
                    route.provider.clone(),
                    route.model.clone(),
                    route.base_url.clone(),
                ),
                None => (
                    slot.provider.clone(),
                    slot.model.clone(),
                    slot.base_url.clone(),
                ),
            };
            let inherits_base =
                slot.inherit_from.is_some() && edit.is_none_or(|edit| edit.route.is_none());
            let route = if inherits_base {
                let base = slot.inherit_from.clone().unwrap_or_default();
                let effective = slot
                    .effective
                    .as_ref()
                    .map(|triple| triple.model.clone())
                    .filter(|model| !model.is_empty())
                    .unwrap_or_else(|| text::NOT_SET.to_owned());
                format!("follows {base}: {effective}")
            } else if follows_main(&provider) {
                if main.is_some() {
                    text::FOLLOWS_MAIN.to_owned()
                } else {
                    format!("{} (none set)", text::FOLLOWS_MAIN)
                }
            } else {
                let mut words = format!("{model} (via {provider})");
                if self.unlisted(&provider, &model) {
                    words.push(' ');
                    words.push_str(text::NOT_IN_LIST);
                }
                words
            };
            let effort = match edit.and_then(|edit| edit.effort) {
                Some(Some(level)) => level.as_wire().to_owned(),
                Some(None) => text::INHERITED.to_owned(),
                None => match slot.reasoning_effort.as_deref() {
                    Some(level) if !level.is_empty() => level.to_owned(),
                    _ => text::INHERITED.to_owned(),
                },
            };
            line.push_str(&format!("  {route}  effort: {effort}"));
            if !base_url.is_empty() {
                line.push_str("  [endpoint]");
            }
            if edit.is_some() {
                line.push(' ');
                line.push_str(text::CHANGED_MARK);
            }
            let mut item = Item::pick(Kind::Helper(index), line);
            if stale.contains(&slot.task) && edit.is_none_or(|edit| edit.route.is_none()) {
                item.marker = Some('!');
            }
            out.push(item);
        }
        if out.is_empty() {
            out.push(Item::plain("No helper slots."));
        }
        out
    }

    fn subagent_items(&self) -> Vec<Item> {
        let roles = self.sub_work();
        let mut out = Vec::new();
        let route = match &roles.route {
            RoleRoute::Inherit => "Route: follows main model".to_owned(),
            RoleRoute::Pinned { provider, model } => {
                let mut words = format!("Route: {model} (via {provider})");
                if self.unlisted(provider, model) {
                    words.push(' ');
                    words.push_str(text::NOT_IN_LIST);
                }
                words
            }
            RoleRoute::Partial { provider, model } => {
                if model.is_empty() {
                    format!("Route: {provider}, model inherited")
                } else {
                    format!("Route: {model}, provider inherited")
                }
            }
        };
        out.push(Item::pick(Kind::SubRoute, route));
        if let Some(url) = &roles.direct_endpoint {
            out.push(Item::plain(format!(
                "Direct endpoint: {}",
                super::diff::mask_url(url)
            )));
        }
        let effort = roles
            .effort
            .map_or(text::INHERITED, |level| level.as_wire());
        out.push(Item::pick(Kind::SubEffort, format!("Effort: {effort}")));
        match &roles.chain {
            ChainMode::Inherit => out.push(Item::pick(
                Kind::SubChainMode,
                "Fallback: inherits the main chain",
            )),
            ChainMode::Disabled => {
                out.push(Item::pick(Kind::SubChainMode, "Fallback: no fallback"))
            }
            ChainMode::Own(entries) => {
                let usable = entries.iter().any(super::fallbacks::is_complete);
                let head = if usable {
                    "Fallback: own chain"
                } else {
                    text::NO_USABLE_CHAIN
                };
                out.push(Item::pick(Kind::SubChainMode, head));
                let (dups, _) = duplicates(entries, &[]);
                let main = self.main_route();
                for (index, entry) in entries.iter().enumerate() {
                    let mut line = format!("  {}. {}", index + 1, summary(entry));
                    if self.unlisted(&entry.provider, &entry.model) {
                        line.push(' ');
                        line.push_str(text::NOT_IN_LIST);
                    }
                    line.push_str(&notes(entry, dups[index], main.as_ref()));
                    out.push(Item::pick(Kind::SubChain(index), line));
                }
            }
        }
        out.push(Item::dim(text::SUBAGENT_HINT));
        out
    }

    fn preset_items(&self) -> Vec<Item> {
        let Some(config) = self.moa_work() else {
            return vec![Item::plain(text::LOADING)];
        };
        config
            .presets
            .iter()
            .enumerate()
            .map(|(index, preset)| {
                let star = if preset.name == config.default_preset {
                    "* "
                } else {
                    "  "
                };
                let off = if preset.enabled { "" } else { " (off)" };
                Item::pick(Kind::Preset(index), format!("{star}{}{off}", preset.name))
            })
            .collect()
    }

    /// The preset under the cursor in the list pane.
    pub(super) fn selected_preset(&self) -> Option<(usize, MoaPresetView)> {
        let config = self.moa_work()?;
        let items = self.preset_items();
        let cursor = self.cursor[super::state::index_of(Section::Moa)];
        match items.get(cursor).map(|item| &item.kind) {
            Some(Kind::Preset(index)) => config
                .presets
                .get(*index)
                .cloned()
                .map(|preset| (*index, preset)),
            _ => None,
        }
    }

    pub(super) fn detail_items(&self) -> Vec<Item> {
        let Some((_, preset)) = self.selected_preset() else {
            return Vec::new();
        };
        let Some(config) = self.moa_work() else {
            return Vec::new();
        };
        let mut out = vec![
            Item::pick(
                Kind::MoaEnabled,
                format!("Enabled: {}", if preset.enabled { "yes" } else { "no" }),
            ),
            Item::plain("References:"),
        ];
        if preset.references.is_empty() {
            out.push(Item::dim("  No references yet. Press a to add one."));
        } else if preset
            .references
            .iter()
            .all(|slot| slot.enabled == Some(false))
        {
            out.push(Item::dim(format!("  {}", text::NO_REFERENCE_ENABLED)));
        }
        for (index, slot) in preset.references.iter().enumerate() {
            let off = if slot.enabled == Some(false) {
                "  (off)"
            } else {
                ""
            };
            out.push(Item::pick(
                Kind::MoaReference(index),
                format!(
                    "  {}. {}  effort: {}{off}",
                    index + 1,
                    slot_words(slot),
                    slot_effort(slot)
                ),
            ));
        }
        out.push(Item::pick(
            Kind::MoaAggregator,
            format!(
                "Aggregator: {}  effort: {}",
                slot_words(&preset.aggregator),
                slot_effort(&preset.aggregator)
            ),
        ));
        out.push(Item::pick(
            Kind::MoaFanout,
            format!("Advisors run: {}", fanout_words(&preset.fanout)),
        ));
        let policy = if preset.degraded_reference_policy == "silent" {
            "carry on (silent)"
        } else {
            "say so (loud)"
        };
        out.push(Item::pick(
            Kind::MoaPolicy,
            format!("If an advisor fails: {policy}"),
        ));
        out.push(Item::pick(
            Kind::MoaTimeout,
            format!(
                "Advisor timeout: {}",
                number_words(preset.reference_timeout, "inherit", " s")
            ),
        ));
        out.push(Item::pick(
            Kind::MoaReferenceTemperature,
            format!(
                "Reference temperature: {}",
                number_words(preset.reference_temperature, "provider default", "")
            ),
        ));
        out.push(Item::pick(
            Kind::MoaAggregatorTemperature,
            format!(
                "Aggregator temperature: {}",
                number_words(preset.aggregator_temperature, "provider default", "")
            ),
        ));
        let active = if config.active_preset.is_empty() {
            "none"
        } else {
            config.active_preset.as_str()
        };
        out.push(Item::dim(format!("Active preset: {active}")));
        let privacy = match config.privacy_filter.as_deref() {
            Some(value) if !value.is_empty() => value,
            _ => "off",
        };
        out.push(Item::dim(format!("Privacy filter: {privacy}")));
        out
    }
}

/// Pins a cursor to a row that can be selected.
pub(super) fn settle_cursor(items: &[Item], cursor: usize) -> usize {
    let cursor = cursor.min(items.len().saturating_sub(1));
    if items.get(cursor).is_some_and(Item::selectable) {
        return cursor;
    }
    items
        .iter()
        .enumerate()
        .skip(cursor)
        .find(|(_, item)| item.selectable())
        .or_else(|| {
            items
                .iter()
                .enumerate()
                .rev()
                .find(|(_, item)| item.selectable())
        })
        .map_or(0, |(index, _)| index)
}

/// Moves a cursor over selectable rows.
pub(super) fn step(items: &[Item], cursor: usize, key: &RolesKey) -> usize {
    let selectable: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.selectable())
        .map(|(index, _)| index)
        .collect();
    if selectable.is_empty() {
        return cursor;
    }
    let at = selectable
        .iter()
        .position(|index| *index >= cursor)
        .unwrap_or(selectable.len() - 1);
    let last = selectable.len() - 1;
    let to = match key {
        RolesKey::Up => at.saturating_sub(1),
        RolesKey::Down => (at + 1).min(last),
        RolesKey::Home => 0,
        RolesKey::End => last,
        RolesKey::PageUp => at.saturating_sub(PAGE),
        RolesKey::PageDown => (at + PAGE).min(last),
        _ => at,
    };
    selectable[to]
}

/// Keeps the cursor row on screen, one row clear of an indicator.
pub(super) fn fit(scroll: &mut usize, cursor: usize, len: usize) {
    if len <= PAGE {
        *scroll = 0;
        return;
    }
    let max = len - PAGE;
    *scroll = (*scroll).min(max);
    if cursor < *scroll || (*scroll > 0 && cursor == *scroll) {
        *scroll = cursor.saturating_sub(1);
    }
    if *scroll + PAGE < len && cursor + 1 >= *scroll + PAGE {
        *scroll = (cursor + 2).saturating_sub(PAGE);
    }
    *scroll = (*scroll).min(max);
}

fn row(item: &Item, selected: bool) -> RowView {
    RowView {
        text: item.text.clone(),
        selected,
        dimmed: item.dim,
        marker: item.marker,
    }
}

/// The visible rows, with `↑ n more` and `↓ n more` replacing the cut-off ends.
pub(super) fn window(
    items: &[Item],
    cursor: usize,
    scroll: usize,
    show_cursor: bool,
) -> Vec<RowView> {
    let len = items.len();
    let (start, end) = if len <= PAGE {
        (0, len)
    } else {
        let start = scroll.min(len - PAGE);
        (start, start + PAGE)
    };
    let mut rows: Vec<RowView> = (start..end)
        .map(|index| {
            row(
                &items[index],
                show_cursor && index == cursor && items[index].selectable(),
            )
        })
        .collect();
    let indicator = |text: String| RowView {
        text,
        selected: false,
        dimmed: true,
        marker: None,
    };
    if start > 0
        && let Some(first) = rows.first_mut()
    {
        *first = indicator(format!("↑ {} more", start + 1));
    }
    if end < len
        && let Some(last) = rows.last_mut()
    {
        *last = indicator(format!("↓ {} more", len - end + 1));
    }
    rows
}
