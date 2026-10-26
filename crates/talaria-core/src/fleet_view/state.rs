//! Overlay reducer. A snapshot is stored so a screen can be built; every other input and every
//! query still panics in this stub.

use crate::directory::rows::RowKey;
use crate::fleet::{FleetRow, NeedItem, ProfileStatus, RowState};

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
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct FleetViewState {
    rows: Vec<FleetRow>,
    queue: Vec<NeedItem>,
    profiles: Vec<(String, ProfileStatus)>,
    scope_note: Option<String>,
    link_note: Option<String>,
    version: u64,
    foreground: Option<RowKey>,
    now: f64,
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
            foreground,
            now,
        }
    }

    #[must_use]
    pub fn focus(&self) -> &Focus {
        todo!("fleet view stub")
    }

    #[must_use]
    pub fn visible(&self) -> Vec<VisibleGroup<'_>> {
        todo!("fleet view stub")
    }

    #[must_use]
    pub fn cursor(&self) -> Option<&RowKey> {
        todo!("fleet view stub")
    }

    #[must_use]
    pub fn peek(&self) -> Option<(&RowKey, &PeekStatus)> {
        todo!("fleet view stub")
    }

    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        todo!("fleet view stub")
    }

    #[must_use]
    pub fn reply_line(&self) -> Option<(&RowKey, &str)> {
        todo!("fleet view stub")
    }

    #[must_use]
    pub fn needs_redraw(&self) -> bool {
        todo!("fleet view stub")
    }

    pub fn take_frame(&mut self) {
        todo!("fleet view stub")
    }

    #[must_use]
    pub fn row_flags(&self, key: &RowKey) -> RowFlags {
        let _ = key;
        todo!("fleet view stub")
    }

    /// Rows whose reply is in flight or unconfirmed: key, sanitized title, and `no reply` or
    /// `connection dropped`. A reply still in flight counts as `no reply`.
    #[must_use]
    pub fn unsettled_replies(&self) -> Vec<(RowKey, String, String)> {
        todo!("fleet view stub")
    }

    /// Put the cursor on `key` and apply the same rules as Space.
    ///
    /// Before the first snapshot this holds the key and emits nothing. A key that is not in
    /// the snapshot sets the notice and emits nothing.
    pub fn open_at(&mut self, key: RowKey) -> Vec<FleetViewEffect> {
        let _ = key;
        todo!("fleet view stub")
    }
}

/// Apply one input. `page` is how many rows PageUp and PageDown move.
///
/// A snapshot is remembered and emits nothing. Every other input panics until the reducer
/// exists, so a test cannot pass by accident.
pub fn update(
    state: &mut FleetViewState,
    input: FleetViewInput,
    page: usize,
) -> Vec<FleetViewEffect> {
    let _ = page;
    match input {
        FleetViewInput::Snapshot {
            version,
            rows,
            queue,
            profiles,
            scope_note,
            link_note,
        } => {
            state.version = version;
            state.rows = rows;
            state.queue = queue;
            state.profiles = profiles;
            state.scope_note = scope_note;
            state.link_note = link_note;
            Vec::new()
        }
        other => {
            let _ = other;
            todo!("fleet view stub")
        }
    }
}
