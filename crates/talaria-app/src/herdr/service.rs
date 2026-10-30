//! Polling and explicit actions for live terminal agents.

use super::HerdrConfig;
use crate::fleet::{FleetHandle, FleetSnapshot};
use std::sync::Arc;
use talaria_core::fleet::RowKey;
use talaria_core::fleet_view::FleetViewEffect;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HerdrError(pub String);

pub struct HerdrService {
    _config: HerdrConfig,
    fleet: FleetHandle,
}

impl HerdrService {
    pub fn start(config: HerdrConfig, fleet: FleetHandle) -> Self {
        Self {
            _config: config,
            fleet,
        }
    }
    pub async fn poll_once(&mut self) -> Result<(), HerdrError> {
        todo!("add terminal agent service")
    }
    pub async fn peek(&mut self, _key: &RowKey) -> Result<String, HerdrError> {
        todo!("add terminal agent service")
    }
    pub async fn prompt(&mut self, _key: &RowKey, _text: &str) -> Result<(), HerdrError> {
        todo!("add terminal agent service")
    }
    pub async fn answer(&mut self, _key: &RowKey, _text: &str) -> Result<(), HerdrError> {
        todo!("add terminal agent service")
    }
    pub async fn interrupt(&mut self, _key: &RowKey) -> Result<(), HerdrError> {
        todo!("add terminal agent service")
    }
    pub async fn focus(&mut self, _key: &RowKey) -> Result<(), HerdrError> {
        todo!("add terminal agent service")
    }
    pub async fn route_effect(&mut self, _effect: &FleetViewEffect) -> Result<(), HerdrError> {
        todo!("add terminal agent action routing")
    }
    pub fn snapshot(&self) -> Arc<FleetSnapshot> {
        self.fleet.snapshot()
    }
    pub async fn close(&mut self) {
        todo!("add terminal agent service")
    }
}
