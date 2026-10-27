use hermes_contract::schemas::{
    McpCatalogEntry, McpOauthPollResult, McpServerRuntimeRow, McpServerSummary,
    McpServersTestResult, ReloadMcpResult,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use std::time::Instant;
mod form;
mod servers;
mod signin;
mod text;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum McpTab {
    #[default]
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
        match self {
            Self::AddServer { token: true, .. } => Some(SecretSlot::Token),
            Self::SetKey { slot, .. } => Some(*slot),
            Self::RelayCallback { .. } => Some(SecretSlot::Paste),
            _ => None,
        }
    }
}
#[derive(Debug, Default)]
pub struct McpState {
    open: bool,
    profile: String,
    connected: bool,
    ssh: bool,
    foreground: Option<String>,
    tab: McpTab,
    filter: String,
    selected: usize,
    servers: Vec<ServerRow>,
    runtime: Vec<McpServerRuntimeRow>,
    catalogue: Vec<McpCatalogEntry>,
    list_loaded: bool,
    catalog_loaded: bool,
    catalog_stale: bool,
    list_error: Option<String>,
    catalog_error: Option<String>,
    status_line: Option<String>,
    notice: Option<String>,
    preserve_notice: bool,
    pending: bool,
    write: Option<Write>,
    card: Option<Card>,
    tests: BTreeMap<String, TestOutcome>,
    secrets: BTreeSet<SecretSlot>,
    after_add: Option<AfterAdd>,
    env_queue: Vec<(u8, String)>,
    installed_hint: BTreeSet<String>,
    hint_command: Option<String>,
    toggle_missing: Option<String>,
    reconnect_refresh: bool,
    apply_on_connect: bool,
    cancelled_starts: BTreeSet<String>,
    cancelled_flows: BTreeSet<String>,
    clock: Option<Instant>,
}
impl McpState {
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn pending_changes(&self) -> bool {
        self.pending
    }
}
#[derive(Debug)]
enum Write {
    Add { name: String, catalogue: bool },
    Key { name: String },
    Toggle { name: String },
    Remove { name: String },
    SignIn { name: String },
    Reload,
}
#[derive(Debug)]
struct AfterAdd {
    name: String,
    oauth: bool,
    catalogue: bool,
}
#[derive(Debug)]
enum Card {
    Detail(String),
    Form(form::Form),
    Key {
        name: String,
        variable: Option<String>,
        focus: usize,
        error: Option<String>,
    },
    Remove(String),
    Install(McpCatalogEntry),
    OfferSignIn(String),
    SignIn(signin::SignIn),
    Done {
        title: String,
        message: String,
    },
    Apply {
        session: Option<String>,
        stage: ApplyStage,
    },
}
#[derive(Debug)]
enum ApplyStage {
    Asking,
    Confirm(String),
    Applying,
}
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct RowView {
    pub name: String,
    pub kind: String,
    pub label: String,
    pub dim: bool,
}
#[derive(Clone, Debug)]
pub struct CardView {
    pub title: String,
    pub lines: Vec<String>,
    pub fields: Vec<FieldView>,
    pub footer: String,
}
#[derive(Clone, Debug)]
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
        let mut lines = vec![
            self.title.clone(),
            match self.tab {
                McpTab::Servers => "[Servers]  Catalogue",
                McpTab::Catalogue => "Servers  [Catalogue]",
            }
            .into(),
        ];
        if let Some(banner) = &self.banner {
            lines.push(banner.clone());
        }
        lines.push(if self.filter.is_empty() {
            "type to filter".into()
        } else {
            format!("filter: {}", self.filter)
        });
        if let Some(value) = &self.placeholder {
            lines.push(value.clone());
        }
        lines.extend(
            self.rows
                .iter()
                .map(|r| format!("{} {} {}", r.name, r.kind, r.label)),
        );
        if let Some(value) = &self.status_line {
            lines.push(value.clone());
        }
        if let Some(value) = &self.pending_line {
            lines.push(value.clone());
        }
        if let Some(value) = &self.notice {
            lines.push(value.clone());
        }
        lines.push(self.footer.clone());
        if let Some(card) = &self.card {
            lines.push(card.title.clone());
            lines.extend(card.lines.iter().cloned());
            for f in &card.fields {
                lines.push(format!("{}: {}", f.label, f.text));
                if let Some(e) = &f.error {
                    lines.push(e.clone());
                }
                if let Some(w) = &f.warning {
                    lines.push(w.clone());
                }
            }
            lines.push(card.footer.clone());
        }
        lines
    }
}
pub use form::{env_key_for_server, validate_server_name};
pub use servers::server_state;
pub use signin::{over_ssh, parse_redirect};
pub use text::view;
pub fn parse_mcp_args(args: &str) -> Result<McpOpen, &'static str> {
    match args.trim().to_ascii_lowercase().as_str() {
        "" | "servers" => Ok(McpOpen::Tab(McpTab::Servers)),
        "catalog" | "catalogue" => Ok(McpOpen::Tab(McpTab::Catalogue)),
        "apply" => Ok(McpOpen::Apply),
        _ => Err("Usage: /mcp [servers|catalogue|apply]"),
    }
}
pub fn transport_kind(raw: &str) -> TransportKind {
    if raw == "http" || raw.starts_with("TransportSpec(type='http'") {
        TransportKind::Remote
    } else if raw == "stdio" || raw.starts_with("TransportSpec(type='stdio'") {
        TransportKind::Local
    } else {
        TransportKind::Unknown
    }
}
fn refresh() -> Vec<McpEffect> {
    vec![McpEffect::LoadList, McpEffect::LoadStatus]
}
fn forget(state: &mut McpState) -> McpEffect {
    state.secrets.clear();
    McpEffect::ForgetSecrets
}
fn changed(state: &mut McpState) {
    state.pending = true;
    state.catalog_stale = true;
}
fn short(raw: &str) -> String {
    let mut chars = raw.chars();
    let mut s: String = chars.by_ref().take(300).collect();
    if chars.next().is_some() {
        s.push('…');
    }
    s
}
fn rpc_code(f: &Failure, expected: i64) -> bool {
    matches!(f, Failure::Rpc { code, .. } if *code == expected)
}
fn unknown(f: &Failure) -> bool {
    matches!(f, Failure::Unknown | Failure::BadReply)
}
fn error_text(state: &McpState, name: &str, method: &str, error: &Failure, write: bool) -> String {
    match error {
        Failure::Rpc { code: -32601, .. } => format!("Hermes did not answer {method} on this connection."),
        Failure::Unknown | Failure::BadReply if write => "The change may or may not have been saved. Talaria will reload the list when it reconnects.".into(),
        Failure::Unknown | Failure::BadReply => "no answer from Hermes".into(),
        Failure::NotSent(reason) => format!("Not sent: {}.", short(reason)),
        Failure::Rpc { code: 4064, .. } => format!("{name} is no longer configured."),
        Failure::Rpc { code: 4090, .. } | Failure::Http { status: 409, .. } => plugin_text(state, name),
        Failure::Rpc { code: 5024, message, .. } => format!("Hermes could not do that: {}", short(message)),
        Failure::Rpc { message, .. } => format!("Hermes refused: {}", short(message)),
        Failure::Http { status: 400, .. } => format!("Hermes says {name}'s configuration is malformed."),
        Failure::Http { status: 401, .. } => "Hermes refused Talaria's sign-in to its web interface. Restart Talaria; if it persists, run the doctor.".into(),
        Failure::Http { status: 403, detail } => format!("Hermes says this profile is not Talaria's to change ({}).", short(detail.as_deref().unwrap_or("refused"))),
        Failure::Http { status, .. } => format!("Hermes's web interface failed (HTTP {status})."),
        Failure::BadPaste(_) => "That is not the sign-in address: it starts with http://127.0.0.1: and contains code= and state=.".into(),
    }
}
fn plugin_text(state: &McpState, name: &str) -> String {
    let plugin = state
        .servers
        .iter()
        .find(|r| r.summary.name == name)
        .and_then(|r| r.summary.plugin.as_deref())
        .unwrap_or("unknown");
    format!(
        "{} comes from plugin {}; change it in the plugin's settings.",
        short(name),
        short(plugin)
    )
}
fn failure(
    state: &mut McpState,
    name: &str,
    method: &str,
    e: &Failure,
    write: bool,
) -> Vec<McpEffect> {
    state.notice = state
        .open
        .then(|| error_text(state, name, method, e, write));
    if write && unknown(e) {
        state.reconnect_refresh = true;
    }
    if rpc_code(e, 4064) { refresh() } else { vec![] }
}
fn start_test(state: &mut McpState, name: String, now: Instant) -> Vec<McpEffect> {
    if !state.connected
        || state
            .tests
            .values()
            .any(|t| matches!(t, TestOutcome::Running { .. }))
    {
        return vec![];
    }
    state
        .tests
        .insert(name.clone(), TestOutcome::Running { since: now });
    vec![McpEffect::Test { name }]
}
fn finish_add(state: &mut McpState) -> Vec<McpEffect> {
    state.card = None;
    state.write = None;
    state.tab = McpTab::Servers;
    state.filter.clear();
    let mut effects = vec![forget(state)];
    effects.extend(refresh());
    effects
}
fn begin_apply(state: &mut McpState) -> Vec<McpEffect> {
    let session = state.foreground.clone();
    state.card = Some(Card::Apply {
        session: session.clone(),
        stage: ApplyStage::Asking,
    });
    state.write = Some(Write::Reload);
    vec![McpEffect::Reload {
        session_id: session,
        confirm: false,
    }]
}
fn close(state: &mut McpState) -> Vec<McpEffect> {
    state.apply_on_connect = false;
    if matches!(
        &state.card,
        Some(Card::Apply {
            stage: ApplyStage::Confirm(_),
            ..
        })
    ) {
        state.write = None;
    }
    let mut effects = signin::cancel(state);
    state.open = false;
    state.card = None;
    state.filter.clear();
    state.preserve_notice = false;
    state.notice = state
        .pending
        .then(|| "MCP changes are saved. /mcp apply uses them in this session.".into());
    effects.push(forget(state));
    effects.push(McpEffect::Close);
    effects
}
pub fn next_deadline(state: &McpState) -> Option<Instant> {
    if !state.open {
        return None;
    }
    let clock = state.clock?;
    let next_second = |since: Instant| {
        since + Duration::from_secs(clock.saturating_duration_since(since).as_secs() + 1)
    };
    let mut deadlines: Vec<_> = state
        .tests
        .values()
        .filter_map(|test| {
            if let TestOutcome::Running { since } = test {
                Some(next_second(*since))
            } else {
                None
            }
        })
        .collect();
    if let Some(Card::SignIn(signin)) = &state.card {
        if let Some(since) = signin.since {
            deadlines.push(next_second(since));
        }
        if state.connected && !signin.polling {
            deadlines.extend(signin.due);
        }
    }
    deadlines.into_iter().min()
}

pub fn update(state: &mut McpState, input: McpInput, now: Instant) -> Vec<McpEffect> {
    state.clock = Some(now);
    match input {
        McpInput::Open {
            open,
            profile_name,
            connected,
            over_ssh,
            foreground,
        } => {
            let mut effects = signin::cancel(state);
            state.open = true;
            state.profile = profile_name;
            state.connected = connected;
            state.ssh = over_ssh;
            state.foreground = foreground;
            state.apply_on_connect = open == McpOpen::Apply && !connected;
            state.tab = match open {
                McpOpen::Tab(tab) => tab,
                McpOpen::Apply => McpTab::Servers,
            };
            state.filter.clear();
            state.card = None;
            state.notice = None;
            state.preserve_notice = false;
            state.hint_command = None;
            effects.push(forget(state));
            if connected {
                effects.extend(refresh());
                if state.tab == McpTab::Catalogue {
                    effects.push(McpEffect::LoadCatalog);
                }
                if open == McpOpen::Apply && state.write.is_none() {
                    effects.extend(begin_apply(state));
                }
            }
            effects
        }
        McpInput::Close => close(state),
        McpInput::Key(key) if state.open => handle_key(state, key, now),
        McpInput::Key(_) => vec![],
        McpInput::Tick => {
            if state.open {
                signin::tick(state, now)
            } else {
                vec![]
            }
        }
        McpInput::Foreground(id) => {
            state.foreground = id;
            vec![]
        }
        McpInput::Connection { connected } => {
            let was_connected = state.connected;
            state.connected = connected;
            if connected && (!was_connected || state.reconnect_refresh) {
                state.reconnect_refresh = false;
                let mut effects = refresh();
                if state.tab == McpTab::Catalogue {
                    effects.push(McpEffect::LoadCatalog);
                }
                if state.open && state.apply_on_connect && state.write.is_none() {
                    state.apply_on_connect = false;
                    effects.extend(begin_apply(state));
                }
                effects
            } else {
                vec![]
            }
        }
        McpInput::OpenFailed => {
            if state.open {
                state.notice = Some("Could not open a browser. Copy the link with c.".into());
            }
            vec![]
        }
        McpInput::SecretEdited { slot, empty } => {
            if state.open
                && (view(state, now).secret_focus == Some(slot)
                    || matches!(&state.card, Some(Card::Form(_))))
            {
                if empty {
                    state.secrets.remove(&slot);
                } else {
                    state.secrets.insert(slot);
                }
                if let Some(Card::Form(form)) = &mut state.card {
                    form.dirty = true;
                }
            }
            vec![]
        }
        McpInput::SecretSubmitted { slot } => submit_secret(state, slot, now),
        McpInput::ListLoaded(result) => {
            let selected = servers::selected_name(state);
            state.list_loaded = true;
            match result {
                Ok(mut list) => {
                    list.sort_by(|a, b| a.name.cmp(&b.name));
                    state.servers = list
                        .into_iter()
                        .map(|summary| ServerRow {
                            runtime: state
                                .runtime
                                .iter()
                                .find(|r| r.name == summary.name)
                                .cloned(),
                            summary,
                        })
                        .collect();
                    state.list_error = None;
                    if let Some(name) = selected {
                        servers::select_name(state, &name);
                    } else {
                        servers::select_name(state, "");
                    }
                    if let Some(name) = state.toggle_missing.take() {
                        state.notice = Some(
                            if state.servers.iter().any(|r| r.summary.name == name) {
                                "This Hermes cannot turn MCP servers on or off from Talaria (its web interface answered 404).".into()
                            } else {
                                format!("{name} is no longer configured.")
                            },
                        );
                    }
                    if let Some(after) = state.after_add.take() {
                        if state.servers.iter().any(|r| r.summary.name == after.name) {
                            state.tab = McpTab::Servers;
                            state.filter.clear();
                            servers::select_name(state, &after.name);
                            if state.open && state.connected {
                                if after.oauth {
                                    if after.catalogue {
                                        state.card = Some(Card::OfferSignIn(after.name));
                                    } else {
                                        return signin::start(state, after.name);
                                    }
                                } else {
                                    return start_test(state, after.name, now);
                                }
                            }
                        } else if state.open {
                            state.notice = Some(format!(
                                "{} is installed but not listed yet. ctrl+r reloads.",
                                short(&after.name)
                            ));
                        }
                    }
                }
                Err(e) => {
                    state.list_error = Some(error_text(state, "", "mcp.servers.list", &e, false));
                }
            }
            vec![]
        }
        McpInput::StatusLoaded(result) => {
            match result {
                Ok(runtime) => {
                    state.runtime = runtime;
                    state.status_line = None;
                }
                Err(e) => {
                    state.runtime.clear();
                    state.status_line = Some(format!(
                        "Could not read which servers are running: {}.",
                        error_text(state, "", "mcp.servers.status", &e, false)
                    ));
                }
            }
            for row in &mut state.servers {
                row.runtime = state
                    .runtime
                    .iter()
                    .find(|r| r.name == row.summary.name)
                    .cloned();
            }
            vec![]
        }
        McpInput::CatalogLoaded(result) => {
            let selected = servers::selected_name(state);
            state.catalog_loaded = true;
            match result {
                Ok(mut catalogue) => {
                    catalogue.sort_by(|a, b| a.name.cmp(&b.name));
                    state.catalogue = catalogue;
                    state.catalog_error = None;
                    state.catalog_stale = false;
                }
                Err(e) => {
                    state.catalog_error = Some(error_text(state, "", "mcp.catalog", &e, false));
                }
            }
            if let Some(name) = selected {
                servers::select_name(state, &name);
            } else {
                servers::select_name(state, "");
            }
            vec![]
        }
        McpInput::Tested { name, result } => servers::tested(state, name, result, now),
        McpInput::Added { name, result } => added(state, name, result),
        McpInput::KeySaved {
            name,
            env_var,
            result,
        } => key_saved(state, name, env_var, result),
        McpInput::Toggled { name, result } => {
            if !matches!(&state.write,Some(Write::Toggle { name: target }) if target == &name) {
                return vec![];
            }
            state.write = None;
            match result {
                Ok(enabled) => {
                    if let Some(row) = state.servers.iter_mut().find(|r| r.summary.name == name) {
                        row.summary.enabled = enabled;
                    }
                    changed(state);
                    state.notice = None;
                    refresh()
                }
                Err(Failure::Http { status: 404, .. }) => {
                    state.toggle_missing = Some(name);
                    refresh()
                }
                Err(Failure::NotSent(_)) => {
                    state.notice = Some("Could not reach Hermes's web interface.".into());
                    vec![]
                }
                Err(e) => failure(state, &name, "enabled", &e, true),
            }
        }
        McpInput::Removed { name, result } => {
            if !matches!(&state.write,Some(Write::Remove { name: target }) if target == &name) {
                return vec![];
            }
            state.write = None;
            match result {
                Ok(()) => {
                    changed(state);
                    state.notice = state.open.then(|| format!("Removed {}.", short(&name)));
                    refresh()
                }
                Err(e) if rpc_code(&e, 4064) => {
                    state.notice = state
                        .open
                        .then(|| format!("{} was already gone.", short(&name)));
                    refresh()
                }
                Err(e) => failure(state, &name, "mcp.servers.remove", &e, true),
            }
        }
        McpInput::SignInStarted { name, result } => signin::started(state, name, result, now),
        McpInput::Polled { name, result } => signin::polled(state, name, result, now),
        McpInput::CallbackRelayed { name, result } => {
            if let Some(Card::SignIn(signin)) = &mut state.card {
                if signin.name != name || !signin.callback {
                    return vec![];
                }
                signin.paste = false;
                signin.callback = false;
                state.notice = Some(match result {
                    Ok(()) => "Sent the address to Hermes; waiting for it to finish.".into(),
                    Err(Failure::BadPaste(e)) => error_text(
                        state,
                        &name,
                        "mcp.servers.oauth.callback",
                        &Failure::BadPaste(e),
                        false,
                    ),
                    Err(Failure::Rpc { message, .. }) => {
                        format!("Hermes did not accept the address: {}", short(&message))
                    }
                    Err(e) => error_text(state, &name, "mcp.servers.oauth.callback", &e, false),
                });
                vec![forget(state)]
            } else {
                vec![]
            }
        }
        McpInput::Reloaded(result) => reloaded(state, result),
    }
}

fn added(
    state: &mut McpState,
    name: String,
    result: Result<McpServerSummary, Failure>,
) -> Vec<McpEffect> {
    if !matches!(&state.write, Some(Write::Add { name: expected, .. }) if expected == &name) {
        return vec![];
    }
    let Some(Write::Add {
        name: expected,
        catalogue,
    }) = state.write.take()
    else {
        return vec![];
    };
    if name != expected {
        state.write = Some(Write::Add {
            name: expected,
            catalogue,
        });
        return vec![];
    }
    match result {
        Ok(server) => {
            changed(state);
            state.after_add = Some(AfterAdd {
                name: name.clone(),
                oauth: server.auth.as_deref() == Some("oauth"),
                catalogue,
            });
            if catalogue {
                state.installed_hint.insert(name.clone());
            }
            if !state.open {
                state.env_queue.clear();
                return finish_add(state);
            }
            if let Some((i, env)) = state.env_queue.first() {
                state.write = Some(Write::Key { name: name.clone() });
                return vec![McpEffect::SetKey {
                    name,
                    env_var: Some(env.clone()),
                    slot: SecretSlot::EnvValue(*i),
                }];
            }
            state.notice = None;
            finish_add(state)
        }
        Err(e) => {
            state.env_queue.clear();
            state.notice = state.open.then(|| match &e {
                Failure::Rpc {
                    code: 4090,
                    message,
                    ..
                } if catalogue => {
                    let _ = message;
                    format!("{} is already installed.", short(&name))
                }
                Failure::Rpc {
                    code: 4090,
                    message,
                    ..
                } if !message.contains("plugin") => {
                    format!("A server named {} already exists.", short(&name))
                }
                Failure::Rpc { code: 4001, .. } => {
                    format!("Hermes refused {}: its command looks unsafe.", short(&name))
                }
                Failure::Rpc { code: 4063, .. } if catalogue => format!(
                    "Hermes no longer offers {}. ctrl+r reloads the catalogue.",
                    short(&name)
                ),
                Failure::Rpc { code: 4063, .. } => "Hermes needs a URL or a command.".into(),
                Failure::Rpc {
                    code: 5024,
                    message,
                    ..
                } if catalogue => format!("Could not install {}: {}", short(&name), short(message)),
                _ => error_text(state, &name, "mcp.servers.add", &e, true),
            });
            if unknown(&e) {
                state.reconnect_refresh = true;
            }
            let mut effects = vec![forget(state)];
            if catalogue {
                state.card = None;
                if rpc_code(&e, 4090) {
                    effects.extend(refresh());
                    effects.push(McpEffect::LoadCatalog);
                }
            }
            effects
        }
    }
}
fn key_saved(
    state: &mut McpState,
    name: String,
    env_var: Option<String>,
    result: Result<String, Failure>,
) -> Vec<McpEffect> {
    if !matches!(&state.write,Some(Write::Key { name: target }) if target == &name) {
        return vec![];
    }
    state.write = None;
    if state.after_add.as_ref().is_some_and(|a| a.name == name) && !state.env_queue.is_empty() {
        let (_, env) = state.env_queue.remove(0);
        if env_var.as_deref() != Some(env.as_str()) {
            state.env_queue.clear();
        }
        match result {
            Ok(_) => {
                changed(state);
                if !state.open {
                    state.env_queue.clear();
                    return finish_add(state);
                }
                if let Some((i, env)) = state.env_queue.first() {
                    state.write = Some(Write::Key { name: name.clone() });
                    return vec![McpEffect::SetKey {
                        name,
                        env_var: Some(env.clone()),
                        slot: SecretSlot::EnvValue(*i),
                    }];
                }
                state.notice = None;
            }
            Err(e) => {
                state.env_queue.clear();
                state.notice = state.open.then(|| {
                    format!(
                        "Added {}, but saving {} failed: {}. ctrl+k on the server sets it.",
                        short(&name),
                        short(&env),
                        error_text(state, &name, "mcp.servers.set_api_key", &e, true)
                            .trim_end_matches('.')
                    )
                });
                state.preserve_notice = true;
                if unknown(&e) {
                    state.reconnect_refresh = true;
                }
            }
        }
        return finish_add(state);
    }
    state.card = None;
    let mut effects = vec![forget(state)];
    match result {
        Ok(env) => {
            changed(state);
            state.notice = state
                .open
                .then(|| format!("Saved the key for {} as {}.", short(&name), short(&env)));
            effects.extend(refresh());
        }
        Err(e) if rpc_code(&e, 4063) => {
            state.notice = state.open.then(|| "That is not a valid key.".into());
        }
        Err(e) => effects.extend(failure(state, &name, "mcp.servers.set_api_key", &e, true)),
    }
    effects
}
fn reloaded(state: &mut McpState, result: Result<ReloadMcpResult, Failure>) -> Vec<McpEffect> {
    if !matches!(&state.write, Some(Write::Reload)) {
        return vec![];
    }
    match result {
        Ok(reply) if reply.status == hermes_contract::schemas::ReloadMcpStatus::ConfirmRequired => {
            if let Some(Card::Apply { stage, .. }) = &mut state.card {
                *stage = ApplyStage::Confirm(short(
                    reply
                        .message
                        .as_deref()
                        .unwrap_or("Reloading MCP refreshes every chat of this profile."),
                ));
            } else {
                state.write = None;
            }
        }
        Ok(reply) => {
            state.write = None;
            state.card = None;
            state.pending = false;
            let mut text = match reply.session {
                Some(hermes_contract::schemas::ReloadMcpSession::Applied) => "MCP servers reloaded. This session's tools change from its next turn.".into(),
                Some(hermes_contract::schemas::ReloadMcpSession::Unchanged) => "MCP servers reloaded. This session's tools are unchanged.".into(),
                Some(hermes_contract::schemas::ReloadMcpSession::Busy) => "MCP servers reloaded. This session is busy: its new tools apply after you apply again once the turn ends, or when it is reopened.".into(),
                None => format!("MCP servers reloaded for profile {}.",short(&state.profile)),
            };
            if let Some(names) = reply.unavailable.filter(|n| !n.is_empty()) {
                text.push_str(&format!(
                    " Did not connect: {}.",
                    names
                        .iter()
                        .map(|n| short(n))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            state.notice = state.open.then_some(text);
            return refresh();
        }
        Err(e) => {
            state.write = None;
            state.card = None;
            state.notice = state.open.then(||match &e {
                Failure::Unknown | Failure::BadReply => "Hermes did not confirm the reload; it may or may not have run. ctrl+y asks again.".into(),
                Failure::Rpc { reason:Some(reason),detail,.. } if reason == "mcp_servers_unavailable" => format!("Nothing changed: {}.",short(detail.as_deref().unwrap_or("MCP servers unavailable")).trim_end_matches('.')),
                Failure::Rpc { reason:Some(reason),.. } if reason == "permission_denied" => "This connection may not reload MCP servers.".into(),
                Failure::Rpc { reason:Some(reason),.. } if reason == "invalid_params" => "Hermes refused Talaria's reload request (invalid_params).".into(),
                _ => error_text(state,"","reload.mcp",&e,true),
            });
            if unknown(&e) {
                state.reconnect_refresh = true;
            }
        }
    }
    vec![]
}

fn submit_secret(state: &mut McpState, slot: SecretSlot, now: Instant) -> Vec<McpEffect> {
    if !state.open {
        return vec![];
    }
    let secret_focus = view(state, now).secret_focus;
    match state.card.as_mut() {
        Some(Card::Form(_)) if secret_focus == Some(slot) => handle_key(state, McpKey::Tab, now),
        Some(Card::Key { .. }) if slot == SecretSlot::KeyValue => save_key(state),
        Some(Card::SignIn(signin))
            if slot == SecretSlot::Paste && signin.paste && !signin.callback && state.connected =>
        {
            if !state.secrets.contains(&slot) {
                return vec![];
            }
            let Some(flow) = &signin.flow else {
                return vec![];
            };
            signin.callback = true;
            vec![McpEffect::RelayCallback {
                name: signin.name.clone(),
                flow: flow.clone(),
            }]
        }
        _ => vec![],
    }
}
fn save_key(state: &mut McpState) -> Vec<McpEffect> {
    if !state.connected || state.write.is_some() {
        return vec![];
    }
    let Some(Card::Key {
        name,
        variable,
        focus,
        error,
    }) = &mut state.card
    else {
        return vec![];
    };
    if variable
        .as_ref()
        .is_some_and(|v| !form::valid_env(v.trim()))
    {
        *focus = 0;
        *error = Some(form::ENV_ERROR.into());
        return vec![];
    }
    if !state.secrets.contains(&SecretSlot::KeyValue) {
        *focus = usize::from(variable.is_some());
        *error = Some("Enter a value.".into());
        return vec![];
    }
    let name = name.clone();
    let env_var = variable.as_ref().map(|v| v.trim().to_string());
    state.write = Some(Write::Key { name: name.clone() });
    state.notice = Some("Waiting for Hermes…".into());
    vec![McpEffect::SetKey {
        name,
        env_var,
        slot: SecretSlot::KeyValue,
    }]
}
fn handle_key(state: &mut McpState, key: McpKey, now: Instant) -> Vec<McpEffect> {
    let key = match key {
        McpKey::Action(McpAction::OpenLink) if !matches!(&state.card,Some(Card::SignIn(s)) if s.flow.is_some()) => {
            McpKey::Char('o')
        }
        McpKey::Action(McpAction::PasteRedirect) if !matches!(&state.card,Some(Card::SignIn(s)) if s.flow.is_some()) => {
            McpKey::Char('p')
        }
        McpKey::Action(McpAction::Copy) => {
            let command = match &state.card {
                Some(Card::SignIn(signin)) if signin.flow.is_some() => Some(signin.url.clone()),
                Some(Card::Detail(name))
                    if state.servers.iter().any(|r| {
                        r.summary.name == *name && r.summary.auth.as_deref() == Some("oauth")
                    }) =>
                {
                    Some(text::login_command(state, name))
                }
                Some(Card::Install(entry)) if !entry.requires.is_empty() => {
                    Some(text::install_command(state, &entry.name))
                }
                Some(Card::Done { message, .. })
                    if message.contains("Some catalogue servers need values") =>
                {
                    state.hint_command.clone()
                }
                None if state.notice.as_ref().is_some_and(|notice| {
                    notice.contains("Some catalogue servers need values")
                }) =>
                {
                    state.hint_command.clone()
                }
                _ => None,
            };
            if let Some(command) = command {
                state.notice = Some("Copied to the clipboard if your terminal allows it.".into());
                return vec![McpEffect::CopyText(command)];
            }
            McpKey::Char('c')
        }
        key => key,
    };
    if let Some(card) = state.card.take() {
        match card {
            Card::Form(mut form) => {
                if state.write.is_some() {
                    state.notice = Some("Waiting for Hermes.".into());
                    state.card = Some(Card::Form(form));
                    return vec![];
                }
                let (closed, effects) = form.key(key, state);
                if !closed {
                    state.card = Some(Card::Form(form));
                }
                return effects;
            }
            Card::Key {
                name,
                mut variable,
                mut focus,
                mut error,
            } => {
                match key {
                    McpKey::Esc => return vec![forget(state)],
                    McpKey::Tab | McpKey::Down | McpKey::Enter
                        if variable.is_some() && focus == 0 =>
                    {
                        focus = 1;
                        error = None;
                    }
                    McpKey::BackTab | McpKey::Up if variable.is_some() => {
                        focus = 0;
                        error = None;
                    }
                    McpKey::Enter | McpKey::Action(McpAction::Save) => {
                        state.card = Some(Card::Key {
                            name,
                            variable,
                            focus,
                            error,
                        });
                        return save_key(state);
                    }
                    McpKey::Char(c) if focus == 0 => {
                        if let Some(var) = &mut variable {
                            var.push(c);
                        }
                    }
                    McpKey::Paste(s) if focus == 0 => {
                        if let Some(var) = &mut variable {
                            var.push_str(&s);
                        }
                    }
                    McpKey::Backspace if focus == 0 => {
                        if let Some(var) = &mut variable {
                            var.pop();
                        }
                    }
                    _ => {}
                }
                state.card = Some(Card::Key {
                    name,
                    variable,
                    focus,
                    error,
                });
                return vec![];
            }
            Card::SignIn(mut signin) => match key {
                McpKey::Esc if signin.paste => {
                    signin.paste = false;
                    state.card = Some(Card::SignIn(signin));
                    return vec![forget(state)];
                }
                McpKey::Esc => {
                    state.card = Some(Card::SignIn(signin));
                    let mut effects = signin::cancel(state);
                    effects.push(forget(state));
                    return effects;
                }
                McpKey::Action(McpAction::OpenLink) if signin.flow.is_some() => {
                    let url = signin.url.clone();
                    state.card = Some(Card::SignIn(signin));
                    if form::address_parts(&url).is_some_and(|p| p.scheme == "https") {
                        return vec![McpEffect::OpenUrl(url)];
                    }
                    state.notice = Some("Could not open a browser. Copy the link with c.".into());
                    return vec![];
                }
                McpKey::Action(McpAction::PasteRedirect) if signin.flow.is_some() => {
                    signin.paste = true;
                    state.card = Some(Card::SignIn(signin));
                    return vec![forget(state)];
                }
                _ => {
                    state.card = Some(Card::SignIn(signin));
                    return vec![];
                }
            },
            Card::Apply { session, stage } => match (&stage, &key) {
                (ApplyStage::Confirm(_), McpKey::Char('y')) if state.connected => {
                    state.card = Some(Card::Apply {
                        session: session.clone(),
                        stage: ApplyStage::Applying,
                    });
                    return vec![McpEffect::Reload {
                        session_id: session,
                        confirm: true,
                    }];
                }
                (ApplyStage::Confirm(_), McpKey::Char('n') | McpKey::Esc) => {
                    state.write = None;
                    return vec![];
                }
                _ => {
                    state.card = Some(Card::Apply { session, stage });
                    return vec![];
                }
            },
            Card::Remove(name) => match key {
                McpKey::Esc | McpKey::Char('n') => return vec![],
                McpKey::Enter | McpKey::Char('y') if state.connected && state.write.is_none() => {
                    state.write = Some(Write::Remove { name: name.clone() });
                    state.notice = Some("Waiting for Hermes…".into());
                    return vec![McpEffect::Remove { name }];
                }
                _ => {
                    state.card = Some(Card::Remove(name));
                    return vec![];
                }
            },
            Card::Install(entry) => match key {
                McpKey::Esc => return vec![forget(state)],
                McpKey::Enter
                    if entry.requires.is_empty() && state.connected && state.write.is_none() =>
                {
                    let name = entry.name.clone();
                    state.write = Some(Write::Add {
                        name: name.clone(),
                        catalogue: true,
                    });
                    state.notice = Some("Waiting for Hermes…".into());
                    state.card = Some(Card::Install(entry));
                    return vec![McpEffect::AddServer {
                        name: name.clone(),
                        preset: Some(name),
                        config: None,
                        token: false,
                    }];
                }
                _ => {
                    state.card = Some(Card::Install(entry));
                    return vec![];
                }
            },
            Card::OfferSignIn(name) => match key {
                McpKey::Esc | McpKey::Char('n') => return vec![forget(state)],
                McpKey::Enter | McpKey::Char('y') if state.connected && state.write.is_none() => {
                    return signin::start(state, name);
                }
                _ => {
                    state.card = Some(Card::OfferSignIn(name));
                    return vec![];
                }
            },
            Card::Detail(name) => {
                if !matches!(key, McpKey::Esc) {
                    state.card = Some(Card::Detail(name));
                }
                return vec![];
            }
            Card::Done { title, message } => {
                if matches!(
                    key,
                    McpKey::Tab
                        | McpKey::BackTab
                        | McpKey::Action(McpAction::NextTab | McpAction::PrevTab)
                ) {
                    state.card = Some(Card::Done { title, message });
                }
                return vec![forget(state)];
            }
        }
    }
    match key {
        McpKey::Esc => {
            if state.filter.is_empty() {
                close(state)
            } else {
                state.filter.clear();
                state.selected = 0;
                vec![]
            }
        }
        McpKey::Tab | McpKey::BackTab | McpKey::Action(McpAction::NextTab | McpAction::PrevTab) => {
            state.tab = match state.tab {
                McpTab::Servers => McpTab::Catalogue,
                McpTab::Catalogue => McpTab::Servers,
            };
            state.filter.clear();
            state.selected = 0;
            state.notice = None;
            let mut effects = vec![forget(state)];
            if state.connected
                && state.tab == McpTab::Catalogue
                && (!state.catalog_loaded || state.catalog_stale)
            {
                effects.push(McpEffect::LoadCatalog);
            }
            effects
        }
        McpKey::Up | McpKey::PageUp => {
            state.selected = state
                .selected
                .saturating_sub(if matches!(key, McpKey::PageUp) { 10 } else { 1 });
            vec![]
        }
        McpKey::Down | McpKey::PageDown | McpKey::End => {
            let len = match state.tab {
                McpTab::Servers => servers::visible_servers(state).len(),
                McpTab::Catalogue => servers::visible_catalogue(state).len(),
            };
            state.selected = if matches!(key, McpKey::End) {
                len.saturating_sub(1)
            } else {
                (state.selected
                    + if matches!(key, McpKey::PageDown) {
                        10
                    } else {
                        1
                    })
                .min(len.saturating_sub(1))
            };
            vec![]
        }
        McpKey::Home => {
            state.selected = 0;
            vec![]
        }
        McpKey::Char(c) => {
            state.filter.push(c);
            state.selected = 0;
            vec![]
        }
        McpKey::Paste(s) => {
            state.filter.push_str(&s);
            state.selected = 0;
            vec![]
        }
        McpKey::Backspace => {
            state.filter.pop();
            state.selected = 0;
            vec![]
        }
        McpKey::Action(McpAction::Reload) if state.connected => {
            let mut effects = refresh();
            if state.tab == McpTab::Catalogue {
                effects.push(McpEffect::LoadCatalog);
            }
            effects
        }
        McpKey::Enter => {
            let Some(name) = servers::selected_name(state) else {
                return vec![];
            };
            if state.tab == McpTab::Servers {
                state.card = Some(Card::Detail(name));
            } else if let Some(entry) = state.catalogue.iter().find(|r| r.name == name).cloned() {
                if entry.installed {
                    state.tab = McpTab::Servers;
                    state.filter.clear();
                    servers::select_name(state, &name);
                    if !state.servers.iter().any(|r| r.summary.name == name) {
                        state.notice = Some(format!(
                            "{} is installed but not listed yet. ctrl+r reloads.",
                            short(&name)
                        ));
                    }
                } else {
                    state.card = Some(Card::Install(entry));
                }
            }
            vec![]
        }
        McpKey::Action(action) => list_action(state, action, now),
        _ => vec![],
    }
}
fn list_action(state: &mut McpState, action: McpAction, now: Instant) -> Vec<McpEffect> {
    if !state.connected {
        return vec![];
    }
    if !matches!(action, McpAction::Test) && state.write.is_some() {
        state.notice = Some("Waiting for Hermes.".into());
        return vec![];
    }
    state.preserve_notice = false;
    if matches!(action, McpAction::Add) {
        state.card = Some(Card::Form(form::Form::default()));
        state.notice = None;
        return vec![forget(state)];
    }
    if matches!(action, McpAction::Apply) {
        return begin_apply(state);
    }
    if state.tab != McpTab::Servers {
        return vec![];
    }
    let Some(name) = servers::selected_name(state) else {
        return vec![];
    };
    let Some(server) = state
        .servers
        .iter()
        .find(|r| r.summary.name == name)
        .map(|r| r.summary.clone())
    else {
        return vec![];
    };
    if matches!(action, McpAction::Test) {
        return start_test(state, name, now);
    }
    if matches!(
        action,
        McpAction::Toggle | McpAction::SetKey | McpAction::SignIn | McpAction::Remove
    ) && server.source == hermes_contract::schemas::McpServerSource::Plugin
    {
        state.notice = Some(plugin_text(state, &name));
        return vec![];
    }
    match action {
        McpAction::Toggle => {
            state.write = Some(Write::Toggle { name: name.clone() });
            state.notice = Some("Waiting for Hermes…".into());
            vec![McpEffect::SetEnabled {
                name,
                enabled: !server.enabled,
            }]
        }
        McpAction::Remove => {
            state.card = Some(Card::Remove(name));
            vec![]
        }
        McpAction::SetKey => {
            if server.auth.as_deref() == Some("oauth") {
                state.notice = Some(format!("{} signs in with OAuth: ctrl+o.", short(&name)));
                return vec![];
            }
            let variable = server.url.is_none().then(|| env_key_for_server(&name));
            state.card = Some(Card::Key {
                name,
                variable,
                focus: 0,
                error: None,
            });
            state.notice = None;
            vec![forget(state)]
        }
        McpAction::SignIn => {
            if server.url.is_none() {
                state.notice =
                    Some("Local servers take keys as environment variables: ctrl+k.".into());
            } else if server.auth.as_deref() == Some("header") {
                state.notice = Some(format!(
                    "{} uses an API key header: ctrl+k replaces it.",
                    short(&name)
                ));
            } else if server.auth.as_deref() != Some("oauth") {
                state.notice = Some(format!("{} does not ask for sign-in.", short(&name)));
            } else {
                return signin::start(state, name);
            }
            vec![]
        }
        _ => vec![],
    }
}
