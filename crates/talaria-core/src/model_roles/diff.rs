//! The key level diff shown before a write. Secrets are masked here, once.

use serde_json::Value;

use super::fallbacks::summary;
use super::moa::changed_fields;
use super::state::ModelRolesState;
use super::subagents::subagent_patch;
use super::{ChainMode, DiffLine, EffortChange, FallbackEntry, Section, text};
use crate::settings::is_secret_key;

fn line(key: &str, before: String, after: String) -> DiffLine {
    DiffLine {
        key: key.to_owned(),
        before,
        after,
    }
}

/// A line that is a sentence rather than a before and after pair.
fn note(sentence: String) -> DiffLine {
    line(&sentence, String::new(), String::new())
}

/// Strings as written, `null` and absent as `(not set)`, `""` as `""`. A non-empty value under
/// a secret key is only ever `[key set]`.
pub(super) fn show(key: &str, value: Option<&Value>) -> String {
    if (key.ends_with("base_url") || key.ends_with("api_url"))
        && let Some(Value::String(url)) = value
    {
        return mask_url(url);
    }
    match value {
        None | Some(Value::Null) => text::NOT_SET.to_owned(),
        Some(Value::String(word)) if word.is_empty() => "\"\"".to_owned(),
        Some(_) if is_secret_key(key) => text::KEY_SET.to_owned(),
        Some(Value::String(word)) => word.clone(),
        Some(other) => other.to_string(),
    }
}

/// Keeps credentials embedded in a URL's user information out of sight.
pub(super) fn mask_url(url: &str) -> String {
    if url.is_empty() {
        return "\"\"".to_owned();
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    match rest[..end].rfind('@') {
        Some(at) => format!("{scheme}://{}@{}", text::KEY_SET, &rest[at + 1..]),
        None => url.to_owned(),
    }
}

fn shown_text(key: &str, value: &str) -> String {
    show(key, Some(&Value::String(value.to_owned())))
}

/// A slot's stored value, masked by the same key rule as the new one.
fn stored(key: &str, value: Option<&str>) -> String {
    match value {
        Some(word) => shown_text(key, word),
        None => text::NOT_SET.to_owned(),
    }
}

fn chain_lines(
    label: &str,
    before: Option<&[FallbackEntry]>,
    after: Option<&[FallbackEntry]>,
    empty: &str,
) -> Vec<DiffLine> {
    let mut out = Vec::new();
    for (word, entries) in [("before", before), ("after", after)] {
        out.push(note(format!("{label} ({word}):")));
        match entries {
            Some(entries) if !entries.is_empty() => {
                for (index, entry) in entries.iter().enumerate() {
                    out.push(note(format!("  {}. {}", index + 1, summary(entry))));
                }
            }
            Some(_) => out.push(note("  (none)".to_owned())),
            None => out.push(note(format!("  {empty}"))),
        }
    }
    if let Some(entries) = after {
        for (index, entry) in entries.iter().enumerate() {
            if entry.was_string {
                out.push(note(text::string_diff(index + 1)));
            }
        }
    }
    out
}

fn chain_entries(chain: &ChainMode) -> Option<&[FallbackEntry]> {
    match chain {
        ChainMode::Inherit => None,
        ChainMode::Disabled => Some(&[]),
        ChainMode::Own(entries) => Some(entries.as_slice()),
    }
}

impl ModelRolesState {
    pub(super) fn diff_lines(&self, section: Section) -> Vec<DiffLine> {
        match section {
            Section::Main => self.main_diff(),
            Section::Helpers => self.helper_diff(),
            Section::Fallbacks => chain_lines(
                "fallback_providers",
                Some(&self.fallback_base()),
                Some(&self.fallback_work()),
                "",
            ),
            Section::Subagents => self.subagent_diff(),
            Section::Moa => self.moa_diff(),
        }
    }

    fn main_diff(&self) -> Vec<DiffLine> {
        let mut out = Vec::new();
        if let Some(route) = &self.edits.main_route {
            let before = self.main_route();
            out.push(line(
                "model.provider",
                before.as_ref().map_or_else(
                    || text::NOT_SET.to_owned(),
                    |(provider, _)| shown_text("model.provider", provider),
                ),
                shown_text("model.provider", &route.provider),
            ));
            out.push(line(
                "model.default",
                before.as_ref().map_or_else(
                    || text::NOT_SET.to_owned(),
                    |(_, model)| shown_text("model.default", model),
                ),
                shown_text("model.default", &route.model),
            ));
        }
        if let Some(context) = self.edits.main_context {
            let before = self.raw_value().map_or(0, |raw| raw.model_context_length);
            let words = |value: u64| {
                if value == 0 {
                    "0 (auto)".to_owned()
                } else {
                    value.to_string()
                }
            };
            out.push(line("model_context_length", words(before), words(context)));
        }
        if let Some(effort) = self.edits.main_effort {
            let before = self.raw_value().map_or_else(
                || text::NOT_SET.to_owned(),
                |raw| show("agent.reasoning_effort", Some(&raw.agent_reasoning_effort)),
            );
            out.push(line(
                "agent.reasoning_effort",
                before,
                shown_text(
                    "agent.reasoning_effort",
                    effort.map_or("", |level| level.as_wire()),
                ),
            ));
        }
        out
    }

    fn helper_diff(&self) -> Vec<DiffLine> {
        let slots = self.helper_slots();
        let mut out = Vec::new();
        for assignment in self.helper_assignments() {
            let slot = slots.iter().find(|slot| slot.task == assignment.task);
            let task = &assignment.task;
            let pairs = [
                (
                    "provider",
                    slot.map(|slot| slot.provider.as_str()),
                    Some(assignment.provider.as_str()),
                ),
                (
                    "model",
                    slot.map(|slot| slot.model.as_str()),
                    Some(assignment.model.as_str()),
                ),
                (
                    "base_url",
                    slot.map(|slot| slot.base_url.as_str()),
                    if !assignment.base_url.is_empty() {
                        Some(assignment.base_url.as_str())
                    } else if assignment.provider == "auto"
                        && slot.is_some_and(|slot| !slot.base_url.is_empty())
                    {
                        Some("")
                    } else {
                        None
                    },
                ),
            ];
            for (name, before, after) in pairs {
                let Some(after) = after else { continue };
                if before == Some(after) {
                    continue;
                }
                let key = format!("auxiliary.{task}.{name}");
                out.push(line(&key, stored(&key, before), shown_text(&key, after)));
            }
            let key = format!("auxiliary.{task}.reasoning_effort");
            let before = slot.and_then(|slot| slot.reasoning_effort.as_deref());
            match &assignment.reasoning_effort {
                EffortChange::Omit => {}
                EffortChange::Inherit => {
                    if before.is_some_and(|word| !word.is_empty()) {
                        out.push(line(&key, stored(&key, before), text::NOT_SET.to_owned()));
                    }
                }
                EffortChange::Level(level) => {
                    out.push(line(&key, stored(&key, before), level.as_wire().to_owned()));
                }
            }
        }
        out
    }

    fn subagent_diff(&self) -> Vec<DiffLine> {
        let before = self.sub_base();
        let after = self.sub_work();
        let patch = subagent_patch(&before, &after);
        let Some(delegation) = patch.get("delegation").and_then(Value::as_object) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for name in [
            "provider",
            "model",
            "base_url",
            "api_key",
            "api_mode",
            "reasoning_effort",
        ] {
            let Some(next) = delegation.get(name) else {
                continue;
            };
            let key = format!("delegation.{name}");
            let was = self.raw_delegation(name);
            let old = show(&key, was.as_ref());
            let new = show(&key, Some(next));
            let unchanged = was
                .as_ref()
                .map_or_else(|| next.as_str() == Some(""), |old_value| old_value == next);
            if !unchanged {
                out.push(line(&key, old, new));
            }
        }
        if delegation.contains_key("fallback_providers") {
            out.extend(chain_lines(
                "delegation.fallback_providers",
                chain_entries(&before.chain),
                chain_entries(&after.chain),
                "(inherits the main chain)",
            ));
        } else if let ChainMode::Own(entries) = &after.chain {
            for (index, entry) in entries.iter().enumerate() {
                if entry.was_string {
                    out.push(note(text::string_diff(index + 1)));
                }
            }
        }
        out
    }

    fn moa_diff(&self) -> Vec<DiffLine> {
        let Some(work) = self.moa_work() else {
            return Vec::new();
        };
        let first = self.first_save();
        let loaded = self.moa_base();
        let mut out = Vec::new();
        let (before_default, before_active) = match (loaded, first) {
            (Some(config), false) => (
                Some(config.default_preset.as_str()),
                Some(config.active_preset.as_str()),
            ),
            _ => (None, None),
        };
        for (key, before, after) in [
            (
                "moa.default_preset",
                before_default,
                work.default_preset.as_str(),
            ),
            (
                "moa.active_preset",
                before_active,
                work.active_preset.as_str(),
            ),
        ] {
            if before != Some(after) {
                let old =
                    before.map_or_else(|| text::NOT_SET.to_owned(), |word| shown_text(key, word));
                out.push(line(key, old, shown_text(key, after)));
            }
        }
        let old_presets = match (loaded, first) {
            (Some(config), false) => config.presets.as_slice(),
            _ => &[],
        };
        for preset in &work.presets {
            match old_presets.iter().find(|old| old.name == preset.name) {
                None => out.push(note(format!("moa.presets.{}: added", preset.name))),
                Some(old) => {
                    let fields = changed_fields(old, preset);
                    if !fields.is_empty() {
                        out.push(note(format!(
                            "moa.presets.{}: changed ({})",
                            preset.name,
                            fields.join(", ")
                        )));
                    }
                }
            }
        }
        for old in old_presets {
            if !work.presets.iter().any(|preset| preset.name == old.name) {
                out.push(note(format!("moa.presets.{}: removed", old.name)));
            }
        }
        if first {
            out.push(note(text::FIRST_SAVE_DIFF.to_owned()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_helper_values_are_masked_like_new_ones() {
        let shown = stored(
            "auxiliary.vision.base_url",
            Some("https://user:secret@example.invalid/v1"),
        );
        assert!(!shown.contains("secret"));
        assert!(shown.contains(text::KEY_SET));
        assert_eq!(
            stored("auxiliary.vision.api_key", Some("abc")),
            text::KEY_SET
        );
        assert_eq!(stored("auxiliary.vision.model", None), text::NOT_SET);
    }
}
