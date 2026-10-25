//! The picker's state machine: modes, inputs and effects (#231).

use std::collections::BTreeMap;

use super::export::{ExportFormat, REDACT_BY_DEFAULT};
use super::rows::{DirectoryRow, RowKey, RowOrigin, matches_folded, order};
use super::title::{TitleError, clean_title};

/// How one profile's lists are doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileLoad {
    Pending,
    Loaded,
    Failed { reason: String },
}

/// What the picker is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerMode {
    List,
    Search,
    Rename(LineEditor),
    Export(ExportDialog),
    Waiting(WaitingFor),
}

/// The rename editor: text, caret and its held limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineEditor {
    /// The text as it stands.
    pub text: String,
    /// The caret, counted in characters.
    pub caret: usize,
}

impl LineEditor {
    /// An editor holding `text`, with the caret at its end.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            caret: text.chars().count(),
        }
    }

    /// Insert one character at the caret.
    pub fn insert(&mut self, character: char) {
        let index = self.byte_index(self.caret);
        self.text.insert(index, character);
        self.caret += 1;
    }

    /// Remove the character before the caret.
    pub fn backspace(&mut self) {
        if self.caret == 0 {
            return;
        }
        let index = self.byte_index(self.caret - 1);
        self.text.remove(index);
        self.caret -= 1;
    }

    /// Move the caret one character left.
    pub fn left(&mut self) {
        self.caret = self.caret.saturating_sub(1);
    }

    /// Move the caret one character right.
    pub fn right(&mut self) {
        let caret = self.caret + 1;
        if caret <= self.text.chars().count() {
            self.caret = caret;
        }
    }

    /// Put the caret at the start of the line.
    pub fn home(&mut self) {
        self.caret = 0;
    }

    /// Put the caret at the end of the line.
    pub fn end(&mut self) {
        self.caret = self.text.chars().count();
    }

    /// The byte index of the caret.
    fn byte_index(&self, caret: usize) -> usize {
        self.text
            .char_indices()
            .nth(caret)
            .map(|(index, _)| index)
            .unwrap_or(self.text.len())
    }
}

/// The export dialog: its row, format and redaction state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportDialog {
    pub key: RowKey,
    pub format: ExportFormat,
    pub redact: bool,
}

/// What a waiting picker waits for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitingFor {
    Rename(RowKey),
    Branch,
    Export(RowKey),
    Open(RowKey),
}

/// The profile filter: all profiles or one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileFilter {
    All,
    One(String),
}

/// The picker's state: the rows, the cursor, the mode and how each profile answered.
pub struct PickerState {
    /// Every row the gateway has sent, live over stored, ordered.
    rows: Vec<DirectoryRow>,
    /// The places of the rows under the filter and query, in order.
    visible: Vec<usize>,
    /// What the picker is doing.
    mode: PickerMode,
    /// The cursor, counted in visible rows.
    cursor: usize,
    /// The search text.
    query: String,
    /// The profiles the gateway serves, in order.
    profiles: Vec<String>,
    /// The profile filter.
    filter: ProfileFilter,
    /// The list generation the picker waits for.
    generation: u64,
    /// The notice line.
    notice: Option<String>,
    /// One failed profile and scope pair, with its reason.
    unavailable: Vec<(String, RowOrigin, String)>,
    /// The scope note the gateway sent.
    scope_note: Option<String>,
    /// The side connection's failure, when it failed.
    side_failure: Option<String>,
    /// The session the foreground holds.
    foreground: Option<RowKey>,
    /// How each profile and scope answered, so the header can say what is pending.
    loads: BTreeMap<(String, RowOrigin), ProfileLoad>,
    /// Whether a session action runs.
    busy: bool,
    /// The folder the export dialog names, as the operator reads it.
    save_folder: String,
}

impl PickerState {
    /// A picker that has not opened yet.
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            visible: Vec::new(),
            mode: PickerMode::List,
            cursor: 0,
            query: String::new(),
            profiles: Vec::new(),
            filter: ProfileFilter::All,
            generation: 0,
            notice: None,
            unavailable: Vec::new(),
            scope_note: None,
            side_failure: None,
            foreground: None,
            loads: BTreeMap::new(),
            busy: false,
            save_folder: ".".to_owned(),
        }
    }

    /// Name the folder exports from the dialog go to: the launch folder, with `~` for the home
    /// folder.
    pub fn set_save_folder(&mut self, folder: impl Into<String>) {
        self.save_folder = folder.into();
    }

    /// The folder exports from the dialog go to, as the dialog shows it.
    pub fn save_folder(&self) -> &str {
        &self.save_folder
    }

    /// The rows under the filter and query, ordered.
    pub fn visible_rows(&self) -> Vec<&DirectoryRow> {
        self.visible
            .iter()
            .map(|index| &self.rows[*index])
            .collect()
    }

    /// The row under the cursor, if any.
    pub fn selected(&self) -> Option<&DirectoryRow> {
        self.visible
            .get(self.cursor)
            .map(|index| &self.rows[*index])
    }

    /// The current mode.
    pub fn mode(&self) -> &PickerMode {
        &self.mode
    }

    /// The notice line, if any.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The switch or branch the picker waited for did not happen: back to the list, with
    /// why as the notice.
    pub fn end_wait(&mut self, notice: impl Into<String>) {
        if matches!(self.mode, PickerMode::Waiting(_)) {
            self.mode = PickerMode::List;
        }
        self.notice = Some(notice.into());
    }

    /// One failed profile and scope pair: profile, origin, reason.
    pub fn unavailable(&self) -> Vec<(String, RowOrigin, String)> {
        self.unavailable.clone()
    }

    /// The profiles the gateway serves, in order.
    pub fn served_profiles(&self) -> &[String] {
        &self.profiles
    }

    /// The session the foreground holds.
    pub fn foreground(&self) -> Option<&RowKey> {
        self.foreground.as_ref()
    }

    /// The profile filter in force.
    pub fn filter(&self) -> &ProfileFilter {
        &self.filter
    }

    /// The search text.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// How each profile and scope answered.
    pub fn loads(&self) -> &BTreeMap<(String, RowOrigin), ProfileLoad> {
        &self.loads
    }

    /// The first profile whose list has not answered yet.
    pub fn pending_profile(&self) -> Option<&str> {
        self.profiles
            .iter()
            .find(|profile| {
                [RowOrigin::Live, RowOrigin::Stored].iter().any(|origin| {
                    matches!(
                        self.loads.get(&((**profile).clone(), *origin)),
                        Some(ProfileLoad::Pending)
                    )
                })
            })
            .map(String::as_str)
    }

    /// The banner lines, at most two: the scope note, then a side failure.
    pub fn banners(&self) -> Vec<String> {
        let mut banners = Vec::new();
        if let Some(note) = &self.scope_note {
            banners.push(note.clone());
        }
        if let Some(failure) = &self.side_failure {
            banners.push(failure.clone());
        }
        banners.truncate(2);
        banners
    }

    /// Whether the filter lets this profile through.
    fn filter_allows(&self, profile: &str) -> bool {
        match &self.filter {
            ProfileFilter::All => true,
            ProfileFilter::One(one) => one == profile,
        }
    }

    /// Rebuild the list of the rows under the filter and query. The query is
    /// folded once, and each field is folded into the same buffer.
    fn refresh_visible(&mut self) {
        let query = self.query.to_lowercase();
        let mut buffer = String::new();
        let mut visible = std::mem::take(&mut self.visible);
        visible.clear();
        for (index, row) in self.rows.iter().enumerate() {
            if !self.filter_allows(&row.key.profile) {
                continue;
            }
            if query.is_empty() || matches_folded(row, &query, &mut buffer) {
                visible.push(index);
            }
        }
        self.visible = visible;
    }

    /// Put the cursor back on the row it held, or as near as it can.
    fn sync_cursor(&mut self, held: Option<RowKey>) {
        if self.visible.is_empty() {
            self.cursor = 0;
            return;
        }
        if let Some(held) = held
            && let Some(index) = self
                .visible
                .iter()
                .position(|index| self.rows[*index].key == held)
        {
            self.cursor = index;
            return;
        }
        if self.cursor >= self.visible.len() {
            self.cursor = self.visible.len() - 1;
        }
    }

    /// The key the cursor holds now.
    fn held_key(&self) -> Option<RowKey> {
        self.selected().map(|row| row.key.clone())
    }
}

impl Default for PickerState {
    fn default() -> Self {
        Self::new()
    }
}

/// One picker command from a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerAction {
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    Open,
    Rename,
    Branch,
    Export,
    Search,
    ProfileNext,
    ProfilePrev,
    Refresh,
    Close,
    FormatMarkdown,
    FormatJson,
    FormatHtml,
    ToggleRedact,
}

/// One input to the picker.
#[derive(Clone, Debug, PartialEq)]
pub enum PickerInput {
    Opened {
        profiles: Vec<String>,
        foreground: Option<RowKey>,
        scope_note: Option<String>,
        cached: Vec<DirectoryRow>,
        generation: u64,
    },
    Listed {
        generation: u64,
        profile: String,
        origin: RowOrigin,
        rows: Vec<DirectoryRow>,
        skipped: usize,
    },
    ListFailed {
        generation: u64,
        profile: String,
        origin: RowOrigin,
        reason: String,
    },
    SideFailed {
        reason: String,
    },
    Action(PickerAction),
    Text(char),
    Paste(String),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Enter,
    Escape,
    CtrlC,
    Renamed {
        key: RowKey,
        title: String,
    },
    Notice(String),
    Busy(bool),
    Resize {
        cols: u16,
        rows: u16,
    },
}

/// What the application does after one input.
#[derive(Clone, Debug, PartialEq)]
pub enum PickerEffect {
    List {
        generation: u64,
    },
    Open(RowKey),
    Rename {
        key: RowKey,
        title: String,
    },
    Branch {
        title: Option<String>,
    },
    Export {
        key: RowKey,
        format: ExportFormat,
        redact: bool,
    },
    Close,
}

/// Apply one input. No input or output: the effects say what to do.
pub fn update(state: &mut PickerState, input: PickerInput, page: usize) -> Vec<PickerEffect> {
    match input {
        PickerInput::Opened {
            profiles,
            foreground,
            scope_note,
            cached,
            generation,
        } => {
            state.profiles = profiles;
            state.foreground = foreground;
            state.scope_note = scope_note;
            state.generation = generation;
            state.rows = cached;
            state.loads.clear();
            for profile in &state.profiles {
                for origin in [RowOrigin::Live, RowOrigin::Stored] {
                    state
                        .loads
                        .insert((profile.clone(), origin), ProfileLoad::Pending);
                }
            }
            order(&mut state.rows);
            state.refresh_visible();
            state.mode = PickerMode::List;
            state.cursor = 0;
            vec![PickerEffect::List { generation }]
        }
        PickerInput::Listed {
            generation,
            profile,
            origin,
            rows: listed,
            skipped,
        } => {
            // A reply of an older generation is dropped; a newer one is the
            // current list.
            if generation < state.generation {
                return Vec::new();
            }
            state.generation = generation;
            let held = state.held_key();
            state
                .loads
                .insert((profile.clone(), origin), ProfileLoad::Loaded);
            state
                .rows
                .retain(|row| !(row.key.profile == profile && row.origin == origin));
            state.rows.extend(listed.iter().cloned());
            order(&mut state.rows);
            if skipped > 0 {
                state.notice = Some(format!(
                    "{skipped} rows from Hermes could not be read and are not shown."
                ));
            } else if listed.len() == 100 {
                let scope = match origin {
                    RowOrigin::Live => "live",
                    RowOrigin::Stored => "earlier",
                };
                state.notice = Some(format!(
                    "{profile}: only the newest 100 {scope} sessions are listed."
                ));
            }
            state.refresh_visible();
            state.sync_cursor(held);
            Vec::new()
        }
        PickerInput::ListFailed {
            generation,
            profile,
            origin,
            reason,
        } => {
            if generation < state.generation {
                return Vec::new();
            }
            state.generation = generation;
            state.loads.insert(
                (profile.clone(), origin),
                ProfileLoad::Failed {
                    reason: reason.clone(),
                },
            );
            state
                .unavailable
                .push((profile.clone(), origin, reason.clone()));
            state.notice = Some(format!("{profile}: {reason}"));
            Vec::new()
        }
        PickerInput::SideFailed { reason } => {
            state.side_failure = Some(reason);
            Vec::new()
        }
        PickerInput::Renamed { key, title } => {
            for row in &mut state.rows {
                if row.key == key {
                    row.title = title.clone();
                }
            }
            state.refresh_visible();
            Vec::new()
        }
        PickerInput::Notice(notice) => {
            state.notice = Some(notice);
            Vec::new()
        }
        PickerInput::Busy(busy) => {
            state.busy = busy;
            Vec::new()
        }
        PickerInput::Resize { .. } => Vec::new(),
        PickerInput::Action(action) => action_effect(state, action, page),
        PickerInput::Text(character) => {
            let held = state.held_key();
            let mut searched = false;
            match &mut state.mode {
                PickerMode::Search => {
                    state.query.push(character);
                    searched = true;
                }
                PickerMode::Rename(editor) => editor.insert(character),
                _ => {}
            }
            if searched {
                state.refresh_visible();
                state.sync_cursor(held);
            }
            Vec::new()
        }
        PickerInput::Paste(text) => {
            let held = state.held_key();
            let mut searched = false;
            match &mut state.mode {
                PickerMode::Search => {
                    state.query.push_str(&text);
                    searched = true;
                }
                PickerMode::Rename(editor) => {
                    for character in text.chars() {
                        editor.insert(character);
                    }
                }
                _ => {}
            }
            if searched {
                state.refresh_visible();
                state.sync_cursor(held);
            }
            Vec::new()
        }
        PickerInput::Backspace => {
            let held = state.held_key();
            let mut searched = false;
            match &mut state.mode {
                PickerMode::Search => {
                    state.query.pop();
                    searched = true;
                }
                PickerMode::Rename(editor) => editor.backspace(),
                _ => {}
            }
            if searched {
                state.refresh_visible();
                state.sync_cursor(held);
            }
            Vec::new()
        }
        PickerInput::Delete => {
            let held = state.held_key();
            let mut searched = false;
            if matches!(&state.mode, PickerMode::Search) {
                state.query.pop();
                searched = true;
            }
            if searched {
                state.refresh_visible();
                state.sync_cursor(held);
            }
            Vec::new()
        }
        PickerInput::Left => {
            match &mut state.mode {
                PickerMode::Search => {}
                PickerMode::Rename(editor) => editor.left(),
                _ => {}
            }
            Vec::new()
        }
        PickerInput::Right => {
            match &mut state.mode {
                PickerMode::Search => {}
                PickerMode::Rename(editor) => editor.right(),
                _ => {}
            }
            Vec::new()
        }
        PickerInput::Home => {
            match &mut state.mode {
                PickerMode::Search => {}
                PickerMode::Rename(editor) => editor.home(),
                _ => {}
            }
            Vec::new()
        }
        PickerInput::End => {
            match &mut state.mode {
                PickerMode::Search => {}
                PickerMode::Rename(editor) => editor.end(),
                _ => {}
            }
            Vec::new()
        }
        PickerInput::Enter => match state.mode.clone() {
            PickerMode::Search => {
                state.mode = PickerMode::List;
                Vec::new()
            }
            PickerMode::Rename(editor) => {
                let Some(key) = state.selected().map(|row| row.key.clone()) else {
                    return Vec::new();
                };
                match clean_title(&editor.text) {
                    Ok(title) => {
                        let unchanged = state
                            .rows
                            .iter()
                            .find(|row| row.key == key)
                            .is_some_and(|row| row.title == title);
                        if unchanged {
                            state.notice = Some("That is already the title.".to_owned());
                            return Vec::new();
                        }
                        state.mode = PickerMode::List;
                        vec![PickerEffect::Rename { key, title }]
                    }
                    Err(TitleError::Empty) => {
                        state.notice = Some("A title cannot be empty.".to_owned());
                        Vec::new()
                    }
                    Err(TitleError::TooLong { chars }) => {
                        state.notice = Some(format!(
                            "That title is {chars} characters long; a title holds 100 at most."
                        ));
                        Vec::new()
                    }
                }
            }
            PickerMode::Export(dialog) => {
                state.mode = PickerMode::List;
                vec![PickerEffect::Export {
                    key: dialog.key,
                    format: dialog.format,
                    redact: dialog.redact,
                }]
            }
            _ => Vec::new(),
        },
        PickerInput::Escape => match state.mode.clone() {
            PickerMode::Search => {
                state.mode = PickerMode::List;
                state.query.clear();
                state.refresh_visible();
                state.sync_cursor(None);
                Vec::new()
            }
            PickerMode::Rename(_) => {
                state.mode = PickerMode::List;
                Vec::new()
            }
            PickerMode::Export(_) => {
                state.mode = PickerMode::List;
                Vec::new()
            }
            PickerMode::Waiting(_) => vec![PickerEffect::Close],
            PickerMode::List => vec![PickerEffect::Close],
        },
        PickerInput::CtrlC => vec![PickerEffect::Close],
    }
}

/// Apply one action to the picker.
fn action_effect(state: &mut PickerState, action: PickerAction, page: usize) -> Vec<PickerEffect> {
    match state.mode.clone() {
        PickerMode::Waiting(_) => Vec::new(),
        PickerMode::Search => {
            match action {
                PickerAction::Up => move_cursor(state, -1),
                PickerAction::Down => move_cursor(state, 1),
                PickerAction::PageUp => move_cursor(state, -(page as isize)),
                PickerAction::PageDown => move_cursor(state, page as isize),
                PickerAction::Top => state.cursor = 0,
                PickerAction::Bottom => {
                    let last = state.visible.len().saturating_sub(1);
                    state.cursor = last;
                }
                _ => {}
            }
            Vec::new()
        }
        PickerMode::Rename(_) => Vec::new(),
        PickerMode::Export(_) => {
            if let PickerMode::Export(dialog) = &mut state.mode {
                match action {
                    PickerAction::FormatMarkdown => dialog.format = ExportFormat::Markdown,
                    PickerAction::FormatJson => dialog.format = ExportFormat::Json,
                    PickerAction::FormatHtml => dialog.format = ExportFormat::Html,
                    PickerAction::ToggleRedact => dialog.redact = !dialog.redact,
                    _ => {}
                }
            }
            Vec::new()
        }
        PickerMode::List => list_action(state, action, page),
    }
}

/// Apply one action in the list mode.
fn list_action(state: &mut PickerState, action: PickerAction, page: usize) -> Vec<PickerEffect> {
    match action {
        PickerAction::Up => move_cursor(state, -1),
        PickerAction::Down => move_cursor(state, 1),
        PickerAction::PageUp => move_cursor(state, -(page as isize)),
        PickerAction::PageDown => move_cursor(state, page as isize),
        PickerAction::Top => state.cursor = 0,
        PickerAction::Bottom => {
            let last = state.visible.len().saturating_sub(1);
            state.cursor = last;
        }
        PickerAction::Search => {
            state.mode = PickerMode::Search;
        }
        PickerAction::Open => {
            let Some(key) = state.selected().map(|row| row.key.clone()) else {
                return Vec::new();
            };
            state.mode = PickerMode::Waiting(WaitingFor::Open(key.clone()));
            return vec![PickerEffect::Open(key)];
        }
        PickerAction::Rename => {
            let Some(row) = state.selected() else {
                return Vec::new();
            };
            let editor = LineEditor::new(&row.title);
            state.mode = PickerMode::Rename(editor);
        }
        PickerAction::Branch => {
            let Some(key) = state.selected().map(|row| row.key.clone()) else {
                return Vec::new();
            };
            if state.foreground.as_ref() == Some(&key) {
                state.mode = PickerMode::Waiting(WaitingFor::Branch);
                return vec![PickerEffect::Branch { title: None }];
            }
            state.notice = Some(
                "Branch works on the session you are in. Open this one first (Enter), then press b."
                    .to_owned(),
            );
        }
        PickerAction::Export => {
            let Some(key) = state.selected().map(|row| row.key.clone()) else {
                return Vec::new();
            };
            state.mode = PickerMode::Export(ExportDialog {
                key,
                format: ExportFormat::Markdown,
                redact: REDACT_BY_DEFAULT,
            });
        }
        PickerAction::ProfileNext => cycle_profile(state, true),
        PickerAction::ProfilePrev => cycle_profile(state, false),
        PickerAction::Refresh => {
            return vec![PickerEffect::List {
                generation: state.generation,
            }];
        }
        PickerAction::Close => {
            return vec![PickerEffect::Close];
        }
        _ => {}
    }
    Vec::new()
}

/// Move the cursor by `delta` rows, clamped to the visible rows.
fn move_cursor(state: &mut PickerState, delta: isize) {
    let count = state.visible.len();
    if count == 0 {
        state.cursor = 0;
        return;
    }
    let next = state.cursor as isize + delta;
    state.cursor = next.clamp(0, count as isize - 1) as usize;
}

/// Move the profile filter one profile along.
fn cycle_profile(state: &mut PickerState, forward: bool) {
    if state.profiles.is_empty() {
        state.filter = ProfileFilter::All;
        return;
    }
    let index = state.profiles.iter().position(|profile| {
        Some(profile)
            == match &state.filter {
                ProfileFilter::All => None,
                ProfileFilter::One(one) => Some(one),
            }
    });
    let next = match (index, forward) {
        (None, true) => 0,
        (None, false) => state.profiles.len() - 1,
        (Some(index), true) => (index + 1) % state.profiles.len(),
        (Some(index), false) => (index + state.profiles.len() - 1) % state.profiles.len(),
    };
    state.filter = ProfileFilter::One(state.profiles[next].clone());
    state.refresh_visible();
    state.cursor = 0;
}
