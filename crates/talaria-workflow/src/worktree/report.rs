//! A declared report is untrusted file input from exactly one worker's worktree.
use crate::engine::{AgentRequest, CallId, Isolation};
use serde_json::Value;
use std::{
    fs,
    io::{self, ErrorKind, Read},
    path::Path,
};

const MAX_REPORT_BYTES: u64 = 1024 * 1024;

fn valid_path(path: &str) -> bool {
    path.len() <= 1024
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !matches!(part, "" | "." | "..") && !part.eq_ignore_ascii_case(".git"))
}

pub(crate) fn validate_report_option(request: &AgentRequest) -> Result<(), String> {
    let Some(path) = &request.report_file else {
        return Ok(());
    };
    if !valid_path(path) {
        return Err("reportFile must be a worktree-relative path without dot components".into());
    }
    if request.isolation != Some(Isolation::Worktree) || request.schema.is_none() {
        return Err("reportFile requires isolation: 'worktree' and a schema".into());
    }
    Ok(())
}

/// Read one declared file from a run's exact isolated worker folder and validate its JSON.
pub fn read_report_file(
    run_dir: &Path,
    folder: &Path,
    call: &CallId,
    path: &str,
    schema: &Value,
) -> Result<(Vec<u8>, Value), String> {
    let bytes = read_report_bytes(run_dir, folder, call, path).map_err(|e| e.to_string())?;
    let value = crate::hermes::validate::validate_json(&bytes, schema)
        .map_err(|errors| errors.join("; "))?;
    Ok((bytes, value))
}

pub(crate) fn read_report_bytes(
    run_dir: &Path,
    folder: &Path,
    call: &CallId,
    path: &str,
) -> io::Result<Vec<u8>> {
    if !valid_path(path) || !super::provenance::valid_digest(&call.digest) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "invalid report path or call",
        ));
    }
    let run_dir = run_dir.canonicalize()?;
    let mut target = run_dir.join("worktrees");
    let name = format!("{}-{}", call.digest, call.occurrence);
    if folder != target.join(&name) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "report folder is not this call's worktree",
        ));
    }

    // Check every component without following links, including the worktree directory itself.
    let (parents, file_name) = path.rsplit_once('/').unwrap_or(("", path));
    if !fs::symlink_metadata(&target)?.is_dir() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid worktree directory",
        ));
    }
    for part in std::iter::once(name.as_str()).chain(parents.split('/').filter(|p| !p.is_empty())) {
        target.push(part);
        if !fs::symlink_metadata(&target)?.is_dir() {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "invalid worktree report directory",
            ));
        }
    }
    target.push(file_name);
    let before = fs::symlink_metadata(&target)?;
    // Check before opening so a special file such as a named pipe cannot block the runner.
    if !before.is_file() || before.len() > MAX_REPORT_BYTES || target.canonicalize()? != target {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "report must be a regular file of at most 1 MiB inside the worktree",
        ));
    }
    let file = crate::untrusted_file::open_read_only(&target)?;
    let opened = file.metadata()?;
    if !opened.is_file() || opened.len() > MAX_REPORT_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid report file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.nlink() != 1 || opened.dev() != before.dev() || opened.ino() != before.ino() {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "linked or replaced report file",
            ));
        }
    }
    let mut bytes = Vec::new();
    file.take(MAX_REPORT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_REPORT_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "report exceeds 1 MiB",
        ));
    }
    Ok(bytes)
}
