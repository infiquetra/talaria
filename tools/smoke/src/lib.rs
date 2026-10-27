//! Black-box live smoke test: drives the built binary through the Herdr command line.

pub mod clock;
pub mod config;
pub mod report;
pub mod screen;
pub mod screen2a;
pub mod screen2b;
pub mod screen3;
pub mod screen4a;
pub mod scrub;
pub mod steps;
pub mod steps2a;
pub mod steps2b;
pub mod steps3;
pub mod steps4a;
pub mod suite;
pub mod suite4a;
pub mod terminal;
pub mod tmux;
pub mod wording;
pub mod wording2b;
pub mod wording3;
pub mod wording4a;

pub use clock::{Clock, ManualClock, SystemClock};
pub use config::{Phase2aConfig, SmokeConfig, launch_line, launch_line_env};
pub use report::{PaneCapture, SmokeReport, StepOutcome};
pub use screen::{
    Card, Token, approval_card, count_token, counting_values, exit_code, highest_number, settled,
};
pub use screen2a::{
    AgentState, EditHeader, Mark, MentionRow, Pager, Palette, PaletteRow, PasteLabel, ShellCell,
    SideCell, TaskView, ThemeReport, ToolHeader, agent_state, edit_header, file_label, has_debris,
    has_diff_rows, mention_list, pager_state, palette_state, paste_labels, prompt_after,
    rows_after, shell_cell, side_cell, task_records, task_view, theme_report, tool_header,
};
pub use screen2b::{
    ContextHeader, ContextRows, PickerState, PrintCheck, RewindList, UsageBlock, context_header,
    context_rows, folder_segment, lines_after_last, picker_state, print_check, queue_indicator,
    rewind_list, rewound_line, status_approvals, usage_block,
};
pub use screen3::{
    LaunchResult, deny_digit, fleet_headers, fleet_row_state, has_here_row, launch_result,
    launched_id, needs_you_ids, needs_you_segment, opened_notice, worktree_line,
};
pub use scrub::{Scrubber, public_safety_violations, public_safety_violations_2a};
pub use steps::{CleanupError, TabGuard, run_smoke};
pub use steps2a::run_smoke_2a;
pub use steps2b::run_smoke_2b;
pub use steps3::{
    KEY_JUMP, KEY_PUSH, KEY_STOP, KEY_SUBAGENTS, KEY_SUBMIT_FALLBACK, STEP_LIMITS, STEP_ORDER,
    run_smoke_3,
};
pub use suite::Suite;
pub use terminal::{HerdrCli, NewTab, PaneId, ReadSource, TabId, TermError, Terminal};
pub use tmux::{Paster, TmuxCli, TmuxGuard};
pub use wording2b::{
    APPROVALS_MANUAL, APPROVALS_OFF, CD_DONE, CD_MISSING, CD_MOVING, CD_SHOW, COMPACT_REFUSAL,
    COMPRESSED, CTX_ARG, CTX_EMPTY, CTX_HEADER, CTX_ROWS, EXPORT_BAD, EXPORTED, EXPORTED_REDACTED,
    FAST_REFUSAL, LOOP_LINE, LOOP_USAGE, MODEL_REFUSAL, NO_GATEWAY, NOT_STARTED, NOTICE_OFF,
    PICKER_CURRENT_ROW, PICKER_EFFORT, PICKER_GROUP, PICKER_NO_MATCH, PICKER_NOT_SET_UP,
    PICKER_TITLE, PRINT_OK, QUEUE_INDICATOR, QUEUE_PREVIEW, RENAME_SHOW, RENAMED, REWIND_TITLE,
    REWOUND, STEER_IDLE, USAGE_EMPTY, USAGE_ROWS, USAGE_TITLE, USER_LINE_PREFIX,
};
pub use wording3::{
    AGENTS_UNKNOWN_OPTION, APPROVAL_DETAIL_PREFIX, BG_CANNOT_COMBINE, BG_USAGE_PREFIX, DENY_SUFFIX,
    FLEET_TITLE, HEADER_DONE, HEADER_IDLE, HEADER_NEEDS_YOU, HEADER_WORKING, HERE_SUFFIX,
    LAUNCHED_IN_PROFILE, LAUNCHED_PREFIX, NEEDS_YOU_MARK, NEEDS_YOU_NOTICE,
    NEEDS_YOU_SEGMENT_SUFFIX, NO_GATEWAY_HOME, NO_PROCESSES, NO_SESSION_MATCHES_PREFIX,
    NO_SESSIONS_MATCH, NO_SUBAGENTS, NOT_IN_HERDR, OPENED_HERDR_SUFFIX, OPENED_PREFIX,
    OPENED_TMUX_SUFFIX, PROFILE_NOT_SERVED, RESUMED_PREFIX, STATUS_LAUNCHED, WORKTREE_FROM,
    WORKTREE_PREFIX,
};
