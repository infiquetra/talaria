//! One stored result, or a single correction turn.
use crate::engine::AgentRequest;
use crate::journal::{NextStep, ResolvedLaunch, SettledTurn, TurnPolicy};

#[derive(Clone, Copy, Debug, Default)]
pub struct SchemaTurnPolicy;

impl TurnPolicy for SchemaTurnPolicy {
    fn first_turn(&self, request: &AgentRequest, launch: &ResolvedLaunch) -> String {
        let _ = (request, launch);
        todo!("hermes binding stub")
    }

    fn after_turn(&self, request: &AgentRequest, turns: &[SettledTurn]) -> NextStep {
        let _ = (request, turns);
        todo!("hermes binding stub")
    }
}
