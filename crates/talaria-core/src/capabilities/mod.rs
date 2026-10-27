mod install;
mod rows;
mod state;
mod text;
pub use install::{classify_install_input, redact_paths};
pub use state::CapabilitiesState;
pub use text::format_text;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CapTab {
    #[default]
    Skills,
    Hub,
    Toolsets,
    Plugins,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRef {
    pub session_id: String,
    pub profile: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillRow {
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    pub enabled: bool,
    pub usage: u64,
    pub provenance: Provenance,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provenance {
    Hub,
    Bundled,
    External,
    Local,
    Other(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HubItem {
    pub name: String,
    pub description: String,
    pub source: String,
    pub trust: Option<String>,
    pub install_key: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub severity: String,
    pub category: String,
    pub file: String,
    pub line: u64,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanReport {
    pub verdict: String,
    pub policy: ScanPolicy,
    pub policy_reason: String,
    pub source: String,
    pub trust: String,
    pub counts: [u32; 4],
    pub findings: Vec<Finding>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanPolicy {
    Allow,
    Ask,
    Block,
    Other(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolsetRow {
    pub name: String,
    pub label: String,
    pub description: String,
    pub platform: String,
    pub platform_label: String,
    pub enabled: bool,
    pub configured: bool,
    pub tools: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginServer {
    pub name: String,
    pub state: String,
    pub sentence: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginSetting {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub has_value: bool,
    pub display_value: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginRow {
    pub name: String,
    pub key: Option<String>,
    pub version: String,
    pub description: String,
    pub source: PluginSource,
    pub status: PluginStatus,
    pub servers: Vec<PluginServer>,
    pub catalog_tier: Option<String>,
    pub update_available: bool,
    pub settings: Vec<PluginSetting>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginSource {
    Bundled,
    User,
    Git,
    Entrypoint,
    Other(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginStatus {
    Enabled,
    Disabled,
    NotEnabled,
    Other(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallTarget {
    CatalogName(String),
    Repository(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallInputError;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityChange {
    Skill { name: String, enabled: bool },
    Toolset { name: String, enabled: bool },
    Plugin { key: String, enabled: bool },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAction {
    NextTab,
    PrevTab,
    SelectTab(CapTab),
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Toggle,
    Filter,
    NextPage,
    PrevPage,
    Install,
    Catalog,
    Remove,
    ShowAll,
    Refresh,
    Confirm,
    Cancel,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HubPage {
    pub items: Vec<HubItem>,
    pub page: u64,
    pub total_pages: u64,
    pub total: u64,
    pub query: Option<String>,
    pub timed_out: Vec<String>,
    pub installed: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapData {
    Skills(Vec<SkillRow>),
    Hub(HubPage),
    Toolsets(Vec<ToolsetRow>),
    Plugins(Vec<PluginRow>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapError {
    Connect,
    Ticket,
    NotLoopback,
    TlsNotSupported,
    Timeout {
        sent: bool,
    },
    Dropped {
        sent: bool,
    },
    Unauthorized,
    Forbidden {
        detail: String,
    },
    NotFound {
        route: String,
    },
    Refused {
        status: u16,
        detail: Option<String>,
    },
    Server {
        status: u16,
    },
    BodyTooLarge,
    BadBody,
    NotConnected,
    Encode,
    InvalidTimeout,
    TimedOut,
    Unknown,
    BadResult,
    Rpc {
        code: i64,
        message: String,
        reason: Option<String>,
        log: Option<String>,
    },
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activation {
    pub live_servers: Vec<String>,
    pub live_skills: Vec<String>,
    pub deferred: Vec<(String, Vec<String>)>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteReceipt {
    pub unchanged: bool,
    pub plugin: Option<PluginRow>,
    pub post_setup_started: Option<String>,
    pub restart_required: bool,
    pub activation: Activation,
    pub plugin_name: Option<String>,
    pub warnings: Vec<String>,
    pub missing_env: Vec<String>,
    pub known_issues: Vec<String>,
    pub enabled: bool,
    pub cleared_memory_provider: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginLookup {
    Found {
        name: String,
        repository: String,
        sha: Option<String>,
    },
    Unknown,
    InvalidName,
    Unavailable,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatalogEntry {
    pub name: String,
    pub title: String,
    pub description: String,
    pub tier: String,
    pub sha_short: String,
    pub installed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum CapInput {
    Open(CapTab),
    Close,
    DismissMessage,
    Key(KeyAction),
    Char(char),
    Backspace,
    SetForeground(Option<SessionRef>),
    Tick,
    SkillsChanged,
    Loaded {
        tab: CapTab,
        generation: u64,
        result: Result<CapData, CapError>,
    },
    WriteDone {
        op_id: u64,
        result: Result<WriteReceipt, CapError>,
    },
    ReadBack {
        op_id: u64,
        result: Result<CapData, CapError>,
    },
    DetailLoaded {
        tab: CapTab,
        generation: u64,
        result: Result<String, CapError>,
    },
    ScanDone(Result<ScanReport, CapError>),
    PreviewDone(Result<String, CapError>),
    LookupDone(Result<PluginLookup, CapError>),
    CatalogLoaded {
        generation: u64,
        result: Result<Vec<CatalogEntry>, CapError>,
    },
    ReloadDone(Result<String, CapError>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapEffect {
    ReadBack {
        profile: String,
        tab: CapTab,
        op_id: u64,
    },
    Fetch {
        profile: String,
        tab: CapTab,
        generation: u64,
    },
    FetchDetail {
        profile: String,
        tab: CapTab,
        key: String,
        generation: u64,
    },
    HubBrowse {
        profile: String,
        page: u64,
        generation: u64,
    },
    HubSearch {
        profile: String,
        q: String,
        generation: u64,
    },
    HubPreview {
        profile: String,
        identifier: String,
    },
    HubScan {
        profile: String,
        identifier: String,
    },
    ToggleSkill {
        profile: String,
        op_id: u64,
        name: String,
        enabled: bool,
    },
    ToggleToolset {
        profile: String,
        op_id: u64,
        name: String,
        enabled: bool,
    },
    TogglePlugin {
        profile: String,
        op_id: u64,
        key: String,
        enable: bool,
    },
    InstallSkill {
        profile: String,
        op_id: u64,
        install_key: String,
    },
    ReloadSkills {
        session_id: String,
    },
    LookupPlugin {
        profile: String,
        name: String,
    },
    InstallPlugin {
        profile: String,
        op_id: u64,
        target: InstallTarget,
    },
    RemovePlugin {
        profile: String,
        op_id: u64,
        name: String,
    },
    FetchCatalog {
        profile: String,
        generation: u64,
    },
    ScheduleTick {
        after_ms: u64,
    },
    Notice(String),
    Dispose,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapListRow {
    pub name: String,
    pub marker: String,
    pub label: String,
    pub description: String,
    pub category: Option<String>,
    pub usage: u64,
    pub version: String,
    pub tool_count: usize,
    pub server_count: usize,
    pub configured: bool,
    pub platform: String,
    pub platform_label: String,
    pub trust: String,
    pub source: String,
    pub installed: bool,
    pub update_available: bool,
    pub is_header: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapView {
    pub profile: String,
    pub tab: CapTab,
    pub title: String,
    pub tabs: Vec<String>,
    pub rows: Vec<CapListRow>,
    pub selected: usize,
    pub detail: Vec<String>,
    pub detail_focus: bool,
    pub detail_scroll: usize,
    pub message: Vec<String>,
    pub input: Option<String>,
    pub text_focus: bool,
    pub filter: String,
    pub footer: String,
    pub hints: String,
    pub open: bool,
    pub loading: bool,
    pub catalog: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapText {
    FilterPrompt,
    SearchPrompt,
    InstallPrompt,
    Loading,
    EmptySkills,
    EmptyToolsets,
    EmptyPlugins,
    EmptyHub,
    NoSearchResults,
    SlowSources,
    SkillChanged,
    ToolsetChanged,
    PostSetup,
    PluginChanged,
    Mismatch,
    Busy,
    LostAnswer,
    ConfirmSkill,
    DeniedSkill,
    InstallingSkill,
    InstalledSkill,
    InstallFailed,
    Unsupported,
    Refused,
    HubUnavailable,
    ConfirmCatalog,
    ConfirmRepository,
    InvalidInput,
    UnknownPlugin,
    CatalogUnavailable,
    InstalledPlugin,
    ConfirmRemove,
    RemovedPlugin,
    BundledRemove,
    ReadOnly,
    ProfileUnavailable,
    Truncated,
    CatalogListUnavailable,
    Unauthorized,
    Unreachable,
    MissingInstall,
    ReadTimeout,
    ReadBackFailed,
    MissingContent,
    MissingHubItem,
    PackageRemove,
    UnchangedPlugin,
    AmbiguousRemove,
    MissingChangedRow,
    BadCommand,
    TooSmall,
    FindingsTail,
    EmptyFilter,
    EmptyLog,
}
#[derive(Clone, Debug, Default)]
pub struct TextArgs {
    pub profile: String,
    pub name: String,
    pub tab: CapTab,
    pub enabled: bool,
    pub label: String,
    pub key: String,
    pub message: String,
    pub source: String,
    pub trust: String,
    pub verdict: String,
    pub reason: String,
    pub counts: [u32; 4],
    pub number: usize,
    pub query: String,
    pub sources: Vec<String>,
    pub repository: String,
    pub sha: Option<String>,
    pub activation: Activation,
    pub restart_required: bool,
    pub warnings: Vec<String>,
    pub missing_env: Vec<String>,
    pub known_issues: Vec<String>,
    pub cleared_memory_provider: bool,
    pub session_line: Option<Result<String, String>>,
}
