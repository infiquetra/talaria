//! A phase 2b scratch folder: the two git folders `a` and `b` with one empty commit each
//! and the empty home, under a private root, as the setup of the run builds them.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh phase 2b scratch folder for tests that need one without running the suite.
pub struct Scratch2b {
    _tmp: tempfile::TempDir,
    root: PathBuf,
}

impl Scratch2b {
    /// Builds the root with the two git folders and the empty home.
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().expect("temporary folder for the scratch");
        let root = tmp.path().join("talaria-smoke-test2b00");
        fs::create_dir(&root).expect("scratch folder");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
            .expect("the scratch folder is private");
        for folder in ["a", "b"] {
            let dir = root.join(folder);
            fs::create_dir(&dir).expect("a git folder");
            git(&dir, &["init", "-b", "main"]);
            git(
                &dir,
                &[
                    "-c",
                    "user.name=smoke",
                    "-c",
                    "user.email=smoke@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "init",
                ],
            );
        }
        fs::create_dir(root.join("empty-home")).expect("the empty home");
        Scratch2b { _tmp: tmp, root }
    }

    /// The scratch root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The one `talaria-smoke-*` folder under `tmp`, if exactly one exists.
    pub fn find_scratch(tmp: &Path) -> Option<PathBuf> {
        let read = fs::read_dir(tmp).ok()?;
        let mut found = Vec::new();
        for entry in read.flatten() {
            let path = entry.path();
            if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("talaria-smoke-"))
            {
                found.push(path);
            }
        }
        if found.len() == 1 { found.pop() } else { None }
    }

    /// The scratch left nothing behind: no `talaria-smoke-*` folder and no exported
    /// Markdown file anywhere under `tmp`.
    pub fn assert_removed(tmp: &Path) {
        let mut leftovers = Vec::new();
        if let Ok(read) = fs::read_dir(tmp) {
            for entry in read.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if name.starts_with("talaria-smoke-") {
                    leftovers.push(path);
                }
            }
        }
        assert!(
            leftovers.is_empty(),
            "the scratch was removed: {leftovers:?}"
        );
    }
}

/// Runs git with a clean environment, failing the test on any error.
fn git(dir: &Path, args: &[&str]) {
    let outcome = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env_clear()
        .output()
        .expect("run git");
    assert!(
        outcome.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&outcome.stderr)
    );
}
