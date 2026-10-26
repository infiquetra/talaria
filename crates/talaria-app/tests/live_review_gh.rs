//! Live `gh` round trip for the review. Issue #245.
//!
//! The operator runs this with `gh` signed in and `TALARIA_LIVE_GH_REPO` set to
//! a throwaway repository's clone URL. It uses only `std::process::Command`
//! with literal argument lists, so it runs before any Talaria review code
//! exists. With `TALARIA_GH_RECORD_DIR` set it writes the redacted replies the
//! recorded tests answer with. It always closes the pull request it opens and
//! never deletes the repository.
//!
//! Ignored unless someone asks for it. A skip prints
//! `LIVE_REVIEW_RESULT: SKIPPED (<reason>)`; with `TALARIA_LIVE_REQUIRE=1` a
//! skip fails.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn skip(reason: &str) {
    println!("LIVE_REVIEW_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
}

fn gh_available() -> bool {
    Command::new("gh")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success())
}

fn git_available() -> bool {
    Command::new("git")
        .arg("version")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Run `gh` in `cwd` with a clean git config but the operator's `gh` sign-in.
fn gh(cwd: &Path, global: &Path, args: &[&str]) -> Output {
    Command::new("gh")
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", global)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("gh runs")
}

/// Run git in `cwd` with an isolated global config.
fn git(root: &Path, cwd: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", root.join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("git runs")
}

fn ok(output: &Output, what: &str) -> String {
    if output.status.success() {
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    } else {
        panic!(
            "{what} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
}

/// Eight hex digits that differ every run.
fn live_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|span| span.as_nanos())
        .unwrap_or(0);
    let mixed = (std::process::id() as u128).wrapping_mul(0x9E3779B1)
        ^ nanos
        ^ ((nanos >> 32) * 0x85EBCA6B);
    format!("{:08x}", (mixed & 0xFFFF_FFFF) as u32)
}

struct Redact {
    pair: String,
    owner: String,
    login: String,
    tmp: String,
}

impl Redact {
    fn apply(&self, text: &str) -> String {
        text.replace(&self.pair, "fixture-owner/fixture-repo")
            .replace(&self.owner, "fixture-owner")
            .replace(&self.login, "fixture-user")
            .replace(&self.tmp, "<tmp>")
    }
}

fn write_reply(dir: &Path, name: &str, prefix: &[&str], output: &Output, redact: &Redact) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let body = serde_json::json!({
        "args_prefix": prefix,
        "exit": output.status.code().unwrap_or(-1),
        "stdout": redact.apply(&stdout),
        "stderr": redact.apply(&stderr),
    });
    std::fs::write(
        dir.join(name),
        serde_json::to_string_pretty(&body).expect("reply json"),
    )
    .expect("write reply");
}

/// A real draft pull request, recorded and closed again.
#[test]
#[ignore = "needs gh signed in and TALARIA_LIVE_GH_REPO"]
fn draft_pr_round_trip() {
    if !git_available() {
        skip("git not found on PATH");
        return;
    }
    if !gh_available() {
        skip("gh not found on PATH");
        return;
    }
    let Some(repo_url) = std::env::var_os("TALARIA_LIVE_GH_REPO") else {
        skip("TALARIA_LIVE_GH_REPO is not set");
        return;
    };
    let tmp = tempfile::tempdir().expect("temp folder");
    let root = tmp.path();
    std::fs::write(root.join("gitconfig"), "").expect("gitconfig");
    let global = root.join("gitconfig");

    let status = gh(root, &global, &["auth", "status"]);
    if !status.status.success() {
        skip("gh is not signed in");
        return;
    }
    let login =
        String::from_utf8_lossy(&gh(root, &global, &["api", "user", "-q", ".login"]).stdout)
            .trim()
            .to_owned();

    // Clone the throwaway repository, branch, commit, push.
    let clone = git(root, root, &["clone", &repo_url.to_string_lossy(), "repo"]);
    ok(&clone, "clone");
    let repo = root.join("repo");
    let branch = format!("talaria/live-{}", live_suffix());
    ok(
        &git(root, &repo, &["checkout", "-q", "-b", &branch]),
        "branch",
    );
    std::fs::write(repo.join("live.txt"), "live recording\n").expect("live file");
    ok(&git(root, &repo, &["add", "live.txt"]), "add");
    ok(
        &git(
            root,
            &repo,
            &[
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-q",
                "-m",
                "live recording",
            ],
        ),
        "commit",
    );
    ok(
        &git(root, &repo, &["push", "-q", "-u", "origin", &branch]),
        "push",
    );

    let pair = String::from_utf8_lossy(
        &gh(
            &repo,
            &global,
            &[
                "repo",
                "view",
                "--json",
                "nameWithOwner",
                "-q",
                ".nameWithOwner",
            ],
        )
        .stdout,
    )
    .trim()
    .to_owned();
    let owner = pair.split('/').next().unwrap_or_default().to_owned();
    assert!(
        pair.contains('/') && !owner.is_empty(),
        "owner/name parses: {pair}"
    );
    let name = pair.split('/').nth(1).unwrap_or_default().to_owned();
    let redact = Redact {
        pair: pair.clone(),
        owner,
        login,
        tmp: root.to_string_lossy().into_owned(),
    };

    // Create the draft pull request.
    let created = gh(
        &repo,
        &global,
        &[
            "pr", "create", "--draft", "--base", "main", "--head", &branch, "--fill",
        ],
    );
    println!(
        "create stdout: {}",
        String::from_utf8_lossy(&created.stdout)
    );
    println!(
        "create stderr: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    ok(&created, "pr create");
    let number = String::from_utf8_lossy(&created.stdout)
        .lines()
        .rev()
        .find(|line| line.starts_with("https://"))
        .and_then(|line| line.rsplit('/').next().map(str::to_owned))
        .expect("the create output prints a URL");

    // Query status, print the raw reply, and record when asked. The pull
    // request is closed afterwards whatever happens in here.
    let outcome = (|| -> Result<(), String> {
        let query = format!(
            "query {{ repository(owner: \"{pair_owner}\", name: \"{name}\") {{ b0: pullRequests(headRefName: \"{branch}\", first: 5, orderBy: {{field: CREATED_AT, direction: DESC}}) {{ nodes {{ number state isDraft isCrossRepository url headRefName reviewDecision commits(last: 1) {{ nodes {{ commit {{ statusCheckRollup {{ state }} }} }} }} }} }} }}",
            pair_owner = pair.split('/').next().unwrap_or_default(),
        );
        let queried = gh(
            &repo,
            &global,
            &["api", "graphql", "-f", &format!("query={query}")],
        );
        println!(
            "graphql stdout: {}",
            String::from_utf8_lossy(&queried.stdout)
        );
        if !queried.status.success() {
            return Err(format!(
                "graphql failed: {}",
                String::from_utf8_lossy(&queried.stderr).trim()
            ));
        }
        if let Some(dir) = std::env::var_os("TALARIA_GH_RECORD_DIR") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).expect("record folder");
            write_reply(
                &dir,
                "auth-status-signed-in.json",
                &["auth", "status"],
                &status,
                &redact,
            );
            let viewed = gh(
                &repo,
                &global,
                &[
                    "repo",
                    "view",
                    "--json",
                    "nameWithOwner",
                    "-q",
                    ".nameWithOwner",
                ],
            );
            write_reply(
                &dir,
                "repo-view-github.json",
                &["repo", "view"],
                &viewed,
                &redact,
            );
            write_reply(&dir, "pr-create.json", &["pr", "create"], &created, &redact);
            write_reply(
                &dir,
                "api-graphql-status.json",
                &["api", "graphql"],
                &queried,
                &redact,
            );

            // Signed out: an empty config folder and no token variables.
            let empty = root.join("empty-gh-config");
            std::fs::create_dir_all(&empty).expect("empty config");
            let mut signed_out = Command::new("gh");
            signed_out
                .args(["auth", "status"])
                .current_dir(&repo)
                .env("GH_CONFIG_DIR", &empty)
                .env_remove("GH_TOKEN")
                .env_remove("GITHUB_TOKEN")
                .env("GIT_CONFIG_GLOBAL", &global)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .stdin(std::process::Stdio::null());
            let signed_out = signed_out.output().expect("signed-out gh runs");
            write_reply(
                &dir,
                "auth-status-signed-out.json",
                &["auth", "status"],
                &signed_out,
                &redact,
            );

            // Not GitHub: a clone whose origin is a local bare repository.
            ok(
                &git(root, root, &["init", "--bare", "-q", "local.git"]),
                "bare",
            );
            ok(
                &git(root, root, &["clone", "-q", "local.git", "local"]),
                "local clone",
            );
            let local = root.join("local");
            let not_github = gh(
                &local,
                &global,
                &[
                    "repo",
                    "view",
                    "--json",
                    "nameWithOwner",
                    "-q",
                    ".nameWithOwner",
                ],
            );
            write_reply(
                &dir,
                "repo-view-not-github.json",
                &["repo", "view"],
                &not_github,
                &redact,
            );
        }
        Ok(())
    })();

    let closed = gh(&repo, &global, &["pr", "close", &number, "--delete-branch"]);
    println!("close stdout: {}", String::from_utf8_lossy(&closed.stdout));
    println!("close stderr: {}", String::from_utf8_lossy(&closed.stderr));
    if !closed.status.success() {
        panic!(
            "gh pr close {number} failed; close it by hand: {}",
            String::from_utf8_lossy(&closed.stderr).trim()
        );
    }
    outcome.expect("the round trip succeeds");
}
