//! The permissions service for #237: the typed calls behind the reducer.
//!
//! Reads and writes the profile's approval mode and a session's bypass through
//! the config read and write verbs. Every call carries an explicit word, never
//! asks Hermes to toggle, and never names a profile.

use std::sync::Arc;
use std::time::Duration;

use hermes_client::channel::Client;
use hermes_client::{CallError, CallOptions};
use hermes_contract::methods::canonical::{config_get, config_set};
use hermes_contract::schemas::{CanonicalConfigSetParams, ConfigGetParams, ConfigSetResult, Value};
use serde_json::Value as Json;
use talaria_core::permissions::{ApprovalMode, CallFailure, PermissionsEffect, PermissionsInput};

/// How long one permissions call waits for Hermes.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// The key of the profile's approval mode.
const MODE_KEY: &str = "approvals.mode";

/// The key of a session's approval bypass.
const BYPASS_KEY: &str = "yolo";

/// Runs the permissions reducer's read and write effects.
pub struct PermissionsService {
    client: Arc<Client>,
}

impl PermissionsService {
    /// A service that calls through the given connection.
    pub fn new(client: Arc<Client>) -> PermissionsService {
        PermissionsService { client }
    }

    /// Performs one effect and returns the input its reply feeds back, if any.
    pub async fn execute(&self, effect: PermissionsEffect) -> Option<PermissionsInput> {
        match effect {
            PermissionsEffect::ReadProfileMode { generation } => {
                let result = self.read_mode().await;
                Some(PermissionsInput::ProfileModeRead { generation, result })
            }
            PermissionsEffect::ReadSessionOff {
                session_id,
                generation,
            } => {
                let result = self.read_bypass(&session_id).await;
                Some(PermissionsInput::SessionOffRead {
                    session_id,
                    generation,
                    result,
                })
            }
            PermissionsEffect::SetSessionOff {
                session_id,
                generation,
                off,
            } => {
                let result = self.write_bypass(&session_id, off).await;
                Some(PermissionsInput::SessionOffSet {
                    session_id,
                    generation,
                    sent: off,
                    result,
                })
            }
            PermissionsEffect::SetProfileMode { generation, mode } => {
                let result = self.write_mode(mode).await;
                Some(PermissionsInput::ProfileModeSet {
                    generation,
                    sent: mode,
                    result,
                })
            }
            PermissionsEffect::Notice { .. } | PermissionsEffect::Redraw => None,
        }
    }

    /// Maps one call error to the failure the reducer handles.
    pub fn classify(&self, error: &CallError) -> CallFailure {
        match error {
            CallError::NotConnected | CallError::Encode | CallError::InvalidTimeout => {
                CallFailure::NotSent
            }
            CallError::Rpc(rpc) => CallFailure::Refused {
                reason: rpc.reason.clone().unwrap_or_default(),
                rpc_code: rpc.code,
                message: rpc.message.clone(),
            },
            CallError::BadResult { .. } => CallFailure::Unreadable,
            CallError::TimedOut { .. } | CallError::Unknown { .. } => CallFailure::Unknown,
        }
    }

    async fn read_mode(&self) -> Result<ApprovalMode, CallFailure> {
        let reply = self
            .client
            .call_with::<config_get::Marker>(get_params(MODE_KEY, None), options())
            .await
            .map_err(|error| self.classify(&error))?;
        reply
            .value
            .as_deref()
            .and_then(ApprovalMode::parse)
            .ok_or(CallFailure::Unreadable)
    }

    async fn read_bypass(&self, session_id: &str) -> Result<bool, CallFailure> {
        let reply = self
            .client
            .call_with::<config_get::Marker>(get_params(BYPASS_KEY, Some(session_id)), options())
            .await
            .map_err(|error| self.classify(&error))?;
        reply
            .value
            .as_deref()
            .and_then(parse_bypass)
            .ok_or(CallFailure::Unreadable)
    }

    async fn write_bypass(&self, session_id: &str, off: bool) -> Result<bool, CallFailure> {
        let word = if off { "1" } else { "0" };
        let reply = self.set(BYPASS_KEY, Some(session_id), word).await?;
        word_of(&reply)
            .and_then(parse_bypass)
            .ok_or(CallFailure::Unreadable)
    }

    async fn write_mode(&self, mode: ApprovalMode) -> Result<ApprovalMode, CallFailure> {
        let reply = self.set(MODE_KEY, None, mode.as_wire()).await?;
        word_of(&reply)
            .and_then(ApprovalMode::parse)
            .ok_or(CallFailure::Unreadable)
    }

    async fn set(
        &self,
        key: &str,
        session_id: Option<&str>,
        word: &str,
    ) -> Result<ConfigSetResult, CallFailure> {
        let params = CanonicalConfigSetParams {
            confirm_expensive_model: None,
            key: key.to_owned(),
            profile: None,
            scope: None,
            session_id: session_id.map(str::to_owned),
            value: Json::String(word.to_owned()),
        };
        self.client
            .call_with::<config_set::Marker>(params, options())
            .await
            .map_err(|error| self.classify(&error))
    }
}

fn options() -> CallOptions {
    CallOptions {
        timeout: Some(CALL_TIMEOUT),
    }
}

fn get_params(key: &str, session_id: Option<&str>) -> ConfigGetParams {
    ConfigGetParams {
        cwd: None,
        key: key.to_owned(),
        profile: None,
        session_id: session_id.map(str::to_owned),
    }
}

/// The bypass words Hermes documents; anything else is not a reading.
fn parse_bypass(word: &str) -> Option<bool> {
    match word {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// The result's value when Hermes sent a word; a boolean or nothing is not a word.
fn word_of(reply: &ConfigSetResult) -> Option<&str> {
    match &reply.value {
        Value::String(word) => Some(word),
        Value::Boolean(_) | Value::Null => None,
    }
}
