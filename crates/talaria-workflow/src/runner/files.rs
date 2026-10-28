use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}
pub(crate) fn absolute(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.into())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
pub(crate) fn private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}
pub(crate) fn sync_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("file has no parent folder"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temp, value)?;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    sync_dir(parent)
}
pub(crate) fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<T> {
    serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LaunchFile {
    pub v: u32,
    pub script_path: PathBuf,
    pub script_dir: PathBuf,
    pub cwd: PathBuf,
    pub workflows_dir: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RunnerFile {
    pub pid: u32,
    pub started_at_ms: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct FailureView {
    pub name: String,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct OutcomeFile {
    pub v: u32,
    pub outcome: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<FailureView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub finished_at_ms: i64,
    pub summary: Value,
}
fn present_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}
pub(crate) fn remove_owned_runner(dir: &Path) -> io::Result<()> {
    match read_json::<RunnerFile>(&dir.join("runner.json")) {
        Ok(owner) if owner.pid == std::process::id() => remove(&dir.join("runner.json")),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
pub(crate) fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => path.parent().map_or(Ok(()), sync_dir),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
pub(crate) fn remove_other_runner(dir: &Path) -> io::Result<()> {
    match read_json::<RunnerFile>(&dir.join("runner.json")) {
        Ok(owner) if owner.pid != std::process::id() => remove(&dir.join("runner.json")),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
pub(crate) fn clear_transient_controls(dir: &Path) -> io::Result<()> {
    for name in ["pause.json", "stop.json"] {
        remove(&dir.join("control").join(name))?;
    }
    Ok(())
}
pub(crate) fn consume_restarts(dir: &Path, keys: &[String]) -> io::Result<()> {
    for key in keys {
        if key.len() != 32
            || !key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid restart key",
            ));
        }
        remove(&dir.join("control").join(format!("restart-{key}.json")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_live_child_retains_its_own_pid_marker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runner.json");
        write_json(
            &path,
            &RunnerFile {
                pid: std::process::id(),
                started_at_ms: 0,
            },
        )
        .unwrap();
        remove_other_runner(dir.path()).unwrap();
        assert!(path.exists());
        remove_owned_runner(dir.path()).unwrap();
        assert!(!path.exists());
    }
    #[test]
    fn a_null_completed_result_stays_present_after_reload() {
        let result = OutcomeFile {
            v: 1,
            outcome: "completed".into(),
            result: Some(Value::Null),
            error: None,
            reason: None,
            finished_at_ms: 0,
            summary: Value::Null,
        };
        let bytes = serde_json::to_vec(&result).unwrap();
        let loaded: OutcomeFile = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(loaded.result, Some(Value::Null));
    }
    #[test]
    fn transient_cleanup_preserves_restart_requests_until_acceptance() {
        let dir = tempfile::tempdir().unwrap();
        let control = dir.path().join("control");
        fs::create_dir(&control).unwrap();
        let key = "a".repeat(32);
        let restart = control.join(format!("restart-{key}.json"));
        for path in [
            control.join("pause.json"),
            control.join("stop.json"),
            restart.clone(),
        ] {
            fs::write(path, b"{}").unwrap();
        }
        clear_transient_controls(dir.path()).unwrap();
        assert!(restart.exists());
        assert!(!control.join("pause.json").exists());
        assert!(!control.join("stop.json").exists());
        consume_restarts(dir.path(), &[key]).unwrap();
        assert!(!restart.exists());
    }
}
