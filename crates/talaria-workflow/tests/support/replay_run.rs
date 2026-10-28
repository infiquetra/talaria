//! Replay a recorded journal into a temporary run folder.
#![allow(dead_code)]

use std::path::Path;

use talaria_workflow::journal::RunDir;

use crate::lines;

/// Copy `journal` into a new run folder and write the fixture manifest.
pub fn replay_run(journal: &Path) -> (tempfile::TempDir, RunDir) {
    let root = tempfile::tempdir().expect("temp run");
    let folder = root.path().join(lines::RUN_ID);
    std::fs::create_dir_all(&folder).expect("run folder");
    std::fs::write(folder.join("manifest.json"), lines::manifest_json()).expect("manifest");
    std::fs::copy(journal, folder.join("journal.jsonl")).expect("journal");
    (root, RunDir::open(&folder).expect("open replay run"))
}

/// A run folder with a manifest and no journal yet.
pub fn empty_run() -> (tempfile::TempDir, RunDir) {
    let root = tempfile::tempdir().expect("temp run");
    let folder = root.path().join(lines::RUN_ID);
    std::fs::create_dir_all(&folder).expect("run folder");
    std::fs::write(folder.join("manifest.json"), lines::manifest_json()).expect("manifest");
    (root, RunDir::open(&folder).expect("open replay run"))
}
