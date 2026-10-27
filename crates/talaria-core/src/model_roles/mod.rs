//! Model roles for one Hermes profile: main, fallbacks, helpers, subagents, mixture of agents.

mod fallbacks;
mod helpers;
mod main_role;
mod moa;
mod picker;
mod review;
mod state;
mod subagents;
mod text;
mod types;

pub use fallbacks::{fallback_patch, route_identity};
pub use helpers::stale_helpers;
pub use main_role::main_config_patch;
pub use moa::{fanout_words, validate_moa};
pub use picker::{PickerStep, RouteCatalog, RouteProvider, RouteRow, filter_routes};
pub use review::review_lines;
pub use state::ModelRolesState;
pub use subagents::subagent_patch;
pub use text::{
    APPLIES_MAIN, APPLIES_SHORT, BUILTIN_LABELS, BUILTIN_TASKS, CONFIRM_PROMPT, CONFLICT,
    DUPLICATE, ENTRY_NEEDS_MODEL, FIRST_SAVE_DIFF, FOLLOWS_MAIN, HELPERS_PARTIAL, HELPERS_UNKNOWN,
    IGNORED_MARK, INHERITED, KEY_SET, LAST_PRESET, LOADING_MODELS, MAIN_PARTIAL, MOA_NOTICE,
    NO_ANSWER, NO_FALLBACKS, NO_MAIN, NOT_CONNECTED, NOT_IN_LIST, NOT_SERVED, NOT_SET, NOT_SET_UP,
    PENDING_PROMPT, RESTART, REVIEW_FOOTER, RUNNING_FOOTER, SAME_AS_MAIN, SAVED_DEFAULT,
    SAVED_NOTICE, STALE_OFFER, STRING_DIFF, TAB_ROW, TITLE_PREFIX, UNAVAILABLE, UNKNOWN, USAGE,
};
pub use types::*;
