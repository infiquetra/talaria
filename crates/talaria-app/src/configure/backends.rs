use hermes_client::{
    Client,
    rest::{RestClient, RestOptions},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use talaria_core::{
    capabilities::{CapData, CapEffect, CapInput, CapTab, PluginStatus},
    configure::*,
    cron::{CronEffect, CronInput},
    mcp::{McpEffect, McpInput},
    model_roles as roles,
    settings::{self, SettingsEffect, SettingsInput, WriteResult},
};

#[allow(async_fn_in_trait)]
pub trait ConfigureBackends: Send + Sync {
    async fn read_context(&self, profile: &str) -> Result<Context, String>;
    async fn apply(&self, step: &ApplyStep) -> StepResult;
}

pub struct SiblingBackends {
    settings: Arc<crate::settings::SettingsService>,
    client: Option<Client>,
    consent: crate::connectors::ConnectorsConsent,
    api_origin: Option<String>,
    served_profiles: Vec<String>,
    make_minter: Option<crate::settings::MinterFactory>,
    profile: Mutex<Option<String>>,
    detector: SecretDetector,
}

fn detect(text: &str) -> bool {
    !hermes_client::scan_for_secrets(text).is_empty()
}

enum McpExpected {
    Present,
    Enabled(bool),
    Absent,
}

impl SiblingBackends {
    pub fn new(
        settings: Arc<crate::settings::SettingsService>,
        client: Client,
        consent: crate::connectors::ConnectorsConsent,
        api_origin: Option<String>,
        served_profiles: Vec<String>,
        make_minter: crate::settings::MinterFactory,
    ) -> Self {
        let profile = (served_profiles.len() == 1).then(|| served_profiles[0].clone());
        Self {
            settings,
            client: Some(client),
            consent,
            api_origin,
            served_profiles,
            make_minter: Some(make_minter),
            profile: Mutex::new(profile),
            detector: detect,
        }
    }

    pub fn for_settings(settings: Arc<crate::settings::SettingsService>) -> Self {
        Self {
            settings,
            client: None,
            consent: Default::default(),
            api_origin: None,
            served_profiles: Vec::new(),
            make_minter: None,
            profile: Mutex::new(None),
            detector: detect,
        }
    }

    pub fn with_detector(mut self, detector: SecretDetector) -> Self {
        self.detector = detector;
        self
    }
    fn clean(&self, text: &str) -> String {
        if (self.detector)(text) {
            WITHHELD.into()
        } else {
            talaria_core::providers::text::sanitize(text)
        }
    }
    fn clean_value(&self, value: &mut Value) {
        self.clean_value_at(value, "");
    }
    fn clean_value_at(&self, value: &mut Value, path: &str) {
        if settings::is_secret_key(path) && !value.as_str().is_some_and(env_template) {
            *value = Value::String("<secret>".into());
            return;
        }
        match value {
            Value::String(text) => *text = self.clean(text),
            Value::Array(items) => {
                for item in items {
                    self.clean_value_at(item, path);
                }
            }
            Value::Object(items) => {
                for (key, mut value) in std::mem::take(items) {
                    let nested = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    self.clean_value_at(&mut value, &nested);
                    items.insert(self.clean(&key), value);
                }
            }
            _ => {}
        }
    }
    fn rest(&self, profile: &str) -> Result<RestClient, String> {
        if !self.served_profiles.iter().any(|name| name == profile) {
            return Err("profile is not served".into());
        }
        RestClient::new(
            self.api_origin
                .as_deref()
                .ok_or("dashboard address unavailable")?,
            self.make_minter
                .as_ref()
                .ok_or("ticket minter unavailable")?(profile),
            RestOptions::default(),
        )
        .map_err(|_| "dashboard unavailable".into())
    }
    fn capabilities(&self) -> Result<crate::capabilities::CapabilitiesService, String> {
        Ok(crate::capabilities::CapabilitiesService::new(
            self.api_origin.clone(),
            self.served_profiles.clone(),
            self.make_minter
                .clone()
                .ok_or("ticket minter unavailable")?,
            self.client.clone().ok_or("not connected")?,
            String::new(),
            String::new(),
        ))
    }
    fn model_roles(&self, profile: &str) -> Result<crate::model_roles::ModelRolesService, String> {
        Ok(crate::model_roles::ModelRolesService::new(
            crate::model_roles::ModelRolesOpen {
                profile: profile.into(),
                connection_profile: profile.into(),
                api_origin: self.api_origin.clone(),
                rest: Some(self.rest(profile)?),
                socket: self.client.clone(),
                served_profiles: self.served_profiles.clone(),
            },
        ))
    }
    fn mcp(&self, profile: &str) -> Result<crate::mcp::McpService, String> {
        Ok(crate::mcp::McpService::new(
            self.client.clone().ok_or("not connected")?,
            self.rest(profile)?,
            profile.into(),
        ))
    }
    fn cron(&self, profile: &str) -> Result<crate::cron::CronService, String> {
        Ok(crate::cron::CronService::new(
            self.client.clone().ok_or("not connected")?,
            self.rest(profile).ok(),
            profile.into(),
            0,
        ))
    }
    async fn cap_list(&self, profile: &str, tab: CapTab) -> Section<CapRow> {
        let Ok(service) = self.capabilities() else {
            return unavailable();
        };
        match service
            .execute(CapEffect::Fetch {
                profile: profile.into(),
                tab,
                generation: 0,
            })
            .await
            .into_iter()
            .next()
        {
            Some(CapInput::Loaded {
                result: Ok(data), ..
            }) => Section::Available(match data {
                CapData::Skills(rows) => rows
                    .into_iter()
                    .map(|r| CapRow {
                        kind: CapKind::Skill,
                        name: self.clean(&r.name),
                        enabled: r.enabled,
                        description: self.clean(&r.description),
                    })
                    .collect(),
                CapData::Toolsets(rows) => rows
                    .into_iter()
                    .map(|r| CapRow {
                        kind: CapKind::Toolset,
                        name: self.clean(&r.name),
                        enabled: r.enabled,
                        description: self.clean(&r.description),
                    })
                    .collect(),
                CapData::Plugins(rows) => rows
                    .into_iter()
                    .map(|r| CapRow {
                        kind: CapKind::Plugin,
                        name: self.clean(r.key.as_deref().unwrap_or(&r.name)),
                        enabled: r.status == PluginStatus::Enabled,
                        description: self.clean(&r.description),
                    })
                    .collect(),
                CapData::Hub(_) => return unavailable(),
            }),
            _ => unavailable(),
        }
    }
    async fn mcp_list(&self, profile: &str) -> Section<McpRow> {
        let Ok(service) = self.mcp(profile) else {
            return unavailable();
        };
        match service
            .execute(McpEffect::LoadList, None)
            .await
            .into_iter()
            .next()
        {
            Some(McpInput::ListLoaded(Ok(rows))) => Section::Available(
                rows.into_iter()
                    .map(|r| McpRow {
                        name: self.clean(&r.name),
                        kind: self.clean(&r.transport),
                        enabled: r.enabled,
                        plugin: r.plugin.is_some(),
                    })
                    .collect(),
            ),
            _ => unavailable(),
        }
    }
    async fn mcp_catalogue(&self, profile: &str) -> Section<CatalogueRow> {
        let Ok(service) = self.mcp(profile) else {
            return unavailable();
        };
        match service
            .execute(McpEffect::LoadCatalog, None)
            .await
            .into_iter()
            .next()
        {
            Some(McpInput::CatalogLoaded(Ok(rows))) => Section::Available(
                rows.into_iter()
                    .map(|r| CatalogueRow {
                        name: self.clean(&r.name),
                        description: self.clean(&r.description),
                        kind: self.clean(&r.transport),
                        installed: r.installed,
                        enabled: r.enabled,
                        requires: r.requires.iter().map(|s| self.clean(s)).collect(),
                    })
                    .collect(),
            ),
            _ => unavailable(),
        }
    }
    async fn cron_list(&self, profile: &str) -> Section<CronRow> {
        let Ok(service) = self.cron(profile) else {
            return unavailable();
        };
        match service
            .execute(CronEffect::List { seq: 0 })
            .await
            .into_iter()
            .next()
        {
            Some(CronInput::Listed {
                result: Ok(list), ..
            }) => Section::Available(
                list.jobs
                    .into_iter()
                    .map(|r| CronRow {
                        job_id: self.clean(&r.job_id),
                        name: self.clean(&r.name),
                        schedule: self.clean(&r.schedule),
                        state: self.clean(&r.state),
                    })
                    .collect(),
            ),
            _ => unavailable(),
        }
    }
    async fn providers(&self, profile: &str) -> Section<ProviderRow> {
        let (Some(client), Ok(rest)) = (self.client.clone(), self.rest(profile)) else {
            return unavailable();
        };
        let service = crate::providers::ProviderService::new(client, rest, profile.into());
        match service
            .execute(talaria_core::providers::ProvidersEffect::LoadOptions, None)
            .await
            .into_iter()
            .next()
        {
            Some(talaria_core::providers::ProvidersInput::OptionsLoaded(Ok(options))) => {
                Section::Available(
                    options
                        .providers
                        .into_iter()
                        .map(|r| ProviderRow {
                            slug: self.clean(&r.slug),
                            configured: r.authenticated.unwrap_or(false),
                        })
                        .collect(),
                )
            }
            _ => unavailable(),
        }
    }
    async fn main_model(&self, profile: &str) -> Section<MainModelRow> {
        let Ok(service) = self.model_roles(profile) else {
            return unavailable();
        };
        match service
            .execute(roles::RolesEffect::Load {
                read: roles::Read::Auxiliary,
            })
            .await
            .into_iter()
            .next()
        {
            Some(roles::RolesInput::Loaded(roles::Loaded::Auxiliary(aux))) => {
                Section::Available(vec![MainModelRow {
                    provider: self.clean(&aux.main.provider),
                    model: self.clean(&aux.main.model),
                }])
            }
            _ => unavailable(),
        }
    }
    async fn routes(&self, profile: &str) -> Section<RouteRow> {
        let Ok(service) = self.model_roles(profile) else {
            return unavailable();
        };
        match service
            .execute(roles::RolesEffect::LoadCatalog { refresh: false })
            .await
            .into_iter()
            .next()
        {
            Some(roles::RolesInput::CatalogLoaded(catalog)) => Section::Available(
                catalog
                    .providers
                    .into_iter()
                    .map(|r| RouteRow {
                        slug: self.clean(&r.slug),
                        authenticated: r.authenticated,
                        models: r.models.iter().map(|s| self.clean(s)).collect(),
                        api_url: self.clean(&r.api_url),
                    })
                    .collect(),
            ),
            _ => unavailable(),
        }
    }
    async fn profiles(&self, profile: &str) -> Section<String> {
        let Some(client) = self.client.clone() else {
            return unavailable();
        };
        match crate::profiles::ProfilesService::new(client, Default::default(), profile.into())
            .execute(talaria_core::profiles::ProfilesEffect::List { generation: 0 })
            .await
        {
            Some(talaria_core::profiles::ProfilesInput::Listed(_, Ok(rows))) => {
                Section::Available(rows.iter().map(|row| self.clean(&row.name)).collect())
            }
            _ => unavailable(),
        }
    }
    async fn connectors(&self, profile: &str) -> Section<(String, bool)> {
        if !self.consent.granted(profile) {
            return Section::Unavailable("connector consent has not been granted".into());
        }
        let (Some(client), Ok(rest)) = (self.client.clone(), self.rest(profile)) else {
            return unavailable();
        };
        let service = crate::connectors::ConnectorsService::new(
            client,
            rest,
            profile.into(),
            self.consent.clone(),
        );
        match service
            .execute(
                talaria_core::connectors::ConnectorsEffect::ListConnectors,
                None,
            )
            .await
            .into_iter()
            .next()
        {
            Some(talaria_core::connectors::ConnectorsInput::ListReply(Ok(reply)))
                if reply.available =>
            {
                Section::Available(
                    reply
                        .connectors
                        .into_iter()
                        .map(|row| (self.clean(&row.connector), row.enabled))
                        .collect(),
                )
            }
            _ => unavailable(),
        }
    }

    async fn apply_settings(
        &self,
        rows: &[(usize, String)],
        changes: &BTreeMap<String, Value>,
        baseline: &Value,
    ) -> StepResult {
        let fresh = match self
            .settings
            .execute(SettingsEffect::FetchFresh)
            .await
            .into_iter()
            .next()
        {
            Some(SettingsInput::Fresh(fresh)) => fresh,
            _ => {
                return all_rows(
                    rows,
                    RowOutcome::Unknown("could not read current settings".into()),
                );
            }
        };
        let (conflicts, _) = settings::find_conflicts(changes, baseline, &fresh);
        let mut sent = changes.clone();
        let mut outcomes = BTreeMap::new();
        for conflict in conflicts {
            sent.remove(&conflict.key);
            outcomes.insert(
                conflict.key,
                RowOutcome::Conflict(format!(
                    "changed since the proposal: now {}",
                    self.clean(&conflict.now.unwrap_or(Value::Null).to_string())
                )),
            );
        }
        if !sent.is_empty() {
            let written = self
                .settings
                .execute(SettingsEffect::Write {
                    patch: settings::build_patch(&sent),
                })
                .await
                .into_iter()
                .next();
            let failure = match written {
                Some(SettingsInput::Written(WriteResult::Ok | WriteResult::Unknown)) => None,
                Some(SettingsInput::Written(WriteResult::Refused { detail, .. })) => {
                    Some(RowOutcome::Failed(
                        self.clean(
                            detail
                                .as_deref()
                                .unwrap_or("Hermes refused the settings change"),
                        ),
                    ))
                }
                Some(SettingsInput::Written(_)) => Some(RowOutcome::Failed(
                    "settings change was refused or not sent".into(),
                )),
                _ => Some(RowOutcome::Unknown("no write result".into())),
            };
            if let Some(failure) = failure {
                for key in sent.keys() {
                    outcomes.insert(key.clone(), failure.clone());
                }
            } else {
                let after = match self
                    .settings
                    .execute(SettingsEffect::ReadBack)
                    .await
                    .into_iter()
                    .next()
                {
                    Some(SettingsInput::ReadBack { effective, .. }) => Some(effective),
                    _ => None,
                };
                for (key, value) in &sent {
                    let outcome = match &after {
                        None => RowOutcome::Unknown("settings read-back failed".into()),
                        Some(after) => match settings::classify_readback(
                            value,
                            settings::lookup(baseline, key),
                            settings::lookup(after, key),
                        ) {
                            settings::KeyOutcome::Applied => RowOutcome::Applied,
                            settings::KeyOutcome::Adjusted(value) => {
                                RowOutcome::Adjusted(self.clean(&value.to_string()))
                            }
                            settings::KeyOutcome::NotApplied => {
                                RowOutcome::NotApplied("Hermes did not apply this setting".into())
                            }
                            settings::KeyOutcome::ChangedElsewhere(value) => {
                                RowOutcome::Conflict(self.clean(&value.to_string()))
                            }
                        },
                    };
                    outcomes.insert(key.clone(), outcome);
                }
            }
        }
        StepResult::Rows(
            rows.iter()
                .map(|(row, key)| {
                    (
                        *row,
                        outcomes
                            .remove(key)
                            .unwrap_or(RowOutcome::Unknown("no result".into())),
                    )
                })
                .collect(),
        )
    }

    async fn apply_main_model(&self, profile: &str, step: &ApplyStep) -> StepResult {
        let ApplyStep::MainModel {
            row,
            provider,
            model,
            base_url,
            baseline,
            confirmed,
        } = step
        else {
            return StepResult::Rows(Vec::new());
        };
        let Ok(service) = self.model_roles(profile) else {
            return one(*row, RowOutcome::Failed("model service unavailable".into()));
        };
        let fresh = service
            .execute(roles::RolesEffect::FetchFresh {
                section: roles::Section::Main,
            })
            .await;
        let Some(roles::RolesInput::Fresh { reads, .. }) = fresh.first() else {
            return one(
                *row,
                RowOutcome::Unknown("could not read current model".into()),
            );
        };
        let Some(current) = main_from_reads(reads) else {
            return one(*row, RowOutcome::Unknown("current model missing".into()));
        };
        if current.provider != baseline.provider || current.model != baseline.model {
            return one(
                *row,
                RowOutcome::Conflict("main model changed since the proposal".into()),
            );
        }
        let result = service
            .execute(roles::RolesEffect::Assign {
                step: roles::ApplyStep::MainModel,
                assignment: roles::Assignment {
                    scope: roles::AssignScopeView::Main,
                    task: String::new(),
                    provider: provider.clone(),
                    model: model.clone(),
                    reasoning_effort: roles::EffortChange::Omit,
                    base_url: base_url.clone(),
                    confirmed: *confirmed,
                },
            })
            .await;
        match result.into_iter().next() {
            Some(roles::RolesInput::Applied {
                reply:
                    roles::ApplyReply::Assigned(roles::AssignOutcome::ConfirmRequired { message }),
                ..
            }) => {
                return StepResult::NeedsConfirm {
                    row: *row,
                    message: self.clean(&message),
                };
            }
            Some(roles::RolesInput::ApplyFailed {
                error: roles::RolesError::OutcomeUnknown,
                ..
            })
            | Some(roles::RolesInput::Applied { .. }) => {}
            Some(roles::RolesInput::ApplyFailed { error, .. }) => {
                return one(*row, RowOutcome::Failed(self.clean(&roles_error(error))));
            }
            _ => {
                return one(
                    *row,
                    RowOutcome::Unknown("no model assignment result".into()),
                );
            }
        }
        let result = service
            .execute(roles::RolesEffect::ReadBack {
                section: roles::Section::Main,
            })
            .await;
        let Some(roles::RolesInput::ReadBack { reads, .. }) = result.first() else {
            return one(*row, RowOutcome::Unknown("model read-back failed".into()));
        };
        match main_from_reads(reads) {
            Some(actual) if actual.provider == *provider && actual.model == *model => {
                one(*row, RowOutcome::Applied)
            }
            Some(actual) => one(
                *row,
                RowOutcome::NotApplied(self.clean(&format!(
                    "Hermes now uses {}/{}",
                    actual.provider, actual.model
                ))),
            ),
            None => one(
                *row,
                RowOutcome::Unknown("model read-back omitted the route".into()),
            ),
        }
    }

    async fn apply_mcp(
        &self,
        profile: &str,
        row: usize,
        name: &str,
        effect: McpEffect,
        expected: McpExpected,
    ) -> StepResult {
        let Ok(service) = self.mcp(profile) else {
            return one(row, RowOutcome::Failed("MCP service unavailable".into()));
        };
        let failure = match service.execute(effect, None).await.into_iter().next() {
            Some(McpInput::Added { result, .. }) => result.map(|_| ()).err(),
            Some(McpInput::Toggled { result, .. }) => result.map(|_| ()).err(),
            Some(McpInput::Removed { result, .. }) => result.err(),
            _ => return one(row, RowOutcome::Unknown("no MCP write result".into())),
        };
        if let Some(error) = failure {
            return one(
                row,
                match error {
                    talaria_core::mcp::Failure::Unknown => {
                        RowOutcome::Unknown("MCP outcome unknown".into())
                    }
                    talaria_core::mcp::Failure::Rpc { code: 4064, .. } => {
                        RowOutcome::NoChange("already gone".into())
                    }
                    error => RowOutcome::Failed(self.clean(&format!("{error:?}"))),
                },
            );
        }
        match service
            .execute(McpEffect::LoadList, None)
            .await
            .into_iter()
            .next()
        {
            Some(McpInput::ListLoaded(Ok(rows))) => {
                let found = rows.iter().find(|r| r.name == name);
                let agrees = match expected {
                    McpExpected::Present => found.is_some(),
                    McpExpected::Enabled(enabled) => found.is_some_and(|r| r.enabled == enabled),
                    McpExpected::Absent => found.is_none(),
                };
                one(
                    row,
                    if agrees {
                        RowOutcome::Applied
                    } else {
                        RowOutcome::NotApplied(
                            "Hermes reported success but the server list does not agree".into(),
                        )
                    },
                )
            }
            _ => one(row, RowOutcome::Unknown("MCP list read-back failed".into())),
        }
    }

    async fn apply_cron(
        &self,
        profile: &str,
        row: usize,
        effect: CronEffect,
        action: Option<&CronControl>,
    ) -> StepResult {
        let Ok(service) = self.cron(profile) else {
            return one(row, RowOutcome::Failed("cron service unavailable".into()));
        };
        let (job_id, failure) = match service.execute(effect).await.into_iter().next() {
            Some(CronInput::Added(result)) => match result {
                Ok(reply) => (reply.job_id, None),
                Err(e) => (String::new(), Some(e)),
            },
            Some(CronInput::PauseSet { job_id, result }) => (job_id, result.err()),
            Some(CronInput::Removed { job_id, result }) => (job_id, result.err()),
            _ => return one(row, RowOutcome::Unknown("no cron write result".into())),
        };
        if let Some(error) = failure {
            return one(
                row,
                match error {
                    talaria_core::cron::CallFailure::Unknown => {
                        RowOutcome::Unknown("cron outcome unknown".into())
                    }
                    talaria_core::cron::CallFailure::Tool {
                        job_saved: true, ..
                    } => RowOutcome::Failed(
                        "The job was saved but not scheduled; do not create it again.".into(),
                    ),
                    error => RowOutcome::Failed(self.clean(&format!("{error:?}"))),
                },
            );
        }
        match service
            .execute(CronEffect::List { seq: 0 })
            .await
            .into_iter()
            .next()
        {
            Some(CronInput::Listed {
                result: Ok(reply), ..
            }) => {
                let found = reply.jobs.iter().find(|job| job.job_id == job_id);
                let agrees = match action {
                    Some(CronControl::Remove) => found.is_none(),
                    Some(CronControl::Pause) => {
                        found.is_some_and(|job| job.state == "paused" || !job.enabled)
                    }
                    Some(CronControl::Resume) => {
                        found.is_some_and(|job| job.state != "paused" && job.enabled)
                    }
                    None => found.is_some(),
                };
                one(
                    row,
                    if agrees {
                        RowOutcome::Applied
                    } else {
                        RowOutcome::NotApplied("cron list read-back does not agree".into())
                    },
                )
            }
            _ => one(
                row,
                RowOutcome::Unknown("cron list read-back failed".into()),
            ),
        }
    }
}

impl ConfigureBackends for SiblingBackends {
    async fn read_context(&self, profile: &str) -> Result<Context, String> {
        {
            let mut connected = self
                .profile
                .lock()
                .map_err(|_| "configuration profile unavailable")?;
            if connected
                .as_deref()
                .is_some_and(|current| current != profile)
            {
                return Err("configuration is restricted to the connected profile".into());
            }
            *connected = Some(profile.into());
        }
        let (
            settings,
            mcp_servers,
            mcp_catalogue,
            skills,
            toolsets,
            plugins,
            cron_jobs,
            providers,
            main_model,
            routes,
            profiles,
            connectors,
        ) = tokio::join!(
            self.settings.execute(SettingsEffect::Load {
                profile: profile.into()
            }),
            self.mcp_list(profile),
            self.mcp_catalogue(profile),
            self.cap_list(profile, CapTab::Skills),
            self.cap_list(profile, CapTab::Toolsets),
            self.cap_list(profile, CapTab::Plugins),
            self.cron_list(profile),
            self.providers(profile),
            self.main_model(profile),
            self.routes(profile),
            self.profiles(profile),
            self.connectors(profile),
        );
        let Some(SettingsInput::Loaded(mut snapshot)) = settings.into_iter().next() else {
            return Err("required settings context is unavailable".into());
        };
        self.clean_value(&mut snapshot.effective);
        self.clean_value(&mut snapshot.raw);
        self.clean_value(&mut snapshot.defaults);
        for field in &mut snapshot.schema.fields {
            field.key = self.clean(&field.key);
            field.description = self.clean(&field.description);
            field.category = self.clean(&field.category);
            if let Some(options) = &mut field.options {
                for option in options {
                    self.clean_value(option);
                }
            }
            if let settings::FieldKind::Other(kind) = &mut field.kind {
                *kind = self.clean(kind);
            }
        }
        for category in &mut snapshot.schema.category_order {
            *category = self.clean(category);
        }
        Ok(Context {
            profile: self.clean(profile),
            hermes_version: None,
            schema: snapshot.schema,
            effective: snapshot.effective,
            raw: snapshot.raw,
            defaults: snapshot.defaults,
            inventory: Inventory {
                mcp_servers,
                mcp_catalogue,
                skills,
                toolsets,
                plugins,
                cron_jobs,
                providers,
                main_model,
                routes,
                profiles,
                connectors,
            },
        })
    }

    async fn apply(&self, step: &ApplyStep) -> StepResult {
        if let ApplyStep::Settings {
            rows,
            changes,
            baseline,
        } = step
        {
            return self.apply_settings(rows, changes, baseline).await;
        }
        let profile = self.profile.lock().ok().and_then(|p| p.clone());
        let Some(profile) = profile else {
            return one(
                step_row(step),
                RowOutcome::Failed("connected profile has not been established".into()),
            );
        };
        match step {
            ApplyStep::MainModel { .. } => self.apply_main_model(&profile, step).await,
            ApplyStep::McpAdd {
                row,
                name,
                preset,
                config,
            } => {
                self.apply_mcp(
                    &profile,
                    *row,
                    name,
                    McpEffect::AddServer {
                        name: name.clone(),
                        preset: preset.clone(),
                        config: config.clone(),
                        token: false,
                    },
                    McpExpected::Present,
                )
                .await
            }
            ApplyStep::McpEnable { row, name, enabled } => {
                self.apply_mcp(
                    &profile,
                    *row,
                    name,
                    McpEffect::SetEnabled {
                        name: name.clone(),
                        enabled: *enabled,
                    },
                    McpExpected::Enabled(*enabled),
                )
                .await
            }
            ApplyStep::McpRemove { row, name } => {
                self.apply_mcp(
                    &profile,
                    *row,
                    name,
                    McpEffect::Remove { name: name.clone() },
                    McpExpected::Absent,
                )
                .await
            }
            ApplyStep::Capability { row, change } => {
                let outcome = match self.capabilities() {
                    Ok(service) => match service.apply(&profile, change.clone()).await {
                        crate::capabilities::ChangeOutcome::Applied => RowOutcome::Applied,
                        crate::capabilities::ChangeOutcome::Unchanged => {
                            RowOutcome::NoChange("already set".into())
                        }
                        crate::capabilities::ChangeOutcome::Refused(message) => {
                            RowOutcome::Failed(self.clean(&message))
                        }
                        crate::capabilities::ChangeOutcome::Unknown => {
                            RowOutcome::Unknown("capability outcome unknown".into())
                        }
                        crate::capabilities::ChangeOutcome::ReadBackMismatch => {
                            RowOutcome::NotApplied("capability read-back does not agree".into())
                        }
                    },
                    Err(message) => RowOutcome::Failed(message),
                };
                one(*row, outcome)
            }
            ApplyStep::CronAdd {
                row,
                name,
                schedule,
                prompt,
                repeat,
                deliver,
            } => {
                if deliver != "local" {
                    return one(
                        *row,
                        RowOutcome::Failed("configure only supports local cron delivery".into()),
                    );
                }
                self.apply_cron(
                    &profile,
                    *row,
                    CronEffect::Add(talaria_core::cron::AddRequest {
                        name: Some(name.clone()),
                        schedule: schedule.clone(),
                        prompt: prompt.clone(),
                        repeat: *repeat,
                        deliver: "local".into(),
                    }),
                    None,
                )
                .await
            }
            ApplyStep::CronControl {
                row,
                job_id,
                action,
            } => {
                self.apply_cron(
                    &profile,
                    *row,
                    match action {
                        CronControl::Pause => CronEffect::SetPaused {
                            job_id: job_id.clone(),
                            paused: true,
                        },
                        CronControl::Resume => CronEffect::SetPaused {
                            job_id: job_id.clone(),
                            paused: false,
                        },
                        CronControl::Remove => CronEffect::Remove {
                            job_id: job_id.clone(),
                        },
                    },
                    Some(action),
                )
                .await
            }
            ApplyStep::Settings { .. } => unreachable!("settings returned above"),
        }
    }
}

fn unavailable<T>() -> Section<T> {
    Section::Unavailable("inventory unavailable".into())
}

fn env_template(text: &str) -> bool {
    text.strip_prefix("${")
        .and_then(|inner| inner.strip_suffix('}'))
        .is_some_and(|name| {
            let name = name.strip_prefix("env:").unwrap_or(name);
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}
fn one(row: usize, outcome: RowOutcome) -> StepResult {
    StepResult::Rows(vec![(row, outcome)])
}
fn all_rows(rows: &[(usize, String)], outcome: RowOutcome) -> StepResult {
    StepResult::Rows(
        rows.iter()
            .map(|(row, _)| (*row, outcome.clone()))
            .collect(),
    )
}
fn step_row(step: &ApplyStep) -> usize {
    match step {
        ApplyStep::MainModel { row, .. }
        | ApplyStep::McpAdd { row, .. }
        | ApplyStep::McpEnable { row, .. }
        | ApplyStep::McpRemove { row, .. }
        | ApplyStep::Capability { row, .. }
        | ApplyStep::CronAdd { row, .. }
        | ApplyStep::CronControl { row, .. } => *row,
        ApplyStep::Settings { rows, .. } => rows.first().map_or(0, |(row, _)| *row),
    }
}
fn main_from_reads(reads: &[roles::Loaded]) -> Option<&roles::RouteTriple> {
    reads.iter().find_map(|read| match read {
        roles::Loaded::Auxiliary(aux) => Some(&aux.main),
        _ => None,
    })
}
fn roles_error(error: roles::RolesError) -> String {
    match error {
        roles::RolesError::NotSent { reason } => format!("not sent: {reason}"),
        roles::RolesError::Refused { detail, .. } => {
            detail.unwrap_or_else(|| "model assignment refused".into())
        }
        roles::RolesError::RestartRequired => {
            "The gateway needs a restart before changing the main model.".into()
        }
        error => format!("{error:?}"),
    }
}
