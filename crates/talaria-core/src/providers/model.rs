use hermes_contract::schemas::{ModelOptionProvider, ModelOptionsResult};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProvidersTab {
    #[default]
    Keys,
    Accounts,
    Endpoints,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    Tab(ProvidersTab),
    Provider { slug: String },
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvRow {
    pub env_var: String,
    pub slug: String,
    pub label: String,
    pub is_set: bool,
    pub tail: Option<String>,
    pub shared_with: Vec<(String, String)>,
    pub primary: bool,
    pub is_password: bool,
    pub channel_managed: bool,
    pub custom: bool,
    pub profile_slugs: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub device_code: bool,
    pub cli_command: Option<String>,
    pub docs_url: Option<String>,
    pub disconnect_hint: Option<String>,
    pub disconnect_command: Option<String>,
    pub disconnectable: bool,
    pub status: AccountStatus,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountStatus {
    SignedIn {
        source_label: String,
        expires_in_s: Option<i64>,
    },
    Unknown(String),
    NotSignedIn,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignInStart {
    pub session_id: String,
    pub user_code: String,
    pub verification_url: String,
    pub expires_in: Option<u64>,
    pub poll_interval: Option<i64>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct PollReply {
    pub status: PollStatus,
    pub error_message: Option<String>,
    pub account_email: Option<String>,
    pub model: Option<String>,
    pub retryable: Option<bool>,
    pub retry_after: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PollStatus {
    Pending,
    Approved,
    Denied,
    Expired,
    Error,
    Cancelled,
    Other(String),
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub models: Vec<String>,
    pub api_mode: Option<String>,
    pub context_length: Option<u64>,
    pub discover_models: bool,
    pub has_api_key: bool,
    pub is_current: bool,
    pub source: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EndpointList {
    pub endpoints: Vec<Endpoint>,
    pub current_provider: String,
    pub current_model: String,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EndpointDraft {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: ApiKeyChange,
    pub api_mode: Option<String>,
    pub context_length: Option<u64>,
    pub discover_models: bool,
    pub models: Option<Vec<String>>,
    pub model_details: Option<Vec<serde_json::Value>>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ApiKeyChange {
    #[default]
    Keep,
    Clear,
    Typed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyCheck {
    pub ok: bool,
    pub reachable: bool,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EndpointCheck {
    pub ok: bool,
    pub reachable: bool,
    pub message: String,
    pub models: Vec<String>,
    pub model_details: Vec<serde_json::Value>,
    pub resolved_base_url: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Refused { code: i64, message: String },
    Http { status: u16, detail: Option<String> },
    NotSent(String),
    BadReply,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProvidersAction {
    NextTab,
    PrevTab,
    Reload,
    Remove,
    Test,
    Save,
    ClearKey,
    OpenLink,
    Copy,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvidersKey {
    Char(char),
    Paste(String),
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
    Clear,
    CtrlU,
    Action(ProvidersAction),
}
#[derive(Clone, Debug, PartialEq)]
// Keep the generated provider result by value in the fixed service-message interface.
#[allow(clippy::large_enum_variant)]
pub enum ProvidersInput {
    Open {
        target: OpenTarget,
        profile_name: String,
        connected: bool,
    },
    Key(ProvidersKey),
    Tick,
    Connection {
        connected: bool,
    },
    Close,
    OpenFailed,
    SecretEdited {
        empty: bool,
    },
    SecretSubmitted,
    SecretCleared,
    EnvLoaded(Result<Vec<EnvRow>, Failure>),
    OptionsLoaded(Result<ModelOptionsResult, Failure>),
    OauthLoaded(Result<Vec<Account>, Failure>),
    EndpointsLoaded(Result<EndpointList, Failure>),
    KeyChecked(Result<KeyCheck, Failure>),
    KeySaved(Result<ModelOptionProvider, Failure>),
    Disconnected(Result<(), Failure>),
    SignInStarted {
        attempt: u64,
        result: Result<SignInStart, Failure>,
    },
    SignInPolled {
        session_id: String,
        result: Result<PollReply, Failure>,
    },
    SignedOut(Result<(), Failure>),
    EndpointChecked(Result<EndpointCheck, Failure>),
    EndpointSaved(Result<EndpointList, Failure>),
    EndpointDeleted(Result<EndpointList, Failure>),
}
#[derive(Clone, Debug, PartialEq)]
pub enum ProvidersEffect {
    LoadEnv,
    LoadOptions,
    LoadOauth,
    LoadEndpoints,
    CheckKey {
        env_var: String,
    },
    SaveKey {
        slug: String,
    },
    ForgetSecret,
    Disconnect {
        slug: String,
    },
    StartSignIn {
        id: String,
        attempt: u64,
    },
    Poll {
        id: String,
        session_id: String,
        after: Duration,
    },
    CancelSignIn {
        session_id: String,
    },
    SignOut {
        id: String,
    },
    CheckEndpoint(EndpointDraft),
    SaveEndpoint(EndpointDraft),
    DeleteEndpoint {
        id: String,
    },
    OpenUrl(String),
    CopyText(String),
    Close,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardKind {
    Key,
    External,
    Waiting,
    Ended,
    RemoveConfirm,
    SignOutConfirm,
    DisconnectHint,
    EndpointForm,
    DeleteConfirm,
}
#[derive(Clone, PartialEq, Eq)]
pub struct CardView {
    pub kind: CardKind,
    pub lines: Vec<String>,
    pub footer: String,
}
pub type ProvidersCard = CardView;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvidersView {
    pub title: String,
    pub tab: ProvidersTab,
    pub rows: Vec<String>,
    pub selected: Option<usize>,
    pub filter: String,
    pub card: Option<ProvidersCard>,
    pub notice: Option<String>,
    pub banner: Option<String>,
    pub secret_focus: bool,
    pub footer: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Load {
    pub running: bool,
    pub again: bool,
    pub done: bool,
    pub error: Option<Failure>,
}
#[derive(Clone, Debug)]
pub(crate) struct KeyCard {
    pub row: EnvRow,
    pub empty: bool,
    pub unchecked: bool,
    pub message: Option<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Form {
    pub draft: EndpointDraft,
    pub field: usize,
    pub context: String,
    pub dirty: bool,
    pub cleared: bool,
    pub result: Option<EndpointCheck>,
    pub original: Option<Endpoint>,
    pub mode_changed: bool,
}
#[derive(Clone, Debug)]
pub(crate) enum Card {
    Key(KeyCard),
    Confirm {
        title: String,
        effect: ProvidersEffect,
    },
    External {
        title: String,
        lines: Vec<String>,
        copy: Option<String>,
    },
    Starting {
        account: Account,
        attempt: u64,
    },
    Waiting {
        account: Account,
        start: SignInStart,
        expires: Option<Instant>,
        repaint: Instant,
        failures: u8,
    },
    Approved {
        account: Account,
        text: String,
    },
    Ended {
        account: Account,
        text: String,
        retryable: bool,
        retry_at: Option<Instant>,
    },
    Form(Form),
    Discard(Form),
}
#[derive(Clone, Debug)]
pub(crate) enum Busy {
    CheckKey,
    DiscardedKeyCheck,
    SaveKey(EnvRow),
    Disconnect(String),
    Start(u64),
    SignOut(String),
    CheckEndpoint,
    DiscardedEndpointCheck,
    SaveEndpoint(String),
    DeleteEndpoint(String),
}
#[derive(Clone, Default)]
pub struct ProvidersState {
    pub(crate) open: bool,
    pub(crate) profile: String,
    pub(crate) connected: bool,
    pub(crate) tab: ProvidersTab,
    pub(crate) filter: String,
    pub(crate) selected: usize,
    pub(crate) env: Vec<EnvRow>,
    pub(crate) keys: Vec<EnvRow>,
    pub(crate) options: Option<ModelOptionsResult>,
    pub(crate) accounts: Vec<Account>,
    pub(crate) endpoints: EndpointList,
    pub(crate) loads: [Load; 4],
    pub(crate) card: Option<Card>,
    pub(crate) busy: Option<Busy>,
    pub(crate) target: Option<String>,
    pub(crate) notice: Option<String>,
    pub(crate) attempt: u64,
}

impl std::fmt::Debug for PollReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PollReply")
            .field("status", &self.status)
            .field("error_message", &self.error_message)
            .field("account_email", &"[redacted]")
            .field("model", &self.model)
            .field("retryable", &self.retryable)
            .field("retry_after", &self.retry_after)
            .finish()
    }
}
impl std::fmt::Debug for CardView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("CardView");
        debug.field("kind", &self.kind);
        if self.kind == CardKind::Ended {
            debug.field("lines", &"[screen only]");
        } else {
            debug.field("lines", &self.lines);
        }
        debug.field("footer", &self.footer).finish()
    }
}
impl std::fmt::Debug for ProvidersState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvidersState")
            .field("open", &self.open)
            .field("tab", &self.tab)
            .field("connected", &self.connected)
            .field("selected", &self.selected)
            .field("card_open", &self.card.is_some())
            .field("busy", &self.busy)
            .finish_non_exhaustive()
    }
}
impl ProvidersState {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
}
