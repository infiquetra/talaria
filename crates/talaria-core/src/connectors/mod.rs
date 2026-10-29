//! Connectors and messaging overlay state. Pure: no client and no terminal.

#![allow(clippy::large_enum_variant)]

mod accounts;
mod link;
mod messaging;
mod pairing;
mod projection;
mod reducer;
mod rows;
mod rules;
mod text;

use std::time::Instant;

pub use accounts::{
    AccountLine, AccountRow, AccountView, AliasProblem, account_view, alias_problem,
};
pub use link::{AccountAddress, Operation, Target};
pub use messaging::{
    Drafts, EnvVar, HomeChannel, PlatformRow, PlatformsReply, TestReply, UpdateReply, parse_list,
    platform_label, review_rows,
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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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

impl std::fmt::Debug for ConnectorsKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Char(_) => "Char([redacted])",
            Self::NextTab => "NextTab",
            Self::PrevTab => "PrevTab",
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Open => "Open",
            Self::Toggle => "Toggle",
            Self::Filter => "Filter",
            Self::Reload => "Reload",
            Self::Add => "Add",
            Self::Rename => "Rename",
            Self::ReconnectOrCopy => "ReconnectOrCopy",
            Self::Remove => "Remove",
            Self::ToolsOrTest => "ToolsOrTest",
            Self::Retired => "Retired",
            Self::OpenLink => "OpenLink",
            Self::Wake => "Wake",
            Self::ClearField => "ClearField",
            Self::Advanced => "Advanced",
            Self::Approve => "Approve",
            Self::Save => "Save",
            Self::Doctor => "Doctor",
            Self::Confirm => "Confirm",
            Self::Cancel => "Cancel",
            Self::Close => "Close",
            Self::Backspace => "Backspace",
        })
    }
}

/// A name prompt, a list editor, or a confirmation.
#[derive(Clone, PartialEq, Eq)]
pub enum Prompt {
    Alias { slug: String, text: String },
    Rename { connection_id: String, text: String },
    List { key: String, text: String },
    Confirm(ConfirmKind),
}

impl std::fmt::Debug for Prompt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Alias { slug, text } => formatter
                .debug_struct("Alias")
                .field("slug", slug)
                .field("text_len", &text.len())
                .finish(),
            Self::Rename { text, .. } => formatter
                .debug_struct("Rename")
                .field("text_len", &text.len())
                .finish(),
            Self::List { key, text } => formatter
                .debug_struct("List")
                .field("key", key)
                .field("text_len", &text.len())
                .finish(),
            Self::Confirm(kind) => formatter.debug_tuple("Confirm").field(kind).finish(),
        }
    }
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
#[derive(Clone, PartialEq, Eq)]
pub enum ConnectorsFailure {
    Refused {
        code: i64,
        reason: Option<String>,
        message: String,
    },
    Http {
        status: u16,
        detail: Option<String>,
    },
    NotSent(String),
    Unknown,
    BadReply,
}

impl std::fmt::Debug for ConnectorsFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused { code, reason, .. } => formatter
                .debug_struct("Refused")
                .field("code", code)
                .field("reason", reason)
                .finish(),
            Self::Http { status, .. } => formatter
                .debug_struct("Http")
                .field("status", status)
                .finish(),
            Self::NotSent(_) => formatter.write_str("NotSent([detail])"),
            Self::Unknown => formatter.write_str("Unknown"),
            Self::BadReply => formatter.write_str("BadReply"),
        }
    }
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
    pending_toggle: Option<PendingToggle>,
    pane: Pane,
    closed: bool,
    opened_tab: ConnectorsTab,
    loaded: bool,
    list_followups: bool,
    platforms_loaded: bool,
    catalog: Vec<CatalogRow>,
    policy: Option<Policy>,
    policy_error: Option<String>,
    accounts_error: Option<String>,
    pairing_error: Option<String>,
    unavailable: Option<(String, bool)>,
    notice: Option<Line>,
    selected: usize,
    card_selected: usize,
    review_scroll: usize,
    show_retired: bool,
    advanced: bool,
    filtering: bool,
    tools: Option<ToolsReply>,
    tool_drafts: std::collections::BTreeMap<String, bool>,
    drafts: std::collections::BTreeMap<String, Drafts>,
    draft_sequence: u64,
    draft_versions: std::collections::BTreeMap<(String, String), u64>,
    submitted_versions: std::collections::BTreeMap<String, u64>,
    changed_fields: std::collections::BTreeSet<String>,
    operation: Option<Operation>,
    next_poll: Option<Instant>,
    next_frame: Option<Instant>,
    poll_in_flight: bool,
    wake_in_flight: bool,
    wake_poll_pending: bool,
    expired_polled: bool,
    connect_slug: Option<String>,
    connect_alias: Option<String>,
    write_account: Option<AccountRow>,
    write_slug: Option<String>,
    write_platform: Option<String>,
    toggle_enabled: Option<bool>,
    awaiting: u8,
    last_platform_signal: Option<Instant>,
    last_pairing_signal: Option<Instant>,
    testing: std::collections::BTreeSet<String>,
    secret_editor: Option<String>,
    secret_edit_empty: bool,
    copy_command: Option<String>,
    restart_notice: bool,
    gateway_start_command: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
struct PendingToggle {
    id: String,
    name: String,
    enabled: bool,
}

#[derive(Clone, PartialEq, Eq)]
enum Pane {
    List,
    Connector(String),
    Tools(String),
    Platform(String),
    Link,
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
    /// Whether the list filter currently owns printable input.
    pub fn is_filtering(&self) -> bool {
        self.filtering
    }
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
            pending_toggle: None,
            pane: Pane::List,
            closed: false,
            opened_tab: ConnectorsTab::Connectors,
            loaded: false,
            list_followups: false,
            platforms_loaded: false,
            catalog: Vec::new(),
            policy: None,
            policy_error: None,
            accounts_error: None,
            pairing_error: None,
            unavailable: None,
            notice: None,
            selected: 0,
            card_selected: 0,
            review_scroll: 0,
            show_retired: false,
            advanced: false,
            filtering: false,
            tools: None,
            tool_drafts: std::collections::BTreeMap::new(),
            drafts: std::collections::BTreeMap::new(),
            draft_sequence: 0,
            draft_versions: std::collections::BTreeMap::new(),
            submitted_versions: std::collections::BTreeMap::new(),
            changed_fields: std::collections::BTreeSet::new(),
            operation: None,
            next_poll: None,
            next_frame: None,
            poll_in_flight: false,
            wake_in_flight: false,
            wake_poll_pending: false,
            expired_polled: false,
            connect_slug: None,
            connect_alias: None,
            write_account: None,
            write_slug: None,
            write_platform: None,
            toggle_enabled: None,
            awaiting: 0,
            last_platform_signal: None,
            last_pairing_signal: None,
            testing: std::collections::BTreeSet::new(),
            secret_editor: None,
            secret_edit_empty: true,
            copy_command: None,
            restart_notice: false,
            gateway_start_command: None,
        }
    }
}

impl Default for ConnectorsState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectorsState {
    /// A submitted write keeps its slot until its required readbacks answer.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.write.is_some()
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
    /// A viewport-relative review position computed by the presentation layer.
    ReviewScroll(usize),
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
    PlatformsLoaded(Result<PlatformsReply, ConnectorsFailure>),
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
    SecretEdited {
        key: String,
        empty: bool,
    },
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
            Self::ReviewScroll(offset) => {
                formatter.debug_tuple("ReviewScroll").field(offset).finish()
            }
            Self::Paste(text) => formatter
                .debug_struct("Paste")
                .field("len", &text.len())
                .finish(),
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
            Self::PlatformsLoaded(reply) => write!(formatter, "PlatformsLoaded({})", reply.is_ok()),
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
    LoadTools {
        slug: String,
        refresh: bool,
    },
    Connect {
        slug: String,
        address: AccountAddress,
    },
    PollOperation {
        op_id: String,
    },
    WakeOperation {
        op_id: String,
    },
    RenameAccount {
        connection_id: String,
        alias: String,
    },
    RemoveAccount {
        connection_id: String,
    },
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
    TestPlatform {
        id: String,
    },
    ApprovePairing {
        platform: String,
        request_id: String,
    },
    RevokePairing {
        platform: String,
        user_id: String,
    },
    OpenUrl(String),
    CopyText(String),
    OpenDoctor,
    EditSecret {
        key: String,
    },
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
            Self::PollOperation { op_id } => formatter
                .debug_struct("PollOperation")
                .field("op_id", op_id)
                .finish(),
            Self::WakeOperation { op_id } => formatter
                .debug_struct("WakeOperation")
                .field("op_id", op_id)
                .finish(),
            Self::RenameAccount {
                connection_id,
                alias,
            } => formatter
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
            Self::TestPlatform { id } => formatter
                .debug_struct("TestPlatform")
                .field("id", id)
                .finish(),
            Self::ApprovePairing {
                platform,
                request_id,
            } => formatter
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
            Self::EditSecret { key } => formatter
                .debug_struct("EditSecret")
                .field("key", key)
                .finish(),
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
    pub review_scroll: usize,
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
#[derive(Clone, PartialEq)]
pub enum Screen {
    Consent {
        text: String,
    },
    Loading,
    Unavailable {
        text: String,
        command: Option<String>,
    },
    ConnectorList {
        rows: Vec<Row>,
        selected: usize,
        filter: Option<String>,
    },
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
    Review {
        title: String,
        rows: Vec<crate::settings::ReviewRow>,
    },
    Confirm {
        text: String,
    },
    Prompt {
        title: String,
        text: String,
        help: Option<String>,
        problem: Option<String>,
    },
    SecretEditor {
        key: String,
        title: String,
        help: Option<String>,
    },
}

impl std::fmt::Debug for Screen {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Consent { text } => formatter
                .debug_struct("Consent")
                .field("text", text)
                .finish(),
            Self::Loading => formatter.write_str("Loading"),
            Self::Unavailable { text, command } => formatter
                .debug_struct("Unavailable")
                .field("text", text)
                .field("command", command)
                .finish(),
            Self::ConnectorList { rows, selected, .. } => formatter
                .debug_struct("ConnectorList")
                .field("rows", rows)
                .field("selected", selected)
                .finish(),
            Self::ConnectorCard {
                title,
                accounts,
                retired,
                selected,
                ..
            } => formatter
                .debug_struct("ConnectorCard")
                .field("title", title)
                .field("accounts", accounts)
                .field("retired", retired)
                .field("selected", selected)
                .finish(),
            Self::LinkCard {
                app,
                left,
                starting,
                ..
            } => formatter
                .debug_struct("LinkCard")
                .field("app", app)
                .field("left", left)
                .field("starting", starting)
                .finish(),
            Self::Tools {
                header,
                cached,
                rows,
                selected,
            } => formatter
                .debug_struct("Tools")
                .field("header", header)
                .field("cached", cached)
                .field("rows", rows)
                .field("selected", selected)
                .finish(),
            Self::PlatformList {
                rows,
                selected,
                restart_line,
                other_pairing,
            } => formatter
                .debug_struct("PlatformList")
                .field("rows", rows)
                .field("selected", selected)
                .field("restart_line", restart_line)
                .field("other_pairing", &other_pairing.len())
                .finish(),
            Self::PlatformCard {
                fields,
                pairing,
                selected,
                ..
            } => formatter
                .debug_struct("PlatformCard")
                .field("fields", fields)
                .field("pairing", pairing)
                .field("selected", selected)
                .finish(),
            Self::Review { title, rows } => formatter
                .debug_struct("Review")
                .field("title", title)
                .field("rows", rows)
                .finish(),
            Self::Confirm { .. } => formatter.write_str("Confirm([text])"),
            Self::Prompt {
                title,
                text,
                help,
                problem,
            } => formatter
                .debug_struct("Prompt")
                .field("title", title)
                .field("text_len", &text.len())
                .field("help", help)
                .field("problem", problem)
                .finish(),
            Self::SecretEditor { key, title, help } => formatter
                .debug_struct("SecretEditor")
                .field("key", key)
                .field("title", title)
                .field("help", help)
                .finish(),
        }
    }
}

/// A list row.
#[derive(Clone, PartialEq, Eq)]
pub struct Row {
    pub label: String,
    pub state: String,
    pub marker: Option<char>,
}

impl std::fmt::Debug for Row {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Row")
            .field("label_len", &self.label.len())
            .field("state", &self.state)
            .field("marker", &self.marker)
            .finish()
    }
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
#[derive(Clone, PartialEq, Eq)]
pub enum FieldValue {
    Set,
    NotSet,
    List(Vec<String>),
    Draft,
    WillClear,
}

impl std::fmt::Debug for FieldValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Set => formatter.write_str("Set"),
            Self::NotSet => formatter.write_str("NotSet"),
            Self::List(values) => formatter
                .debug_struct("List")
                .field("entries", &values.len())
                .finish(),
            Self::Draft => formatter.write_str("Draft"),
            Self::WillClear => formatter.write_str("WillClear"),
        }
    }
}

/// Apply one input. The clock is the caller's.
#[must_use]
pub fn update(
    state: &mut ConnectorsState,
    input: ConnectorsInput,
    now: Instant,
) -> Vec<ConnectorsEffect> {
    reducer::update(state, input, now)
}

/// Draw data for the current state.
#[must_use]
pub fn view(state: &ConnectorsState, now: Instant) -> ConnectorsView {
    projection::view(state, now)
}

/// When the screen should wake. Only a link wait has one.
#[must_use]
pub fn next_deadline(state: &ConnectorsState) -> Option<Instant> {
    state.operation.as_ref()?;
    [
        state.next_frame,
        state.next_poll.filter(|_| !state.poll_in_flight),
    ]
    .into_iter()
    .flatten()
    .min()
}
