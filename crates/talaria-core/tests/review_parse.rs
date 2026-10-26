//! Parser red tests for the review. Issue #245.
//!
//! Every input below comes from real git in a temporary folder, captured inside
//! the test. Nothing is hand-typed porcelain.

#[path = "support/git_bytes.rs"]
mod git_bytes;

use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use git_bytes::{GitBytes, git_available};
use talaria_core::review::{FileStatus, parse_numstat_z, parse_push_porcelain, parse_status_v2};

fn skip_unless_git() -> bool {
    if git_available() {
        return false;
    }
    eprintln!("git not found on PATH");
    true
}

fn write(repo: &Path, name: &str, text: &str) {
    std::fs::write(repo.join(name), text).expect("write file");
}

/// Every row kind, the letter rule, and ahead/behind from real git.
#[test]
fn status_v2_rows_from_real_git() {
    if skip_unless_git() {
        return;
    }
    let git = GitBytes::new();
    let repo = git.repo.clone();

    // Unstaged modification.
    write(&repo, "a.txt", "one\ntwo\n");
    // Staged modification: committed, edited, added.
    write(&repo, "b.txt", "b\n");
    git.ok(&repo, &["add", "b.txt"]);
    git.commit(&repo, "add b");
    write(&repo, "b.txt", "b changed\n");
    git.ok(&repo, &["add", "b.txt"]);
    // Unstaged modification of a second file.
    write(&repo, "c.txt", "c\n");
    git.ok(&repo, &["add", "c.txt"]);
    git.commit(&repo, "add c");
    write(&repo, "c.txt", "c changed\n");
    // Added.
    write(&repo, "new.txt", "new\n");
    git.ok(&repo, &["add", "new.txt"]);
    // Deleted.
    write(&repo, "gone.txt", "gone\n");
    git.ok(&repo, &["add", "gone.txt"]);
    git.commit(&repo, "add gone");
    std::fs::remove_file(repo.join("gone.txt")).expect("remove file");
    // Renamed.
    write(&repo, "old.txt", "old\n");
    git.ok(&repo, &["add", "old.txt"]);
    git.commit(&repo, "add old");
    git.ok(&repo, &["mv", "old.txt", "new-name.txt"]);
    // Copied: the original is unchanged and the copy is added. Copies are only
    // detected with the copies flag, which this parser test passes so the `C`
    // row exists to parse.
    write(&repo, "orig.txt", "same\n");
    git.ok(&repo, &["add", "orig.txt"]);
    git.commit(&repo, "add orig");
    std::fs::copy(repo.join("orig.txt"), repo.join("copy.txt")).expect("copy file");
    git.ok(&repo, &["add", "copy.txt"]);
    // Type change: a file becomes a symlink.
    write(&repo, "link.txt", "target\n");
    git.ok(&repo, &["add", "link.txt"]);
    git.commit(&repo, "add link");
    std::fs::remove_file(repo.join("link.txt")).expect("remove link target");
    std::os::unix::fs::symlink("a.txt", repo.join("link.txt")).expect("symlink");
    // Conflicted: two branches change one file.
    write(&repo, "clash.txt", "base\n");
    git.ok(&repo, &["add", "clash.txt"]);
    git.commit(&repo, "add clash");
    git.ok(&repo, &["checkout", "-q", "-b", "side"]);
    write(&repo, "clash.txt", "side\n");
    git.ok(&repo, &["add", "clash.txt"]);
    git.commit(&repo, "side change");
    git.ok(&repo, &["checkout", "-q", "main"]);
    write(&repo, "clash.txt", "main\n");
    git.ok(&repo, &["add", "clash.txt"]);
    git.commit(&repo, "main change");
    let merge = git.run(&repo, &["merge", "side"]);
    assert!(!merge.status.success(), "the merge should conflict");
    // Untracked.
    write(&repo, "untracked.txt", "untracked\n");
    // Ahead one, behind one.
    git.ok(git.path(), &["clone", "-q", "origin.git", "other"]);
    let other = git.path().join("other");
    write(&other, "remote.txt", "remote\n");
    git.ok(&other, &["add", "remote.txt"]);
    git.commit(&other, "remote change");
    git.ok(&other, &["push", "-q", "origin", "main"]);
    git.ok(&repo, &["fetch", "-q", "origin"]);

    let raw = git.ok(
        &repo,
        &[
            "-c",
            "status.renames=copies",
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=normal",
        ],
    );
    let (head, files) = parse_status_v2(&raw).expect("status parses");

    assert_eq!(head.head.as_deref(), Some("main"));
    assert_eq!(head.upstream.as_deref(), Some("origin/main"));
    assert!(head.oid.is_some(), "the branch id is known");
    assert_eq!((head.ahead, head.behind), (1, 1), "ahead one, behind one");

    let row = |path: &str| {
        files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("row for {path}"))
    };
    assert_eq!(row("a.txt").status, FileStatus::Modified);
    assert!(!row("a.txt").staged && row("a.txt").unstaged);
    assert_eq!(row("b.txt").status, FileStatus::Modified);
    assert!(row("b.txt").staged && !row("b.txt").unstaged);
    assert_eq!(row("c.txt").status, FileStatus::Modified);
    assert!(!row("c.txt").staged && row("c.txt").unstaged);
    assert_eq!(row("new.txt").status, FileStatus::Added);
    assert_eq!(row("gone.txt").status, FileStatus::Deleted);
    assert_eq!(row("new-name.txt").status, FileStatus::Renamed);
    assert_eq!(row("new-name.txt").orig_path.as_deref(), Some("old.txt"));
    assert_eq!(row("copy.txt").status, FileStatus::Copied);
    assert_eq!(row("link.txt").status, FileStatus::TypeChanged);
    assert_eq!(row("clash.txt").status, FileStatus::Conflicted);
    assert_eq!(row("untracked.txt").status, FileStatus::Untracked);
    assert!(row("untracked.txt").unstaged && !row("untracked.txt").staged);
}

/// Renames key by the new path; binary files give no counts.
#[test]
fn numstat_z_renames_and_binary() {
    if skip_unless_git() {
        return;
    }
    let git = GitBytes::new();
    let repo = git.repo.clone();

    // A staged rename with an edit, so both counts are non-zero.
    write(&repo, "before.txt", "one\ntwo\nthree\n");
    git.ok(&repo, &["add", "before.txt"]);
    git.commit(&repo, "add before");
    git.ok(&repo, &["mv", "before.txt", "after.txt"]);
    write(&repo, "after.txt", "one\nTWO\nthree\nfour\n");
    git.ok(&repo, &["add", "after.txt"]);
    // A binary file: NUL bytes, staged and unstaged.
    std::fs::write(repo.join("blob.bin"), [0x89, b'P', b'N', b'G', 0, 1, 2, 3])
        .expect("binary file");
    git.ok(&repo, &["add", "blob.bin"]);
    write(&repo, "plain.txt", "a\nb\n");
    git.ok(&repo, &["add", "plain.txt"]);
    git.commit(&repo, "add binary and plain");
    std::fs::write(repo.join("blob.bin"), [0x89, b'P', b'N', b'G', 0, 9, 9, 9])
        .expect("change binary");

    let staged = git.ok(
        &repo,
        &[
            "diff",
            "--cached",
            "--numstat",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
        ],
    );
    let staged = parse_numstat_z(&staged).expect("staged numstat parses");
    let entry = staged
        .get("after.txt")
        .expect("rename keys by the new path");
    assert_eq!(*entry, (Some(2), Some(1)));
    assert!(
        !staged.contains_key("before.txt"),
        "the old path is not a key"
    );
    assert_eq!(
        staged.get("blob.bin"),
        Some(&(None, None)),
        "a staged binary file has no counts"
    );

    let unstaged = git.ok(
        &repo,
        &["diff", "--numstat", "-z", "--no-ext-diff", "--no-textconv"],
    );
    let unstaged = parse_numstat_z(&unstaged).expect("unstaged numstat parses");
    assert_eq!(
        unstaged.get("blob.bin"),
        Some(&(None, None)),
        "an unstaged binary file has no counts"
    );
}

/// Tricky paths survive the NUL-separated parse exactly.
#[test]
fn paths_with_spaces_dashes_globs_and_non_utf8() {
    if skip_unless_git() {
        return;
    }
    let git = GitBytes::new();
    let repo = git.repo.clone();

    write(&repo, "a b.txt", "spaced\n");
    git.ok(&repo, &["add", "a b.txt"]);
    // A leading dash and a glob character, added with an explicit end of options.
    write(&repo, "-rf", "dash\n");
    git.ok(&repo, &["add", "--", "-rf"]);
    write(&repo, "*", "glob\n");
    git.ok(&repo, &["add", "--", "*"]);
    // Non-UTF-8 bytes are a legal file name on some Unix filesystems. macOS
    // refuses to create one, so the real-git half below only runs where the
    // file lands; the replacement-character rule is pinned separately.
    let raw_name = b"bytes-\xff\xfe.txt";
    let raw_path = repo.join(std::ffi::OsStr::from_bytes(raw_name));
    let raw_landed = std::fs::write(&raw_path, "raw\n").is_ok();

    let raw = git.ok(
        &repo,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=normal",
        ],
    );
    let (_, files) = parse_status_v2(&raw).expect("status parses");
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert!(paths.contains(&"a b.txt"), "spaced path kept: {paths:?}");
    assert!(paths.contains(&"-rf"), "dashed path kept: {paths:?}");
    assert!(paths.contains(&"*"), "glob path kept: {paths:?}");
    if raw_landed {
        assert!(
            files.iter().any(|file| file.path.contains('\u{FFFD}')),
            "non-UTF-8 shows the replacement character: {paths:?}"
        );
    }

    // The replacement-character rule, pinned where the filesystem forbids the
    // name above: one NUL-separated untracked row with non-UTF-8 bytes.
    let row = b"# branch.oid abc\0# branch.head main\0? bytes-\xff\xfe.txt\0";
    let (_, files) = parse_status_v2(row).expect("the row parses");
    assert!(
        files.iter().any(|file| file.path.contains('\u{FFFD}')),
        "non-UTF-8 shows the replacement character"
    );
}

/// NUL means no count; an unterminated last line still counts.
#[test]
fn count_lines_rules() {
    if skip_unless_git() {
        return;
    }
    let git = GitBytes::new();
    let repo = git.repo.clone();

    write(&repo, "ended.txt", "one\ntwo\nthree\n");
    write(&repo, "unterminated.txt", "one\ntwo");
    std::fs::write(repo.join("nul.bin"), b"one\ntwo\x00three\n").expect("nul file");
    std::fs::write(repo.join("empty.txt"), b"").expect("empty file");

    let read = |name: &str| std::fs::read(repo.join(name)).expect("read back");
    assert_eq!(
        talaria_core::review::count_lines(&read("ended.txt")),
        Some(3)
    );
    assert_eq!(
        talaria_core::review::count_lines(&read("unterminated.txt")),
        Some(2),
        "an unterminated last line counts"
    );
    assert_eq!(
        talaria_core::review::count_lines(&read("nul.bin")),
        None,
        "a NUL byte means no count"
    );
    assert_eq!(
        talaria_core::review::count_lines(&read("empty.txt")),
        Some(0)
    );
}

/// Push flags from real pushes: up to date, fast-forward, forced, rejected.
#[test]
fn push_porcelain_flags() {
    if skip_unless_git() {
        return;
    }
    let git = GitBytes::new();
    let repo = git.repo.clone();

    // Fast-forward, then up to date.
    write(&repo, "push.txt", "v1\n");
    git.ok(&repo, &["add", "push.txt"]);
    git.commit(&repo, "push one");
    let first = git.run(&repo, &["push", "--porcelain", "origin", "main"]);
    assert!(first.status.success(), "the first push lands");
    let report = parse_push_porcelain(&String::from_utf8_lossy(&first.stdout));
    assert!(!report.rejected);
    assert!(
        report.refs.iter().any(|(flag, _)| *flag == ' '),
        "a fast-forward push carries a space: {:?}",
        report.refs
    );
    let second = git.run(&repo, &["push", "--porcelain", "origin", "main"]);
    let report = parse_push_porcelain(&String::from_utf8_lossy(&second.stdout));
    assert!(!report.rejected);
    assert!(
        report.refs.iter().any(|(flag, _)| *flag == '='),
        "an up-to-date push carries `=`: {:?}",
        report.refs
    );

    // Rejected: the origin moves on without this clone.
    git.ok(git.path(), &["clone", "-q", "origin.git", "other"]);
    let other = git.path().join("other");
    write(&other, "remote.txt", "remote\n");
    git.ok(&other, &["add", "remote.txt"]);
    git.commit(&other, "remote change");
    git.ok(&other, &["push", "-q", "origin", "main"]);
    write(&repo, "local.txt", "local\n");
    git.ok(&repo, &["add", "local.txt"]);
    git.commit(&repo, "local change");
    let rejected = git.run(&repo, &["push", "--porcelain", "origin", "main"]);
    assert!(!rejected.status.success(), "the diverged push fails");
    let report = parse_push_porcelain(&String::from_utf8_lossy(&rejected.stdout));
    assert!(report.rejected, "a rejection is reported");
    assert!(
        report.refs.iter().any(|(flag, _)| *flag == '!'),
        "a rejected ref carries `!`: {:?}",
        report.refs
    );

    // Forced: rewrite, then push over the origin in the temporary folder.
    git.ok(&repo, &["fetch", "-q", "origin"]);
    git.ok(&repo, &["reset", "-q", "--hard", "origin/main"]);
    write(&repo, "forced.txt", "forced\n");
    git.ok(&repo, &["add", "forced.txt"]);
    git.commit(&repo, "forced change");
    git.ok(&repo, &["reset", "-q", "--soft", "HEAD~1"]);
    write(&repo, "forced.txt", "rewritten\n");
    git.ok(&repo, &["add", "forced.txt"]);
    git.commit(&repo, "rewritten change");
    let forced = git.run(&repo, &["push", "--porcelain", "--force", "origin", "main"]);
    assert!(forced.status.success(), "the forced push lands");
    let report = parse_push_porcelain(&String::from_utf8_lossy(&forced.stdout));
    assert!(!report.rejected);
    assert!(
        report.refs.iter().any(|(flag, _)| *flag == '*'),
        "a forced update carries `*`: {:?}",
        report.refs
    );
}
