//! Pure helpers over phase 2a screen text, one per check the steps make.
//!
//! Every helper takes raw screen text and reads it the way the steps do: lines with
//! box-drawing characters removed and each line trimmed, except [`has_debris`], which looks
//! at the raw lines for the debris itself.
//!
//! Stage 0 stub: every public function panics `not implemented`.

use crate::Token;

/// The mark of a tool header: finished well or finished badly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// The `✓` mark.
    Success,
    /// The `✗` mark.
    Failure,
}

/// A parsed tool header row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolHeader {
    /// The row that holds the header.
    pub line_index: usize,
    /// The header's mark.
    pub mark: Mark,
    /// The exit code after the failure mark, if any.
    pub exit: Option<i32>,
}

/// A parsed edit header row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditHeader {
    /// The row that holds the header.
    pub line_index: usize,
    /// Whether the stat fragment is present.
    pub stat_ok: bool,
}

/// One palette row: the command name and its tag, if shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteRow {
    /// The command name.
    pub name: String,
    /// The right-aligned tag, if shown.
    pub tag: Option<String>,
}

/// The parsed palette: whether the footer is present and the rows in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Whether the footer row is present.
    pub footer: bool,
    /// The selectable rows in order.
    pub rows: Vec<PaletteRow>,
}

/// One mention row: the file and its folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionRow {
    /// The file's base name.
    pub file: String,
    /// The parent folder.
    pub folder: String,
}

/// A parsed collapsed-paste label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteLabel {
    /// The paste number.
    pub number: u32,
    /// The line count after the `+`.
    pub lines: u32,
}

/// A parsed shell cell: the header row and the output rows it owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellCell {
    /// The row that holds the header.
    pub header_index: usize,
    /// The output rows under the header.
    pub output_has: Vec<String>,
}

/// A parsed side-answer cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideCell {
    /// The row that holds the header.
    pub header_index: usize,
    /// The row that holds the footer, or the header row when the cell failed and carries
    /// no footer.
    pub footer_index: usize,
    /// The answer text between the header and the footer.
    pub answer: String,
    /// Whether the answer failed.
    pub failed: bool,
    /// Whether the answer is empty.
    pub empty: bool,
}

/// The parsed pager frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pager {
    /// Whether the pager is open.
    pub open: bool,
    /// Whether it follows new output.
    pub following: bool,
    /// The confirmed search: match index, match count, query.
    pub search: Option<(u32, u32, String)>,
    /// Whether the frame reports no match.
    pub no_match: bool,
}

/// A parsed theme report: which of the five lines is present, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeReport {
    /// One flag per line of [`crate::wording::THEME_LINES`], in order.
    pub present: [bool; 5],
}

/// The task panel: expanded, collapsed or absent, with the in-progress row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskView {
    /// The expanded panel and the in-progress row, if any.
    Expanded { active_row: Option<String> },
    /// The collapsed panel; the header line names the step in progress.
    Collapsed { active_row: Option<String> },
    /// No panel.
    Absent,
}

/// The agent state of a pane-state report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentState {
    /// Idle.
    Idle,
    /// Working.
    Working,
    /// Blocked.
    Blocked,
}

/// The tool header for `command`: the first line starting `⏺ Shell(<command>)` that holds
/// the success mark or the failure mark with an exit code.
pub fn tool_header(_text: &str, _command: &str) -> Option<ToolHeader> {
    unimplemented!("not implemented")
}

/// Whether any of the `within` lines after line `from` holds `needle`.
pub fn rows_after(_text: &str, _from: usize, _within: usize, _needle: &str) -> bool {
    unimplemented!("not implemented")
}

/// The edit header for `file`: the first line starting `⏺ Edit` or `⏺ Write` that holds
/// the file name. `stat_ok` tells whether it also holds the stat fragment.
pub fn edit_header(_text: &str, _file: &str) -> Option<EditHeader> {
    unimplemented!("not implemented")
}

/// Whether the numbered diff rows sit at or after line `from`.
pub fn has_diff_rows(_text: &str, _from: usize) -> bool {
    unimplemented!("not implemented")
}

/// Whether any raw line holds diff-header or escape debris.
pub fn has_debris(_text: &str) -> bool {
    unimplemented!("not implemented")
}

/// The parsed palette: footer presence and rows in order.
pub fn palette_state(_visible: &str) -> Palette {
    unimplemented!("not implemented")
}

/// The mention row for the scenario's file, with its folder, if the list shows it.
pub fn mention_list(_visible: &str) -> Option<MentionRow> {
    unimplemented!("not implemented")
}

/// Every collapsed-paste label in the text, in order.
pub fn paste_labels(_text: &str) -> Vec<PasteLabel> {
    unimplemented!("not implemented")
}

/// Whether any line holds the dropped-file label for `name`.
pub fn file_label(_text: &str, _name: &str) -> bool {
    unimplemented!("not implemented")
}

/// The parsed shell cell: the header row and the output rows it owns.
pub fn shell_cell(_text: &str, _command: &str) -> Option<ShellCell> {
    unimplemented!("not implemented")
}

/// The parsed side-answer cell for `question`.
pub fn side_cell(_text: &str, _question: &str) -> Option<SideCell> {
    unimplemented!("not implemented")
}

/// The parsed pager frame.
pub fn pager_state(_visible: &str) -> Pager {
    unimplemented!("not implemented")
}

/// The parsed theme report: which lines are present.
pub fn theme_report(_text: &str) -> ThemeReport {
    unimplemented!("not implemented")
}

/// The task panel: expanded, collapsed or absent, with the in-progress row.
pub fn task_view(_visible: &str) -> TaskView {
    unimplemented!("not implemented")
}

/// Every task record in the text, in order, as `(completed, total)`.
pub fn task_records(_text: &str) -> Vec<(u32, u32)> {
    unimplemented!("not implemented")
}

/// The agent state of a pane-state report, or `None` when the `talaria` label is absent
/// or no state word is shown. Matching is case-insensitive.
pub fn agent_state(_pane_get_text: &str) -> Option<AgentState> {
    unimplemented!("not implemented")
}

/// The prompt the phase 2a tool steps submit: `<action>, then join these two words with a
/// hyphen and reply with only the result: <word> and <number>`.
pub fn prompt_after(_token: &Token, _action: &str) -> String {
    unimplemented!("not implemented")
}
