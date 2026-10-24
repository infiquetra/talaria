//! The external status command: how it is started, bounded, cleaned up and shut down.
//! Every program here is a small shell script written into a temporary directory whose
//! unique name doubles as the token that finds the script's processes.

#![cfg(unix)]

mod status_support;

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use status_support::{
    KillByToken, no_process_with, no_process_with_async, path_env, pgrep_count, pid_alive,
    pid_gone_within, temp_dir, token, wait_for_pid, wait_until_async, write_script,
};
use talaria_tui::status::{
    CommandLimits, CommandResult, CommandSpec, Decision, Failure, ParsedOutput, SessionRef,
    SkipReason, StatusCommandService, StatusDocument, StatusInputs, Tone, external_segment,
    run_command,
};

fn limits(timeout: Duration) -> CommandLimits {
    CommandLimits {
        timeout,
        stdout_bytes: 16384,
        stderr_bytes: 4096,
        rows: 8,
    }
}

fn two_seconds() -> CommandLimits {
    limits(Duration::from_secs(2))
}

fn spec(argv: Vec<String>, launch_dir: &Path) -> CommandSpec {
    CommandSpec {
        argv,
        launch_dir: launch_dir.to_path_buf(),
        env_allowlist: Vec::new(),
    }
}

fn script_spec(script: &Path, launch_dir: &Path) -> CommandSpec {
    spec(vec![script.to_string_lossy().into_owned()], launch_dir)
}

fn no_credentials(_name: &str) -> bool {
    false
}

fn flags_only_langchain(name: &str) -> bool {
    name == "LANGCHAIN_API_KEY"
}

fn document() -> StatusDocument {
    let inputs = StatusInputs {
        session: Some(SessionRef {
            id: "sess-1".to_string(),
            title: None,
        }),
        ..StatusInputs::default()
    };
    StatusDocument::from_inputs(&inputs).expect("a document for a session")
}

/// The row texts and the truncated flag of a result that parsed into rows.
fn rows_of(result: &CommandResult) -> (Vec<String>, bool) {
    match &result.parsed {
        ParsedOutput::Rows { rows, truncated } => (
            rows.iter().map(|row| row.text.clone()).collect(),
            *truncated,
        ),
        other => panic!("expected rows, got {other:?}"),
    }
}

fn failure_of(result: &CommandResult) -> &Failure {
    match &result.parsed {
        ParsedOutput::Failure(failure) => failure,
        other => panic!("expected a failure, got {other:?}"),
    }
}

async fn next_result(receiver: &Receiver<CommandResult>, limit: Duration) -> Option<CommandResult> {
    let mut found = None;
    wait_until_async(limit, || {
        found = receiver.try_recv().ok();
        found.is_some()
    })
    .await;
    found
}

fn service(spec: CommandSpec, parent_env: Vec<(String, String)>) -> StatusCommandService {
    StatusCommandService::new(
        spec,
        Duration::from_secs(5),
        parent_env,
        no_credentials,
        two_seconds(),
    )
}

#[tokio::test]
async fn command_is_started_directly_without_a_shell() {
    let dir = temp_dir();
    let _guard = KillByToken(token(dir.path()));
    let script = write_script(
        dir.path(),
        "args.sh",
        "echo \"$#\"\nfor a in \"$@\"; do echo \"$a\"; done",
    );
    let argv = vec![
        script.to_string_lossy().into_owned(),
        "$HOME".to_string(),
        "*".to_string(),
        ";".to_string(),
    ];
    let result = run_command(
        &spec(argv, dir.path()),
        path_env(),
        Vec::new(),
        &two_seconds(),
    )
    .await;
    let (rows, truncated) = rows_of(&result);
    assert_eq!(rows, ["3", "$HOME", "*", ";"]);
    assert!(!truncated);
}

#[tokio::test]
async fn stdin_receives_the_document_and_a_silent_command_is_fine() {
    let dir = temp_dir();
    let _guard = KillByToken(token(dir.path()));

    let echoing = write_script(dir.path(), "echo.sh", "printf 'got: '\ncat");
    let sent = b"{\"version\":1}\n".to_vec();
    let result = run_command(
        &script_spec(&echoing, dir.path()),
        path_env(),
        sent,
        &two_seconds(),
    )
    .await;
    let (rows, _) = rows_of(&result);
    assert_eq!(rows, ["got: {\"version\":1}"]);

    // A command that never reads its input, given far more than a pipe holds: the broken
    // pipe on the write is ignored and the run still reads what the command printed.
    let deaf = write_script(dir.path(), "deaf.sh", "echo ok");
    let started = Instant::now();
    let result = run_command(
        &script_spec(&deaf, dir.path()),
        path_env(),
        vec![b'x'; 1 << 20],
        &two_seconds(),
    )
    .await;
    let (rows, truncated) = rows_of(&result);
    assert_eq!(rows, ["ok"]);
    assert!(!truncated);
    assert!(started.elapsed() < Duration::from_millis(1500));

    // A command that prints nothing at all is a failure with its own marker.
    let silent = write_script(dir.path(), "silent.sh", "exit 0");
    let result = run_command(
        &script_spec(&silent, dir.path()),
        path_env(),
        b"{}\n".to_vec(),
        &two_seconds(),
    )
    .await;
    assert_eq!(failure_of(&result), &Failure::NoOutput);
}

#[tokio::test]
async fn working_directory_is_the_launch_directory() {
    let dir = temp_dir();
    let _guard = KillByToken(token(dir.path()));
    let launch = dir.path().join("launch");
    fs::create_dir(&launch).expect("launch directory");
    let script = write_script(dir.path(), "where.sh", "pwd -P");
    let result = run_command(
        &script_spec(&script, &launch),
        path_env(),
        Vec::new(),
        &two_seconds(),
    )
    .await;
    let (rows, _) = rows_of(&result);
    assert_eq!(rows.len(), 1);
    let printed = fs::canonicalize(&rows[0]).expect("the printed directory exists");
    let expected = fs::canonicalize(&launch).expect("launch directory");
    assert_eq!(printed, expected);
}

#[tokio::test]
async fn nonzero_exit_gives_a_marker_with_the_first_stderr_line() {
    let dir = temp_dir();
    let _guard = KillByToken(token(dir.path()));
    let script = write_script(
        dir.path(),
        "fail.sh",
        "echo out\nprintf '\\n  \\nboom\\nsecond line\\n' >&2\nexit 3",
    );
    let result = run_command(
        &script_spec(&script, dir.path()),
        path_env(),
        Vec::new(),
        &two_seconds(),
    )
    .await;
    let failure = failure_of(&result);
    assert_eq!(
        failure,
        &Failure::NonzeroExit {
            code: Some(3),
            stderr_line: Some("boom".to_string()),
        }
    );
    assert_eq!(failure.marker(), "status: command failed (exit 3): boom");
    // The standard output of a failed run is not shown, and neither is any other stderr.
    let segment = external_segment(&result).expect("a marker segment");
    assert_eq!(segment.text, "status: command failed (exit 3): boom");
    assert_eq!(segment.tone, Tone::Warning);
    assert!(!segment.text.contains("out"));
    assert!(!segment.text.contains("second"));

    // Exit with nothing on stderr: the marker has no suffix.
    let quiet = write_script(dir.path(), "quiet.sh", "exit 7");
    let result = run_command(
        &script_spec(&quiet, dir.path()),
        path_env(),
        Vec::new(),
        &two_seconds(),
    )
    .await;
    assert_eq!(
        failure_of(&result),
        &Failure::NonzeroExit {
            code: Some(7),
            stderr_line: None,
        }
    );

    // Death by a signal has no exit code.
    let killed = write_script(dir.path(), "killed.sh", "kill -KILL $$");
    let result = run_command(
        &script_spec(&killed, dir.path()),
        path_env(),
        Vec::new(),
        &two_seconds(),
    )
    .await;
    assert_eq!(
        failure_of(&result),
        &Failure::NonzeroExit {
            code: None,
            stderr_line: None,
        }
    );
}

#[tokio::test]
async fn timeout_kills_the_command_and_its_helpers() {
    let dir = temp_dir();
    let token = token(dir.path());
    let _guard = KillByToken(token.clone());
    let pid_file = dir.path().join("helper.pid");
    let script = write_script(
        dir.path(),
        "slow.sh",
        &format!("sleep 30 &\necho $! > {}\nwait", pid_file.display()),
    );
    let started = Instant::now();
    let result = run_command(
        &script_spec(&script, dir.path()),
        path_env(),
        Vec::new(),
        &limits(Duration::from_millis(300)),
    )
    .await;
    assert!(started.elapsed() < Duration::from_millis(1500));
    assert_eq!(failure_of(&result), &Failure::TimedOut);

    let helper = wait_for_pid(&pid_file, Duration::from_millis(200))
        .await
        .expect("the script recorded its helper");
    assert!(
        pid_gone_within(helper, Duration::from_secs(2)).await,
        "the helper outlived the timeout"
    );
    assert!(no_process_with_async(&token, Duration::from_secs(2)).await);
}

#[tokio::test]
async fn a_backgrounded_helper_does_not_outlive_the_run() {
    let dir = temp_dir();
    let token = token(dir.path());
    let _guard = KillByToken(token.clone());

    // The helper's output is redirected, so it holds no pipe; the leader exits at once.
    let quiet_pid = dir.path().join("quiet.pid");
    let quiet = write_script(
        dir.path(),
        "quiet.sh",
        &format!(
            "sleep 30 >/dev/null 2>&1 &\necho $! > {}\necho ok",
            quiet_pid.display()
        ),
    );
    let result = run_command(
        &script_spec(&quiet, dir.path()),
        path_env(),
        Vec::new(),
        &two_seconds(),
    )
    .await;
    let (rows, _) = rows_of(&result);
    assert_eq!(rows, ["ok"]);
    let helper = wait_for_pid(&quiet_pid, Duration::from_millis(200))
        .await
        .expect("the script recorded its helper");
    assert!(
        pid_gone_within(helper, Duration::from_secs(2)).await,
        "the quiet helper outlived the run"
    );

    // The helper keeps the output pipes open. The leader's exit must end the run at once,
    // not leave it waiting for the timeout.
    let holding_pid = dir.path().join("holding.pid");
    let holding = write_script(
        dir.path(),
        "holding.sh",
        &format!("sleep 30 &\necho $! > {}\necho ok", holding_pid.display()),
    );
    let started = Instant::now();
    let result = run_command(
        &script_spec(&holding, dir.path()),
        path_env(),
        Vec::new(),
        &two_seconds(),
    )
    .await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the run waited for the helper"
    );
    let (rows, _) = rows_of(&result);
    assert_eq!(rows, ["ok"]);
    let helper = wait_for_pid(&holding_pid, Duration::from_millis(200))
        .await
        .expect("the script recorded its helper");
    assert!(
        pid_gone_within(helper, Duration::from_secs(2)).await,
        "the pipe-holding helper outlived the run"
    );
    assert!(no_process_with_async(&token, Duration::from_secs(2)).await);
}

#[tokio::test]
async fn missing_and_unstartable_commands_are_markers() {
    let dir = temp_dir();
    let _guard = KillByToken(token(dir.path()));

    let missing = dir.path().join("does-not-exist");
    let result = run_command(
        &script_spec(&missing, dir.path()),
        path_env(),
        Vec::new(),
        &two_seconds(),
    )
    .await;
    assert_eq!(failure_of(&result), &Failure::NotFound);
    assert_eq!(failure_of(&result).marker(), "status: command not found");

    // A file without the execute bit cannot be started.
    let plain = dir.path().join("plain.sh");
    fs::write(&plain, "#!/bin/sh\necho ok\n").expect("write plain file");
    let result = run_command(
        &script_spec(&plain, dir.path()),
        path_env(),
        Vec::new(),
        &two_seconds(),
    )
    .await;
    assert!(
        matches!(failure_of(&result), Failure::SpawnFailed(_)),
        "got {:?}",
        result.parsed
    );
    assert_eq!(
        failure_of(&result).marker(),
        "status: could not start command"
    );

    // A program name with an embedded NUL byte cannot be passed to the operating system.
    let result = run_command(
        &spec(vec!["bad\0name".to_string()], dir.path()),
        path_env(),
        Vec::new(),
        &two_seconds(),
    )
    .await;
    assert!(
        matches!(failure_of(&result), Failure::SpawnFailed(_)),
        "got {:?}",
        result.parsed
    );
}

#[tokio::test]
async fn endless_output_is_capped_without_buffering_it_all() {
    let dir = temp_dir();
    let token = token(dir.path());
    let _guard = KillByToken(token.clone());
    let script = write_script(dir.path(), "endless.sh", "while :; do echo yyyyyyyy; done");
    let started = Instant::now();
    let result = run_command(
        &script_spec(&script, dir.path()),
        path_env(),
        Vec::new(),
        &two_seconds(),
    )
    .await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the run waited for the timeout"
    );
    let (rows, truncated) = rows_of(&result);
    assert!(truncated);
    assert_eq!(rows.len(), 8);
    assert!(rows[..7].iter().all(|row| row == "yyyyyyyy"));
    assert!(rows[7].starts_with("… truncated"), "got {:?}", rows[7]);
    assert!(no_process_with_async(&token, Duration::from_secs(2)).await);
}

#[tokio::test]
async fn child_environment_is_default_deny() {
    let dir = temp_dir();
    let _guard = KillByToken(token(dir.path()));
    let script = write_script(dir.path(), "env.sh", "env | sort | tr '\\n' ' '");
    let mut command = script_spec(&script, dir.path());
    command.env_allowlist = vec![
        "OPENAI_API_KEY".to_string(),
        "LANGCHAIN_API_KEY".to_string(),
        "TALARIA_GATEWAY_URL".to_string(),
        "FIXTURE_VISIBLE".to_string(),
    ];
    let mut parent: Vec<(String, String)> = vec![
        ("MY_CANARY".to_string(), "canary-value-one".to_string()),
        ("OPENAI_API_KEY".to_string(), "canary-value-two".to_string()),
        (
            "LANGCHAIN_API_KEY".to_string(),
            "canary-value-three".to_string(),
        ),
        (
            "TALARIA_GATEWAY_URL".to_string(),
            "ws://h:1/p?ticket=zz".to_string(),
        ),
        ("TALARIA_PROFILE".to_string(), "fixture".to_string()),
        ("FIXTURE_VISIBLE".to_string(), "yes".to_string()),
    ];
    parent.extend(path_env());
    parent.push((
        "HOME".to_string(),
        std::env::var("HOME").unwrap_or_else(|_| "fixture-home".to_string()),
    ));

    let mut service = StatusCommandService::new(
        command,
        Duration::from_secs(5),
        parent,
        flags_only_langchain,
        two_seconds(),
    );
    let (sender, receiver) = mpsc::channel();
    let decision = service.on_tick(Instant::now(), &document(), move |result| {
        let _ = sender.send(result);
    });
    assert_eq!(decision, Decision::Run);
    let result = next_result(&receiver, Duration::from_secs(3))
        .await
        .expect("the command finished");
    service.shutdown().await;

    let (rows, _) = rows_of(&result);
    assert_eq!(rows.len(), 1, "got {rows:?}");
    let entries: Vec<&str> = rows[0].split_whitespace().collect();
    let has = |name: &str| {
        let prefix = format!("{name}=");
        entries.iter().any(|entry| entry.starts_with(&prefix))
    };
    assert!(!has("MY_CANARY"), "an unlisted variable was forwarded");
    assert!(!has("OPENAI_API_KEY"), "a provider key was forwarded");
    assert!(
        !has("LANGCHAIN_API_KEY"),
        "a flagged credential was forwarded"
    );
    assert!(
        !entries.iter().any(|entry| entry.contains("canary-value")),
        "a canary value reached the command: {entries:?}"
    );
    assert!(
        !entries.iter().any(|entry| entry.contains("ticket")),
        "the raw gateway address reached the command: {entries:?}"
    );
    assert!(entries.contains(&"TALARIA_GATEWAY_URL=ws://h:1/p"));
    assert!(entries.contains(&"TALARIA_PROFILE=fixture"));
    assert!(entries.contains(&"FIXTURE_VISIBLE=yes"));
    assert!(has("PATH"));
    assert!(has("HOME"));
}

#[tokio::test]
async fn service_skips_a_tick_while_a_command_runs() {
    let dir = temp_dir();
    let _guard = KillByToken(token(dir.path()));
    let marker = dir.path().join("runs.txt");
    let script = write_script(
        dir.path(),
        "once.sh",
        &format!("echo run >> {}\nsleep 1\necho ok", marker.display()),
    );
    let mut service = service(script_spec(&script, dir.path()), path_env());
    let doc = document();
    let (sender, receiver) = mpsc::channel();
    let now = Instant::now();
    let first = service.on_tick(now, &doc, move |result| {
        let _ = sender.send(result);
    });
    let (other_sender, other_receiver) = mpsc::channel();
    let second = service.on_tick(now, &doc, move |result| {
        let _ = other_sender.send(result);
    });
    assert_eq!(first, Decision::Run);
    assert_eq!(second, Decision::Skip(SkipReason::Running));
    assert_eq!(
        service.next_wake(now),
        None,
        "nothing can start while it runs"
    );

    assert!(
        wait_until_async(Duration::from_secs(1), || marker.exists()).await,
        "the command never started"
    );
    let result = next_result(&receiver, Duration::from_secs(4))
        .await
        .expect("the command finished");
    let (rows, _) = rows_of(&result);
    assert_eq!(rows, ["ok"]);
    let runs = fs::read_to_string(&marker).expect("marker file");
    assert_eq!(runs.lines().count(), 1, "the command ran more than once");
    assert!(
        other_receiver.try_recv().is_err(),
        "a skipped tick reported a result"
    );
    assert!(
        service.next_wake(Instant::now()).is_some(),
        "an idle service must say when to wake"
    );
    service.shutdown().await;
}

/// A script that records its own process id, then waits with a helper.
fn long_script(dir: &Path, leader_pid: &Path, helper_pid: &Path) -> PathBuf {
    write_script(
        dir,
        "long.sh",
        &format!(
            "echo $$ > {}\nsleep 30 &\necho $! > {}\nwait",
            leader_pid.display(),
            helper_pid.display()
        ),
    )
}

#[tokio::test]
async fn shutdown_kills_a_command_in_flight() {
    // A command in flight is killed, with its helper, before shutdown returns.
    {
        let dir = temp_dir();
        let token = token(dir.path());
        let _guard = KillByToken(token.clone());
        let leader_file = dir.path().join("leader.pid");
        let helper_file = dir.path().join("helper.pid");
        let script = long_script(dir.path(), &leader_file, &helper_file);
        let mut service = service(script_spec(&script, dir.path()), path_env());
        let decision = service.on_tick(Instant::now(), &document(), |_| {});
        assert_eq!(decision, Decision::Run);
        let leader = wait_for_pid(&leader_file, Duration::from_secs(1))
            .await
            .expect("the command started");
        let helper = wait_for_pid(&helper_file, Duration::from_secs(1))
            .await
            .expect("the command started a helper");

        let started = Instant::now();
        service.shutdown().await;
        assert!(started.elapsed() < Duration::from_millis(1500));
        assert!(!pid_alive(leader), "the command was still alive at return");
        assert!(
            pid_gone_within(helper, Duration::from_secs(2)).await,
            "the helper outlived shutdown"
        );
        assert!(no_process_with_async(&token, Duration::from_secs(2)).await);

        // A second call is harmless and quick.
        let again = tokio::time::timeout(Duration::from_secs(1), service.shutdown()).await;
        assert!(again.is_ok(), "the second shutdown did not return");
    }

    // Shutdown straight after the tick, before the command has written anything: the
    // child may still be starting, and shutdown must neither miss it nor wait out the
    // two second timeout.
    {
        let dir = temp_dir();
        let token = token(dir.path());
        let _guard = KillByToken(token.clone());
        let leader_file = dir.path().join("leader.pid");
        let helper_file = dir.path().join("helper.pid");
        let script = long_script(dir.path(), &leader_file, &helper_file);
        let mut service = service(script_spec(&script, dir.path()), path_env());
        let decision = service.on_tick(Instant::now(), &document(), |_| {});
        assert_eq!(decision, Decision::Run);
        let started = Instant::now();
        service.shutdown().await;
        assert!(
            started.elapsed() < Duration::from_millis(1500),
            "shutdown waited for the command to time out"
        );
        assert!(no_process_with_async(&token, Duration::from_secs(2)).await);
        if let Some(helper) = wait_for_pid(&helper_file, Duration::from_millis(100)).await {
            assert!(pid_gone_within(helper, Duration::from_secs(2)).await);
        }
    }

    // After a finished run there is nothing to signal: a process in a group of its own
    // is left alone.
    {
        let dir = temp_dir();
        let token = token(dir.path());
        let _guard = KillByToken(token.clone());
        let bystander_script =
            write_script(dir.path(), "bystander.sh", "while :; do sleep 1; done");
        let mut bystander = Command::new(&bystander_script)
            .process_group(0)
            .spawn()
            .expect("start the bystander");
        let bystander_pid = bystander.id();

        let quick = write_script(dir.path(), "quick.sh", "echo ok");
        let mut service = service(script_spec(&quick, dir.path()), path_env());
        let (sender, receiver) = mpsc::channel();
        let decision = service.on_tick(Instant::now(), &document(), move |result| {
            let _ = sender.send(result);
        });
        assert_eq!(decision, Decision::Run);
        let result = next_result(&receiver, Duration::from_secs(3))
            .await
            .expect("the quick command finished");
        let (rows, _) = rows_of(&result);
        assert_eq!(rows, ["ok"]);

        service.shutdown().await;
        service.shutdown().await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let still_running = bystander.try_wait().expect("poll the bystander").is_none();
        let still_alive = pid_alive(bystander_pid);
        let _ = bystander.kill();
        let _ = bystander.wait();
        assert!(
            still_running && still_alive,
            "shutdown signalled an unrelated process"
        );
    }

    // After shutdown, a tick starts nothing, whether or not anything ever ran.
    {
        let dir = temp_dir();
        let token = token(dir.path());
        let _guard = KillByToken(token.clone());
        let marker = dir.path().join("started.txt");
        let script = write_script(
            dir.path(),
            "marker.sh",
            &format!("echo started > {}\necho ok", marker.display()),
        );
        let mut service = service(script_spec(&script, dir.path()), path_env());
        let started = Instant::now();
        service.shutdown().await;
        assert!(started.elapsed() < Duration::from_millis(500));

        let (sender, receiver) = mpsc::channel();
        let decision = service.on_tick(Instant::now(), &document(), move |result| {
            let _ = sender.send(result);
        });
        assert_eq!(decision, Decision::Skip(SkipReason::Stopped));
        let again = service.on_tick(Instant::now(), &document(), |_| {});
        assert_eq!(again, Decision::Skip(SkipReason::Stopped));
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!marker.exists(), "a command started after shutdown");
        assert!(receiver.try_recv().is_err());
        assert_eq!(pgrep_count(&token), 0);
        assert!(no_process_with(&token));
    }
}
