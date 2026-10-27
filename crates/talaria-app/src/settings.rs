use hermes_client::rest::TicketMinter;
use std::sync::Arc;
use talaria_core::settings::{SettingsEffect, SettingsInput};
pub type MinterFactory = Arc<dyn Fn(&str) -> Arc<dyn TicketMinter> + Send + Sync>;
pub struct SettingsService;
impl SettingsService {
    pub fn new(
        _api_origin: Option<String>,
        _served_profiles: Vec<String>,
        _make_minter: MinterFactory,
    ) -> Self {
        todo!("settings stub")
    }
    pub async fn execute(&self, _effect: SettingsEffect) -> Vec<SettingsInput> {
        todo!("settings stub")
    }
}

pub fn append_history(
    _history: &mut crate::comforts::prompt_history::PromptHistory,
    _text: &str,
) -> Option<String> {
    todo!("settings stub")
}
