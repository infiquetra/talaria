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
