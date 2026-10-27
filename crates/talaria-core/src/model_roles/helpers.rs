//! Helper slots that no longer match the main provider.

use super::{AuxSlot, StaleSlot, text};

/// Slots pinned to a provider other than the main one. An empty main provider makes none stale,
/// and a slot on a local endpoint never is.
pub fn stale_helpers(slots: &[AuxSlot], main_provider: &str) -> Vec<StaleSlot> {
    let main = main_provider.trim().to_lowercase();
    if main.is_empty() {
        return Vec::new();
    }
    slots
        .iter()
        .filter(|slot| {
            let provider = slot.provider.trim().to_lowercase();
            !slot.local_endpoint
                && !matches!(provider.as_str(), "" | "auto" | "main")
                && provider != main
        })
        .map(|slot| StaleSlot {
            task: slot.task.clone(),
            provider: slot.provider.clone(),
            model: slot.model.clone(),
        })
        .collect()
}

/// True when the slot takes its route from the main model.
pub(super) fn follows_main(provider: &str) -> bool {
    matches!(
        provider.trim().to_lowercase().as_str(),
        "" | "auto" | "main"
    )
}

/// The label and hint shown for a slot: Desktop's words for the built-ins, the plugin's own
/// otherwise, and the bare key for a slot this Talaria does not know.
pub(super) fn label_and_hint(slot: &AuxSlot) -> (String, Option<String>) {
    if let Some(index) = text::BUILTIN_TASKS
        .iter()
        .position(|task| *task == slot.task)
    {
        let (label, hint) = text::BUILTIN_LABELS[index];
        return (label.to_owned(), Some(hint.to_owned()));
    }
    let label = slot
        .label
        .as_deref()
        .filter(|label| !label.trim().is_empty())
        .unwrap_or(slot.task.as_str());
    (
        label.to_owned(),
        slot.hint.clone().filter(|hint| !hint.trim().is_empty()),
    )
}
