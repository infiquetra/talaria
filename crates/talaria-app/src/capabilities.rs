use crate::settings::MinterFactory;
use hermes_client::Client;
use talaria_core::capabilities::{CapEffect, CapInput, CapabilityChange};

pub struct CapabilitiesService;
impl CapabilitiesService {
    pub fn new(
        _api_origin: Option<String>,
        _served_profiles: Vec<String>,
        _make_minter: MinterFactory,
        _socket: Client,
        _hermes_home: String,
        _user_home: String,
    ) -> Self {
        todo!("capabilities stub")
    }
    pub async fn execute(&self, _effect: CapEffect) -> Vec<CapInput> {
        todo!("capabilities stub")
    }
    pub async fn apply(&self, _profile: &str, _change: CapabilityChange) -> ChangeOutcome {
        todo!("capabilities stub")
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeOutcome {
    Applied,
    Unchanged,
    Refused(String),
    Unknown,
    ReadBackMismatch,
}
