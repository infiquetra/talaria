//! The tmux command line the phase 2a suite drives, against the fake tmux binary.
//!
//! Every test builds a `TmuxCli` on a wrapper script that points the fake at its own state
//! folder, so parallel tests never share environment. All six fail until the stub in
//! `src/tmux.rs` is replaced.

mod support;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use smoke::{Paster, TmuxCli};
use support::tmux_fake::TmuxFake;

/// Runs the fake once with these arguments, without the wrapper environment.
fn call(fake: &TmuxFake, args: &[&str]) -> std::process::Output {
    Command::new(&fake.wrapper)
        .args(args)
        .env_clear()
        .output()
        .expect("run the fake tmux")
}

/// A scratch folder shaped like the run's, with a marker file path inside it.
struct Scratch {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    marker: PathBuf,
    run_id: String,
}

impl Scratch {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("temporary folder for the scratch");
        let root = tmp.path().join("talaria-smoke-test0abc");
        fs::create_dir(&root).expect("scratch folder");
        let marker = root.join("tmux-exit.txt");
        Scratch {
            _tmp: tmp,
            root,
            marker,
            run_id: "test0abc".to_owned(),
        }
    }
}

#[test]
fn paste_loads_a_private_buffer_then_pastes_with_p_and_r() {
    let fake = TmuxFake::new();
    let scratch = Scratch::new();
    let cli = TmuxCli::new(&fake.wrapper, &scratch.marker, &scratch.root);

    cli.paste("talaria-smoke-test0abc", "hello paste")
        .expect("paste works");

    let log = fake.log();
    assert_eq!(log.len(), 2, "one load and one paste: {log:?}");
    let buffer = format!("smoke-{}", scratch.run_id);
    assert_eq!(log[0][0], "load-buffer");
    assert_eq!(&log[0][1..3], &["-b", buffer.as_str()]);
    assert_eq!(log[0].len(), 4, "load-buffer carries the file: {log:?}");
    assert_eq!(
        log[1],
        vec![
            "paste-buffer",
            "-p",
            "-r",
            "-d",
            "-b",
            buffer.as_str(),
            "-t",
            "talaria-smoke-test0abc"
        ],
        "paste-buffer brackets, keeps line feeds and consumes the buffer"
    );
}

#[test]
fn pasted_file_is_private_and_removed_and_text_never_in_arguments() {
    let fake = TmuxFake::new();
    let scratch = Scratch::new();
    let cli = TmuxCli::new(&fake.wrapper, &scratch.marker, &scratch.root);

    let text = "line one\nline \"two\" $(three)\n";
    cli.paste("talaria-smoke-test0abc", text)
        .expect("paste works");

    // The text never appears in the logged arguments.
    for args in fake.log() {
        for arg in &args {
            assert!(
                !arg.contains("line one") && !arg.contains("$(three)"),
                "pasted text leaked into the arguments: {args:?}"
            );
        }
    }
    // The loaded file carried the text, was private, and is gone after the call.
    let load = &fake.log()[0];
    let file = PathBuf::from(&load[3]);
    assert!(
        !file.exists(),
        "the buffer file is removed after the call: {}",
        file.display()
    );
    let copy = fs::read_to_string(fake.dir.join("buffer_copy.txt")).expect("the fake kept a copy");
    assert_eq!(copy, text, "the file carried the pasted text");
    let mode =
        fs::read_to_string(fake.dir.join("buffer_mode.txt")).expect("the fake kept the mode");
    assert_eq!(mode.trim(), "600", "the buffer file is private");
}

#[test]
fn session_exists_follows_the_exit_status_for_both_error_texts() {
    let fake = TmuxFake::new();
    let scratch = Scratch::new();
    let cli = TmuxCli::new(&fake.wrapper, &scratch.marker, &scratch.root);

    // The fake's error texts match the recorded tmux output.
    let never = TmuxFake::with_env(&["FAKE_TMUX_NEVER_UP=1"]);
    let missing = call(&never, &["has-session", "-t", "talaria-smoke-absent"]);
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&missing.stderr),
        include_str!("fixtures/tmux/has_session_missing.txt"),
        "the missing-session text is the recorded one"
    );
    let gone = TmuxFake::with_env(&["FAKE_TMUX_NO_SERVER=1"]);
    let no_server = call(&gone, &["has-session", "-t", "talaria-smoke-absent"]);
    assert_eq!(no_server.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&no_server.stderr),
        include_str!("fixtures/tmux/has_session_no_server.txt"),
        "the no-server text is the recorded one"
    );

    assert!(
        cli.session_exists("talaria-smoke-test0abc")
            .expect("exists works"),
        "a live session exists"
    );
    cli.kill_session("talaria-smoke-test0abc")
        .expect("kill works");
    assert!(
        !cli.session_exists("talaria-smoke-test0abc")
            .expect("exists works"),
        "exit 1 with the missing-session text means gone"
    );
    let cli_gone = TmuxCli::new(&gone.wrapper, &scratch.marker, &scratch.root);
    assert!(
        !cli_gone
            .session_exists("talaria-smoke-test0abc")
            .expect("exists works"),
        "exit 1 with the no-server text means gone"
    );
}

#[test]
fn kill_session_treats_a_missing_session_or_server_as_ok() {
    let fake = TmuxFake::new();
    let scratch = Scratch::new();
    let cli = TmuxCli::new(&fake.wrapper, &scratch.marker, &scratch.root);

    cli.kill_session("talaria-smoke-absent")
        .expect("killing a missing session is success");
    cli.kill_session("talaria-smoke-test0abc")
        .expect("killing a live session is success");
    cli.kill_session("talaria-smoke-test0abc")
        .expect("killing twice is success");

    let gone = TmuxFake::with_env(&["FAKE_TMUX_NO_SERVER=1"]);
    let cli_gone = TmuxCli::new(&gone.wrapper, &scratch.marker, &scratch.root);
    cli_gone
        .kill_session("talaria-smoke-test0abc")
        .expect("killing with no server is success");
}

#[test]
fn exit_code_reads_the_marker_file() {
    let fake = TmuxFake::new();
    let scratch = Scratch::new();
    let cli = TmuxCli::new(&fake.wrapper, &scratch.marker, &scratch.root);

    assert_eq!(cli.exit_code(), None, "a missing marker file is None");
    fs::write(&scratch.marker, "SMOKE_EXIT:0\n").expect("write the marker");
    assert_eq!(cli.exit_code(), Some(0), "the marker gives zero");
    fs::write(&scratch.marker, "SMOKE_EXIT:7\n").expect("write the marker");
    assert_eq!(cli.exit_code(), Some(7), "the marker gives seven");
}

#[test]
fn missing_tmux_is_a_clear_error() {
    let scratch = Scratch::new();
    let missing = scratch.root.join("no-such-tmux");
    let cli = TmuxCli::new(&missing, &scratch.marker, &scratch.root);

    let paste = cli
        .paste("talaria-smoke-test0abc", "hello")
        .expect_err("paste without tmux fails");
    assert!(
        paste.to_string().contains("tmux"),
        "the error names tmux: {paste}"
    );
    let exists = cli
        .session_exists("talaria-smoke-test0abc")
        .expect_err("session_exists without tmux fails");
    assert!(
        exists.to_string().contains("tmux"),
        "the error names tmux: {exists}"
    );
}
