//! The review state machine. Pure: no input, output or clock reads. Issue #245.

use std::path::PathBuf;

use crate::directory::rows::RowKey;
use crate::review::draft::DraftError;
use crate::review::model::{BranchHead, ChangedFile};
use crate::review::pr::PrStatus;
use crate::review::ship::GhReady;

/// Which changes the review shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The working tree against `HEAD`. Every action runs here.
    Uncommitted,
    /// The branch against its base. Read-only.
    Branch,
}

/// Where the keys go.
#[derive(Clone, Debug, PartialEq)]
pub enum ReviewFocus {
    /// The file list, with the diff panel below.
    List,
    /// The diff fills the list's place. Windows under 24 rows.
    DiffOnly,
    /// Discard asks first. The path it names.
    ConfirmDiscard(String),
    /// The commit editor.
    Commit,
    /// A draft over typed text asks first.
    ConfirmReplaceDraft,
    /// A draft pull request asks first.
    ConfirmPr,
}

/// The folder under review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewTarget {
    /// The folder under review.
    pub folder: PathBuf,
    /// The work tree root that holds it.
    pub main_root: PathBuf,
    /// The reviewed session, when the review opened from one.
    pub session: Option<RowKey>,
    /// The worktree record behind the folder, when there is one.
    pub record: Option<RecordView>,
    /// The header title.
    pub title: String,
}

/// The worktree record behind a review, as the review needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordView {
    /// The worktree branch.
    pub branch: String,
    /// The ref the tree was branched from.
    pub base_ref: String,
    /// The commit the tree was branched from.
    pub base_commit: String,
}

/// One file listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The branch head the listing was read at.
    pub head: BranchHead,
    /// The file rows, sorted by path, capped.
    pub files: Vec<ChangedFile>,
    /// The full count before the cap.
    pub total: usize,
    /// The Branch scope base, when the scope is Branch.
    pub base: Option<String>,
    /// The default branch. `None` means not known yet.
    pub default_branch: Option<String>,
    /// An `origin` remote exists.
    pub has_origin: bool,
    /// Active commit hooks in the effective hooks folder.
    pub hooks: u32,
}

/// One input to the state machine.
#[derive(Clone, Debug, PartialEq)]
pub enum ReviewInput {
    /// A review key.
    Key(ReviewKey),
    /// Pasted text for the editor.
    Paste(String),
    /// A listing arrived.
    Listed {
        /// Which scope it was read for.
        scope: Scope,
        /// The listing, or the notice text.
        result: Result<Listing, String>,
    },
    /// A file diff arrived.
    DiffLoaded {
        /// Which path it was read for.
        path: String,
        /// Which scope it was read for.
        scope: Scope,
        /// The diff text, or the notice text.
        result: Result<String, String>,
    },
    /// A git action finished.
    ActionDone {
        /// Which action finished.
        action: GitAction,
        /// The notice on success, the error on failure.
        result: Result<String, String>,
    },
    /// A drafted message arrived.
    Drafted {
        /// The draft sequence. Old replies are ignored.
        seq: u64,
        /// The draft text, or why drafting failed.
        result: Result<String, DraftError>,
    },
    /// Pull-request status for the branch, when known.
    PrStatus(Option<PrStatus>),
    /// `gh` readiness changed.
    GhReadiness(GhReady),
    /// The session's model and draft gate.
    SessionInfo {
        /// The session's own model, when known.
        model: Option<String>,
        /// Whether drafting may run.
        can_draft: bool,
    },
    /// The reviewed session started or stopped working.
    SessionWorking(bool),
    /// A clock tick for polls and notice expiry.
    Tick {
        /// Seconds since 1970.
        now: f64,
    },
}

/// One review key, after the key map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewKey {
    Up,
    Down,
    Top,
    Bottom,
    PageUp,
    PageDown,
    ShowDiff,
    Scope,
    StageToggle,
    StageAll,
    UnstageAll,
    Discard,
    Commit,
    Push,
    PullRequest,
    OpenPr,
    Refresh,
    Draft,
    Submit,
    Newline,
    Char(char),
    Backspace,
    Esc,
    CtrlC,
    Enter,
    Other,
}

/// One git mutation the service runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitAction {
    /// Stage paths.
    Stage(Vec<String>),
    /// Unstage paths.
    Unstage(Vec<String>),
    /// Discard one row. The row kind picks the commands.
    Discard(ChangedFile),
    /// Commit with a message, staging all first when asked.
    Commit {
        /// The commit message.
        message: String,
        /// Nothing was staged, so stage all first.
        stage_all: bool,
    },
    /// Push the branch to `origin`.
    Push,
    /// Open a draft pull request.
    CreatePr {
        /// The base branch.
        base: String,
        /// The head branch.
        branch: String,
    },
}

/// One effect for the service to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewEffect {
    /// Read a listing.
    List(Scope),
    /// Read one file's diff.
    LoadDiff {
        /// Which path to read.
        path: String,
        /// Which scope to read it for.
        scope: Scope,
    },
    /// Run one git mutation.
    Run(GitAction),
    /// Draft a message. Replies carry the sequence back.
    Draft {
        /// The draft sequence.
        seq: u64,
        /// The editor text at the press.
        avoid: String,
    },
    /// Refresh pull-request status at once.
    RefreshPr,
    /// Open a URL in the browser.
    OpenUrl(String),
    /// Close the review.
    Close,
    /// Draw another frame.
    Redraw,
}

/// The review state. Step 0 keeps it opaque; the reducer owns the fields.
#[derive(Debug)]
pub struct ReviewState;

impl ReviewState {
    /// Open a review. The effects read the first listing.
    pub fn open(target: ReviewTarget, now: f64) -> (ReviewState, Vec<ReviewEffect>) {
        let _ = (target, now);
        todo!()
    }

    /// Handle one input. The effects run outside the reducer.
    pub fn update(&mut self, input: ReviewInput) -> Vec<ReviewEffect> {
        let _ = input;
        todo!()
    }

    /// Borrow what the drawing needs.
    pub fn view(&self) -> ReviewView<'_> {
        todo!()
    }
}

/// What the drawing needs, borrowed from the state. Step 0 keeps it opaque;
/// the reducer owns the fields.
#[derive(Clone, Copy, Debug)]
pub struct ReviewView<'a>(pub std::marker::PhantomData<&'a ()>);
