use crate::commands::local::LocalAction;
use serde_json::Value;
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigSchemaView {
    pub fields: Vec<FieldView>,
    pub category_order: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FieldView {
    pub key: String,
    pub kind: FieldKind,
    pub description: String,
    pub category: String,
    pub options: Option<Vec<Value>>,
    pub searchable: bool,
    pub clearable: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Boolean,
    Number,
    String,
    Text,
    List,
    Select,
    Object,
    Other(String),
}
impl ConfigSchemaView {
    pub fn from_json_str(_text: &str) -> Result<Self, serde_json::Error> {
        todo!("settings stub")
    }
}
#[derive(Debug)]
pub struct SettingsSnapshot {
    pub profile: String,
    pub schema: ConfigSchemaView,
    pub effective: Value,
    pub raw: Value,
    pub defaults: Value,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ValueKind {
    Bool,
    Integer,
    Float,
    Text,
    StringList,
    JsonList,
    Nullable,
    Choice,
    ReadOnly(ReadOnlyReason),
}
#[derive(Clone, Debug, PartialEq)]
pub enum ReadOnlyReason {
    OwnedElsewhere,
    EnvReference(Vec<String>),
    HoldsSecret,
    Unsupported(String),
}
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyEffect {
    Now,
    NewSessions,
    NeedsRestart,
    MayNeedRestart,
}
#[derive(Clone, Debug, PartialEq)]
pub struct InputError;
pub fn apply_effect(_key: &str) -> (ApplyEffect, Option<&'static str>) {
    todo!("settings stub")
}
pub fn is_secret_key(_key: &str) -> bool {
    todo!("settings stub")
}
pub fn contains_secret_key(_value: &Value) -> bool {
    todo!("settings stub")
}
pub fn env_reference_names(_raw: &Value) -> Vec<String> {
    todo!("settings stub")
}
pub fn value_kind(
    _field: &FieldView,
    _current: Option<&Value>,
    _default: Option<&Value>,
    _raw: Option<&Value>,
) -> ValueKind {
    todo!("settings stub")
}
pub fn parse_input(
    _kind: &ValueKind,
    _field: &FieldView,
    _text: &str,
    _current: Option<&Value>,
) -> Result<Value, InputError> {
    todo!("settings stub")
}
pub fn build_patch(_changes: &BTreeMap<String, Value>) -> Value {
    todo!("settings stub")
}
#[derive(Clone, Debug, PartialEq)]
pub enum KeyOutcome {
    Applied,
    NotApplied,
    Adjusted(Value),
    ChangedElsewhere(Value),
}
pub fn classify_readback(
    _sent: &Value,
    _before: Option<&Value>,
    _after: Option<&Value>,
) -> KeyOutcome {
    todo!("settings stub")
}
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub key: String,
    pub yours: Value,
    pub baseline: Option<Value>,
    pub now: Option<Value>,
}
pub fn find_conflicts(
    _changes: &BTreeMap<String, Value>,
    _baseline: &Value,
    _fresh: &Value,
) -> (Vec<Conflict>, Vec<String>) {
    todo!("settings stub")
}
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewRow {
    pub key: String,
    pub old: String,
    pub new: String,
    pub effect_label: String,
    pub warnings: Vec<String>,
}
pub fn review_rows(
    _snapshot: &SettingsSnapshot,
    _pending: &BTreeMap<String, Value>,
) -> Vec<ReviewRow> {
    todo!("settings stub")
}
pub fn secret_key_in_args(_args: &str) -> Option<String> {
    todo!("settings stub")
}
pub fn is_secret_config_line(_text: &str) -> bool {
    todo!("settings stub")
}
pub fn parse_config_command(_args: &str) -> Result<ConfigCommand, CommandError> {
    todo!("settings stub")
}
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigCommand {
    Open { filter: Option<String> },
    Set(Vec<(String, String)>),
}
#[derive(Clone, Debug, PartialEq)]
pub enum CommandError {
    Secret { key: String },
    Syntax { detail: String },
}
pub struct SettingsLink {
    pub label: &'static str,
    pub action: LocalAction,
}
pub const SETTINGS_LINKS: &[SettingsLink] = &[];
#[derive(Debug)]
pub struct SettingsState;
impl SettingsState {
    pub fn new(_links: &'static [SettingsLink]) -> Self {
        todo!("settings stub")
    }
    pub fn update(&mut self, _input: SettingsInput) -> Vec<SettingsEffect> {
        todo!("settings stub")
    }
    pub fn view(&self) -> SettingsView {
        todo!("settings stub")
    }
}
#[derive(Debug)]
pub enum SettingsInput {
    Open {
        profile: String,
        filter: Option<String>,
    },
    Command {
        profile: String,
        args: String,
    },
    Loaded(SettingsSnapshot),
    LoadFailed(FailReason),
    Key(SettingsKey),
    Fresh(Value),
    FreshFailed(FailReason),
    Written(WriteResult),
    ReadBack {
        effective: Value,
        raw: Value,
    },
    ReadBackFailed(FailReason),
    RevealTimeout(u64),
}
#[derive(Debug, PartialEq)]
pub enum SettingsEffect {
    Load { profile: String },
    FetchFresh,
    Write { patch: Value },
    ReadBack,
    Notice(String),
    ScheduleReveal { id: u64, after: Duration },
    ForgetHistoryLine,
    OpenLocal(LocalAction),
    CloseOverlay,
}
#[derive(Clone, Debug, PartialEq)]
pub enum WriteResult {
    Ok,
    Refused { status: u16, detail: Option<String> },
    Unauthorized,
    Forbidden,
    NotFound { route: String },
    Server { status: u16 },
    Unknown,
    NotSent(FailReason),
}
#[derive(Clone, Debug, PartialEq)]
pub enum FailReason {
    NotConnected,
    ProfileNotServed,
    Unauthorized,
    Timeout { secs: u64 },
    Dropped,
    Http { status: u16 },
    NotFound { route: String },
    BadData,
    Ticket(String),
    NotLoopback,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsKey {
    Action(SettingsAction),
    Char(char),
    Edit(EditKey),
    Paste(String),
}
#[derive(Clone, Debug, PartialEq)]
pub enum SettingsAction {
    Up,
    Down,
    PageUp,
    PageDown,
    PrevCategory,
    NextCategory,
    Edit,
    Toggle,
    Filter,
    Default,
    UndoField,
    Reveal,
    Review,
    Reload,
    Help,
    Close,
}
#[derive(Clone, Debug, PartialEq)]
pub enum EditKey {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Backspace,
    Delete,
    ClearLine,
    Enter,
    Esc,
}
#[derive(Default)]
pub struct SettingsView {
    pub screen: Screen,
    pub profile: String,
    pub text_focus: bool,
    pub categories: Vec<String>,
    pub rows: Vec<SettingsRow>,
    pub cursor: usize,
    pub banner: Vec<String>,
    pub detail: String,
    pub footer: String,
    pub review_rows: Vec<ReviewRow>,
    pub conflict_rows: Vec<Conflict>,
    pub filter: String,
}
#[derive(Default)]
pub struct SettingsRow {
    pub key: String,
    pub value: String,
    pub origin: String,
    pub marker: String,
    pub effect_label: String,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Closed,
    Loading,
    LoadError,
    Browsing,
    Editing,
    Choosing,
    Help,
    Reviewing,
    Checking,
    Conflict,
    Applying,
    Verifying,
    ConfirmDiscard,
}
