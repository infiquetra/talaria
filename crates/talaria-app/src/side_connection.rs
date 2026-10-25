//! The side connection: a second link for listing, picker renames and exports (#231).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use hermes_client::{
    CallError, CallOptions, Client, CloseReason, ConnectOptions, DiscoverOptions, Endpoint,
    EnsurePolicy, Inbound, InboundStream, ProfileId, Purpose, Scope, TicketError, connect_with,
    discover_with, handshake, mint_ticket,
};
use serde_json::Value;
use tokio::sync::Notify;

/// How long a call whose connection ended waits to learn why it ended.
const ENDING_WAIT: Duration = Duration::from_millis(500);

/// What the side connection may reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SideScope {
    /// Across profiles, over these served profiles.
    Host { profiles: Vec<String> },
    /// The foreground profile only, with why host scope failed.
    ProfileOnly { profile: String, why: String },
}

/// Why the side connection failed to open.
///
/// Each kind has one fixed phrase, so the view says the same thing every time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SideOpenError {
    Discovery(String),
    Ticket(String),
    Connect(String),
    Handshake(String),
}

impl SideOpenError {
    /// The phrase the view shows for this failure.
    pub fn phrase(&self) -> &'static str {
        match self {
            SideOpenError::Discovery(_) => "gateway not found",
            SideOpenError::Ticket(_) => "ticket refused",
            SideOpenError::Connect(_) => "connection refused",
            SideOpenError::Handshake(_) => "handshake failed",
        }
    }
}

/// Why a side call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SideCallError {
    Refused {
        code: i64,
        reason: Option<String>,
        data: Value,
    },
    TooLarge,
    Dropped,
    Timeout,
}

impl SideCallError {
    /// The phrase the view shows for this failure: the refusal's own reason, or
    /// `code <n>` when it carries none.
    pub fn phrase(&self) -> String {
        match self {
            SideCallError::Refused {
                reason: Some(reason),
                ..
            } => reason.clone(),
            SideCallError::Refused { code, .. } => format!("code {code}"),
            SideCallError::TooLarge => "the reply was too large".to_owned(),
            SideCallError::Dropped => "connection closed".to_owned(),
            SideCallError::Timeout => "no answer within 10 s".to_owned(),
        }
    }
}

/// How the side connection ended, as its inbound stream reported it.
#[derive(Default)]
struct Ending {
    /// The connection ended.
    ended: AtomicBool,
    /// It ended on a frame the client could not read, such as a reply over the 8 MiB cap.
    unreadable: AtomicBool,
    /// Wakes a call waiting to learn why the connection ended.
    notify: Notify,
}

/// The client over one connection. No session tracking, no reconnect loop.
pub struct SideConnection {
    client: Client,
    scope: SideScope,
    closed: AtomicBool,
    ending: Arc<Ending>,
}

impl SideConnection {
    /// Discover, mint a host-scoped interactive ticket, connect, handshake.
    /// Falls back once to a profile-scoped ticket when host scope is refused.
    pub async fn open(
        home: &Path,
        foreground_profile: &str,
    ) -> Result<SideConnection, SideOpenError> {
        let options = DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        };
        let endpoint = discover_with(home, &options)
            .await
            .map_err(|error| SideOpenError::Discovery(error.to_string()))?;
        let profile = endpoint
            .resolve_profile(foreground_profile)
            .map_err(|error| SideOpenError::Ticket(error.to_string()))?;
        let host = Scope::host(Purpose::Interactive)
            .map_err(|error| SideOpenError::Ticket(error.to_string()))?;
        match connect_over(home, &endpoint, &profile, host).await {
            Ok((client, inbound)) => {
                let profiles = endpoint
                    .profiles()
                    .iter()
                    .map(|served| served.name.clone())
                    .collect();
                Ok(SideConnection {
                    client,
                    scope: SideScope::Host { profiles },
                    closed: AtomicBool::new(false),
                    ending: watch(inbound),
                })
            }
            Err(why) => {
                // The gateway refused the host scope: the foreground profile alone
                // still answers, so the picker lists that profile only.
                let (client, inbound) = connect_over(home, &endpoint, &profile, Scope::profile())
                    .await
                    .map_err(|error| SideOpenError::Ticket(error.to_string()))?;
                Ok(SideConnection {
                    client,
                    scope: SideScope::ProfileOnly {
                        profile: foreground_profile.to_owned(),
                        why,
                    },
                    closed: AtomicBool::new(false),
                    ending: watch(inbound),
                })
            }
        }
    }

    /// What this connection may reach.
    pub fn scope(&self) -> &SideScope {
        &self.scope
    }

    /// The client over this connection, for a caller that makes its own calls.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// True after the gateway closed it or the connection dropped.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Call a method with a timeout.
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, SideCallError> {
        let result = self
            .client
            .call_raw(
                method,
                params,
                CallOptions {
                    timeout: Some(timeout),
                },
            )
            .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let mut error = call_error(error);
                if matches!(error, SideCallError::Dropped) {
                    self.closed.store(true, Ordering::SeqCst);
                    // The client reports a reply it could not read as a closed connection;
                    // the inbound stream says which it was.
                    let ended = self.ending.notify.notified();
                    if !self.ending.ended.load(Ordering::SeqCst) {
                        let _ = tokio::time::timeout(ENDING_WAIT, ended).await;
                    }
                    if self.ending.unreadable.load(Ordering::SeqCst) {
                        error = SideCallError::TooLarge;
                    }
                }
                Err(error)
            }
        }
    }

    /// Close the connection.
    pub async fn close(self) {
        self.closed.store(true, Ordering::SeqCst);
        self.client.close().await;
    }
}

/// Connect over one scope. The error says why the ticket was refused.
async fn connect_over(
    home: &Path,
    endpoint: &Endpoint,
    profile: &ProfileId,
    scope: Scope,
) -> Result<(Client, InboundStream), String> {
    let ticket = mint_ticket(home, endpoint, profile, Purpose::Interactive, scope)
        .await
        .map_err(|error| match error {
            // The reason a ticket was refused is one fixed phrase.
            TicketError::Refused(_) => "ticket refused".to_owned(),
            other => other.to_string(),
        })?;
    let mut connection = connect_with(endpoint, ticket, &ConnectOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    handshake(&mut connection)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Client::start(connection.into_parts()))
}

/// Refuse every question the gateway asks on the side connection, and note how it ended.
fn watch(mut inbound: InboundStream) -> Arc<Ending> {
    let ending = Arc::new(Ending::default());
    let seen = Arc::clone(&ending);
    tokio::spawn(async move {
        while let Some(item) = inbound.next().await {
            match item {
                // Approvals and questions belong to the foreground connection; the side one
                // gives the refusal the foreground gives a kind it does not support.
                Inbound::ServerRequest(request) => {
                    let kind = serde_json::to_value(&request.request)
                        .ok()
                        .and_then(|value| value.as_object()?.keys().next().cloned())
                        .unwrap_or_else(|| "unknown".to_owned());
                    let _ = request
                        .responder
                        .fail(
                            -32601,
                            &format!("talaria does not support server request: {kind}"),
                        )
                        .await;
                }
                Inbound::Closed(closed) => {
                    // A frame the client cannot read ends the connection this way.
                    if closed.reason == CloseReason::Error(std::io::ErrorKind::InvalidData) {
                        seen.unreadable.store(true, Ordering::SeqCst);
                    }
                    break;
                }
                Inbound::Event(_) | Inbound::Malformed(_) => {}
            }
        }
        seen.ended.store(true, Ordering::SeqCst);
        seen.notify.notify_waiters();
    });
    ending
}

/// The side call's failure.
pub fn call_error(error: CallError) -> SideCallError {
    match error {
        CallError::Rpc(rpc) => SideCallError::Refused {
            code: rpc.code,
            reason: rpc.reason,
            data: rpc.data.unwrap_or(Value::Null),
        },
        CallError::TimedOut { .. } => SideCallError::Timeout,
        _ => SideCallError::Dropped,
    }
}
