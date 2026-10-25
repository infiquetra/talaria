//! The directory service: listing, picker renames and exports (#231).

use std::path::PathBuf;
use std::time::Duration;

use hermes_client::CallOptions;
use serde_json::{Value, json};
use talaria_core::directory::export::{ExportFormat, safe_file_name};
use talaria_core::directory::fences::{seed_untracked, with_profile};
use talaria_core::directory::rows::{DirectoryRow, RowKey, RowOrigin};
use talaria_core::directory::state::PickerInput;
use talaria_core::mutation::{
    MutationLedger, MutationOutcome, MutationRequest, classify_error, classify_result,
};

use super::app::AppMessage;
use super::side_connection::{SideCallError, SideConnection, SideOpenError, SideScope};
use super::switch::write_export_file;

/// How long one list call waits.
const LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// How long one rename waits.
const RENAME_TIMEOUT: Duration = Duration::from_secs(15);
/// How long one export waits.
const EXPORT_TIMEOUT: Duration = Duration::from_secs(60);

/// The notice a mutation gets when the connection dropped before the answer.
/// The change may have been applied, so the same rename again sends the same
/// request and Hermes replays the receipt instead of applying it twice.
const UNKNOWN_OUTCOME_TEXT: &str = "The connection dropped before Hermes answered. Do the same \
     again to retry; Hermes will not apply it twice.";

/// Where an export goes: a folder with an automatic name, or an explicit path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportTarget {
    Auto { folder: PathBuf },
    Explicit { path: PathBuf },
}

/// How a rename ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameResult {
    Renamed {
        title: String,
        revision: Option<i64>,
    },
    Failed {
        text: String,
    },
}

/// How an export ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportResult {
    Written { path: PathBuf, bytes: u64 },
    Failed { text: String },
}

/// Lists sessions and renames and exports rows over the side connection.
pub struct DirectoryService {
    connection: Option<SideConnection>,
    generation: u64,
    only: Option<Vec<String>>,
    home: Option<PathBuf>,
    foreground_profile: Option<String>,
}

/// The outcome of a rename call, whether the gateway answered or refused. A
/// refusal arrives as an error and carries the fences the next attempt has to
/// be prepared against. A call whose reply never arrived is unknown: the
/// change may or may not have been applied.
fn rename_outcome(result: Result<Value, SideCallError>) -> Result<MutationOutcome, String> {
    match result {
        Ok(value) => Ok(classify_result(&value)),
        Err(SideCallError::Refused { code, reason, data }) => Ok(classify_error(
            code,
            reason.as_deref().unwrap_or_default(),
            Some(&data),
        )),
        Err(SideCallError::Dropped | SideCallError::Timeout) => Ok(MutationOutcome::Unknown),
        Err(error) => Err(error.phrase().to_owned()),
    }
}

impl DirectoryService {
    /// A service with no connection.
    pub fn new() -> Self {
        Self {
            connection: None,
            generation: 0,
            only: None,
            home: None,
            foreground_profile: None,
        }
    }

    /// Open the side connection for the foreground profile. The home folder
    /// and the profile are kept, so a later call can open a new connection
    /// when the gateway closed this one.
    pub async fn open(
        &mut self,
        home: &std::path::Path,
        foreground_profile: &str,
    ) -> Result<SideScope, SideOpenError> {
        let connection = SideConnection::open(home, foreground_profile).await?;
        let scope = connection.scope().clone();
        self.connection = Some(connection);
        self.home = Some(home.to_path_buf());
        self.foreground_profile = Some(foreground_profile.to_owned());
        Ok(scope)
    }

    /// The side connection, opened again when the gateway closed the last one.
    /// The home folder and the foreground profile are kept from the first
    /// open, so the new connection is opened the same way.
    async fn reopen(&mut self) -> Result<(), SideOpenError> {
        let (Some(home), Some(profile)) = (self.home.clone(), self.foreground_profile.clone())
        else {
            return Err(SideOpenError::Connect(
                "the side connection was never opened".to_owned(),
            ));
        };
        let connection = SideConnection::open(&home, &profile).await?;
        self.connection = Some(connection);
        Ok(())
    }

    /// List both scopes of every reachable profile. Returns the generation;
    /// each reply arrives as a message. One task per profile and scope.
    pub fn start_list(&mut self, tx: tokio::sync::mpsc::Sender<AppMessage>) -> u64 {
        self.generation += 1;
        let generation = self.generation;
        let Some(connection) = self.connection.as_ref() else {
            return generation;
        };
        // A host-scoped connection names the profile on every call; a
        // profile-only one already knows its profile, so it sends no `profile`.
        let (profiles, name_profile) = match connection.scope() {
            SideScope::Host { profiles } => (profiles.clone(), true),
            SideScope::ProfileOnly { profile, .. } => (vec![profile.clone()], false),
        };
        let profiles = match &self.only {
            Some(only) => profiles
                .into_iter()
                .filter(|profile| only.contains(profile))
                .collect(),
            None => profiles,
        };
        for profile in profiles {
            for origin in [RowOrigin::Live, RowOrigin::Stored] {
                let client = connection.client().clone();
                let tx = tx.clone();
                let profile = profile.clone();
                tokio::spawn(async move {
                    let scope = match origin {
                        RowOrigin::Live => "live",
                        RowOrigin::Stored => "stored",
                    };
                    let mut params = json!({"scope": scope, "limit": 100});
                    if name_profile {
                        params["profile"] = json!(profile);
                    }
                    let input = match client
                        .call_raw(
                            "session.list",
                            params,
                            CallOptions {
                                timeout: Some(LIST_TIMEOUT),
                            },
                        )
                        .await
                    {
                        Ok(value) => {
                            let mut rows = Vec::new();
                            let mut skipped = 0;
                            for row in value["sessions"].as_array().cloned().unwrap_or_default() {
                                match DirectoryRow::from_list_row(&profile, origin, &row) {
                                    Some(row) => rows.push(row),
                                    None => skipped += 1,
                                }
                            }
                            PickerInput::Listed {
                                generation,
                                profile,
                                origin,
                                rows,
                                skipped,
                            }
                        }
                        Err(error) => PickerInput::ListFailed {
                            generation,
                            profile,
                            origin,
                            reason: super::side_connection::call_error(error).phrase(),
                        },
                    };
                    let _ = tx.send(AppMessage::Picker(input)).await;
                });
            }
        }
        generation
    }

    /// The profile a call names: only a host-scoped connection names one.
    fn profile_of(&self, profile: &str) -> Option<String> {
        match self
            .connection
            .as_ref()
            .map(|connection| connection.scope())
        {
            Some(SideScope::Host { .. }) => Some(profile.to_owned()),
            _ => None,
        }
    }

    /// Rename a row over the side connection: build the request with the ledger, send it,
    /// settle it, and after a fence refusal try once more at the fences Hermes named.
    pub async fn rename(
        &mut self,
        ledger: &mut MutationLedger,
        key: &RowKey,
        title: &str,
    ) -> RenameResult {
        // A row the ledger has not seen is prepared against revision zero.
        seed_untracked(ledger, &key.session_id);
        let mut payload = serde_json::Map::new();
        payload.insert("title".to_owned(), Value::String(title.to_owned()));
        let Ok(request) = ledger.build(&key.session_id, "rename", payload, false) else {
            return RenameResult::Failed {
                text: "Hermes could not rename the session: the session is unknown.".to_owned(),
            };
        };
        let outcome = match self.send_rename(key, &request).await {
            Ok(outcome) => outcome,
            Err(text) => return RenameResult::Failed { text },
        };
        ledger.settle(&request, &outcome);
        if let MutationOutcome::FenceRefused { .. } = &outcome
            && let Some(retry) = ledger.retry_after_fence(&request, &outcome)
        {
            let outcome = match self.send_rename(key, &retry).await {
                Ok(outcome) => outcome,
                Err(text) => return RenameResult::Failed { text },
            };
            ledger.settle(&retry, &outcome);
            return Self::rename_result(&outcome, title);
        }
        Self::rename_result(&outcome, title)
    }

    /// Send one prepared rename over the side connection. A connection the gateway closed
    /// is opened again first. `Err` is the failure text when nothing could be sent.
    pub async fn send_rename(
        &mut self,
        key: &RowKey,
        request: &MutationRequest,
    ) -> Result<MutationOutcome, String> {
        if self
            .connection
            .as_ref()
            .is_some_and(|connection| connection.is_closed())
            && let Err(error) = self.reopen().await
        {
            return Err(format!(
                "Hermes could not rename the session: {}",
                error.phrase()
            ));
        }
        let Some(connection) = self.connection.as_ref() else {
            return Err("the side connection is closed".to_owned());
        };
        let profile = self.profile_of(&key.profile);
        let params = with_profile(request, profile.as_deref());
        rename_outcome(
            connection
                .call("session.mutate", params, RENAME_TIMEOUT)
                .await,
        )
        .map_err(|phrase| format!("Hermes could not rename the session: {phrase}"))
    }

    /// What a settled rename tells the operator.
    pub fn rename_result(outcome: &MutationOutcome, title: &str) -> RenameResult {
        match outcome {
            MutationOutcome::Committed(result) => RenameResult::Renamed {
                title: result["title"].as_str().unwrap_or(title).to_owned(),
                revision: result["revision"].as_i64(),
            },
            // The reply never came: the change may or may not have been applied. The ledger
            // keeps the request, so the same rename again sends it as it is and Hermes
            // replays the receipt instead of applying the change twice.
            MutationOutcome::Unknown => RenameResult::Failed {
                text: UNKNOWN_OUTCOME_TEXT.to_owned(),
            },
            MutationOutcome::FenceRefused { .. } => RenameResult::Failed {
                text: "Hermes refused the rename: the session changed while the rename was being prepared. Try again."
                    .to_owned(),
            },
            MutationOutcome::Refused { rpc_code, .. } if *rpc_code == -32603 => RenameResult::Failed {
                text: "Hermes could not rename the session: another session already has this title."
                    .to_owned(),
            },
            _ => RenameResult::Failed {
                text: "Hermes could not rename the session.".to_owned(),
            },
        }
    }

    /// Export a row over the side connection and write the file.
    pub async fn export(
        &mut self,
        key: &RowKey,
        format: ExportFormat,
        redact: bool,
        target: ExportTarget,
    ) -> ExportResult {
        let Some(connection) = self.connection.as_ref() else {
            return ExportResult::Failed {
                text: "the side connection is closed".to_owned(),
            };
        };
        let mut params = json!({
            "session_id": key.session_id,
            "format": format.wire(),
            "redact": redact,
        });
        if let Some(profile) = self.profile_of(&key.profile) {
            params["profile"] = json!(profile);
        }
        match connection
            .call("session.export", params, EXPORT_TIMEOUT)
            .await
        {
            Ok(value) => {
                let content = value["content"].as_str().unwrap_or_default().to_owned();
                let name = value["filename"].as_str().unwrap_or_default();
                let name = safe_file_name(name, format, &key.session_id);
                match write_export_file(&target, &name, &content) {
                    Ok((path, bytes)) => ExportResult::Written { path, bytes },
                    Err(text) => ExportResult::Failed { text },
                }
            }
            Err(error) => ExportResult::Failed {
                text: format!("Hermes could not export the session: {}", error.phrase()),
            },
        }
    }

    /// Close the side connection, if one is open.
    pub async fn close(&mut self) {
        if let Some(connection) = self.connection.take() {
            connection.close().await;
        }
    }

    /// List only these profiles. The recorder and live tests use it.
    #[doc(hidden)]
    pub fn restrict_profiles(&mut self, only: Vec<String>) {
        self.only = Some(only);
    }
}

impl Default for DirectoryService {
    fn default() -> Self {
        Self::new()
    }
}
