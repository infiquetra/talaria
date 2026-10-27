//! Providers overlay state. `update` returns no effects and `view` is empty.

use std::time::{Duration, Instant};

use hermes_contract::schemas::{ModelOptionProvider, ModelOptionsResult};

use super::accounts::{Account, PollReply, SignInStart};
use super::endpoints::{EndpointCheck, EndpointDraft, EndpointList};
use super::keys::{EnvRow, KeyCheck};
use super::text::USAGE;

/// Which providers tab is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProvidersTab {
    /// API keys.
    Keys,
    /// Account sign-in.
    Accounts,
    /// Custom endpoints.
    Endpoints,
}

/// How `/providers` was opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    /// A named tab.
    Tab(ProvidersTab),
    /// A provider slug from the model picker.
    Provider {
        /// Provider slug.
        slug: String,
    },
}

/// What to do with a saved endpoint key. The typed text is not stored here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiKeyChange {
    /// Leave the saved key as it is.
    Keep,
    /// Remove the saved key.
    Clear,
    /// Replace the saved key with the typed text.
    Typed,
}

/// A failure the service reports to the overlay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The gateway returned an error object.
    Refused {
        /// Error code.
        code: i64,
        /// Error message.
        message: String,
    },
    /// An HTTP status.
    Http {
        /// Status code.
        status: u16,
        /// Detail string, when the body had one.
        detail: Option<String>,
    },
    /// The request was not written.
    NotSent(String),
    /// The reply could not be read.
    BadReply,
    /// The outcome is unknown.
    Unknown,
}

/// A key the overlay handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvidersKey {
    /// One typed character.
    Char(char),
    /// Bracketed paste. Not used while the secret field is focused.
    Paste(String),
    /// Enter.
    Enter,
    /// Escape.
    Esc,
    /// Tab.
    Tab,
    /// Shift+Tab.
    BackTab,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Home.
    Home,
    /// End.
    End,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Backspace.
    Backspace,
    /// Ctrl+U in the secret field.
    CtrlU,
    /// One of the nine providers bindings.
    Action(ProvidersAction),
}

/// The nine providers bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProvidersAction {
    /// Next tab.
    NextTab,
    /// Previous tab.
    PrevTab,
    /// Reload the lists.
    Reload,
    /// Remove or sign out.
    Remove,
    /// Test an endpoint.
    Test,
    /// Save an endpoint.
    Save,
    /// Clear a saved endpoint key.
    ClearKey,
    /// Open a link.
    OpenLink,
    /// Copy a code or a command.
    Copy,
}

/// One fact fed to the overlay.
#[derive(Clone, Debug, PartialEq)]
pub enum ProvidersInput {
    /// Open the overlay.
    Open {
        /// Tab or provider slug.
        target: OpenTarget,
        /// Connected profile name.
        profile_name: String,
        /// Whether the socket is up.
        connected: bool,
    },
    /// A key press.
    Key(ProvidersKey),
    /// The countdown timer fired.
    Tick,
    /// The socket connected or dropped.
    Connection {
        /// Whether the socket is up.
        connected: bool,
    },
    /// Close the overlay.
    Close,
    /// The browser could not be opened.
    OpenFailed,
    /// The secret field changed. The text stays in the field.
    SecretEdited {
        /// Whether the field is empty.
        empty: bool,
    },
    /// The secret field was submitted.
    SecretSubmitted,
    /// The secret field was cleared.
    SecretCleared,
    /// Environment list reply.
    EnvLoaded(Result<Vec<EnvRow>, Failure>),
    /// `model.options` reply.
    OptionsLoaded(Result<ModelOptionsResult, Failure>),
    /// Account list reply.
    OauthLoaded(Result<Vec<Account>, Failure>),
    /// Endpoint list reply.
    EndpointsLoaded(Result<EndpointList, Failure>),
    /// Key check reply.
    KeyChecked(Result<KeyCheck, Failure>),
    /// Key save reply.
    KeySaved(Result<ModelOptionProvider, Failure>),
    /// Credential removal reply.
    Disconnected(Result<(), Failure>),
    /// Sign-in start reply. `attempt` matches the start that was sent.
    SignInStarted {
        /// Attempt counter.
        attempt: u64,
        /// Start reply.
        result: Result<SignInStart, Failure>,
    },
    /// Poll reply.
    SignInPolled {
        /// Session the poll was for.
        session_id: String,
        /// Poll reply.
        result: Result<PollReply, Failure>,
    },
    /// Sign-out reply.
    SignedOut(Result<(), Failure>),
    /// Endpoint test reply.
    EndpointChecked(Result<EndpointCheck, Failure>),
    /// Endpoint save reply.
    EndpointSaved(Result<EndpointList, Failure>),
    /// Endpoint delete reply.
    EndpointDeleted(Result<EndpointList, Failure>),
}

/// Work the app should do after one step.
#[derive(Clone, Debug, PartialEq)]
pub enum ProvidersEffect {
    /// Load `GET /api/env`.
    LoadEnv,
    /// Load `model.options`.
    LoadOptions,
    /// Load the account list.
    LoadOauth,
    /// Load the endpoint list.
    LoadEndpoints,
    /// Check the typed key for this variable.
    CheckKey {
        /// Variable name.
        env_var: String,
    },
    /// Save the typed key for this provider.
    SaveKey {
        /// Provider slug.
        slug: String,
    },
    /// Zero the secret field.
    ForgetSecret,
    /// Remove credentials for this provider.
    Disconnect {
        /// Provider slug.
        slug: String,
    },
    /// Start a device-code sign-in.
    StartSignIn {
        /// Provider id.
        id: String,
        /// Attempt counter.
        attempt: u64,
    },
    /// Poll a sign-in after `after`.
    Poll {
        /// Provider id.
        id: String,
        /// Session id.
        session_id: String,
        /// Delay before the poll.
        after: Duration,
    },
    /// Cancel a sign-in session.
    CancelSignIn {
        /// Session id.
        session_id: String,
    },
    /// Sign out of an account.
    SignOut {
        /// Provider id.
        id: String,
    },
    /// Test an endpoint draft.
    CheckEndpoint(EndpointDraft),
    /// Save an endpoint draft.
    SaveEndpoint(EndpointDraft),
    /// Delete an endpoint.
    DeleteEndpoint {
        /// Endpoint id.
        id: String,
    },
    /// Open an https link.
    OpenUrl(String),
    /// Copy text that is not a key.
    CopyText(String),
    /// Close the overlay.
    Close,
}

/// What kind of card is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardKind {
    /// API key entry.
    Key,
    /// Remove-credentials confirmation.
    RemoveConfirm,
    /// External account instructions.
    External,
    /// Device-code wait.
    Waiting,
    /// Sign-in finished or stopped.
    Ended,
    /// Sign-out confirmation.
    SignOutConfirm,
    /// Sign-out is not offered.
    DisconnectHint,
    /// Endpoint form.
    EndpointForm,
    /// Delete-endpoint confirmation.
    DeleteConfirm,
}

/// One open card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardView {
    /// Which card.
    pub kind: CardKind,
    /// Body lines. No key text.
    pub lines: Vec<String>,
    /// Card footer.
    pub footer: String,
}

/// What the screen draws. It never carries key text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvidersView {
    /// Title line.
    pub title: String,
    /// Selected tab.
    pub tab: ProvidersTab,
    /// Row lines, including a dim second line when a row has one.
    pub rows: Vec<String>,
    /// Filter text.
    pub filter: String,
    /// Selected row, when the list has one.
    pub selected: Option<usize>,
    /// Open card.
    pub card: Option<CardView>,
    /// Notice line.
    pub notice: Option<String>,
    /// Connection banner.
    pub banner: Option<String>,
    /// Whether the secret field takes input.
    pub secret_focus: bool,
    /// List footer.
    pub footer: String,
}

/// Overlay state. The stub keeps no rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvidersState {
    open: bool,
}

impl Default for ProvidersState {
    fn default() -> Self {
        Self::new()
    }
}

impl ProvidersState {
    /// A closed overlay.
    #[must_use]
    pub fn new() -> Self {
        Self { open: false }
    }
}

/// Apply one input. The stub returns no effects.
#[must_use]
pub fn update(
    state: &mut ProvidersState,
    input: ProvidersInput,
    now: Instant,
) -> Vec<ProvidersEffect> {
    let _ = (state, input, now);
    Vec::new()
}

/// Draw data for the overlay. The stub is an empty screen.
#[must_use]
pub fn view(state: &ProvidersState, now: Instant) -> ProvidersView {
    let _ = (state.open, now);
    ProvidersView {
        title: String::new(),
        tab: ProvidersTab::Keys,
        rows: Vec::new(),
        filter: String::new(),
        selected: None,
        card: None,
        notice: None,
        banner: None,
        secret_focus: false,
        footer: String::new(),
    }
}

/// Next countdown instant. The stub has no card, so there is no deadline.
#[must_use]
pub fn next_deadline(state: &ProvidersState) -> Option<Instant> {
    let _ = state;
    None
}

/// Parse the words after `/providers`.
#[must_use]
pub fn parse_providers_args(args: &str) -> Result<OpenTarget, &'static str> {
    let words: Vec<&str> = args.split_whitespace().collect();
    match words.as_slice() {
        [] => Ok(OpenTarget::Tab(ProvidersTab::Keys)),
        [word] if word.eq_ignore_ascii_case("keys") => Ok(OpenTarget::Tab(ProvidersTab::Keys)),
        [word] if word.eq_ignore_ascii_case("accounts") => {
            Ok(OpenTarget::Tab(ProvidersTab::Accounts))
        }
        [word] if word.eq_ignore_ascii_case("endpoints") => {
            Ok(OpenTarget::Tab(ProvidersTab::Endpoints))
        }
        _ => Err(USAGE),
    }
}
