//! Account rows and sign-in replies. The reducer does not fill them yet.

/// How an account row is signed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountStatus {
    /// Signed in. `expires_in_s` is seconds from the load instant, when known.
    SignedIn {
        /// Where the credential came from.
        source_label: String,
        /// Seconds until expiry. Negative means already expired.
        expires_in_s: Option<i64>,
    },
    /// The status could not be read.
    Unknown(String),
    /// Not signed in.
    NotSignedIn,
}

/// One account provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// Provider id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// True when the flow is device code.
    pub device_code: bool,
    /// Command to run outside this program.
    pub cli_command: Option<String>,
    /// Documentation link.
    pub docs_url: Option<String>,
    /// Hint when sign-out is not offered.
    pub disconnect_hint: Option<String>,
    /// Command shown when sign-out is not offered.
    pub disconnect_command: Option<String>,
    /// Whether sign-out is offered.
    pub disconnectable: bool,
    /// Sign-in status.
    pub status: AccountStatus,
}

/// A started device-code sign-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignInStart {
    /// Session id.
    pub session_id: String,
    /// Code the person types.
    pub user_code: String,
    /// Page to open.
    pub verification_url: String,
    /// Lifetime in seconds, when the server sent one.
    pub expires_in: Option<u64>,
    /// Suggested poll spacing in seconds.
    pub poll_interval: Option<i64>,
}

/// One poll answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollReply {
    /// Status word.
    pub status: PollStatus,
    /// Error text.
    pub error_message: Option<String>,
    /// Account email, when present.
    pub account_email: Option<String>,
    /// Default model, when present.
    pub model: Option<String>,
    /// Whether another try is allowed.
    pub retryable: Option<bool>,
    /// Seconds to wait before another try.
    pub retry_after: Option<u64>,
}

/// Poll status. Anything else is [`PollStatus::Other`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PollStatus {
    /// Still waiting.
    Pending,
    /// Approved.
    Approved,
    /// Denied.
    Denied,
    /// The code expired.
    Expired,
    /// The server reported an error.
    Error,
    /// Cancelled.
    Cancelled,
    /// A status word this program does not know.
    Other(String),
}
