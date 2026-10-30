//! Remote gateway configuration, authentication and attachment.

pub mod capture;
pub mod connect;
pub mod dial;
mod http;
pub mod loopback;
pub mod native_auth;
pub mod pkce;
pub mod profile_methods;
pub mod status;
pub mod token_store;
pub mod ws_ticket;

use serde_json::Value;
use std::fmt;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use url::Url;

#[derive(Clone, PartialEq, Eq)]
pub struct RemoteName(String);
impl RemoteName {
    pub fn parse(name: &str) -> Result<Self, RemoteError> {
        let valid = !name.is_empty()
            && name.len() <= 32
            && name.as_bytes()[0].is_ascii_alphanumeric()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if valid {
            Ok(Self(name.to_owned()))
        } else {
            Err(RemoteError::InvalidName)
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    #[cfg(feature = "test-support")]
    pub fn for_test(name: &str) -> Self {
        Self(name.to_owned())
    }
}
impl fmt::Debug for RemoteName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("RemoteName").field(&self.0).finish()
    }
}

#[derive(Debug, Clone)]
pub struct RemoteConfig {
    pub name: RemoteName,
    pub base_url: Url,
    pub provider: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateStatus {
    pub auth_required: bool,
    pub auth_providers: Vec<String>,
    pub auth_flows: Vec<String>,
}

pub struct SignInOptions {
    pub no_browser: bool,
    pub paste: bool,
    pub cancel: CancellationToken,
}
#[derive(Clone, PartialEq, Eq)]
pub enum SignInError {
    Insecure,
    NotGated,
    NoNativeFlow,
    StateMismatch,
    Denied(String),
    Cancelled,
    TimedOut,
    CodeRejected,
    RequestRejected,
    OutcomeUnknown(String),
    Io(String),
}
impl fmt::Display for SignInError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Insecure => f.write_str("a remote must use https:// unless it is 127.0.0.1 or [::1]"),
            Self::NotGated => f.write_str("the gateway is not gated"),
            Self::NoNativeFlow => f.write_str("the gateway has no interactive sign-in provider"),
            Self::StateMismatch => f.write_str("Sign-in callback did not match this sign-in (possible cross-site request); nothing was redeemed"),
            Self::Denied(reason) => write!(f, "The gateway refused the sign-in: {}", if matches!(reason.as_str(), "access_denied" | "login_required" | "temporarily_unavailable" | "invalid_request") { reason } else { "denied" }),
            Self::Cancelled => f.write_str("Sign-in cancelled"),
            Self::TimedOut => f.write_str("Sign-in timed out after 10 minutes"),
            Self::CodeRejected => f.write_str("Sign-in code was refused or expired; run talaria remote login <name>"),
            Self::RequestRejected => f.write_str("The gateway rejected Talaria's sign-in request (HTTP 422); this is a Talaria bug, please report it"),
            Self::OutcomeUnknown(cause) => write!(f, "Sign-in outcome unknown ({}); run talaria remote login <name>", match cause.as_str() { "HTTP 500" => "HTTP 500", "HTTP 502" => "HTTP 502", "HTTP 503" => "HTTP 503", "HTTP 504" => "HTTP 504", "timeout" => "timeout", _ => "connection failed" }),
            Self::Io(_) => f.write_str("could not complete sign-in input or output"),
        }
    }
}
impl fmt::Debug for SignInError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for SignInError {}
#[derive(Clone, PartialEq, Eq)]
pub enum RefreshError {
    SessionExpired,
    ProviderUnreachable,
    Other(String),
}
impl fmt::Display for RefreshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SessionExpired => "sign-in expired; run talaria remote login <name>",
            Self::ProviderUnreachable => "the sign-in provider is unreachable (HTTP 503)",
            Self::Other(_) => "could not refresh the sign-in",
        })
    }
}
impl fmt::Debug for RefreshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for RefreshError {}
#[derive(Clone, PartialEq, Eq)]
pub enum RemoteError {
    Insecure { url: String },
    NotGated { url: String },
    NoNativeFlow { url: String },
    NotSignedIn { name: String },
    RefusedAfterRefresh { name: String },
    UnservedProfile(String),
    Other(String),
    InvalidName,
    InvalidUrl,
    TokenPermissions { name: String },
    Certificate,
    Http(u16),
    Refresh(RefreshError),
}
impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Insecure { url } => write!(
                f,
                "Refusing {url}: a remote must use https:// unless it is 127.0.0.1 or [::1]."
            ),
            Self::NotGated { url } => write!(f, "the gateway at {url} is not gated"),
            Self::NoNativeFlow { url } => write!(
                f,
                "the gateway at {url} has no interactive sign-in provider"
            ),
            Self::NotSignedIn { name } => write!(f, "not signed in to {name}."),
            Self::RefusedAfterRefresh { name } => write!(
                f,
                "the gateway refused the sign-in for {name} after a refresh."
            ),
            Self::UnservedProfile(name) => {
                write!(f, "the remote gateway does not serve the profile {name}")
            }
            Self::TokenPermissions { name } => write!(
                f,
                "token file for {name} is readable by others; fix its permissions or sign in again"
            ),
            Self::InvalidName => f.write_str("remote names must match [a-z0-9][a-z0-9-]{0,31}"),
            Self::InvalidUrl => {
                f.write_str("invalid remote URL; user info, query and fragment are not allowed")
            }
            Self::Certificate => f.write_str("the remote certificate could not be verified"),
            Self::Http(status) => write!(f, "the gateway request failed (HTTP {status})"),
            Self::Refresh(error) => fmt::Display::fmt(error, f),
            Self::Other(_) => f.write_str("could not complete the remote operation"),
        }
    }
}
impl fmt::Debug for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for RemoteError {}

#[derive(Clone, PartialEq, Eq, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct Secret(String);
impl Secret {
    #[cfg(feature = "test-support")]
    pub fn for_test(value: &str) -> Self {
        Self(value.to_owned())
    }
    #[cfg(feature = "test-support")]
    pub fn expose_for_test(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct TokenSet {
    access: Secret,
    refresh: Secret,
    pub expires_at: i64,
    pub provider: String,
    pub user_id: String,
    pub saved_at: i64,
}
impl TokenSet {
    #[cfg(feature = "test-support")]
    pub fn access_for_test(&self) -> &str {
        self.access.expose_for_test()
    }
    #[cfg(feature = "test-support")]
    pub fn refresh_for_test(&self) -> &str {
        self.refresh.expose_for_test()
    }
    #[cfg(feature = "test-support")]
    pub fn for_test(access: &str, refresh: &str, expires_at: i64, saved_at: i64) -> Self {
        Self {
            access: Secret::for_test(access),
            refresh: Secret::for_test(refresh),
            expires_at,
            provider: "basic".into(),
            user_id: "fixture".into(),
            saved_at,
        }
    }
}
impl fmt::Display for TokenSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct WsTicket(Secret);
impl WsTicket {
    #[cfg(feature = "test-support")]
    pub fn for_test(value: &str) -> Self {
        Self(Secret::for_test(value))
    }
}
impl fmt::Display for WsTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

pub type ParamInjector = Arc<dyn Fn(&str, &mut Value) + Send + Sync>;
pub use connect::connect_remote;
pub use dial::RemoteDial;
pub use native_auth::{refresh, sign_in};
pub use profile_methods::{PROFILE_METHODS, profile_injector};
pub use status::probe_status;
pub use token_store::TokenStore;
pub use ws_ticket::mint_ws_ticket;

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs().min(i64::MAX as u64) as i64)
}

/// Validate security before any network request. Prefix paths are retained.
pub fn validate_base(base: &Url) -> Result<(), RemoteError> {
    if !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
        || base.host().is_none()
    {
        return Err(RemoteError::InvalidUrl);
    }
    if base.scheme() == "https"
        || (base.scheme() == "http"
            && matches!(base.host(), Some(url::Host::Ipv4(ip)) if ip == std::net::Ipv4Addr::LOCALHOST))
        || (base.scheme() == "http"
            && matches!(base.host(), Some(url::Host::Ipv6(ip)) if ip == std::net::Ipv6Addr::LOCALHOST))
    {
        return Ok(());
    }
    Err(RemoteError::Insecure {
        url: base.to_string(),
    })
}
pub(crate) fn endpoint(base: &Url, path: &str) -> Url {
    let mut url = base.clone();
    url.set_path(&format!("{}{}", base.path().trim_end_matches('/'), path));
    url.set_query(None);
    url.set_fragment(None);
    url
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}
impl fmt::Debug for WsTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}
