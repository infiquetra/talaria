//! Text shown by the configuration screens.
pub const SETTINGS_TITLE_FMT: &str = "Settings · profile ";
pub const SETTINGS_NOT_SET: &str = "(not set)";
pub const SETTINGS_NUMBER_REFUSAL: &str = "Enter a whole number.";
pub const NO_CHANGES_TO_REVIEW: &str = "No changes to review.";
pub const SETTINGS_SECRET_REFUSAL: &str =
    "delegation.api_key is a secret; set it in /config, not on the command line. Nothing was sent.";
pub const SETTINGS_CATEGORIES: [&str; 3] = ["general", "agent", "terminal"];
pub const SETTINGS_SET: &str = "set";
pub const SETTINGS_DEFAULT: &str = "default";
pub const SETTINGS_RESTART_FLAG: &str = "may need a restart";
pub const PROVIDERS_TITLE_FMT: &str = "Providers · profile ";
pub const PROVIDERS_NO_KEY: &str = "no key";
pub const PROVIDERS_ADD_ENDPOINT: &str = "+ Add an endpoint";
pub const PROVIDERS_KEY_CARD_TITLE_FMT: &str = "API key for ";
pub const PROVIDERS_KEY_MASK: &str = "••••••••••••••••••••••••••••••••••••••••…";
pub const PROVIDERS_OLLAMA: &str = "Ollama Cloud";
pub const PROVIDERS_OPENROUTER: &str = "OpenRouter";
pub const PROVIDERS_READY: &str = "ready ·";
pub const PROVIDERS_KEY_TAIL: &str = "key …";
pub const PROVIDERS_SIGNED_IN: &str = "signed in";
pub const PROVIDERS_NOT_SIGNED_IN: &str = "not signed in";
pub const MODELS_TITLE_FMT: &str = "Model roles - profile ";
pub const MODELS_PRESET_INCOMPLETE_FMT: &str =
    "preset 'uat-test' reference 1: provider and model are required";
pub const MODELS_REVIEW_TITLE_FMT: &str = "Review changes to ";
pub const MODELS_UNSAVED_PROMPT_FMT: &str =
    "You have unapplied changes in Mixture of Agents. a apply · d discard · Esc stay";
pub const MODELS_FAILED: &str = "Failed";
pub const MODELS_HELPERS_FIRST: &str = "Vision";
pub const MODELS_HELPERS_TWELFTH: &str = "Curator";
pub const PROFILES_HEADER: &str = "Profiles (";
pub const PROFILES_SERVED: &str = "served";
pub const PROFILES_NAME_REFUSALS: [&str; 3] = [
    "Use lower-case letters, digits, - or _, starting with a letter or digit, up to 64 characters.",
    "default is reserved by Hermes. Choose another name.",
    "A profile named fixture already exists.",
];
pub const PROFILES_UNKNOWN_FMT: &str = "talaria profiles: no profile named ";
pub const PROFILES_DETAIL_LAUNCH: &str = "talaria --profile fixture";
pub const PROFILES_DETAIL_SKILLS: &str = "Skills:";
pub const DOCTOR_SUMMARY_RE: &str = r"Doctor: ([0-9]+) ok, ([0-9]+) warnings, ([0-9]+) failures";
pub const DOCTOR_FORMAT: &str = "talaria-doctor-v1";
pub const DOCTOR_FIX_NO_GATEWAY: &str =
    "Fix: Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.";
pub const DOCTOR_UNKNOWN_PROFILE_FMT: &str = "the gateway never discovered a profile named ";
pub const DOCTOR_RUNNING: &str = "running: ";
pub const DOCTOR_BUILT_FOR: &str = "built for: ";
pub const DOCTOR_RESULT: &str = "Result:";
pub const WRITE_BANNERS: [&str; 3] = ["Saved ", "Not restarted", "Gateway restarted"];
