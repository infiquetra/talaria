//! Executes scheduled-job effects on the gateway socket and the dashboard.

use hermes_client::Client;
use hermes_client::rest::RestClient;
use talaria_core::cron::{CronEffect, CronInput};

pub struct CronService {
    profile: String,
    generation: u64,
    rest: Option<RestClient>,
    _client: Client,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CronTag {
    pub generation: u64,
    pub profile: String,
    pub run_name: Option<String>,
}

impl CronService {
    pub fn new(
        client: Client,
        rest: Option<RestClient>,
        profile: String,
        generation: u64,
    ) -> CronService {
        CronService {
            profile,
            generation,
            rest,
            _client: client,
        }
    }

    pub fn rest_status(&self) -> Result<(), String> {
        if self.rest.is_none() {
            Err("Hermes did not give a dashboard address (api_origin)".into())
        } else {
            Ok(())
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn tag_for(&self, effect: &CronEffect) -> CronTag {
        let _ = effect;
        todo!()
    }

    pub async fn execute(&self, effect: CronEffect) -> Vec<CronInput> {
        let _ = effect;
        todo!()
    }

    pub fn input_for_event(&self, type_name: &str) -> Option<CronInput> {
        let _ = type_name;
        todo!()
    }
}

pub fn hide_secrets(_text: &str) -> String {
    todo!()
}

/// Maps a dashboard failure onto the pure failure the overlay shows.
pub fn classify_rest(error: &hermes_client::rest::RestError) -> talaria_core::cron::RestFailure {
    let _ = error;
    todo!()
}
