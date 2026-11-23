//! Rename or delete one served profile over the gateway's profile routes.
//!
//! Hermes accepts `PATCH` and `DELETE /api/profiles/<name>` from a native client only with a
//! `native-profile-registry` ticket minted for that profile's own home, and never for the
//! profile the gateway was started from.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::rest::{Method, RequestOpts, RestClient, RestError, TicketMinter};
use crate::{Endpoint, ProfileId, Purpose, Scope, Ticket, TicketError, mint_ticket};

/// Hermes stops the profile's gateway and waits up to a minute for it to let go of the folder.
pub const REGISTRY_DEADLINE: Duration = Duration::from_secs(90);

/// Mints `native-profile-registry` tickets for one served profile.
pub struct ProfileRegistryMinter {
    home: PathBuf,
    endpoint: Arc<Endpoint>,
    profile: ProfileId,
}

impl ProfileRegistryMinter {
    pub fn new(home: PathBuf, endpoint: Arc<Endpoint>, profile: ProfileId) -> Self {
        Self {
            home,
            endpoint,
            profile,
        }
    }
}

impl TicketMinter for ProfileRegistryMinter {
    fn mint(
        &self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Ticket, TicketError>> + Send + '_>> {
        Box::pin(mint_ticket(
            &self.home,
            &self.endpoint,
            &self.profile,
            Purpose::NativeProfileRegistry,
            Scope::profile(),
        ))
    }
}

/// What a delete reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeleteReply {
    /// The profile is gone.
    Deleted,
    /// The folder is gone, but its session identity is not cleared yet.
    SettlementPending {
        /// The command Hermes says finishes it.
        retry_command: String,
    },
}

/// Why a rename or delete did not succeed.
#[derive(Debug)]
pub enum RegistryError {
    /// The name cannot be a served profile's name, so nothing was sent.
    InvalidName,
    /// The request failed before Hermes answered, or the answer was unreadable.
    Rest(RestError),
    /// Hermes answered with an error status and, usually, its own words.
    Failed { status: u16, detail: Option<String> },
}

/// Delete one profile. Hermes asks for no confirmation on this route.
pub async fn delete_profile(rest: &RestClient, name: &str) -> Result<DeleteReply, RegistryError> {
    let reply = send(rest, Method::Delete, name, None).await?;
    if reply["settlement_pending"] == true {
        let retry_command = reply["retry_command"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        return Ok(DeleteReply::SettlementPending { retry_command });
    }
    Ok(DeleteReply::Deleted)
}

/// Rename one profile; returns the name Hermes kept, which it normalises.
pub async fn rename_profile(
    rest: &RestClient,
    name: &str,
    new_name: &str,
) -> Result<String, RegistryError> {
    let reply = send(
        rest,
        Method::Patch,
        name,
        Some(json!({"new_name": new_name})),
    )
    .await?;
    reply["name"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| RegistryError::Rest(RestError::BadBody("no name".into())))
}

async fn send(
    rest: &RestClient,
    method: Method,
    name: &str,
    body: Option<Value>,
) -> Result<Value, RegistryError> {
    // Served names are lower-case letters, digits, `-` and `_`; anything else never reaches a path.
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(RegistryError::InvalidName);
    }
    let path = format!("/api/profiles/{name}");
    let body = body
        .map(|body| serde_json::to_vec(&body))
        .transpose()
        .map_err(|_| RegistryError::Rest(RestError::BadBody("invalid request JSON".into())))?;
    let opts = RequestOpts {
        ticket: true,
        deadline: Some(REGISTRY_DEADLINE),
    };
    let (status, bytes, ticket) = rest
        .exchange(method, &path, body, opts)
        .await
        .map_err(RegistryError::Rest)?;
    match status {
        200..=299 => {
            let reply: Value = serde_json::from_slice(&bytes)
                .map_err(|_| RegistryError::Rest(RestError::BadBody("invalid JSON".into())))?;
            if reply["ok"] == true {
                Ok(reply)
            } else {
                Err(RegistryError::Rest(RestError::BadBody("not ok".into())))
            }
        }
        401 => Err(RegistryError::Rest(RestError::Unauthorized)),
        _ => {
            let detail = serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| {
                v.get("detail").and_then(Value::as_str).map(|text| {
                    let text = ticket.as_ref().map_or_else(
                        || text.to_owned(),
                        |ticket| text.replace(ticket.expose(), "<redacted>"),
                    );
                    text.chars().take(300).collect::<String>()
                })
            });
            Err(RegistryError::Failed { status, detail })
        }
    }
}
