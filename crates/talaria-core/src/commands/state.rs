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

impl CommandsState {
    fn fetch(&mut self, after: Duration) -> CommandsEffect {
        let request_id = self.next_id;
        self.next_id += 1;
        self.in_flight = Some(request_id);
        CommandsEffect::FetchCatalog {
            request_id,
            session_id: self.bound_session.clone(),
            after,
        }
    }

    fn loading(&mut self) {
        if self.status != CatalogStatus::Ready {
            self.status = CatalogStatus::Loading;
        }
    }
}

/// How long the one automatic retry waits after a failed fetch.
pub const RETRY_AFTER: Duration = Duration::from_secs(2);

/// Apply one input. Issue #223.
///
/// A new connection or a new foreground session fetches at once and abandons a fetch in
/// flight, whose reply then has an older id and is ignored. `skills.changed` during a fetch
/// asks for exactly one more fetch after it. A failed fetch keeps the old catalogue and is
/// retried once, two seconds later.
#[must_use]
pub fn update(state: &mut CommandsState, input: CommandsInput) -> Vec<CommandsEffect> {
    match input {
        CommandsInput::Connected => {
            state.refused.clear();
            state.retried = false;
            state.dirty = false;
            state.loading();
            vec![state.fetch(Duration::ZERO)]
        }
        CommandsInput::ForegroundSession(session) => {
            if session == state.bound_session {
                return Vec::new();
            }
            state.bound_session = session;
            state.dirty = false;
            state.loading();
            vec![state.fetch(Duration::ZERO)]
        }
        CommandsInput::SkillsChanged => {
            if state.in_flight.is_some() {
                state.dirty = true;
                Vec::new()
            } else {
                vec![state.fetch(Duration::ZERO)]
            }
        }
        CommandsInput::CatalogArrived { request_id, result } => {
            if state.in_flight != Some(request_id) {
                return Vec::new();
            }
            state.in_flight = None;
            state.catalog = Some(CommandCatalog::from_result(&result));
            state.status = CatalogStatus::Ready;
            state.retried = false;
            let mut effects = vec![CommandsEffect::CatalogUpdated];
            if state.dirty {
                state.dirty = false;
                effects.push(state.fetch(Duration::ZERO));
            }
            effects
        }
        CommandsInput::CatalogFailed { request_id, reason } => {
            if state.in_flight != Some(request_id) {
                return Vec::new();
            }
            state.in_flight = None;
            state.status = CatalogStatus::Failed(reason);
            let mut effects = vec![CommandsEffect::CatalogUpdated];
            if state.dirty {
                state.dirty = false;
                effects.push(state.fetch(Duration::ZERO));
            } else if !state.retried {
                state.retried = true;
                effects.push(state.fetch(RETRY_AFTER));
            }
            effects
        }
        CommandsInput::Refused { name } => {
            let bare = name.trim().trim_start_matches('/').to_lowercase();
            if !bare.is_empty() {
                state.refused.insert(bare);
            }
            vec![CommandsEffect::CatalogUpdated]
        }
    }
}
