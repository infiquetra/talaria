//! Executes settings effects on profile-scoped, single-request HTTP connections.

use std::fmt;
use std::sync::{Arc, Mutex};

use hermes_client::config_api::{self, ConfigSchema};
use hermes_client::rest::{RestClient, RestError, RestOptions, TicketMinter};
use talaria_core::settings::{
    ConfigSchemaView, FailReason, FieldKind, FieldView, SettingsEffect, SettingsInput,
    SettingsSnapshot, WriteResult,
};

pub type MinterFactory = Arc<dyn Fn(&str) -> Arc<dyn TicketMinter> + Send + Sync>;

pub struct SettingsService {
    api_origin: Option<String>,
    served_profiles: Vec<String>,
    make_minter: MinterFactory,
    client: Mutex<Option<Arc<RestClient>>>,
}

impl fmt::Debug for SettingsService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsService")
            .field("connected", &self.api_origin.is_some())
            .field("served_count", &self.served_profiles.len())
            .finish_non_exhaustive()
    }
}

impl SettingsService {
    pub fn new(
        api_origin: Option<String>,
        served_profiles: Vec<String>,
        make_minter: MinterFactory,
    ) -> Self {
        Self {
            api_origin,
            served_profiles,
            make_minter,
            client: Mutex::new(None),
        }
    }

    pub async fn execute(&self, effect: SettingsEffect) -> Vec<SettingsInput> {
        let input = match effect {
            SettingsEffect::Load { profile } => match self.load(&profile).await {
                Ok(snapshot) => SettingsInput::Loaded(snapshot),
                Err(reason) => SettingsInput::LoadFailed(reason),
            },
            SettingsEffect::FetchFresh => match self.active() {
                Ok(rest) => match config_api::config_read(&rest, true).await {
                    Ok(value) => SettingsInput::Fresh(value),
                    Err(error) => SettingsInput::FreshFailed(read_failure(error)),
                },
                Err(reason) => SettingsInput::FreshFailed(reason),
            },
            SettingsEffect::Write { patch } => match self.active() {
                Ok(rest) => {
                    SettingsInput::Written(match config_api::config_write(&rest, &patch).await {
                        Ok(()) => WriteResult::Ok,
                        Err(error) => write_failure(error),
                    })
                }
                Err(reason) => SettingsInput::Written(WriteResult::NotSent(reason)),
            },
            SettingsEffect::ReadBack => match self.active() {
                Ok(rest) => match tokio::try_join!(
                    config_api::config_read(&rest, true),
                    config_api::config_read(&rest, false),
                ) {
                    Ok((effective, raw)) => SettingsInput::ReadBack { effective, raw },
                    Err(error) => SettingsInput::ReadBackFailed(read_failure(error)),
                },
                Err(reason) => SettingsInput::ReadBackFailed(reason),
            },
            _ => return Vec::new(),
        };
        vec![input]
    }

    fn active(&self) -> Result<Arc<RestClient>, FailReason> {
        self.client
            .lock()
            .ok()
            .and_then(|client| client.clone())
            .ok_or(FailReason::NotConnected)
    }

    async fn load(&self, profile: &str) -> Result<SettingsSnapshot, FailReason> {
        {
            let mut client = self.client.lock().map_err(|_| FailReason::NotConnected)?;
            *client = None;
        }
        let origin = self.api_origin.as_deref().ok_or(FailReason::NotConnected)?;
        if !self.served_profiles.iter().any(|name| name == profile) {
            return Err(FailReason::ProfileNotServed);
        }
        let rest = Arc::new(
            RestClient::new(origin, (self.make_minter)(profile), RestOptions::default())
                .map_err(read_failure)?,
        );
        let (schema, defaults, effective, raw) = tokio::try_join!(
            config_api::config_schema(&rest),
            config_api::config_defaults(&rest),
            config_api::config_read(&rest, true),
            config_api::config_read(&rest, false),
        )
        .map_err(read_failure)?;
        *self.client.lock().map_err(|_| FailReason::NotConnected)? = Some(rest);
        Ok(SettingsSnapshot {
            profile: profile.to_owned(),
            schema: schema_view(schema),
            effective,
            raw,
            defaults,
        })
    }
}

/// Resolve the current control endpoint without starting or changing the gateway.
pub(crate) async fn discover_service(home: std::path::PathBuf) -> Option<Arc<SettingsService>> {
    use hermes_client::rest::NativeHttpMinter;
    use hermes_client::{DiscoverOptions, EnsurePolicy, discover_with};
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
    let profiles = endpoint
        .profiles()
        .iter()
        .map(|profile| profile.name.clone())
        .collect();
    let factory = Arc::new(move |profile: &str| -> Arc<dyn TicketMinter> {
        let profile = endpoint
            .resolve_profile(profile)
            .expect("served profile was checked before minting");
        Arc::new(NativeHttpMinter::new(
            home.clone(),
            endpoint.clone(),
            profile,
        ))
    });
    Some(Arc::new(SettingsService::new(
        Some(origin),
        profiles,
        factory,
    )))
}

/// Preserve the transport schema's field order without a dependency from the core to HTTP.
fn schema_view(schema: ConfigSchema) -> ConfigSchemaView {
    use hermes_client::config_api::FieldKind as TransportKind;
    ConfigSchemaView {
        category_order: schema.category_order,
        fields: schema
            .fields
            .into_iter()
            .map(|field| FieldView {
                key: field.key,
                kind: match field.kind {
                    TransportKind::Boolean => FieldKind::Boolean,
                    TransportKind::Number => FieldKind::Number,
                    TransportKind::String => FieldKind::String,
                    TransportKind::Text => FieldKind::Text,
                    TransportKind::List => FieldKind::List,
                    TransportKind::Select => FieldKind::Select,
                    TransportKind::Object => FieldKind::Object,
                    TransportKind::Other(value) => FieldKind::Other(value),
                },
                description: field.description,
                category: field.category,
                options: field.options,
                searchable: field.searchable,
                clearable: field.clearable,
            })
            .collect(),
    }
}

fn read_failure(error: RestError) -> FailReason {
    match error {
        RestError::Unauthorized => FailReason::Unauthorized,
        RestError::Timeout { .. } => FailReason::Timeout { secs: 15 },
        RestError::Dropped { .. } => FailReason::Dropped,
        RestError::NotFound { route } => FailReason::NotFound { route },
        RestError::Forbidden { .. } => FailReason::Http { status: 403 },
        RestError::Refused { status, .. } | RestError::Server { status } => {
            FailReason::Http { status }
        }
        RestError::BodyTooLarge | RestError::BadBody(_) => FailReason::BadData,
        RestError::Ticket(error) => FailReason::Ticket(error.to_string()),
        RestError::NotLoopback | RestError::TlsNotSupported => FailReason::NotLoopback,
        RestError::Connect(_) => FailReason::NotConnected,
    }
}

fn write_failure(error: RestError) -> WriteResult {
    if error.outcome_unknown() {
        return WriteResult::Unknown;
    }
    match error {
        RestError::Unauthorized => WriteResult::Unauthorized,
        RestError::Forbidden { .. } => WriteResult::Forbidden,
        RestError::NotFound { route } => WriteResult::NotFound { route },
        RestError::Refused { status, detail } => WriteResult::Refused { status, detail },
        RestError::Server { status } => WriteResult::Server { status },
        RestError::BodyTooLarge | RestError::BadBody(_) => WriteResult::Unknown,
        other => WriteResult::NotSent(read_failure(other)),
    }
}

/// Secret command arguments never enter either persistent or in-memory prompt history.
pub fn append_history(
    history: &mut crate::comforts::prompt_history::PromptHistory,
    text: &str,
) -> Option<String> {
    if talaria_core::settings::is_secret_config_line(text) {
        None
    } else {
        history.append(text)
    }
}
