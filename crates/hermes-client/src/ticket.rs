//! Mint one control-socket ticket for the session WebSocket.
//!
//! `expires_in_seconds` is ignored. The gateway enforces the lifetime.
//! A host scope cannot be built for the native HTTP purpose.

use std::path::Path;
use std::time::Duration;

use crate::discovery::{
    Endpoint, NodeKind, NodeProblem, ProfileId, unsafe_node_message, unsafe_node_step,
};

/// What the ticket is allowed to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Interactive,
    NativeHttp,
}

impl Purpose {
    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::NativeHttp => "native-http",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    Profile,
    Host,
}

/// Profile or host. A host scope exists only for [`Purpose::Interactive`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    kind: ScopeKind,
}

impl Scope {
    pub fn profile() -> Self {
        Self {
            kind: ScopeKind::Profile,
        }
    }

    pub fn host(purpose: Purpose) -> Result<Self, ScopeError> {
        match purpose {
            Purpose::Interactive => Ok(Self {
                kind: ScopeKind::Host,
            }),
            Purpose::NativeHttp => Err(ScopeError),
        }
    }

    pub(crate) fn as_wire(self) -> &'static str {
        match self.kind {
            ScopeKind::Profile => "profile",
            ScopeKind::Host => "host",
        }
    }

    pub(crate) fn is_host(self) -> bool {
        matches!(self.kind, ScopeKind::Host)
    }
}

/// `Scope::host` was asked for a purpose that cannot carry a host scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeError;

impl std::fmt::Display for ScopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a host ticket needs the interactive purpose")
    }
}

impl std::error::Error for ScopeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TicketOptions {
    pub control_timeout: Duration,
}

impl Default for TicketOptions {
    fn default() -> Self {
        Self {
            control_timeout: Duration::from_secs(5),
        }
    }
}

/// A single-use gateway ticket. The secret stays private.
pub struct Ticket {
    value: String,
    purpose: Purpose,
    identity: Option<(String, ProfileId)>,
}

impl std::fmt::Debug for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = (&self.value, self.purpose, &self.identity);
        f.write_str("Ticket(<redacted>)")
    }
}

impl Ticket {
    /// Accept 1 to 256 letters, digits, `-`, and `_`. Identity is unknown.
    pub fn parse(value: String) -> Result<Self, TicketError> {
        let _ = value;
        todo!("P1-01")
    }

    /// The secret, for the WebSocket subprotocol only.
    pub(crate) fn expose(&self) -> &str {
        let _ = self;
        todo!("P1-01")
    }

    pub(crate) fn purpose(&self) -> Purpose {
        self.purpose
    }

    pub(crate) fn identity(&self) -> Option<(&str, &ProfileId)> {
        self.identity
            .as_ref()
            .map(|(instance, profile)| (instance.as_str(), profile))
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum TicketError {
    StaleInstance,
    BindingRejected,
    NotAuthenticated,
    CapacityExhausted,
    InvalidRequest,
    IdentityChanged,
    Malformed,
    InvalidReply,
    ScopeMismatch,
    Refused(String),
    ReplyTooLarge,
    Timeout,
    Io(std::io::ErrorKind),
    UnsafeNode {
        node: NodeKind,
        problem: NodeProblem,
    },
}

impl std::fmt::Debug for TicketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInstance => f.write_str("StaleInstance"),
            Self::BindingRejected => f.write_str("BindingRejected"),
            Self::NotAuthenticated => f.write_str("NotAuthenticated"),
            Self::CapacityExhausted => f.write_str("CapacityExhausted"),
            Self::InvalidRequest => f.write_str("InvalidRequest"),
            Self::IdentityChanged => f.write_str("IdentityChanged"),
            Self::Malformed => f.write_str("Malformed"),
            Self::InvalidReply => f.write_str("InvalidReply"),
            Self::ScopeMismatch => f.write_str("ScopeMismatch"),
            Self::Refused(_) => f.write_str("Refused(<redacted>)"),
            Self::ReplyTooLarge => f.write_str("ReplyTooLarge"),
            Self::Timeout => f.write_str("Timeout"),
            Self::Io(kind) => f.debug_tuple("Io").field(kind).finish(),
            Self::UnsafeNode { node, problem } => f
                .debug_struct("UnsafeNode")
                .field("node", node)
                .field("problem", problem)
                .finish(),
        }
    }
}

impl std::fmt::Display for TicketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::StaleInstance => "the gateway restarted since discovery",
            Self::BindingRejected => "the gateway does not serve that profile",
            Self::NotAuthenticated => "the gateway could not confirm who is asking",
            Self::CapacityExhausted => "the gateway has too many unused tickets",
            Self::InvalidRequest => "Talaria sent a malformed ticket request",
            Self::IdentityChanged => "the gateway changed while the ticket was requested",
            Self::Malformed => "the gateway returned a ticket in an unexpected form",
            Self::InvalidReply => "the gateway sent a ticket reply Talaria could not read.",
            Self::ScopeMismatch => "a host ticket needs the interactive purpose",
            Self::Refused(_) => "the gateway refused the ticket request.",
            Self::ReplyTooLarge => "the gateway sent a ticket reply that is too large.",
            Self::Timeout => "the gateway did not answer the ticket request in time.",
            Self::Io(_) => "the ticket request failed.",
            Self::UnsafeNode { node, problem } => {
                return f.write_str(unsafe_node_message(*node, *problem));
            }
        };
        f.write_str(text)
    }
}

impl std::error::Error for TicketError {}

impl TicketError {
    pub fn next_step(&self) -> &'static str {
        match self {
            Self::StaleInstance | Self::IdentityChanged => "Discover the gateway again.",
            Self::BindingRejected => "Check the profile name against the list the gateway serves.",
            Self::NotAuthenticated => {
                "Run Talaria as the user who runs the gateway; if the Hermes home is writable by its group, run `chmod g-w` on it."
            }
            Self::CapacityExhausted => "Wait a minute and try again.",
            Self::InvalidRequest | Self::ScopeMismatch => "Report a bug.",
            Self::Malformed => "Update Talaria and Hermes to matching versions.",
            Self::InvalidReply | Self::ReplyTooLarge => "Update Hermes and try again.",
            Self::Refused(_) => "Check `hermes gateway status`.",
            Self::Timeout | Self::Io(_) => "Try again.",
            Self::UnsafeNode { node, problem } => unsafe_node_step(*node, *problem),
        }
    }
}

pub async fn mint_ticket(
    home: &Path,
    endpoint: &Endpoint,
    profile_id: &ProfileId,
    purpose: Purpose,
    scope: Scope,
) -> Result<Ticket, TicketError> {
    mint_ticket_with(
        home,
        endpoint,
        profile_id,
        purpose,
        scope,
        &TicketOptions::default(),
    )
    .await
}

pub async fn mint_ticket_with(
    home: &Path,
    endpoint: &Endpoint,
    profile_id: &ProfileId,
    purpose: Purpose,
    scope: Scope,
    options: &TicketOptions,
) -> Result<Ticket, TicketError> {
    let _ = (home, endpoint, profile_id, purpose, scope, options);
    todo!("P1-01")
}
