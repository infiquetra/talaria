//! Real git in temporary folders for parser tests. Issue #245.
//!
//! Every git child gets a home and a global config inside the temporary folder.
//! Nothing here reads or writes the operator's home or global git config.
//! Tests skip with `git not found on PATH` when git is absent.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// True when `git` runs.
pub fn git_available() -> bool {
    Command::new("git")
        .arg("version")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success())
}

/// A temporary folder with a bare `origin.git` and a clone.
pub struct GitBytes {
    tmp: tempfile::TempDir,
    /// The bare origin.
    pub origin: PathBuf,
    /// The clone tests change.
    pub repo: PathBuf,
}

impl GitBytes {
    /// Build the fixture with one commit pushed to `origin/main`.
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().expect("temp folder");
        let home = tmp.path().join("home");
        let global = tmp.path().join("gitconfig");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::write(
            &global,
            "[user]\n\tname = fixture\n\temail = fixture@example.invalid\n",
        )
        .expect("gitconfig");
        let origin = tmp.path().join("origin.git");
        let repo = tmp.path().join("repo");
        run_ok(
            tmp.path(),
            tmp.path(),
            &["init", "--bare", "-b", "main", "origin.git"],
        );
        run_ok(tmp.path(), tmp.path(), &["clone", "origin.git", "repo"]);
        std::fs::write(repo.join("a.txt"), "one\n").expect("seed file");
        run_ok(tmp.path(), &repo, &["add", "a.txt"]);
        commit(tmp.path(), &repo, "seed");
        run_ok(tmp.path(), &repo, &["push", "origin", "main"]);
        Self { tmp, origin, repo }
    }

    /// The temporary folder.
    pub fn path(&self) -> &Path {
        self.tmp.path()
    }

    /// Run git in `cwd`. The caller inspects the status.
    pub fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        command(self.tmp.path(), cwd, args)
            .output()
            .unwrap_or_else(|_| panic!("git {} did not start", scrub_args(args)))
    }

    /// Run git and keep raw standard output. Panics with scrubbed text on failure.
    pub fn ok(&self, cwd: &Path, args: &[&str]) -> Vec<u8> {
        let output = self.run(cwd, args);
        if output.status.success() {
            output.stdout
        } else {
            panic!(
                "git {} failed: {}",
                scrub_args(args),
                scrub(&String::from_utf8_lossy(&output.stderr))
            );
        }
    }

    /// Commit `message` with the fixture identity.
    pub fn commit(&self, cwd: &Path, message: &str) {
        commit(self.tmp.path(), cwd, message);
    }
}

/// Commit with `-c user.name=fixture -c user.email=fixture@example.invalid`.
fn commit(root: &Path, repo: &Path, message: &str) {
    run_ok(
        root,
        repo,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            message,
        ],
    );
}

fn run_ok(root: &Path, cwd: &Path, args: &[&str]) -> String {
    let output = command(root, cwd, args)
        .output()
        .unwrap_or_else(|_| panic!("git {} did not start", scrub_args(args)));
    if output.status.success() {
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    } else {
        panic!(
            "git {} failed: {}",
            scrub_args(args),
            scrub(&String::from_utf8_lossy(&output.stderr))
        );
    }
}

fn command(root: &Path, cwd: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(cwd)
        .env("HOME", root.join("home"))
        .env("GIT_CONFIG_GLOBAL", root.join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .stdin(std::process::Stdio::null());
    command
}

fn scrub_args(args: &[&str]) -> String {
    args.iter()
        .map(|arg| {
            if arg.contains('/') || arg.contains('\\') {
                "<path>"
            } else {
                arg
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn scrub(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            if word.contains('/') || word.contains('\\') {
                "<path>"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
