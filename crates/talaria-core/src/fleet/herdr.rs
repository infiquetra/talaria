//! Pure mapping of live terminal agents into fleet observations.

use super::ExternalSession;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HerdrPane {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub agent: String,
    pub name: String,
    pub status: String,
    pub title: String,
}

pub fn decode_list(_value: &Value) -> Result<Vec<HerdrPane>, String> {
    todo!("add terminal agent mapping")
}

pub fn decode_agent(_value: &Value) -> Result<HerdrPane, String> {
    todo!("add terminal agent mapping")
}

pub fn project(_pane: &HerdrPane, _visible: &str, _at: f64) -> HerdrProjection {
    todo!("add terminal agent mapping")
}

#[derive(Clone, Debug, PartialEq)]
pub struct HerdrProjection {
    pub session: ExternalSession,
    pub workspace: String,
    pub tab: String,
    pub pane: String,
    pub visible: String,
}

impl super::FleetRegistry {
    pub fn observe_herdr(&mut self, _projection: HerdrProjection) -> Vec<super::FleetEffect> {
        todo!("add terminal agent mapping")
    }
}
