//! Mixture of Agents checks and wording.

use serde_json::Map;

use super::{MoaConfigView, MoaPresetView, SlotView};

/// Hermes's own checks, in Hermes's order and wording, so the page refuses what Hermes would.
/// Per preset: each reference with a problem, then the missing-complete-reference line when
/// every reference has one (an empty list too), then the aggregator. The timeout and policy
/// checks come last for the preset.
pub fn validate_moa(config: &MoaConfigView) -> Vec<String> {
    let mut problems = Vec::new();
    for preset in &config.presets {
        let name = &preset.name;
        let mut broken = 0;
        for (index, slot) in preset.references.iter().enumerate() {
            if let Some(problem) = slot_problem(slot) {
                broken += 1;
                problems.push(format!(
                    "preset '{name}' reference {}: {problem}",
                    index + 1
                ));
            }
        }
        if broken == preset.references.len() {
            problems.push(format!(
                "preset '{name}': needs at least one complete reference model"
            ));
        }
        if let Some(problem) = slot_problem(&preset.aggregator) {
            problems.push(format!("preset '{name}' aggregator: {problem}"));
        }
        if let Some(timeout) = preset.reference_timeout
            && !(timeout.abs() < f64::INFINITY && timeout > 0.0)
        {
            problems.push(format!(
                "preset '{name}': the advisor timeout must be a number above 0"
            ));
        }
        if !matches!(preset.degraded_reference_policy.as_str(), "loud" | "silent") {
            problems.push(format!(
                "preset '{name}': the advisor failure policy must be loud or silent"
            ));
        }
    }
    problems
}

fn slot_problem(slot: &SlotView) -> Option<String> {
    let provider = slot.provider.trim();
    let model = slot.model.trim();
    if provider.is_empty() && model.is_empty() {
        Some("provider and model are required".to_owned())
    } else if provider.is_empty() {
        Some("provider is required".to_owned())
    } else if model.is_empty() {
        Some(format!(
            "model is required (provider '{provider}' has no model selected)"
        ))
    } else if provider.eq_ignore_ascii_case("moa") {
        Some(
            "the Mixture of Agents provider cannot be used inside a preset (recursive MoA)"
                .to_owned(),
        )
    } else {
        None
    }
}

/// How often advisors run, in words. `every_n:1` is every step; anything unparseable is the
/// default of once per user turn.
pub fn fanout_words(fanout: &str) -> String {
    match parse_fanout(fanout) {
        Fanout::UserTurn => "once per user turn".to_owned(),
        Fanout::PerIteration => "every tool step".to_owned(),
        Fanout::EveryN(count) => format!("every {count} tool steps"),
    }
}

pub(super) enum Fanout {
    UserTurn,
    PerIteration,
    EveryN(u32),
}

pub(super) fn parse_fanout(fanout: &str) -> Fanout {
    match fanout.trim() {
        "per_iteration" => Fanout::PerIteration,
        other => match other.strip_prefix("every_n:").map(str::parse::<u32>) {
            Some(Ok(1)) => Fanout::PerIteration,
            Some(Ok(count)) if count >= 2 => Fanout::EveryN(count),
            _ => Fanout::UserTurn,
        },
    }
}

pub(super) fn empty_slot() -> SlotView {
    SlotView {
        provider: String::new(),
        model: String::new(),
        reasoning_effort: None,
        enabled: Some(true),
        extra: Map::new(),
    }
}

pub(super) fn new_preset(name: &str) -> MoaPresetView {
    MoaPresetView {
        name: name.to_owned(),
        enabled: true,
        references: Vec::new(),
        aggregator: SlotView {
            enabled: None,
            ..empty_slot()
        },
        reference_temperature: None,
        aggregator_temperature: None,
        reference_timeout: None,
        degraded_reference_policy: "loud".to_owned(),
        fanout: "user_turn".to_owned(),
        extra: Map::new(),
    }
}

/// `Ok(trimmed name)` or the message to show.
pub(super) fn check_name(config: &MoaConfigView, raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    let taken = config.presets.iter().any(|preset| preset.name == name);
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) || taken {
        Err(super::text::CHOOSE_ANOTHER)
    } else {
        Ok(name.to_owned())
    }
}

pub(super) fn slot_words(slot: &SlotView) -> String {
    if slot.provider.trim().is_empty() && slot.model.trim().is_empty() {
        return super::text::CHOOSE_MODEL.to_owned();
    }
    let model = if slot.model.trim().is_empty() {
        "(no model)"
    } else {
        slot.model.as_str()
    };
    let provider = if slot.provider.trim().is_empty() {
        "(no provider)"
    } else {
        slot.provider.as_str()
    };
    format!("{model} (via {provider})")
}

pub(super) fn effort_words(slot: &SlotView) -> String {
    match slot.reasoning_effort.as_deref() {
        Some(level) if !level.is_empty() => level.to_owned(),
        _ => super::text::INHERITED.to_owned(),
    }
}

pub(super) fn number_words(value: Option<f64>, none: &str, unit: &str) -> String {
    value.map_or_else(|| none.to_owned(), |number| format!("{number}{unit}"))
}

/// The fields of a preset that differ, named as Hermes names them.
pub(super) fn changed_fields(before: &MoaPresetView, after: &MoaPresetView) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if before.enabled != after.enabled {
        fields.push("enabled");
    }
    if before.references != after.references {
        fields.push("reference_models");
    }
    if before.aggregator != after.aggregator {
        fields.push("aggregator");
    }
    if before.reference_temperature != after.reference_temperature {
        fields.push("reference_temperature");
    }
    if before.aggregator_temperature != after.aggregator_temperature {
        fields.push("aggregator_temperature");
    }
    if before.reference_timeout != after.reference_timeout {
        fields.push("reference_timeout");
    }
    if before.degraded_reference_policy != after.degraded_reference_policy {
        fields.push("degraded_reference_policy");
    }
    if before.fanout != after.fanout {
        fields.push("fanout");
    }
    if before.extra != after.extra {
        fields.push("other settings");
    }
    fields
}
