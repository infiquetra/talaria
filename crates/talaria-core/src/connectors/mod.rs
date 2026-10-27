//! Connectors and messaging overlay state. Pure: no client and no terminal.

#![allow(clippy::large_enum_variant)]

mod accounts;
mod link;
mod messaging;
mod pairing;
mod rows;
mod rules;
mod text;

use std::time::Instant;

pub use accounts::{AccountLine, AccountRow, AccountView, AliasProblem, account_view, alias_problem};
pub use link::{AccountAddress, Operation, Target};
pub use messaging::{
    Drafts, EnvVar, HomeChannel, PlatformRow, PlatformsReply, TestReply, UpdateReply,
    parse_list, platform_label, review_rows,
};
pub use pairing::{ApprovedPair, Pairing, PendingPair};
pub use rows::{CatalogReply, CatalogRow, ConnectorRow, ListReply, connector_label};
pub use rules::{
    EffectivePolicy, Policy, PolicyBody, PolicyLayer, PolicySetReply, PolicyTags, ToolSpec,
    ToolsReply,
};

/// Which tab is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectorsTab {
    Connectors,
    Messaging,
}

/// How the overlay was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectorsOpen {
    Tab(ConnectorsTab),
}

/// Keys the connectors map produces. Paste is an input, not a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConnectorsKey {
    NextTab,
    PrevTab,
    Up,
    Down,
    Open,
    Toggle,
    Filter,
    Reload,
    Add,
    Rename,
    ReconnectOrCopy,
    Remove,
    ToolsOrTest,
    Retired,
    OpenLink,
    Wake,
    ClearField,
    Advanced,
    Approve,
    Save,
    Doctor,
    Confirm,
    Cancel,
    Close,
    Char(char),
    Backspace,
}

/// A name prompt, a list editor, or a confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prompt {
    Alias { slug: String, text: String },
    Rename { connection_id: String, text: String },
    List { key: String, text: String },
    Confirm(ConfirmKind),
}

/// Which confirmation is on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmKind {
    RemoveAccount,
    DiscardTools,
    SaveTools,
    SavePlatform,
    DiscardPlatform,
    TogglePlatform,
    ApprovePairing,
    RevokePairing,
}

/// Which write holds the single slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteKind {
    Connect,
    Rename,
    Remove,
    SetConnector,
    SetTools,
    UpdatePlatform,
    Approve,
    Revoke,
}

/// Refusal kept with the gateway's reason code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectorsFailure {
    Refused {
        code: i64,
        reason: Option<String>,
        message: String,
    },
    Http { status: u16, detail: Option<String> },
    NotSent(String),
    Unknown,
    BadReply,
}

/// Removed account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedReply {
    pub connection_id: String,
}

/// Overlay state. Debug prints counts, never labels, links, names, or list entries.
#[derive(Clone, PartialEq)]
pub struct ConnectorsState {
    profile: String,
    tab: ConnectorsTab,
    consented: bool,
    connectors: Vec<ConnectorRow>,
    accounts: Vec<AccountRow>,
    platforms: Vec<PlatformRow>,
    pairing: Pairing,
    link: Option<String>,
    write: Option<WriteKind>,
    filter: String,
    prompt: Option<Prompt>,
}

impl std::fmt::Debug for ConnectorsState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectorsState")
            .field("profile", &self.profile)
            .field("tab", &self.tab)
            .field("consented", &self.consented)
            .field("connectors", &self.connectors.len())
            .field("accounts", &self.accounts.len())
            .field("platforms", &self.platforms.len())
            .field("pending", &self.pairing.pending.len())
            .field("approved", &self.pairing.approved.len())
            .field("link", &self.link.as_ref().map(|_| "[link]"))
            .field("write", &self.write)
            .field("filter_len", &self.filter.len())
            .field("prompt", &self.prompt.is_some())
            .finish()
    }
}

impl ConnectorsState {
    /// An empty overlay. Nothing has been loaded and consent is not recorded here.
    #[must_use]
    pub fn new() -> Self {
        Self {
            profile: String::new(),
            tab: ConnectorsTab::Connectors,
            consented: false,
            connectors: Vec::new(),
            accounts: Vec::new(),
            platforms: Vec::new(),
            pairing: Pairing::default(),
            link: None,
            write: None,
            filter: String::new(),
            prompt: None,
        }
    }
}

impl Default for ConnectorsState {
    fn default() -> Self {
        Self::new()
    }
}

/// Something that happened to the overlay.
#[derive(Clone, PartialEq)]
pub enum ConnectorsInput {
    Open {
        open: ConnectorsOpen,
        profile: String,
        consented: bool,
    },
    Key(ConnectorsKey),
    Paste(String),
    ListReply(Result<ListReply, ConnectorsFailure>),
    CatalogReply(Result<Vec<CatalogRow>, ConnectorsFailure>),
    AccountsReply(Result<Vec<AccountRow>, ConnectorsFailure>),
    PolicyReply(Result<Policy, ConnectorsFailure>),
    ToolsReply {
        slug: String,
        reply: Result<ToolsReply, ConnectorsFailure>,
    },
    ConnectReply(Result<Operation, ConnectorsFailure>),
    OperationReply {
        op_id: String,
        reply: Result<Operation, ConnectorsFailure>,
    },
    WakeReply(Result<(), ConnectorsFailure>),
    RenameReply(Result<AccountRow, ConnectorsFailure>),
    RemoveReply(Result<RemovedReply, ConnectorsFailure>),
    PolicySetReply(Result<PolicySetReply, ConnectorsFailure>),
    ConnectionUpdate {
        op_id: String,
        owner_is_account: bool,
    },
    PlatformsReply(Result<Vec<PlatformRow>, ConnectorsFailure>),
    PairingReply(Result<Pairing, ConnectorsFailure>),
    UpdateReply {
        id: String,
        reply: Result<UpdateReply, ConnectorsFailure>,
    },
    TestReply {
        id: String,
        reply: Result<TestReply, ConnectorsFailure>,
    },
    ApproveReply(Result<(), ConnectorsFailure>),
    RevokeReply(Result<(), ConnectorsFailure>),
    PlatformsChanged,
    PairingChanged,
    SecretEdited { key: String, empty: bool },
    Tick,
    Reconnected,
}

impl std::fmt::Debug for ConnectorsInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open {
                open,
                profile,
                consented,
            } => formatter
                .debug_struct("Open")
                .field("open", open)
                .field("profile", profile)
                .field("consented", consented)
                .finish(),
            Self::Key(key) => formatter.debug_tuple("Key").field(key).finish(),
            Self::Paste(text) => formatter.debug_struct("Paste").field("len", &text.len()).finish(),
            Self::ListReply(reply) => write!(formatter, "ListReply({})", reply.is_ok()),
            Self::CatalogReply(reply) => write!(formatter, "CatalogReply({})", reply.is_ok()),
            Self::AccountsReply(reply) => write!(formatter, "AccountsReply({})", reply.is_ok()),
            Self::PolicyReply(reply) => write!(formatter, "PolicyReply({})", reply.is_ok()),
            Self::ToolsReply { slug, reply } => formatter
                .debug_struct("ToolsReply")
                .field("slug", slug)
                .field("ok", &reply.is_ok())
                .finish(),
            Self::ConnectReply(reply) => write!(formatter, "ConnectReply({})", reply.is_ok()),
            Self::OperationReply { op_id, reply } => formatter
                .debug_struct("OperationReply")
                .field("op_id", op_id)
                .field("ok", &reply.is_ok())
                .finish(),
            Self::WakeReply(reply) => write!(formatter, "WakeReply({})", reply.is_ok()),
            Self::RenameReply(reply) => write!(formatter, "RenameReply({})", reply.is_ok()),
            Self::RemoveReply(reply) => write!(formatter, "RemoveReply({})", reply.is_ok()),
            Self::PolicySetReply(reply) => write!(formatter, "PolicySetReply({})", reply.is_ok()),
            Self::ConnectionUpdate {
                op_id,
                owner_is_account,
            } => formatter
                .debug_struct("ConnectionUpdate")
                .field("op_id", op_id)
                .field("owner_is_account", owner_is_account)
                .finish(),
            Self::PlatformsReply(reply) => write!(formatter, "PlatformsReply({})", reply.is_ok()),
            Self::PairingReply(reply) => write!(formatter, "PairingReply({})", reply.is_ok()),
            Self::UpdateReply { id, reply } => formatter
                .debug_struct("UpdateReply")
                .field("id", id)
                .field("ok", &reply.is_ok())
                .finish(),
            Self::TestReply { id, reply } => formatter
                .debug_struct("TestReply")
                .field("id", id)
                .field("ok", &reply.is_ok())
                .finish(),
            Self::ApproveReply(reply) => write!(formatter, "ApproveReply({})", reply.is_ok()),
            Self::RevokeReply(reply) => write!(formatter, "RevokeReply({})", reply.is_ok()),
            Self::PlatformsChanged => formatter.write_str("PlatformsChanged"),
            Self::PairingChanged => formatter.write_str("PairingChanged"),
            Self::SecretEdited { key, empty } => formatter
                .debug_struct("SecretEdited")
                .field("key", key)
                .field("empty", empty)
                .finish(),
            Self::Tick => formatter.write_str("Tick"),
            Self::Reconnected => formatter.write_str("Reconnected"),
        }
    }
}

/// What the overlay asks the application to do.
#[derive(Clone, PartialEq, Eq)]
pub enum ConnectorsEffect {
    GrantConsent,
    ListConnectors,
    LoadCatalog,
    LoadAccounts,
    LoadPolicy,
    LoadTools { slug: String, refresh: bool },
    Connect { slug: String, address: AccountAddress },
    PollOperation { op_id: String },
    WakeOperation { op_id: String },
    RenameAccount { connection_id: String, alias: String },
    RemoveAccount { connection_id: String },
    SetConnector {
        slug: String,
        enabled: bool,
        expected_revision: String,
    },
    SetTools {
        slug: String,
        disabled_tools: Vec<String>,
        expected_revision: String,
    },
    LoadPlatforms,
    LoadPairing,
    UpdatePlatform {
        id: String,
        enabled: Option<bool>,
        typed_keys: Vec<String>,
        lists: std::collections::BTreeMap<String, Vec<String>>,
        clear_env: Vec<String>,
    },
    TestPlatform { id: String },
    ApprovePairing { platform: String, request_id: String },
    RevokePairing { platform: String, user_id: String },
    OpenUrl(String),
    CopyText(String),
    OpenDoctor,
    EditSecret { key: String },
    ForgetSecrets,
    Notice(String),
    Close,
}

impl std::fmt::Debug for ConnectorsEffect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GrantConsent => formatter.write_str("GrantConsent"),
            Self::ListConnectors => formatter.write_str("ListConnectors"),
            Self::LoadCatalog => formatter.write_str("LoadCatalog"),
            Self::LoadAccounts => formatter.write_str("LoadAccounts"),
            Self::LoadPolicy => formatter.write_str("LoadPolicy"),
            Self::LoadTools { slug, refresh } => formatter
                .debug_struct("LoadTools")
                .field("slug", slug)
                .field("refresh", refresh)
                .finish(),
            Self::Connect { slug, address } => formatter
                .debug_struct("Connect")
                .field("slug", slug)
                .field("address", address)
                .finish(),
            Self::PollOperation { op_id } => formatter.debug_struct("PollOperation").field("op_id", op_id).finish(),
            Self::WakeOperation { op_id } => formatter.debug_struct("WakeOperation").field("op_id", op_id).finish(),
            Self::RenameAccount { connection_id, alias } => formatter
                .debug_struct("RenameAccount")
                .field("connection_len", &connection_id.len())
                .field("alias_len", &alias.len())
                .finish(),
            Self::RemoveAccount { connection_id } => formatter
                .debug_struct("RemoveAccount")
                .field("connection_len", &connection_id.len())
                .finish(),
            Self::SetConnector {
                slug,
                enabled,
                expected_revision,
            } => formatter
                .debug_struct("SetConnector")
                .field("slug", slug)
                .field("enabled", enabled)
                .field("expected_revision", expected_revision)
                .finish(),
            Self::SetTools {
                slug,
                disabled_tools,
                expected_revision,
            } => formatter
                .debug_struct("SetTools")
                .field("slug", slug)
                .field("disabled_tools", disabled_tools)
                .field("expected_revision", expected_revision)
                .finish(),
            Self::LoadPlatforms => formatter.write_str("LoadPlatforms"),
            Self::LoadPairing => formatter.write_str("LoadPairing"),
            Self::UpdatePlatform {
                id,
                enabled,
                typed_keys,
                lists,
                clear_env,
            } => formatter
                .debug_struct("UpdatePlatform")
                .field("id", id)
                .field("enabled", enabled)
                .field("typed_keys", typed_keys)
                .field(
                    "lists",
                    &lists
                        .iter()
                        .map(|(key, values)| (key.clone(), values.len()))
                        .collect::<std::collections::BTreeMap<_, _>>(),
                )
                .field("clear_env", clear_env)
                .finish(),
            Self::TestPlatform { id } => formatter.debug_struct("TestPlatform").field("id", id).finish(),
            Self::ApprovePairing { platform, request_id } => formatter
                .debug_struct("ApprovePairing")
                .field("platform", platform)
                .field("request_len", &request_id.len())
                .finish(),
            Self::RevokePairing { platform, user_id } => formatter
                .debug_struct("RevokePairing")
                .field("platform", platform)
                .field("user_len", &user_id.len())
                .finish(),
            Self::OpenUrl(_) => formatter.write_str("OpenUrl([link])"),
            Self::CopyText(_) => formatter.write_str("CopyText([link])"),
            Self::OpenDoctor => formatter.write_str("OpenDoctor"),
            Self::EditSecret { key } => formatter.debug_struct("EditSecret").field("key", key).finish(),
            Self::ForgetSecrets => formatter.write_str("ForgetSecrets"),
            Self::Notice(text) => formatter.debug_tuple("Notice").field(text).finish(),
            Self::Close => formatter.write_str("Close"),
        }
    }
}

/// What the screen draws.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectorsView {
    pub title: String,
    pub tab: ConnectorsTab,
    pub screen: Screen,
    pub notice: Option<Line>,
    pub footer: Vec<(String, String)>,
}

/// One painted line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub tone: Tone,
}

/// Colour of a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Warning,
    Error,
}

/// The body of the overlay.
#[derive(Clone, Debug, PartialEq)]
pub enum Screen {
    Consent { text: String },
    Loading,
    Unavailable { text: String, command: Option<String> },
    ConnectorList { rows: Vec<Row>, selected: usize, filter: Option<String> },
    ConnectorCard {
        title: String,
        lines: Vec<Line>,
        accounts: Vec<Row>,
        retired: usize,
        selected: usize,
    },
    LinkCard {
        app: String,
        link: Option<String>,
        left: Option<String>,
        ssh: bool,
        starting: bool,
    },
    Tools {
        header: String,
        cached: bool,
        rows: Vec<ToolRow>,
        selected: usize,
    },
    PlatformList {
        rows: Vec<Row>,
        selected: usize,
        restart_line: bool,
        other_pairing: Vec<String>,
    },
    PlatformCard {
        lines: Vec<Line>,
        fields: Vec<FieldRow>,
        pairing: Vec<Row>,
        selected: usize,
    },
    Review { title: String, rows: Vec<crate::settings::ReviewRow> },
    Confirm { text: String },
    Prompt { title: String, text: String, help: Option<String>, problem: Option<String> },
    SecretEditor { key: String, title: String, help: Option<String> },
}

/// A list row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub label: String,
    pub state: String,
    pub marker: Option<char>,
}

/// A tool row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolRow {
    pub name: String,
    pub facet: String,
    pub state: ToolState,
    pub draft: bool,
    pub deprecated: bool,
}

/// Whether a tool is on, off, or locked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolState {
    On,
    Off,
    Locked,
}

/// One field on a platform card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldRow {
    pub key: String,
    pub prompt: String,
    pub required: bool,
    pub value: FieldValue,
}

/// What a field shows. A typed secret is never the characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldValue {
    Set,
    NotSet,
    List(Vec<String>),
    Draft,
    WillClear,
}

/// Apply one input. The clock is the caller's.
#[must_use]
pub fn update(
    state: &mut ConnectorsState,
    input: ConnectorsInput,
    now: Instant,
) -> Vec<ConnectorsEffect> {
    let _ = (state, input, now);
    todo!("connectors stub")
}

/// Draw data for the current state.
#[must_use]
pub fn view(state: &ConnectorsState, now: Instant) -> ConnectorsView {
    let _ = (state, now);
    todo!("connectors stub")
}

/// When the screen should wake. Only a link wait has one.
#[must_use]
pub fn next_deadline(state: &ConnectorsState) -> Option<Instant> {
    let _ = state;
    todo!("connectors stub")
}
