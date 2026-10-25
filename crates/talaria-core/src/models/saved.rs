//! Saved favourites and per-profile defaults. Issue 230.

use std::collections::BTreeMap;

use hermes_contract::schemas::ModelOptionsResult;

use super::effort::Effort;
use super::picker::{EffortCommand, ModelCommand};

/// One profile's saved model choice. Each field may be absent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedDefault {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub effort: Option<Effort>,
}

/// The on-disk file. Unknown top-level keys are dropped on the next write.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedModels {
    pub favourites: Vec<String>,
    pub defaults: BTreeMap<String, SavedDefault>,
}

impl SavedModels {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let _ = text;
        todo!()
    }

    pub fn to_json(&self) -> Result<String, String> {
        let _ = self;
        todo!()
    }
}

/// Why a saved model and provider were dropped together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaleReason {
    ProviderNotSetUp,
    ModelNotListed,
    ListUnreadable,
}

/// The stale check's answer. The effort is never dropped here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedCheck {
    Keep,
    DropModelAndProvider(StaleReason),
}

pub fn check_saved_default(
    saved: &SavedDefault,
    options: Option<&ModelOptionsResult>,
) -> SavedCheck {
    let _ = (saved, options);
    todo!()
}

/// Parse the words after `/model`. The error text is the usage line.
pub fn parse_model_command(arg: &str) -> Result<ModelCommand, String> {
    let _ = arg;
    todo!()
}

/// Parse the words after `/effort`. The error text is the usage line.
pub fn parse_effort_command(arg: &str) -> Result<EffortCommand, String> {
    let _ = arg;
    todo!()
}
