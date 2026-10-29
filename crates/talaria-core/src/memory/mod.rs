//! Pure state and presentation data for memory settings.
use crate::settings::SecretField;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, fmt, time::Duration};
mod fields;
mod state;
mod text;
pub use fields::*;
pub use state::MemoryState;
pub use text::*;

#[derive(Clone, Debug, PartialEq)]
pub struct StatusView {
    pub active: String,
    pub providers: Vec<ProviderRowView>,
    pub builtin_files: BuiltinFiles,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderRowView {
    pub name: String,
    pub description: String,
    pub available: bool,
    pub configured: bool,
    pub status: String,
    pub setup: ProviderSetup,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderSetup {
    pub pip_dependencies: Vec<String>,
    pub python_dependencies_declared: bool,
    pub external_dependencies: Vec<ExternalDependency>,
    pub required_env: Vec<String>,
    pub dependencies_installed: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExternalDependency {
    pub name: String,
    pub install: String,
    pub check: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltinFiles {
    pub memory: u64,
    pub user: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigSurface {
    Declared,
    Plain,
}
#[derive(Clone, Debug, PartialEq)]
pub enum FieldKind {
    Text,
    Secret,
    Select,
    Bool,
    Integer,
    Number,
    Json,
    Other(String),
}
#[derive(Clone, Debug, PartialEq)]
pub struct FieldOption {
    pub value: String,
    pub label: String,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FieldView {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    pub description: String,
    pub placeholder: String,
    pub required: bool,
    pub value: Value,
    pub is_set: bool,
    pub options: Vec<FieldOption>,
    pub when: Option<Map<String, Value>>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigView {
    pub name: String,
    pub label: String,
    pub surface: ConfigSurface,
    pub fields: Vec<FieldView>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetTarget {
    Memory,
    User,
    All,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CuratorView {
    pub enabled: bool,
    pub paused: bool,
    pub interval_hours: Option<f64>,
    pub last_run_at: Option<String>,
    pub min_idle_hours: Option<f64>,
    pub stale_after_days: Option<f64>,
    pub archive_after_days: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ActionView {
    pub name: String,
    pub running: bool,
    pub exit_code: Option<i64>,
    pub pid: Option<i64>,
    pub lines: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailText(pub String);
#[derive(Clone, Debug, PartialEq)]
pub enum WriteOutcome {
    Ok(Value),
    Refused { status: u16, detail: Option<String> },
    Failed(FailText),
    Unknown,
    NotSent(FailText),
}
#[derive(Clone, Default, PartialEq)]
pub struct SubmittedValues {
    pub plain: BTreeMap<String, Value>,
    pub secrets: BTreeMap<String, SecretField>,
}
impl fmt::Debug for SubmittedValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubmittedValues")
            .field("plain_keys", &self.plain.keys().collect::<Vec<_>>())
            .field("secret_keys", &self.secrets.keys().collect::<Vec<_>>())
            .finish()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum MemoryInput {
    Open {
        profile: String,
    },
    NotConnected,
    StatusLoaded(Result<StatusView, FailText>),
    CuratorLoaded(Result<CuratorView, FailText>),
    ConfigLoaded {
        name: String,
        result: Result<ConfigView, FailText>,
    },
    ProviderSet(WriteOutcome),
    ConfigSaved(WriteOutcome),
    ResetDone(WriteOutcome),
    PausedSet(WriteOutcome),
    RunStarted(WriteOutcome),
    ActionPolled {
        follow: u64,
        result: Result<ActionView, FailText>,
    },
    Key(MemoryKey),
}
#[derive(Clone, Debug, PartialEq)]
pub enum MemoryEffect {
    LoadStatus,
    LoadCurator,
    LoadConfig {
        name: String,
    },
    SetProvider {
        name: String,
    },
    SaveConfig {
        name: String,
        surface: ConfigSurface,
        values: SubmittedValues,
    },
    Reset {
        target: ResetTarget,
    },
    SetPaused(bool),
    RunCurator,
    PollAction {
        follow: u64,
        after: Duration,
    },
    OpenSettings {
        profile: String,
        filter: &'static str,
    },
    CopyText(String),
    Notice(String),
    CloseOverlay,
}
#[derive(Clone, PartialEq, Eq)]
pub enum MemoryKey {
    Up,
    Down,
    Open,
    Use,
    Delete,
    DeleteBoth,
    Pause,
    Run,
    Settings,
    CuratorSettings,
    Reload,
    Copy,
    Save,
    Close,
    Left,
    Right,
    Tab,
    Space,
    Char(char),
    Edit(EditOp),
    Paste(String),
}
impl fmt::Debug for MemoryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Open => "Open",
            Self::Use => "Use",
            Self::Delete => "Delete",
            Self::DeleteBoth => "DeleteBoth",
            Self::Pause => "Pause",
            Self::Run => "Run",
            Self::Settings => "Settings",
            Self::CuratorSettings => "CuratorSettings",
            Self::Reload => "Reload",
            Self::Copy => "Copy",
            Self::Save => "Save",
            Self::Close => "Close",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Tab => "Tab",
            Self::Space => "Space",
            Self::Char(_) => "Char(<redacted>)",
            Self::Paste(_) => "Paste(<redacted>)",
            Self::Edit(op) => return f.debug_tuple("Edit").field(op).finish(),
        };
        f.write_str(name)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOp {
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
}
#[derive(Clone, Debug, PartialEq)]
pub enum FieldError {
    NotWholeNumber,
    NotNumber,
    BelowMin(f64),
    AboveMax(f64),
    NotJsonContainer,
    NotAnOption,
    ReadOnly(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SavedOutcome {
    Saved,
    NotSaved,
    Adjusted(String),
    Cleared(String),
    SecretStored,
    SecretSetUnconfirmed,
    SecretNotStored,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryView {
    pub title: String,
    pub banner: Option<String>,
    pub providers: Section<Vec<RowView>>,
    pub active_text: String,
    pub files: Section<Vec<RowView>>,
    pub curator: Section<CuratorRowView>,
    pub cursor: usize,
    pub card: Option<CardView>,
    pub confirm: Option<ConfirmView>,
    pub following: Option<FollowView>,
    pub editing: bool,
    pub footer: Vec<(String, String)>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Section<T> {
    Loading,
    Failed(String),
    Ready(T),
}
#[derive(Clone, Debug, PartialEq)]
pub struct RowView {
    pub marker: char,
    pub name: String,
    pub description: String,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CuratorRowView {
    pub status: String,
    pub schedule: String,
    pub last_run: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CardView {
    pub name: String,
    pub label: String,
    pub description: String,
    pub needs: Vec<String>,
    pub fields: Vec<FieldRowView>,
    pub setup: Option<String>,
    pub result_lines: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FieldRowView {
    pub label: String,
    pub value: String,
    pub pending: bool,
    pub selected: bool,
    /// Caret position in characters when this value is being edited.
    pub cursor: Option<usize>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmView {
    pub lines: Vec<String>,
    pub buttons: [String; 2],
    pub selected: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FollowView {
    pub elapsed_s: u64,
    pub lines: Vec<String>,
}

#[cfg(test)]
mod privacy_tests {
    use super::*;

    #[test]
    fn editor_input_debug_hides_characters_and_pastes() {
        let sentinel = "editor-credential-sentinel";
        let key = MemoryKey::Paste(sentinel.into());
        assert!(!format!("{key:?}").contains(sentinel));
        let input = MemoryInput::Key(key);
        assert!(!format!("{input:?}").contains(sentinel));
        assert_eq!(format!("{:?}", MemoryKey::Char('x')), "Char(<redacted>)");
    }
}
