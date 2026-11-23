//! Records which source the binary was built from, for the run manifest.
//!
//! `TALARIA_BUILD_COMMIT` is the short commit hash and `TALARIA_BUILD_TREE` says whether the
//! build tree had uncommitted changes to tracked files (`clean` or `modified`). A build without
//! git, such as one from a source archive, records `unknown` for both.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
        ])
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    // The workspace root holds the crates folder.
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or(&manifest)
        .to_path_buf();
    // A source archive unpacked inside some other repository must not take that one's commit.
    let own = git(&root, &["rev-parse", "--show-toplevel"])
        .and_then(|top| PathBuf::from(top).canonicalize().ok())
        .zip(root.canonicalize().ok())
        .is_some_and(|(top, root)| top == root);
    let commit = own
        .then(|| git(&root, &["rev-parse", "--short=12", "HEAD"]))
        .flatten()
        .filter(|c| !c.is_empty());
    let tree = commit.as_ref().and_then(|_| {
        git(
            &root,
            &[
                "status",
                "--porcelain",
                "--untracked-files=no",
                "--ignore-submodules=all",
            ],
        )
        .map(|out| if out.is_empty() { "clean" } else { "modified" })
    });
    println!(
        "cargo:rustc-env=TALARIA_BUILD_COMMIT={}",
        commit.as_deref().unwrap_or("unknown")
    );
    println!(
        "cargo:rustc-env=TALARIA_BUILD_TREE={}",
        tree.unwrap_or("unknown")
    );
    for path in watched(&root, commit.is_some()) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

/// The paths whose change makes the build script run again: the checked-out commit (`HEAD` and
/// the branch reference it names, loose or packed) and the source files, and nothing else. Only
/// paths that exist are named, because a missing one would rerun every build.
///
/// The index is not watched. Staging a file changes neither the commit nor whether tracked
/// files differ from it, and Git and editors rewrite the index for many other reasons. A
/// change to a tracked file is seen through the source paths; a commit through the reference.
///
/// A repository using the reftable reference format keeps no loose reference files or
/// `packed-refs`, and its `HEAD` file never changes, so a new commit there does not rerun this
/// script: the recorded commit then stays at the one built before, until a source file changes.
pub(crate) fn watched(root: &Path, commit_known: bool) -> Vec<PathBuf> {
    let mut watched = vec![
        root.join("crates"),
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
    ];
    if commit_known {
        if let Some(head) = git(
            root,
            &["rev-parse", "--path-format=absolute", "--git-path", "HEAD"],
        ) {
            watched.push(PathBuf::from(head));
        }
        if let Some(common) = git(
            root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        ) {
            let common = PathBuf::from(common);
            watched.push(common.join("packed-refs"));
            if let Some(reference) = git(root, &["symbolic-ref", "-q", "HEAD"]) {
                watched.push(common.join(reference));
            }
        }
    }
    watched.retain(|p| p.exists());
    watched
}
