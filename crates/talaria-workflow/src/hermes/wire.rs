//! Profile-bound transport adapters.
use futures::future::BoxFuture;
use hermes_client::liveness::ConnectHook;
pub use hermes_client::replay::Cursor;
pub use hermes_contract::events::EventFrame;
use serde_json::{Map, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::mpsc;

pub trait Wire: Send + Sync + 'static {
    fn call(
        &self,
        method: &'static str,
        params: Map<String, Value>,
        timeout: Duration,
    ) -> BoxFuture<'static, Result<Value, WireError>>;
    fn generation(&self) -> u64;
    fn track(&self, session_id: &str, cursor: Cursor);
    fn resync(&self, session_id: &str, cursor: Cursor);
    fn take_events(&self) -> Option<mpsc::Receiver<WireEvent>>;
    fn dropped_items(&self) -> u64 {
        0
    }
}

pub trait WireFactory: Send + Sync + 'static {
    fn open(&self, profile: &str) -> BoxFuture<'static, Result<Arc<dyn Wire>, WireError>>;
}

#[derive(Clone, Debug)]
pub enum WireEvent {
    Event {
        session_id: String,
        frame: EventFrame,
    },
    Gap {
        session_id: String,
        reason: String,
    },
    Gone {
        session_id: String,
    },
    Reattached {
        session_id: String,
    },
    Reconnected {
        generation: u64,
    },
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum WireError {
    #[error("gateway RPC refused with code {code}")]
    Refused {
        code: i64,
        message: String,
        data: Value,
    },
    #[error("connection lost")]
    Closed,
    #[error("gateway timeout")]
    Timeout,
    #[error("profile unavailable")]
    NotServed(String),
    #[error("wire failure: {0}")]
    Other(String),
}

#[derive(Clone)]
pub struct LinkWireFactory {
    home: PathBuf,
    hook: Option<ConnectHook>,
}

impl LinkWireFactory {
    pub fn new(home: PathBuf) -> Self {
        Self { home, hook: None }
    }

    pub fn on_connect(mut self, hook: ConnectHook) -> Self {
        self.hook = Some(hook);
        self
    }
}

impl WireFactory for LinkWireFactory {
    fn open(&self, profile: &str) -> BoxFuture<'static, Result<Arc<dyn Wire>, WireError>> {
        let _ = (profile, &self.home, &self.hook);
        todo!("hermes binding stub")
    }
}

pub struct LinkWire;

pub fn accept_wire_fixture(type_name: &str, value: &Value) -> Result<(), String> {
    let _ = (type_name, value);
    todo!("hermes binding stub")
}
