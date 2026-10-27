//! What the page was last told, and what it would write now.

use serde_json::Value;

use super::fallbacks::{parse_entries, same_chain};
use super::main_role::effort_from_raw;
use super::state::{ModelRolesState, SECTIONS};
use super::subagents::from_raw;
use super::{
    ChainMode, FallbackEntry, MainRoles, MoaConfigView, RawRoles, RoleRoute, Section, SubagentRoles,
};

impl ModelRolesState {
    pub(super) fn raw_value(&self) -> Option<&RawRoles> {
        self.raw.value()
    }

    /// The main route as Hermes holds it: model information first, the helper reply's `main`
    /// until that answers.
    pub(super) fn main_route(&self) -> Option<(String, String)> {
        if let Some(info) = self.info.value()
            && !info.model.trim().is_empty()
        {
            return Some((info.provider.clone(), info.model.clone()));
        }
        let main = &self.aux.value()?.main;
        (!main.provider.trim().is_empty() || !main.model.trim().is_empty())
            .then(|| (main.provider.clone(), main.model.clone()))
    }

    pub(super) fn main_provider(&self) -> String {
        self.aux
            .value()
            .map(|aux| aux.main.provider.clone())
            .or_else(|| self.info.value().map(|info| info.provider.clone()))
            .unwrap_or_default()
    }

    pub(super) fn fallback_base(&self) -> Vec<FallbackEntry> {
        self.raw_value()
            .map(|raw| parse_entries(&raw.fallback_providers))
            .unwrap_or_default()
    }

    pub(super) fn fallback_work(&self) -> Vec<FallbackEntry> {
        self.edits
            .fallbacks
            .clone()
            .unwrap_or_else(|| self.fallback_base())
    }

    pub(super) fn legacy_entries(&self) -> Vec<FallbackEntry> {
        self.raw_value()
            .map(|raw| parse_entries(&raw.fallback_model))
            .unwrap_or_default()
    }

    pub(super) fn sub_base(&self) -> SubagentRoles {
        from_raw(
            &self
                .raw_value()
                .map(|raw| raw.delegation.clone())
                .unwrap_or_default(),
        )
    }

    pub(super) fn sub_work(&self) -> SubagentRoles {
        self.edits
            .subagents
            .clone()
            .unwrap_or_else(|| self.sub_base())
    }

    pub(super) fn moa_base(&self) -> Option<&MoaConfigView> {
        self.moa.value()
    }

    pub(super) fn moa_work(&self) -> Option<MoaConfigView> {
        self.edits.moa.clone().or_else(|| self.moa.value().cloned())
    }

    /// The profile has no presets of its own, so a save writes Hermes's example.
    pub(super) fn first_save(&self) -> bool {
        self.raw_value().is_some_and(|raw| !raw.has_moa_presets) && self.moa.value().is_some()
    }

    pub(super) fn main_before(&self) -> MainRoles {
        let route = match self.main_route() {
            Some((provider, model)) => RoleRoute::Pinned { provider, model },
            None => RoleRoute::Inherit,
        };
        let (effort, effort_read_only, context_override) = match self.raw_value() {
            Some(raw) => {
                let (effort, read_only) = effort_from_raw(&raw.agent_reasoning_effort);
                (effort, read_only, raw.model_context_length)
            }
            None => (None, false, 0),
        };
        MainRoles {
            route,
            context_override,
            effort,
            effort_read_only,
        }
    }

    pub(super) fn pending(&self, section: Section) -> bool {
        match section {
            Section::Main => {
                self.edits.main_route.is_some()
                    || self.edits.main_context.is_some()
                    || self.edits.main_effort.is_some()
            }
            Section::Helpers => !self.edits.helpers.is_empty(),
            Section::Fallbacks => self
                .edits
                .fallbacks
                .as_ref()
                .is_some_and(|work| !same_chain(work, &self.fallback_base())),
            Section::Subagents => self
                .edits
                .subagents
                .as_ref()
                .is_some_and(|work| !same_roles(work, &self.sub_base())),
            Section::Moa => self
                .edits
                .moa
                .as_ref()
                .is_some_and(|work| Some(work) != self.moa.value()),
        }
    }

    pub(super) fn pending_section(&self) -> Option<Section> {
        SECTIONS.into_iter().find(|section| self.pending(*section))
    }

    /// A review can write something: pending edits, entries that Hermes ignores until saved,
    /// or the first save of the example presets.
    pub(super) fn has_changes(&self, section: Section) -> bool {
        self.pending(section)
            || match section {
                Section::Fallbacks => self.fallback_work().iter().any(|entry| entry.was_string),
                Section::Subagents => match self.sub_work().chain {
                    ChainMode::Own(entries) => entries.iter().any(|entry| entry.was_string),
                    _ => false,
                },
                Section::Moa => self.first_save(),
                _ => false,
            }
    }

    /// The raw `${...}` text of a delegation key, as read.
    pub(super) fn raw_delegation(&self, key: &str) -> Option<Value> {
        self.raw_value()
            .and_then(|raw| raw.delegation.get(key).cloned())
    }
}

pub(super) fn same_roles(left: &SubagentRoles, right: &SubagentRoles) -> bool {
    left.route == right.route
        && left.effort == right.effort
        && left.direct_endpoint == right.direct_endpoint
        && left.endpoint_keys == right.endpoint_keys
        && match (&left.chain, &right.chain) {
            (ChainMode::Own(a), ChainMode::Own(b)) => same_chain(a, b),
            (a, b) => a == b,
        }
}
