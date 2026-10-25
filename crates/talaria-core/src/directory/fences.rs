//! Small fence rules on top of the mutation ledger (#231).

use crate::mutation::{MutationLedger, MutationOutcome, MutationRequest};

/// Seed revision 0 and generation 0 for a session the ledger holds no fences
/// for. Adopting is monotonic, so a seed never lowers a real value.
pub fn seed_untracked(ledger: &mut MutationLedger, session_id: &str) {
    let fences = ledger.fences(session_id);
    if fences.revision.is_none() && fences.execution_generation.is_none() {
        ledger.fences_mut(session_id).adopt(Some(0), Some(0));
    }
}

/// A branch may retry once only when the refusal is a fence refusal whose
/// generation equals the generation that was sent.
pub fn branch_may_retry(sent_generation: i64, outcome: &MutationOutcome) -> bool {
    matches!(
        outcome,
        MutationOutcome::FenceRefused {
            execution_generation: Some(generation),
            ..
        } if *generation == sent_generation
    )
}

/// The params for the side connection: the request's params with `profile`
/// inserted when one is given.
pub fn with_profile(request: &MutationRequest, profile: Option<&str>) -> serde_json::Value {
    let mut params = request.to_params();
    if let Some(profile) = profile
        && let Some(object) = params.as_object_mut()
    {
        object.insert(
            "profile".to_owned(),
            serde_json::Value::String(profile.to_owned()),
        );
    }
    params
}
