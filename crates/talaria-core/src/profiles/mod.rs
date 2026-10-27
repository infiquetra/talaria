//! The profiles screen state: listing, creating and configuring profiles. Issue #253.

pub mod diff;
pub mod names;
pub mod roster;
pub mod text;

pub use diff::{SoulDiff, line_diff};
pub use names::{NameError, validate_new_name};
pub use roster::{
    GatewayView, HostTopology, ProfileRow, RosterEntry, classify, join_roster,
    normalize_profile_name,
};
pub use text::{Explanation, explain};

/// Whether the gateway serves a profile, and why not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServeState {
    /// Served by this gateway.
    Served,
    /// The gateway could not serve it.
    Parked {
        /// The scrubbed reason.
        reason: String,
    },
    /// Stopped by the operator.
    Stopped,
    /// Running its own gateway.
    OwnGateway,
    /// The gateway serves only its own profile.
    HostSingle {
        /// The scrubbed reason.
        reason: String,
    },
    /// Created moments ago by this process.
    Starting,
    /// The gateway never discovered it.
    NeverDiscovered,
    /// Discovery failed; nothing was classified.
    Unknown,
}

/// One writable profile section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    /// The description.
    Description,
    /// The soul.
    Soul,
}

/// Why a profile call did not return a decoded result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallFailure {
    /// The outcome is unknown: a timeout or a drop after sending.
    Unknown,
    /// Nothing was sent.
    NotSent,
    /// The reply did not decode.
    Unreadable,
    /// Hermes refused the call.
    Refused {
        /// The error code.
        code: i64,
        /// The error message.
        message: String,
        /// The `data.reason` word, when present.
        reason: Option<String>,
    },
}

/// A described profile. It holds no path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Description {
    /// The profile name.
    pub name: String,
    /// The description.
    pub description: String,
    /// The soul text.
    pub soul: String,
    /// The model provider.
    pub model_provider: String,
    /// The default model.
    pub model_default: String,
    /// `(name, enabled)` skills.
    pub skills: Vec<(String, bool)>,
    /// `(label, enabled)` toolsets.
    pub toolsets: Vec<(String, bool)>,
    /// Whether the toolset choice is explicit.
    pub toolsets_pinned: bool,
    /// `(name, enabled, transport)` Model Context Protocol servers.
    pub mcp_servers: Vec<(String, bool, String)>,
}

/// A profile creation request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateRequest {
    /// The normalised new name.
    pub name: String,
    /// The description, when set.
    pub description: Option<String>,
    /// The clone source, when set.
    pub clone_from: Option<String>,
    /// Copy provider keys from the connected profile.
    pub mirror_credentials: bool,
}

/// A created profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedProfile {
    /// The new name.
    pub name: String,
    /// Whether provider keys were copied.
    pub keys_copied: bool,
}

/// A configure answer for the one section sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    /// Whether every sent section applied.
    pub ok: bool,
    /// The value for the one section sent; absent counts as false.
    pub applied: bool,
}

/// A rescan answer, read in precedence order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RescanSummary {
    /// A finished reconcile.
    Finished {
        /// Profiles added.
        added: usize,
        /// Profiles parked by this reconcile.
        parked: usize,
        /// Profiles removed.
        removed: usize,
    },
    /// The gateway is still rescanning.
    Pending,
    /// The gateway serves one profile only.
    SingleProfile,
}

/// Why a rescan did not return a summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RescanFailure {
    /// The gateway predates the verb.
    UnknownVerb,
    /// No answer within the deadline.
    Timeout,
    /// Any other failure, with the words for the notice.
    Other(String),
}

/// What the raw editor returned. No cleaning, no trimming.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RawEdit {
    /// The text changed; the exact new text.
    Changed(String),
    /// The text is byte-equal to what was opened.
    Unchanged,
    /// The edit failed; nothing was sent.
    Failed(String),
}

/// A key chord turned into a profiles action, plus text-field keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// Move the selection up.
    Up,
    /// Move the selection down.
    Down,
    /// Move the selection up by a page.
    PageUp,
    /// Move the selection down by a page.
    PageDown,
    /// Move the selection to the first row.
    First,
    /// Move the selection to the last row.
    Last,
    /// Describe now, without the delay. The key layer sends this in wide layouts.
    Open,
    /// Open the detail panel. The key layer sends this in narrow layouts.
    OpenDetail,
    /// Filter by name, display name or model.
    Filter,
    /// Refresh every source.
    Refresh,
    /// Rescan, after a confirmation.
    Rescan,
    /// Open the create form.
    New,
    /// Edit the description.
    EditDescription,
    /// Edit the soul.
    EditSoul,
    /// Undo the innermost open thing, or close.
    Back,
    /// Insert a character in a text field.
    Char(char),
    /// Delete before the cursor.
    Backspace,
    /// Move the cursor left, or cycle a choice back.
    Left,
    /// Move the cursor right, or cycle a choice forward.
    Right,
    /// Move the cursor to the start.
    Home,
    /// Move the cursor to the end.
    End,
    /// Move to the next field.
    Tab,
    /// Move to the previous field.
    BackTab,
    /// Toggle the focused switch.
    Space,
    /// Advance the focused field or panel.
    Enter,
    /// Review from any form field.
    CtrlEnter,
    /// Clear the focused field.
    ClearLine,
}

/// Why discovery failed, in the gateway's own words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryFailure {
    /// The discovery error message.
    pub message: String,
    /// The discovery next step.
    pub next_step: String,
}

/// One input to the profiles state machine.
#[derive(Clone, Debug)]
pub enum ProfilesInput {
    /// Open the overlay.
    Open,
    /// Refresh every source.
    Refresh,
    /// Discovery answered.
    Discovered(u64, Result<GatewayView, DiscoveryFailure>),
    /// The roster call answered.
    Listed(u64, Result<Vec<RosterEntry>, String>),
    /// The topology call answered.
    Topology(u64, Result<HostTopology, String>),
    /// A describe call answered.
    Described(u64, String, Result<Description, CallFailure>),
    /// A key was pressed.
    Key(KeyAction),
    /// A create call answered.
    CreateAnswered(Result<CreatedProfile, CallFailure>),
    /// A configure call answered.
    ConfigureAnswered(String, Section, Result<Applied, CallFailure>),
    /// A rescan answered.
    RescanAnswered(Result<RescanSummary, RescanFailure>),
    /// The raw editor returned.
    EditorReturned(RawEdit),
    /// A scheduled tick fired.
    Tick {
        /// The token the schedule gave.
        token: u64,
    },
    /// The overlay closed.
    Closed,
}

/// One effect the profiles state machine asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum ProfilesEffect {
    /// Discover the gateway, never starting one.
    Discover {
        /// The refresh generation.
        generation: u64,
    },
    /// List the roster.
    List {
        /// The refresh generation.
        generation: u64,
    },
    /// Read the host topology.
    Topology {
        /// The refresh generation.
        generation: u64,
    },
    /// Describe one profile.
    Describe {
        /// The refresh generation.
        generation: u64,
        /// The profile name.
        name: String,
    },
    /// Create one profile.
    Create(CreateRequest),
    /// Configure one section of one profile.
    Configure {
        /// The profile name.
        name: String,
        /// The section to write.
        section: Section,
        /// The exact text to send.
        value: String,
    },
    /// Ask the gateway to rescan.
    Rescan,
    /// Open the raw editor on this exact text.
    OpenEditor(String),
    /// Deliver a tick after this delay.
    ScheduleTick {
        /// Milliseconds to wait.
        after_ms: u64,
        /// The token the tick carries back.
        token: u64,
    },
    /// Cancel every scheduled tick.
    CancelTicks,
    /// Show a notice while closed.
    Notice(String),
    /// Draw the overlay again.
    Redraw,
    /// Close the overlay: nothing was open to undo.
    Close,
}

/// One list row the screen draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowView {
    /// The name cell.
    pub name_cell: String,
    /// The state label.
    pub label: String,
    /// The label tone.
    pub tone: RowTone,
    /// The model cell.
    pub model_cell: String,
    /// The short next step.
    pub next_step: String,
}

/// The tone of a row label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowTone {
    /// A settled state.
    Muted,
    /// A state that needs attention.
    Warning,
}

/// The detail of one profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DetailView {
    /// `<name> · <label>`.
    pub title: String,
    /// The wrapped state text.
    pub state_text: String,
    /// The detail lines in draw order.
    pub lines: Vec<String>,
}

/// Which create-form field has focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormField {
    /// The name field.
    Name,
    /// The description field.
    Description,
    /// The start-from choice.
    StartFrom,
    /// The copy-keys switch.
    CopyKeys,
}

/// The create form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormView {
    /// The typed name.
    pub name: String,
    /// The name cursor, in characters.
    pub name_cursor: usize,
    /// The typed description.
    pub description: String,
    /// The description cursor, in characters.
    pub description_cursor: usize,
    /// The start-from choices in order.
    pub start_from: Vec<String>,
    /// The selected choice.
    pub start_index: usize,
    /// Whether provider keys are copied.
    pub copy_keys: bool,
    /// The profile the keys come from.
    pub copy_from: String,
    /// The focused field.
    pub focus: FormField,
    /// The name error under the field, when any.
    pub name_error: Option<String>,
}

/// The panel the overlay shows instead of the detail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Panel {
    /// The wide-layout detail.
    Detail(DetailView),
    /// The create form.
    Form(FormView),
    /// A review, one line per row.
    Review(Vec<String>),
    /// The one-line description input.
    DescriptionInput {
        /// The profile name.
        name: String,
        /// The input text.
        text: String,
        /// The cursor, in characters.
        cursor: usize,
        /// The error under the input, when any.
        error: Option<String>,
    },
    /// The narrow-layout detail.
    NarrowDetail(DetailView),
    /// No panel.
    None,
}

/// Everything the profiles screen draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfilesView {
    /// The first load has not completed.
    pub loading: bool,
    /// The header line.
    pub header: String,
    /// The list rows.
    pub rows: Vec<RowView>,
    /// The selected row.
    pub selected: Option<usize>,
    /// The active filter text.
    pub filter: Option<String>,
    /// The open panel.
    pub panel: Panel,
    /// The shown notice.
    pub notice: Option<String>,
    /// The shown confirmation.
    pub confirm: Option<String>,
}

/// The profiles screen state. The builder owns the fields.
#[derive(Debug)]
#[allow(dead_code)]
pub struct ProfilesState {
    connected_profile: String,
}

impl ProfilesState {
    /// Track profiles for one connected profile.
    #[must_use]
    pub fn new(connected_profile: String) -> Self {
        Self { connected_profile }
    }

    /// Apply one input at one monotonic clock reading.
    pub fn update(&mut self, _input: ProfilesInput, _now_ms: u64) -> Vec<ProfilesEffect> {
        todo!("profiles stub")
    }

    /// Everything the screen draws.
    #[must_use]
    pub fn view(&self) -> ProfilesView {
        todo!("profiles stub")
    }
}
