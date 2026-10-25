//! Black-box live smoke test: drives the built binary through the Herdr command line.

pub mod clock;
pub mod config;
pub mod report;
pub mod screen;
pub mod screen2a;
pub mod scrub;
pub mod steps;
pub mod steps2a;
pub mod suite;
pub mod terminal;
pub mod tmux;
pub mod wording;

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
pub use scrub::{Scrubber, public_safety_violations, public_safety_violations_2a};
pub use steps::{CleanupError, TabGuard, run_smoke};
pub use steps2a::run_smoke_2a;
pub use suite::Suite;
pub use terminal::{HerdrCli, NewTab, PaneId, ReadSource, TabId, TermError, Terminal};
pub use tmux::{Paster, TmuxCli, TmuxGuard};
