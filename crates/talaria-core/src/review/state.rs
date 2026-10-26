//! The review state machine. Pure: no input, output or clock reads. Issue #245.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::diff::{DiffModel, parse_diff};
use crate::directory::rows::RowKey;
use crate::review::draft::DraftError;
use crate::review::model::{BranchHead, ChangedFile, DIFF_MAX_LINES, FileStatus, REVIEW_FILE_CAP};
use crate::review::pr::{PrLabel, PrStatus, ReviewDecision, pr_label};
use crate::review::ship::{
    GhReady, ShipRefusal, can_commit, can_open_pr, can_push, counted, in_worktrees, refusal_text,
};

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

/// How long an ordinary notice stays, in seconds.
const NOTICE_SECONDS: f64 = 8.0;
/// How often the list reloads while it has focus and nothing runs, in seconds.
const POLL_SECONDS: f64 = 5.0;
/// How many diff lines a page key scrolls.
const DIFF_PAGE: usize = 10;

/// Shown while the first listing has not arrived.
pub const LOADING: &str = "Loading changes\u{2026}";
/// Shown in place of an untracked folder's diff.
const UNTRACKED_FOLDER: &str = "Untracked folder: stage it to see its files.";

/// A notice, with the time it goes away. Without a time it stays until replaced.
#[derive(Clone, Debug)]
struct Notice {
    text: String,
    until: Option<f64>,
}

/// What the diff panel holds for one file.
#[derive(Clone, Debug)]
enum DiffBody {
    /// A parsed diff, with the number of lines cut past the limit.
    Parsed { model: DiffModel, cut: usize },
    /// A notice or an error in place of a diff.
    Text(String),
}

/// One file's diff, as it arrived.
#[derive(Clone, Debug)]
struct LoadedDiff {
    path: String,
    scope: Scope,
    body: DiffBody,
}

/// The review state. The reducer owns every field; the drawing borrows a view.
#[derive(Debug)]
pub struct ReviewState {
    target: ReviewTarget,
    scope: Scope,
    uncommitted: Option<Listing>,
    branch: Option<Listing>,
    cursor_path: Option<String>,
    cursor_index: usize,
    diff: Option<LoadedDiff>,
    diff_requested: Option<(String, Scope)>,
    diff_scroll: usize,
    focus: ReviewFocus,
    editor: String,
    last_draft: Option<String>,
    draft_seq: u64,
    drafting: Option<u64>,
    draft_hidden: bool,
    busy: Option<&'static str>,
    notice: Option<Notice>,
    pr: Option<PrStatus>,
    gh: Option<GhReady>,
    working: bool,
    model: Option<String>,
    can_draft: bool,
    now: f64,
    last_list_at: f64,
}

/// What a mutation key found out before it could run.
enum Gate {
    /// Go ahead.
    Open,
    /// Refused; the notice is set.
    Refused,
}

impl ReviewState {
    /// Open a review. The effects read the first listing.
    pub fn open(target: ReviewTarget, now: f64) -> (ReviewState, Vec<ReviewEffect>) {
        let can_draft = target.session.is_some();
        let state = ReviewState {
            target,
            scope: Scope::Uncommitted,
            uncommitted: None,
            branch: None,
            cursor_path: None,
            cursor_index: 0,
            diff: None,
            diff_requested: None,
            diff_scroll: 0,
            focus: ReviewFocus::List,
            editor: String::new(),
            last_draft: None,
            draft_seq: 0,
            drafting: None,
            draft_hidden: false,
            busy: None,
            notice: None,
            pr: None,
            gh: None,
            working: false,
            model: None,
            can_draft,
            now,
            last_list_at: now,
        };
        (state, vec![ReviewEffect::List(Scope::Uncommitted)])
    }

    /// Handle one input. The effects run outside the reducer.
    pub fn update(&mut self, input: ReviewInput) -> Vec<ReviewEffect> {
        match input {
            ReviewInput::Key(key) => self.key(key),
            ReviewInput::Paste(text) => self.paste(&text),
            ReviewInput::Listed { scope, result } => self.listed(scope, result),
            ReviewInput::DiffLoaded {
                path,
                scope,
                result,
            } => self.diff_loaded(path, scope, result),
            ReviewInput::ActionDone { action, result } => self.action_done(&action, result),
            ReviewInput::Drafted { seq, result } => self.drafted(seq, result),
            ReviewInput::PrStatus(status) => {
                if self.pr == status {
                    return Vec::new();
                }
                self.pr = status;
                vec![ReviewEffect::Redraw]
            }
            ReviewInput::GhReadiness(gh) => {
                self.gh = Some(gh);
                Vec::new()
            }
            ReviewInput::SessionInfo { model, can_draft } => {
                if self.model == model && self.can_draft == can_draft {
                    return Vec::new();
                }
                self.model = model;
                self.can_draft = can_draft;
                vec![ReviewEffect::Redraw]
            }
            ReviewInput::SessionWorking(working) => {
                if self.working == working {
                    return Vec::new();
                }
                self.working = working;
                vec![ReviewEffect::Redraw]
            }
            ReviewInput::Tick { now } => self.tick(now),
        }
    }

    // ----- state helpers -------------------------------------------------

    /// The folder under review and what it was opened from.
    #[must_use]
    pub fn target(&self) -> &ReviewTarget {
        &self.target
    }

    /// Which changes the review shows now.
    #[must_use]
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// The latest successful listing for the current scope.
    #[must_use]
    pub fn listing(&self) -> Option<&Listing> {
        match self.scope {
            Scope::Uncommitted => self.uncommitted.as_ref(),
            Scope::Branch => self.branch.as_ref(),
        }
    }

    /// A row of the current listing, by path.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<&ChangedFile> {
        self.files().iter().find(|file| file.path == path)
    }

    fn files(&self) -> &[ChangedFile] {
        self.listing()
            .map_or(&[], |listing| listing.files.as_slice())
    }

    fn cursor_file(&self) -> Option<&ChangedFile> {
        self.files().get(self.cursor_index)
    }

    fn notify(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            until: Some(self.now + NOTICE_SECONDS),
        });
    }

    fn notify_sticky(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            until: None,
        });
    }

    fn refuse(&mut self, refusal: &ShipRefusal) -> Vec<ReviewEffect> {
        self.notify(refusal_text(refusal));
        vec![ReviewEffect::Redraw]
    }

    /// Put the cursor back on its path, or on the row now at its place.
    fn sync_cursor(&mut self) {
        let found = self
            .cursor_path
            .as_ref()
            .and_then(|path| self.files().iter().position(|file| &file.path == path));
        let len = self.files().len();
        if len == 0 {
            self.cursor_path = None;
            self.cursor_index = 0;
            return;
        }
        self.cursor_index = found.unwrap_or_else(|| self.cursor_index.min(len - 1));
        self.cursor_path = Some(self.files()[self.cursor_index].path.clone());
    }

    fn move_cursor_to(&mut self, index: usize, effects: &mut Vec<ReviewEffect>) {
        let len = self.files().len();
        if len == 0 {
            return;
        }
        let index = index.min(len - 1);
        if index == self.cursor_index && self.cursor_path.is_some() {
            return;
        }
        self.cursor_index = index;
        self.cursor_path = Some(self.files()[index].path.clone());
        self.diff_scroll = 0;
        self.ensure_diff(effects);
        effects.push(ReviewEffect::Redraw);
    }

    /// Ask for the cursor row's diff when it is not loaded or asked for.
    fn ensure_diff(&mut self, effects: &mut Vec<ReviewEffect>) {
        let Some(file) = self.cursor_file() else {
            return;
        };
        if file.is_dir {
            return;
        }
        let key = (file.path.clone(), self.scope);
        if self.diff_requested.as_ref() == Some(&key) {
            return;
        }
        effects.push(ReviewEffect::LoadDiff {
            path: key.0.clone(),
            scope: key.1,
        });
        self.diff_requested = Some(key);
    }

    fn base_name(&self) -> Option<String> {
        self.target.record.as_ref().map(|record| {
            record
                .base_ref
                .strip_prefix("origin/")
                .unwrap_or(&record.base_ref)
                .to_owned()
        })
    }

    fn branch_name(&self) -> Option<&str> {
        self.listing()
            .and_then(|listing| listing.head.head.as_deref())
    }

    /// The refusals every mutation key shares: the read-only scope first,
    /// then a mutation already running.
    fn mutation_gate(&mut self) -> Gate {
        if self.scope == Scope::Branch {
            self.notify(refusal_text(&ShipRefusal::ReadOnlyScope));
            return Gate::Refused;
        }
        if let Some(action) = self.busy {
            self.notify(refusal_text(&ShipRefusal::Busy(action)));
            return Gate::Refused;
        }
        Gate::Open
    }

    fn run(&mut self, action: GitAction) -> Vec<ReviewEffect> {
        self.busy = Some(match action {
            GitAction::Stage(_) => "staging",
            GitAction::Unstage(_) => "unstaging",
            GitAction::Discard(_) => "discard",
            GitAction::Commit { .. } => "commit",
            GitAction::Push => "push",
            GitAction::CreatePr { .. } => "pull request",
        });
        vec![ReviewEffect::Run(action), ReviewEffect::Redraw]
    }

    // ----- inputs other than keys ----------------------------------------

    fn tick(&mut self, now: f64) -> Vec<ReviewEffect> {
        self.now = now;
        let mut effects = Vec::new();
        if self
            .notice
            .as_ref()
            .and_then(|notice| notice.until)
            .is_some_and(|until| now >= until)
        {
            self.notice = None;
            effects.push(ReviewEffect::Redraw);
        }
        if self.focus == ReviewFocus::List
            && self.busy.is_none()
            && now - self.last_list_at >= POLL_SECONDS
        {
            self.last_list_at = now;
            effects.push(ReviewEffect::List(self.scope));
        }
        effects
    }

    fn listed(&mut self, scope: Scope, result: Result<Listing, String>) -> Vec<ReviewEffect> {
        let mut listing = match result {
            Ok(listing) => listing,
            Err(text) => {
                if scope != self.scope {
                    return Vec::new();
                }
                self.notify(text);
                return vec![ReviewEffect::Redraw];
            }
        };
        if listing.files.len() > REVIEW_FILE_CAP {
            listing.total = listing.total.max(listing.files.len());
            listing.files.truncate(REVIEW_FILE_CAP);
        }
        let slot = match scope {
            Scope::Uncommitted => &mut self.uncommitted,
            Scope::Branch => &mut self.branch,
        };
        if slot.as_ref() == Some(&listing) {
            return Vec::new();
        }
        *slot = Some(listing);
        if scope != self.scope {
            return Vec::new();
        }
        self.sync_cursor();
        // The files changed, so the diff on screen may be stale.
        self.diff_requested = None;
        let mut effects = Vec::new();
        self.ensure_diff(&mut effects);
        effects.push(ReviewEffect::Redraw);
        effects
    }

    fn diff_loaded(
        &mut self,
        path: String,
        scope: Scope,
        result: Result<String, String>,
    ) -> Vec<ReviewEffect> {
        let on_cursor = self
            .cursor_file()
            .is_some_and(|file| file.path == path && scope == self.scope);
        if !on_cursor {
            return Vec::new();
        }
        let same_file = self
            .diff
            .as_ref()
            .is_some_and(|loaded| loaded.path == path && loaded.scope == scope);
        let body = match result {
            Ok(text) => {
                let (kept, cut) = cut_lines(&text, DIFF_MAX_LINES);
                DiffBody::Parsed {
                    model: parse_diff(kept),
                    cut,
                }
            }
            Err(text) => DiffBody::Text(text),
        };
        if !same_file {
            self.diff_scroll = 0;
        }
        self.diff = Some(LoadedDiff { path, scope, body });
        self.clamp_scroll();
        vec![ReviewEffect::Redraw]
    }

    fn clamp_scroll(&mut self) {
        let rows = match self.diff.as_ref().map(|loaded| &loaded.body) {
            Some(DiffBody::Parsed { model, .. }) => model.row_count() + model.notes.len(),
            _ => 0,
        };
        self.diff_scroll = self.diff_scroll.min(rows.saturating_sub(1));
    }

    fn action_done(
        &mut self,
        action: &GitAction,
        result: Result<String, String>,
    ) -> Vec<ReviewEffect> {
        self.busy = None;
        let mut effects = Vec::new();
        match result {
            Ok(text) => {
                self.notify(text);
                match action {
                    GitAction::Commit { .. } => {
                        self.editor.clear();
                        self.last_draft = None;
                        self.drafting = None;
                        self.focus = ReviewFocus::List;
                    }
                    GitAction::Push | GitAction::CreatePr { .. } => {
                        effects.push(ReviewEffect::RefreshPr);
                    }
                    _ => {}
                }
            }
            Err(text) => {
                self.notify(text);
                if matches!(action, GitAction::Push | GitAction::CreatePr { .. }) {
                    effects.push(ReviewEffect::RefreshPr);
                }
            }
        }
        self.last_list_at = self.now;
        self.diff_requested = None;
        effects.push(ReviewEffect::List(self.scope));
        effects.push(ReviewEffect::Redraw);
        effects
    }

    fn drafted(&mut self, seq: u64, result: Result<String, DraftError>) -> Vec<ReviewEffect> {
        if self.drafting != Some(seq) {
            return Vec::new();
        }
        self.drafting = None;
        match result {
            Ok(text) => {
                if text.trim() == self.editor.trim() {
                    self.notify("The model returned the same message.");
                } else {
                    self.editor.clone_from(&text);
                    self.last_draft = Some(text);
                    self.notice = None;
                }
            }
            Err(error) => {
                if error == DraftError::Unsupported {
                    self.draft_hidden = true;
                }
                self.notify(draft_error_text(&error));
            }
        }
        vec![ReviewEffect::Redraw]
    }

    fn paste(&mut self, text: &str) -> Vec<ReviewEffect> {
        if self.focus != ReviewFocus::Commit {
            return Vec::new();
        }
        if self.drafting.is_some() {
            self.notify("Wait for drafting to finish.");
            return vec![ReviewEffect::Redraw];
        }
        let cleaned = text.replace("\r\n", "\n").replace('\r', "\n");
        self.editor.extend(
            cleaned
                .chars()
                .filter(|c| *c == '\n' || *c == '\t' || !c.is_control()),
        );
        vec![ReviewEffect::Redraw]
    }

    // ----- keys ----------------------------------------------------------

    fn key(&mut self, key: ReviewKey) -> Vec<ReviewEffect> {
        match self.focus.clone() {
            ReviewFocus::List => self.list_key(key),
            ReviewFocus::DiffOnly => self.diff_only_key(key),
            ReviewFocus::ConfirmDiscard(path) => self.confirm_discard_key(&path, key),
            ReviewFocus::Commit => self.editor_key(key),
            ReviewFocus::ConfirmReplaceDraft => self.confirm_replace_key(key),
            ReviewFocus::ConfirmPr => self.confirm_pr_key(key),
        }
    }

    fn list_key(&mut self, key: ReviewKey) -> Vec<ReviewEffect> {
        let mut effects = Vec::new();
        match key {
            ReviewKey::Up => self.move_cursor_to(self.cursor_index.saturating_sub(1), &mut effects),
            ReviewKey::Down => self.move_cursor_to(self.cursor_index + 1, &mut effects),
            ReviewKey::Top => self.move_cursor_to(0, &mut effects),
            ReviewKey::Bottom => self.move_cursor_to(usize::MAX, &mut effects),
            ReviewKey::PageUp => self.scroll_diff(false, DIFF_PAGE, &mut effects),
            ReviewKey::PageDown => self.scroll_diff(true, DIFF_PAGE, &mut effects),
            ReviewKey::ShowDiff | ReviewKey::Enter => {
                if self.cursor_file().is_some() {
                    self.focus = ReviewFocus::DiffOnly;
                    self.ensure_diff(&mut effects);
                    effects.push(ReviewEffect::Redraw);
                }
            }
            ReviewKey::Scope => {
                self.scope = match self.scope {
                    Scope::Uncommitted => Scope::Branch,
                    Scope::Branch => Scope::Uncommitted,
                };
                self.diff_scroll = 0;
                self.sync_cursor();
                self.last_list_at = self.now;
                effects.push(ReviewEffect::List(self.scope));
                self.ensure_diff(&mut effects);
                effects.push(ReviewEffect::Redraw);
            }
            ReviewKey::StageToggle => return self.stage_toggle(),
            ReviewKey::StageAll => return self.stage_all(),
            ReviewKey::UnstageAll => return self.unstage_all(),
            ReviewKey::Discard => return self.ask_discard(),
            ReviewKey::Commit => return self.open_editor(),
            ReviewKey::Push => return self.push(),
            ReviewKey::PullRequest => return self.ask_pr(),
            ReviewKey::OpenPr => return self.open_pr(),
            ReviewKey::Refresh => {
                self.last_list_at = self.now;
                self.diff_requested = None;
                effects.push(ReviewEffect::List(self.scope));
                effects.push(ReviewEffect::RefreshPr);
                effects.push(ReviewEffect::Redraw);
            }
            ReviewKey::Esc | ReviewKey::CtrlC => effects.push(ReviewEffect::Close),
            ReviewKey::Submit
            | ReviewKey::Draft
            | ReviewKey::Newline
            | ReviewKey::Char(_)
            | ReviewKey::Backspace
            | ReviewKey::Other => {}
        }
        effects
    }

    fn scroll_diff(&mut self, down: bool, lines: usize, effects: &mut Vec<ReviewEffect>) {
        let before = self.diff_scroll;
        self.diff_scroll = if down {
            self.diff_scroll.saturating_add(lines)
        } else {
            self.diff_scroll.saturating_sub(lines)
        };
        self.clamp_scroll();
        if self.diff_scroll != before {
            effects.push(ReviewEffect::Redraw);
        }
    }

    fn diff_only_key(&mut self, key: ReviewKey) -> Vec<ReviewEffect> {
        let mut effects = Vec::new();
        match key {
            ReviewKey::Up => self.scroll_diff(false, 1, &mut effects),
            ReviewKey::Down => self.scroll_diff(true, 1, &mut effects),
            ReviewKey::PageUp => self.scroll_diff(false, DIFF_PAGE, &mut effects),
            ReviewKey::PageDown => self.scroll_diff(true, DIFF_PAGE, &mut effects),
            ReviewKey::Top => self.scroll_diff(false, usize::MAX, &mut effects),
            ReviewKey::Bottom => self.scroll_diff(true, usize::MAX, &mut effects),
            ReviewKey::Esc | ReviewKey::CtrlC => {
                self.focus = ReviewFocus::List;
                effects.push(ReviewEffect::Redraw);
            }
            _ => {}
        }
        effects
    }

    /// The row a stage, unstage or discard key acts on, or the notice why not.
    fn target_row(&mut self) -> Option<ChangedFile> {
        let Some(file) = self.cursor_file().cloned() else {
            if self.listing().is_none() {
                self.notify(LOADING);
            }
            return None;
        };
        if in_worktrees(&file.path) {
            self.notify(refusal_text(&ShipRefusal::WorktreesFolder));
            return None;
        }
        if file.status == FileStatus::Conflicted {
            let count = self.conflicted();
            self.notify(refusal_text(&ShipRefusal::Conflicted(count)));
            return None;
        }
        Some(file)
    }

    fn conflicted(&self) -> u32 {
        let count = self
            .files()
            .iter()
            .filter(|file| file.status == FileStatus::Conflicted)
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    fn stage_toggle(&mut self) -> Vec<ReviewEffect> {
        if matches!(self.mutation_gate(), Gate::Refused) {
            return vec![ReviewEffect::Redraw];
        }
        let Some(file) = self.target_row() else {
            return vec![ReviewEffect::Redraw];
        };
        let mut paths = vec![file.path.clone()];
        if file.status == FileStatus::Renamed {
            paths.extend(file.orig_path.clone());
        }
        if file.staged {
            self.run(GitAction::Unstage(paths))
        } else {
            self.run(GitAction::Stage(paths))
        }
    }

    fn stage_all(&mut self) -> Vec<ReviewEffect> {
        if matches!(self.mutation_gate(), Gate::Refused) {
            return vec![ReviewEffect::Redraw];
        }
        let conflicted = self.conflicted();
        if conflicted > 0 {
            return self.refuse(&ShipRefusal::Conflicted(conflicted));
        }
        if self.files().iter().all(|file| in_worktrees(&file.path)) {
            return Vec::new();
        }
        // An empty path list means every change.
        self.run(GitAction::Stage(Vec::new()))
    }

    fn unstage_all(&mut self) -> Vec<ReviewEffect> {
        if matches!(self.mutation_gate(), Gate::Refused) {
            return vec![ReviewEffect::Redraw];
        }
        if !self.files().iter().any(|file| file.staged) {
            return Vec::new();
        }
        self.run(GitAction::Unstage(Vec::new()))
    }

    fn ask_discard(&mut self) -> Vec<ReviewEffect> {
        if matches!(self.mutation_gate(), Gate::Refused) {
            return vec![ReviewEffect::Redraw];
        }
        let Some(file) = self.target_row() else {
            return vec![ReviewEffect::Redraw];
        };
        self.notice = None;
        self.focus = ReviewFocus::ConfirmDiscard(file.path);
        vec![ReviewEffect::Redraw]
    }

    fn confirm_discard_key(&mut self, path: &str, key: ReviewKey) -> Vec<ReviewEffect> {
        self.focus = ReviewFocus::List;
        if key == ReviewKey::Discard {
            let row = self.files().iter().find(|file| file.path == path).cloned();
            if let Some(row) = row {
                return self.run(GitAction::Discard(row));
            }
        }
        self.notify("Discard cancelled.");
        vec![ReviewEffect::Redraw]
    }

    fn open_editor(&mut self) -> Vec<ReviewEffect> {
        if matches!(self.mutation_gate(), Gate::Refused) {
            return vec![ReviewEffect::Redraw];
        }
        if let Some(listing) = self.listing()
            && let Err(refusal) = can_commit(listing, self.scope, "-")
        {
            return self.refuse(&refusal);
        }
        self.notice = None;
        self.focus = ReviewFocus::Commit;
        vec![ReviewEffect::Redraw]
    }

    fn push(&mut self) -> Vec<ReviewEffect> {
        if matches!(self.mutation_gate(), Gate::Refused) {
            return vec![ReviewEffect::Redraw];
        }
        let base = self.base_name();
        let Some(listing) = self.listing() else {
            self.notify(LOADING);
            return vec![ReviewEffect::Redraw];
        };
        match can_push(listing, self.scope, base.as_deref()) {
            Ok(()) => self.run(GitAction::Push),
            Err(refusal) => self.refuse(&refusal),
        }
    }

    /// The pull request's base and head, when the gates let one open. The
    /// count of commits ahead of the base is the runner's to check: the
    /// listing only knows the count ahead of the upstream.
    fn pr_plan(&self) -> Result<(String, String), ShipRefusal> {
        let listing = self.listing().ok_or(ShipRefusal::DefaultUnknown)?;
        let configured_base = self.base_name();
        can_push(listing, self.scope, configured_base.as_deref())?;
        let base = configured_base
            .or_else(|| listing.default_branch.clone())
            .ok_or(ShipRefusal::DefaultUnknown)?;
        let gh = self.gh.clone().unwrap_or(GhReady::Ready {
            repo: String::new(),
        });
        let open = self.pr.as_ref();
        can_open_pr(listing, self.scope, open, &gh, &base, 1)?;
        let branch = listing.head.head.clone().ok_or(ShipRefusal::Detached)?;
        Ok((base, branch))
    }

    fn ask_pr(&mut self) -> Vec<ReviewEffect> {
        if matches!(self.mutation_gate(), Gate::Refused) {
            return vec![ReviewEffect::Redraw];
        }
        if self.listing().is_none() {
            self.notify(LOADING);
            return vec![ReviewEffect::Redraw];
        }
        match self.pr_plan() {
            Ok(_) => {
                self.notice = None;
                self.focus = ReviewFocus::ConfirmPr;
                vec![ReviewEffect::Redraw]
            }
            Err(refusal) => self.refuse(&refusal),
        }
    }

    fn confirm_pr_key(&mut self, key: ReviewKey) -> Vec<ReviewEffect> {
        self.focus = ReviewFocus::List;
        if key == ReviewKey::Enter {
            if let Some(action) = self.busy {
                return self.refuse(&ShipRefusal::Busy(action));
            }
            return match self.pr_plan() {
                Ok((base, branch)) => self.run(GitAction::CreatePr { base, branch }),
                Err(refusal) => self.refuse(&refusal),
            };
        }
        self.notify("Pull request cancelled.");
        vec![ReviewEffect::Redraw]
    }

    fn open_pr(&mut self) -> Vec<ReviewEffect> {
        match self.pr.as_ref() {
            Some(status) => vec![ReviewEffect::OpenUrl(status.url.clone())],
            None => {
                let branch = self.branch_name().unwrap_or("this branch").to_owned();
                self.notify(format!("No pull request is known for {branch} yet."));
                vec![ReviewEffect::Redraw]
            }
        }
    }

    // ----- the commit editor ---------------------------------------------

    fn editor_key(&mut self, key: ReviewKey) -> Vec<ReviewEffect> {
        match key {
            ReviewKey::Esc | ReviewKey::CtrlC => {
                if self.drafting.take().is_some() {
                    self.notice = None;
                } else {
                    self.focus = ReviewFocus::List;
                }
                vec![ReviewEffect::Redraw]
            }
            ReviewKey::Char(c) => self.type_text(|editor| editor.push(c)),
            ReviewKey::Newline | ReviewKey::Enter => self.type_text(|editor| editor.push('\n')),
            ReviewKey::Backspace => self.type_text(|editor| {
                editor.pop();
            }),
            ReviewKey::Submit => self.submit(),
            ReviewKey::Draft => self.draft(),
            _ => Vec::new(),
        }
    }

    fn type_text(&mut self, edit: impl FnOnce(&mut String)) -> Vec<ReviewEffect> {
        if self.drafting.is_some() {
            self.notify("Wait for drafting to finish.");
        } else {
            edit(&mut self.editor);
        }
        vec![ReviewEffect::Redraw]
    }

    fn submit(&mut self) -> Vec<ReviewEffect> {
        if self.drafting.is_some() {
            self.notify("Wait for drafting to finish.");
            return vec![ReviewEffect::Redraw];
        }
        if let Some(action) = self.busy {
            return self.refuse(&ShipRefusal::Busy(action));
        }
        let message = trim_message(&self.editor);
        let Some(listing) = self.listing() else {
            self.notify(LOADING);
            return vec![ReviewEffect::Redraw];
        };
        match can_commit(listing, self.scope, &message) {
            Ok(stage_all) => self.run(GitAction::Commit { message, stage_all }),
            Err(refusal) => self.refuse(&refusal),
        }
    }

    fn draft(&mut self) -> Vec<ReviewEffect> {
        if self.draft_hidden {
            return Vec::new();
        }
        if let Some(action) = self.busy {
            return self.refuse(&ShipRefusal::Busy(action));
        }
        if self.drafting.is_some() {
            self.notify("Wait for drafting to finish.");
            return vec![ReviewEffect::Redraw];
        }
        if self.target.session.is_none() {
            self.notify(draft_error_text(&DraftError::NoSession));
            return vec![ReviewEffect::Redraw];
        }
        if !self.can_draft {
            self.notify(draft_error_text(&DraftError::NoOwnRoute));
            return vec![ReviewEffect::Redraw];
        }
        let typed = !self.editor.is_empty() && self.last_draft.as_deref() != Some(&self.editor);
        if typed {
            self.notify_sticky(
                "Replace your message with a draft? Press ctrl+d again; any other key cancels.",
            );
            self.focus = ReviewFocus::ConfirmReplaceDraft;
            return vec![ReviewEffect::Redraw];
        }
        self.start_draft()
    }

    fn start_draft(&mut self) -> Vec<ReviewEffect> {
        self.draft_seq += 1;
        let seq = self.draft_seq;
        self.drafting = Some(seq);
        let model = self.model_name().to_owned();
        self.notify_sticky(format!("Drafting a message with {model}\u{2026}"));
        vec![
            ReviewEffect::Draft {
                seq,
                avoid: self.editor.clone(),
            },
            ReviewEffect::Redraw,
        ]
    }

    fn model_name(&self) -> &str {
        self.model
            .as_deref()
            .filter(|model| !model.is_empty())
            .unwrap_or("the session's model")
    }

    fn confirm_replace_key(&mut self, key: ReviewKey) -> Vec<ReviewEffect> {
        self.focus = ReviewFocus::Commit;
        if key == ReviewKey::Draft {
            return self.start_draft();
        }
        self.notice = None;
        vec![ReviewEffect::Redraw]
    }

    // ----- the view ------------------------------------------------------

    /// Borrow what the drawing needs.
    pub fn view(&self) -> ReviewView<'_> {
        let listing = self.listing();
        let files = self.files();
        let (added, removed) = files.iter().fold((0_u64, 0_u64), |(added, removed), file| {
            (
                added + u64::from(file.added.unwrap_or(0)),
                removed + u64::from(file.removed.unwrap_or(0)),
            )
        });
        ReviewView {
            title: &self.target.title,
            folder: &self.target.folder,
            scope: self.scope,
            loaded: listing.is_some(),
            head: listing.map(|listing| &listing.head),
            files,
            total: listing.map_or(0, |listing| listing.total.max(listing.files.len())),
            added,
            removed,
            cursor: if files.is_empty() {
                None
            } else {
                Some(self.cursor_index.min(files.len() - 1))
            },
            base: listing.and_then(|listing| listing.base.as_deref()),
            base_ref: self
                .target
                .record
                .as_ref()
                .map(|record| record.base_ref.as_str()),
            focus: &self.focus,
            diff: self.diff_view(),
            editor: self.editor_view(),
            notice: self.notice_text(),
            pr: self.pr.as_ref().map(|status| pr_label(status, false)),
            review: self.pr.as_ref().and_then(|status| status.review),
            footer: self.footer(),
        }
    }

    fn diff_view(&self) -> DiffContent<'_> {
        let Some(file) = self.cursor_file() else {
            return DiffContent::Empty;
        };
        if file.is_dir {
            return DiffContent::Message(Cow::Borrowed(UNTRACKED_FOLDER));
        }
        match self.diff.as_ref() {
            Some(loaded) if loaded.path == file.path && loaded.scope == self.scope => match &loaded
                .body
            {
                DiffBody::Text(text) => DiffContent::Message(Cow::Borrowed(text)),
                DiffBody::Parsed { model, cut } => DiffContent::Diff {
                    path: &loaded.path,
                    model,
                    scroll: self.diff_scroll,
                    cut_note: (*cut > 0).then(|| {
                        format!(
                            "Diff cut at {} lines; {} more. Run git diff in a shell for the rest.",
                            grouped(DIFF_MAX_LINES),
                            grouped(*cut)
                        )
                    }),
                },
            },
            _ => DiffContent::Empty,
        }
    }

    fn editor_view(&self) -> Option<EditorView<'_>> {
        let editing = matches!(
            self.focus,
            ReviewFocus::Commit | ReviewFocus::ConfirmReplaceDraft
        );
        if !editing {
            return None;
        }
        let branch = self.branch_name().unwrap_or("this branch");
        let header = match self.listing() {
            None => "Commit your changes".to_owned(),
            Some(listing) => {
                let staged = listing.files.iter().filter(|file| file.staged).count();
                if staged > 0 {
                    format!("Commit {} on {branch}", counted(staged, "staged file"))
                } else {
                    format!(
                        "Nothing is staged: this commit will include all {} changed files on {branch}.",
                        listing.total.max(listing.files.len())
                    )
                }
            }
        };
        let mut warnings = Vec::new();
        if self.working {
            warnings.push(
                "The agent is still working in this folder; edits after this commit are not in it."
                    .to_owned(),
            );
        }
        if let Some(listing) = self.listing()
            && listing.hooks > 0
        {
            warnings.push(format!(
                "Hooks: {} active (they run as in your shell).",
                listing.hooks
            ));
        }
        Some(EditorView {
            header,
            warnings,
            text: &self.editor,
            drafting: self.drafting.is_some(),
        })
    }

    fn notice_text(&self) -> Option<Cow<'_, str>> {
        match &self.focus {
            ReviewFocus::ConfirmDiscard(path) => {
                let untracked = self
                    .files()
                    .iter()
                    .find(|file| &file.path == path)
                    .is_some_and(|file| file.status == FileStatus::Untracked);
                let ask = if untracked {
                    format!("Delete untracked {path}?")
                } else {
                    format!("Discard changes to {path}?")
                };
                return Some(Cow::Owned(format!(
                    "{ask} Press X again to discard; any other key cancels."
                )));
            }
            ReviewFocus::ConfirmPr => {
                if let Ok((base, branch)) = self.pr_plan() {
                    let mut text = format!(
                        "Open a draft pull request from {branch} into {base}? Enter opens, any other key cancels."
                    );
                    let dirty = self.files().len();
                    if dirty > 0 {
                        text.push_str(&format!(
                            " {} uncommitted and will not be in it.",
                            if dirty == 1 {
                                "1 file is".to_owned()
                            } else {
                                format!("{dirty} files are")
                            }
                        ));
                    }
                    return Some(Cow::Owned(text));
                }
            }
            _ => {}
        }
        if let Some(notice) = &self.notice {
            return Some(Cow::Borrowed(notice.text.as_str()));
        }
        let listing = self.listing()?;
        (listing.total > listing.files.len()).then(|| {
            Cow::Owned(format!(
                "Showing {} of {} changed files.",
                grouped(listing.files.len()),
                grouped(listing.total)
            ))
        })
    }

    fn footer(&self) -> Cow<'static, str> {
        match &self.focus {
            ReviewFocus::List => match self.scope {
                Scope::Uncommitted => Cow::Borrowed(
                    "space stage \u{b7} a stage all \u{b7} c commit \u{b7} p push \u{b7} P pull request \u{b7} X discard \u{b7} tab scope \u{b7} r refresh \u{b7} esc close",
                ),
                Scope::Branch => Cow::Borrowed(
                    "tab uncommitted \u{b7} o open pull request \u{b7} r refresh \u{b7} esc close",
                ),
            },
            ReviewFocus::DiffOnly => {
                Cow::Borrowed("\u{2191}\u{2193} scroll \u{b7} esc back to the list")
            }
            ReviewFocus::ConfirmDiscard(_) => {
                Cow::Borrowed("X discard \u{b7} any other key cancels")
            }
            ReviewFocus::ConfirmPr => Cow::Borrowed("enter open \u{b7} any other key cancels"),
            ReviewFocus::ConfirmReplaceDraft => {
                Cow::Borrowed("ctrl+d draft \u{b7} any other key cancels")
            }
            ReviewFocus::Commit => {
                let drafting_possible =
                    self.target.session.is_some() && self.can_draft && !self.draft_hidden;
                if drafting_possible {
                    Cow::Owned(format!(
                        "ctrl+s commit \u{b7} ctrl+d draft with {} \u{b7} esc back",
                        self.model_name()
                    ))
                } else {
                    Cow::Borrowed("ctrl+s commit \u{b7} esc back")
                }
            }
        }
    }
}

/// The commit message as git gets it: trailing whitespace off every line,
/// blank lines off both ends.
fn trim_message(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let start = lines.iter().position(|line| !line.is_empty());
    let end = lines.iter().rposition(|line| !line.is_empty());
    match (start, end) {
        (Some(start), Some(end)) => lines[start..=end].join("\n"),
        _ => String::new(),
    }
}

/// The text up to `limit` lines, and how many lines were left out.
fn cut_lines(text: &str, limit: usize) -> (&str, usize) {
    let mut offset = 0;
    for (count, piece) in text.split_inclusive('\n').enumerate() {
        if count == limit {
            return (&text[..offset], text[offset..].lines().count());
        }
        offset += piece.len();
    }
    (text, 0)
}

/// A number with a comma between thousands.
fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The notice for a drafting failure.
fn draft_error_text(error: &DraftError) -> String {
    match error {
        DraftError::NoSession => {
            "This folder has no live session, so there is no model to draft with.".to_owned()
        }
        DraftError::NoOwnRoute => "This session has no model of its own to draft with.".to_owned(),
        DraftError::SessionGone => {
            "This session no longer exists on the gateway, so there is no model to draft with."
                .to_owned()
        }
        DraftError::Busy => {
            "Hermes is busy drafting other messages; try again in a moment.".to_owned()
        }
        DraftError::Unsupported => "This Hermes does not offer message drafting.".to_owned(),
        DraftError::Failed(reason) => format!("Drafting failed: {reason}"),
        DraftError::NoReply => "No draft after 90 s.".to_owned(),
        DraftError::Draining => {
            "Hermes is shutting down; try drafting again after it restarts.".to_owned()
        }
    }
}

/// What the diff panel shows.
#[derive(Clone, Debug)]
pub enum DiffContent<'a> {
    /// Nothing to show: no row, or the diff has not arrived.
    Empty,
    /// A line of text in place of a diff.
    Message(Cow<'a, str>),
    /// A parsed diff.
    Diff {
        /// The file the diff belongs to.
        path: &'a str,
        /// The parsed diff.
        model: &'a DiffModel,
        /// How many lines the panel is scrolled down.
        scroll: usize,
        /// The notice for a diff cut at the line limit.
        cut_note: Option<String>,
    },
}

/// The commit editor.
#[derive(Clone, Debug)]
pub struct EditorView<'a> {
    /// The line above the text.
    pub header: String,
    /// Warning lines under the header.
    pub warnings: Vec<String>,
    /// The message text.
    pub text: &'a str,
    /// A draft is on its way, so the text does not take input.
    pub drafting: bool,
}

/// What the drawing needs, borrowed from the state.
#[derive(Clone, Debug)]
pub struct ReviewView<'a> {
    /// The header title.
    pub title: &'a str,
    /// The folder under review.
    pub folder: &'a Path,
    /// Which changes are shown.
    pub scope: Scope,
    /// A listing for this scope has arrived.
    pub loaded: bool,
    /// The branch head, once a listing arrived.
    pub head: Option<&'a BranchHead>,
    /// The file rows.
    pub files: &'a [ChangedFile],
    /// The full count of changed files, before the cap.
    pub total: usize,
    /// Added lines over the rows with known counts.
    pub added: u64,
    /// Removed lines over the rows with known counts.
    pub removed: u64,
    /// The row the cursor is on.
    pub cursor: Option<usize>,
    /// The Branch scope base.
    pub base: Option<&'a str>,
    /// The ref the worktree was branched from, when there is a record.
    pub base_ref: Option<&'a str>,
    /// Where the keys go.
    pub focus: &'a ReviewFocus,
    /// The diff panel.
    pub diff: DiffContent<'a>,
    /// The commit editor, while it is open.
    pub editor: Option<EditorView<'a>>,
    /// The line above the footer.
    pub notice: Option<Cow<'a, str>>,
    /// The pull-request label, when one is known.
    pub pr: Option<PrLabel>,
    /// The pull request's review decision, when known.
    pub review: Option<ReviewDecision>,
    /// The key hints for the focus.
    pub footer: Cow<'static, str>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::pr::{Checks, PrState};

    fn target(session: bool) -> ReviewTarget {
        ReviewTarget {
            folder: PathBuf::from("repo"),
            main_root: PathBuf::from("repo"),
            session: session.then(|| RowKey {
                profile: "p".to_owned(),
                session_id: "s".to_owned(),
            }),
            record: None,
            title: "repo".to_owned(),
        }
    }

    fn file(path: &str) -> ChangedFile {
        ChangedFile {
            path: path.to_owned(),
            orig_path: None,
            status: FileStatus::Modified,
            staged: false,
            unstaged: true,
            added: Some(1),
            removed: Some(0),
            is_dir: false,
        }
    }

    fn listing(files: Vec<ChangedFile>) -> Listing {
        Listing {
            head: BranchHead {
                oid: Some("abcdef0123".to_owned()),
                head: Some("feature".to_owned()),
                upstream: None,
                ahead: 0,
                behind: 0,
            },
            total: files.len(),
            files,
            base: None,
            default_branch: Some("main".to_owned()),
            has_origin: true,
            hooks: 0,
        }
    }

    fn loaded(session: bool, files: Vec<ChangedFile>) -> ReviewState {
        let (mut state, effects) = ReviewState::open(target(session), 100.0);
        assert_eq!(effects, vec![ReviewEffect::List(Scope::Uncommitted)]);
        let _ = state.update(ReviewInput::Listed {
            scope: Scope::Uncommitted,
            result: Ok(listing(files)),
        });
        state
    }

    fn press(state: &mut ReviewState, key: ReviewKey) -> Vec<ReviewEffect> {
        state.update(ReviewInput::Key(key))
    }

    fn notice(state: &ReviewState) -> String {
        state.notice_text().map(Cow::into_owned).unwrap_or_default()
    }

    fn done(state: &mut ReviewState, action: GitAction) {
        let _ = state.update(ReviewInput::ActionDone {
            action,
            result: Ok("done".to_owned()),
        });
    }

    #[test]
    fn grouped_puts_commas_between_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(2_000), "2,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }

    #[test]
    fn cut_lines_keeps_the_limit_and_counts_the_rest() {
        assert_eq!(cut_lines("a\nb\n", 5), ("a\nb\n", 0));
        assert_eq!(cut_lines("a\nb\n", 2), ("a\nb\n", 0));
        assert_eq!(cut_lines("a\nb\nc\nd\n", 2), ("a\nb\n", 2));
        assert_eq!(cut_lines("a\nb\nc", 2), ("a\nb\n", 1));
    }

    #[test]
    fn trim_message_follows_the_commit_rule() {
        assert_eq!(
            trim_message("\n\nfeat: x  \n\n\nbody  \n\n"),
            "feat: x\n\n\nbody"
        );
        assert_eq!(trim_message("  \n "), "");
    }

    #[test]
    fn accessors_read_the_current_scope() {
        let mut state = loaded(false, vec![file("a"), file("b")]);
        assert_eq!(state.target().title, "repo");
        assert_eq!(state.scope(), Scope::Uncommitted);
        assert_eq!(state.listing().map(|l| l.files.len()), Some(2));
        assert_eq!(state.file("b").map(|f| f.path.as_str()), Some("b"));
        assert!(state.file("zz").is_none());
        let _ = press(&mut state, ReviewKey::Scope);
        assert_eq!(state.scope(), Scope::Branch);
        assert!(state.listing().is_none());
        assert!(state.file("a").is_none());
    }

    #[test]
    fn cursor_moves_and_asks_for_the_diff() {
        let mut state = loaded(false, vec![file("a"), file("b"), file("c")]);
        let effects = press(&mut state, ReviewKey::Down);
        assert!(effects.contains(&ReviewEffect::LoadDiff {
            path: "b".to_owned(),
            scope: Scope::Uncommitted,
        }));
        let _ = press(&mut state, ReviewKey::Bottom);
        assert_eq!(state.view().cursor, Some(2));
        assert!(
            press(&mut state, ReviewKey::Down).is_empty(),
            "no move, no frame"
        );
        let _ = press(&mut state, ReviewKey::Top);
        assert_eq!(state.view().cursor, Some(0));
        let _ = press(&mut state, ReviewKey::Up);
        assert_eq!(state.view().cursor, Some(0));
    }

    #[test]
    fn a_stale_diff_reply_is_ignored() {
        let mut state = loaded(false, vec![file("a"), file("b")]);
        let effects = state.update(ReviewInput::DiffLoaded {
            path: "b".to_owned(),
            scope: Scope::Uncommitted,
            result: Ok("@@ -1 +1 @@\n-x\n+y\n".to_owned()),
        });
        assert!(effects.is_empty());
        let effects = state.update(ReviewInput::DiffLoaded {
            path: "a".to_owned(),
            scope: Scope::Uncommitted,
            result: Err("File too large to show here (9 MiB). Run git diff in a shell.".to_owned()),
        });
        assert_eq!(effects, vec![ReviewEffect::Redraw]);
        assert!(matches!(state.view().diff, DiffContent::Message(_)));
    }

    #[test]
    fn diff_pages_scroll_within_the_diff() {
        let mut state = loaded(false, vec![file("a")]);
        let text = format!("@@ -1,30 +1,30 @@\n{}", " line\n".repeat(30));
        let _ = state.update(ReviewInput::DiffLoaded {
            path: "a".to_owned(),
            scope: Scope::Uncommitted,
            result: Ok(text),
        });
        let _ = press(&mut state, ReviewKey::PageDown);
        assert!(matches!(
            state.view().diff,
            DiffContent::Diff { scroll: 10, .. }
        ));
        let _ = press(&mut state, ReviewKey::PageUp);
        assert!(matches!(
            state.view().diff,
            DiffContent::Diff { scroll: 0, .. }
        ));
        let _ = press(&mut state, ReviewKey::ShowDiff);
        assert_eq!(*state.view().focus, ReviewFocus::DiffOnly);
        let _ = press(&mut state, ReviewKey::Bottom);
        assert!(matches!(
            state.view().diff,
            DiffContent::Diff { scroll: 29, .. }
        ));
        let _ = press(&mut state, ReviewKey::Top);
        let _ = press(&mut state, ReviewKey::Down);
        assert!(matches!(
            state.view().diff,
            DiffContent::Diff { scroll: 1, .. }
        ));
        let _ = press(&mut state, ReviewKey::Esc);
        assert_eq!(*state.view().focus, ReviewFocus::List);
    }

    #[test]
    fn untracked_folder_has_no_diff() {
        let mut folder = file("build/");
        folder.status = FileStatus::Untracked;
        folder.is_dir = true;
        let mut state = loaded(false, vec![folder]);
        assert!(matches!(state.view().diff, DiffContent::Message(_)));
        let effects = press(&mut state, ReviewKey::ShowDiff);
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, ReviewEffect::LoadDiff { .. }))
        );
    }

    #[test]
    fn stage_toggle_carries_both_paths_of_a_rename() {
        let mut renamed = file("new.rs");
        renamed.status = FileStatus::Renamed;
        renamed.orig_path = Some("old.rs".to_owned());
        let mut state = loaded(false, vec![renamed]);
        let effects = press(&mut state, ReviewKey::StageToggle);
        assert!(effects.contains(&ReviewEffect::Run(GitAction::Stage(vec![
            "new.rs".to_owned(),
            "old.rs".to_owned()
        ]))));
    }

    #[test]
    fn worktrees_rows_are_never_staged_or_discarded() {
        let mut state = loaded(false, vec![file(".worktrees/")]);
        for key in [ReviewKey::StageToggle, ReviewKey::Discard] {
            let effects = press(&mut state, key);
            assert!(!effects.iter().any(|e| matches!(e, ReviewEffect::Run(_))));
            assert_eq!(
                notice(&state),
                "Talaria does not stage or discard .worktrees/: it holds other checkouts."
            );
        }
        let effects = press(&mut state, ReviewKey::StageAll);
        assert!(effects.is_empty(), "nothing but .worktrees/ to stage");
    }

    #[test]
    fn stage_all_and_unstage_all_use_an_empty_path_list() {
        let mut staged = file("s");
        staged.staged = true;
        let mut state = loaded(false, vec![file("a"), staged]);
        let effects = press(&mut state, ReviewKey::StageAll);
        assert!(effects.contains(&ReviewEffect::Run(GitAction::Stage(Vec::new()))));
        done(&mut state, GitAction::Stage(Vec::new()));
        let effects = press(&mut state, ReviewKey::UnstageAll);
        assert!(effects.contains(&ReviewEffect::Run(GitAction::Unstage(Vec::new()))));
        done(&mut state, GitAction::Unstage(Vec::new()));
        let mut conflicted = file("c");
        conflicted.status = FileStatus::Conflicted;
        let mut state = loaded(false, vec![conflicted]);
        let effects = press(&mut state, ReviewKey::StageAll);
        assert!(!effects.iter().any(|e| matches!(e, ReviewEffect::Run(_))));
        assert_eq!(notice(&state), "Resolve the 1 conflicted file first.");
        assert!(
            press(&mut state, ReviewKey::UnstageAll).is_empty(),
            "nothing staged"
        );
    }

    #[test]
    fn a_busy_slot_answers_with_the_action_name() {
        let mut state = loaded(false, vec![file("a")]);
        let _ = press(&mut state, ReviewKey::Push);
        let _ = press(&mut state, ReviewKey::StageAll);
        assert_eq!(notice(&state), "Wait for push to finish.");
        let _ = press(&mut state, ReviewKey::Commit);
        assert_eq!(notice(&state), "Wait for push to finish.");
        assert_eq!(*state.view().focus, ReviewFocus::List);
    }

    #[test]
    fn push_refusals_show_their_text() {
        let mut state = loaded(false, vec![file("a")]);
        let mut detached = listing(vec![file("a")]);
        detached.head.head = None;
        let _ = state.update(ReviewInput::Listed {
            scope: Scope::Uncommitted,
            result: Ok(detached),
        });
        let effects = press(&mut state, ReviewKey::Push);
        assert!(!effects.iter().any(|e| matches!(e, ReviewEffect::Run(_))));
        assert_eq!(
            notice(&state),
            "HEAD is detached: check out a branch first."
        );
    }

    #[test]
    fn push_and_pull_request_wait_for_a_listing() {
        let (mut state, _) = ReviewState::open(target(false), 1.0);
        for key in [ReviewKey::Push, ReviewKey::PullRequest] {
            let effects = press(&mut state, key);
            assert!(!effects.iter().any(|e| matches!(e, ReviewEffect::Run(_))));
            assert_eq!(notice(&state), LOADING);
        }
        let _ = press(&mut state, ReviewKey::StageToggle);
        assert_eq!(notice(&state), LOADING);
    }

    #[test]
    fn pull_request_asks_then_creates_and_cancels() {
        let mut state = loaded(false, vec![file("a"), file("b")]);
        let _ = state.update(ReviewInput::GhReadiness(GhReady::Ready {
            repo: "o/r".to_owned(),
        }));
        let _ = press(&mut state, ReviewKey::PullRequest);
        assert_eq!(*state.view().focus, ReviewFocus::ConfirmPr);
        assert_eq!(
            notice(&state),
            "Open a draft pull request from feature into main? Enter opens, any other key cancels. 2 files are uncommitted and will not be in it."
        );
        let _ = press(&mut state, ReviewKey::Down);
        assert_eq!(notice(&state), "Pull request cancelled.");
        let _ = press(&mut state, ReviewKey::PullRequest);
        let effects = press(&mut state, ReviewKey::Enter);
        assert!(effects.contains(&ReviewEffect::Run(GitAction::CreatePr {
            base: "main".to_owned(),
            branch: "feature".to_owned(),
        })));
        let effects = state.update(ReviewInput::ActionDone {
            action: GitAction::CreatePr {
                base: "main".to_owned(),
                branch: "feature".to_owned(),
            },
            result: Ok("Draft pull request opened: https://x/pull/1".to_owned()),
        });
        assert!(effects.contains(&ReviewEffect::RefreshPr));
        assert!(effects.contains(&ReviewEffect::List(Scope::Uncommitted)));
    }

    #[test]
    fn pull_request_refusals_come_in_order() {
        let mut state = loaded(false, vec![file("a")]);
        let _ = state.update(ReviewInput::GhReadiness(GhReady::NotFound));
        let _ = press(&mut state, ReviewKey::PullRequest);
        assert!(notice(&state).starts_with("GitHub CLI (gh) not found"));
        let _ = state.update(ReviewInput::PrStatus(Some(PrStatus {
            number: 4,
            state: PrState::Open,
            draft: true,
            checks: Checks::None,
            review: Some(ReviewDecision::Approved),
            url: "https://x/pull/4".to_owned(),
            branch: "feature".to_owned(),
        })));
        let _ = press(&mut state, ReviewKey::PullRequest);
        assert_eq!(
            notice(&state),
            "Pull request #4 already exists: https://x/pull/4"
        );
        let effects = press(&mut state, ReviewKey::OpenPr);
        assert_eq!(
            effects,
            vec![ReviewEffect::OpenUrl("https://x/pull/4".to_owned())]
        );
        // No default branch and no record: the base is unknown.
        let mut unknown = listing(vec![file("a")]);
        unknown.default_branch = None;
        let _ = state.update(ReviewInput::PrStatus(None));
        let _ = state.update(ReviewInput::Listed {
            scope: Scope::Uncommitted,
            result: Ok(unknown),
        });
        let _ = press(&mut state, ReviewKey::PullRequest);
        assert_eq!(
            notice(&state),
            "Talaria cannot tell origin's default branch, so it will not push."
        );
    }

    #[test]
    fn open_pr_without_a_known_pull_request_says_so() {
        let mut state = loaded(false, vec![file("a")]);
        let effects = press(&mut state, ReviewKey::OpenPr);
        assert_eq!(effects, vec![ReviewEffect::Redraw]);
        assert_eq!(notice(&state), "No pull request is known for feature yet.");
    }

    #[test]
    fn a_record_supplies_the_pull_request_base() {
        let mut with_record = target(false);
        with_record.record = Some(RecordView {
            branch: "feature".to_owned(),
            base_ref: "origin/develop".to_owned(),
            base_commit: "abc".to_owned(),
        });
        let (mut state, _) = ReviewState::open(with_record, 1.0);
        let _ = state.update(ReviewInput::Listed {
            scope: Scope::Uncommitted,
            result: Ok(listing(vec![file("a")])),
        });
        let _ = press(&mut state, ReviewKey::PullRequest);
        assert!(
            notice(&state).contains("into develop?"),
            "{}",
            notice(&state)
        );
    }

    #[test]
    fn refresh_and_scope_reload() {
        let mut state = loaded(false, vec![file("a")]);
        let effects = press(&mut state, ReviewKey::Refresh);
        assert!(effects.contains(&ReviewEffect::List(Scope::Uncommitted)));
        assert!(effects.contains(&ReviewEffect::RefreshPr));
        let effects = press(&mut state, ReviewKey::Scope);
        assert!(effects.contains(&ReviewEffect::List(Scope::Branch)));
        let view = state.view();
        assert_eq!(view.scope, Scope::Branch);
        assert!(!view.loaded, "the Branch listing has not arrived");
    }

    #[test]
    fn polling_and_notice_expiry_follow_the_tick() {
        let mut state = loaded(false, vec![file("a")]);
        assert!(state.update(ReviewInput::Tick { now: 104.0 }).is_empty());
        let effects = state.update(ReviewInput::Tick { now: 105.0 });
        assert_eq!(effects, vec![ReviewEffect::List(Scope::Uncommitted)]);
        assert!(state.update(ReviewInput::Tick { now: 106.0 }).is_empty());
        // Busy or an open editor stops the poll.
        let _ = press(&mut state, ReviewKey::Commit);
        assert!(state.update(ReviewInput::Tick { now: 200.0 }).is_empty());
        let _ = press(&mut state, ReviewKey::Esc);
        let _ = press(&mut state, ReviewKey::Refresh);
        let _ = press(&mut state, ReviewKey::OpenPr);
        assert!(!notice(&state).is_empty());
        let effects = state.update(ReviewInput::Tick { now: 300.0 });
        assert!(effects.contains(&ReviewEffect::Redraw));
        assert_eq!(notice(&state), "");
    }

    #[test]
    fn a_failed_listing_shows_its_text() {
        let (mut state, _) = ReviewState::open(target(false), 1.0);
        let effects = state.update(ReviewInput::Listed {
            scope: Scope::Uncommitted,
            result: Err("git failed".to_owned()),
        });
        assert_eq!(effects, vec![ReviewEffect::Redraw]);
        assert_eq!(notice(&state), "git failed");
        let effects = state.update(ReviewInput::Listed {
            scope: Scope::Branch,
            result: Err("other scope".to_owned()),
        });
        assert!(effects.is_empty());
    }

    #[test]
    fn small_inputs_redraw_only_on_change() {
        let mut state = loaded(true, vec![file("a")]);
        assert_eq!(
            state.update(ReviewInput::SessionWorking(true)),
            vec![ReviewEffect::Redraw]
        );
        assert!(state.update(ReviewInput::SessionWorking(true)).is_empty());
        let info = ReviewInput::SessionInfo {
            model: Some("m".to_owned()),
            can_draft: true,
        };
        assert_eq!(state.update(info.clone()), vec![ReviewEffect::Redraw]);
        assert!(state.update(info).is_empty());
        assert!(state.update(ReviewInput::PrStatus(None)).is_empty());
    }

    #[test]
    fn the_editor_takes_text_paste_and_backspace() {
        let mut state = loaded(false, vec![file("a")]);
        let _ = press(&mut state, ReviewKey::Commit);
        let _ = press(&mut state, ReviewKey::Char('h'));
        let _ = press(&mut state, ReviewKey::Char('i'));
        let _ = press(&mut state, ReviewKey::Newline);
        let _ = press(&mut state, ReviewKey::Enter);
        let _ = press(&mut state, ReviewKey::Backspace);
        let _ = state.update(ReviewInput::Paste("a\r\nb\x1b[0m".to_owned()));
        assert_eq!(state.view().editor.expect("editor").text, "hi\na\nb[0m");
        assert!(press(&mut state, ReviewKey::Other).is_empty());
        // A paste with the editor closed goes nowhere.
        let _ = press(&mut state, ReviewKey::Esc);
        assert!(state.update(ReviewInput::Paste("x".to_owned())).is_empty());
        // Esc keeps the text.
        let _ = press(&mut state, ReviewKey::Commit);
        assert_eq!(state.view().editor.expect("editor").text, "hi\na\nb[0m");
    }

    #[test]
    fn commit_submit_trims_and_checks_the_message() {
        let mut state = loaded(false, vec![file("a")]);
        let _ = press(&mut state, ReviewKey::Commit);
        let _ = press(&mut state, ReviewKey::Submit);
        assert_eq!(notice(&state), "Write a commit message first.");
        for c in "x  \n\n".chars() {
            let key = if c == '\n' {
                ReviewKey::Newline
            } else {
                ReviewKey::Char(c)
            };
            let _ = press(&mut state, key);
        }
        let effects = press(&mut state, ReviewKey::Submit);
        assert!(effects.contains(&ReviewEffect::Run(GitAction::Commit {
            message: "x".to_owned(),
            stage_all: true,
        })));
        // A failure keeps the editor open and its text.
        let _ = state.update(ReviewInput::ActionDone {
            action: GitAction::Commit {
                message: "x".to_owned(),
                stage_all: true,
            },
            result: Err("Commit failed: no".to_owned()),
        });
        assert_eq!(*state.view().focus, ReviewFocus::Commit);
        // Success closes it and forgets the text.
        let _ = press(&mut state, ReviewKey::Submit);
        done(
            &mut state,
            GitAction::Commit {
                message: "x".to_owned(),
                stage_all: true,
            },
        );
        assert_eq!(*state.view().focus, ReviewFocus::List);
        let _ = press(&mut state, ReviewKey::Commit);
        assert_eq!(state.view().editor.expect("editor").text, "");
    }

    #[test]
    fn the_editor_opens_only_when_a_commit_could_run() {
        let mut state = loaded(false, vec![]);
        let _ = press(&mut state, ReviewKey::Commit);
        assert_eq!(notice(&state), "Nothing to commit.");
        assert_eq!(*state.view().focus, ReviewFocus::List);
    }

    #[test]
    fn editor_header_hooks_and_footer_without_a_session() {
        let mut rows = listing(vec![file("a")]);
        rows.hooks = 2;
        let (mut state, _) = ReviewState::open(target(false), 1.0);
        let _ = state.update(ReviewInput::Listed {
            scope: Scope::Uncommitted,
            result: Ok(rows),
        });
        let _ = press(&mut state, ReviewKey::Commit);
        let view = state.view();
        let editor = view.editor.as_ref().expect("editor");
        assert_eq!(
            editor.warnings,
            vec!["Hooks: 2 active (they run as in your shell).".to_owned()]
        );
        assert_eq!(view.footer, "ctrl+s commit \u{b7} esc back");
    }

    fn draftable() -> ReviewState {
        let mut state = loaded(true, vec![file("a")]);
        let _ = state.update(ReviewInput::SessionInfo {
            model: Some("m1".to_owned()),
            can_draft: true,
        });
        let _ = press(&mut state, ReviewKey::Commit);
        state
    }

    #[test]
    fn drafting_blocks_typing_and_submit_until_it_ends() {
        let mut state = draftable();
        let effects = press(&mut state, ReviewKey::Draft);
        assert!(effects.contains(&ReviewEffect::Draft {
            seq: 1,
            avoid: String::new(),
        }));
        assert_eq!(notice(&state), "Drafting a message with m1\u{2026}");
        for key in [ReviewKey::Char('x'), ReviewKey::Submit, ReviewKey::Draft] {
            let _ = press(&mut state, key);
            assert_eq!(notice(&state), "Wait for drafting to finish.");
        }
        let _ = state.update(ReviewInput::Paste("x".to_owned()));
        assert_eq!(state.view().editor.expect("editor").text, "");
        let _ = state.update(ReviewInput::Drafted {
            seq: 1,
            result: Ok("feat: draft".to_owned()),
        });
        assert_eq!(state.view().editor.expect("editor").text, "feat: draft");
    }

    #[test]
    fn draft_errors_have_their_own_texts() {
        let cases = [
            (
                DraftError::NoSession,
                "This folder has no live session, so there is no model to draft with.",
            ),
            (
                DraftError::NoOwnRoute,
                "This session has no model of its own to draft with.",
            ),
            (
                DraftError::SessionGone,
                "This session no longer exists on the gateway, so there is no model to draft with.",
            ),
            (
                DraftError::Busy,
                "Hermes is busy drafting other messages; try again in a moment.",
            ),
            (
                DraftError::Failed("boom".to_owned()),
                "Drafting failed: boom",
            ),
            (DraftError::NoReply, "No draft after 90 s."),
            (
                DraftError::Draining,
                "Hermes is shutting down; try drafting again after it restarts.",
            ),
        ];
        for (error, text) in cases {
            let mut state = draftable();
            let _ = press(&mut state, ReviewKey::Draft);
            let _ = state.update(ReviewInput::Drafted {
                seq: 1,
                result: Err(error),
            });
            assert_eq!(notice(&state), text);
            assert!(state.view().editor.is_some());
        }
    }

    #[test]
    fn unsupported_drafting_hides_the_key() {
        let mut state = draftable();
        let _ = press(&mut state, ReviewKey::Draft);
        let _ = state.update(ReviewInput::Drafted {
            seq: 1,
            result: Err(DraftError::Unsupported),
        });
        assert_eq!(
            notice(&state),
            "This Hermes does not offer message drafting."
        );
        assert_eq!(state.view().footer, "ctrl+s commit \u{b7} esc back");
        assert!(press(&mut state, ReviewKey::Draft).is_empty());
    }

    #[test]
    fn drafting_without_a_route_sends_nothing() {
        let mut state = loaded(false, vec![file("a")]);
        let _ = press(&mut state, ReviewKey::Commit);
        let _ = press(&mut state, ReviewKey::Draft);
        assert_eq!(
            notice(&state),
            "This folder has no live session, so there is no model to draft with."
        );
        let mut state = loaded(true, vec![file("a")]);
        let _ = state.update(ReviewInput::SessionInfo {
            model: None,
            can_draft: false,
        });
        let _ = press(&mut state, ReviewKey::Commit);
        let effects = press(&mut state, ReviewKey::Draft);
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, ReviewEffect::Draft { .. }))
        );
        assert_eq!(
            notice(&state),
            "This session has no model of its own to draft with."
        );
    }

    #[test]
    fn drafting_over_typed_text_cancels_on_another_key() {
        let mut state = draftable();
        let _ = press(&mut state, ReviewKey::Char('t'));
        let _ = press(&mut state, ReviewKey::Draft);
        assert_eq!(*state.view().focus, ReviewFocus::ConfirmReplaceDraft);
        assert!(state.view().editor.is_some());
        let _ = press(&mut state, ReviewKey::Char('z'));
        assert_eq!(*state.view().focus, ReviewFocus::Commit);
        assert_eq!(state.view().editor.expect("editor").text, "t");
        assert_eq!(notice(&state), "");
    }

    #[test]
    fn a_name_for_a_missing_model_falls_back() {
        let mut state = draftable();
        let _ = state.update(ReviewInput::SessionInfo {
            model: Some(String::new()),
            can_draft: true,
        });
        assert_eq!(
            state.view().footer,
            "ctrl+s commit \u{b7} ctrl+d draft with the session's model \u{b7} esc back"
        );
    }

    #[test]
    fn a_discard_of_a_vanished_row_cancels() {
        let mut state = loaded(false, vec![file("a")]);
        let _ = press(&mut state, ReviewKey::Discard);
        let _ = state.update(ReviewInput::Listed {
            scope: Scope::Uncommitted,
            result: Ok(listing(vec![file("z")])),
        });
        let effects = press(&mut state, ReviewKey::Discard);
        assert!(!effects.iter().any(|e| matches!(e, ReviewEffect::Run(_))));
        assert_eq!(notice(&state), "Discard cancelled.");
    }

    #[test]
    fn untracked_discard_asks_to_delete() {
        let mut row = file("n.txt");
        row.status = FileStatus::Untracked;
        let mut state = loaded(false, vec![row]);
        let _ = press(&mut state, ReviewKey::Discard);
        assert_eq!(
            notice(&state),
            "Delete untracked n.txt? Press X again to discard; any other key cancels."
        );
        let _ = press(&mut state, ReviewKey::CtrlC);
        assert_eq!(notice(&state), "Discard cancelled.");
        assert_eq!(*state.view().focus, ReviewFocus::List);
    }

    #[test]
    fn the_file_cap_cuts_and_every_scope_footer_reads() {
        let mut state = loaded(false, vec![file("a")]);
        let mut many = listing((0..2_001).map(|n| file(&format!("f{n:04}"))).collect());
        many.total = 2_001;
        let _ = state.update(ReviewInput::Listed {
            scope: Scope::Uncommitted,
            result: Ok(many),
        });
        assert_eq!(state.view().files.len(), REVIEW_FILE_CAP);
        assert_eq!(notice(&state), "Showing 2,000 of 2,001 changed files.");
        for key in [ReviewKey::Discard, ReviewKey::Commit, ReviewKey::Scope] {
            let _ = press(&mut state, key);
            assert!(!state.view().footer.is_empty());
            let _ = press(&mut state, ReviewKey::Esc);
        }
    }
}
