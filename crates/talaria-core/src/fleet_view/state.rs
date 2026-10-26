//! Overlay reducer: rows, cursor, filters, peek, reply, stop and answer focus. Pure; time arrives
//! in `Tick` inputs.

use std::collections::{HashMap, HashSet};

use super::detail::{row_detail, title_or_untitled};
use crate::directory::rows::{RowKey, format_age};
use crate::fleet::{FleetRow, NeedItem, ProfileStatus, RowState};

/// The longest title a notice names before it is cut.
const TITLE_CHARS: usize = 40;

/// Where keys go while the overlay is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Focus {
    List,
    Filter,
    Reply,
    ConfirmStop,
    Answer,
}

/// A key from the fleet key map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FleetAction {
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    Open,
    Peek,
    Reply,
    Stop,
    Answer,
    Filter,
    ProfileNext,
    ProfilePrev,
    Close,
    /// Start a background session (the app opens the launch dialog).
    New,
    /// Start a background session in a new worktree.
    NewInWorktree,
    /// Review the row's session folder.
    Review,
}

/// How far a peek has got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeekStatus {
    Opening,
    Open {
        has_open_prompt: bool,
        running: bool,
    },
    Failed {
        text: String,
    },
}

/// Which profile's rows are on screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileFilter {
    All,
    One(String),
}

/// The result of one reply, fed back into the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplyOutcome {
    /// Admission status `started`.
    Sent,
    /// Admission status `queued`.
    Queued,
    /// Admission status `terminal`.
    AlreadyRan,
    /// Admission status `unknown`.
    StateUnknown,
    /// The reply was taken as a voice stop.
    VoiceStopped,
    /// Hermes refused the call.
    Refused { reason: String },
    /// The attach failed, so nothing was sent.
    NotAttached { reason: String },
    /// The call may have been sent. `why` is `no reply` or `connection dropped`.
    Unknown { why: String },
}

/// The result of one stop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopOutcome {
    Stopping,
    NothingRunning,
    Refused { reason: String },
    NotAttached { reason: String },
    Unknown { why: String },
}

/// One input to [`update`].
#[derive(Clone, Debug, PartialEq)]
pub enum FleetViewInput {
    Snapshot {
        version: u64,
        rows: Vec<FleetRow>,
        queue: Vec<NeedItem>,
        profiles: Vec<(String, ProfileStatus)>,
        scope_note: Option<String>,
        link_note: Option<String>,
    },
    Action(FleetAction),
    Text(char),
    Paste(String),
    Backspace,
    Left,
    Right,
    Home,
    End,
    Enter,
    Escape,
    CtrlC,
    AnyOtherKey,
    Peek {
        key: RowKey,
        status: PeekStatus,
    },
    PeekClosed,
    ReplyDone {
        key: RowKey,
        outcome: ReplyOutcome,
    },
    StopDone {
        key: RowKey,
        outcome: StopOutcome,
    },
    AnswerFocusEnded,
    Notice(String),
    Resize {
        cols: u16,
        rows: u16,
    },
    Tick {
        now: f64,
    },
}

/// What the application should do after an input. `Close` is last in its batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetViewEffect {
    OpenPeek(RowKey),
    ClosePeek,
    SendReply { key: RowKey, text: String },
    ResendReply(RowKey),
    Stop(RowKey),
    Acknowledge(RowKey),
    Open(RowKey),
    FocusAnswer,
    /// Open the review of this row's session folder.
    Review(RowKey),
    Close,
}

/// Read-only flags for one row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowFlags {
    pub reply_in_flight: bool,
    pub unconfirmed: bool,
    pub stop_in_flight: bool,
}

/// One visible group, in [`RowState`] order, with each row's detail text.
pub struct VisibleGroup<'a> {
    pub state: RowState,
    pub rows: Vec<(&'a FleetRow, String)>,
}

/// The overlay. Fields stay private; tests use the methods below.
#[derive(Clone, Debug)]
pub struct FleetViewState {
    rows: Vec<FleetRow>,
    queue: Vec<NeedItem>,
    profiles: Vec<(String, ProfileStatus)>,
    scope_note: Option<String>,
    link_note: Option<String>,
    version: u64,
    seen_snapshot: bool,
    foreground: Option<RowKey>,
    now: f64,
    size: Option<(u16, u16)>,
    focus: Focus,
    cursor: Option<RowKey>,
    filter: String,
    profile_filter: ProfileFilter,
    peek: Option<(RowKey, PeekStatus)>,
    notice: Option<String>,
    /// The row the reply editor or the stop confirmation is for.
    target: Option<RowKey>,
    drafts: HashMap<RowKey, String>,
    caret: usize,
    last_sent: HashMap<RowKey, String>,
    in_flight: HashSet<RowKey>,
    unconfirmed: HashMap<RowKey, String>,
    stopping: HashSet<RowKey>,
    held_open_at: Option<RowKey>,
    titles: HashMap<RowKey, String>,
    dirty: bool,
    drawn_version: u64,
}

impl FleetViewState {
    /// An overlay that has not seen a snapshot yet.
    #[must_use]
    pub fn new(foreground: Option<RowKey>, now: f64) -> Self {
        Self {
            rows: Vec::new(),
            queue: Vec::new(),
            profiles: Vec::new(),
            scope_note: None,
            link_note: None,
            version: 0,
            seen_snapshot: false,
            foreground,
            now,
            size: None,
            focus: Focus::List,
            cursor: None,
            filter: String::new(),
            profile_filter: ProfileFilter::All,
            peek: None,
            notice: None,
            target: None,
            drafts: HashMap::new(),
            caret: 0,
            last_sent: HashMap::new(),
            in_flight: HashSet::new(),
            unconfirmed: HashMap::new(),
            stopping: HashSet::new(),
            held_open_at: None,
            titles: HashMap::new(),
            dirty: true,
            drawn_version: 0,
        }
    }

    #[must_use]
    pub fn focus(&self) -> &Focus {
        &self.focus
    }

    /// Groups in [`RowState`] order with the filters applied; empty groups are omitted.
    #[must_use]
    pub fn visible(&self) -> Vec<VisibleGroup<'_>> {
        let mut groups: Vec<VisibleGroup<'_>> = Vec::new();
        for state in GROUP_ORDER {
            let rows: Vec<(&FleetRow, String)> = self
                .rows
                .iter()
                .filter(|row| row.state == state)
                .filter_map(|row| {
                    let detail = self.detail_of(row);
                    self.shows(row, &detail).then_some((row, detail))
                })
                .collect();
            if !rows.is_empty() {
                groups.push(VisibleGroup { state, rows });
            }
        }
        groups
    }

    #[must_use]
    pub fn cursor(&self) -> Option<&RowKey> {
        self.cursor.as_ref()
    }

    /// The text filter, as typed.
    #[must_use]
    pub fn filter_text(&self) -> &str {
        &self.filter
    }

    /// Which profile's rows are shown.
    #[must_use]
    pub fn profile_filter(&self) -> &ProfileFilter {
        &self.profile_filter
    }

    /// The note for a scope narrowed to one profile.
    #[must_use]
    pub fn scope_note(&self) -> Option<&str> {
        self.scope_note.as_deref()
    }

    /// The note about the fleet connection.
    #[must_use]
    pub fn link_note(&self) -> Option<&str> {
        self.link_note.as_deref()
    }

    /// Every profile with its listing status, in the order the snapshot gave.
    #[must_use]
    pub fn profiles(&self) -> &[(String, ProfileStatus)] {
        &self.profiles
    }

    /// How many rows the snapshot holds, before any filter.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// The time the last tick or snapshot carried, for ages.
    #[must_use]
    pub fn now(&self) -> f64 {
        self.now
    }

    /// Whether a snapshot has arrived yet.
    #[must_use]
    pub fn seen_snapshot(&self) -> bool {
        self.seen_snapshot
    }

    /// The row of the foreground session, when one is known.
    #[must_use]
    pub fn foreground(&self) -> Option<&RowKey> {
        self.foreground.as_ref()
    }

    /// The snapshot row for `key`, shown or filtered out.
    #[must_use]
    pub fn find_row(&self, key: &RowKey) -> Option<&FleetRow> {
        self.row(key)
    }

    #[must_use]
    pub fn peek(&self) -> Option<(&RowKey, &PeekStatus)> {
        self.peek.as_ref().map(|(key, status)| (key, status))
    }

    /// The notice line. In the reply editor without a newer notice it is the prompt line,
    /// `Reply to <title>: ` followed by the draft with line breaks drawn `↵`.
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    #[must_use]
    pub fn reply_line(&self) -> Option<(&RowKey, &str)> {
        let key = self.target.as_ref()?;
        if self.focus == Focus::Reply {
            return self
                .drafts
                .get(key)
                .map(|draft| (key, draft.as_str()))
                .or(Some((key, "")));
        }
        self.drafts.get(key).map(|draft| (key, draft.as_str()))
    }

    #[must_use]
    pub fn needs_redraw(&self) -> bool {
        self.dirty
    }

    pub fn take_frame(&mut self) {
        self.dirty = false;
        self.drawn_version = self.version;
    }

    #[must_use]
    pub fn row_flags(&self, key: &RowKey) -> RowFlags {
        RowFlags {
            reply_in_flight: self.in_flight.contains(key),
            unconfirmed: self.unconfirmed.contains_key(key),
            stop_in_flight: self.stopping.contains(key),
        }
    }

    /// Rows whose reply is in flight or unconfirmed: key, sanitized title, and `no reply` or
    /// `connection dropped`. A reply still in flight counts as `no reply`.
    #[must_use]
    pub fn unsettled_replies(&self) -> Vec<(RowKey, String, String)> {
        let mut keys: Vec<&RowKey> = self
            .in_flight
            .iter()
            .chain(self.unconfirmed.keys())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        keys.sort();
        keys.into_iter()
            .map(|key| {
                let why = if self.in_flight.contains(key) {
                    "no reply".to_owned()
                } else {
                    self.unconfirmed
                        .get(key)
                        .cloned()
                        .unwrap_or_else(|| "no reply".to_owned())
                };
                (key.clone(), self.title_of(key), why)
            })
            .collect()
    }

    /// Put the cursor on `key` and apply the same rules as Space.
    ///
    /// Before the first snapshot this holds the key and emits nothing. A key that is not in
    /// the snapshot sets the notice and emits nothing.
    pub fn open_at(&mut self, key: RowKey) -> Vec<FleetViewEffect> {
        if !self.seen_snapshot {
            self.held_open_at = Some(key);
            return Vec::new();
        }
        self.dirty = true;
        if !self.rows.iter().any(|row| row.key == key) {
            self.notice = Some(GONE_FROM_FLEET.to_owned());
            return Vec::new();
        }
        // The row may be hidden by a filter; clear them so the cursor is on a visible row.
        if !self.visible_keys().contains(&key) {
            self.filter.clear();
            self.profile_filter = ProfileFilter::All;
        }
        self.cursor = Some(key.clone());
        if self.peek.as_ref().is_some_and(|(peeked, _)| *peeked == key) {
            return Vec::new();
        }
        self.open_peek(key)
    }

    fn row(&self, key: &RowKey) -> Option<&FleetRow> {
        self.rows.iter().find(|row| row.key == *key)
    }

    fn is_foreground(&self, key: &RowKey) -> bool {
        self.foreground.as_ref() == Some(key) || self.row(key).is_some_and(|row| row.foreground)
    }

    fn items_of(&self, key: &RowKey) -> Vec<&NeedItem> {
        let mut items: Vec<&NeedItem> = self
            .queue
            .iter()
            .filter(|item| item.id.key == *key)
            .collect();
        items.sort_by_key(|item| item.seq);
        items
    }

    fn detail_of(&self, row: &FleetRow) -> String {
        row_detail(row, &self.items_of(&row.key))
    }

    fn shows(&self, row: &FleetRow, detail: &str) -> bool {
        let profile_ok = match &self.profile_filter {
            ProfileFilter::All => true,
            ProfileFilter::One(profile) => row.key.profile == *profile,
        };
        profile_ok && super::detail::matches(row, detail, &self.filter)
    }

    fn visible_keys(&self) -> Vec<RowKey> {
        self.visible()
            .iter()
            .flat_map(|group| group.rows.iter().map(|(row, _)| row.key.clone()))
            .collect()
    }

    /// The title a text names: `(untitled)` when empty, cut to 40 characters.
    fn title_of(&self, key: &RowKey) -> String {
        let title = self
            .row(key)
            .map(title_or_untitled)
            .or_else(|| self.titles.get(key).cloned())
            .unwrap_or_else(|| "(untitled)".to_owned());
        cut(&title, TITLE_CHARS)
    }

    fn ages(&self, now: f64) -> Vec<String> {
        self.visible()
            .iter()
            .flat_map(|group| group.rows.iter())
            .map(|(row, _)| format_age(Some(row.state_since), now))
            .collect()
    }

    /// Keep the cursor on a visible row after the visible list changed.
    fn fix_cursor(&mut self, before: &[RowKey]) {
        let visible = self.visible_keys();
        if visible.is_empty() {
            self.cursor = None;
            return;
        }
        match &self.cursor {
            Some(cursor) if visible.contains(cursor) => {}
            Some(cursor) => {
                let index = before
                    .iter()
                    .position(|key| key == cursor)
                    .unwrap_or(usize::MAX)
                    .min(visible.len() - 1);
                self.cursor = Some(visible[index].clone());
            }
            None => self.cursor = Some(visible[0].clone()),
        }
    }

    fn open_peek(&mut self, key: RowKey) -> Vec<FleetViewEffect> {
        let finished = self
            .row(&key)
            .is_some_and(|row| matches!(row.state, RowState::Done | RowState::Failed));
        self.peek = Some((key.clone(), PeekStatus::Opening));
        let mut effects = vec![FleetViewEffect::OpenPeek(key.clone())];
        if finished {
            effects.push(FleetViewEffect::Acknowledge(key));
        }
        effects
    }

    /// `OpenPeek` for an action that needs the attachment, when the row is not peeked yet.
    fn ensure_peek(&mut self, key: &RowKey) -> Vec<FleetViewEffect> {
        if self.peek.as_ref().is_some_and(|(peeked, _)| peeked == key) {
            return Vec::new();
        }
        self.open_peek(key.clone())
    }

    fn running(&self, key: &RowKey) -> bool {
        self.row(key)
            .is_some_and(|row| matches!(row.state, RowState::Working | RowState::NeedsYou))
    }

    fn reply_prompt(&self) -> Option<String> {
        let key = self.target.as_ref()?;
        let draft = self.drafts.get(key).map(String::as_str).unwrap_or_default();
        Some(format!(
            "Reply to {}: {}",
            self.title_of(key),
            draft.replace('\n', "\u{21b5}")
        ))
    }

    fn refresh_reply_notice(&mut self) {
        self.notice = self.reply_prompt();
    }
}

const GROUP_ORDER: [RowState; 5] = [
    RowState::NeedsYou,
    RowState::Working,
    RowState::Failed,
    RowState::Done,
    RowState::Idle,
];

const GONE_FROM_FLEET: &str = "That session is no longer in the fleet.";
const NOTHING_TO_ANSWER: &str = "Nothing to answer in this session.";
const STOP_CANCELLED: &str = "Stop cancelled.";
const SENDING: &str = "Sending the last reply\u{2026}";

/// Cut to `limit` characters with an ellipsis.
fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(limit.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

fn reply_text(title: &str, outcome: &ReplyOutcome) -> String {
    match outcome {
        ReplyOutcome::Sent => format!("Sent to \"{title}\"."),
        ReplyOutcome::Queued => {
            format!("Queued for \"{title}\": it runs after the current turn.")
        }
        ReplyOutcome::AlreadyRan => format!("Hermes had already run this reply in \"{title}\"."),
        ReplyOutcome::StateUnknown => {
            format!("Hermes accepted the reply for \"{title}\" but reports its state as unknown.")
        }
        ReplyOutcome::VoiceStopped => {
            "Hermes took this as a voice stop phrase; nothing was queued.".to_owned()
        }
        ReplyOutcome::Refused { reason } if reason == "queue_full" => {
            format!("\"{title}\" already has 32 queued prompts. Wait for some to run.")
        }
        ReplyOutcome::Refused { reason } => format!("Hermes refused the reply ({reason})."),
        ReplyOutcome::NotAttached { reason } => {
            format!("Did not send the reply: could not open \"{title}\" ({reason}).")
        }
        ReplyOutcome::Unknown { why } => format!(
            "The reply may not have been sent ({why}). Press r, then enter, to send it again; Hermes will not queue it twice."
        ),
    }
}

fn stop_text(title: &str, outcome: &StopOutcome) -> String {
    match outcome {
        StopOutcome::Stopping => format!("Stopping \"{title}\"\u{2026}"),
        StopOutcome::NothingRunning => format!("Nothing is running in \"{title}\"."),
        StopOutcome::Refused { reason } if reason == "stale_generation" => {
            "Hermes did not stop the turn (stale_generation). If it is still running, press x, then x again.".to_owned()
        }
        StopOutcome::Refused { reason } => format!("Hermes did not stop the turn ({reason})."),
        StopOutcome::NotAttached { reason } => {
            format!("Did not stop \"{title}\": could not open the session ({reason}).")
        }
        StopOutcome::Unknown { why } => format!(
            "The stop may not have reached Hermes ({why}). If the turn is still running, press x again."
        ),
    }
}

fn confirm_text(title: &str) -> String {
    format!(
        "Stop \"{title}\"? Its running turn and background subagents stop and its open approvals are denied; queued prompts still run. Press x again to stop, any other key to keep it running."
    )
}

/// Inputs that are keys the operator pressed, as opposed to results and snapshots.
fn is_key(input: &FleetViewInput) -> bool {
    matches!(
        input,
        FleetViewInput::Action(_)
            | FleetViewInput::Text(_)
            | FleetViewInput::Paste(_)
            | FleetViewInput::Backspace
            | FleetViewInput::Left
            | FleetViewInput::Right
            | FleetViewInput::Home
            | FleetViewInput::End
            | FleetViewInput::Enter
            | FleetViewInput::Escape
            | FleetViewInput::CtrlC
            | FleetViewInput::AnyOtherKey
    )
}

/// Apply one input. `page` is how many rows PageUp and PageDown move.
pub fn update(
    state: &mut FleetViewState,
    input: FleetViewInput,
    page: usize,
) -> Vec<FleetViewEffect> {
    if is_key(&input) {
        state.dirty = true;
        return match state.focus {
            Focus::List => list_key(state, input, page),
            Focus::Filter => filter_key(state, input),
            Focus::Reply => reply_key(state, input),
            Focus::ConfirmStop => confirm_key(state, input),
            Focus::Answer => answer_key(state, input),
        };
    }
    match input {
        FleetViewInput::Snapshot {
            version,
            rows,
            queue,
            profiles,
            scope_note,
            link_note,
        } => snapshot(state, version, rows, queue, profiles, scope_note, link_note),
        FleetViewInput::Peek { key, status } => {
            match &state.peek {
                Some((peeked, held)) if *peeked == key => {
                    if *held != status {
                        state.peek = Some((key, status));
                        state.dirty = true;
                    }
                }
                Some(_) => {}
                // A peek the app reports without a Space of its own: the foreground row's.
                None => {
                    state.peek = Some((key, status));
                    state.dirty = true;
                }
            }
            Vec::new()
        }
        FleetViewInput::PeekClosed => {
            if state.peek.take().is_some() {
                state.dirty = true;
            }
            if state.focus == Focus::Answer {
                state.focus = Focus::List;
            }
            Vec::new()
        }
        FleetViewInput::ReplyDone { key, outcome } => {
            state.dirty = true;
            state.in_flight.remove(&key);
            match &outcome {
                ReplyOutcome::Refused { .. } | ReplyOutcome::NotAttached { .. } => {
                    if let Some(text) = state.last_sent.get(&key).cloned() {
                        state.drafts.insert(key.clone(), text);
                    }
                    if matches!(outcome, ReplyOutcome::Refused { .. }) {
                        state.unconfirmed.remove(&key);
                    }
                }
                ReplyOutcome::Unknown { why } => {
                    state.unconfirmed.insert(key.clone(), why.clone());
                }
                _ => {
                    state.unconfirmed.remove(&key);
                }
            }
            state.notice = Some(reply_text(&state.title_of(&key), &outcome));
            Vec::new()
        }
        FleetViewInput::StopDone { key, outcome } => {
            state.dirty = true;
            state.stopping.remove(&key);
            state.notice = Some(stop_text(&state.title_of(&key), &outcome));
            Vec::new()
        }
        FleetViewInput::AnswerFocusEnded => {
            if state.focus == Focus::Answer {
                state.focus = Focus::List;
                state.dirty = true;
            }
            Vec::new()
        }
        FleetViewInput::Notice(text) => {
            state.notice = Some(text);
            state.dirty = true;
            Vec::new()
        }
        FleetViewInput::Resize { cols, rows } => {
            if state.size != Some((cols, rows)) {
                state.size = Some((cols, rows));
                state.dirty = true;
            }
            Vec::new()
        }
        FleetViewInput::Tick { now } => {
            if state.ages(state.now) != state.ages(now) {
                state.dirty = true;
            }
            state.now = now;
            Vec::new()
        }
        // Keys were handled above.
        _ => Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn snapshot(
    state: &mut FleetViewState,
    version: u64,
    rows: Vec<FleetRow>,
    queue: Vec<NeedItem>,
    profiles: Vec<(String, ProfileStatus)>,
    scope_note: Option<String>,
    link_note: Option<String>,
) -> Vec<FleetViewEffect> {
    let first = !state.seen_snapshot;
    state.seen_snapshot = true;
    let changed = first
        || state.rows != rows
        || state.queue != queue
        || state.profiles != profiles
        || state.scope_note != scope_note
        || state.link_note != link_note;
    let before = state.visible_keys();
    for row in &rows {
        state.titles.insert(row.key.clone(), title_or_untitled(row));
    }
    state.version = version;
    state.rows = rows;
    state.queue = queue;
    state.profiles = profiles;
    state.scope_note = scope_note;
    state.link_note = link_note;
    if let ProfileFilter::One(profile) = &state.profile_filter
        && !state.profiles.iter().any(|(name, _)| name == profile)
    {
        state.profile_filter = ProfileFilter::All;
    }
    if changed && version != state.drawn_version {
        state.dirty = true;
    }
    let mut effects = Vec::new();
    if first {
        let held = state.held_open_at.take();
        let visible = state.visible();
        let needs = visible
            .iter()
            .find(|group| group.state == RowState::NeedsYou)
            .and_then(|group| group.rows.first())
            .or_else(|| visible.first().and_then(|group| group.rows.first()))
            .map(|(row, _)| row.key.clone());
        drop(visible);
        state.cursor = needs;
        if let Some(key) = held {
            effects = state.open_at(key);
        }
    } else {
        state.fix_cursor(&before);
    }
    effects
}

fn list_key(
    state: &mut FleetViewState,
    input: FleetViewInput,
    page: usize,
) -> Vec<FleetViewEffect> {
    state.notice = None;
    let action = match input {
        FleetViewInput::Action(action) => action,
        FleetViewInput::Enter => FleetAction::Open,
        FleetViewInput::Escape => return escape_list(state),
        FleetViewInput::CtrlC => return close_all(state),
        _ => return Vec::new(),
    };
    let visible = state.visible_keys();
    let at = state
        .cursor
        .as_ref()
        .and_then(|cursor| visible.iter().position(|key| key == cursor));
    let mut move_to = |index: usize| {
        if let Some(key) = visible.get(index) {
            state.cursor = Some(key.clone());
        }
    };
    let last = visible.len().saturating_sub(1);
    match action {
        FleetAction::Up => move_to(at.map_or(0, |at| at.saturating_sub(1))),
        FleetAction::Down => move_to(at.map_or(0, |at| (at + 1).min(last))),
        FleetAction::PageUp => move_to(at.map_or(0, |at| at.saturating_sub(page.max(1)))),
        FleetAction::PageDown => move_to(at.map_or(0, |at| (at + page.max(1)).min(last))),
        FleetAction::Top => move_to(0),
        FleetAction::Bottom => move_to(last),
        FleetAction::Close => return close_all(state),
        FleetAction::Filter => {
            state.focus = Focus::Filter;
        }
        FleetAction::ProfileNext | FleetAction::ProfilePrev => {
            cycle_profile(state, action == FleetAction::ProfileNext, &visible);
        }
        FleetAction::Peek => return peek_key(state),
        FleetAction::Open => return open_key(state),
        FleetAction::Reply => return reply_start(state),
        FleetAction::Stop => return stop_start(state),
        FleetAction::Answer => return answer_start(state),
        // The app opens the launch dialog; the overlay itself does nothing.
        FleetAction::New | FleetAction::NewInWorktree => {}
        // Step 0: reviewing a row does nothing yet.
        FleetAction::Review => {}
    }
    Vec::new()
}

fn escape_list(state: &mut FleetViewState) -> Vec<FleetViewEffect> {
    if !state.filter.is_empty() {
        let before = state.visible_keys();
        state.filter.clear();
        state.fix_cursor(&before);
        return Vec::new();
    }
    if state.peek.take().is_some() {
        return vec![FleetViewEffect::ClosePeek];
    }
    vec![FleetViewEffect::Close]
}

fn close_all(state: &mut FleetViewState) -> Vec<FleetViewEffect> {
    let mut effects = Vec::new();
    if state.peek.take().is_some() {
        effects.push(FleetViewEffect::ClosePeek);
    }
    effects.push(FleetViewEffect::Close);
    effects
}

fn cycle_profile(state: &mut FleetViewState, forward: bool, before: &[RowKey]) {
    let names: Vec<String> = state
        .profiles
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    // All, then each profile, then All again.
    let position = match &state.profile_filter {
        ProfileFilter::All => 0,
        ProfileFilter::One(profile) => names
            .iter()
            .position(|name| name == profile)
            .map_or(0, |index| index + 1),
    };
    let count = names.len() + 1;
    let next = if forward {
        (position + 1) % count
    } else {
        (position + count - 1) % count
    };
    state.profile_filter = if next == 0 {
        ProfileFilter::All
    } else {
        ProfileFilter::One(names[next - 1].clone())
    };
    state.fix_cursor(before);
}

fn peek_key(state: &mut FleetViewState) -> Vec<FleetViewEffect> {
    let Some(key) = state.cursor.clone() else {
        return Vec::new();
    };
    if state
        .peek
        .as_ref()
        .is_some_and(|(peeked, _)| *peeked == key)
    {
        state.peek = None;
        return vec![FleetViewEffect::ClosePeek];
    }
    state.open_peek(key)
}

fn open_key(state: &mut FleetViewState) -> Vec<FleetViewEffect> {
    let Some(key) = state.cursor.clone() else {
        return Vec::new();
    };
    let finished = state
        .row(&key)
        .is_some_and(|row| matches!(row.state, RowState::Done | RowState::Failed));
    let mut effects = Vec::new();
    if finished {
        effects.push(FleetViewEffect::Acknowledge(key.clone()));
    }
    effects.push(FleetViewEffect::Open(key));
    effects
}

fn reply_start(state: &mut FleetViewState) -> Vec<FleetViewEffect> {
    let Some(key) = state.cursor.clone() else {
        return Vec::new();
    };
    if state.in_flight.contains(&key) {
        state.notice = Some(SENDING.to_owned());
        return Vec::new();
    }
    let effects = state.ensure_peek(&key);
    state.caret = state.drafts.get(&key).map_or(0, String::len);
    state.target = Some(key);
    state.focus = Focus::Reply;
    state.refresh_reply_notice();
    effects
}

fn reply_key(state: &mut FleetViewState, input: FleetViewInput) -> Vec<FleetViewEffect> {
    let Some(key) = state.target.clone() else {
        state.focus = Focus::List;
        return Vec::new();
    };
    let mut draft = state.drafts.remove(&key).unwrap_or_default();
    let mut caret = state.caret.min(draft.len());
    let mut effects = Vec::new();
    let mut keep_editing = true;
    match input {
        FleetViewInput::Text(c) => {
            draft.insert(caret, c);
            caret += c.len_utf8();
        }
        FleetViewInput::Paste(text) => {
            draft.insert_str(caret, &text);
            caret += text.len();
        }
        FleetViewInput::Backspace => {
            if let Some((at, _)) = draft[..caret].char_indices().next_back() {
                draft.remove(at);
                caret = at;
            }
        }
        FleetViewInput::Left => {
            caret = draft[..caret]
                .char_indices()
                .next_back()
                .map_or(0, |(at, _)| at);
        }
        FleetViewInput::Right => {
            caret = draft[caret..]
                .chars()
                .next()
                .map_or(caret, |c| caret + c.len_utf8());
        }
        FleetViewInput::Home => caret = 0,
        FleetViewInput::End => caret = draft.len(),
        FleetViewInput::Escape | FleetViewInput::CtrlC => {
            keep_editing = false;
            state.notice = None;
        }
        FleetViewInput::Enter => {
            let blank = draft.trim().is_empty();
            match (blank, state.unconfirmed.contains_key(&key)) {
                (true, false) => {}
                (true, true) => {
                    state.in_flight.insert(key.clone());
                    effects.push(FleetViewEffect::ResendReply(key.clone()));
                    draft.clear();
                    keep_editing = false;
                    state.notice = None;
                }
                (false, true) => {
                    keep_editing = false;
                    state.notice = Some(format!(
                        "The last reply to \"{}\" may not have been sent. Clear this line and press enter to resend it.",
                        state.title_of(&key)
                    ));
                }
                (false, false) => {
                    state.in_flight.insert(key.clone());
                    state.last_sent.insert(key.clone(), draft.clone());
                    effects.push(FleetViewEffect::SendReply {
                        key: key.clone(),
                        text: std::mem::take(&mut draft),
                    });
                    keep_editing = false;
                    state.notice = None;
                }
            }
        }
        // Actions and other keys do nothing while the editor has the keys.
        _ => {}
    }
    if !draft.is_empty() {
        state.drafts.insert(key.clone(), draft);
    }
    state.caret = caret;
    if keep_editing {
        state.refresh_reply_notice();
    } else {
        state.focus = Focus::List;
    }
    effects
}

fn filter_key(state: &mut FleetViewState, input: FleetViewInput) -> Vec<FleetViewEffect> {
    let before = state.visible_keys();
    match input {
        FleetViewInput::Text(c) if !c.is_control() => state.filter.push(c),
        FleetViewInput::Paste(text) => {
            state
                .filter
                .extend(text.chars().filter(|c| !c.is_control()));
        }
        FleetViewInput::Backspace => {
            state.filter.pop();
        }
        FleetViewInput::Enter => state.focus = Focus::List,
        FleetViewInput::Escape | FleetViewInput::CtrlC => {
            state.filter.clear();
            state.focus = Focus::List;
        }
        _ => {}
    }
    state.fix_cursor(&before);
    if state.visible_keys().is_empty() && !state.filter.is_empty() && !state.rows.is_empty() {
        state.notice = Some(format!("No sessions match \"{}\".", state.filter));
    } else if state.focus == Focus::Filter
        || state
            .notice
            .as_deref()
            .is_some_and(|text| text.starts_with("No sessions match"))
    {
        state.notice = None;
    }
    Vec::new()
}

fn stop_start(state: &mut FleetViewState) -> Vec<FleetViewEffect> {
    let Some(key) = state.cursor.clone() else {
        return Vec::new();
    };
    let title = state.title_of(&key);
    if state.stopping.contains(&key) {
        state.notice = Some(stop_text(&title, &StopOutcome::Stopping));
        return Vec::new();
    }
    if !state.running(&key) {
        state.notice = Some(stop_text(&title, &StopOutcome::NothingRunning));
        return Vec::new();
    }
    state.target = Some(key);
    state.focus = Focus::ConfirmStop;
    state.notice = Some(confirm_text(&title));
    Vec::new()
}

fn confirm_key(state: &mut FleetViewState, input: FleetViewInput) -> Vec<FleetViewEffect> {
    state.focus = Focus::List;
    let Some(key) = state.target.clone() else {
        return Vec::new();
    };
    if input != FleetViewInput::Action(FleetAction::Stop) {
        state.notice = Some(STOP_CANCELLED.to_owned());
        return Vec::new();
    }
    let title = state.title_of(&key);
    // The turn may have ended between the two presses.
    if !state.running(&key) {
        state.notice = Some(stop_text(&title, &StopOutcome::NothingRunning));
        return Vec::new();
    }
    let mut effects = state.ensure_peek(&key);
    effects.push(FleetViewEffect::Stop(key.clone()));
    state.stopping.insert(key);
    state.notice = Some(stop_text(&title, &StopOutcome::Stopping));
    effects
}

fn answer_start(state: &mut FleetViewState) -> Vec<FleetViewEffect> {
    let card = match &state.peek {
        Some((
            key,
            PeekStatus::Open {
                has_open_prompt: true,
                ..
            },
        )) => Some(key.clone()),
        _ => None,
    };
    let Some(key) = card else {
        state.notice = Some(NOTHING_TO_ANSWER.to_owned());
        return Vec::new();
    };
    if state.is_foreground(&key) {
        // The foreground card takes the keys once the overlay is gone.
        return vec![FleetViewEffect::Close];
    }
    state.focus = Focus::Answer;
    vec![FleetViewEffect::FocusAnswer]
}

fn answer_key(state: &mut FleetViewState, input: FleetViewInput) -> Vec<FleetViewEffect> {
    // Every key but Ctrl+C belongs to the card; the app forwards it there.
    if input == FleetViewInput::CtrlC {
        state.focus = Focus::List;
    }
    Vec::new()
}
