//! Small fence rules on top of the mutation ledger (#231).

use crate::mutation::{MutationLedger, MutationOutcome, MutationRequest};

/// Seed revision 0 and generation 0 for a session the ledger holds no fences
/// for. Adopting is monotonic, so a seed never lowers a real value.
pub fn seed_untracked(_ledger: &mut MutationLedger, _session_id: &str) {
    unimplemented!("#231")
}

/// A branch may retry once only when the refusal is a fence refusal whose
/// generation equals the generation that was sent.
pub fn branch_may_retry(_sent_generation: i64, _outcome: &MutationOutcome) -> bool {
    unimplemented!("#231")
}

/// The params for the side connection: the request's params with `profile`
/// inserted when one is given.
pub fn with_profile(_request: &MutationRequest, _profile: Option<&str>) -> serde_json::Value {
    unimplemented!("#231")
}
