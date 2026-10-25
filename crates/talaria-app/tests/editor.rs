//! The external editor: resolution, hand-off, and result handling.
//!
//! Behavior contract: issue 228.

#![cfg(unix)]

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;

use talaria_app::comforts::editor::{
    EditResult, EditorError, cleared_notice, edit_draft, parse_notice, resolve_editor,
    unchanged_hint_notice,
};

fn require_non_root() {
    if rustix::process::geteuid().is_root() {
        panic!("run as a non-root user: this test forces a write failure with folder permissions");
    }
}

fn env_of(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn is_executable(names: &[&str]) -> impl Fn(&str) -> bool {
    let present: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
    move |name: &str| present.iter().any(|known| known == name)
}

#[test]
fn resolution_order() {
    // Config wins over the environment.
    let env = env_of(&[("VISUAL", "nano"), ("EDITOR", "emacs")]);
    let lookup = |name: &str| env.get(name).cloned();
    assert_eq!(
        resolve_editor(Some("code --wait"), &lookup, &is_executable(&[])),
        Ok(vec!["code".to_owned(), "--wait".to_owned()])
    );
    // Then VISUAL, then EDITOR, skipping blank values.
    assert_eq!(
        resolve_editor(None, &lookup, &is_executable(&[])),
        Ok(vec!["nano".to_owned()])
    );
    let env = env_of(&[("VISUAL", "   "), ("EDITOR", "emacs")]);
    let lookup = |name: &str| env.get(name).cloned();
    assert_eq!(
        resolve_editor(Some("  "), &lookup, &is_executable(&[])),
        Ok(vec!["emacs".to_owned()])
    );
    // Then the first executable on the path.
    let env = env_of(&[]);
    let lookup = |name: &str| env.get(name).cloned();
    assert_eq!(
        resolve_editor(None, &lookup, &is_executable(&["nano", "vi"])),
        Ok(vec!["nano".to_owned()])
    );
    assert_eq!(
        resolve_editor(None, &lookup, &is_executable(&["emacs"])),
        Ok(vec!["emacs".to_owned()])
    );
    // Else vi.
    assert_eq!(
        resolve_editor(None, &lookup, &is_executable(&[])),
        Ok(vec!["vi".to_owned()])
    );
}

#[test]
fn shell_style_split_and_parse_error() {
    let env = env_of(&[]);
    let lookup = |name: &str| env.get(name).cloned();
    assert_eq!(
        resolve_editor(Some("code --wait"), &lookup, &is_executable(&[])),
        Ok(vec!["code".to_owned(), "--wait".to_owned()])
    );
    assert_eq!(
        resolve_editor(
            Some("'/opt/My Editor/bin/ed' -w"),
            &lookup,
            &is_executable(&[])
        ),
        Ok(vec!["/opt/My Editor/bin/ed".to_owned(), "-w".to_owned()])
    );
    let error = resolve_editor(Some("\"code --wait"), &lookup, &is_executable(&[]));
    assert!(
        matches!(error, Err(EditorError::Parse(_))),
        "an open quote is a parse error: {error:?}"
    );
    assert_eq!(
        parse_notice("unterminated quote"),
        "Could not read the editor command: unterminated quote."
    );
}

fn run_editor(script: &str, draft: &str) -> EditResult {
    let tmp = tempfile::tempdir().unwrap();
    let argv = vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()];
    edit_draft(&argv, draft, tmp.path(), tmp.path())
}

#[test]
fn round_trip_loads_text_without_sending() {
    let out = run_editor("printf 'edited\\033[31m\\r\\n' > \"$0\"", "original draft");
    assert_eq!(out, EditResult::Loaded("edited[31m".to_owned()));

    let out = run_editor("printf 'a\\r\\nb' > \"$0\"", "original draft");
    assert_eq!(out, EditResult::Loaded("a\nb".to_owned()));
}

#[test]
fn failures_keep_the_draft() {
    require_non_root();
    let out = run_editor("exit 3", "keep me");
    assert_eq!(
        out,
        EditResult::Failed("The editor exited with status 3; the draft is unchanged.".to_owned())
    );

    let out = run_editor("kill -TERM $$", "keep me");
    assert_eq!(
        out,
        EditResult::Failed(
            "The editor was stopped by signal 15; the draft is unchanged.".to_owned()
        )
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = edit_draft(
        &["talaria-no-such-editor".to_owned()],
        "keep me",
        tmp.path(),
        tmp.path(),
    );
    assert_eq!(
        out,
        EditResult::Failed(
            "Could not start the editor \"talaria-no-such-editor\": not found. Set $VISUAL or \
             $EDITOR, or [editor] command in ~/.talaria/config.toml."
                .to_owned()
        )
    );

    let out = run_editor("printf '\\377' > \"$0\"", "keep me");
    assert_eq!(
        out,
        EditResult::Failed(
            "The edited text could not be read (not valid UTF-8); the draft is unchanged."
                .to_owned()
        )
    );

    let out = run_editor("head -c 2097152 /dev/zero | tr '\\0' a > \"$0\"", "keep me");
    assert_eq!(
        out,
        EditResult::Failed(
            "The edited text could not be read (larger than 1 MiB); the draft is unchanged."
                .to_owned()
        )
    );

    let tmp = tempfile::tempdir().unwrap();
    let locked = tmp.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    let mut permissions = std::fs::metadata(&locked).unwrap().permissions();
    permissions.set_mode(0o500);
    std::fs::set_permissions(&locked, permissions).unwrap();
    let out = edit_draft(&["true".to_owned()], "keep me", tmp.path(), &locked);
    assert_eq!(
        out,
        EditResult::Failed(
            "Could not create a temporary file for the editor (permission denied); the draft is \
             unchanged."
                .to_owned()
        )
    );
    let mut permissions = std::fs::metadata(&locked).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&locked, permissions).unwrap();
}

#[test]
fn empty_or_unchanged_results() {
    let out = run_editor(": > \"$0\"", "x");
    assert_eq!(out, EditResult::Cleared);
    assert_eq!(
        cleared_notice(),
        "The edited text was empty; the draft was cleared."
    );

    let tmp = tempfile::tempdir().unwrap();
    let out = edit_draft(&["true".to_owned()], "", tmp.path(), tmp.path());
    assert_eq!(out, EditResult::Unchanged { hint: true });
    assert_eq!(
        unchanged_hint_notice(),
        "The editor returned at once without changes. A graphical editor needs its wait flag, \
         for example \"code --wait\"."
    );

    let out = edit_draft(&["true".to_owned()], "x  ", tmp.path(), tmp.path());
    assert_eq!(out, EditResult::Unchanged { hint: true });
}

#[test]
fn temp_file_private_and_always_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let side = tmp.path().join("mode.txt");
    // The fake editor records the temporary file's mode bits without touching its
    // content; the hand-off still removes the file afterwards.
    let argv = vec![
        "sh".to_owned(),
        "-c".to_owned(),
        "ls -l \"$1\" | cut -c1-10 > \"$0\"".to_owned(),
        side.to_string_lossy().into_owned(),
    ];
    let out = edit_draft(&argv, "draft", tmp.path(), tmp.path());
    assert_eq!(out, EditResult::Unchanged { hint: true });
    assert_eq!(
        std::fs::read_to_string(&side).unwrap(),
        "-rw-------\n",
        "the temporary file is private"
    );

    let entries: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
    assert!(
        entries.iter().all(|entry| {
            entry
                .as_ref()
                .is_ok_and(|entry| entry.file_name() == "mode.txt")
        }),
        "the temporary file is gone after every path"
    );

    // A failing run removes it too.
    let out = edit_draft(
        &["sh".to_owned(), "-c".to_owned(), "exit 3".to_owned()],
        "draft",
        tmp.path(),
        tmp.path(),
    );
    assert!(matches!(out, EditResult::Failed(_)));
    let entries: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
    assert_eq!(entries.len(), 1);
}
