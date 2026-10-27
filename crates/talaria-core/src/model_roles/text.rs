//! Every string the model roles page shows.

#![allow(dead_code)]

pub const USAGE: &str = "Usage: /models";
pub const NOT_SERVED: &str =
    "Profile {name} is not served by this gateway, so its settings cannot be opened here.";
pub const NOT_CONNECTED: &str = "Model roles need the Hermes gateway, and Talaria is not connected. \
Ctrl+R retry · Esc close";
pub const TITLE_PREFIX: &str = "Model roles - profile ";
pub const TAB_ROW: &str = "1 Main  2 Fallbacks  3 Helpers  4 Subagents  5 Mixture of Agents";
pub const APPLIES_MAIN: &str =
    "Applies to new sessions. Running sessions keep their model; change a session with /model.";
pub const APPLIES_SHORT: &str = "Applies to new sessions.";
pub const RUNNING_FOOTER: &str = "Sessions already running may keep their current setting.";
pub const NO_MAIN: &str = "No main model set. Choose one with Enter.";
pub const NO_FALLBACKS: &str = "No fallback models. When the main model fails, the turn fails.";
pub const SAVED_DEFAULT: &str = "Talaria starts new sessions with {provider}/{model} (your Talaria default), which overrides this.";
pub const CONFIRM_PROMPT: &str = "Use this model anyway? y/n";
pub const PENDING_PROMPT: &str =
    "You have unapplied changes in {section}. a apply · d discard · Esc stay";
pub const CONFLICT: &str = "Changed outside Talaria since you opened this page";
pub const UNKNOWN: &str =
    "The result of the last change is unknown. These are the values Hermes holds now.";
pub const STALE_OFFER: &str =
    "These helpers still use another provider: {slots}. Make them follow the main model? y/n";
pub const SAVED_NOTICE: &str = "Saved. Applies to new sessions.";
pub const REVIEW_FOOTER: &str = "Enter apply  Esc back";
pub const KEY_SET: &str = "[key set]";
pub const NOT_SET: &str = "(not set)";
pub const NOT_IN_LIST: &str = "(not in the model list)";
pub const NOT_SET_UP: &str = "not set up: add it under Providers";
pub const LOADING_MODELS: &str = "loading models...";
pub const UNAVAILABLE: &str = "This Hermes has no {route}.";
pub const NO_ANSWER: &str = "Hermes did not answer. Ctrl+R to retry.";
pub const RESTART: &str = "Hermes needs a restart before it can change models.";
pub const DOCTOR_RESTART: &str = "Run the doctor to restart the gateway.";
pub const ENTRY_NEEDS_MODEL: &str = "Entry {n} needs a model";
pub const STRING_DIFF: &str = "Entry {n} was ignored by Hermes; saving makes it active.";
pub const IGNORED_MARK: &str = "(ignored by Hermes until saved)";
pub const SAME_AS_MAIN: &str = "same as main model";
pub const DUPLICATE: &str = "duplicate: Hermes skips it";
pub const LAST_PRESET: &str = "A profile keeps at least one preset.";
pub const MOA_NOTICE: &str = "No presets of your own. Hermes shows its example preset below; \
saving writes it into this profile, and enabled presets then appear as models under \
\"Mixture of Agents\".";
pub const FIRST_SAVE_DIFF: &str =
    "This writes Mixture of Agents presets into this profile for the first time.";
pub const FOLLOWS_MAIN: &str = "follows main model";
pub const INHERITED: &str = "inherited";
pub const HELPERS_PARTIAL: &str = "Saved: {saved}. Not saved: {rest}.";
pub const HELPERS_UNKNOWN: &str = "Saved: {saved}. Unknown: {unknown}. Not saved: {rest}.";
pub const MAIN_PARTIAL: &str = "Saved: model. Not saved: {rest}.";

pub const BUILTIN_TASKS: [&str; 12] = [
    "vision",
    "compression",
    "skills_hub",
    "approval",
    "mcp",
    "title_generation",
    "review",
    "voice_chat",
    "triage_specifier",
    "kanban_decomposer",
    "profile_describer",
    "curator",
];

pub const BUILTIN_LABELS: [(&str, &str); 12] = [
    ("Vision", "Image analysis"),
    ("Compression", "Context compaction"),
    ("Skills hub", "Skill search"),
    ("Approval", "Smart auto-approve"),
    ("MCP", "MCP tool routing"),
    ("Title gen", "Session titles"),
    ("Review", "/review reviewer subagent"),
    ("Voice chat", "Spoken voice-mode replies"),
    ("Triage specifier", "Kanban spec fleshing"),
    ("Kanban decomposer", "Task decomposition"),
    ("Profile describer", "Auto profile descriptions"),
    ("Curator", "Skill-usage review"),
];

pub(super) const LOADING: &str = "Loading";
pub(super) const NO_CHANGES: &str = "No changes to apply.";
pub(super) const CHECKING: &str = "Checking for outside changes...";
pub(super) const APPLYING: &str = "Applying...";
pub(super) const LEGACY_RULE: &str =
    "Legacy entry (fallback_model): edit with the hermes fallback command";
pub(super) const NO_USABLE_CHAIN: &str =
    "Fallback: own chain (no usable entry, so Hermes ignores it)";
pub(super) const SUBAGENT_HINT: &str = "Other subagent limits are in Settings.";
pub(super) const NO_REFERENCE_ENABLED: &str = "No reference is enabled.";
pub(super) const CHOOSE_ANOTHER: &str = "Choose another name";
pub(super) const CATALOG_FAILED: &str = "Could not load the model list. Ctrl+R to retry.";
pub(super) const REFRESHING: &str = "refreshing...";
pub(super) const CHANGED_MARK: &str = "(changed)";
pub(super) const NO_STALE: &str = "No helper uses another provider.";
pub(super) const SIGN_IN: &str = "Talaria could not sign in to the dashboard API.";
pub(super) const DOCTOR: &str = "Run the doctor.";
pub(super) const BAD_REPLY: &str = "Hermes sent a reply Talaria could not read. Ctrl+R to retry.";
pub(super) const REFUSED: &str = "Hermes refused this change:";
pub(super) const PROBLEM: &str = "Hermes found a problem:";
pub(super) const PROBLEM_BARE: &str = "Hermes found a problem with the request.";
pub(super) const NOT_SAVED_SERVER: &str = "Hermes could not save the change.";
pub(super) const READ_BACK_FAILED: &str =
    "The change was sent, but Hermes did not answer the read-back. Ctrl+R to retry.";
pub(super) const CHECK_FAILED: &str =
    "Talaria could not check for outside changes, so nothing was written.";
pub(super) const MAIN_ROW_EMPTY: &str = "(not set)";
pub(super) const CHOOSE_MODEL: &str = "(choose a model)";
pub(super) const EFFORT_CHOICES: [&str; 9] = [
    "inherit", "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];
pub(super) const FANOUT_CHOICES: [&str; 3] = [
    "once per user turn",
    "every tool step",
    "every N tool steps",
];
pub(super) const KEYS_HELP: [&str; 4] = [
    "1-5 Tab section  Up Down Home End PgUp PgDn move  Left Right pane  Enter open or change",
    "e effort  c context  i inherit  a add  x remove  K J move  Space toggle  D default  m chain",
    "R reset stale helpers  Ctrl+S review and apply  u discard  Ctrl+R reload",
    "Esc close or back  ? close this list",
];

pub(super) fn not_served(name: &str) -> String {
    NOT_SERVED.replace("{name}", name)
}

pub(super) fn section_name(section: super::Section) -> &'static str {
    match section {
        super::Section::Main => "Main",
        super::Section::Fallbacks => "Fallbacks",
        super::Section::Helpers => "Helpers",
        super::Section::Subagents => "Subagents",
        super::Section::Moa => "Mixture of Agents",
    }
}

pub(super) fn tab_label(section: super::Section) -> String {
    let number = match section {
        super::Section::Main => 1,
        super::Section::Fallbacks => 2,
        super::Section::Helpers => 3,
        super::Section::Subagents => 4,
        super::Section::Moa => 5,
    };
    format!("{number} {}", section_name(section))
}

pub(super) fn unavailable(route: &str) -> String {
    UNAVAILABLE.replace("{route}", route)
}

pub(super) fn entry_needs_model(number: usize) -> String {
    ENTRY_NEEDS_MODEL.replace("{n}", &number.to_string())
}

pub(super) fn string_diff(number: usize) -> String {
    STRING_DIFF.replace("{n}", &number.to_string())
}

pub(super) fn pending_prompt(section: &str) -> String {
    PENDING_PROMPT.replace("{section}", section)
}

pub(super) fn stale_offer(slots: &str) -> String {
    STALE_OFFER.replace("{slots}", slots)
}

pub(super) fn count(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
pub(super) const UNAPPLIED: &str = "Unapplied changes. Ctrl+S to review, u to discard.";
