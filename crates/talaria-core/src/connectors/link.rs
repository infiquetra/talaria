//! A connect operation after the application has turned the deadline into an instant.

use std::time::Instant;

use serde::Deserialize;

/// One connect or poll snapshot. `deadline` is already clamped.
#[derive(Clone, PartialEq, Eq)]
pub struct Operation {
    pub op_id: String,
    pub deadline: Instant,
    pub settled: bool,
    pub targets: Vec<Target>,
}

impl std::fmt::Debug for Operation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Operation")
            .field("op_id_len", &self.op_id.len())
            .field("settled", &self.settled)
            .field("targets", &self.targets.len())
            .finish()
    }
}

/// The first target is the only one this overlay reads.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Target {
    pub name: String,
    pub state: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub connect_url: Option<String>,
}

impl std::fmt::Debug for Target {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Target")
            .field("name", &self.name)
            .field("state", &self.state)
            .field("detail_len", &self.detail.as_ref().map(String::len))
            .field("link", &self.connect_url.as_ref().map(|_| "[link]"))
            .finish()
    }
}

/// Where a new or repaired account is addressed.
#[derive(Clone, PartialEq, Eq)]
pub enum AccountAddress {
    Alias(String),
    Repair { connection_id: String },
}

impl std::fmt::Debug for AccountAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Alias(value) => f.debug_tuple("Alias").field(&value.len()).finish(),
            Self::Repair { connection_id } => f
                .debug_struct("Repair")
                .field("connection_len", &connection_id.len())
                .finish(),
        }
    }
}
