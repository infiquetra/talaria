//! Agent working folders that still hold uncommitted changes when a run ends.
use crate::{
    engine::EngineEvent,
    journal::{ROUTE_WARNING, Record, RecordLine},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path, process::Command};

/// One agent working folder with edits nobody committed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeftoverFolder {
    pub path: String,
    pub changed_files: usize,
}

/// The working folders the run's agents used that hold uncommitted changes, in path order.
/// Nothing is changed or deleted; a folder that is not a git work tree, or that is gone, is
/// left out.
pub(crate) fn leftover_folders(records: &[RecordLine]) -> Vec<LeftoverFolder> {
    let folders: BTreeSet<&str> = records
        .iter()
        .filter_map(|r| match &r.record {
            Record::CallIntent { launch, .. } => launch.cwd.as_deref(),
            _ => None,
        })
        .collect();
    folders
        .into_iter()
        .filter_map(|path| {
            changed_files(Path::new(path)).map(|changed_files| LeftoverFolder {
                path: path.into(),
                changed_files,
            })
        })
        .filter(|folder| folder.changed_files > 0)
        .collect()
}

fn changed_files(folder: &Path) -> Option<usize> {
    if !folder.is_dir() {
        return None;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(["status", "--porcelain"])
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
    })
}

/// The plain sentence shown for one leftover folder.
pub(crate) fn describe(folder: &LeftoverFolder) -> String {
    format!(
        "uncommitted changes left in an agent working folder: {} ({} changed file{})",
        folder.path,
        folder.changed_files,
        if folder.changed_files == 1 { "" } else { "s" }
    )
}

/// The journal's notes about work found on an agent's route that the run was not following,
/// without their fixed opening words.
pub(crate) fn untracked_work(records: &[RecordLine]) -> Vec<String> {
    records
        .iter()
        .filter_map(|r| match &r.record {
            Record::Engine {
                event: Some(EngineEvent::Log { message, .. }),
                ..
            } => message.strip_prefix(ROUTE_WARNING).map(str::to_owned),
            _ => None,
        })
        .collect()
}
