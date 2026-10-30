//! Remote gateway configuration, authentication and attachment.

pub mod capture;
pub mod connect;
pub mod dial;
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
    pub fn parse(_name: &str) -> Result<Self, RemoteError> {
        todo!("add remote gateway implementation")
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("add remote gateway implementation")
    }
}
impl std::error::Error for SignInError {}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshError {
    SessionExpired,
    ProviderUnreachable,
    Other(String),
}
impl fmt::Display for RefreshError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("add remote gateway implementation")
    }
}
impl std::error::Error for RefreshError {}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteError {
    Insecure { url: String },
    NotGated { url: String },
    NoNativeFlow { url: String },
    NotSignedIn { name: String },
    RefusedAfterRefresh { name: String },
    UnservedProfile(String),
    Other(String),
}
impl fmt::Display for RemoteError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("add remote gateway implementation")
    }
}
impl std::error::Error for RemoteError {}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("add remote gateway implementation")
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
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
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("add remote gateway implementation")
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsTicket(Secret);
impl WsTicket {
    #[cfg(feature = "test-support")]
    pub fn for_test(value: &str) -> Self {
        Self(Secret::for_test(value))
    }
}
impl fmt::Display for WsTicket {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("add remote gateway implementation")
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
