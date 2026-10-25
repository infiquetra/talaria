//! Model catalogue calls, saved defaults at launch, and the effort write.

use std::collections::BTreeSet;
use std::path::PathBuf;

use hermes_client::{CallError, Client};
use hermes_contract::{
    methods::{
        canonical::{config_get, config_set},
        legacy::model_options,
    },
    schemas::{CanonicalConfigSetParams, ConfigGetParams, ModelOptionsParams, ModelOptionsResult},
};
use talaria_core::models::{
    ModelsEffect, ModelsInput, SavedCheck, SavedDefault, StaleReason, check_saved_default,
    notice_not_offered, notice_stale, notice_t20,
};

use crate::launch::{NewSession, SessionRequest, StartMode, new_request_id, plain};
use crate::models_file::SavedModelsFile;
use crate::mutation::{MutationClient, call_outcome};
use crate::sessions::{OpenError, OpenedSession, SessionService};

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
        let input = match effect {
            ModelsEffect::LoadOptions {
                session_id,
                refresh,
            } => match load_options(&self.client, Some(session_id.clone()), true, refresh).await {
                Ok(result) => ModelsInput::OptionsLoaded { session_id, result },
                Err(error) => ModelsInput::OptionsFailed {
                    session_id,
                    unknown_method: unknown_method(&error),
                    reason: error.to_string(),
                },
            },
            ModelsEffect::ReadEffort { session_id } => {
                let params = ConfigGetParams {
                    session_id: Some(session_id.clone()),
                    key: "reasoning".into(),
                    cwd: None,
                    profile: None,
                };
                match self.client.call::<config_get::Marker>(params).await {
                    Ok(result) => ModelsInput::EffortRead {
                        session_id,
                        value: result.value,
                    },
                    Err(error) => ModelsInput::EffortReadFailed {
                        session_id,
                        unknown_method: unknown_method(&error),
                        reason: error.to_string(),
                    },
                }
            }
            ModelsEffect::Mutate(request) => {
                let outcome = self.mutation.send(&request).await;
                ModelsInput::MutationReturned { request, outcome }
            }
            ModelsEffect::SetEffort { session_id, level } => {
                let params = CanonicalConfigSetParams {
                    session_id: Some(session_id.clone()),
                    key: "reasoning".into(),
                    value: level.map_or("reset", |level| level.as_wire()).into(),
                    confirm_expensive_model: None,
                    profile: None,
                    scope: None,
                };
                let outcome = self
                    .client
                    .call::<config_set::Marker>(params)
                    .await
                    .map(|_| ())
                    .map_err(call_outcome);
                ModelsInput::EffortSetReturned {
                    session_id,
                    level,
                    outcome,
                }
            }
            ModelsEffect::SaveFile(saved) => {
                let path = self.models_path.clone();
                match tokio::task::spawn_blocking(move || saved.write(&path)).await {
                    Ok(Ok(())) => ModelsInput::FileSaved,
                    Ok(Err(reason)) => ModelsInput::FileFailed { reason },
                    Err(_) => ModelsInput::FileFailed {
                        reason: "the file writer stopped".into(),
                    },
                }
            }
            ModelsEffect::OpenOverlay
            | ModelsEffect::CloseOverlay
            | ModelsEffect::Notice { .. } => {
                return Vec::new();
            }
        };
        vec![input]
    }
}

fn unknown_method(error: &CallError) -> bool {
    matches!(error, CallError::Rpc(error) if error.code == -32601)
}

async fn load_options(
    client: &Client,
    session_id: Option<String>,
    include_unconfigured: bool,
    refresh: bool,
) -> Result<ModelOptionsResult, CallError> {
    client
        .call::<model_options::Marker>(ModelOptionsParams {
            explicit_only: true,
            include_unconfigured,
            refresh,
            profile: None,
            session_id,
        })
        .await
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
    mut new: NewSession,
    saved: Option<&SavedDefault>,
    flags: FlagsGiven,
    offered: &OfferedCreateKeys,
) -> (NewSession, SavedApplied) {
    let mut applied = SavedApplied {
        applied: Vec::new(),
        dropped_not_offered: Vec::new(),
    };
    let Some(saved) = saved else {
        return (new, applied);
    };
    let fields = [
        (
            "model",
            !flags.model_or_provider,
            saved.model.clone(),
            &mut new.model,
        ),
        (
            "provider",
            !flags.model_or_provider,
            saved.provider.clone(),
            &mut new.provider,
        ),
        (
            "reasoning",
            !flags.effort,
            saved.effort.map(|effort| effort.as_wire().to_string()),
            &mut new.reasoning,
        ),
    ];
    for (key, enabled, value, destination) in fields {
        if enabled
            && destination.is_none()
            && let Some(value) = value
        {
            if offered.0.contains(key) {
                *destination = Some(value);
                applied.applied.push(key);
            } else {
                applied.dropped_not_offered.push(key);
            }
        }
    }
    (new, applied)
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
    let service = SessionService::with_client("saved-launch", client.clone());
    match open_with_saved_default(&service, new, saved, flags, offered).await {
        Ok((opened, notices)) => SavedLaunchOutcome {
            notices,
            session_id: opened.session_id,
        },
        Err(_) => SavedLaunchOutcome {
            notices: vec!["The session could not be created.".into()],
            session_id: String::new(),
        },
    }
}

/// The shared preflight and create path for interactive and plain launches.
pub async fn open_with_saved_default(
    service: &SessionService,
    new: NewSession,
    saved: Option<&SavedDefault>,
    flags: FlagsGiven,
    offered: &OfferedCreateKeys,
) -> Result<(OpenedSession, Vec<String>), OpenError> {
    let mut notices = Vec::new();
    let mut kept = saved.cloned();
    if !flags.model_or_provider
        && let Some(saved) = kept.as_mut()
        && (saved.model.is_some() || saved.provider.is_some())
    {
        let options = match service.client() {
            Some(client) => load_options(client, None, false, false).await.ok(),
            None => None,
        };
        if let SavedCheck::DropModelAndProvider(why) = check_saved_default(saved, options.as_ref())
        {
            let why = match why {
                StaleReason::ProviderNotSetUp => "the provider is not set up",
                StaleReason::ModelNotListed => "the model is not listed",
                StaleReason::ListUnreadable => "the model list could not be read",
            };
            notices.push(notice_stale(
                &plain(saved.model.as_deref().unwrap_or_default()),
                &plain(saved.provider.as_deref().unwrap_or_default()),
                why,
            ));
            saved.model = None;
            saved.provider = None;
        }
    }
    let (mut new, applied) = apply_saved_default(new, kept.as_ref(), flags, offered);
    notices.extend(
        applied
            .dropped_not_offered
            .iter()
            .map(|key| notice_not_offered(key)),
    );
    match service.open_planned(SessionRequest::New(new.clone())).await {
        Ok(opened) => Ok((opened, notices)),
        Err(error)
            if !applied.applied.is_empty()
                && matches!(error, OpenError::Refused { .. } | OpenError::NotFound) =>
        {
            let reason = match &error {
                OpenError::Refused { reason } => reason.as_str(),
                OpenError::NotFound => "not_found",
                _ => return Err(error),
            };
            let what = saved_description(&new, &applied);
            for key in applied.applied {
                match key {
                    "model" => new.model = None,
                    "provider" => new.provider = None,
                    "reasoning" => new.reasoning = None,
                    _ => {}
                }
            }
            new.request_id = new_request_id();
            notices.push(notice_t20(&what, &plain(reason)));
            service
                .open_planned(SessionRequest::New(new))
                .await
                .map(|opened| (opened, notices))
        }
        Err(error) => Err(error),
    }
}

fn saved_description(new: &NewSession, applied: &SavedApplied) -> String {
    let mut parts = Vec::new();
    if applied.applied.contains(&"model") || applied.applied.contains(&"provider") {
        match (&new.model, &new.provider) {
            (Some(model), Some(provider)) => {
                parts.push(format!("{} on {}", plain(model), plain(provider)))
            }
            (Some(model), None) => parts.push(plain(model)),
            (None, Some(provider)) => parts.push(format!("provider {}", plain(provider))),
            (None, None) => {}
        }
    }
    if applied.applied.contains(&"reasoning")
        && let Some(reasoning) = &new.reasoning
    {
        parts.push(format!("effort {}", plain(reasoning)));
    }
    parts.join(", ")
}
