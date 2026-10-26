//! The Talaria texts the phase 3 steps depend on.
//!
//! Every line is the text named by issue #248. Each constant names its source as the
//! issue number and the screen it comes from. Keep these literal texts separate from the
//! runner's timeout and failure explanations.

// #242, the empty subagent panel
/// The empty subagent panel.
pub const NO_SUBAGENTS: &str = "No subagents in this session yet.";
// #242, an unknown /agents option
/// The refusal for an unknown `/agents` option.
pub const AGENTS_UNKNOWN_OPTION: &str =
    "Unknown /agents option: frobnicate. Use /agents, /agents pause or /agents resume.";

// #243, the empty process list
/// The empty background-process list.
pub const NO_PROCESSES: &str = "No background processes in this session.";
// #243, /bg with no prompt
/// The start of the `/bg` usage line.
pub const BG_USAGE_PREFIX: &str = "Usage: /bg <prompt>";

// #240, the foreground row
/// The suffix on the foreground row of the fleet view.
pub const HERE_SUFFIX: &str = " (here)";
// #240, a filter that matches nothing
/// The fleet-view line when the filter matches nothing.
pub const NO_SESSIONS_MATCH: &str = "No sessions match \"zzz-no-match\".";
// #240, the deny line on the approval card
/// The suffix of a numbered deny choice. The digit stands in front of it.
pub const DENY_SUFFIX: &str = ". deny";
// #240, the approval row detail
/// The start of the approval detail on a fleet row.
pub const APPROVAL_DETAIL_PREFIX: &str = "approval: ";

// #244, the status-line count
/// The mark in front of the status-line count.
pub const NEEDS_YOU_MARK: &str = "\u{2691} ";
// #244, the status-line count
/// The words after the count on the status line.
pub const NEEDS_YOU_SEGMENT_SUFFIX: &str = " needs you";
// #244, the notice when a request waited while Talaria was closed
/// The start-up notice for one waiting background session.
pub const NEEDS_YOU_NOTICE: &str = "1 background session needs you; press alt+a to see it.";

// #241, a background launch
/// The status field of a launch that started.
pub const STATUS_LAUNCHED: &str = "\"status\":\"launched\"";
// #241, the refused -p combination
/// The refusal printed when `--bg` is combined with `-p`.
pub const BG_CANNOT_COMBINE: &str = "talaria --bg cannot be combined with -p.";
// #241, the worktree receipt
/// The start of the worktree receipt, through `talaria/`.
pub const WORKTREE_PREFIX: &str = "Worktree: branch talaria/";
// #241, the worktree receipt
/// The middle of the worktree receipt, in front of the commit.
pub const WORKTREE_FROM: &str = " from origin/main at ";
// #241, the launched session line
/// The start of the plain-text launch line.
pub const LAUNCHED_PREFIX: &str = "Launched session ";
// #241, the launched session line
/// The middle of the plain-text launch line, in front of the profile.
pub const LAUNCHED_IN_PROFILE: &str = " in profile ";

// #239, an unserved profile
/// The refusal for a profile this gateway does not serve.
pub const PROFILE_NOT_SERVED: &str = "profile \"nosuch\" is not served by this gateway";
// #239, an empty Hermes home
/// The refusal when no gateway is running for the Hermes home.
pub const NO_GATEWAY_HOME: &str = "no Hermes gateway is running for this Hermes home";
// #239, the group headers
/// The group header for sessions that need an answer.
pub const HEADER_NEEDS_YOU: &str = "NEEDS YOU";
// #239, the group headers
/// The group header for sessions that are still working.
pub const HEADER_WORKING: &str = "WORKING";
// #239, the group headers that stay out of the busy-fleet reason
/// The group header for idle sessions.
pub const HEADER_IDLE: &str = "IDLE";
// #239, the group headers that stay out of the busy-fleet reason
/// The group header for finished sessions.
pub const HEADER_DONE: &str = "DONE";

// #246, the resumed banner
/// The start of the line `attach` prints when a session opens.
pub const RESUMED_PREFIX: &str = "\u{2500}\u{2500} Resumed:";
// #246, a reference that matches nothing
/// The start of the line `attach` prints when nothing matches, through the opening quote.
pub const NO_SESSION_MATCHES_PREFIX: &str = "talaria: no session matches \"";
// #246, the new-pane notice
/// The start of the Herdr open-in-pane notice, through the opening quote.
pub const OPENED_PREFIX: &str = "Opened \"";
// #246, the new-pane notice
/// The end of the Herdr open-in-pane notice.
pub const OPENED_HERDR_SUFFIX: &str = "\" in a new Herdr pane.";
// #246, the tmux open-in-pane notice, which the Herdr reader must not accept
/// The end of the tmux open-in-pane notice.
pub const OPENED_TMUX_SUFFIX: &str = "\" in a new tmux pane.";
// #246, outside a multiplexer
/// The notice when Talaria is not inside Herdr or tmux.
pub const NOT_IN_HERDR: &str = "Not inside Herdr or tmux";
