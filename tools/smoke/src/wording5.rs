//! Exact text expected from workflow screens and commands.

// The empty workflow picker, specified by issue #268.
pub const WORKFLOWS_EMPTY: &str = "no runs yet — talaria workflow run starts one";
// The status with no skill installed, specified by issue #272.
pub const ULTRACODE_STATUS: &str =
    "Ultracode is off for this session. Skill: not found in profile fixture.";
// Built-in installation and listing, specified by issue #271.
pub const CREATED_FOLDER: &str = "created folder ";
pub const CREATED: &str = "created ";
pub const UNCHANGED: &str = "unchanged ";
pub const INSTALLED: &str = "installed";
pub const MISSING_TIERS: &str = "missing tiers:";
// The empty workflow list, specified by issue #269.
pub const EMPTY_LIST: &str = "No workflow runs found.";
// The empty run picker title; profiled run views have a separate title.
pub const WORKFLOWS_TITLE: &str = "Workflows";
// A selected run shows its profile in the separate progress view.
pub const WORKFLOW_RUN_TITLE: &str = "Workflows · profile ";
// The session status badge, specified by issue #272.
pub const ULTRACODE_BADGE: &str = "ultracode";
// The message composer and running-turn indicator, specified by issue #210.
pub const COMPOSER: &str = ">";
pub const COMPOSER_HINT: &str = "> Type a message. Enter sends, Shift+Enter adds a line.";
pub const THINKING: &str = "∴ thinking";
// The legacy keyboard protocol cannot distinguish Shift+Enter.
pub const COMPOSER_HINT_ALT: &str = "> Type a message. Enter sends, Alt+Enter adds a line.";
// The active-work label on the status line.
pub const WORKING: &str = "working";
// The stopped-gateway diagnostic, specified by issue #269.
pub const NO_GATEWAY: &str = "gateway not found";
// The built-in workflow names, specified by issue #271.
pub const BUILTINS: [&str; 4] = [
    "fix-failing-tests",
    "research",
    "review-branch",
    "ultracode",
];
