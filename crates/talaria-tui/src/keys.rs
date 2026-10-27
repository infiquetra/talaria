//! Plain key values, chord text, and the composer key map.

use std::collections::BTreeMap;

use crossterm::event::{KeyEvent, KeyEventKind, KeyModifiers};

/// Where an action is looked up. Later screens add their own variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyContext {
    /// The draft box at the bottom of the screen.
    Composer,
    /// An open completion list: the slash palette or the `@` list. Issue #223.
    Completion,
    /// The full-screen model picker, before composer bindings.
    ModelPicker,
    /// The full-screen transcript.
    Pager,
    /// A queued row is selected. Issue 232. Queue and steer.
    QueueSelection,
    /// The session picker overlay list.
    SessionPicker,
    /// The picker's export dialog.
    SessionPickerExport,
    /// The fleet overlay list.
    Fleet,
    /// The full-screen subagent panel. Issue #242.
    Agents,
    /// The review overlay list. Issue #245.
    Review,
    /// The commit message editor. Issue #245.
    CommitMessage,
    /// The settings screen.
    Settings,
}

/// Model commands use separate contexts without changing composer editing actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModelAction {
    ModelPicker,
    EffortDown,
    EffortUp,
    ModelScope,
    ModelFavourite,
    ModelRefresh,
}

impl ModelAction {
    pub const ALL: [Self; 6] = [
        Self::ModelPicker,
        Self::EffortDown,
        Self::EffortUp,
        Self::ModelScope,
        Self::ModelFavourite,
        Self::ModelRefresh,
    ];

    pub const fn config_name(self) -> &'static str {
        match self {
            Self::ModelPicker => "model_picker",
            Self::EffortDown => "effort_down",
            Self::EffortUp => "effort_up",
            Self::ModelScope => "model_scope",
            Self::ModelFavourite => "model_favourite",
            Self::ModelRefresh => "model_refresh",
        }
    }

    pub fn from_config_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.config_name() == name)
    }

    pub const fn context(self) -> KeyContext {
        match self {
            Self::ModelPicker => KeyContext::Composer,
            _ => KeyContext::ModelPicker,
        }
    }

    const fn default_chords(self) -> &'static [&'static str] {
        match self {
            Self::ModelPicker => &["alt+p"],
            Self::EffortDown => &["left"],
            Self::EffortUp => &["right"],
            Self::ModelScope => &["ctrl+g"],
            Self::ModelFavourite => &["ctrl+f"],
            Self::ModelRefresh => &["ctrl+r"],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Binding {
    Composer(Action),
    Model(ModelAction),
}

impl Binding {
    fn all() -> impl Iterator<Item = Self> {
        Action::ALL
            .into_iter()
            .map(Self::Composer)
            .chain(ModelAction::ALL.into_iter().map(Self::Model))
    }
    fn index(self) -> usize {
        match self {
            Self::Composer(action) => action.index(),
            Self::Model(action) => Action::ALL.len() + action as usize,
        }
    }
    fn context(self) -> KeyContext {
        match self {
            Self::Composer(action) => action.context(),
            Self::Model(action) => action.context(),
        }
    }
    fn config_name(self) -> &'static str {
        match self {
            Self::Composer(action) => action.config_name(),
            Self::Model(action) => action.config_name(),
        }
    }
    fn from_config_name(name: &str) -> Option<Self> {
        Action::from_config_name(name)
            .map(Self::Composer)
            .or_else(|| ModelAction::from_config_name(name).map(Self::Model))
    }
    fn default_chords(self) -> &'static [&'static str] {
        match self {
            Self::Composer(action) => action.default_chords(),
            Self::Model(action) => action.default_chords(),
        }
    }
}

/// One command a key chord can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    Submit,
    Newline,
    Interrupt,
    Cancel,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    LineStart,
    LineEnd,
    BufferStart,
    BufferEnd,
    WordLeft,
    WordRight,
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    KillToLineEnd,
    KillToLineStart,
    Yank,
    /// Next row in an open completion list.
    CompletionNext,
    /// Previous row in an open completion list.
    CompletionPrev,
    /// Accept the selected completion.
    CompletionAccept,
    /// Close the open completion list.
    CompletionClose,
    OpenEditor,
    HistorySearch,
    Suspend,
    AttachClipboardImage,
    /// Open the transcript from the draft, or close it when it is already open.
    TranscriptOpen,
    /// One line toward the start of the transcript.
    PagerLineUp,
    /// One line toward the end of the transcript.
    PagerLineDown,
    /// A page toward the start of the transcript.
    PagerPageUp,
    /// A page toward the end of the transcript.
    PagerPageDown,
    /// Half a page toward the start of the transcript.
    PagerHalfUp,
    /// Half a page toward the end of the transcript.
    PagerHalfDown,
    /// The start of the transcript.
    PagerTop,
    /// The end of the transcript.
    PagerBottom,
    /// The previous message the person sent.
    PagerPrevPrompt,
    /// The next message the person sent.
    PagerNextPrompt,
    /// Start typing a search.
    PagerSearch,
    /// The next search match.
    PagerNextMatch,
    /// The previous search match.
    PagerPrevMatch,
    /// Switch expanded and compact cells.
    PagerDetails,
    /// Close the transcript.
    PagerClose,
    /// Open or close the task checklist drawer.
    ToggleTasks,
    /// Switches this session's approvals off or back on (#237).
    ApprovalsToggle,
    /// Edit the selected queued row. Issue 232. Queue and steer.
    QueueEdit,
    /// Remove the selected queued row, or discard a lost one.
    QueueRemove,
    /// Steer the selected queued row into the running turn.
    QueueSteer,
    /// Send the selected queued row now.
    QueueSendNow,
    /// Move the selected queued row toward the head.
    QueueMoveUp,
    /// Move the selected queued row toward the tail.
    QueueMoveDown,
    /// Leave selection. Does not interrupt.
    QueueLeave,
    /// Send the draft now.
    ComposerSendNow,
    /// Steer the draft. Unbound until a person sets a key.
    ComposerSteer,
    /// Open the session picker from the composer. Unbound until a person sets a key.
    SessionPickerOpen,
    /// Move the picker cursor one row up.
    PickerUp,
    /// Move the picker cursor one row down.
    PickerDown,
    /// Move the picker cursor one page up.
    PickerPageUp,
    /// Move the picker cursor one page down.
    PickerPageDown,
    /// The first picker row.
    PickerTop,
    /// The last picker row.
    PickerBottom,
    /// Open the picked session.
    PickerOpen,
    /// Rename the picked session.
    PickerRename,
    /// Branch the current session.
    PickerBranch,
    /// Open the export dialog for the picked session.
    PickerExport,
    /// Search the list.
    PickerSearch,
    /// The next profile filter.
    PickerProfileNext,
    /// The previous profile filter.
    PickerProfilePrev,
    /// List the sessions again.
    PickerRefresh,
    /// Close the picker.
    PickerClose,
    /// Export as Markdown, in the export dialog.
    PickerFormatMarkdown,
    /// Export as JSON, in the export dialog.
    PickerFormatJson,
    /// Export as HTML, in the export dialog.
    PickerFormatHtml,
    /// Switch secret redaction for the export, in the export dialog.
    PickerToggleRedact,
    /// Open the fleet overlay from the composer. Unbound until the operator binds it.
    FleetViewOpen,
    /// Move the fleet cursor up.
    FleetUp,
    /// Move the fleet cursor down.
    FleetDown,
    /// Move the fleet cursor up by a page.
    FleetPageUp,
    /// Move the fleet cursor down by a page.
    FleetPageDown,
    /// Move the fleet cursor to the first row.
    FleetTop,
    /// Move the fleet cursor to the last row.
    FleetBottom,
    /// Open the fleet row as the foreground session.
    FleetOpen,
    /// Open a fleet session in another multiplexer pane.
    FleetOpenInPane,
    /// Toggle the peek panel for the fleet row.
    FleetPeek,
    /// Reply to the fleet row's session.
    FleetReply,
    /// Stop the fleet row's running turn, after a confirmation.
    FleetStop,
    /// Answer the open card in the peek.
    FleetAnswer,
    /// Edit the fleet text filter.
    FleetFilter,
    /// The next fleet profile filter.
    FleetProfileNext,
    /// The previous fleet profile filter.
    FleetProfilePrev,
    /// Open the subagent panel from the composer, or close it. Issue #242.
    AgentsPanel,
    /// Move the panel selection one row up.
    AgentsUp,
    /// Move the panel selection one row down.
    AgentsDown,
    /// Open the tail view for the selected row.
    AgentsTail,
    /// Open the detail view for the selected row.
    AgentsDetail,
    /// Back to the list, or close the panel from the list.
    AgentsBack,
    /// Close the panel.
    AgentsClose,
    /// Steer the selected row.
    AgentsSteer,
    /// Stop the selected row.
    AgentsStop,
    /// Stop every running row, with confirmation.
    AgentsStopAll,
    /// Pause spawning, or resume when paused.
    AgentsPause,
    /// Move the panel to the next attached session.
    AgentsNextSession,
    /// A page toward the start of the list or view.
    AgentsPageUp,
    /// A page toward the end of the list or view.
    AgentsPageDown,
    /// The first row.
    AgentsTop,
    /// The last row.
    AgentsBottom,
    /// Open the selected child's session in another multiplexer pane.
    AgentsOpenInPane,
    /// Start a background session from the fleet list.
    FleetNew,
    /// Start a background session in a new worktree from the fleet list.
    FleetNewWorktree,
    /// Jump to the oldest background request.
    AttentionNext,
    /// Review the fleet row's session folder.
    FleetReview,
    /// Move the review cursor up. Issue #245.
    ReviewUp,
    /// Move the review cursor down. Issue #245.
    ReviewDown,
    /// First review file. Issue #245.
    ReviewTop,
    /// Last review file. Issue #245.
    ReviewBottom,
    /// Show the selected diff in place of the list. Issue #245.
    ReviewShowDiff,
    /// Scroll the diff panel up. Issue #245.
    ReviewDiffPageUp,
    /// Scroll the diff panel down. Issue #245.
    ReviewDiffPageDown,
    /// Switch Uncommitted and Branch scopes. Issue #245.
    ReviewScope,
    /// Stage or unstage the selected file. Issue #245.
    ReviewStage,
    /// Stage everything. Issue #245.
    ReviewStageAll,
    /// Unstage everything. Issue #245.
    ReviewUnstageAll,
    /// Discard the selected file, after a second press. Issue #245.
    ReviewDiscard,
    /// Open the commit editor. Issue #245.
    ReviewCommit,
    /// Push the branch. Issue #245.
    ReviewPush,
    /// Open a draft pull request. Issue #245.
    ReviewPullRequest,
    /// Open the known pull request in the browser. Issue #245.
    ReviewOpenPr,
    /// Reload the list and refresh status. Issue #245.
    ReviewRefresh,
    /// Commit the editor text. Issue #245.
    CommitSubmit,
    /// Draft a message with the session's model. Issue #245.
    CommitDraft,
}

/// What a key does in an open completion list: the slash palette or the `@` list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionAction {
    /// Select the next row.
    Next,
    /// Select the previous row.
    Prev,
    /// Accept the selected row.
    Accept,
    /// Close the list.
    Close,
}

impl CompletionAction {
    /// The completion meaning of a key-map action, for the four completion actions.
    #[must_use]
    pub const fn from_action(action: Action) -> Option<Self> {
        match action {
            Action::CompletionNext => Some(Self::Next),
            Action::CompletionPrev => Some(Self::Prev),
            Action::CompletionAccept => Some(Self::Accept),
            Action::CompletionClose => Some(Self::Close),
            _ => None,
        }
    }
}

impl Action {
    /// Every action, in the order used for conflicts and notices.
    pub const ALL: [Self; 131] = [
        Self::Submit,
        Self::Newline,
        Self::Interrupt,
        Self::Cancel,
        Self::MoveLeft,
        Self::MoveRight,
        Self::MoveUp,
        Self::MoveDown,
        Self::LineStart,
        Self::LineEnd,
        Self::BufferStart,
        Self::BufferEnd,
        Self::WordLeft,
        Self::WordRight,
        Self::DeleteBackward,
        Self::DeleteForward,
        Self::DeleteWordBackward,
        Self::KillToLineEnd,
        Self::KillToLineStart,
        Self::Yank,
        Self::CompletionNext,
        Self::CompletionPrev,
        Self::CompletionAccept,
        Self::CompletionClose,
        Self::OpenEditor,
        Self::HistorySearch,
        Self::Suspend,
        Self::AttachClipboardImage,
        Self::TranscriptOpen,
        Self::PagerLineUp,
        Self::PagerLineDown,
        Self::PagerPageUp,
        Self::PagerPageDown,
        Self::PagerHalfUp,
        Self::PagerHalfDown,
        Self::PagerTop,
        Self::PagerBottom,
        Self::PagerPrevPrompt,
        Self::PagerNextPrompt,
        Self::PagerSearch,
        Self::PagerNextMatch,
        Self::PagerPrevMatch,
        Self::PagerDetails,
        Self::PagerClose,
        Self::ToggleTasks,
        Self::ApprovalsToggle,
        Self::QueueEdit,
        Self::QueueRemove,
        Self::QueueSteer,
        Self::QueueSendNow,
        Self::QueueMoveUp,
        Self::QueueMoveDown,
        Self::QueueLeave,
        Self::ComposerSendNow,
        Self::ComposerSteer,
        Self::SessionPickerOpen,
        Self::PickerUp,
        Self::PickerDown,
        Self::PickerPageUp,
        Self::PickerPageDown,
        Self::PickerTop,
        Self::PickerBottom,
        Self::PickerOpen,
        Self::PickerRename,
        Self::PickerBranch,
        Self::PickerExport,
        Self::PickerSearch,
        Self::PickerProfileNext,
        Self::PickerProfilePrev,
        Self::PickerRefresh,
        Self::PickerClose,
        Self::PickerFormatMarkdown,
        Self::PickerFormatJson,
        Self::PickerFormatHtml,
        Self::PickerToggleRedact,
        Self::FleetViewOpen,
        Self::FleetUp,
        Self::FleetDown,
        Self::FleetPageUp,
        Self::FleetPageDown,
        Self::FleetTop,
        Self::FleetBottom,
        Self::FleetOpen,
        Self::FleetOpenInPane,
        Self::FleetPeek,
        Self::FleetReply,
        Self::FleetStop,
        Self::FleetAnswer,
        Self::FleetFilter,
        Self::FleetProfileNext,
        Self::FleetProfilePrev,
        Self::AgentsPanel,
        Self::AgentsUp,
        Self::AgentsDown,
        Self::AgentsTail,
        Self::AgentsDetail,
        Self::AgentsBack,
        Self::AgentsClose,
        Self::AgentsSteer,
        Self::AgentsStop,
        Self::AgentsStopAll,
        Self::AgentsPause,
        Self::AgentsNextSession,
        Self::AgentsPageUp,
        Self::AgentsPageDown,
        Self::AgentsTop,
        Self::AgentsBottom,
        Self::AgentsOpenInPane,
        Self::FleetNew,
        Self::FleetNewWorktree,
        Self::AttentionNext,
        Self::FleetReview,
        Self::ReviewUp,
        Self::ReviewDown,
        Self::ReviewTop,
        Self::ReviewBottom,
        Self::ReviewShowDiff,
        Self::ReviewDiffPageUp,
        Self::ReviewDiffPageDown,
        Self::ReviewScope,
        Self::ReviewStage,
        Self::ReviewStageAll,
        Self::ReviewUnstageAll,
        Self::ReviewDiscard,
        Self::ReviewCommit,
        Self::ReviewPush,
        Self::ReviewPullRequest,
        Self::ReviewOpenPr,
        Self::ReviewRefresh,
        Self::CommitSubmit,
        Self::CommitDraft,
    ];

    /// Name used in the keys table of the configuration file.
    #[must_use]
    pub const fn config_name(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Newline => "newline",
            Self::Interrupt => "interrupt",
            Self::Cancel => "cancel",
            Self::MoveLeft => "move_left",
            Self::MoveRight => "move_right",
            Self::MoveUp => "move_up",
            Self::MoveDown => "move_down",
            Self::LineStart => "line_start",
            Self::LineEnd => "line_end",
            Self::BufferStart => "buffer_start",
            Self::BufferEnd => "buffer_end",
            Self::WordLeft => "word_left",
            Self::WordRight => "word_right",
            Self::DeleteBackward => "delete_backward",
            Self::DeleteForward => "delete_forward",
            Self::DeleteWordBackward => "delete_word_backward",
            Self::KillToLineEnd => "kill_to_line_end",
            Self::KillToLineStart => "kill_to_line_start",
            Self::Yank => "yank",
            Self::CompletionNext => "completion_next",
            Self::CompletionPrev => "completion_prev",
            Self::CompletionAccept => "completion_accept",
            Self::CompletionClose => "completion_close",
            Self::OpenEditor => "open_editor",
            Self::HistorySearch => "history_search",
            Self::Suspend => "suspend",
            Self::AttachClipboardImage => "attach_clipboard_image",
            Self::TranscriptOpen => "transcript_open",
            Self::PagerLineUp => "pager_line_up",
            Self::PagerLineDown => "pager_line_down",
            Self::PagerPageUp => "pager_page_up",
            Self::PagerPageDown => "pager_page_down",
            Self::PagerHalfUp => "pager_half_up",
            Self::PagerHalfDown => "pager_half_down",
            Self::PagerTop => "pager_top",
            Self::PagerBottom => "pager_bottom",
            Self::PagerPrevPrompt => "pager_prev_prompt",
            Self::PagerNextPrompt => "pager_next_prompt",
            Self::PagerSearch => "pager_search",
            Self::PagerNextMatch => "pager_next_match",
            Self::PagerPrevMatch => "pager_prev_match",
            Self::PagerDetails => "pager_details",
            Self::PagerClose => "pager_close",
            Self::ToggleTasks => "toggle_tasks",
            Self::ApprovalsToggle => "approvals_toggle",
            Self::QueueEdit => "queue_edit",
            Self::QueueRemove => "queue_remove",
            Self::QueueSteer => "queue_steer",
            Self::QueueSendNow => "queue_send_now",
            Self::QueueMoveUp => "queue_move_up",
            Self::QueueMoveDown => "queue_move_down",
            Self::QueueLeave => "queue_leave",
            Self::ComposerSendNow => "composer_send_now",
            Self::ComposerSteer => "composer_steer",
            Self::SessionPickerOpen => "session_picker_open",
            Self::PickerUp => "picker_up",
            Self::PickerDown => "picker_down",
            Self::PickerPageUp => "picker_page_up",
            Self::PickerPageDown => "picker_page_down",
            Self::PickerTop => "picker_top",
            Self::PickerBottom => "picker_bottom",
            Self::PickerOpen => "picker_open",
            Self::PickerRename => "picker_rename",
            Self::PickerBranch => "picker_branch",
            Self::PickerExport => "picker_export",
            Self::PickerSearch => "picker_search",
            Self::PickerProfileNext => "picker_profile_next",
            Self::PickerProfilePrev => "picker_profile_prev",
            Self::PickerRefresh => "picker_refresh",
            Self::PickerClose => "picker_close",
            Self::PickerFormatMarkdown => "picker_format_markdown",
            Self::PickerFormatJson => "picker_format_json",
            Self::PickerFormatHtml => "picker_format_html",
            Self::PickerToggleRedact => "picker_toggle_redact",
            Self::FleetViewOpen => "fleet_view_open",
            Self::FleetUp => "fleet_up",
            Self::FleetDown => "fleet_down",
            Self::FleetPageUp => "fleet_page_up",
            Self::FleetPageDown => "fleet_page_down",
            Self::FleetTop => "fleet_top",
            Self::FleetBottom => "fleet_bottom",
            Self::FleetOpen => "fleet_open",
            Self::FleetOpenInPane => "fleet_open_in_pane",
            Self::FleetPeek => "fleet_peek",
            Self::FleetReply => "fleet_reply",
            Self::FleetStop => "fleet_stop",
            Self::FleetAnswer => "fleet_answer",
            Self::FleetFilter => "fleet_filter",
            Self::FleetProfileNext => "fleet_profile_next",
            Self::FleetProfilePrev => "fleet_profile_prev",
            Self::AgentsPanel => "agents_panel",
            Self::AgentsUp => "agents_up",
            Self::AgentsDown => "agents_down",
            Self::AgentsTail => "agents_tail",
            Self::AgentsDetail => "agents_detail",
            Self::AgentsBack => "agents_back",
            Self::AgentsClose => "agents_close",
            Self::AgentsSteer => "agents_steer",
            Self::AgentsStop => "agents_stop",
            Self::AgentsStopAll => "agents_stop_all",
            Self::AgentsPause => "agents_pause",
            Self::AgentsNextSession => "agents_next_session",
            Self::AgentsPageUp => "agents_page_up",
            Self::AgentsPageDown => "agents_page_down",
            Self::AgentsTop => "agents_top",
            Self::AgentsBottom => "agents_bottom",
            Self::AgentsOpenInPane => "agents_open_in_pane",
            Self::FleetNew => "fleet_new",
            Self::FleetNewWorktree => "fleet_new_worktree",
            Self::AttentionNext => "attention_next",
            Self::FleetReview => "fleet_review",
            Self::ReviewUp => "review_up",
            Self::ReviewDown => "review_down",
            Self::ReviewTop => "review_top",
            Self::ReviewBottom => "review_bottom",
            Self::ReviewShowDiff => "review_show_diff",
            Self::ReviewDiffPageUp => "review_diff_page_up",
            Self::ReviewDiffPageDown => "review_diff_page_down",
            Self::ReviewScope => "review_scope",
            Self::ReviewStage => "review_stage",
            Self::ReviewStageAll => "review_stage_all",
            Self::ReviewUnstageAll => "review_unstage_all",
            Self::ReviewDiscard => "review_discard",
            Self::ReviewCommit => "review_commit",
            Self::ReviewPush => "review_push",
            Self::ReviewPullRequest => "review_pull_request",
            Self::ReviewOpenPr => "review_open_pr",
            Self::ReviewRefresh => "review_refresh",
            Self::CommitSubmit => "commit_submit",
            Self::CommitDraft => "commit_draft",
        }
    }

    /// The action named in the configuration file, if the name is known.
    #[must_use]
    pub fn from_config_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.config_name() == name)
    }

    /// The screen this action belongs to.
    #[must_use]
    pub const fn context(self) -> KeyContext {
        match self {
            Self::CompletionNext
            | Self::CompletionPrev
            | Self::CompletionAccept
            | Self::CompletionClose => KeyContext::Completion,
            Self::PickerFormatMarkdown
            | Self::PickerFormatJson
            | Self::PickerFormatHtml
            | Self::PickerToggleRedact => KeyContext::SessionPickerExport,
            Self::SessionPickerOpen => KeyContext::Composer,
            Self::Submit
            | Self::Newline
            | Self::Interrupt
            | Self::Cancel
            | Self::MoveLeft
            | Self::MoveRight
            | Self::MoveUp
            | Self::MoveDown
            | Self::LineStart
            | Self::LineEnd
            | Self::BufferStart
            | Self::BufferEnd
            | Self::WordLeft
            | Self::WordRight
            | Self::DeleteBackward
            | Self::DeleteForward
            | Self::DeleteWordBackward
            | Self::KillToLineEnd
            | Self::KillToLineStart
            | Self::Yank
            | Self::TranscriptOpen
            | Self::OpenEditor
            | Self::HistorySearch
            | Self::Suspend
            | Self::AttachClipboardImage
            | Self::ToggleTasks
            | Self::ApprovalsToggle
            | Self::ComposerSendNow
            | Self::ComposerSteer
            | Self::AgentsPanel => KeyContext::Composer,
            Self::AttentionNext => KeyContext::Composer,
            Self::PagerLineUp
            | Self::PagerLineDown
            | Self::PagerPageUp
            | Self::PagerPageDown
            | Self::PagerHalfUp
            | Self::PagerHalfDown
            | Self::PagerTop
            | Self::PagerBottom
            | Self::PagerPrevPrompt
            | Self::PagerNextPrompt
            | Self::PagerSearch
            | Self::PagerNextMatch
            | Self::PagerPrevMatch
            | Self::PagerDetails
            | Self::PagerClose => KeyContext::Pager,
            Self::QueueEdit
            | Self::QueueRemove
            | Self::QueueSteer
            | Self::QueueSendNow
            | Self::QueueMoveUp
            | Self::QueueMoveDown
            | Self::QueueLeave => KeyContext::QueueSelection,
            Self::PickerUp
            | Self::PickerDown
            | Self::PickerPageUp
            | Self::PickerPageDown
            | Self::PickerTop
            | Self::PickerBottom
            | Self::PickerOpen
            | Self::PickerRename
            | Self::PickerBranch
            | Self::PickerExport
            | Self::PickerSearch
            | Self::PickerProfileNext
            | Self::PickerProfilePrev
            | Self::PickerRefresh
            | Self::PickerClose => KeyContext::SessionPicker,
            Self::FleetViewOpen => KeyContext::Composer,
            Self::FleetUp
            | Self::FleetDown
            | Self::FleetPageUp
            | Self::FleetPageDown
            | Self::FleetTop
            | Self::FleetBottom
            | Self::FleetOpen
            | Self::FleetOpenInPane
            | Self::FleetPeek
            | Self::FleetReply
            | Self::FleetStop
            | Self::FleetAnswer
            | Self::FleetFilter
            | Self::FleetProfileNext
            | Self::FleetProfilePrev
            | Self::FleetNew
            | Self::FleetNewWorktree
            | Self::FleetReview => KeyContext::Fleet,
            Self::AgentsUp
            | Self::AgentsDown
            | Self::AgentsTail
            | Self::AgentsDetail
            | Self::AgentsBack
            | Self::AgentsClose
            | Self::AgentsSteer
            | Self::AgentsStop
            | Self::AgentsStopAll
            | Self::AgentsPause
            | Self::AgentsNextSession
            | Self::AgentsPageUp
            | Self::AgentsPageDown
            | Self::AgentsTop
            | Self::AgentsBottom
            | Self::AgentsOpenInPane => KeyContext::Agents,
            Self::ReviewUp
            | Self::ReviewDown
            | Self::ReviewTop
            | Self::ReviewBottom
            | Self::ReviewShowDiff
            | Self::ReviewDiffPageUp
            | Self::ReviewDiffPageDown
            | Self::ReviewScope
            | Self::ReviewStage
            | Self::ReviewStageAll
            | Self::ReviewUnstageAll
            | Self::ReviewDiscard
            | Self::ReviewCommit
            | Self::ReviewPush
            | Self::ReviewPullRequest
            | Self::ReviewOpenPr
            | Self::ReviewRefresh => KeyContext::Review,
            Self::CommitSubmit | Self::CommitDraft => KeyContext::CommitMessage,
        }
    }

    /// Position in [`Self::ALL`].
    const fn index(self) -> usize {
        self as usize
    }

    /// The built-in chords, in display order.
    const fn default_chords(self) -> &'static [&'static str] {
        match self {
            Self::Submit => &["enter"],
            Self::Newline => &["shift+enter", "alt+enter", "ctrl+j"],
            Self::Interrupt => &["esc"],
            Self::Cancel => &["ctrl+c"],
            Self::MoveLeft => &["left"],
            Self::MoveRight => &["right"],
            Self::MoveUp => &["up"],
            Self::MoveDown => &["down"],
            Self::LineStart => &["home", "ctrl+a"],
            Self::LineEnd => &["end", "ctrl+e"],
            Self::BufferStart => &["ctrl+home"],
            Self::BufferEnd => &["ctrl+end"],
            Self::WordLeft => &["alt+left", "ctrl+left", "alt+b"],
            Self::WordRight => &["alt+right", "ctrl+right", "alt+f"],
            Self::DeleteBackward => &["backspace", "ctrl+h"],
            Self::DeleteForward => &["delete"],
            Self::DeleteWordBackward => &["ctrl+w", "alt+backspace"],
            Self::KillToLineEnd => &["ctrl+k"],
            Self::KillToLineStart => &["ctrl+u"],
            Self::Yank => &["ctrl+y"],
            Self::CompletionNext => &["down", "ctrl+n"],
            Self::CompletionPrev => &["up", "ctrl+p"],
            Self::CompletionAccept => &["tab"],
            Self::CompletionClose => &["esc"],
            Self::OpenEditor => &["ctrl+g", "alt+g"],
            Self::HistorySearch => &["ctrl+r"],
            Self::Suspend => &["ctrl+z"],
            Self::AttachClipboardImage => &["ctrl+v", "alt+v"],
            // Issue 222: the transcript chords. The plain letters belong to the transcript
            // screen only, so they never stop the composer from typing.
            Self::TranscriptOpen => &["ctrl+o"],
            Self::PagerLineUp => &["up", "k"],
            Self::PagerLineDown => &["down", "j"],
            Self::PagerPageUp => &["pageup", "b"],
            Self::PagerPageDown => &["pagedown", "space"],
            Self::PagerHalfUp => &["ctrl+u"],
            Self::PagerHalfDown => &["ctrl+d"],
            Self::PagerTop => &["g", "home"],
            Self::PagerBottom => &["shift+g", "end"],
            Self::PagerPrevPrompt => &["{"],
            Self::PagerNextPrompt => &["}"],
            Self::PagerSearch => &["/"],
            Self::PagerNextMatch => &["n"],
            Self::PagerPrevMatch => &["shift+n"],
            Self::PagerDetails => &["t"],
            Self::PagerClose => &["q"],
            Self::ToggleTasks => &["ctrl+t"],
            Self::ApprovalsToggle => &["shift+tab"],
            Self::QueueEdit => &["enter", "e"],
            Self::QueueRemove => &["d", "delete", "ctrl+x"],
            Self::QueueSteer => &["s"],
            Self::QueueSendNow => &["n"],
            Self::QueueMoveUp => &["K", "shift+up"],
            Self::QueueMoveDown => &["J", "shift+down"],
            Self::QueueLeave => &["esc"],
            Self::ComposerSendNow => &["ctrl+enter"],
            Self::ComposerSteer => &[],
            Self::SessionPickerOpen => &[],
            Self::PickerUp => &["up", "k"],
            Self::PickerDown => &["down", "j"],
            Self::PickerPageUp => &["pageup"],
            Self::PickerPageDown => &["pagedown"],
            Self::PickerTop => &["home", "g"],
            Self::PickerBottom => &["end", "G"],
            Self::PickerOpen => &["enter"],
            Self::PickerRename => &["r"],
            Self::PickerBranch => &["b"],
            Self::PickerExport => &["e"],
            Self::PickerSearch => &["/"],
            Self::PickerProfileNext => &["tab"],
            Self::PickerProfilePrev => &["shift+tab"],
            Self::PickerRefresh => &["ctrl+r"],
            Self::PickerClose => &["q"],
            Self::PickerFormatMarkdown => &["m"],
            Self::PickerFormatJson => &["j"],
            Self::PickerFormatHtml => &["h"],
            Self::PickerToggleRedact => &["x"],
            Self::FleetViewOpen => &[],
            Self::FleetUp => &["up", "k"],
            Self::FleetDown => &["down", "j"],
            Self::FleetPageUp => &["pageup"],
            Self::FleetPageDown => &["pagedown"],
            Self::FleetTop => &["home", "g"],
            Self::FleetBottom => &["end", "shift+g"],
            Self::FleetOpen => &["enter"],
            Self::FleetOpenInPane => &["o"],
            Self::FleetPeek => &["space"],
            Self::FleetReply => &["r"],
            Self::FleetStop => &["x"],
            Self::FleetAnswer => &["a"],
            Self::FleetFilter => &["/"],
            Self::FleetProfileNext => &["tab"],
            Self::FleetProfilePrev => &["shift+tab"],
            Self::AgentsPanel => &["alt+s"],
            Self::AgentsUp => &["up", "k"],
            Self::AgentsDown => &["down", "j"],
            Self::AgentsTail => &["enter"],
            Self::AgentsDetail => &["d", "right"],
            Self::AgentsBack => &["esc", "left"],
            Self::AgentsClose => &["q"],
            Self::AgentsSteer => &["e"],
            Self::AgentsStop => &["x"],
            Self::AgentsStopAll => &["shift+x"],
            Self::AgentsPause => &["p"],
            Self::AgentsNextSession => &["tab"],
            Self::AgentsPageUp => &["pageup"],
            Self::AgentsPageDown => &["pagedown"],
            Self::AgentsTop => &["g", "home"],
            Self::AgentsBottom => &["shift+g", "end"],
            Self::AgentsOpenInPane => &["o"],
            Self::FleetNew => &["n"],
            Self::FleetNewWorktree => &["w"],
            Self::AttentionNext => &["alt+a"],
            Self::FleetReview => &["v"],
            Self::ReviewUp => &["up", "k"],
            Self::ReviewDown => &["down", "j"],
            Self::ReviewTop => &["home", "g"],
            Self::ReviewBottom => &["end", "shift+g"],
            Self::ReviewShowDiff => &["enter"],
            Self::ReviewDiffPageUp => &["pageup"],
            Self::ReviewDiffPageDown => &["pagedown"],
            Self::ReviewScope => &["tab"],
            Self::ReviewStage => &["space"],
            Self::ReviewStageAll => &["a"],
            Self::ReviewUnstageAll => &["shift+a"],
            Self::ReviewDiscard => &["shift+x"],
            Self::ReviewCommit => &["c"],
            Self::ReviewPush => &["p"],
            Self::ReviewPullRequest => &["shift+p"],
            Self::ReviewOpenPr => &["o"],
            Self::ReviewRefresh => &["r"],
            Self::CommitSubmit => &["ctrl+s"],
            Self::CommitDraft => &["ctrl+d"],
        }
    }
}

/// A key after it has been turned into Talaria's own value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// The key that was pressed.
    pub code: KeyCode,
    /// Which modifier keys were held.
    pub mods: Mods,
    /// Press, repeat, or release.
    pub kind: KeyKind,
}

impl From<KeyEvent> for Key {
    fn from(event: KeyEvent) -> Self {
        use crossterm::event::KeyCode as Term;
        let code = match event.code {
            Term::Char(c) => KeyCode::Char(c),
            Term::Enter => KeyCode::Enter,
            Term::Esc => KeyCode::Esc,
            Term::Tab | Term::BackTab => KeyCode::Tab,
            Term::Backspace => KeyCode::Backspace,
            Term::Delete => KeyCode::Delete,
            Term::Insert => KeyCode::Insert,
            Term::Up => KeyCode::Up,
            Term::Down => KeyCode::Down,
            Term::Left => KeyCode::Left,
            Term::Right => KeyCode::Right,
            Term::Home => KeyCode::Home,
            Term::End => KeyCode::End,
            Term::PageUp => KeyCode::PageUp,
            Term::PageDown => KeyCode::PageDown,
            Term::F(n) => KeyCode::F(n),
            _ => KeyCode::Other,
        };
        let held = event.modifiers;
        let mods = Mods {
            ctrl: held.contains(KeyModifiers::CONTROL),
            alt: held.contains(KeyModifiers::ALT),
            // crossterm reports Shift+Tab as BackTab, with or without the shift flag.
            shift: held.contains(KeyModifiers::SHIFT) || event.code == Term::BackTab,
            super_key: held.contains(KeyModifiers::SUPER),
            other: held.intersects(KeyModifiers::HYPER | KeyModifiers::META),
        };
        let kind = match event.kind {
            KeyEventKind::Press => KeyKind::Press,
            KeyEventKind::Repeat => KeyKind::Repeat,
            KeyEventKind::Release => KeyKind::Release,
        };
        Self { code, mods, kind }
    }
}

/// The key identity, without modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// A typed character.
    Char(char),
    /// Enter or return.
    Enter,
    /// Escape.
    Esc,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Forward delete.
    Delete,
    /// Insert.
    Insert,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// A function key. `F(1)` is F1.
    F(u8),
    /// A key this list does not name.
    Other,
}

/// Modifier keys. `other` is set for hyper or meta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods {
    /// Control.
    pub ctrl: bool,
    /// Alt or option.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
    /// Super or command.
    pub super_key: bool,
    /// Hyper or meta, which match no chord.
    pub other: bool,
}

/// Whether the key went down, repeated, or came up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyKind {
    /// The key was pressed.
    Press,
    /// The key is repeating.
    Repeat,
    /// The key was released. Releases never match a chord.
    Release,
}

/// One binding: modifiers plus a single key. A character is stored lower-cased; an uppercase
/// letter in the text sets `shift`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChord {
    code: KeyCode,
    ctrl: bool,
    alt: bool,
    shift: bool,
    super_key: bool,
}

/// The key names a chord may use, besides a single character and `f1` to `f24`.
const NAMED_KEYS: [(&str, KeyCode); 16] = [
    ("enter", KeyCode::Enter),
    ("esc", KeyCode::Esc),
    ("escape", KeyCode::Esc),
    ("tab", KeyCode::Tab),
    ("backspace", KeyCode::Backspace),
    ("delete", KeyCode::Delete),
    ("insert", KeyCode::Insert),
    ("up", KeyCode::Up),
    ("down", KeyCode::Down),
    ("left", KeyCode::Left),
    ("right", KeyCode::Right),
    ("home", KeyCode::Home),
    ("end", KeyCode::End),
    ("pageup", KeyCode::PageUp),
    ("pagedown", KeyCode::PageDown),
    ("space", KeyCode::Char(' ')),
];

/// The reason `KeyChord::parse` gives when a key is fixed in the picker's contexts.
const FIXED_KEY_REASON: &str = "esc and ctrl+c are fixed in the session picker";

impl KeyChord {
    /// Parse chord text such as `ctrl+j` or `shift+enter`. The error is the reason.
    pub fn parse(text: &str, context: KeyContext) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("empty".to_string());
        }
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        if parts.iter().any(|part| part.is_empty()) {
            return Err("missing key".to_string());
        }
        let (key, modifiers) = parts
            .split_last()
            .ok_or_else(|| "missing key".to_string())?;
        let mut chord = Self {
            code: KeyCode::Other,
            ctrl: false,
            alt: false,
            shift: false,
            super_key: false,
        };
        for modifier in modifiers {
            let slot = match modifier.to_lowercase().as_str() {
                "ctrl" | "control" => &mut chord.ctrl,
                "alt" | "option" => &mut chord.alt,
                "shift" => &mut chord.shift,
                "super" => &mut chord.super_key,
                _ => return Err(format!("unknown modifier \"{modifier}\"")),
            };
            if *slot {
                return Err(format!("repeated modifier \"{modifier}\""));
            }
            *slot = true;
        }
        chord.code = parse_key(key, &mut chord.shift)?;
        if context == KeyContext::Pager && Self::is_pager_reserved(&chord) {
            return Err("reserved in the transcript pager".to_string());
        }
        // Esc and Ctrl+C close the session picker, so no picker action may take them.
        if matches!(
            context,
            KeyContext::SessionPicker | KeyContext::SessionPickerExport | KeyContext::Fleet
        ) && (chord.code == KeyCode::Esc || (chord.code == KeyCode::Char('c') && chord.ctrl))
        {
            return Err(FIXED_KEY_REASON.to_string());
        }
        let printable = matches!(chord.code, KeyCode::Char(_));
        if context == KeyContext::Composer
            && printable
            && !(chord.ctrl || chord.alt || chord.super_key)
        {
            return Err("a plain letter would stop that key from typing".to_string());
        }
        Ok(chord)
    }

    /// Whether the transcript screen keeps this chord for itself. Escape cancels a search and
    /// Control-C closes the screen; neither may be bound to an action.
    fn is_pager_reserved(chord: &Self) -> bool {
        chord.code == KeyCode::Esc || (chord.ctrl && chord.code == KeyCode::Char('c'))
    }

    /// The key a press of this chord produces, for code that drives an editor with a chord.
    #[must_use]
    pub fn to_key(&self) -> Key {
        let code = match self.code {
            KeyCode::Char(c) if self.shift => {
                let mut upper = c.to_uppercase();
                match (upper.next(), upper.next()) {
                    (Some(single), None) => KeyCode::Char(single),
                    _ => KeyCode::Char(c),
                }
            }
            other => other,
        };
        Key {
            code,
            mods: Mods {
                ctrl: self.ctrl,
                alt: self.alt,
                shift: self.shift,
                super_key: self.super_key,
                other: false,
            },
            kind: KeyKind::Press,
        }
    }

    /// Whether this chord matches a key. Releases and hyper or meta never match.
    #[must_use]
    pub fn matches(&self, key: &Key) -> bool {
        if key.kind == KeyKind::Release || key.mods.other {
            return false;
        }
        let mods = key.mods;
        if self.ctrl != mods.ctrl || self.alt != mods.alt || self.super_key != mods.super_key {
            return false;
        }
        match (self.code, key.code) {
            (KeyCode::Char(want), KeyCode::Char(got)) => {
                let shifted = mods.shift || got.is_uppercase();
                lower(got) == want && self.shift == shifted
            }
            (want, got) => want == got && self.shift == mods.shift,
        }
    }

    /// Display text such as `Shift+Enter` or `Ctrl+C`.
    #[must_use]
    pub fn label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (held, name) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.super_key, "Super"),
        ] {
            if held {
                parts.push(name.to_string());
            }
        }
        parts.push(key_label(self.code));
        parts.join("+")
    }

    /// True when a terminal can send this chord only with keyboard enhancement.
    #[must_use]
    pub fn needs_enhancement(&self) -> bool {
        self.super_key || (self.code == KeyCode::Enter && (self.shift || self.ctrl))
    }

    /// F1, F2, Ctrl+B or Ctrl+S, which some layer may take before Talaria sees it.
    fn is_risky(&self) -> bool {
        let bare = !(self.alt || self.shift || self.super_key);
        match self.code {
            KeyCode::F(1 | 2) => bare && !self.ctrl,
            KeyCode::Char('b' | 's') => bare && self.ctrl,
            _ => false,
        }
    }
}

fn lower(c: char) -> char {
    let mut lowered = c.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

/// The key part of a chord. An uppercase letter sets `shift`.
fn parse_key(key: &str, shift: &mut bool) -> Result<KeyCode, String> {
    let name = key.to_lowercase();
    if let Some((_, code)) = NAMED_KEYS.iter().find(|(known, _)| *known == name) {
        return Ok(*code);
    }
    if name == "plus" {
        return Ok(KeyCode::Char('+'));
    }
    if let Some(number) = name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&number)
        && name.len() > 1
        && !name[1..].starts_with('0')
    {
        return Ok(KeyCode::F(number));
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => {
            if c.is_uppercase() {
                *shift = true;
            }
            Ok(KeyCode::Char(lower(c)))
        }
        _ => Err(format!("unknown key \"{key}\"")),
    }
}

fn key_label(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char('+') => "Plus".to_string(),
        KeyCode::Char(c) => c.to_uppercase().collect(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        KeyCode::Other => "Other".to_string(),
    }
}

/// The literal file name every notice uses, never the real path.
const FILE: &str = "~/.talaria/config.toml";

/// The chords bound to each composer action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMap {
    /// Indexed by position in [`Action::ALL`].
    chords: Vec<Vec<KeyChord>>,
}

impl KeyMap {
    /// Resolve a key in the settings screen.
    pub fn resolve_settings(
        &self,
        _context: KeyContext,
        _key: &Key,
    ) -> Option<talaria_core::settings::SettingsAction> {
        todo!("settings stub")
    }

    /// The built-in bindings.
    #[must_use]
    pub fn defaults() -> Self {
        let chords = Action::ALL
            .iter()
            .map(|action| default_chords(*action))
            .chain(ModelAction::ALL.into_iter().map(|action| {
                action
                    .default_chords()
                    .iter()
                    .filter_map(|text| KeyChord::parse(text, action.context()).ok())
                    .collect()
            }))
            .collect();
        Self { chords }
    }

    /// Apply the keys table from configuration on top of the defaults. Returns the map and
    /// one plain notice per problem: first those about each entry, in key-name order, then
    /// the conflicts, in action order of the action that lost.
    #[must_use]
    pub fn from_entries(entries: &BTreeMap<String, Vec<String>>) -> (Self, Vec<String>) {
        let mut map = Self::defaults();
        let mut explicit = [false; Action::ALL.len() + ModelAction::ALL.len()];
        let mut notices = Vec::new();

        for (name, texts) in entries {
            let Some(action) = Binding::from_config_name(name) else {
                let known: Vec<&str> = Action::ALL.iter().map(|a| a.config_name()).collect();
                notices.push(format!(
                    "Ignoring [keys] {name} in {FILE}: it is not a known action. Known actions: \
                     {}.",
                    known.join(", ")
                ));
                continue;
            };
            let mut parsed: Vec<KeyChord> = Vec::new();
            let mut fixed = false;
            for text in texts {
                match KeyChord::parse(text, parse_context(action)) {
                    Ok(chord) => {
                        // A written binding that only repeats the default needs no warning.
                        let is_default = action.default_chords().iter().any(|default| {
                            KeyChord::parse(default, parse_context(action)).as_ref() == Ok(&chord)
                        });
                        if chord.is_risky() && !is_default {
                            notices.push(format!(
                                "[keys] {name} in {FILE}: {} may not reach Talaria in some \
                                 terminals (the system, Herdr or terminal flow control can take \
                                 it); the binding is kept.",
                                chord.label()
                            ));
                        }
                        if !parsed.contains(&chord) {
                            parsed.push(chord);
                        }
                    }
                    Err(reason) if reason == FIXED_KEY_REASON => {
                        // The context fixes this key. One notice, and the default stands.
                        notices.push(format!(
                            "[keys] {name} in {FILE}: \"{text}\" is unreadable in the session \
                             picker ({reason}); the default ({}) is kept.",
                            map.labels_of(action)
                        ));
                        fixed = true;
                        break;
                    }
                    Err(reason) => notices.push(format!(
                        "[keys] {name} in {FILE}: \"{text}\" is not a key Talaria understands \
                         ({reason}); ignoring it."
                    )),
                }
            }
            if fixed {
                continue;
            }
            if !texts.is_empty() && parsed.is_empty() {
                notices.push(format!(
                    "[keys] {name} in {FILE} has no usable key; using the default ({}).",
                    map.labels_of(action)
                ));
                continue;
            }
            map.chords[action.index()] = parsed;
            explicit[action.index()] = true;
        }

        notices.extend(map.resolve_conflicts(&explicit));
        (map, notices)
    }

    /// The labels of one action's chords, for a notice.
    fn labels_of(&self, action: Binding) -> String {
        self.chords[action.index()]
            .iter()
            .map(KeyChord::label)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Give each chord held by more than one action of a context to one winner.
    fn resolve_conflicts(&mut self, explicit: &[bool]) -> Vec<String> {
        // (loser index, notice), sorted by loser at the end.
        let mut lost: Vec<(usize, String)> = Vec::new();
        for holder in Binding::all() {
            let mut position = 0;
            while position < self.chords[holder.index()].len() {
                let chord = self.chords[holder.index()][position].clone();
                let holders: Vec<Binding> = Binding::all()
                    .filter(|other| {
                        parse_context(*other) == parse_context(holder)
                            && self.chords[other.index()].contains(&chord)
                    })
                    .collect();
                position += 1;
                if holders.len() < 2 {
                    continue;
                }
                let Some(winner) = holders
                    .iter()
                    .copied()
                    .find(|action| explicit[action.index()])
                    .or_else(|| holders.first().copied())
                else {
                    continue;
                };
                for loser in holders.into_iter().filter(|action| *action != winner) {
                    let chords = &mut self.chords[loser.index()];
                    chords.retain(|held| *held != chord);
                    let (loser_name, winner_name, label) =
                        (loser.config_name(), winner.config_name(), chord.label());
                    if explicit[loser.index()] {
                        lost.push((
                            loser.index(),
                            format!(
                                "[keys] {loser_name} in {FILE}: {label} is already bound to \
                                 {winner_name}; keeping it on {winner_name} only."
                            ),
                        ));
                    } else if chords.is_empty() {
                        lost.push((
                            loser.index(),
                            format!(
                                "[keys] {loser_name} in {FILE} lost its default key {label} \
                                 because {winner_name} now uses it; {loser_name} has no key \
                                 until you bind one."
                            ),
                        ));
                    }
                    if loser == holder {
                        position -= 1;
                    }
                }
            }
        }
        lost.sort_by_key(|(index, _)| *index);
        lost.into_iter().map(|(_, notice)| notice).collect()
    }

    /// The completion-list action bound to this key, if any. Only the four actions of
    /// [`KeyContext::Completion`] are looked at, so a chord shared with the composer still
    /// reaches the list while it is open.
    #[must_use]
    pub fn completion(&self, key: &Key) -> Option<CompletionAction> {
        Action::ALL
            .into_iter()
            .filter(|action| action.context() == KeyContext::Completion)
            .find(|action| self.chords(*action).iter().any(|chord| chord.matches(key)))
            .and_then(CompletionAction::from_action)
    }

    /// The action bound to this key, if any.
    /// The action bound to this key in the given context, if any.
    ///
    /// Only the actions of that context are considered, so a key that names a transcript
    /// action never stops the composer from typing it.
    #[must_use]
    pub fn resolve(&self, key: &Key, context: KeyContext) -> Option<Action> {
        Action::ALL
            .into_iter()
            .filter(|action| {
                action.context() == context
                    || (context == KeyContext::Agents && *action == Action::AgentsPanel)
            })
            .find(|action| self.chords(*action).iter().any(|chord| chord.matches(key)))
    }

    /// The action bound to this key in one context. Issue 232. Queue and steer.
    #[must_use]
    pub fn resolve_in(&self, key: &Key, context: KeyContext) -> Option<Action> {
        Action::ALL.into_iter().find(|action| {
            (action.context() == context
                || (context == KeyContext::Agents && *action == Action::AgentsPanel))
                && self.chords(*action).iter().any(|chord| chord.matches(key))
        })
    }

    /// Model actions are resolved in their active context before ordinary composer actions.
    #[must_use]
    pub fn resolve_model(&self, context: KeyContext, key: &Key) -> Option<ModelAction> {
        ModelAction::ALL.into_iter().find(|action| {
            action.context() == context
                && self
                    .model_chords(*action)
                    .iter()
                    .any(|chord| chord.matches(key))
        })
    }

    #[must_use]
    pub fn model_chords(&self, action: ModelAction) -> &[KeyChord] {
        &self.chords[Binding::Model(action).index()]
    }

    /// Chords bound to one action, in binding order.
    #[must_use]
    pub fn chords(&self, action: Action) -> &[KeyChord] {
        &self.chords[action.index()]
    }

    /// Label of the first chord bound to an action.
    #[must_use]
    pub fn label(&self, action: Action) -> Option<String> {
        self.chords(action).first().map(KeyChord::label)
    }
}

/// The context a chord of a binding is read in.
///
/// The transcript key is read in the transcript context, so a plain letter may open the
/// transcript. Every other binding is read in the context it runs in.
fn parse_context(binding: Binding) -> KeyContext {
    match binding {
        Binding::Composer(Action::TranscriptOpen) => KeyContext::Pager,
        other => other.context(),
    }
}

/// The built-in chords of one action. Every default text parses; the defaults test checks it.
fn default_chords(action: Action) -> Vec<KeyChord> {
    action
        .default_chords()
        .iter()
        .filter_map(|text| KeyChord::parse(text, action.context()).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::{Action, CompletionAction, Key, KeyChord, KeyContext, KeyMap, ModelAction};

    fn parse(text: &str) -> Result<KeyChord, String> {
        KeyChord::parse(text, KeyContext::Composer)
    }

    fn labels(map: &KeyMap, action: Action) -> Vec<String> {
        map.chords(action).iter().map(KeyChord::label).collect()
    }

    fn entries(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(name, chords)| {
                (
                    (*name).to_string(),
                    chords.iter().map(|chord| (*chord).to_string()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn chord_parse_accepts_and_refuses() {
        let ctrl_j = parse("ctrl+j").expect("ctrl+j");
        assert_eq!(ctrl_j.label(), "Ctrl+J");
        assert!(!ctrl_j.needs_enhancement());
        let shift_enter = parse("Shift+Enter").expect("Shift+Enter");
        assert_eq!(shift_enter.label(), "Shift+Enter");
        assert!(shift_enter.needs_enhancement());
        let f24 = parse("f24").expect("f24");
        assert_eq!(f24.label(), "F24");
        assert!(!f24.needs_enhancement());

        assert_eq!(parse("control+j").unwrap().label(), "Ctrl+J");
        assert_eq!(parse("option+enter").unwrap().label(), "Alt+Enter");
        assert_eq!(parse("escape").unwrap().label(), "Esc");

        let refused = [
            ("", "empty"),
            (" ", "empty"),
            ("   ", "empty"),
            ("+", "missing key"),
            ("ctrl+", "missing key"),
            ("ctrl++", "missing key"),
            ("hyper+a", "unknown modifier \"hyper\""),
            ("banana", "unknown key \"banana\""),
            ("a", "a plain letter would stop that key from typing"),
            ("A", "a plain letter would stop that key from typing"),
            ("shift+a", "a plain letter would stop that key from typing"),
            ("shift+x", "a plain letter would stop that key from typing"),
            ("f25", "unknown key \"f25\""),
            ("ctrl+ctrl+a", "repeated modifier \"ctrl\""),
        ];
        for (text, reason) in refused {
            assert_eq!(parse(text).unwrap_err(), reason, "{text:?}");
        }
    }

    #[test]
    fn chord_matches_crossterm_events() {
        let shift_enter = parse("shift+enter").unwrap();
        let plain_enter = parse("enter").unwrap();
        let shifted = Key::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        assert!(shift_enter.matches(&shifted));
        assert!(!plain_enter.matches(&shifted));

        let map = KeyMap::defaults();
        let backspace = Key::from(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        let ctrl_h = Key::from(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL));
        assert_eq!(
            map.resolve(&backspace, KeyContext::Composer),
            Some(Action::DeleteBackward)
        );
        assert_eq!(
            map.resolve(&ctrl_h, KeyContext::Composer),
            Some(Action::DeleteBackward)
        );

        let hyper = Key::from(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::HYPER));
        assert_eq!(map.resolve(&hyper, KeyContext::Composer), None);
        assert!(!shift_enter.matches(&hyper));

        let back_tab = Key::from(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(
            map.resolve(&back_tab, KeyContext::Composer),
            Some(Action::ApprovalsToggle)
        );

        let release = Key::from(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        assert_eq!(map.resolve(&release, KeyContext::Composer), None);
        assert!(!plain_enter.matches(&release));

        let repeat = Key::from(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ));
        assert_eq!(
            map.resolve(&repeat, KeyContext::Composer),
            Some(Action::Submit)
        );
        assert!(plain_enter.matches(&repeat));
    }

    #[test]
    fn completion_chords_do_not_shadow_composer_comforts() {
        let (map, notices) =
            KeyMap::from_entries(&entries(&[("completion_next", &["ctrl+g", "ctrl+z"])]));
        assert!(notices.is_empty(), "{notices:?}");
        for (character, action) in [('g', Action::OpenEditor), ('z', Action::Suspend)] {
            let key = Key::from(KeyEvent::new(
                KeyCode::Char(character),
                KeyModifiers::CONTROL,
            ));
            assert_eq!(map.resolve(&key, KeyContext::Composer), Some(action));
            assert_eq!(map.completion(&key), Some(CompletionAction::Next));
        }
    }

    #[test]
    fn defaults_are_safe_and_unambiguous() {
        assert_eq!(Action::ALL.len(), 131);
        for action in Action::ALL {
            let wanted = match action {
                Action::CompletionNext
                | Action::CompletionPrev
                | Action::CompletionAccept
                | Action::CompletionClose => KeyContext::Completion,
                Action::PagerLineUp
                | Action::PagerLineDown
                | Action::PagerPageUp
                | Action::PagerPageDown
                | Action::PagerHalfUp
                | Action::PagerHalfDown
                | Action::PagerTop
                | Action::PagerBottom
                | Action::PagerPrevPrompt
                | Action::PagerNextPrompt
                | Action::PagerSearch
                | Action::PagerNextMatch
                | Action::PagerPrevMatch
                | Action::PagerDetails
                | Action::PagerClose => KeyContext::Pager,
                Action::QueueEdit
                | Action::QueueRemove
                | Action::QueueSteer
                | Action::QueueSendNow
                | Action::QueueMoveUp
                | Action::QueueMoveDown
                | Action::QueueLeave => KeyContext::QueueSelection,
                Action::PickerFormatMarkdown
                | Action::PickerFormatJson
                | Action::PickerFormatHtml
                | Action::PickerToggleRedact => KeyContext::SessionPickerExport,
                Action::PickerUp
                | Action::PickerDown
                | Action::PickerPageUp
                | Action::PickerPageDown
                | Action::PickerTop
                | Action::PickerBottom
                | Action::PickerOpen
                | Action::PickerRename
                | Action::PickerBranch
                | Action::PickerExport
                | Action::PickerSearch
                | Action::PickerProfileNext
                | Action::PickerProfilePrev
                | Action::PickerRefresh
                | Action::PickerClose => KeyContext::SessionPicker,
                Action::FleetUp
                | Action::FleetDown
                | Action::FleetPageUp
                | Action::FleetPageDown
                | Action::FleetTop
                | Action::FleetBottom
                | Action::FleetOpen
                | Action::FleetOpenInPane
                | Action::FleetPeek
                | Action::FleetReply
                | Action::FleetStop
                | Action::FleetAnswer
                | Action::FleetFilter
                | Action::FleetProfileNext
                | Action::FleetProfilePrev
                | Action::FleetNew
                | Action::FleetNewWorktree
                | Action::FleetReview => KeyContext::Fleet,
                Action::ReviewUp
                | Action::ReviewDown
                | Action::ReviewTop
                | Action::ReviewBottom
                | Action::ReviewShowDiff
                | Action::ReviewDiffPageUp
                | Action::ReviewDiffPageDown
                | Action::ReviewScope
                | Action::ReviewStage
                | Action::ReviewStageAll
                | Action::ReviewUnstageAll
                | Action::ReviewDiscard
                | Action::ReviewCommit
                | Action::ReviewPush
                | Action::ReviewPullRequest
                | Action::ReviewOpenPr
                | Action::ReviewRefresh => KeyContext::Review,
                Action::CommitSubmit | Action::CommitDraft => KeyContext::CommitMessage,
                Action::AgentsUp
                | Action::AgentsDown
                | Action::AgentsTail
                | Action::AgentsDetail
                | Action::AgentsBack
                | Action::AgentsClose
                | Action::AgentsSteer
                | Action::AgentsStop
                | Action::AgentsStopAll
                | Action::AgentsPause
                | Action::AgentsNextSession
                | Action::AgentsPageUp
                | Action::AgentsPageDown
                | Action::AgentsTop
                | Action::AgentsBottom
                | Action::AgentsOpenInPane => KeyContext::Agents,
                _ => KeyContext::Composer,
            };
            assert_eq!(action.context(), wanted);
            let name = action.config_name();
            assert_eq!(Action::from_config_name(name), Some(action), "{name}");
            let context = match action {
                Action::CompletionNext
                | Action::CompletionPrev
                | Action::CompletionAccept
                | Action::CompletionClose => KeyContext::Completion,
                Action::PagerLineUp
                | Action::PagerLineDown
                | Action::PagerPageUp
                | Action::PagerPageDown
                | Action::PagerHalfUp
                | Action::PagerHalfDown
                | Action::PagerTop
                | Action::PagerBottom
                | Action::PagerPrevPrompt
                | Action::PagerNextPrompt
                | Action::PagerSearch
                | Action::PagerNextMatch
                | Action::PagerPrevMatch
                | Action::PagerDetails
                | Action::PagerClose => KeyContext::Pager,
                Action::QueueEdit
                | Action::QueueRemove
                | Action::QueueSteer
                | Action::QueueSendNow
                | Action::QueueMoveUp
                | Action::QueueMoveDown
                | Action::QueueLeave => KeyContext::QueueSelection,
                Action::PickerFormatMarkdown
                | Action::PickerFormatJson
                | Action::PickerFormatHtml
                | Action::PickerToggleRedact => KeyContext::SessionPickerExport,
                Action::PickerUp
                | Action::PickerDown
                | Action::PickerPageUp
                | Action::PickerPageDown
                | Action::PickerTop
                | Action::PickerBottom
                | Action::PickerOpen
                | Action::PickerRename
                | Action::PickerBranch
                | Action::PickerExport
                | Action::PickerSearch
                | Action::PickerProfileNext
                | Action::PickerProfilePrev
                | Action::PickerRefresh
                | Action::PickerClose => KeyContext::SessionPicker,
                Action::FleetUp
                | Action::FleetDown
                | Action::FleetPageUp
                | Action::FleetPageDown
                | Action::FleetTop
                | Action::FleetBottom
                | Action::FleetOpen
                | Action::FleetOpenInPane
                | Action::FleetPeek
                | Action::FleetReply
                | Action::FleetStop
                | Action::FleetAnswer
                | Action::FleetFilter
                | Action::FleetProfileNext
                | Action::FleetProfilePrev
                | Action::FleetNew
                | Action::FleetNewWorktree
                | Action::FleetReview => KeyContext::Fleet,
                Action::ReviewUp
                | Action::ReviewDown
                | Action::ReviewTop
                | Action::ReviewBottom
                | Action::ReviewShowDiff
                | Action::ReviewDiffPageUp
                | Action::ReviewDiffPageDown
                | Action::ReviewScope
                | Action::ReviewStage
                | Action::ReviewStageAll
                | Action::ReviewUnstageAll
                | Action::ReviewDiscard
                | Action::ReviewCommit
                | Action::ReviewPush
                | Action::ReviewPullRequest
                | Action::ReviewOpenPr
                | Action::ReviewRefresh => KeyContext::Review,
                Action::CommitSubmit | Action::CommitDraft => KeyContext::CommitMessage,
                Action::AgentsUp
                | Action::AgentsDown
                | Action::AgentsTail
                | Action::AgentsDetail
                | Action::AgentsBack
                | Action::AgentsClose
                | Action::AgentsSteer
                | Action::AgentsStop
                | Action::AgentsStopAll
                | Action::AgentsPause
                | Action::AgentsNextSession
                | Action::AgentsPageUp
                | Action::AgentsPageDown
                | Action::AgentsTop
                | Action::AgentsBottom
                | Action::AgentsOpenInPane => KeyContext::Agents,
                _ => KeyContext::Composer,
            };
            assert_eq!(action.context(), context, "{name}");
        }
        assert_eq!(Action::QueueEdit.context(), KeyContext::QueueSelection);
        assert_eq!(Action::QueueLeave.context(), KeyContext::QueueSelection);
        assert_eq!(Action::ComposerSendNow.context(), KeyContext::Composer);
        assert_eq!(Action::ComposerSteer.context(), KeyContext::Composer);
        assert!(Action::ComposerSteer.default_chords().is_empty());
        assert_eq!(Action::from_config_name("not_an_action"), None);

        let map = KeyMap::defaults();
        let expected = [
            (Action::Submit, vec!["Enter"]),
            (Action::Newline, vec!["Shift+Enter", "Alt+Enter", "Ctrl+J"]),
            (Action::Interrupt, vec!["Esc"]),
            (Action::Cancel, vec!["Ctrl+C"]),
            (Action::MoveLeft, vec!["Left"]),
            (Action::MoveRight, vec!["Right"]),
            (Action::MoveUp, vec!["Up"]),
            (Action::MoveDown, vec!["Down"]),
            (Action::LineStart, vec!["Home", "Ctrl+A"]),
            (Action::LineEnd, vec!["End", "Ctrl+E"]),
            (Action::BufferStart, vec!["Ctrl+Home"]),
            (Action::BufferEnd, vec!["Ctrl+End"]),
            (Action::WordLeft, vec!["Alt+Left", "Ctrl+Left", "Alt+B"]),
            (Action::WordRight, vec!["Alt+Right", "Ctrl+Right", "Alt+F"]),
            (Action::DeleteBackward, vec!["Backspace", "Ctrl+H"]),
            (Action::DeleteForward, vec!["Delete"]),
            (Action::DeleteWordBackward, vec!["Ctrl+W", "Alt+Backspace"]),
            (Action::KillToLineEnd, vec!["Ctrl+K"]),
            (Action::KillToLineStart, vec!["Ctrl+U"]),
            (Action::Yank, vec!["Ctrl+Y"]),
            (Action::CompletionNext, vec!["Down", "Ctrl+N"]),
            (Action::CompletionPrev, vec!["Up", "Ctrl+P"]),
            (Action::CompletionAccept, vec!["Tab"]),
            (Action::CompletionClose, vec!["Esc"]),
            (Action::OpenEditor, vec!["Ctrl+G", "Alt+G"]),
            (Action::HistorySearch, vec!["Ctrl+R"]),
            (Action::Suspend, vec!["Ctrl+Z"]),
            (Action::AttachClipboardImage, vec!["Ctrl+V", "Alt+V"]),
            (Action::TranscriptOpen, vec!["Ctrl+O"]),
            (Action::PagerLineUp, vec!["Up", "K"]),
            (Action::PagerLineDown, vec!["Down", "J"]),
            (Action::PagerPageUp, vec!["PageUp", "B"]),
            (Action::PagerPageDown, vec!["PageDown", "Space"]),
            (Action::PagerHalfUp, vec!["Ctrl+U"]),
            (Action::PagerHalfDown, vec!["Ctrl+D"]),
            (Action::PagerTop, vec!["G", "Home"]),
            (Action::PagerBottom, vec!["Shift+G", "End"]),
            (Action::PagerPrevPrompt, vec!["{"]),
            (Action::PagerNextPrompt, vec!["}"]),
            (Action::PagerSearch, vec!["/"]),
            (Action::PagerNextMatch, vec!["N"]),
            (Action::PagerPrevMatch, vec!["Shift+N"]),
            (Action::PagerDetails, vec!["T"]),
            (Action::PagerClose, vec!["Q"]),
            (Action::ToggleTasks, vec!["Ctrl+T"]),
            (Action::ApprovalsToggle, vec!["Shift+Tab"]),
            (Action::QueueEdit, vec!["Enter", "E"]),
            (Action::QueueRemove, vec!["D", "Delete", "Ctrl+X"]),
            (Action::QueueSteer, vec!["S"]),
            (Action::QueueSendNow, vec!["N"]),
            (Action::QueueMoveUp, vec!["Shift+K", "Shift+Up"]),
            (Action::QueueMoveDown, vec!["Shift+J", "Shift+Down"]),
            (Action::QueueLeave, vec!["Esc"]),
            (Action::ComposerSendNow, vec!["Ctrl+Enter"]),
            (Action::ComposerSteer, vec![]),
            (Action::SessionPickerOpen, vec![]),
            (Action::PickerUp, vec!["Up", "K"]),
            (Action::PickerDown, vec!["Down", "J"]),
            (Action::PickerPageUp, vec!["PageUp"]),
            (Action::PickerPageDown, vec!["PageDown"]),
            (Action::PickerTop, vec!["Home", "G"]),
            (Action::PickerBottom, vec!["End", "Shift+G"]),
            (Action::PickerOpen, vec!["Enter"]),
            (Action::PickerRename, vec!["R"]),
            (Action::PickerBranch, vec!["B"]),
            (Action::PickerExport, vec!["E"]),
            (Action::PickerSearch, vec!["/"]),
            (Action::PickerProfileNext, vec!["Tab"]),
            (Action::PickerProfilePrev, vec!["Shift+Tab"]),
            (Action::PickerRefresh, vec!["Ctrl+R"]),
            (Action::PickerClose, vec!["Q"]),
            (Action::PickerFormatMarkdown, vec!["M"]),
            (Action::PickerFormatJson, vec!["J"]),
            (Action::PickerFormatHtml, vec!["H"]),
            (Action::PickerToggleRedact, vec!["X"]),
            (Action::AgentsPanel, vec!["Alt+S"]),
            (Action::AgentsUp, vec!["Up", "K"]),
            (Action::AgentsDown, vec!["Down", "J"]),
            (Action::AgentsTail, vec!["Enter"]),
            (Action::AgentsDetail, vec!["D", "Right"]),
            (Action::AgentsBack, vec!["Esc", "Left"]),
            (Action::AgentsClose, vec!["Q"]),
            (Action::AgentsSteer, vec!["E"]),
            (Action::AgentsStop, vec!["X"]),
            (Action::AgentsStopAll, vec!["Shift+X"]),
            (Action::AgentsPause, vec!["P"]),
            (Action::AgentsNextSession, vec!["Tab"]),
            (Action::AgentsPageUp, vec!["PageUp"]),
            (Action::AgentsPageDown, vec!["PageDown"]),
            (Action::AgentsTop, vec!["G", "Home"]),
            (Action::AgentsBottom, vec!["Shift+G", "End"]),
            (Action::AgentsOpenInPane, vec!["O"]),
            (Action::FleetNew, vec!["N"]),
            (Action::FleetNewWorktree, vec!["W"]),
            (Action::FleetReview, vec!["V"]),
        ];
        for (action, want) in expected {
            assert_eq!(labels(&map, action), want, "{}", action.config_name());
        }

        let risky = ["F1", "F2", "Ctrl+B", "Ctrl+S"];
        // The same chord may belong to two contexts. It may not belong to two actions of one
        // context.
        let mut seen: Vec<(KeyContext, KeyChord, Action)> = Vec::new();
        for action in Action::ALL {
            for chord in map.chords(action) {
                let label = chord.label();
                // The review's commit editor submits with Ctrl+S, as its issue settled; raw mode
                // turns flow control off, and the key is configurable.
                let settled = action == Action::CommitSubmit && label == "Ctrl+S";
                assert!(
                    settled || !risky.contains(&label.as_str()),
                    "{label} is bound to {}",
                    action.config_name()
                );
                if let Some((_, _, other)) = seen
                    .iter()
                    .find(|(context, held, _)| *context == action.context() && held == chord)
                {
                    panic!(
                        "{} shares {label} with {}",
                        action.config_name(),
                        other.config_name()
                    );
                }
                seen.push((action.context(), chord.clone(), action));
            }
        }

        for (code, mods) in [
            (KeyCode::F(1), KeyModifiers::NONE),
            (KeyCode::F(2), KeyModifiers::NONE),
            (KeyCode::Char('b'), KeyModifiers::CONTROL),
            (KeyCode::Char('s'), KeyModifiers::CONTROL),
        ] {
            let key = Key::from(KeyEvent::new(code, mods));
            assert_eq!(map.resolve(&key, KeyContext::Composer), None);
        }
    }

    #[test]
    fn from_entries_reports_each_problem_once() {
        let (map, notices) = KeyMap::from_entries(&entries(&[
            ("bogus", &["x"]),
            ("delete_forward", &["ctrl+s"]),
            ("line_start", &["hyper+a"]),
            ("move_left", &["banana", "left"]),
            ("submit", &["alt+enter"]),
            ("word_left", &["ctrl+o"]),
            ("word_right", &["ctrl+o"]),
            ("yank", &[]),
        ]));
        let known = Action::ALL
            .iter()
            .map(|action| action.config_name())
            .collect::<Vec<_>>()
            .join(", ");
        let expected = [
            format!(
                "Ignoring [keys] bogus in ~/.talaria/config.toml: it is not a known action. \
                 Known actions: {known}."
            ),
            "[keys] delete_forward in ~/.talaria/config.toml: Ctrl+S may not reach Talaria in \
             some terminals (the system, Herdr or terminal flow control can take it); the \
             binding is kept."
                .to_string(),
            "[keys] line_start in ~/.talaria/config.toml: \"hyper+a\" is not a key Talaria \
             understands (unknown modifier \"hyper\"); ignoring it."
                .to_string(),
            "[keys] line_start in ~/.talaria/config.toml has no usable key; using the default \
             (Home, Ctrl+A)."
                .to_string(),
            "[keys] move_left in ~/.talaria/config.toml: \"banana\" is not a key Talaria \
             understands (unknown key \"banana\"); ignoring it."
                .to_string(),
            "[keys] word_right in ~/.talaria/config.toml: Ctrl+O is already bound to word_left; \
             keeping it on word_left only."
                .to_string(),
        ];
        assert_eq!(notices, expected);
        assert_eq!(labels(&map, Action::Submit), vec!["Alt+Enter".to_string()]);
        assert_eq!(
            labels(&map, Action::Newline),
            vec!["Shift+Enter".to_string(), "Ctrl+J".to_string()]
        );
        assert_eq!(labels(&map, Action::Yank), Vec::<String>::new());
        assert_eq!(labels(&map, Action::WordRight), Vec::<String>::new());
        assert_eq!(labels(&map, Action::MoveLeft), vec!["Left".to_string()]);
        assert_eq!(
            labels(&map, Action::DeleteForward),
            vec!["Ctrl+S".to_string()]
        );
        assert_eq!(
            labels(&map, Action::LineStart),
            vec!["Home".to_string(), "Ctrl+A".to_string()]
        );
        assert!(
            notices
                .iter()
                .all(|notice| !notice.to_lowercase().contains("ctrl+j")),
            "{notices:?}"
        );

        let (map, notices) = KeyMap::from_entries(&entries(&[("submit", &["esc"])]));
        assert_eq!(
            notices,
            vec![
                "[keys] interrupt in ~/.talaria/config.toml lost its default key Esc because \
                 submit now uses it; interrupt has no key until you bind one."
                    .to_string()
            ]
        );
        assert_eq!(labels(&map, Action::Submit), vec!["Esc".to_string()]);
        assert_eq!(labels(&map, Action::Interrupt), Vec::<String>::new());
    }

    #[test]
    fn a_fixed_key_keeps_the_default_and_reports_once() {
        let (map, notices) = KeyMap::from_entries(&entries(&[("picker_close", &["esc"])]));
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("unreadable"), "the notice: {notices:?}");
        assert_eq!(map, KeyMap::defaults());
    }

    #[test]
    fn model_bindings_use_their_own_context_and_keep_composer_editing() {
        let map = KeyMap::defaults();
        let left = Key::from(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(
            map.resolve(&left, KeyContext::Composer),
            Some(Action::MoveLeft)
        );
        assert_eq!(
            map.resolve_model(KeyContext::ModelPicker, &left),
            Some(ModelAction::EffortDown)
        );
        assert_eq!(map.resolve_model(KeyContext::Composer, &left), None);
        let open = Key::from(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT));
        assert_eq!(
            map.resolve_model(KeyContext::Composer, &open),
            Some(ModelAction::ModelPicker)
        );
        assert_eq!(map.resolve_model(KeyContext::ModelPicker, &open), None);
        assert_eq!(Action::ALL.len(), 131);
    }

    #[test]
    fn explicit_model_binding_wins_within_its_context_only() {
        let (map, notices) = KeyMap::from_entries(&entries(&[
            ("model_picker", &["ctrl+u"]),
            ("model_scope", &["ctrl+r"]),
        ]));
        assert_eq!(map.chords(Action::KillToLineStart), &[]);
        assert_eq!(map.model_chords(ModelAction::ModelRefresh), &[]);
        let ctrl_r = Key::from(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
        assert_eq!(
            map.resolve_model(KeyContext::ModelPicker, &ctrl_r),
            Some(ModelAction::ModelScope)
        );
        assert_eq!(notices.len(), 2);
        let (map, notices) = KeyMap::from_entries(&entries(&[("model_refresh", &["bad chord"])]));
        assert_eq!(notices.len(), 2);
        assert_eq!(
            map.model_chords(ModelAction::ModelRefresh)[0].label(),
            "Ctrl+R"
        );
    }
}
