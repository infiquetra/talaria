//! Runs one model-role effect and turns the reply into inputs.

use hermes_client::{Client, RestClient};
use talaria_core::model_roles::{RolesEffect, RolesInput};

/// What the page needs in order to read and write one profile.
pub struct ModelRolesService {
    profile: String,
    connection_profile: String,
    api_origin: Option<String>,
    rest: Option<RestClient>,
    socket: Option<Client>,
    served_profiles: Vec<String>,
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

impl ModelRolesService {
    pub fn new(open: ModelRolesOpen) -> Self {
        Self {
            profile: open.profile,
            connection_profile: open.connection_profile,
            api_origin: open.api_origin,
            rest: open.rest,
            socket: open.socket,
            served_profiles: open.served_profiles,
        }
    }

    pub async fn execute(&self, effect: RolesEffect) -> Vec<RolesInput> {
        let _ = (
            &self.profile,
            &self.connection_profile,
            &self.api_origin,
            &self.rest,
            &self.socket,
            &self.served_profiles,
            effect,
        );
        todo!("model roles stub")
    }
}
