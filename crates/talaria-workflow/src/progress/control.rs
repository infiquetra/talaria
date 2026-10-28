//! A create-new, durable channel from viewers to the detached runner.

use crate::journal::RunDir;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Write},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlRequest {
    Pause,
    Resume,
    Stop,
    Restart { key: String },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ControlId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlOutcome {
    Published,
    AlreadyRequested,
    Removed,
    Refused { reason: String },
}
#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("already requested")]
    AlreadyRequested,
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("invalid agent key: {0}")]
    BadKey(String),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingRequest {
    pub v: u32,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub at_ms: i64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingControls {
    pub pause: Option<PendingRequest>,
    pub stop: Option<PendingRequest>,
    pub restarts: Vec<PendingRequest>,
    pub warnings: Vec<String>,
}
pub struct RunControl;
impl RunControl {
    pub fn request_pause(dir: &RunDir, at_ms: i64) -> Result<ControlOutcome, ControlError> {
        publish(dir, "pause", None, at_ms)
    }
    pub fn request_stop(dir: &RunDir, at_ms: i64) -> Result<ControlOutcome, ControlError> {
        publish(dir, "stop", None, at_ms)
    }
    pub fn request_restart(
        dir: &RunDir,
        key: &str,
        at_ms: i64,
    ) -> Result<ControlOutcome, ControlError> {
        if !valid_key(key) {
            return Err(ControlError::BadKey(key.into()));
        }
        publish(dir, "restart", Some(key), at_ms)
    }
    pub fn request_resume(dir: &RunDir) -> Result<ControlOutcome, ControlError> {
        let folder = dir.path().join("control");
        match fs::remove_file(folder.join("pause.json")) {
            Ok(()) => {
                File::open(folder)?.sync_all()?;
                Ok(ControlOutcome::Removed)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(ControlOutcome::Removed),
            Err(e) => Err(e.into()),
        }
    }
    pub fn pending(dir: &RunDir) -> PendingControls {
        let mut pending = PendingControls::default();
        let entries = match fs::read_dir(dir.path().join("control")) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return pending,
            Err(e) => {
                pending
                    .warnings
                    .push(format!("cannot read controls: {}", e.kind()));
                return pending;
            }
        };
        for entry in entries {
            let result = (|| -> Result<(), String> {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.ends_with(".json") {
                    return Ok(());
                }
                let request: PendingRequest =
                    serde_json::from_slice(&fs::read(entry.path()).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                if request.v != 1 {
                    return Err("unsupported control version".into());
                }
                match (name.as_str(), request.kind.as_str(), request.key.as_deref()) {
                    ("pause.json", "pause", None) => pending.pause = Some(request),
                    ("stop.json", "stop", None) => pending.stop = Some(request),
                    (_, "restart", Some(key))
                        if valid_key(key) && name == format!("restart-{key}.json") =>
                    {
                        pending.restarts.push(request)
                    }
                    _ => return Err("invalid control request".into()),
                }
                Ok(())
            })();
            if let Err(reason) = result {
                pending.warnings.push(reason);
            }
        }
        pending.restarts.sort_by(|a, b| a.key.cmp(&b.key));
        pending
    }
}
fn valid_key(key: &str) -> bool {
    key.len() == 32
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn publish(
    dir: &RunDir,
    kind: &str,
    key: Option<&str>,
    at_ms: i64,
) -> Result<ControlOutcome, ControlError> {
    let folder = dir.path().join("control");
    fs::create_dir_all(&folder)?;
    let name = key.map_or_else(
        || format!("{kind}.json"),
        |key| format!("restart-{key}.json"),
    );
    let request = PendingRequest {
        v: 1,
        kind: kind.into(),
        key: key.map(str::to_owned),
        at_ms,
    };
    let bytes = serde_json::to_vec(&request).map_err(io::Error::other)?;
    let mut temp = tempfile::NamedTempFile::new_in(&folder)?;
    temp.write_all(&bytes)?;
    temp.as_file().sync_all()?;
    match fs::hard_link(temp.path(), folder.join(name)) {
        Ok(()) => {
            File::open(folder)?.sync_all()?;
            Ok(ControlOutcome::Published)
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(ControlOutcome::AlreadyRequested),
        Err(e) => Err(e.into()),
    }
}
