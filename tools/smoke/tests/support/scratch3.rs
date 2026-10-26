//! A phase 3 scratch folder: a bare origin, two clones, the probe folder and an empty home.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh phase 3 scratch folder for tests that need one without running the suite.
pub struct Scratch3 {
    _tmp: tempfile::TempDir,
    root: PathBuf,
}

impl Scratch3 {
    /// Builds the root the suite creates before it opens a tab.
    ///
    /// `origin.git` is a bare repository. `a` is a clone whose `main` is the commit `init`
    /// and whose `origin/main` is the later commit `two`. `other` is the clone that pushed
    /// `two`. `probe/talaria-smoke-probe` and `empty-home` are empty folders.
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().expect("temporary folder for the scratch");
        let root = tmp.path().join("talaria-smoke-test300");
        fs::create_dir(&root).expect("scratch folder");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
            .expect("the scratch folder is private");
        let origin = root.join("origin.git");
        git(&root, &["init", "--bare", "-b", "main", "origin.git"]);
        let session = root.join("a");
        git(&root, &["init", "-b", "main", "a"]);
        commit(&session, "init");
        git(
            &session,
            &["remote", "add", "origin", &origin.to_string_lossy()],
        );
        git(&session, &["push", "-u", "origin", "main"]);
        let other = root.join("other");
        git(&root, &["clone", "origin.git", "other"]);
        commit(&other, "two");
        git(&other, &["push", "origin", "main"]);
        git(&session, &["fetch", "origin"]);
        fs::create_dir_all(root.join("probe").join("talaria-smoke-probe"))
            .expect("the probe folder");
        fs::create_dir(root.join("empty-home")).expect("the empty home");
        Scratch3 { _tmp: tmp, root }
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

    /// The scratch left nothing behind: no `talaria-smoke-*` folder under `tmp`.
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

fn commit(dir: &Path, message: &str) {
    git(
        dir,
        &[
            "-c",
            "user.name=smoke",
            "-c",
            "user.email=smoke@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            message,
        ],
    );
}
