//! The build script's list of paths that make it run again.

#[path = "../../build.rs"]
#[allow(dead_code)]
mod build_script;

use std::{fs, path::Path, process::Command};

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[test]
fn the_build_script_watches_the_commit_and_sources_but_not_the_index() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::create_dir_all(root.join("crates/one")).unwrap();
    fs::write(root.join("crates/one/lib.rs"), "").unwrap();
    fs::write(root.join("Cargo.toml"), "").unwrap();
    fs::write(root.join("Cargo.lock"), "").unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "base"]);
    let git_dir = root.join(".git");
    assert!(git_dir.join("index").exists());
    let watched = build_script::watched(&root, true);
    for want in [
        root.join("crates"),
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        git_dir.join("HEAD"),
        git_dir.join("refs/heads/main"),
    ] {
        assert!(
            watched.contains(&want),
            "{} missing from {watched:?}",
            want.display()
        );
    }
    // Staging a file rewrites the index, which changes neither the commit nor the source.
    assert!(!watched.iter().any(|p| p.ends_with("index")), "{watched:?}");
    // A packed reference is watched through packed-refs; a missing loose file is not named.
    git(&root, &["pack-refs", "--all"]);
    let packed = build_script::watched(&root, true);
    assert!(packed.contains(&git_dir.join("packed-refs")), "{packed:?}");
    assert!(packed.iter().all(|p| p.exists()), "{packed:?}");
    // Without a commit, only the source paths are watched.
    assert_eq!(
        build_script::watched(&root, false),
        vec![
            root.join("crates"),
            root.join("Cargo.toml"),
            root.join("Cargo.lock")
        ]
    );
}
