//! Supplementary lifecycle checks use real Git and never contact a gateway.
use std::{fs, path::Path, process::Command};
use talaria_workflow::worktree::GitWorktrees;

fn git(path: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, GitWorktrees) {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("source");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]);
    fs::write(repo.join("tracked"), "base\n").unwrap();
    fs::write(repo.join(".gitignore"), "ignored\n").unwrap();
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            "base",
        ],
    );
    let repo = repo.canonicalize().unwrap();
    let provider = GitWorktrees::new(temp.path().join("run"), repo.clone(), "run1".into());
    (temp, repo, provider)
}

#[test]
fn dirty_base_uses_only_committed_content_and_records_dirtiness() {
    let (temp, repo, provider) = fixture();
    fs::write(repo.join("tracked"), "uncommitted").unwrap();
    fs::write(repo.join("untracked"), "untracked").unwrap();
    fs::write(repo.join("ignored"), "ignored").unwrap();
    let digest = "a".repeat(64);
    let path = provider.for_call(&digest, 0).unwrap();
    assert_eq!(fs::read(path.join("tracked")).unwrap(), b"base\n");
    assert!(!path.join("untracked").exists() && !path.join("ignored").exists());
    let metadata: serde_json::Value = serde_json::from_slice(
        &fs::read(
            temp.path()
                .join("run/worktrees")
                .join(format!("{digest}-0.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(metadata["base_dirty"], true);
    assert_eq!(fs::read(repo.join("tracked")).unwrap(), b"uncommitted");
}

#[test]
fn malformed_identity_is_refused_before_creating_directories() {
    let (temp, _, provider) = fixture();
    for digest in [
        "".into(),
        "a".repeat(63),
        "A".repeat(64),
        "é".repeat(32),
        format!("../{}", "a".repeat(61)),
    ] {
        assert_eq!(
            provider.for_call(&digest, 0).unwrap_err(),
            "invalid call identity: digest must be 64 lower-case hexadecimal characters"
        );
    }
    assert!(!temp.path().join("run").exists());
}

#[test]
fn tampered_provenance_never_adopts_or_deletes_a_foreign_branch() {
    let (temp, repo, provider) = fixture();
    let digest = "b".repeat(64);
    let path = provider.for_call(&digest, 0).unwrap();
    let file = temp
        .path()
        .join("run/worktrees")
        .join(format!("{digest}-0.json"));
    let mut side: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    side["branch"] = "main".into();
    fs::write(&file, serde_json::to_vec(&side).unwrap()).unwrap();
    assert!(
        provider
            .for_call(&digest, 0)
            .unwrap_err()
            .starts_with("worktree path exists but is not this run's worktree:")
    );
    assert!(provider.list().is_empty());
    assert!(
        provider
            .remove(&path, true)
            .unwrap_err()
            .starts_with("not a workflow worktree:")
    );
    assert!(path.is_dir());
    assert_eq!(git(&repo, &["branch", "--show-current"]), "main");
}

#[test]
fn clean_and_forced_removal_of_missing_folders_preserve_their_branch_rules() {
    let (_temp, repo, provider) = fixture();
    for force in [false, true] {
        let digest = if force { "c" } else { "d" }.repeat(64);
        let path = provider.for_call(&digest, 0).unwrap();
        fs::remove_dir_all(&path).unwrap();
        provider.remove(&path, force).unwrap();
        assert!(provider.list().is_empty());
        let branch = format!("talaria/wf/run1/{digest}-0");
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.success(), !force);
    }
}

#[test]
fn stale_temporary_provenance_is_not_overwritten() {
    let (temp, _, provider) = fixture();
    let digest = "e".repeat(64);
    let directory = temp.path().join("run/worktrees");
    fs::create_dir_all(&directory).unwrap();
    let stale = directory.join(format!(".{digest}-0.json.0.tmp"));
    fs::write(&stale, "old writer's data").unwrap();
    provider.for_call(&digest, 0).unwrap();
    assert_eq!(fs::read_to_string(&stale).unwrap(), "old writer's data");
    assert_eq!(provider.list().len(), 1);
}

#[test]
fn failed_creation_removes_provisional_provenance_without_falling_back() {
    let (temp, repo, _) = fixture();
    let provider = GitWorktrees::new(temp.path().join("other-run"), repo, "invalid..run".into());
    let digest = "f".repeat(64);
    assert!(
        provider
            .for_call(&digest, 0)
            .unwrap_err()
            .starts_with("cannot create worktree")
    );
    assert!(
        !temp
            .path()
            .join("other-run/worktrees")
            .join(format!("{digest}-0.json"))
            .exists()
    );
    assert!(provider.list().is_empty());
}

#[test]
fn diff_collects_committed_staged_unstaged_and_binary_untracked_changes() {
    let (temp, repo, provider) = fixture();
    let path = provider.for_call(&"a".repeat(64), 1).unwrap();
    fs::write(path.join("committed"), "new commit\n").unwrap();
    git(&path, &["add", "committed"]);
    git(
        &path,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            "agent edit",
        ],
    );
    fs::write(path.join("tracked"), "staged\n").unwrap();
    git(&path, &["add", "tracked"]);
    fs::write(path.join("tracked"), "unstaged\n").unwrap();
    fs::write(path.join("binary new file"), b"\0\x01\xffnew").unwrap();
    fs::write(path.join("ignored"), "must not be in the patch").unwrap();
    let patch = provider.diff(&path).unwrap();
    let patch_file = temp.path().join("changes.patch");
    fs::write(&patch_file, &patch).unwrap();
    let comparison = temp.path().join("comparison");
    git(
        &repo,
        &["worktree", "add", "--detach", comparison.to_str().unwrap()],
    );
    git(
        &comparison,
        &["apply", "--binary", patch_file.to_str().unwrap()],
    );
    assert_eq!(fs::read(comparison.join("tracked")).unwrap(), b"unstaged\n");
    assert_eq!(
        fs::read(comparison.join("committed")).unwrap(),
        b"new commit\n"
    );
    assert_eq!(
        fs::read(comparison.join("binary new file")).unwrap(),
        b"\0\x01\xffnew"
    );
    assert!(!comparison.join("ignored").exists());
    assert!(!repo.join("committed").exists());
    assert!(
        provider
            .diff(&repo)
            .unwrap_err()
            .starts_with("not a workflow worktree:")
    );
}

#[test]
fn plain_subdirectory_is_not_adopted_even_when_its_parent_has_the_expected_branch() {
    let (_temp, repo, _) = fixture();
    let digest = "b".repeat(64);
    let branch = format!("talaria/wf/run1/{digest}-0");
    git(&repo, &["branch", "-m", &branch]);
    let run = repo.join("run");
    let folder = run.join("worktrees").join(format!("{digest}-0"));
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("keep"), "plain directory").unwrap();
    let provider = GitWorktrees::new(run, repo.clone(), "run1".into());
    let error = provider.for_call(&digest, 0).unwrap_err();
    assert_eq!(
        error,
        format!(
            "worktree path exists but is not this run's worktree: {}",
            folder.display()
        )
    );
    assert_eq!(
        fs::read_to_string(folder.join("keep")).unwrap(),
        "plain directory"
    );
    assert!(provider.list().is_empty());
}

#[test]
fn discovery_preserves_trailing_space_in_the_repository_path() {
    let (temp, original, _) = fixture();
    let source = temp.path().join("source ");
    fs::rename(&original, &source).unwrap();
    fs::create_dir(&original).unwrap();
    git(&original, &["init", "-b", "main"]);
    fs::write(original.join("tracked"), "decoy\n").unwrap();
    git(&original, &["add", "."]);
    git(
        &original,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            "decoy",
        ],
    );
    let provider = GitWorktrees::new(temp.path().join("space-run"), source, "run1".into());
    let path = provider.for_call(&"c".repeat(64), 0).unwrap();
    assert_eq!(fs::read(path.join("tracked")).unwrap(), b"base\n");
}
