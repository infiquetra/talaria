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
    // Run again when the checked-out commit, the staged state or a source file changes, and
    // not otherwise. Only paths that exist are named: a missing one would rerun every build.
    let mut watched = vec![
        root.join("crates"),
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
    ];
    if commit.is_some() {
        for args in [
            &["rev-parse", "--path-format=absolute", "--git-path", "HEAD"][..],
            &["rev-parse", "--path-format=absolute", "--git-path", "index"][..],
            &["rev-parse", "--path-format=absolute", "--git-common-dir"][..],
        ] {
            if let Some(path) = git(&root, args) {
                let path = PathBuf::from(path);
                if args.last() == Some(&"--git-common-dir") {
                    watched.push(path.join("packed-refs"));
                    if let Some(reference) = git(&root, &["symbolic-ref", "-q", "HEAD"]) {
                        watched.push(path.join(reference));
                    }
                } else {
                    watched.push(path);
                }
            }
        }
    }
    for path in watched.iter().filter(|p| p.exists()) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
