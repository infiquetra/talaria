//! Attachment state and the service that talks to the gateway.
//!
//! The key argument is this crate's own action, not a terminal-crate key. The screen maps its
//! keys onto these names. This crate does not depend on a terminal library.

use std::io::Read;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use hermes_client::{CallError, CallOptions, Client};
use hermes_contract::methods::{
    canonical::image_attach_bytes,
    legacy::{complete_path, file_attach},
};
use hermes_contract::schemas::CompletionItem;
use talaria_core::attachments::{
    Accept, ChipId, ChipKind, ChipStore, CollapseRule, CompletionRequest, DropCandidate,
    ExpandError, Expanded, ImageState, ImageType, MentionState, PasteRoute, accept, route_paste,
};
use talaria_core::session::SessionId;

use crate::clipboard::{ClipboardImageReader, ClipboardResult};

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
    clipboard_started: Option<Instant>,
    clipboard_hint_shown: bool,
    home: PathBuf,
    workspace: PathBuf,
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
            clipboard_started: None,
            clipboard_hint_shown: false,
            home: PathBuf::from("~"),
            workspace: PathBuf::new(),
        }
    }

    /// Supply the operating-system home from application startup.
    pub fn set_home(&mut self, home: PathBuf) {
        self.home = home;
    }

    /// Supply the folder `@file` references are resolved against.
    pub fn set_workspace(&mut self, workspace: PathBuf) {
        self.workspace = workspace;
    }

    /// The configured collapse thresholds.
    pub fn rule(&self) -> CollapseRule {
        self.rule
    }

    /// Whether an asynchronous paste or clipboard read still owns the current draft.
    pub fn has_pending_work(&self) -> bool {
        self.pending_drops > 0 || self.clipboard_busy
    }

    /// Release only the bookkeeping for an identified completed submission.
    pub fn forget_submission(&mut self, expanded: &Expanded) {
        if self.last_expanded.as_ref() == Some(expanded) {
            self.last_expanded = None;
        }
    }

    /// Reconcile the terminal's sanitized result without changing completion focus.
    pub fn sync_draft(&mut self, draft: &str) -> Vec<AttachmentEffect> {
        self.chips.reconcile(draft);
        vec![self.highlights(draft)]
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
        let Some(action) = action else {
            return KeyRoute::PassToComposer;
        };
        if ctx.palette_open || draft.starts_with('/') {
            self.mention.close();
        }
        if self.mention.is_open() {
            match action {
                Action::CompletionNext => self.mention.next(),
                Action::CompletionPrev => self.mention.prev(),
                Action::CompletionClose | Action::Interrupt => self.mention.dismiss(),
                Action::CompletionAccept | Action::Submit if self.mention.has_items() => {
                    let selected = self.mention.selected().map(|item| item.text.clone());
                    let refused = selected.is_some_and(|text| {
                        file_reference_is_credential(&text, &self.workspace, &self.home)
                    });
                    if refused {
                        self.mention.close();
                        return KeyRoute::Consumed(vec![AttachmentEffect::Notice(
                            CREDENTIAL_FILE.to_owned(),
                        )]);
                    }
                    if let (Some(query), Some(item)) =
                        (self.mention.query(), self.mention.selected())
                    {
                        let result = accept(
                            draft,
                            query,
                            item,
                            if action == Action::Submit {
                                Accept::Insert
                            } else {
                                Accept::Complete
                            },
                        );
                        let effects = vec![AttachmentEffect::EditDraft {
                            range: query.start..query.start + query.word.len(),
                            text: result.draft[query.start..result.caret].to_owned(),
                        }];
                        if result.keep_open {
                            self.mention.on_edit(&result.draft, result.caret, now);
                        } else {
                            self.mention.close();
                        }
                        return KeyRoute::Consumed(effects);
                    }
                }
                Action::CompletionAccept => return KeyRoute::Consumed(Vec::new()),
                _ => return self.other_key(action, draft, caret, now),
            }
            return KeyRoute::Consumed(Vec::new());
        }
        self.other_key(action, draft, caret, now)
    }

    fn other_key(&mut self, action: Action, draft: &str, caret: usize, now: Instant) -> KeyRoute {
        let range = match action {
            Action::DeleteBackward => self.chips.label_ending_at(draft, caret),
            Action::DeleteForward => self.chips.label_starting_at(draft, caret),
            Action::AttachClipboardImage => {
                if self.clipboard_busy {
                    return KeyRoute::Consumed(Vec::new());
                }
                self.clipboard_busy = true;
                self.clipboard_started = Some(now);
                self.clipboard_hint_shown = false;
                return KeyRoute::Consumed(vec![AttachmentEffect::ReadClipboard {
                    quiet: false,
                    fallback_text: None,
                }]);
            }
            _ => None,
        };
        if let Some(range) = range {
            let mut edited = draft.to_owned();
            edited.replace_range(range.clone(), "");
            self.chips.reconcile(&edited);
            self.mention.close();
            KeyRoute::Consumed(vec![AttachmentEffect::EditDraft {
                range,
                text: String::new(),
            }])
        } else {
            KeyRoute::PassToComposer
        }
    }

    /// Reconcile after an edit. Only a typed edit feeds the mention list.
    pub fn on_edit(
        &mut self,
        draft: &str,
        caret: usize,
        typed: bool,
        now: Instant,
    ) -> Vec<AttachmentEffect> {
        self.chips.reconcile(draft);
        if typed && !draft.starts_with('/') {
            self.mention.on_edit(draft, caret, now);
        } else {
            self.mention.close();
        }
        if draft.is_empty() {
            self.pending_drops = 0;
            self.clipboard_busy = false;
            self.clipboard_started = None;
        }
        vec![self.highlights(draft)]
    }

    /// Restore paste payloads only for an explicit history recall.
    pub fn on_recall(&mut self, draft: &str, caret: usize, now: Instant) -> Vec<AttachmentEffect> {
        self.chips.restore_pastes(draft);
        self.on_edit(draft, caret, false, now)
    }

    fn highlights(&self, draft: &str) -> AttachmentEffect {
        AttachmentEffect::Highlight(
            self.chips
                .live(draft)
                .into_iter()
                .map(|(chip, range)| {
                    let style = if matches!(
                        chip.kind,
                        ChipKind::Image {
                            state: ImageState::Pending { .. },
                            ..
                        }
                    ) {
                        LabelStyle::Dimmed
                    } else {
                        LabelStyle::Accent
                    };
                    (range, style)
                })
                .collect(),
        )
    }

    /// Route one paste.
    pub fn on_paste(
        &mut self,
        text: &str,
        session: &SessionId,
        folder: &Path,
        rule: &CollapseRule,
    ) -> Vec<AttachmentEffect> {
        self.rule = *rule;
        self.mention.close();
        match route_paste(text, rule, &self.home) {
            PasteRoute::ProbeClipboard { fallback_text } => {
                self.clipboard_busy = true;
                self.clipboard_started = None;
                vec![AttachmentEffect::ReadClipboard {
                    quiet: true,
                    fallback_text,
                }]
            }
            PasteRoute::Drop(candidates) => {
                self.pending_drops = self.pending_drops.saturating_add(1);
                vec![AttachmentEffect::ResolveDrop {
                    session: session.clone(),
                    folder: folder.to_owned(),
                    candidates,
                    original: text.to_owned(),
                }]
            }
            PasteRoute::Collapse => vec![AttachmentEffect::EditDraft {
                range: 0..0,
                text: self.chips.collapse_paste(text),
            }],
            PasteRoute::Inline => vec![AttachmentEffect::EditDraft {
                range: 0..0,
                text: text.to_owned(),
            }],
        }
    }

    /// Request a completed folder or prefix without another debounce.
    pub fn complete_now(&mut self, session: &SessionId, now: Instant) -> Vec<AttachmentEffect> {
        self.mention
            .request_now(now)
            .map_or_else(Vec::new, |request| {
                vec![AttachmentEffect::Complete {
                    session: session.clone(),
                    request,
                }]
            })
    }

    /// Debounce, searching hint, and clipboard hint.
    pub fn on_tick(&mut self, session: &SessionId, now: Instant) -> Vec<AttachmentEffect> {
        let mut effects = Vec::new();
        if let Some(request) = self.mention.tick(now) {
            effects.push(AttachmentEffect::Complete {
                session: session.clone(),
                request,
            });
        }
        if self.clipboard_busy
            && !self.clipboard_hint_shown
            && self.clipboard_started.is_some_and(|start| {
                now.saturating_duration_since(start) >= std::time::Duration::from_millis(150)
            })
        {
            self.clipboard_hint_shown = true;
            effects.push(AttachmentEffect::Notice("Reading the clipboard…".into()));
        }
        effects
    }

    /// Next state deadline, supplied to the application loop without reading a clock.
    pub fn next_deadline(&self) -> Option<Instant> {
        let clipboard = self
            .clipboard_started
            .filter(|_| self.clipboard_busy && !self.clipboard_hint_shown)
            .map(|start| start + std::time::Duration::from_millis(150));
        match (self.mention.next_deadline(), clipboard) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Apply one service result when it still belongs to this draft.
    pub fn on_event(
        &mut self,
        event: AttachmentEvent,
        foreground: &SessionId,
        draft: &str,
        caret: usize,
    ) -> Vec<AttachmentEffect> {
        let owner = match &event {
            AttachmentEvent::Completions { session, .. }
            | AttachmentEvent::CompletionFailed { session, .. }
            | AttachmentEvent::DropImage { session, .. }
            | AttachmentEvent::DropFile { session, .. }
            | AttachmentEvent::DropNotAFile { session, .. }
            | AttachmentEvent::DropRefused { session, .. }
            | AttachmentEvent::ImageStaged { session, .. }
            | AttachmentEvent::FileAttached { session, .. }
            | AttachmentEvent::AttachFailed { session, .. } => Some(session),
            _ => None,
        };
        if owner.is_some_and(|owner| owner != foreground) {
            return Vec::new();
        }
        if !matches!(
            event,
            AttachmentEvent::Completions { .. } | AttachmentEvent::CompletionFailed { .. }
        ) {
            self.mention.close();
        }
        match event {
            AttachmentEvent::Completions {
                generation, items, ..
            } => {
                self.mention.on_items(generation, items);
                Vec::new()
            }
            AttachmentEvent::CompletionFailed {
                generation, reason, ..
            } => {
                self.mention.on_failed(generation, reason);
                Vec::new()
            }
            AttachmentEvent::DropImage {
                bytes,
                ty,
                remainder,
                ..
            } => {
                self.pending_drops = self.pending_drops.saturating_sub(1);
                self.image_effects(bytes, ty, foreground, draft, caret, &remainder)
            }
            AttachmentEvent::ClipboardImage { bytes, ty } => {
                self.clipboard_busy = false;
                self.clipboard_started = None;
                self.image_effects(bytes, ty, foreground, draft, caret, "")
            }
            AttachmentEvent::DropFile {
                path, remainder, ..
            } => {
                vec![AttachmentEffect::AttachFile {
                    session: foreground.clone(),
                    path,
                    remainder,
                }]
            }
            AttachmentEvent::DropNotAFile { original, .. } => {
                self.pending_drops = self.pending_drops.saturating_sub(1);
                let text = if self.rule.should_collapse(&original) {
                    self.chips.collapse_paste(&original)
                } else {
                    original
                };
                vec![AttachmentEffect::EditDraft {
                    range: caret..caret,
                    text,
                }]
            }
            AttachmentEvent::DropRefused { notice, .. } => {
                self.pending_drops = self.pending_drops.saturating_sub(1);
                vec![AttachmentEffect::Notice(notice)]
            }
            AttachmentEvent::ImageStaged {
                chip, path, mime, ..
            } => {
                self.chips.image_ready(chip, path, mime);
                vec![self.highlights(draft)]
            }
            AttachmentEvent::FileAttached {
                name,
                ref_text,
                uploaded,
                remainder,
                ..
            } => {
                self.pending_drops = self.pending_drops.saturating_sub(1);
                let mut text = if uploaded {
                    self.chips.add_file(draft, &name, ref_text, foreground)
                } else {
                    let mut text = ref_text;
                    if !draft
                        .get(caret..)
                        .and_then(|s| s.chars().next())
                        .is_some_and(char::is_whitespace)
                    {
                        text.push(' ');
                    }
                    text
                };
                if !remainder.is_empty() {
                    if !text.ends_with(char::is_whitespace) {
                        text.push(' ');
                    }
                    text.push_str(&remainder);
                }
                vec![AttachmentEffect::EditDraft {
                    range: caret..caret,
                    text,
                }]
            }
            AttachmentEvent::AttachFailed { chip, notice, .. } => {
                if chip.is_none() {
                    self.pending_drops = self.pending_drops.saturating_sub(1);
                }
                let mut effects = vec![AttachmentEffect::Notice(notice)];
                if let Some(chip) = chip
                    && let Some(label) = self.chips.image_failed(chip)
                    && let Some(start) = draft.find(&label)
                {
                    effects.push(AttachmentEffect::EditDraft {
                        range: start..start + label.len(),
                        text: String::new(),
                    });
                }
                effects
            }
            AttachmentEvent::ClipboardEmpty {
                quiet,
                notice,
                fallback_text,
            } => {
                self.clipboard_busy = false;
                self.clipboard_started = None;
                let mut effects = Vec::new();
                if !quiet && let Some(notice) = notice {
                    effects.push(AttachmentEffect::Notice(notice));
                }
                if let Some(text) = fallback_text {
                    effects.push(AttachmentEffect::EditDraft {
                        range: caret..caret,
                        text,
                    });
                }
                effects
            }
        }
    }

    fn image_effects(
        &mut self,
        bytes: Vec<u8>,
        ty: ImageType,
        session: &SessionId,
        draft: &str,
        caret: usize,
        remainder: &str,
    ) -> Vec<AttachmentEffect> {
        match self.chips.begin_image(draft, session) {
            Ok((chip, mut text)) => {
                if !remainder.is_empty() {
                    text.push(' ');
                    text.push_str(remainder);
                }
                vec![
                    AttachmentEffect::EditDraft {
                        range: caret..caret,
                        text,
                    },
                    AttachmentEffect::AttachImage {
                        session: session.clone(),
                        chip,
                        bytes,
                        ext: ty.ext,
                    },
                ]
            }
            Err(_) => vec![AttachmentEffect::Notice(
                "Hermes accepts at most 10 images per prompt.".into(),
            )],
        }
    }

    /// Expand the draft. The error is the notice and the draft stays.
    pub fn on_submit(&mut self, draft: &str) -> Result<Expanded, String> {
        if file_reference_is_credential(draft, &self.workspace, &self.home) {
            return Err(CREDENTIAL_FILE.to_owned());
        }
        let expanded = self.chips.expand(draft).map_err(|error| match error {
            ExpandError::Pending { .. } => "An attachment is still uploading; press Enter again when it finishes.".into(),
            ExpandError::MissingPaste { label } => {
                let n = label.strip_prefix("[Pasted text #").and_then(|s| s.split_whitespace().next()).unwrap_or("?");
                format!("Pasted text #{n} is no longer available. Delete its label or paste the text again.")
            }
            ExpandError::TooManyImages => "Hermes accepts at most 10 images per prompt.".into(),
        })?;
        self.mention.close();
        self.pending_drops = 0;
        self.clipboard_busy = false;
        self.clipboard_started = None;
        self.last_expanded = Some(expanded.clone());
        Ok(expanded)
    }

    /// Apply a submit outcome to the labels.
    pub fn on_submit_outcome(&mut self, outcome: SubmitOutcome) -> Vec<AttachmentEffect> {
        let Some(expanded) = self.last_expanded.clone() else {
            return Vec::new();
        };
        self.settle_submission(&expanded, outcome)
    }

    /// Settle the identified submission even when another draft has since been submitted.
    pub fn settle_submission(
        &mut self,
        expanded: &Expanded,
        outcome: SubmitOutcome,
    ) -> Vec<AttachmentEffect> {
        match outcome {
            SubmitOutcome::Receipt => {
                self.chips.mark_sent(expanded);
                if self.last_expanded.as_ref() == Some(expanded) {
                    self.last_expanded = None;
                }
                Vec::new()
            }
            SubmitOutcome::Refused { reason }
                if reason == "invalid_params" && !expanded.attachments.is_empty() =>
            {
                self.chips.invalidate_images(&expanded.attachments);
                vec![AttachmentEffect::Notice(
                    "Hermes no longer has the attached images; attach them again.".into(),
                )]
            }
            SubmitOutcome::VoiceStopped => {
                self.chips.reconcile("");
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// A new foreground session. Paste labels stay.
    pub fn on_session_change(&mut self, new: &SessionId) -> Vec<AttachmentEffect> {
        self.mention.close();
        self.pending_drops = 0;
        self.clipboard_busy = false;
        self.clipboard_started = None;
        self.last_expanded = None;
        if self.chips.drop_foreign_payloads(new) {
            vec![AttachmentEffect::Notice(
                "Attachments were removed because the session changed.".into(),
            )]
        } else {
            Vec::new()
        }
    }
}

/// Runs effects off the input thread and delivers [`AttachmentEvent`]s.
pub struct AttachmentService {
    client: Option<Client>,
    events: Arc<dyn Fn(AttachmentEvent) + Send + Sync>,
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
            events: Arc::from(events),
            clipboard,
            home,
        }
    }

    /// Start `effect`. The outcome arrives later as an event.
    pub fn execute(&self, effect: AttachmentEffect) {
        let client = self.client.clone();
        let events = Arc::clone(&self.events);
        match effect {
            AttachmentEffect::Complete { session, request } => {
                tokio::spawn(async move {
                    let result = match client {
                        Some(client) => {
                            client
                                .call_with::<complete_path::Marker>(
                                    complete_path::Params {
                                        session_id: Some(session.as_str().to_owned()),
                                        word: Some(request.word),
                                        ..Default::default()
                                    },
                                    CallOptions {
                                        timeout: Some(Duration::from_secs(2)),
                                    },
                                )
                                .await
                        }
                        None => Err(CallError::NotConnected),
                    };
                    events(match result {
                        Ok(result) => AttachmentEvent::Completions {
                            session,
                            generation: request.generation,
                            items: result.items,
                        },
                        Err(error) => AttachmentEvent::CompletionFailed {
                            session,
                            generation: request.generation,
                            reason: refusal_notice(&error, Feature::Completion),
                        },
                    });
                });
            }
            AttachmentEffect::ResolveDrop {
                session,
                folder,
                candidates,
                original,
            } => {
                let home = self.home.clone();
                tokio::task::spawn_blocking(move || {
                    events(resolve_drop(session, &folder, candidates, original, &home))
                });
            }
            AttachmentEffect::AttachImage {
                session,
                chip,
                bytes,
                ext: _,
            } => {
                tokio::spawn(async move {
                    let encoded = tokio::task::spawn_blocking(move || encode_image(&bytes)).await;
                    let result = match encoded {
                        Ok(Ok((content_base64, filename, expected_mime))) => match client {
                            Some(client) => client
                                .call_with::<image_attach_bytes::Marker>(
                                    image_attach_bytes::Params {
                                        session_id: session.as_str().to_owned(),
                                        content_base64: Some(content_base64),
                                        filename: Some(filename),
                                        data: None,
                                        ext: None,
                                        profile: None,
                                    },
                                    CallOptions {
                                        timeout: Some(Duration::from_secs(30)),
                                    },
                                )
                                .await
                                .map_err(|error| refusal_notice(&error, Feature::ImageAttach))
                                .and_then(|result| match result.path {
                                    Some(path)
                                        if result.attached
                                            && !path.trim().is_empty()
                                            && result.mime == expected_mime =>
                                    {
                                        Ok((path, result.mime))
                                    }
                                    _ => Err(INVALID_ATTACHMENT.to_owned()),
                                }),
                            None => Err(refusal_notice(
                                &CallError::NotConnected,
                                Feature::ImageAttach,
                            )),
                        },
                        Ok(Err(notice)) => Err(notice),
                        Err(_) => Err("Talaria could not prepare that image.".to_owned()),
                    };
                    events(match result {
                        Ok((path, mime)) => AttachmentEvent::ImageStaged {
                            session,
                            chip,
                            path,
                            mime,
                        },
                        Err(notice) => AttachmentEvent::AttachFailed {
                            session,
                            chip: Some(chip),
                            notice,
                        },
                    });
                });
            }
            AttachmentEffect::AttachFile {
                session,
                path,
                remainder,
            } => {
                let home = self.home.clone();
                tokio::spawn(async move {
                    let checked =
                        tokio::task::spawn_blocking(move || check_file_for_attach(&path, &home))
                            .await;
                    let result = match checked {
                        Ok(Ok(path)) => match client {
                            Some(client) => client
                                .call_with::<file_attach::Marker>(
                                    file_attach::Params {
                                        session_id: session.as_str().to_owned(),
                                        path: Some(path),
                                        data_url: None,
                                        name: None,
                                        profile: None,
                                        staged_upload: None,
                                    },
                                    CallOptions { timeout: None },
                                )
                                .await
                                .map_err(|error| refusal_notice(&error, Feature::FileAttach))
                                .and_then(|result| {
                                    if result.attached
                                        && !result.name.is_empty()
                                        && !result.path.is_empty()
                                        && !result.ref_path.is_empty()
                                        && result.ref_text.starts_with("@file:")
                                        && result.ref_text.len() > 6
                                    {
                                        Ok(result)
                                    } else {
                                        Err(INVALID_ATTACHMENT.to_owned())
                                    }
                                }),
                            None => Err(refusal_notice(
                                &CallError::NotConnected,
                                Feature::FileAttach,
                            )),
                        },
                        Ok(Err(notice)) => Err(notice),
                        Err(_) => Err("Talaria could not check that file.".to_owned()),
                    };
                    events(match result {
                        Ok(result) => AttachmentEvent::FileAttached {
                            session,
                            name: sanitize_attachment_text(&result.name),
                            ref_text: result.ref_text,
                            uploaded: result.uploaded,
                            remainder,
                        },
                        Err(notice) => AttachmentEvent::AttachFailed {
                            session,
                            chip: None,
                            notice,
                        },
                    });
                });
            }
            AttachmentEffect::ReadClipboard {
                quiet,
                fallback_text,
            } => {
                let clipboard = self.clipboard.clone();
                tokio::task::spawn_blocking(move || {
                    let result = clipboard.read();
                    let notice = match result {
                        ClipboardResult::Image(bytes) => {
                            if bytes.len() > IMAGE_LIMIT {
                                Some("That image is larger than 20 MiB.")
                            } else if let Some(ty) = talaria_core::attachments::sniff_image(&bytes)
                            {
                                events(AttachmentEvent::ClipboardImage { bytes, ty });
                                return;
                            } else {
                                Some(UNSUPPORTED_IMAGE)
                            }
                        }
                        ClipboardResult::NoTool => Some(
                            "Reading clipboard images needs wl-paste (Wayland) or xclip (X11).",
                        ),
                        ClipboardResult::Empty => Some(
                            "No image on the clipboard. To paste text, use your terminal's paste (Cmd+V or Ctrl+Shift+V).",
                        ),
                    };
                    events(AttachmentEvent::ClipboardEmpty {
                        quiet,
                        notice: notice.filter(|_| !quiet).map(str::to_owned),
                        fallback_text,
                    });
                });
            }
            AttachmentEffect::EditDraft { .. }
            | AttachmentEffect::Highlight(_)
            | AttachmentEffect::Notice(_) => {}
        }
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
    match error {
        CallError::TimedOut { .. } if feature == Feature::Completion => {
            "Hermes did not answer in time.".to_owned()
        }
        CallError::Unknown { .. } | CallError::NotConnected if feature == Feature::Completion => {
            "The connection to Hermes dropped.".to_owned()
        }
        CallError::TimedOut { .. } | CallError::Unknown { .. } | CallError::NotConnected => {
            "The attachment may not have reached Hermes; attach it again.".to_owned()
        }
        CallError::Rpc(error) if error.code == -32601 => format!(
            "This Hermes gateway does not support {}.",
            match feature {
                Feature::Completion => "@ completion",
                Feature::FileAttach => "file attachments",
                Feature::ImageAttach => "image attachments",
            }
        ),
        CallError::Rpc(error) => {
            let reason = error.reason.as_deref().unwrap_or(&error.message);
            match (error.code, reason) {
                (5001, _) | (4001, "storage_unavailable") => {
                    "Hermes could not store the attachment (disk or permissions).".to_owned()
                }
                (4001, "invalid_params") => INVALID_ATTACHMENT.to_owned(),
                (4001, "permission_denied") => {
                    "This session is not attached on this connection yet; try again in a moment."
                        .to_owned()
                }
                (4001, "not_found") => "Hermes does not know this session.".to_owned(),
                _ => format!("Hermes refused: {}", sanitize_attachment_text(reason)),
            }
        }
        CallError::BadResult { .. } => "Hermes returned an invalid attachment result.".to_owned(),
        CallError::Encode | CallError::InvalidTimeout => {
            "Talaria could not prepare the attachment request.".to_owned()
        }
    }
}

const IMAGE_LIMIT: usize = 20 * 1024 * 1024;
const FILE_LIMIT: u64 = 50 * 1024 * 1024;
const INVALID_ATTACHMENT: &str =
    "Hermes could not accept that attachment (unsupported or damaged).";
const UNSUPPORTED_IMAGE: &str = "Hermes accepts PNG, JPEG, GIF and WebP images.";
const CREDENTIAL_FILE: &str = "Talaria does not attach credential files.";

fn encode_image(bytes: &[u8]) -> Result<(String, String, &'static str), String> {
    if bytes.len() > IMAGE_LIMIT {
        return Err("That image is larger than 20 MiB.".to_owned());
    }
    let ty = talaria_core::attachments::sniff_image(bytes)
        .ok_or_else(|| UNSUPPORTED_IMAGE.to_owned())?;
    let mut id = [0u8; 16];
    getrandom::fill(&mut id).map_err(|_| "Talaria could not prepare that image.".to_owned())?;
    id[6] = (id[6] & 0x0f) | 0x40;
    id[8] = (id[8] & 0x3f) | 0x80;
    let hex = id
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let filename = format!(
        "{}-{}-{}-{}-{}.{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..],
        ty.ext
    );
    Ok((
        base64::engine::general_purpose::STANDARD.encode(bytes),
        filename,
        ty.mime,
    ))
}

fn resolve_drop(
    session: SessionId,
    folder: &Path,
    candidates: Vec<DropCandidate>,
    original: String,
    home: &Path,
) -> AttachmentEvent {
    for candidate in candidates {
        let path = absolute_path(&candidate.path, folder, home);
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                return AttachmentEvent::DropRefused {
                    session,
                    notice: unreadable(&path),
                };
            }
        };
        let result = inspect_file(&path, &metadata, home).and_then(|(mut file, target)| {
            let mut prefix = [0u8; 12];
            let read = file.read(&mut prefix).map_err(|_| unreadable(&path))?;
            if let Some(ty) = talaria_core::attachments::sniff_image(&prefix[..read]) {
                if metadata.len() > IMAGE_LIMIT as u64 {
                    return Err("That image is larger than 20 MiB.".to_owned());
                }
                let mut bytes = prefix[..read].to_vec();
                file.take(IMAGE_LIMIT as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| unreadable(&path))?;
                if bytes.len() > IMAGE_LIMIT {
                    return Err("That image is larger than 20 MiB.".to_owned());
                }
                verify_target(&path, &target, home)?;
                Ok(AttachmentEvent::DropImage {
                    session: session.clone(),
                    bytes,
                    ty,
                    remainder: candidate.remainder.clone(),
                })
            } else if talaria_core::attachments::unsupported_image(&prefix[..read], &path) {
                Err(UNSUPPORTED_IMAGE.to_owned())
            } else if metadata.len() > FILE_LIMIT {
                Err("Talaria attaches files up to 50 MiB.".to_owned())
            } else {
                verify_target(&path, &target, home)?;
                Ok(AttachmentEvent::DropFile {
                    session: session.clone(),
                    path: path.clone(),
                    remainder: candidate.remainder.clone(),
                })
            }
        });
        return result.unwrap_or_else(|notice| AttachmentEvent::DropRefused { session, notice });
    }
    AttachmentEvent::DropNotAFile { session, original }
}

fn check_file_for_attach(path: &Path, home: &Path) -> Result<String, String> {
    if !path.is_absolute() {
        return Err(unreadable(path));
    }
    let metadata = std::fs::metadata(path).map_err(|_| unreadable(path))?;
    let (_file, target) = inspect_file(path, &metadata, home)?;
    if metadata.len() > FILE_LIMIT {
        return Err("Talaria attaches files up to 50 MiB.".to_owned());
    }
    verify_target(path, &target, home)?;
    gateway_attach_path(&target, home)
}

fn inspect_file(
    path: &Path,
    metadata: &std::fs::Metadata,
    home: &Path,
) -> Result<(std::fs::File, PathBuf), String> {
    let target = std::fs::canonicalize(path).map_err(|_| unreadable(path))?;
    if credential_path(path, home) || credential_path(&target, home) {
        return Err(CREDENTIAL_FILE.to_owned());
    }
    if !metadata.is_file() {
        return Err(unreadable(path));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o444 == 0 {
            return Err(unreadable(path));
        }
    }
    let file = std::fs::File::open(&target).map_err(|_| unreadable(path))?;
    let opened = file.metadata().map_err(|_| unreadable(path))?;
    if !opened.is_file() {
        return Err(unreadable(path));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.dev() != opened.dev() || metadata.ino() != opened.ino() {
            return Err(unreadable(path));
        }
    }
    verify_target(path, &target, home)?;
    Ok((file, target))
}

fn verify_target(path: &Path, expected: &Path, home: &Path) -> Result<(), String> {
    let target = std::fs::canonicalize(path).map_err(|_| unreadable(path))?;
    if credential_path(&target, home) {
        return Err(CREDENTIAL_FILE.to_owned());
    }
    if target != expected {
        return Err(unreadable(path));
    }
    Ok(())
}

fn absolute_path(path: &Path, folder: &Path, home: &Path) -> PathBuf {
    let expanded = if let Ok(tail) = path.strip_prefix("~") {
        home.join(tail)
    } else if path.is_absolute() {
        path.to_path_buf()
    } else {
        folder.join(path)
    };
    // Keep parent components: collapsing them could change a path through a symlink.
    expanded.components().collect()
}

fn credential_path(path: &Path, home: &Path) -> bool {
    let path = normalize_path(path);
    if env_or_private_key_name(&path) {
        return true;
    }
    let home = normalize_path(home);
    let canonical_home = std::fs::canonicalize(&home).unwrap_or_else(|_| home.clone());
    let hermes = std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".hermes"));
    let hermes = if hermes.is_absolute() {
        hermes
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&hermes))
            .unwrap_or(hermes)
    };
    let homes = [home, canonical_home];
    if homes.iter().any(|home| home_credential(&path, home)) {
        return true;
    }
    under_directory(&path, &hermes)
}

/// Env files and private-key material are credential names in any directory.
fn env_or_private_key_name(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    let name = ascii_lower(name.as_encoded_bytes());
    if name.as_slice() == b".env" || name.as_slice() == b".envrc" || name.starts_with(b".env.") {
        return true;
    }
    matches!(
        name.as_slice(),
        b"id_rsa" | b"id_ed25519" | b"id_ecdsa" | b"id_dsa"
    ) || [b".pem".as_slice(), b".key", b".p12", b".pfx"]
        .iter()
        .any(|extension| name.ends_with(extension))
}

/// Home stores include nested paths and the canonical target of a symlinked store.
fn home_credential(path: &Path, home: &Path) -> bool {
    const STORES: &[&str] = &[
        ".ssh",
        ".aws",
        ".gnupg",
        ".kube",
        ".docker",
        ".azure",
        ".config/gh",
        ".config/gcloud",
        ".talaria",
        "Library/Keychains",
        ".password-store",
        ".config/op",
        ".config/1password",
        ".local/share/keyrings",
        ".config/age",
        ".config/sops",
        ".config/git/credentials",
    ];
    const FILES: &[&str] = &[
        ".bashrc",
        ".zshrc",
        ".profile",
        ".bash_profile",
        ".zprofile",
        ".netrc",
        ".pgpass",
        ".npmrc",
        ".pypirc",
        ".git-credentials",
    ];
    STORES
        .iter()
        .any(|directory| under_directory(path, &home.join(directory)))
        || FILES.iter().any(|file| {
            let denied = home.join(file);
            path_eq_ci(path, &normalize_path(&denied))
                || std::fs::canonicalize(&denied).is_ok_and(|denied| path_eq_ci(path, &denied))
        })
}

fn under_directory(path: &Path, directory: &Path) -> bool {
    let directory = normalize_path(directory);
    path_has_prefix_ci(path, &directory)
        || std::fs::canonicalize(&directory)
            .is_ok_and(|directory| path_has_prefix_ci(path, &directory))
}

fn ascii_lower(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(u8::to_ascii_lowercase).collect()
}

fn component_key(component: std::path::Component<'_>) -> Vec<u8> {
    ascii_lower(component.as_os_str().as_encoded_bytes())
}

fn path_has_prefix_ci(path: &Path, prefix: &Path) -> bool {
    let prefix: Vec<Vec<u8>> = prefix.components().map(component_key).collect();
    if prefix.is_empty() {
        return false;
    }
    let path: Vec<Vec<u8>> = path.components().map(component_key).collect();
    path.len() >= prefix.len()
        && path
            .iter()
            .zip(prefix.iter())
            .all(|(left, right)| left == right)
}

fn path_eq_ci(path: &Path, other: &Path) -> bool {
    let path: Vec<Vec<u8>> = path.components().map(component_key).collect();
    let other: Vec<Vec<u8>> = other.components().map(component_key).collect();
    path == other
}

fn file_reference_is_credential(text: &str, workspace: &Path, home: &Path) -> bool {
    file_reference_values(text)
        .iter()
        .any(|value| credential_reference(Path::new(value), workspace, home))
}

fn credential_reference(path: &Path, workspace: &Path, home: &Path) -> bool {
    let lexical = absolute_path(path, workspace, home);
    credential_path(&lexical, home)
        || std::fs::canonicalize(&lexical).is_ok_and(|target| credential_path(&target, home))
}

fn file_reference_values(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut consumed = 0;
    for (at, ch) in text.char_indices() {
        if at < consumed || ch != '@' {
            continue;
        }
        if text[..at]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_alphanumeric() || ch == '_' || ch == '/')
        {
            continue;
        }
        let tail = &text[at + 1..];
        let Some(value) = tail.strip_prefix("file:") else {
            continue;
        };
        if value.is_empty() || value.starts_with(char::is_whitespace) {
            continue;
        }
        let (raw, len) = take_reference_value(value);
        values.push(unquote_reference(raw).to_owned());
        consumed = at + "file:".len() + 1 + len;
    }
    values
}

fn take_reference_value(value: &str) -> (&str, usize) {
    let mut value_len = value.find(char::is_whitespace).unwrap_or(value.len());
    if let Some(quote) = value
        .chars()
        .next()
        .filter(|ch| ['`', '"', '\''].contains(ch))
        && let Some(end) = value[1..]
            .find(quote)
            .map(|end| end + 1)
            .filter(|end| *end > 1 && !value[1..*end].contains('\n'))
    {
        value_len = end + 1;
    }
    (&value[..value_len], value_len)
}

fn unquote_reference(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let quote = bytes[0];
        if matches!(quote, b'`' | b'"' | b'\'') && bytes[bytes.len() - 1] == quote {
            return &value[1..value.len() - 1];
        }
    }
    value
}

/// The pathname Hermes `file.attach` will open after strip, quote removal, and expansion.
fn gateway_attach_path(target: &Path, home: &Path) -> Result<String, String> {
    let Some(serialized) = target.to_str() else {
        return Err(unreadable(target));
    };
    let reinterpreted = hermes_reinterpreted_path(serialized, home);
    if hermes_pathname_unstable(serialized)
        || reinterpreted != serialized
        || credential_path(Path::new(&reinterpreted), home)
    {
        return Err(CREDENTIAL_FILE.to_owned());
    }
    Ok(serialized.to_owned())
}

fn hermes_pathname_unstable(path: &str) -> bool {
    path != path.trim()
        || path.chars().any(char::is_control)
        || path.chars().any(|ch| matches!(ch, '"' | '\'' | '`' | '\\'))
        || path.split('/').any(|segment| segment.starts_with('~'))
        || has_env_syntax(path)
}

fn has_env_syntax(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.iter().enumerate().any(|(index, byte)| {
        if *byte != b'$' {
            return false;
        }
        match bytes.get(index + 1).copied() {
            Some(b'{') => true,
            Some(next) => next.is_ascii_alphabetic() || next == b'_',
            None => false,
        }
    })
}

fn hermes_reinterpreted_path(raw: &str, home: &Path) -> String {
    let mut token = raw.trim().to_owned();
    if token.len() >= 2 {
        let bytes = token.as_bytes();
        let quote = bytes[0];
        if matches!(quote, b'"' | b'\'') && bytes[token.len() - 1] == quote {
            token = token[1..token.len() - 1].trim().to_owned();
        }
    }
    token = token.replace("\\ ", " ");
    token = expand_user(&token, home);
    expand_shell_vars(&token)
}

fn expand_user(token: &str, home: &Path) -> String {
    if token == "~" {
        return home.to_str().unwrap_or("").to_owned();
    }
    if let Some(rest) = token.strip_prefix("~/") {
        return home.join(rest).to_str().unwrap_or("").to_owned();
    }
    token.to_owned()
}

fn expand_shell_vars(token: &str) -> String {
    let mut out = String::new();
    let mut rest = token;
    while let Some(dollar) = rest.find('$') {
        out.push_str(&rest[..dollar]);
        let after = &rest[dollar + 1..];
        if let Some(body) = after.strip_prefix('{')
            && let Some((name, tail)) = body.split_once('}')
            && env_name(name)
        {
            match std::env::var(name) {
                Ok(value) => out.push_str(&value),
                Err(_) => {
                    out.push_str("${");
                    out.push_str(name);
                    out.push('}');
                }
            }
            rest = tail;
            continue;
        }
        let ident_len = after
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            .map(char::len_utf8)
            .sum::<usize>();
        let name = &after[..ident_len];
        if env_name(name) {
            match std::env::var(name) {
                Ok(value) => out.push_str(&value),
                Err(_) => {
                    out.push('$');
                    out.push_str(name);
                }
            }
            rest = &after[ident_len..];
            continue;
        }
        out.push('$');
        rest = after;
    }
    out.push_str(rest);
    out
}

fn env_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn unreadable(path: &Path) -> String {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    format!("Talaria cannot read {}.", sanitize_attachment_text(&name))
}

fn sanitize_attachment_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match u32::from(ch) {
            0x09 => result.push_str("    "),
            code @ 0x00..=0x1f => {
                if let Some(picture) = char::from_u32(0x2400 + code) {
                    result.push(picture);
                }
            }
            0x7f => result.push('\u{2421}'),
            code @ (0x80..=0x9f
            | 0x061c
            | 0x200e
            | 0x200f
            | 0x2028..=0x202e
            | 0x2066..=0x2069
            | 0xe0000..=0xe007f) => result.push_str(&format!("‹U+{code:04X}›")),
            _ => result.push(ch),
        }
    }
    result
}
