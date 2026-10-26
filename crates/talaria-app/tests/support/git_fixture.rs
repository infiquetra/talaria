//! Temporary git repositories for worktree tests.
//!
//! Every git child gets a home and a global config inside the temporary folder.
//! Nothing here reads or writes the operator's home or global git config.
//!
//! More than one test crate includes this file. A crate that only builds a repository
//! does not call every helper, and the temporary-directory field is kept so the folder
//! stays alive until the test ends.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use std::os::unix::fs::PermissionsExt;

/// A bare `origin.git`, a clone whose local `main` lags `origin/main`, and the temp folder
/// that owns both.
pub struct GitRepo {
    tmp: tempfile::TempDir,
    root: PathBuf,
    pub origin: PathBuf,
    pub repo: PathBuf,
    pub local_main: String,
    pub origin_main: String,
}

impl GitRepo {
    /// Build the fixture. Fails the test when git is missing or older than 2.28.
    pub fn new() -> Self {
        require_git();
        let tmp = tempfile::tempdir().expect("temp folder");
        // Git reports real paths, and macOS temporary folders sit behind a symbolic link.
        let root = std::fs::canonicalize(tmp.path()).expect("canonical temp folder");
        let home = tmp.path().join("home");
        let global = tmp.path().join("gitconfig");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::write(&global, "").expect("gitconfig");
        let origin = root.join("origin.git");
        let repo = root.join("repo");
        let other = root.join("other");
        run_ok(
            tmp.path(),
            tmp.path(),
            &["init", "--bare", "-b", "main", "origin.git"],
        );
        run_ok(tmp.path(), tmp.path(), &["clone", "origin.git", "repo"]);
        commit(tmp.path(), &repo, "one");
        run_ok(tmp.path(), &repo, &["push", "origin", "main"]);
        let local_main = rev_parse(tmp.path(), &repo, "main");
        run_ok(tmp.path(), tmp.path(), &["clone", "origin.git", "other"]);
        commit(tmp.path(), &other, "two");
        run_ok(tmp.path(), &other, &["push", "origin", "main"]);
        run_ok(tmp.path(), &repo, &["fetch", "origin"]);
        let origin_main = rev_parse(tmp.path(), &repo, "origin/main");
        assert_ne!(local_main, origin_main, "local main should lag origin/main");
        Self {
            tmp,
            root,
            origin,
            repo,
            local_main,
            origin_main,
        }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Run git in `cwd`. The caller inspects the status.
    pub fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        command(self.tmp.path(), cwd, args)
            .output()
            .unwrap_or_else(|_| panic!("git {} did not start", scrub_args(args)))
    }

    /// Run git and keep standard output. Panics with scrubbed error text when git fails.
    pub fn ok(&self, cwd: &Path, args: &[&str]) -> String {
        let output = self.run(cwd, args);
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
}

/// True when `git` runs: for tests that skip, with a message, when it cannot.
pub fn git_available() -> bool {
    Command::new("git")
        .arg("version")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Fail the test when `git` is missing or older than 2.28. Never skips.
pub fn require_git() {
    let output = Command::new("git")
        .arg("version")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output();
    let ok = match output {
        Ok(output) if output.status.success() => {
            version_ok(&String::from_utf8_lossy(&output.stdout))
        }
        _ => false,
    };
    if !ok {
        panic!("git 2.28 or later is required for these tests");
    }
}

/// A wrapper that sleeps when an argument is `fetch`, then runs git.
pub fn slow_fetch_git(dir: &Path, secs: u64) -> PathBuf {
    let body = format!(
        "#!/bin/sh\nfor arg in \"$@\"; do\n  if [ \"$arg\" = fetch ]; then\n    sleep {secs}\n    break\n  fi\ndone\nexec git \"$@\"\n"
    );
    write_script(dir, "slow-fetch-git", &body)
}

/// A wrapper that, on the first `worktree add`, creates the destination folder with a file in it,
/// then runs git. Git accepts an empty existing folder, so the file is what makes it refuse.
pub fn race_git(dir: &Path) -> PathBuf {
    let body = r#"#!/bin/sh
mark="$0.once"
if [ ! -e "$mark" ]; then
  prev=""
  add=0
  for arg in "$@"; do
    if [ "$prev" = worktree ] && [ "$arg" = add ]; then
      add=1
    fi
    prev=$arg
  done
  if [ "$add" = 1 ]; then
    last=""
    prev=""
    for arg in "$@"; do
      prev=$last
      last=$arg
    done
    if [ -n "$prev" ]; then
      mkdir -p -- "$prev"
      : > "$prev/.claimed"
    fi
    : > "$mark"
  fi
fi
exec git "$@"
"#;
    write_script(dir, "race-git", body)
}

fn version_ok(text: &str) -> bool {
    let Some(version) = text.split_whitespace().nth(2) else {
        return false;
    };
    let mut parts = version.split(['.', '-']);
    let major: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let minor: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    major > 2 || (major == 2 && minor >= 28)
}

fn commit(root: &Path, repo: &Path, name: &str) {
    std::fs::write(repo.join(name), name).expect("commit file");
    run_ok(root, repo, &["add", name]);
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
            name,
        ],
    );
}

fn rev_parse(root: &Path, repo: &Path, rev: &str) -> String {
    run_ok(root, repo, &["rev-parse", rev])
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

fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).expect("wrapper script");
    let mut permissions = std::fs::metadata(&path)
        .expect("wrapper metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("wrapper mode");
    path
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
