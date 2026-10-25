//! Model catalogue calls, saved defaults at launch, and the effort write. Issue 230.

use std::collections::BTreeSet;
use std::path::PathBuf;

use hermes_client::Client;
use talaria_core::models::{ModelsEffect, ModelsInput, SavedDefault};

use crate::launch::{NewSession, StartMode};
use crate::mutation::MutationClient;

/// Runs one model effect against the gateway or the saved file.
pub struct ModelService {
    client: Client,
    mutation: MutationClient,
    models_path: PathBuf,
}

impl ModelService {
    pub fn new(client: Client, models_path: impl Into<PathBuf>) -> Self {
        let mutation = MutationClient::new(client.clone());
        Self {
            client,
            mutation,
            models_path: models_path.into(),
        }
    }

    pub async fn execute(&self, effect: ModelsEffect) -> Vec<ModelsInput> {
        let _ = (&self.client, &self.mutation, &self.models_path, effect);
        todo!()
    }
}

/// Which command-line groups the person already set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlagsGiven {
    pub model_or_provider: bool,
    pub effort: bool,
}

/// The `session.create` parameter names `runtime.describe` offered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferedCreateKeys(pub BTreeSet<String>);

/// Which saved fields were copied onto the new session, and which the gateway does not offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedApplied {
    pub applied: Vec<&'static str>,
    pub dropped_not_offered: Vec<&'static str>,
}

/// Fill empty new-session fields from a saved default the stale check has already kept.
pub fn apply_saved_default(
    new: NewSession,
    saved: Option<&SavedDefault>,
    flags: FlagsGiven,
    offered: &OfferedCreateKeys,
) -> (NewSession, SavedApplied) {
    let _ = (new, saved, flags, offered);
    todo!()
}

/// Resume and continue never read the saved file. A new session does.
pub fn saved_defaults_for_mode(mode: &StartMode) -> bool {
    matches!(mode, StartMode::New)
}

/// The launch step that checks the saved model, fills the create, and retries one refusal.
pub struct SavedLaunchOutcome {
    pub notices: Vec<String>,
    pub session_id: String,
}

pub async fn prepare_saved_launch(
    client: &Client,
    new: NewSession,
    saved: Option<&SavedDefault>,
    flags: FlagsGiven,
    offered: &OfferedCreateKeys,
) -> SavedLaunchOutcome {
    let _ = (client, new, saved, flags, offered);
    todo!()
}
