use hermes_contract::schemas::{
    McpCatalogEntry, McpOauthPollResult, McpServerRuntimeRow, McpServerSummary,
    McpServersTestResult, ReloadMcpResult,
};
use std::time::Instant;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpTab {
    Servers,
    Catalogue,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpOpen {
    Tab(McpTab),
    Apply,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretSlot {
    Token,
    EnvValue(u8), /* 0..8 */
    KeyValue,
    Paste,
}
#[derive(Debug)]
pub enum McpAction {
    NextTab,
    PrevTab,
    Reload,
    Add,
    Test,
    Toggle,
    SetKey,
    SignIn,
    Remove,
    Apply,
    Save,
    OpenLink,
    Copy,
    PasteRedirect,
}
#[derive(Debug)]
pub enum McpKey {
    Char(char),
    Paste(String), /* never while secret_focus is Some */
    Enter,
    Esc,
    Tab,
    BackTab,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Left,
    Right,
    Backspace,
    Action(McpAction),
}
#[derive(Debug)]
pub enum TransportKind {
    Remote,
    Local,
    Unknown,
}
#[derive(Debug)]
pub struct ServerRow {
    pub summary: McpServerSummary,
    pub runtime: Option<McpServerRuntimeRow>,
}
#[derive(Debug)]
pub enum TestOutcome {
    Running {
        since: Instant,
    },
    Ok {
        tools: Vec<(String, String)>,
        prompts: u64,
        resources: u64,
    },
    NeedsSignIn,
    Failed {
        error: String,
    },
}
#[derive(Debug)]
pub enum StateLabel {
    Off,
    NeedsSignIn,
    Connected(u64),
    Lazy(u64),
    Connecting,
    Failed,
    ConfiguredNotReported,
    Unknown,
}
#[derive(Debug)]
pub struct RedirectParts<'a> {
    pub code: Option<&'a str>,
    pub state: &'a str,
    pub error: Option<&'a str>,
    pub iss: Option<&'a str>,
}
#[derive(Debug)]
pub enum RedirectError {
    NotUrl,
    NotHttp,
    WrongHost,
    WrongPath,
    NoState,
    NoCodeOrError,
}
#[derive(Debug)]
pub enum NameError {
    Empty,
    Invalid,
    Taken,
}
#[derive(Debug)]
pub enum Failure {
    Rpc {
        code: i64,
        message: String,
        reason: Option<String>,
        detail: Option<String>,
    },
    Http {
        status: u16,
        detail: Option<String>,
    },
    NotSent(String),
    Unknown,
    BadReply,
    BadPaste(RedirectError),
}
#[derive(Debug)]
pub enum McpInput {
    Open {
        open: McpOpen,
        profile_name: String,
        connected: bool,
        over_ssh: bool,
        foreground: Option<String>,
    },
    Key(McpKey),
    Tick,
    Connection {
        connected: bool,
    },
    Foreground(Option<String>),
    Close,
    OpenFailed,
    SecretEdited {
        slot: SecretSlot,
        empty: bool,
    },
    SecretSubmitted {
        slot: SecretSlot,
    },
    ListLoaded(Result<Vec<McpServerSummary>, Failure>),
    StatusLoaded(Result<Vec<McpServerRuntimeRow>, Failure>),
    CatalogLoaded(Result<Vec<McpCatalogEntry>, Failure>),
    Added {
        name: String,
        result: Result<McpServerSummary, Failure>,
    },
    KeySaved {
        name: String,
        env_var: Option<String>,
        result: Result<String /* env_var */, Failure>,
    },
    Tested {
        name: String,
        result: Result<McpServersTestResult, Failure>,
    },
    Toggled {
        name: String,
        result: Result<bool, Failure>,
    },
    Removed {
        name: String,
        result: Result<(), Failure>,
    },
    SignInStarted {
        name: String,
        result: Result<(String /* flow id */, String /* auth_url */), Failure>,
    },
    Polled {
        name: String,
        result: Result<McpOauthPollResult, Failure>,
    },
    CallbackRelayed {
        name: String,
        result: Result<(), Failure>,
    },
    Reloaded(Result<ReloadMcpResult, Failure>),
}
#[derive(Debug)]
pub enum McpEffect {
    LoadList,
    LoadStatus,
    LoadCatalog,
    AddServer {
        name: String,
        preset: Option<String>,
        config: Option<serde_json::Map<String, serde_json::Value>>,
        token: bool,
    },
    SetKey {
        name: String,
        env_var: Option<String>,
        slot: SecretSlot,
    },
    Test {
        name: String,
    },
    SetEnabled {
        name: String,
        enabled: bool,
    },
    Remove {
        name: String,
    },
    StartSignIn {
        name: String,
    },
    Poll {
        name: String,
        flow: String,
    },
    CancelSignIn {
        name: String,
        flow: String,
    },
    RelayCallback {
        name: String,
        flow: String,
    },
    Reload {
        session_id: Option<String>,
        confirm: bool,
    },
    ForgetSecrets,
    OpenUrl(String),
    CopyText(String),
    Close,
}
impl McpEffect {
    pub fn secret_slot(&self) -> Option<SecretSlot> {
        todo!("mcp stub")
    }
}
#[derive(Debug, Default)]
pub struct McpState {}
#[derive(Debug)]
pub struct McpView {
    pub title: String,
    pub tab: McpTab,
    pub banner: Option<String>,
    pub filter: String,
    pub placeholder: Option<String>,
    pub rows: Vec<RowView>,
    pub selected: Option<usize>,
    pub status_line: Option<String>,
    pub card: Option<CardView>,
    pub pending_line: Option<String>,
    pub notice: Option<String>,
    pub footer: String,
    pub secret_focus: Option<SecretSlot>,
}
#[derive(Debug)]
pub struct RowView {
    pub name: String,
    pub kind: String,
    pub label: String,
    pub dim: bool,
}
#[derive(Debug)]
pub struct CardView {
    pub title: String,
    pub lines: Vec<String>,
    pub fields: Vec<FieldView>,
    pub footer: String,
}
#[derive(Debug)]
pub struct FieldView {
    pub label: String,
    pub text: String, /* empty for a secret */
    pub secret: Option<SecretSlot>,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub focused: bool,
}
impl McpView {
    pub fn text_lines(&self) -> Vec<String> {
        todo!("mcp stub")
    }
}
pub fn update(_state: &mut McpState, _input: McpInput, _now: Instant) -> Vec<McpEffect> {
    todo!("mcp stub")
}
pub fn view(_state: &McpState, _now: Instant) -> McpView {
    todo!("mcp stub")
}
pub fn next_deadline(_state: &McpState) -> Option<Instant> {
    todo!("mcp stub")
}
pub fn parse_mcp_args(_args: &str) -> Result<McpOpen, &'static str> {
    todo!("mcp stub")
}
pub fn over_ssh(_var: impl Fn(&str) -> Option<String>) -> bool {
    todo!("mcp stub")
}
pub fn transport_kind(_raw: &str) -> TransportKind {
    todo!("mcp stub")
}
pub fn server_state(_row: &ServerRow, _test: Option<&TestOutcome>) -> StateLabel {
    todo!("mcp stub")
}
pub fn validate_server_name(_input: &str, _existing: &[String]) -> Result<String, NameError> {
    todo!("mcp stub")
}
pub fn env_key_for_server(_name: &str) -> String {
    todo!("mcp stub")
}
pub fn parse_redirect(_pasted: &str) -> Result<RedirectParts<'_>, RedirectError> {
    todo!("mcp stub")
}
