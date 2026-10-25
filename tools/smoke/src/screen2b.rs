//! Pure screen readers for the phase 2b steps.
//!
//! Stage 0 stubs: every function panics `not implemented`.

/// The model picker overlay, as one screen shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerState {
    /// Whether the picker title line is shown.
    pub title: bool,
    /// The provider group headers, in order.
    pub groups: Vec<String>,
    /// Whether a current-model row is shown.
    pub has_current: bool,
    /// The selected row's text, when a status row is selected.
    pub selected_row: Option<String>,
    /// Whether the not-set-up banner is shown.
    pub not_set_up: bool,
    /// Whether the effort footer is shown.
    pub effort_footer: bool,
    /// The no-match line, when the picker shows one.
    pub no_match: Option<String>,
}

/// The `/context` header line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextHeader {
    /// The model word of the header.
    pub model: String,
    /// Whether the header ends with a percent in parentheses.
    pub percent: bool,
    /// The header's line index.
    pub line_index: usize,
}

/// The category rows under a context header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextRows {
    /// Whether the category row is shown.
    pub categories: bool,
    /// Whether the free-space row is shown.
    pub free_space: bool,
    /// Whether the copied-source row is shown.
    pub source: bool,
}

/// The `/usage` block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageBlock {
    /// The title's line index.
    pub title_index: usize,
    /// Whether the tokens row is shown.
    pub tokens_row: bool,
    /// Whether the cost row is shown.
    pub cost_row: bool,
}

/// The rewind overlay's prompt list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewindList {
    /// Whether the overlay is open.
    pub open: bool,
    /// The listed prompt numbers, in screen order.
    pub prompts: Vec<u32>,
    /// The selected prompt number, if one is selected.
    pub selected: Option<u32>,
}

/// The three facts of a headless `--print` run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintCheck {
    /// Whether the printed text is exactly the answer.
    pub text_ok: bool,
    /// Whether the error stream is empty.
    pub err_empty: bool,
    /// Whether the exit marker says zero.
    pub exit_ok: bool,
    /// Whether the run is a no-gateway failure.
    pub not_started: bool,
}

/// Reads the model picker overlay of one screen.
pub fn picker_state(_text: &str) -> PickerState {
    todo!("not implemented")
}

/// Reads the `/context` header line of one screen.
pub fn context_header(_text: &str) -> Option<ContextHeader> {
    todo!("not implemented")
}

/// Reads the category rows under a context header.
pub fn context_rows(_text: &str, _header_line: usize) -> ContextRows {
    todo!("not implemented")
}

/// Reads the `/usage` block of one screen.
pub fn usage_block(_text: &str) -> Option<UsageBlock> {
    todo!("not implemented")
}

/// Reads the rewind overlay's prompt list of one screen.
pub fn rewind_list(_text: &str) -> RewindList {
    todo!("not implemented")
}

/// The text after `approvals: ` on the status line, if the segment is shown.
pub fn status_approvals(_visible: &str) -> Option<String> {
    todo!("not implemented")
}

/// The line index of the rewound marker for one prompt number.
pub fn rewound_line(_text: &str, _prompt: u32) -> Option<usize> {
    todo!("not implemented")
}

/// The queued count of one screen, if a queued indicator is shown.
pub fn queue_indicator(_text: &str) -> Option<u32> {
    todo!("not implemented")
}

/// The folder segment of the status line, if one is shown.
pub fn folder_segment(_visible: &str) -> Option<String> {
    todo!("not implemented")
}

/// The lines after the last line holding the needle.
pub fn lines_after_last<'a>(_text: &'a str, _needle: &str) -> Vec<&'a str> {
    todo!("not implemented")
}

/// Checks the three facts of a headless `--print` run.
pub fn print_check(_out: &str, _err: &str, _exit: Option<i32>) -> PrintCheck {
    todo!("not implemented")
}
