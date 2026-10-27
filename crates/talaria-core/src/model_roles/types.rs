//! Mirrors of the model-role replies. The application converts them.

use serde_json::{Map, Value};

use crate::models::Effort;

#[derive(Clone, Debug, PartialEq)]
pub struct RolesInfo {
    pub model: String,
    pub provider: String,
    pub auto_context_length: u64,
    pub config_context_length: u64,
    pub capabilities: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteTriple {
    pub provider: String,
    pub model: String,
    pub base_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuxSlot {
    pub task: String,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub reasoning_effort: Option<String>,
    pub local_endpoint: bool,
    pub label: Option<String>,
    pub hint: Option<String>,
    pub plugin: Option<String>,
    pub inherit_from: Option<String>,
    pub effective: Option<RouteTriple>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuxView {
    pub slots: Vec<AuxSlot>,
    pub main: RouteTriple,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlotView {
    pub provider: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub enabled: Option<bool>,
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoaPresetView {
    pub name: String,
    pub enabled: bool,
    pub references: Vec<SlotView>,
    pub aggregator: SlotView,
    pub reference_temperature: Option<f64>,
    pub aggregator_temperature: Option<f64>,
    pub reference_timeout: Option<f64>,
    pub degraded_reference_policy: String,
    pub fanout: String,
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoaConfigView {
    pub default_preset: String,
    pub active_preset: String,
    pub presets: Vec<MoaPresetView>,
    pub privacy_filter: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RawRoles {
    pub fallback_providers: Vec<Value>,
    pub fallback_model: Vec<Value>,
    pub delegation: Map<String, Value>,
    pub model_context_length: u64,
    pub agent_reasoning_effort: Value,
    pub has_moa_presets: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleSlot {
    pub task: String,
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffortChange {
    Omit,
    Inherit,
    Level(Effort),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignScopeView {
    Main,
    Auxiliary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
    pub scope: AssignScopeView,
    pub task: String,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: EffortChange,
    pub base_url: String,
    pub confirmed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignOutcome {
    Main {
        provider: String,
        model: String,
        stale: Vec<StaleSlot>,
    },
    Auxiliary,
    ConfirmRequired {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RolesError {
    NotSent { reason: String },
    OutcomeUnknown,
    BadReply,
    Refused { status: u16, detail: Option<String> },
    RestartRequired,
    Server { status: u16 },
    Forbidden { detail: String },
    NotFound { route: String },
    SignIn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Main,
    Fallbacks,
    Helpers,
    Subagents,
    Moa,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoleRoute {
    Inherit,
    Pinned { provider: String, model: String },
    Partial { provider: String, model: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct FallbackEntry {
    pub provider: String,
    pub model: String,
    pub was_string: bool,
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChainMode {
    Inherit,
    Disabled,
    Own(Vec<FallbackEntry>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubagentRoles {
    pub route: RoleRoute,
    pub effort: Option<Effort>,
    pub chain: ChainMode,
    pub direct_endpoint: Option<String>,
    pub endpoint_keys: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MainRoles {
    pub route: RoleRoute,
    pub context_override: u64,
    pub effort: Option<Effort>,
    pub effort_read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub key: String,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    Info,
    Auxiliary,
    Moa,
    Raw,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum Loaded {
    Info(RolesInfo),
    Auxiliary(AuxView),
    Moa(MoaConfigView),
    Raw(RawRoles),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyStep {
    MainModel,
    MainConfig,
    Helper { task: String },
    Fallbacks,
    Subagents,
    Moa,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyReply {
    Assigned(AssignOutcome),
    Saved,
    MoaSaved(MoaConfigView),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RolesKey {
    Section(Section),
    NextSection,
    PrevSection,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Enter,
    Effort,
    Context,
    Inherit,
    Add,
    Remove,
    MoveUp,
    MoveDown,
    Toggle,
    MakeDefault,
    ChainMode,
    ResetStale,
    Review,
    Discard,
    Reload,
    Escape,
    Help,
    Char(char),
    Backspace,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum RolesInput {
    NotConnected,
    NotServed {
        profile: String,
    },
    Loaded(Loaded),
    LoadFailed {
        read: Read,
        error: RolesError,
    },
    CatalogLoaded(crate::model_roles::RouteCatalog),
    CatalogFailed {
        reason: String,
    },
    Key(RolesKey),
    Fresh {
        section: Section,
        reads: Vec<Loaded>,
    },
    FreshFailed {
        section: Section,
        error: RolesError,
    },
    Applied {
        step: ApplyStep,
        reply: ApplyReply,
    },
    ApplyFailed {
        step: ApplyStep,
        error: RolesError,
    },
    ReadBack {
        section: Section,
        reads: Vec<Loaded>,
    },
    ReadBackFailed {
        section: Section,
        error: RolesError,
    },
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum RolesEffect {
    LoadAll,
    Load {
        read: Read,
    },
    LoadCatalog {
        refresh: bool,
    },
    FetchFresh {
        section: Section,
    },
    Assign {
        step: ApplyStep,
        assignment: Assignment,
    },
    PutConfig {
        step: ApplyStep,
        patch: Value,
    },
    SaveMoa(MoaConfigView),
    ReadBack {
        section: Section,
    },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowView {
    pub text: String,
    pub selected: bool,
    pub dimmed: bool,
    pub marker: Option<char>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabView {
    pub label: String,
    pub current: bool,
    pub stale: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModalView {
    Prompt {
        lines: Vec<String>,
    },
    Review {
        title: String,
        lines: Vec<DiffLine>,
        footer: String,
    },
    Input {
        label: String,
        value: String,
        error: Option<String>,
    },
    Choice {
        title: String,
        options: Vec<String>,
        selected: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutePickerView {
    pub title: String,
    pub filter: String,
    pub rows: Vec<RowView>,
    pub status: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RolesView {
    pub title: String,
    pub tabs: Vec<TabView>,
    pub rows: Vec<RowView>,
    pub detail: Option<Vec<RowView>>,
    pub focus_detail: bool,
    pub notice: Vec<String>,
    pub footer: Vec<String>,
    pub modal: Option<ModalView>,
    pub picker: Option<RoutePickerView>,
    pub raw_keys: bool,
}
