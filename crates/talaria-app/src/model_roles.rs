//! Runs one model-role effect and turns the reply into inputs.

use std::path::PathBuf;
use std::sync::Arc;

use hermes_client::{
    AssignScope, AssignmentReply, AuxiliaryModels, Client, EffortField, MoaConfig, MoaPreset,
    MoaSlot, ModelAssignment, ModelInfo, NamedPreset, RestClient, RestError, RestOptions,
    RouteFields, assign_model, auxiliary_models, config_read, config_write, moa_config, model_info,
    save_moa,
};
use hermes_contract::{
    methods::legacy::model_options,
    schemas::{ModelOptionsParams, ModelOptionsResult},
};
use serde_json::{Map, Value};
use talaria_core::model_roles::{
    ApplyReply, ApplyStep, AssignOutcome, AssignScopeView, Assignment, AuxSlot, AuxView,
    EffortChange, Loaded, MoaConfigView, MoaPresetView, RawRoles, Read, RolesEffect, RolesError,
    RolesInfo, RolesInput, RouteCatalog, RouteTriple, Section, SlotView, StaleSlot,
};

/// What the page needs in order to read and write one profile.
pub struct ModelRolesService {
    profile: String,
    connection_profile: String,
    api_origin: Option<String>,
    rest: Option<RestClient>,
    socket: Option<Client>,
    served_profiles: Vec<String>,
    saved_default: Option<(String, String)>,
}

/// How to build the service for one open page.
pub struct ModelRolesOpen {
    pub profile: String,
    pub connection_profile: String,
    pub api_origin: Option<String>,
    pub rest: Option<RestClient>,
    pub socket: Option<Client>,
    /// Profile names this gateway serves. An empty list means the check is not available.
    pub served_profiles: Vec<String>,
}

/// The keys of the raw configuration that hold delegation model roles.
const DELEGATION_KEYS: [&str; 7] = [
    "model",
    "provider",
    "base_url",
    "api_key",
    "api_mode",
    "reasoning_effort",
    "fallback_providers",
];

impl ModelRolesService {
    pub fn new(open: ModelRolesOpen) -> Self {
        Self {
            profile: open.profile,
            connection_profile: open.connection_profile,
            api_origin: open.api_origin,
            rest: open.rest,
            socket: open.socket,
            served_profiles: open.served_profiles,
            saved_default: None,
        }
    }

    /// Talaria's own saved default for the page's profile, sent with every full load.
    #[must_use]
    pub fn with_saved_default(mut self, saved: Option<(String, String)>) -> Self {
        self.saved_default = saved;
        self
    }

    pub async fn execute(&self, effect: RolesEffect) -> Vec<RolesInput> {
        if matches!(effect, RolesEffect::Close) {
            return Vec::new();
        }
        if self.api_origin.is_none() {
            return vec![RolesInput::NotConnected];
        }
        if !self.served_profiles.is_empty()
            && !self
                .served_profiles
                .iter()
                .any(|name| name == &self.profile)
        {
            return vec![RolesInput::NotServed {
                profile: self.profile.clone(),
            }];
        }
        if let RolesEffect::LoadCatalog { refresh } = effect {
            return vec![self.load_catalog(refresh).await];
        }
        let Some(rest) = self.rest.as_ref() else {
            return vec![RolesInput::NotConnected];
        };
        match effect {
            RolesEffect::LoadAll => {
                let (info, auxiliary, moa, raw) = tokio::join!(
                    read_one(rest, Read::Info),
                    read_one(rest, Read::Auxiliary),
                    read_one(rest, Read::Moa),
                    read_one(rest, Read::Raw),
                );
                vec![
                    info,
                    auxiliary,
                    moa,
                    raw,
                    RolesInput::SavedDefault(self.saved_default.clone()),
                ]
            }
            RolesEffect::Load { read } => vec![read_one(rest, read).await],
            RolesEffect::FetchFresh { section } => vec![fetch(rest, section, false).await],
            RolesEffect::ReadBack { section } => vec![fetch(rest, section, true).await],
            RolesEffect::Assign { step, assignment } => {
                vec![assign(rest, step, &assignment).await]
            }
            RolesEffect::PutConfig { step, patch } => {
                vec![match config_write(rest, &patch).await {
                    Ok(()) => RolesInput::Applied {
                        step,
                        reply: ApplyReply::Saved,
                    },
                    Err(error) => RolesInput::ApplyFailed {
                        step,
                        error: roles_error(&error, true, false),
                    },
                }]
            }
            RolesEffect::SaveMoa(view) => {
                vec![match save_moa(rest, &moa_wire(&view)).await {
                    Ok(saved) => RolesInput::Applied {
                        step: ApplyStep::Moa,
                        reply: ApplyReply::MoaSaved(moa_view(saved)),
                    },
                    Err(error) => RolesInput::ApplyFailed {
                        step: ApplyStep::Moa,
                        error: roles_error(&error, true, false),
                    },
                }]
            }
            RolesEffect::LoadCatalog { .. } | RolesEffect::Close => Vec::new(),
        }
    }

    /// The route list comes over the socket. The profile is named only when it is not the
    /// connection's own.
    async fn load_catalog(&self, refresh: bool) -> RolesInput {
        let Some(socket) = self.socket.as_ref() else {
            return RolesInput::CatalogFailed {
                reason: "Talaria is not connected to the gateway".to_owned(),
            };
        };
        let profile = (self.profile != self.connection_profile).then(|| self.profile.clone());
        let params = ModelOptionsParams {
            explicit_only: true,
            include_unconfigured: true,
            refresh,
            profile,
            session_id: None,
        };
        match socket.call::<model_options::Marker>(params).await {
            Ok(result) => RolesInput::CatalogLoaded(catalog(&result)),
            Err(error) => RolesInput::CatalogFailed {
                reason: error.to_string(),
            },
        }
    }
}

fn catalog(result: &ModelOptionsResult) -> RouteCatalog {
    RouteCatalog::from_result(result)
}

/// One read becomes a loaded value or a failure of its own.
async fn read_one(rest: &RestClient, read: Read) -> RolesInput {
    match load(rest, read).await {
        Ok(loaded) => RolesInput::Loaded(loaded),
        Err(error) => RolesInput::LoadFailed {
            read,
            error: roles_error(&error, false, false),
        },
    }
}

async fn load(rest: &RestClient, read: Read) -> Result<Loaded, RestError> {
    Ok(match read {
        Read::Info => Loaded::Info(info_view(model_info(rest).await?)),
        Read::Auxiliary => Loaded::Auxiliary(aux_view(auxiliary_models(rest).await?)),
        Read::Moa => Loaded::Moa(moa_view(moa_config(rest).await?)),
        Read::Raw => Loaded::Raw(raw_view(&config_read(rest, false).await?)),
    })
}

/// The reads one section needs, for the conflict check or the read-back after a write.
fn reads_for(section: Section, read_back: bool) -> Vec<Read> {
    match section {
        Section::Main if read_back => vec![Read::Info, Read::Auxiliary, Read::Raw],
        Section::Main => vec![Read::Auxiliary, Read::Raw],
        Section::Helpers => vec![Read::Auxiliary],
        Section::Fallbacks | Section::Subagents => vec![Read::Raw],
        Section::Moa if read_back => vec![Read::Moa, Read::Raw],
        Section::Moa => vec![Read::Moa],
    }
}

async fn fetch(rest: &RestClient, section: Section, read_back: bool) -> RolesInput {
    let reads = reads_for(section, read_back);
    let results = load_many(rest, &reads).await;
    let mut loaded = Vec::new();
    for result in results {
        match result {
            Ok(value) => loaded.push(value),
            Err(error) => {
                let error = roles_error(&error, false, false);
                return if read_back {
                    RolesInput::ReadBackFailed { section, error }
                } else {
                    RolesInput::FreshFailed { section, error }
                };
            }
        }
    }
    if read_back {
        RolesInput::ReadBack {
            section,
            reads: loaded,
        }
    } else {
        RolesInput::Fresh {
            section,
            reads: loaded,
        }
    }
}

/// Runs the reads side by side and keeps their order.
async fn load_many(rest: &RestClient, reads: &[Read]) -> Vec<Result<Loaded, RestError>> {
    match reads {
        [a] => vec![load(rest, *a).await],
        [a, b] => {
            let (a, b) = tokio::join!(load(rest, *a), load(rest, *b));
            vec![a, b]
        }
        [a, b, c] => {
            let (a, b, c) = tokio::join!(load(rest, *a), load(rest, *b), load(rest, *c));
            vec![a, b, c]
        }
        _ => {
            let mut results = Vec::new();
            for read in reads {
                results.push(load(rest, *read).await);
            }
            results
        }
    }
}

async fn assign(rest: &RestClient, step: ApplyStep, assignment: &Assignment) -> RolesInput {
    let wire = ModelAssignment {
        scope: match assignment.scope {
            AssignScopeView::Main => AssignScope::Main,
            AssignScopeView::Auxiliary => AssignScope::Auxiliary,
        },
        provider: assignment.provider.clone(),
        model: assignment.model.clone(),
        task: assignment.task.clone(),
        reasoning_effort: match &assignment.reasoning_effort {
            EffortChange::Omit => EffortField::Omit,
            EffortChange::Inherit => EffortField::Inherit,
            EffortChange::Level(level) => EffortField::Level(level.as_wire().to_owned()),
        },
        base_url: assignment.base_url.clone(),
        confirm_expensive_model: assignment.confirmed,
    };
    match assign_model(rest, &wire).await {
        Ok(AssignmentReply::Main {
            provider,
            model,
            stale_aux,
            ..
        }) => RolesInput::Applied {
            step,
            reply: ApplyReply::Assigned(AssignOutcome::Main {
                provider,
                model,
                stale: stale_aux
                    .into_iter()
                    .map(|slot| StaleSlot {
                        task: slot.task,
                        provider: slot.provider,
                        model: slot.model,
                    })
                    .collect(),
            }),
        },
        Ok(AssignmentReply::Auxiliary { .. }) => RolesInput::Applied {
            step,
            reply: ApplyReply::Assigned(AssignOutcome::Auxiliary),
        },
        Ok(AssignmentReply::ConfirmRequired { message }) => RolesInput::Applied {
            step,
            reply: ApplyReply::Assigned(AssignOutcome::ConfirmRequired { message }),
        },
        // The page never asks for a reset, so Hermes resetting every slot is not a reply the
        // request could have earned: treat what was written as unknown.
        Ok(AssignmentReply::Reset) => RolesInput::ApplyFailed {
            step,
            error: RolesError::OutcomeUnknown,
        },
        Err(error) => RolesInput::ApplyFailed {
            step,
            error: roles_error(&error, true, true),
        },
    }
}

/// Maps the transport error to the page's error. `write` marks a change request, and
/// `model_set` the route whose 503 means the gateway needs a restart.
fn roles_error(error: &RestError, write: bool, model_set: bool) -> RolesError {
    if write && error.outcome_unknown() {
        return RolesError::OutcomeUnknown;
    }
    match error {
        RestError::NotLoopback
        | RestError::TlsNotSupported
        | RestError::Connect(_)
        | RestError::Timeout { .. }
        | RestError::Dropped { .. } => RolesError::NotSent {
            reason: error.to_string(),
        },
        RestError::BodyTooLarge | RestError::BadBody(_) if write => RolesError::OutcomeUnknown,
        RestError::BodyTooLarge | RestError::BadBody(_) => RolesError::BadReply,
        RestError::Refused { status, detail } => RolesError::Refused {
            status: *status,
            detail: detail.clone(),
        },
        RestError::Server { status: 503 } if model_set => RolesError::RestartRequired,
        RestError::Server { status } => RolesError::Server { status: *status },
        RestError::Forbidden { detail } => RolesError::Forbidden {
            detail: detail.clone(),
        },
        RestError::NotFound { route } => RolesError::NotFound {
            route: route.clone(),
        },
        RestError::Unauthorized | RestError::Ticket(_) => RolesError::SignIn,
    }
}

fn info_view(info: ModelInfo) -> RolesInfo {
    RolesInfo {
        model: info.model,
        provider: info.provider,
        auto_context_length: info.auto_context_length,
        config_context_length: info.config_context_length,
        capabilities: info.capabilities,
    }
}

fn triple(fields: RouteFields) -> RouteTriple {
    RouteTriple {
        provider: fields.provider,
        model: fields.model,
        base_url: fields.base_url,
    }
}

fn aux_view(models: AuxiliaryModels) -> AuxView {
    AuxView {
        slots: models
            .tasks
            .into_iter()
            .map(|task| AuxSlot {
                task: task.task,
                provider: task.provider,
                model: task.model,
                base_url: task.base_url,
                reasoning_effort: task.reasoning_effort,
                local_endpoint: task.local_endpoint,
                label: task.label,
                hint: task.hint,
                plugin: task.plugin,
                inherit_from: task.inherit_from,
                effective: task.effective.map(triple),
            })
            .collect(),
        main: RouteTriple {
            provider: models.main.provider,
            model: models.main.model,
            base_url: String::new(),
        },
    }
}

fn slot_view(slot: MoaSlot) -> SlotView {
    SlotView {
        provider: slot.provider,
        model: slot.model,
        reasoning_effort: slot.reasoning_effort,
        enabled: slot.enabled,
        extra: slot.extra,
    }
}

fn slot_wire(slot: &SlotView) -> MoaSlot {
    MoaSlot {
        provider: slot.provider.clone(),
        model: slot.model.clone(),
        reasoning_effort: slot.reasoning_effort.clone(),
        enabled: slot.enabled,
        extra: slot.extra.clone(),
    }
}

fn moa_view(config: MoaConfig) -> MoaConfigView {
    MoaConfigView {
        default_preset: config.default_preset,
        active_preset: config.active_preset,
        presets: config
            .presets
            .into_iter()
            .map(|named| MoaPresetView {
                name: named.name,
                enabled: named.preset.enabled,
                references: named
                    .preset
                    .reference_models
                    .into_iter()
                    .map(slot_view)
                    .collect(),
                aggregator: slot_view(named.preset.aggregator),
                reference_temperature: named.preset.reference_temperature,
                aggregator_temperature: named.preset.aggregator_temperature,
                reference_timeout: named.preset.reference_timeout,
                degraded_reference_policy: named.preset.degraded_reference_policy,
                fanout: named.preset.fanout,
                extra: named.preset.extra,
            })
            .collect(),
        privacy_filter: config.privacy_filter,
    }
}

fn moa_wire(view: &MoaConfigView) -> MoaConfig {
    MoaConfig {
        default_preset: view.default_preset.clone(),
        active_preset: view.active_preset.clone(),
        presets: view
            .presets
            .iter()
            .map(|preset| NamedPreset {
                name: preset.name.clone(),
                preset: MoaPreset {
                    enabled: preset.enabled,
                    reference_models: preset.references.iter().map(slot_wire).collect(),
                    aggregator: slot_wire(&preset.aggregator),
                    reference_temperature: preset.reference_temperature,
                    aggregator_temperature: preset.aggregator_temperature,
                    reference_timeout: preset.reference_timeout,
                    degraded_reference_policy: preset.degraded_reference_policy.clone(),
                    fanout: preset.fanout.clone(),
                    extra: preset.extra.clone(),
                },
            })
            .collect(),
        privacy_filter: view.privacy_filter.clone(),
    }
}

/// Only the keys the page shows or writes are kept, as written in the file.
fn raw_view(raw: &Value) -> RawRoles {
    let list = |key: &str| match raw.get(key) {
        Some(Value::Array(items)) => items.clone(),
        Some(item @ Value::Object(_)) => vec![item.clone()],
        _ => Vec::new(),
    };
    let delegation = raw
        .get("delegation")
        .and_then(Value::as_object)
        .map(|map| {
            DELEGATION_KEYS
                .iter()
                .filter_map(|key| {
                    map.get(*key)
                        .map(|value| ((*key).to_owned(), value.clone()))
                })
                .collect::<Map<String, Value>>()
        })
        .unwrap_or_default();
    let model_context_length = raw
        .get("model_context_length")
        .and_then(Value::as_u64)
        .or_else(|| {
            raw.get("model")
                .and_then(|model| model.get("context_length"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0);
    RawRoles {
        fallback_providers: match raw.get("fallback_providers") {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        },
        fallback_model: list("fallback_model"),
        delegation,
        model_context_length,
        agent_reasoning_effort: raw
            .get("agent")
            .and_then(|agent| agent.get("reasoning_effort"))
            .cloned()
            .unwrap_or(Value::Null),
        has_moa_presets: raw
            .get("moa")
            .and_then(|moa| moa.get("presets"))
            .and_then(Value::as_object)
            .is_some_and(|presets| !presets.is_empty()),
    }
}

/// Find the running gateway and build the service for one profile. The gateway is never
/// started or restarted from here. With no gateway the page reports it is not connected.
pub(crate) async fn open_service(
    home: PathBuf,
    profile: String,
    connection_profile: String,
    socket: Option<Client>,
) -> Option<Arc<ModelRolesService>> {
    use hermes_client::{DiscoverOptions, EnsurePolicy, NativeHttpMinter, discover_with};
    let endpoint = Arc::new(
        discover_with(
            &home,
            &DiscoverOptions {
                ensure: EnsurePolicy::Never,
                ..DiscoverOptions::default()
            },
        )
        .await
        .ok()?,
    );
    let origin = endpoint.api_origin().to_owned();
    let served_profiles = endpoint
        .profiles()
        .iter()
        .map(|served| served.name.clone())
        .collect();
    // An unserved profile gets no client; the service says so before it sends anything.
    let rest = endpoint.resolve_profile(&profile).ok().and_then(|id| {
        let minter = Arc::new(NativeHttpMinter::new(home.clone(), endpoint.clone(), id));
        RestClient::new(&origin, minter, RestOptions::default()).ok()
    });
    let saved_default = crate::config::load_models().ok().and_then(|saved| {
        let chosen = saved.defaults.get(&profile)?;
        Some((chosen.provider.clone()?, chosen.model.clone()?))
    });
    Some(Arc::new(
        ModelRolesService::new(ModelRolesOpen {
            profile,
            connection_profile,
            api_origin: Some(origin),
            rest,
            socket,
            served_profiles,
        })
        .with_saved_default(saved_default),
    ))
}
