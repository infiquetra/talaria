//! Pure state and presentation data for memory settings.
use crate::settings::SecretField;
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    fmt,
    time::{Duration, Instant},
};

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
#[derive(Clone, Debug, PartialEq, Eq)]
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
impl fmt::Display for FieldError {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
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
pub struct MemoryState;
impl MemoryState {
    pub fn new() -> Self {
        Self
    }
    pub fn update(&mut self, _input: MemoryInput, _now: Instant) -> Vec<MemoryEffect> {
        todo!()
    }
    pub fn view(&self, _now: Instant) -> MemoryView {
        todo!()
    }
}
impl Default for MemoryState {
    fn default() -> Self {
        Self::new()
    }
}
pub fn format_bytes(_n: u64) -> String {
    todo!()
}
pub fn provider_label(_status: &str) -> String {
    todo!()
}
pub fn curator_lines(_c: &CuratorView) -> (String, String, String) {
    todo!()
}
pub fn field_visible(_field: &FieldView, _values: &BTreeMap<String, String>) -> bool {
    todo!()
}
pub fn validate_input(_field: &FieldView, _text: &str) -> Result<Value, FieldError> {
    todo!()
}
pub fn classify_saved(
    _field: &FieldView,
    _sent: &Value,
    _before: &FieldView,
    _after: &FieldView,
) -> SavedOutcome {
    todo!()
}
pub fn tail_after_header(_lines: &[String], _max: usize) -> Vec<String> {
    todo!()
}
pub fn is_valid_provider_name(_name: &str) -> bool {
    todo!()
}
