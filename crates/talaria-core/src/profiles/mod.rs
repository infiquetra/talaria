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
    /// Describe the selected row now.
    Open,
    /// Open the narrow detail panel.
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
    /// An unbound key dismisses the shown notice without changing a text field.
    UnboundKey,
    /// Insert pasted text into the focused single-line field.
    Paste(String),
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
    /// Close the overlay when there is no inner panel to undo.
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
pub struct ProfilesState {
    open: bool,
    connected: String,
    generation: u64,
    started: bool,
    loaded: bool,
    load: Option<Load>,
    roster: Option<Vec<RosterEntry>>,
    gateway: GatewayView,
    discovery_failure: Option<DiscoveryFailure>,
    topology: Option<HostTopology>,
    rows: Vec<ProfileRow>,
    selected: Option<String>,
    descriptions: std::collections::BTreeMap<String, Result<Description, CallFailure>>,
    requested: std::collections::BTreeSet<String>,
    in_flight: std::collections::BTreeSet<String>,
    fresh_after: std::collections::BTreeSet<String>,
    created: Vec<(String, u64)>,
    kept: std::collections::BTreeMap<String, String>,
    notices: std::collections::VecDeque<String>,
    filter: Option<TextInput>,
    filter_typing: bool,
    panel: EditPanel,
    detail_visible: bool,
    narrow: bool,
    narrow_detail: bool,
    page_size: usize,
    confirm: bool,
    creating: Option<CreateRequest>,
    create_form: Option<FormView>,
    configuring: Option<Review>,
    rescanning: bool,
    reading: Option<(String, ReadPurpose)>,
    editing: Option<SoulEdit>,
    next_token: u64,
    timers: std::collections::BTreeMap<u64, Timer>,
}

const NOT_SENT: &str = "Not sent: Talaria is not connected to the gateway. Nothing changed.";

fn cut_words(text: &str) -> String {
    text.chars().take(300).collect()
}

fn describe_error(error: &CallFailure) -> String {
    match error {
        CallFailure::Refused { code: -32601, .. } => {
            "This gateway does not let Talaria read profile details over its socket.".into()
        }
        CallFailure::Refused { message, .. } => format!("Hermes refused: {}", cut_words(message)),
        CallFailure::Unknown => "Could not read this profile: no answer.".into(),
        CallFailure::NotSent => "Could not read this profile: not connected.".into(),
        CallFailure::Unreadable => "Could not read this profile: unreadable reply.".into(),
    }
}

fn one_line(text: &str) -> String {
    let mut result = String::new();
    let mut newline = false;
    for ch in text.chars() {
        if matches!(ch, '\r' | '\n') {
            if !newline {
                result.push(' ');
            }
            newline = true;
        } else {
            result.push(ch);
            newline = false;
        }
    }
    result
}

#[derive(Debug, Default)]
struct Load {
    discovery: Option<Result<GatewayView, DiscoveryFailure>>,
    roster: Option<Result<Vec<RosterEntry>, String>>,
    topology: Option<Result<HostTopology, String>>,
}

#[derive(Clone, Debug, Default)]
struct TextInput {
    text: String,
    cursor: usize,
}

impl TextInput {
    fn insert_bounded(&mut self, text: &str, max_chars: usize) {
        let remaining = max_chars.saturating_sub(self.text.chars().count());
        let text: String = text.chars().take(remaining).collect();
        self.insert(&text);
    }

    fn insert(&mut self, text: &str) {
        let offset = self
            .text
            .char_indices()
            .nth(self.cursor)
            .map_or(self.text.len(), |(offset, _)| offset);
        self.text.insert_str(offset, text);
        self.cursor += text.chars().count();
    }
    fn new(text: String) -> Self {
        let cursor = text.chars().count();
        Self { text, cursor }
    }
    fn key(&mut self, key: &KeyAction) {
        let mut chars: Vec<char> = self.text.chars().collect();
        self.cursor = self.cursor.min(chars.len());
        match key {
            KeyAction::Char(ch) if !ch.is_control() => {
                chars.insert(self.cursor, *ch);
                self.cursor += 1;
            }
            KeyAction::Space => {
                chars.insert(self.cursor, ' ');
                self.cursor += 1;
            }
            KeyAction::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                chars.remove(self.cursor);
            }
            KeyAction::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyAction::Right => self.cursor = (self.cursor + 1).min(chars.len()),
            KeyAction::Home | KeyAction::First => self.cursor = 0,
            KeyAction::End | KeyAction::Last => self.cursor = chars.len(),
            KeyAction::ClearLine => {
                chars.clear();
                self.cursor = 0;
            }
            _ => {}
        }
        self.text = chars.into_iter().collect();
    }
}

#[derive(Clone, Debug)]
struct DescriptionEdit {
    name: String,
    base: String,
    input: TextInput,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct SoulEdit {
    name: String,
    base: String,
    opened: String,
    result: String,
}

#[derive(Clone, Debug)]
enum Review {
    Create(CreateRequest),
    Description(DescriptionEdit),
    Soul(SoulEdit),
}

#[derive(Debug, Default)]
enum EditPanel {
    #[default]
    None,
    Create(FormView),
    Description(DescriptionEdit),
    Review {
        review: Review,
        back: Box<EditPanel>,
    },
}

#[derive(Clone, Debug)]
enum ReadPurpose {
    Description,
    Soul,
    Guard(SoulEdit),
}

#[derive(Debug)]
enum Timer {
    Describe(String),
    Refresh,
    Expire,
}

impl Default for ProfilesState {
    fn default() -> Self {
        Self::new("default")
    }
}

impl ProfilesState {
    /// Create the process-lifetime state for the connected profile.
    #[must_use]
    pub fn new(connected_profile: impl Into<String>) -> Self {
        Self {
            open: false,
            connected: normalize_profile_name(&connected_profile.into()),
            generation: 0,
            started: false,
            loaded: false,
            load: None,
            roster: None,
            gateway: GatewayView::default(),
            discovery_failure: None,
            topology: None,
            rows: Vec::new(),
            selected: None,
            descriptions: Default::default(),
            requested: Default::default(),
            in_flight: Default::default(),
            fresh_after: Default::default(),
            created: Vec::new(),
            kept: Default::default(),
            notices: Default::default(),
            filter: None,
            filter_typing: false,
            panel: EditPanel::None,
            detail_visible: true,
            narrow: false,
            narrow_detail: false,
            page_size: 10,
            confirm: false,
            creating: None,
            create_form: None,
            configuring: None,
            rescanning: false,
            reading: None,
            editing: None,
            next_token: 0,
            timers: Default::default(),
        }
    }

    /// Whether the overlay is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Update the connected profile after a foreground connection changes.
    pub fn set_connected_profile(&mut self, profile: impl Into<String>) {
        self.connected = normalize_profile_name(&profile.into());
        if let EditPanel::Create(form) = &mut self.panel {
            form.copy_from = self.connected.clone();
        }
    }

    /// Whether fixed text editing keys take precedence over bound actions.
    #[must_use]
    pub fn text_focus(&self) -> bool {
        self.filter_typing
            || matches!(&self.panel, EditPanel::Description(_))
            || matches!(&self.panel, EditPanel::Create(form) if matches!(form.focus, FormField::Name | FormField::Description))
    }

    /// Set presentation facts used for navigation; no terminal type enters the core.
    pub fn set_layout(&mut self, width: u16, page_size: usize) {
        self.narrow = width < 100;
        self.page_size = page_size.max(1);
    }

    fn accept(&self, generation: u64) -> bool {
        self.open && generation == self.generation
    }

    fn close(&mut self) {
        self.open = false;
        self.load = None;
        self.reading = None;
        self.timers.clear();
    }

    fn refresh(&mut self, effects: &mut Vec<ProfilesEffect>) {
        if self.started {
            self.generation += 1;
        } else {
            self.started = true;
        }
        self.load = Some(Load::default());
        self.requested.clear();
        self.in_flight.clear();
        self.fresh_after.clear();
        self.reading = None;
        self.timers
            .retain(|_, timer| !matches!(timer, Timer::Describe(_)));
        effects.extend([
            ProfilesEffect::Discover {
                generation: self.generation,
            },
            ProfilesEffect::List {
                generation: self.generation,
            },
            ProfilesEffect::Topology {
                generation: self.generation,
            },
        ]);
    }

    fn finish_load(&mut self, now_ms: u64, effects: &mut Vec<ProfilesEffect>) {
        if !self.load.as_ref().is_some_and(|load| {
            load.discovery.is_some() && load.roster.is_some() && load.topology.is_some()
        }) {
            return;
        }
        let Some(Load {
            discovery: Some(discovery),
            roster: Some(roster),
            topology: Some(topology),
        }) = self.load.take()
        else {
            return;
        };
        let discovery_failed = discovery.is_err();
        match discovery {
            Ok(gateway) => {
                self.gateway = gateway;
                self.discovery_failure = None;
            }
            Err(failure) => {
                self.gateway = GatewayView::default();
                self.discovery_failure = Some(failure);
            }
        }
        self.roster = match roster {
            Ok(roster) => Some(roster),
            Err(words) => {
                if !discovery_failed {
                    self.notice(format!("Hermes did not return the profile list ({words}). Showing only what the gateway serves."), effects);
                }
                None
            }
        };
        self.topology = match topology {
            Ok(topology) => {
                if topology.own_gateway.is_none() && !discovery_failed {
                    self.notice("Some reasons are unavailable: the gateway did not say which profiles run their own gateway.".into(), effects);
                }
                Some(topology)
            }
            Err(words) => {
                if !discovery_failed {
                    self.notice(format!("Some reasons are unavailable: the gateway did not answer /api/status ({words})."), effects);
                }
                None
            }
        };
        self.descriptions.clear();
        let mut rows = join_roster(
            self.roster.as_deref(),
            &self.gateway,
            self.topology.as_ref(),
            &self.created,
            now_ms,
        );
        if self.discovery_failure.is_some() && self.roster.is_none() {
            for previous in &self.rows {
                if !rows.iter().any(|row| row.name == previous.name) {
                    rows.push(previous.clone());
                }
            }
        }
        self.rows = rows;
        if !self.loaded && self.rows.iter().any(|row| row.name == self.connected) {
            self.selected = Some(self.connected.clone());
        }
        self.loaded = true;
        self.reclassify(now_ms);
        self.ensure_selection();
        if let Some(name) = self.selected.clone() {
            self.describe(name, false, effects);
        }
    }

    fn reclassify(&mut self, now_ms: u64) {
        for row in &mut self.rows {
            row.state = if self.discovery_failure.is_some() {
                ServeState::Unknown
            } else {
                let at = self
                    .created
                    .iter()
                    .find(|(name, _)| name == &row.name)
                    .map(|(_, at)| *at);
                classify(&row.name, &self.gateway, self.topology.as_ref(), at, now_ms)
            };
        }
        self.rows.sort_by_key(|row| {
            (
                row.name != "default",
                row.state != ServeState::Served,
                row.name.clone(),
            )
        });
    }

    fn visible(&self) -> Vec<&ProfileRow> {
        let filter = self
            .filter
            .as_ref()
            .map(|filter| filter.text.to_lowercase())
            .unwrap_or_default();
        self.rows
            .iter()
            .filter(|row| {
                filter.is_empty()
                    || row.name.to_lowercase().contains(&filter)
                    || row.display_name.to_lowercase().contains(&filter)
                    || row
                        .model
                        .as_ref()
                        .is_some_and(|model| model.to_lowercase().contains(&filter))
            })
            .collect()
    }

    fn ensure_selection(&mut self) {
        let visible = self.visible();
        if !visible
            .iter()
            .any(|row| Some(&row.name) == self.selected.as_ref())
        {
            self.selected = visible.first().map(|row| row.name.clone());
        }
    }

    fn explanation(&self, row: &ProfileRow) -> Explanation {
        let mut explanation = explain(&row.state, &row.name);
        if row.state == ServeState::Unknown
            && let Some(failure) = &self.discovery_failure
        {
            explanation.text = failure.message.clone();
            explanation.next_step = failure.next_step.clone();
        }
        explanation
    }

    fn detail(&self, row: &ProfileRow) -> DetailView {
        let explanation = self.explanation(row);
        let mut lines = Vec::new();
        if !row.in_roster && !matches!(self.descriptions.get(&row.name), Some(Ok(_))) {
            lines.push("Hermes did not list this profile's details.".into());
        }
        match self.descriptions.get(&row.name) {
            Some(Ok(description)) => {
                lines.push(if description.model_default.is_empty() {
                    "Model: not set".into()
                } else {
                    format!(
                        "Model: {} · {}",
                        description.model_provider, description.model_default
                    )
                });
                lines.push(format!(
                    "Skills: {} of {} on",
                    description.skills.iter().filter(|(_, on)| *on).count(),
                    description.skills.len()
                ));
                lines.push(if !description.toolsets_pinned {
                    "Toolsets: default set".into()
                } else {
                    format!(
                        "Toolsets: {}",
                        description
                            .toolsets
                            .iter()
                            .filter(|(_, on)| *on)
                            .map(|(label, _)| label.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                });
                lines.push(if description.mcp_servers.is_empty() {
                    "MCP servers: none".into()
                } else {
                    format!(
                        "MCP servers: {}",
                        description
                            .mcp_servers
                            .iter()
                            .map(|(name, on, transport)| format!(
                                "{name} ({}, {transport})",
                                if *on { "on" } else { "off" }
                            ))
                            .collect::<Vec<_>>()
                            .join(" · ")
                    )
                });
                lines.push(if description.description.is_empty() {
                    "Description: none".into()
                } else {
                    format!("Description: {}", description.description)
                });
                lines.push("Soul:".into());
                lines.extend(description.soul.lines().take(6).map(String::from));
            }
            Some(Err(error)) => lines.push(describe_error(error)),
            None => lines.push("Model: reading...".into()),
        }
        DetailView {
            title: format!("{} · {}", row.name, explanation.label),
            state_text: explanation.text,
            lines,
        }
    }

    fn notice(&mut self, text: String, effects: &mut Vec<ProfilesEffect>) {
        if self.open {
            self.notices.push_back(text);
        } else {
            effects.push(ProfilesEffect::Notice(text));
        }
    }

    fn schedule(&mut self, after_ms: u64, timer: Timer, effects: &mut Vec<ProfilesEffect>) {
        self.next_token += 1;
        self.timers.insert(self.next_token, timer);
        effects.push(ProfilesEffect::ScheduleTick {
            after_ms,
            token: self.next_token,
        });
    }

    fn describe(&mut self, name: String, fresh: bool, effects: &mut Vec<ProfilesEffect>) {
        self.timers
            .retain(|_, timer| !matches!(timer, Timer::Describe(_)));
        if fresh && self.in_flight.contains(&name) {
            self.fresh_after.insert(name);
            return;
        }
        if fresh || self.requested.insert(name.clone()) {
            self.requested.insert(name.clone());
            self.in_flight.insert(name.clone());
            effects.push(ProfilesEffect::Describe {
                generation: self.generation,
                name,
            });
        }
    }

    fn described(
        &mut self,
        name: String,
        result: Result<Description, CallFailure>,
        effects: &mut Vec<ProfilesEffect>,
    ) {
        if self.reading.is_none() {
            self.detail_visible = true;
        }
        self.in_flight.remove(&name);
        if self.fresh_after.remove(&name) {
            self.describe(name, true, effects);
            return;
        }
        if let Ok(description) = &result
            && let Some(row) = self.rows.iter_mut().find(|row| row.name == name)
        {
            if !description.model_default.is_empty() {
                row.model = Some(description.model_default.clone());
            }
            if !description.model_provider.is_empty() {
                row.provider = Some(description.model_provider.clone());
            }
        }
        self.descriptions.insert(name.clone(), result.clone());
        let Some((reading_name, _)) = &self.reading else {
            return;
        };
        if reading_name != &name {
            return;
        }
        let Some((_, purpose)) = self.reading.take() else {
            return;
        };
        let description = match result {
            Ok(description) => description,
            Err(error) => {
                self.notice(describe_error(&error), effects);
                return;
            }
        };
        match purpose {
            ReadPurpose::Description => self.open_description(name, description.description),
            ReadPurpose::Soul => {
                let opened = self
                    .kept
                    .get(&name)
                    .cloned()
                    .unwrap_or_else(|| description.soul.clone());
                let max_len = opened.len().max(description.soul.len());
                if max_len > 262_144 {
                    self.notice(
                        format!(
                            "This soul file is too large to edit here ({} KiB).",
                            max_len / 1024
                        ),
                        effects,
                    );
                    return;
                }
                self.editing = Some(SoulEdit {
                    name,
                    base: description.soul,
                    opened: opened.clone(),
                    result: opened.clone(),
                });
                effects.push(ProfilesEffect::OpenEditor(opened));
            }
            ReadPurpose::Guard(edit) => {
                if description.soul == edit.base {
                    self.configuring = Some(Review::Soul(edit.clone()));
                    self.panel = EditPanel::None;
                    effects.push(ProfilesEffect::Configure {
                        name,
                        section: Section::Soul,
                        value: edit.result,
                    });
                } else {
                    self.kept.insert(name.clone(), edit.result);
                    self.panel = EditPanel::None;
                    self.notice(format!("The soul of {name} changed since you opened it, so nothing was sent. Your text is kept: press s to reopen it; the review then compares it with the new version."), effects);
                }
            }
        }
    }

    fn open_description(&mut self, name: String, description: String) {
        self.detail_visible = false;
        let base = one_line(&description);
        self.panel = EditPanel::Description(DescriptionEdit {
            name,
            base: base.clone(),
            input: TextInput::new(base),
            error: None,
        });
    }

    fn key(&mut self, key: KeyAction, effects: &mut Vec<ProfilesEffect>) {
        let key = if key == KeyAction::OpenDetail {
            if !self.confirm && matches!(self.panel, EditPanel::None) {
                self.narrow_detail = true;
            }
            KeyAction::Open
        } else {
            key
        };
        if self.confirm {
            self.confirm = false;
            if key == KeyAction::Char('y') {
                if self.rescanning {
                    self.notice("A rescan is already running.".into(), effects);
                } else {
                    self.rescanning = true;
                    effects.push(ProfilesEffect::Rescan);
                }
            }
            return;
        }
        if key == KeyAction::Rescan {
            if self.rescanning {
                self.notice("A rescan is already running.".into(), effects);
            } else {
                self.confirm = true;
            }
            return;
        }
        if key == KeyAction::Back {
            self.panel = match std::mem::take(&mut self.panel) {
                EditPanel::Review { back, .. } => {
                    self.detail_visible = false;
                    *back
                }
                EditPanel::Create(_) | EditPanel::Description(_) => {
                    self.detail_visible = false;
                    EditPanel::None
                }
                EditPanel::None => {
                    if self.narrow_detail {
                        self.narrow_detail = false;
                        self.detail_visible = true;
                    } else if self.filter.is_some() {
                        self.filter = None;
                        self.filter_typing = false;
                    } else {
                        self.close();
                        effects.push(ProfilesEffect::Close);
                    }
                    EditPanel::None
                }
            };
            return;
        }
        match std::mem::take(&mut self.panel) {
            EditPanel::Create(mut form) => {
                let review = self.form_key(&mut form, &key);
                self.panel = if let Some(request) = review {
                    EditPanel::Review {
                        review: Review::Create(request),
                        back: Box::new(EditPanel::Create(form)),
                    }
                } else {
                    EditPanel::Create(form)
                };
                return;
            }
            EditPanel::Description(mut edit) => {
                if matches!(
                    key,
                    KeyAction::Enter | KeyAction::CtrlEnter | KeyAction::Open
                ) {
                    let text = edit.input.text.trim().to_owned();
                    if text.chars().count() > 200 {
                        edit.error = Some("Use at most 200 characters.".into());
                        self.panel = EditPanel::Description(edit);
                    } else if text == edit.base.trim() {
                        self.notice("No change.".into(), effects);
                    } else {
                        edit.input = TextInput::new(text);
                        self.panel = EditPanel::Review {
                            review: Review::Description(edit.clone()),
                            back: Box::new(EditPanel::Description(edit)),
                        };
                    }
                } else {
                    edit.input.key(&key);
                    edit.error = None;
                    self.panel = EditPanel::Description(edit);
                }
                return;
            }
            EditPanel::Review { .. }
                if matches!(key, KeyAction::EditDescription | KeyAction::EditSoul) => {}
            EditPanel::Review { review, back } => {
                if matches!(
                    key,
                    KeyAction::Enter | KeyAction::CtrlEnter | KeyAction::Open
                ) {
                    match &review {
                        Review::Create(request) => {
                            if self.creating.is_some() {
                                self.notice("A create is already in progress.".into(), effects);
                                self.panel = EditPanel::Review { review, back };
                            } else {
                                if let EditPanel::Create(form) = *back {
                                    self.create_form = Some(form);
                                }
                                self.creating = Some(request.clone());
                                self.detail_visible = false;
                                effects.push(ProfilesEffect::Create(request.clone()));
                            }
                        }
                        Review::Description(edit) => {
                            if self.configuring.is_none() {
                                effects.push(ProfilesEffect::Configure {
                                    name: edit.name.clone(),
                                    section: Section::Description,
                                    value: edit.input.text.clone(),
                                });
                                self.configuring = Some(review);
                                self.detail_visible = false;
                            } else {
                                self.panel = EditPanel::Review { review, back };
                            }
                        }
                        Review::Soul(edit) => {
                            if self.configuring.is_none() && self.reading.is_none() {
                                self.reading =
                                    Some((edit.name.clone(), ReadPurpose::Guard(edit.clone())));
                                self.describe(edit.name.clone(), true, effects);
                            }
                            self.panel = EditPanel::Review { review, back };
                        }
                    }
                } else {
                    self.panel = EditPanel::Review { review, back };
                }
                return;
            }
            EditPanel::None => {}
        }
        if self.filter_typing {
            if matches!(key, KeyAction::Enter | KeyAction::Open) {
                self.filter_typing = false;
            } else if let Some(filter) = &mut self.filter {
                filter.key(&key);
            }
            self.ensure_selection();
            return;
        }
        match key {
            KeyAction::Refresh => self.refresh(effects),
            KeyAction::New => {
                if self.creating.is_some() {
                    self.notice("A create is already in progress.".into(), effects);
                }
                self.detail_visible = false;
                let mut choices = vec!["fresh".to_owned()];
                choices.extend(
                    self.rows
                        .iter()
                        .filter(|row| {
                            if self.roster.is_some() {
                                row.in_roster
                            } else {
                                row.state == ServeState::Served
                            }
                        })
                        .map(|row| row.name.clone()),
                );
                self.panel = EditPanel::Create(FormView {
                    name: String::new(),
                    name_cursor: 0,
                    description: String::new(),
                    description_cursor: 0,
                    start_from: choices,
                    start_index: 0,
                    copy_keys: true,
                    copy_from: self.connected.clone(),
                    focus: FormField::Name,
                    name_error: None,
                });
            }
            KeyAction::Filter => {
                self.filter.get_or_insert_with(TextInput::default);
                self.filter_typing = true;
            }
            KeyAction::EditDescription | KeyAction::EditSoul => {
                let Some(name) = self.selected.clone() else {
                    return;
                };
                self.detail_visible = false;
                if key == KeyAction::EditDescription {
                    if let Some(Ok(description)) = self.descriptions.get(&name) {
                        self.open_description(name, description.description.clone());
                    } else {
                        self.reading = Some((name.clone(), ReadPurpose::Description));
                        let fresh = !self.in_flight.contains(&name);
                        self.describe(name, fresh, effects);
                    }
                } else {
                    self.reading = Some((name.clone(), ReadPurpose::Soul));
                    self.describe(name, true, effects);
                }
            }
            KeyAction::Enter | KeyAction::Open => {
                self.detail_visible = true;
                if self.narrow {
                    self.narrow_detail = true;
                }
                if let Some(name) = self.selected.clone() {
                    self.describe(name, false, effects);
                }
            }
            KeyAction::Up
            | KeyAction::Down
            | KeyAction::PageUp
            | KeyAction::PageDown
            | KeyAction::First
            | KeyAction::Last
            | KeyAction::Home
            | KeyAction::End => {
                self.detail_visible = true;
                let visible = self.visible();
                if visible.is_empty() {
                    return;
                }
                let current = visible
                    .iter()
                    .position(|row| Some(&row.name) == self.selected.as_ref())
                    .unwrap_or(0);
                let last = visible.len() - 1;
                let index = match key {
                    KeyAction::Up => current.saturating_sub(1),
                    KeyAction::Down => (current + 1).min(last),
                    KeyAction::PageUp => current.saturating_sub(self.page_size),
                    KeyAction::PageDown => current.saturating_add(self.page_size).min(last),
                    KeyAction::First | KeyAction::Home => 0,
                    _ => last,
                };
                let name = visible[index].name.clone();
                if self.selected.as_ref() != Some(&name) {
                    self.selected = Some(name.clone());
                    self.timers
                        .retain(|_, timer| !matches!(timer, Timer::Describe(_)));
                    if !self.requested.contains(&name) {
                        self.schedule(150, Timer::Describe(name), effects);
                    }
                }
            }
            _ => {}
        }
    }

    fn form_key(&self, form: &mut FormView, key: &KeyAction) -> Option<CreateRequest> {
        if matches!(key, KeyAction::CtrlEnter)
            || matches!(key, KeyAction::Enter | KeyAction::Open)
                && matches!(form.focus, FormField::Name | FormField::CopyKeys)
        {
            let mut known: Vec<_> = self
                .roster
                .iter()
                .flatten()
                .map(|row| row.name.clone())
                .collect();
            known.extend(self.gateway.served.iter().cloned());
            known.extend(self.gateway.parked.iter().map(|(name, _)| name.clone()));
            if let Some(topology) = &self.topology {
                known.extend(topology.stopped.iter().cloned());
            }
            let name = match validate_new_name(&form.name, &known) {
                Ok(name) => {
                    form.name_error = None;
                    name
                }
                Err(error) => {
                    form.name_error = Some(error.to_string());
                    form.focus = FormField::Name;
                    return None;
                }
            };
            if *key != KeyAction::CtrlEnter && form.focus == FormField::Name {
                form.focus = FormField::Description;
                return None;
            }
            let description = form.description.trim();
            if description.chars().count() > 200 {
                form.name_error = Some("Use at most 200 characters.".into());
                form.focus = FormField::Description;
                return None;
            }
            return Some(CreateRequest {
                name,
                description: (!description.is_empty()).then(|| description.to_owned()),
                clone_from: (form.start_index != 0)
                    .then(|| form.start_from[form.start_index].clone()),
                mirror_credentials: form.copy_keys,
            });
        }
        if matches!(
            key,
            KeyAction::Tab | KeyAction::BackTab | KeyAction::Enter | KeyAction::Open
        ) {
            form.focus = match (form.focus, key == &KeyAction::BackTab) {
                (FormField::Name, false) | (FormField::StartFrom, true) => FormField::Description,
                (FormField::Description, false) | (FormField::CopyKeys, true) => {
                    FormField::StartFrom
                }
                (FormField::StartFrom, false) | (FormField::Name, true) => FormField::CopyKeys,
                _ => FormField::Name,
            };
            return None;
        }
        match form.focus {
            FormField::Name | FormField::Description => {
                let (text, cursor) = if form.focus == FormField::Name {
                    (&mut form.name, &mut form.name_cursor)
                } else {
                    (&mut form.description, &mut form.description_cursor)
                };
                let mut input = TextInput {
                    text: std::mem::take(text),
                    cursor: *cursor,
                };
                let at_limit =
                    form.focus == FormField::Description && input.text.chars().count() >= 200;
                if !at_limit || !matches!(key, KeyAction::Char(_) | KeyAction::Space) {
                    input.key(key);
                }
                *text = input.text;
                *cursor = input.cursor;
                form.name_error = None;
            }
            FormField::StartFrom => {
                if !form.start_from.is_empty() {
                    if *key == KeyAction::Right {
                        form.start_index = (form.start_index + 1) % form.start_from.len();
                    }
                    if *key == KeyAction::Left {
                        form.start_index =
                            (form.start_index + form.start_from.len() - 1) % form.start_from.len();
                    }
                }
            }
            FormField::CopyKeys => {
                if matches!(key, KeyAction::Space | KeyAction::Char(' ')) {
                    form.copy_keys = !form.copy_keys;
                }
            }
        }
        None
    }

    fn review_lines(&self, review: &Review) -> Vec<String> {
        match review {
            Review::Create(request) => vec![
                format!("name: {}", request.name),
                request.description.as_ref().map_or_else(
                    || "description: none".into(),
                    |text| format!("description: \"{text}\""),
                ),
                request.clone_from.as_ref().map_or_else(
                    || "start from: fresh".into(),
                    |name| format!("start from: copy of {name}"),
                ),
                format!(
                    "copy provider keys from {}: {}",
                    self.connected,
                    if request.mirror_credentials {
                        "on"
                    } else {
                        "off"
                    }
                ),
                format!(
                    "Hermes also adds a shell command named {} when that name is free.",
                    request.name
                ),
            ],
            Review::Description(edit) => vec![format!(
                "description: \"{}\" -> \"{}\"",
                edit.base.trim(),
                edit.input.text.trim()
            )],
            Review::Soul(edit) => {
                let diff = line_diff(&edit.base, &edit.result);
                let mut lines = vec![format!("+{} −{} lines", diff.added, diff.removed)];
                lines.extend(diff.first_lines);
                if edit.opened.contains('\u{fffd}') {
                    lines.push("Note: this soul held text that is not UTF-8; saving writes it as the replacement character.".into());
                }
                lines
            }
        }
    }

    fn editor_returned(&mut self, result: RawEdit, effects: &mut Vec<ProfilesEffect>) {
        let Some(mut edit) = self.editing.take() else {
            return;
        };
        match result {
            RawEdit::Failed(words) => {
                self.notice(words, effects);
                return;
            }
            RawEdit::Unchanged => edit.result = edit.opened.clone(),
            RawEdit::Changed(text) => edit.result = text,
        }
        if edit.result == edit.base {
            self.kept.remove(&edit.name);
            self.notice("No change.".into(), effects);
            return;
        }
        if let Some(kept) = self.kept.get_mut(&edit.name) {
            *kept = edit.result.clone();
        }
        if self.open {
            self.panel = EditPanel::Review {
                review: Review::Soul(edit),
                back: Box::new(EditPanel::None),
            };
        }
    }

    fn created_answer(
        &mut self,
        result: Result<CreatedProfile, CallFailure>,
        now_ms: u64,
        effects: &mut Vec<ProfilesEffect>,
    ) {
        self.detail_visible = false;
        let request = self.creating.take();
        let name = request.as_ref().map_or("", |request| request.name.as_str());
        let text = match result {
            Ok(created) => {
                let name = normalize_profile_name(&created.name);
                self.created.retain(|(other, _)| other != &name);
                self.created.push((name.clone(), now_ms));
                if self.open && self.loaded && !self.rows.iter().any(|row| row.name == name) {
                    self.rows.push(ProfileRow {
                        name: name.clone(),
                        display_name: name.clone(),
                        is_default: false,
                        model: None,
                        provider: None,
                        in_roster: false,
                        state: classify(
                            &name,
                            &self.gateway,
                            self.topology.as_ref(),
                            Some(now_ms),
                            now_ms,
                        ),
                    });
                }
                self.create_form = None;
                if self.open {
                    for after in [0, 5_000, 15_000, 35_000, 40_000] {
                        self.schedule(
                            after,
                            if after == 40_000 {
                                Timer::Expire
                            } else {
                                Timer::Refresh
                            },
                            effects,
                        );
                    }
                }
                format!(
                    "Created profile {name}. Provider keys copied: {}. Waiting for the gateway to serve it...",
                    if created.keys_copied { "yes" } else { "no" }
                )
            }
            Err(CallFailure::Unknown | CallFailure::Unreadable) => {
                if self.open {
                    self.refresh(effects);
                }
                format!(
                    "Talaria lost the answer to the create request. Hermes may have created {name}; the list below is fresh."
                )
            }
            Err(CallFailure::NotSent) => {
                if self.open
                    && let Some(form) = self.create_form.take()
                {
                    self.panel = EditPanel::Create(form);
                }
                NOT_SENT.into()
            }
            Err(CallFailure::Refused { code: -32601, .. }) => {
                self.create_form = None;
                format!(
                    "This gateway does not let Talaria create profiles over its socket. Use: hermes profile create {name}"
                )
            }
            Err(CallFailure::Refused { message, .. }) => {
                if self.open
                    && let Some(form) = self.create_form.take()
                {
                    self.panel = EditPanel::Create(form);
                }
                format!("Hermes refused: {}", cut_words(&message))
            }
        };
        self.notice(text, effects);
    }

    fn configured_answer(
        &mut self,
        name: String,
        section: Section,
        result: Result<Applied, CallFailure>,
        effects: &mut Vec<ProfilesEffect>,
    ) {
        self.detail_visible = false;
        let review = self.configuring.take();
        let section_name = if section == Section::Soul {
            "soul"
        } else {
            "description"
        };
        let mut refresh = false;
        let mut restore = false;
        let text = match result {
            Ok(applied) if applied.applied => {
                refresh = true;
                if section == Section::Soul {
                    self.kept.remove(&name);
                    let diff = match &review {
                        Some(Review::Soul(edit)) => line_diff(&edit.base, &edit.result),
                        _ => SoulDiff::default(),
                    };
                    format!(
                        "Saved the soul of {name} (+{} −{} lines).",
                        diff.added, diff.removed
                    )
                } else {
                    format!("Saved the description of {name}.")
                }
            }
            Ok(_) => {
                restore = true;
                format!("Hermes did not save the {section_name} of {name}.")
            }
            Err(CallFailure::Unknown | CallFailure::Unreadable) => {
                refresh = true;
                format!(
                    "Talaria lost the answer to the change. Hermes may have saved the {section_name} of {name}; the list below is fresh."
                )
            }
            Err(CallFailure::NotSent) => {
                restore = true;
                NOT_SENT.into()
            }
            Err(CallFailure::Refused { code: -32601, .. }) => {
                "This gateway does not let Talaria change profiles over its socket.".into()
            }
            Err(CallFailure::Refused { message, .. }) => {
                restore = true;
                format!("Hermes refused: {}", cut_words(&message))
            }
        };
        self.notice(text, effects);
        if self.open {
            if restore && let Some(review) = review {
                let back = match &review {
                    Review::Description(edit) => EditPanel::Description(edit.clone()),
                    _ => EditPanel::None,
                };
                self.panel = EditPanel::Review {
                    review,
                    back: Box::new(back),
                };
            }
            if refresh {
                self.panel = EditPanel::None;
                self.refresh(effects);
            }
        }
    }
    /// Apply one input at one monotonic clock reading.
    pub fn update(&mut self, input: ProfilesInput, now_ms: u64) -> Vec<ProfilesEffect> {
        let before = self.view();
        let selected_before = self.selected.clone();
        let tick = matches!(&input, ProfilesInput::Tick { .. });
        let mut effects = Vec::new();
        match input {
            ProfilesInput::Open => {
                self.open = true;
                self.panel = EditPanel::None;
                self.detail_visible = true;
                self.narrow_detail = false;
                self.confirm = false;
                self.notices.clear();
                self.refresh(&mut effects);
                let remaining: Vec<_> = self
                    .created
                    .iter()
                    .filter_map(|(_, at)| {
                        let elapsed = now_ms.saturating_sub(*at);
                        (elapsed < 40_000).then(|| 40_000 - elapsed)
                    })
                    .collect();
                for after in remaining {
                    self.schedule(after, Timer::Expire, &mut effects);
                }
            }
            ProfilesInput::Closed => {
                self.close();
                effects.push(ProfilesEffect::CancelTicks);
            }
            ProfilesInput::Refresh if self.open => self.refresh(&mut effects),
            ProfilesInput::Discovered(generation, result) if self.accept(generation) => {
                if let Err(failure) = &result {
                    self.discovery_failure = Some(failure.clone());
                    self.reclassify(now_ms);
                    self.notices.clear();
                    self.notice(failure.message.clone(), &mut effects);
                }
                if let Some(load) = &mut self.load {
                    load.discovery = Some(result);
                }
                self.finish_load(now_ms, &mut effects);
            }
            ProfilesInput::Listed(generation, result) if self.accept(generation) => {
                self.detail_visible = true;
                if let Some(load) = &mut self.load {
                    load.roster = Some(result);
                }
                self.finish_load(now_ms, &mut effects);
            }
            ProfilesInput::Topology(generation, result) if self.accept(generation) => {
                if let Some(load) = &mut self.load {
                    load.topology = Some(result);
                }
                self.finish_load(now_ms, &mut effects);
            }
            ProfilesInput::Described(generation, name, result) if self.accept(generation) => {
                self.described(normalize_profile_name(&name), result, &mut effects);
            }
            ProfilesInput::Key(key) if self.open => {
                if !self.confirm {
                    self.notices.pop_front();
                }
                self.key(key, &mut effects);
            }
            ProfilesInput::UnboundKey if self.open => {
                self.notices.pop_front();
            }
            ProfilesInput::Paste(text) if self.open && self.text_focus() => {
                let text = one_line(&text);
                let mut panel = std::mem::take(&mut self.panel);
                match &mut panel {
                    EditPanel::Description(edit) => edit.input.insert(&text),
                    EditPanel::Create(form) => {
                        let (value, cursor) = if form.focus == FormField::Name {
                            (&mut form.name, &mut form.name_cursor)
                        } else {
                            (&mut form.description, &mut form.description_cursor)
                        };
                        let mut input = TextInput {
                            text: std::mem::take(value),
                            cursor: *cursor,
                        };
                        if form.focus == FormField::Description {
                            input.insert_bounded(&text, 200);
                        } else {
                            input.insert(&text);
                        }
                        *value = input.text;
                        *cursor = input.cursor;
                    }
                    _ => {
                        if let Some(filter) = &mut self.filter {
                            filter.insert(&text);
                        }
                    }
                }
                self.panel = panel;
            }
            ProfilesInput::CreateAnswered(result) => {
                self.created_answer(result, now_ms, &mut effects)
            }
            ProfilesInput::ConfigureAnswered(name, section, result) => {
                self.configured_answer(name, section, result, &mut effects)
            }
            ProfilesInput::RescanAnswered(result) => {
                self.rescanning = false;
                self.notices
                    .retain(|notice| notice != "A rescan is already running.");
                let text = match result {
                    Ok(RescanSummary::Finished { added, parked, removed }) => format!("Rescan finished: {added} added, {parked} parked, {removed} removed."),
                    Ok(RescanSummary::Pending) => "The gateway is still rescanning; this list refreshes when it finishes.".into(),
                    Ok(RescanSummary::SingleProfile) => "This gateway serves one profile only, so a rescan changes nothing.".into(),
                    Err(RescanFailure::UnknownVerb) => "This gateway does not support rescanning from a client. Wait up to 30 seconds, or restart it.".into(),
                    Err(RescanFailure::Timeout) => "The gateway did not answer the rescan within 10 seconds; it may still be rescanning. Press r to refresh.".into(),
                    Err(RescanFailure::Other(words)) => format!("The rescan request failed: {words}."),
                };
                self.notice(text, &mut effects);
                if self.open {
                    self.refresh(&mut effects);
                }
            }
            ProfilesInput::EditorReturned(result) => self.editor_returned(result, &mut effects),
            ProfilesInput::Tick { token } if self.open => {
                let Some(timer) = self.timers.remove(&token) else {
                    return Vec::new();
                };
                match timer {
                    Timer::Describe(name) if self.selected.as_ref() == Some(&name) => {
                        self.describe(name, false, &mut effects)
                    }
                    Timer::Refresh => self.refresh(&mut effects),
                    Timer::Expire | Timer::Describe(_) => {}
                }
            }
            _ => return Vec::new(),
        }
        self.reclassify(now_ms);
        self.ensure_selection();
        if self.open
            && self.loaded
            && self.selected != selected_before
            && let Some(name) = self.selected.clone()
            && !self.requested.contains(&name)
            && !self
                .timers
                .values()
                .any(|timer| matches!(timer, Timer::Describe(pending) if pending == &name))
        {
            self.timers
                .retain(|_, timer| !matches!(timer, Timer::Describe(_)));
            self.schedule(150, Timer::Describe(name), &mut effects);
        }
        if self.open && tick && before != self.view() {
            effects.push(ProfilesEffect::Redraw);
        }
        effects
    }

    /// Everything the screen draws.
    #[must_use]
    pub fn view(&self) -> ProfilesView {
        let visible = self.visible();
        let selected = visible
            .iter()
            .position(|row| Some(&row.name) == self.selected.as_ref());
        let panel = match &self.panel {
            EditPanel::Create(form) => Panel::Form(form.clone()),
            EditPanel::Description(edit) => Panel::DescriptionInput {
                name: edit.name.clone(),
                text: edit.input.text.clone(),
                cursor: edit.input.cursor,
                error: edit.error.clone(),
            },
            EditPanel::Review { review, .. } => Panel::Review(self.review_lines(review)),
            EditPanel::None => match selected.and_then(|index| visible.get(index)) {
                Some(row) if self.detail_visible => {
                    let detail = self.detail(row);
                    if self.narrow_detail {
                        Panel::NarrowDetail(detail)
                    } else {
                        Panel::Detail(detail)
                    }
                }
                _ => Panel::None,
            },
        };
        ProfilesView {
            loading: !self.loaded,
            header: format!("Profiles ({} on disk · {} served)", self.roster.as_ref().map_or_else(|| "-".into(), |rows| rows.len().to_string()), self.rows.iter().filter(|row| row.state == ServeState::Served).count()),
            rows: visible.iter().map(|row| {
                let explanation = self.explanation(row);
                RowView {
                    name_cell: if row.display_name != row.name { format!("{} ({})", row.display_name, row.name) } else { row.name.clone() },
                    label: explanation.label.into(),
                    tone: if matches!(row.state, ServeState::Served | ServeState::Starting) { RowTone::Muted } else { RowTone::Warning },
                    model_cell: row.model.as_ref().map_or_else(|| "-".into(), |model| row.provider.as_ref().map_or_else(|| model.clone(), |provider| format!("{model} · {provider}"))),
                    next_step: explanation.next_step,
                }
            }).collect(),
            selected,
            filter: self.filter.as_ref().map(|filter| filter.text.clone()),
            panel,
            notice: self.notices.front().cloned(),
            confirm: self.confirm.then(|| "Ask the gateway to rescan profiles now? It starts serving new profiles and retries parked ones, including their messaging bots. Press y to confirm, any other key to cancel.".into()),
        }
    }
}
