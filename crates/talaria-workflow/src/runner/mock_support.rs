//! The CLI's optional scripted transport uses the Hermes binding's fixture implementation.

use std::{path::PathBuf, sync::Arc};

use serde_json::Value;

use crate::{
    hermes::{WireFactory, mock_wire::ScriptedWire},
    staffing::{FixtureRosterSource, SystemClock},
};

use super::services::{self, RunnerServices};

pub fn services(profile: &str) -> Result<RunnerServices, String> {
    let wire = std::env::var_os("TALARIA_WORKFLOW_MOCK_WIRE")
        .map(PathBuf::from)
        .ok_or_else(|| {
            "TALARIA_WORKFLOW_MOCK_WIRE is required for the scripted transport".to_owned()
        })?;
    let roster = std::env::var_os("TALARIA_WORKFLOW_MOCK_ROSTER")
        .map(PathBuf::from)
        .ok_or_else(|| {
            "TALARIA_WORKFLOW_MOCK_ROSTER is required for the fixture roster".to_owned()
        })?;
    let wires: Arc<dyn WireFactory> =
        Arc::new(ScriptedWire::from_file(&wire).map_err(|e| e.to_string())?);
    let value: Value = serde_json::from_slice(&std::fs::read(roster).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let source = Arc::new(FixtureRosterSource::new());
    source.push(profile, Ok(value));
    Ok(RunnerServices::new(
        wires,
        source,
        Arc::new(SystemClock),
        services::port_config(),
    ))
}
