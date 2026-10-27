use talaria_core::configure::{ApplyStep, Context, StepResult};

#[allow(async_fn_in_trait)]
pub trait ConfigureBackends: Send + Sync {
    async fn read_context(&self, profile: &str) -> Result<Context, String>;
    async fn apply(&self, step: &ApplyStep) -> StepResult;
}

pub struct SiblingBackends;

impl SiblingBackends {
    pub fn new(
        _settings: std::sync::Arc<crate::settings::SettingsService>,
        _client: hermes_client::Client,
        _consent: crate::connectors::ConnectorsConsent,
        _api_origin: Option<String>,
        _served_profiles: Vec<String>,
        _make_minter: crate::settings::MinterFactory,
    ) -> Self {
        Self
    }

    pub fn for_settings(_settings: std::sync::Arc<crate::settings::SettingsService>) -> Self {
        Self
    }
}

impl ConfigureBackends for SiblingBackends {
    async fn read_context(&self, _profile: &str) -> Result<Context, String> {
        todo!()
    }

    async fn apply(&self, _step: &ApplyStep) -> StepResult {
        todo!()
    }
}
