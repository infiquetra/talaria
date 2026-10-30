//! An externally owned fleet session.

use super::{FleetEffect, FleetInput, FleetRegistry, NeedSlot, Outcome, RowKey, RowState, update};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FleetBackend {
    Hermes,
    Acp,
    Herdr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalOption {
    pub id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExternalNeed {
    pub slot: NeedSlot,
    pub text: String,
    pub options: Vec<ExternalOption>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExternalSession {
    pub key: RowKey,
    pub backend: FleetBackend,
    pub agent: String,
    pub title: String,
    pub model: Option<String>,
    pub activity: Option<String>,
    pub state: RowState,
    pub outcome: Option<Outcome>,
    pub used: Option<u64>,
    pub size: Option<u64>,
    pub cost: Option<(f64, String)>,
    pub needs: Vec<ExternalNeed>,
    pub at: f64,
}

impl FleetRegistry {
    pub fn observe_external(&mut self, session: ExternalSession) -> Vec<FleetEffect> {
        update(self, FleetInput::External(session))
    }
}
