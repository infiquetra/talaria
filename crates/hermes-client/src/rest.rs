use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::{Endpoint, ProfileId, Ticket, TicketError};

pub trait TicketMinter: Send + Sync {
    fn mint(&self) -> Pin<Box<dyn Future<Output = Result<Ticket, TicketError>> + Send + '_>>;
}

pub struct NativeHttpMinter;

impl NativeHttpMinter {
    pub fn new(_home: PathBuf, _endpoint: Arc<Endpoint>, _profile: ProfileId) -> Self {
        todo!("settings stub")
    }
}

impl TicketMinter for NativeHttpMinter {
    fn mint(&self) -> Pin<Box<dyn Future<Output = Result<Ticket, TicketError>> + Send + '_>> {
        todo!("settings stub")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Put,
    Post,
    Delete,
}

#[derive(Clone, Debug)]
pub struct RestOptions {
    pub get_deadline: Duration,
    pub write_deadline: Duration,
    pub max_body: usize,
}

impl Default for RestOptions {
    fn default() -> Self {
        Self {
            get_deadline: Duration::from_secs(15),
            write_deadline: Duration::from_secs(30),
            max_body: 8 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RequestOpts {
    pub ticket: bool,
    pub deadline: Option<Duration>,
}

impl Default for RequestOpts {
    fn default() -> Self {
        Self {
            ticket: true,
            deadline: None,
        }
    }
}

pub struct RestClient;

impl std::fmt::Debug for RestClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RestClient")
    }
}

impl std::fmt::Display for RestClient {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!("settings stub")
    }
}

impl RestClient {
    pub fn new(
        _api_origin: &str,
        _minter: Arc<dyn TicketMinter>,
        _options: RestOptions,
    ) -> Result<Self, RestError> {
        todo!("settings stub")
    }

    pub async fn request(
        &self,
        _method: Method,
        _path_and_query: &str,
        _body: Option<&Value>,
        _opts: RequestOpts,
    ) -> Result<Value, RestError> {
        todo!("settings stub")
    }

    pub async fn get_json(&self, _path_and_query: &str) -> Result<Value, RestError> {
        todo!("settings stub")
    }

    pub async fn put_json(&self, _path: &str, _body: &Value) -> Result<Value, RestError> {
        todo!("settings stub")
    }

    pub async fn post_json(&self, _path: &str, _body: &Value) -> Result<Value, RestError> {
        todo!("settings stub")
    }

    pub async fn delete_json(&self, _path: &str) -> Result<Value, RestError> {
        todo!("settings stub")
    }

    #[doc(hidden)]
    pub async fn probe_ticket_reuse(&self, _path: &str) -> Result<(u16, u16), RestError> {
        todo!("settings stub")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RestError {
    #[error("origin must be loopback")]
    NotLoopback,
    #[error("HTTPS is not supported")]
    TlsNotSupported,
    #[error("could not authenticate")]
    Ticket(TicketError),
    #[error("connection failed")]
    Connect(std::io::Error),
    #[error("request timed out")]
    Timeout { sent: bool },
    #[error("connection dropped")]
    Dropped { sent: bool },
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden { detail: String },
    #[error("route not found")]
    NotFound { route: String },
    #[error("request refused")]
    Refused { status: u16, detail: Option<String> },
    #[error("server error")]
    Server { status: u16 },
    #[error("response body too large")]
    BodyTooLarge,
    #[error("invalid response body")]
    BadBody(String),
}

impl RestError {
    pub fn outcome_unknown(&self) -> bool {
        todo!("settings stub")
    }
}
