//! When the catalogue is fetched. Issue #223.

use std::collections::BTreeSet;
use std::time::Duration;

use super::catalog::{CatalogResult, CommandCatalog};

/// Whether a catalogue is held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogStatus {
    /// No fetch has been started.
    NotLoaded,
    /// A fetch is in flight and no catalogue is held.
    Loading,
    /// A catalogue is held.
    Ready,
    /// The last fetch failed. An older catalogue may still be held.
    Failed(String),
}

/// Catalogue store for one connection. Issue #223.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandsState {
    /// The catalogue, once one has arrived.
    pub catalog: Option<CommandCatalog>,
    /// Load state.
    pub status: CatalogStatus,
    /// Id of the fetch in flight.
    pub in_flight: Option<u64>,
    /// A skills change arrived during a fetch.
    pub dirty: bool,
    /// The one automatic retry has been scheduled or used.
    pub retried: bool,
    /// Session the next fetch is bound to.
    pub bound_session: Option<String>,
    /// Bare names refused on this connection.
    pub refused: BTreeSet<String>,
    /// Next request id.
    pub next_id: u64,
}

impl Default for CommandsState {
    fn default() -> Self {
        Self {
            catalog: None,
            status: CatalogStatus::NotLoaded,
            in_flight: None,
            dirty: false,
            retried: false,
            bound_session: None,
            refused: BTreeSet::new(),
            next_id: 1,
        }
    }
}

/// What happened to the connection or a fetch.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum CommandsInput {
    /// A new connection generation, including a reconnect.
    Connected,
    /// The foreground session changed. `None` clears it.
    ForegroundSession(Option<String>),
    /// `skills.changed` arrived.
    SkillsChanged,
    /// A catalogue reply for `request_id`.
    CatalogArrived {
        request_id: u64,
        result: CatalogResult,
    },
    /// That fetch failed.
    CatalogFailed { request_id: u64, reason: String },
    /// A bare command name was refused.
    Refused { name: String },
}

/// What the service should do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandsEffect {
    /// Fetch the catalogue after `after`.
    FetchCatalog {
        request_id: u64,
        session_id: Option<String>,
        after: Duration,
    },
    /// Rows should be derived again.
    CatalogUpdated,
}

/// Apply one input. Issue #223. The body is a stub.
#[must_use]
pub fn update(state: &mut CommandsState, input: CommandsInput) -> Vec<CommandsEffect> {
    let _ = (state, input);
    todo!()
}
