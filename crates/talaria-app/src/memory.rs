//! Executes memory effects for a single connection profile.

use std::sync::Arc;

use hermes_client::rest::RestClient;
use talaria_core::memory::{MemoryEffect, MemoryInput};

pub struct MemoryService {
    _rest: Arc<RestClient>,
    _profile: String,
}

impl MemoryService {
    pub fn new(rest: Arc<RestClient>, profile: String) -> Self {
        Self {
            _rest: rest,
            _profile: profile,
        }
    }

    pub async fn execute(&self, _effect: MemoryEffect) -> Vec<MemoryInput> {
        todo!("execute memory effects")
    }
}
