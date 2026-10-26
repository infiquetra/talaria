//! Guards around paths and names that come from outside the program: links in the tree,
//! a linked `.worktrees` folder, and a default branch name that origin supplies.

#[path = "support/git_fixture.rs"]
mod git_fixture;

use git_fixture::GitRepo;
use talaria_app::bg::CancelToken;
use talaria_app::worktree::include::apply_include;
use talaria_app::worktree::{GitRunner, prepare_worktree};

fn block<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

fn prepare(folder: &std::path::Path) -> Result<(), talaria_app::bg::LaunchError> {
    let git = GitRunner::from_env();
    block(prepare_worktree(
        &git,
        folder,
        "fix",
        "fixture",
        1_700_000_000,
        &CancelToken::new(),
    ))
    .map(|_| ())
}

#[test]
fn include_never_creates_folders_through_a_link_in_the_tree() {
    let repo = GitRepo::new();
    let outside = repo.path().join("outside");
    std::fs::create_dir(&outside).expect("outside");
    let tree = repo.path().join("tree");
    std::fs::create_dir(&tree).expect("tree");
    std::os::unix::fs::symlink(&outside, tree.join("cfg")).expect("link in tree");
    std::fs::create_dir_all(repo.repo.join("cfg/sub")).expect("source folders");
    std::fs::write(repo.repo.join("cfg/sub/y.txt"), "y").expect("source file");
    std::fs::write(repo.repo.join(".worktreeinclude"), "cfg/sub/y.txt\n").expect("include");
    let git = GitRunner::from_env();
    let warnings = block(apply_include(&git, &repo.repo, &tree));
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("cfg/sub/y.txt")),
        "no skip warning"
    );
    let made = outside.read_dir().expect("read outside").count();
    assert_eq!(made, 0, "a folder was created through the link");
}

#[test]
fn include_refuses_a_source_behind_a_linked_folder() {
    let repo = GitRepo::new();
    let elsewhere = repo.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).expect("elsewhere");
    std::fs::write(elsewhere.join("z.txt"), "z").expect("file");
    std::os::unix::fs::symlink(&elsewhere, repo.repo.join("vendor")).expect("link");
    std::fs::write(repo.repo.join(".worktreeinclude"), "vendor/z.txt\n").expect("include");
    let tree = repo.path().join("tree");
    std::fs::create_dir(&tree).expect("tree");
    let git = GitRunner::from_env();
    let warnings = block(apply_include(&git, &repo.repo, &tree));
    assert_eq!(warnings.len(), 1);
    assert!(!tree.join("vendor").exists());
}

#[test]
fn linked_trees_folder_is_refused() {
    let repo = GitRepo::new();
    let outside = repo.path().join("outside");
    std::fs::create_dir(&outside).expect("outside");
    std::os::unix::fs::symlink(&outside, repo.repo.join(".worktrees")).expect("link");
    let error = prepare(&repo.repo).expect_err("a linked .worktrees was used");
    assert!(
        error.what.contains(".worktrees is a symbolic link"),
        "{}",
        error.what
    );
    assert_eq!(outside.read_dir().expect("read outside").count(), 0);
}

#[test]
fn unusable_default_branch_name_is_refused() {
    let repo = GitRepo::new();
    repo.ok(&repo.repo, &["remote", "set-url", "origin", "missing.git"]);
    std::fs::write(
        repo.repo.join(".git/refs/remotes/origin/HEAD"),
        "ref: refs/remotes/origin/-x\n",
    )
    .expect("head");
    let error = prepare(&repo.repo).expect_err("a bad branch name was used");
    assert_eq!(error.what, "could not find the default branch of origin.");
}
