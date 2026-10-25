//! Attachment state and the service that talks to the gateway.
//!
//! The key argument is this crate's own action, not a terminal-crate key. The screen maps its
//! keys onto these names. This crate does not depend on a terminal library.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Instant;

use hermes_client::{CallError, Client};
use hermes_contract::schemas::CompletionItem;
use talaria_core::attachments::{
    ChipId, ChipStore, CollapseRule, CompletionRequest, DropCandidate, Expanded, ImageType,
    MentionState,
};
use talaria_core::session::SessionId;

use crate::clipboard::ClipboardImageReader;

/// What the screen already knows when it routes a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteContext {
    /// A turn is running.
    pub turn_running: bool,
    /// The slash palette is open. False until that screen exists.
    pub palette_open: bool,
}

/// A key this update may consume. The screen maps its own actions onto these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Enter.
    Submit,
    /// Esc while a turn runs and no list is open.
    Interrupt,
    /// Down or Ctrl+N while a list is open.
    CompletionNext,
    /// Up or Ctrl+P while a list is open.
    CompletionPrev,
    /// Tab while a list is open.
    CompletionAccept,
    /// Esc while a list is open.
    CompletionClose,
    /// Ctrl+V or Alt+V.
    AttachClipboardImage,
    /// Backspace.
    DeleteBackward,
    /// Delete.
    DeleteForward,
}

/// Whether the update ate the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyRoute {
    /// Effects to run. The composer does not also see the key.
    Consumed(Vec<AttachmentEffect>),
    /// The composer handles the key.
    PassToComposer,
}

/// How a live label is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelStyle {
    /// A live label.
    Accent,
    /// An image that is still uploading.
    Dimmed,
}

/// What the submit call came back as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubmitOutcome {
    /// Hermes admitted the prompt.
    Receipt,
    /// Hermes refused it. `reason` is the gateway reason.
    Refused { reason: String },
    /// The reply was lost.
    Unknown,
    /// The gateway stopped before admitting anything.
    VoiceStopped,
}

/// Work for the service. The update only builds these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttachmentEffect {
    /// `complete.path`.
    Complete {
        session: SessionId,
        request: CompletionRequest,
    },
    /// Check drop candidates off the input thread.
    ResolveDrop {
        session: SessionId,
        folder: PathBuf,
        candidates: Vec<DropCandidate>,
        original: String,
    },
    /// `image.attach_bytes`.
    AttachImage {
        session: SessionId,
        chip: ChipId,
        bytes: Vec<u8>,
        ext: &'static str,
    },
    /// `file.attach`.
    AttachFile {
        session: SessionId,
        path: PathBuf,
        remainder: String,
    },
    /// Read the clipboard. A quiet probe shows no notice.
    ReadClipboard {
        quiet: bool,
        fallback_text: Option<String>,
    },
    /// Replace a range of the draft.
    EditDraft { range: Range<usize>, text: String },
    /// Accent or dimmed ranges.
    Highlight(Vec<(Range<usize>, LabelStyle)>),
    /// One plain sentence.
    Notice(String),
}

/// A result the service delivers back to the update.
///
/// `Eq` is omitted because a completion row does not implement it.
#[derive(Clone, Debug, PartialEq)]
pub enum AttachmentEvent {
    /// A completion reply.
    Completions {
        session: SessionId,
        generation: u64,
        items: Vec<CompletionItem>,
    },
    /// A completion failure.
    CompletionFailed {
        session: SessionId,
        generation: u64,
        reason: String,
    },
    /// A dropped path is an image.
    DropImage {
        session: SessionId,
        bytes: Vec<u8>,
        ty: ImageType,
        remainder: String,
    },
    /// A dropped path is a file.
    DropFile {
        session: SessionId,
        path: PathBuf,
        remainder: String,
    },
    /// No candidate was a regular file.
    DropNotAFile {
        session: SessionId,
        original: String,
    },
    /// A candidate was refused before any call.
    DropRefused { session: SessionId, notice: String },
    /// An image was staged.
    ImageStaged {
        session: SessionId,
        chip: ChipId,
        path: String,
        mime: String,
    },
    /// A file was attached.
    FileAttached {
        session: SessionId,
        name: String,
        ref_text: String,
        uploaded: bool,
        remainder: String,
    },
    /// Staging failed. `chip` is set when a label must come back out.
    AttachFailed {
        session: SessionId,
        chip: Option<ChipId>,
        notice: String,
    },
    /// The clipboard held an image.
    ClipboardImage { bytes: Vec<u8>, ty: ImageType },
    /// The clipboard held no image.
    ClipboardEmpty {
        quiet: bool,
        notice: Option<String>,
        fallback_text: Option<String>,
    },
}

/// Chips, the mention list, and the last submit.
pub struct AttachmentsState {
    /// Labels and their payloads.
    pub chips: ChipStore,
    /// The `@` list.
    pub mention: MentionState,
    rule: CollapseRule,
    pending_drops: u64,
    clipboard_busy: bool,
    last_expanded: Option<Expanded>,
}

impl AttachmentsState {
    /// Empty chips and a closed list.
    #[must_use]
    pub fn new(rule: CollapseRule, paste_budget_bytes: usize) -> Self {
        Self {
            chips: ChipStore::new(paste_budget_bytes),
            mention: MentionState::new(),
            rule,
            pending_drops: 0,
            clipboard_busy: false,
            last_expanded: None,
        }
    }

    fn note(&self) {
        let _ = (
            &self.chips,
            &self.mention,
            self.rule.lines,
            self.rule.chars,
            self.pending_drops,
            self.clipboard_busy,
            &self.last_expanded,
        );
    }

    /// Route one key. Consumed, or left for the composer.
    pub fn on_key(
        &mut self,
        action: Option<Action>,
        draft: &str,
        caret: usize,
        ctx: RouteContext,
        now: Instant,
    ) -> KeyRoute {
        let _ = (action, draft, caret, ctx, now);
        self.note();
        unimplemented!()
    }

    /// Reconcile after an edit. Only a typed edit feeds the mention list.
    pub fn on_edit(
        &mut self,
        draft: &str,
        caret: usize,
        typed: bool,
        now: Instant,
    ) -> Vec<AttachmentEffect> {
        let _ = (draft, caret, typed, now);
        self.note();
        unimplemented!()
    }

    /// Route one paste.
    pub fn on_paste(
        &mut self,
        text: &str,
        session: &SessionId,
        folder: &Path,
        rule: &CollapseRule,
    ) -> Vec<AttachmentEffect> {
        let _ = (text, session, folder, rule);
        self.note();
        unimplemented!()
    }

    /// Debounce, searching hint, and clipboard hint.
    pub fn on_tick(&mut self, session: &SessionId, now: Instant) -> Vec<AttachmentEffect> {
        let _ = (session, now);
        self.note();
        unimplemented!()
    }

    /// Apply one service result when it still belongs to this draft.
    pub fn on_event(
        &mut self,
        event: AttachmentEvent,
        foreground: &SessionId,
        draft: &str,
        caret: usize,
    ) -> Vec<AttachmentEffect> {
        let _ = (event, foreground, draft, caret);
        self.note();
        unimplemented!()
    }

    /// Expand the draft. The error is the notice and the draft stays.
    pub fn on_submit(&mut self, draft: &str) -> Result<Expanded, String> {
        let _ = draft;
        self.note();
        unimplemented!()
    }

    /// Apply a submit outcome to the labels.
    pub fn on_submit_outcome(&mut self, outcome: SubmitOutcome) -> Vec<AttachmentEffect> {
        let _ = outcome;
        self.note();
        unimplemented!()
    }

    /// A new foreground session. Paste labels stay.
    pub fn on_session_change(&mut self, new: &SessionId) -> Vec<AttachmentEffect> {
        let _ = new;
        self.note();
        unimplemented!()
    }
}

/// Runs effects off the input thread and delivers [`AttachmentEvent`]s.
pub struct AttachmentService {
    client: Option<Client>,
    events: Box<dyn Fn(AttachmentEvent) + Send + Sync>,
    clipboard: ClipboardImageReader,
    home: PathBuf,
}

impl AttachmentService {
    /// `client` is `None` when a test needs no gateway. `home` is the folder `~/` means.
    #[must_use]
    pub fn new(
        client: Option<Client>,
        events: Box<dyn Fn(AttachmentEvent) + Send + Sync>,
        clipboard: ClipboardImageReader,
        home: PathBuf,
    ) -> Self {
        Self {
            client,
            events,
            clipboard,
            home,
        }
    }

    /// Start `effect`. The outcome arrives later as an event.
    pub fn execute(&self, effect: AttachmentEffect) {
        let _ = (
            self.client.is_some(),
            &self.events,
            &self.clipboard,
            &self.home,
            effect,
        );
        unimplemented!()
    }
}

/// Which call a refusal belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    /// `complete.path`.
    Completion,
    /// `file.attach`.
    FileAttach,
    /// `image.attach_bytes`.
    ImageAttach,
}

/// One plain sentence for a gateway refusal.
#[must_use]
pub fn refusal_notice(error: &CallError, feature: Feature) -> String {
    let _ = (error, feature);
    unimplemented!()
}
