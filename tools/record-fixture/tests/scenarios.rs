//! The fixture script's logic: preconditions, evidence, substitutions and the scenario driver.
//!
//! Pure functions (`preflight`, `check_evidence`, `build_substitutions`, `ScratchDir`) are tested
//! directly. `preflight` runs against a mock Hermes home (a control socket plus an in-process
//! WebSocket gateway). `run_scenario` runs over a real `hermes_client::Client` connected to the
//! scripted gateway of `support` (spec 14.2). The exit code of the script follows the error type:
//! a `PreconditionError` is exit 2 and an `AttemptResult::PreconditionAbsent` is exit 2 at once;
//! the binary itself is run for the paths where the property is about what `main` leaves behind.

mod support;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hermes_client::{InboundStream, ReplaySource};
use record_fixture::{
    AttemptResult, DiscoveryFacts, DriverConfig, EnvFacts, Preflight, PreflightInput, SCENARIOS,
    SCRIPTED_SCENARIOS, Scenario, ScratchDir, build_substitutions, check_evidence, preflight,
    run_scenario,
};
use serde_json::{Value, json};
use support::{
    Gateway, HomeSpec, MODEL, MockHome, PROVIDER, Rig, SESSION_ID, Script, StubHermes, admission,
    empty_home, event_frame, plain_script, random_hex, wrong_provider_script,
};

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn scenario(name: &str) -> Scenario {
    Scenario::named(name).unwrap_or_else(|| panic!("scenario {name} is not defined"))
}

#[test]
fn background_work_uses_the_exact_tick_script_and_prompt() {
    let scratch = ScratchDir::create_for("background-work").expect("scratch folder");
    assert_eq!(
        std::fs::read_to_string(scratch.path().join("tick.sh")).expect("tick script"),
        "for i in $(seq 1 600); do echo \"tick $i\"; sleep 1; done\n"
    );
    let background = scenario("background-work");
    assert_eq!(
        background.prompts,
        [
            "Run the command sh tick.sh in the background with the terminal tool (background=true, notify=false). Reply with only its process id."
        ]
    );
    assert_eq!(background.expected_statuses, ["complete"]);
}

fn driver_config(scratch: &Path, attempt_timeout: Duration) -> DriverConfig {
    DriverConfig {
        model: MODEL.to_owned(),
        provider: PROVIDER.to_owned(),
        scratch: scratch.to_path_buf(),
        second: None,
        home: scratch.to_path_buf(),
        attempt_timeout,
    }
}

/// Run one scenario over the rig.
async fn drive(
    rig: &mut Rig,
    name: &str,
    scratch: &Path,
    attempt_timeout: Duration,
) -> AttemptResult {
    let scenario = scenario(name);
    let config = driver_config(scratch, attempt_timeout);
    let inbound: &mut InboundStream = &mut rig.inbound;
    run_scenario(&rig.client, inbound, &scenario, &config).await
}

fn input(home: &Path, login: &str, host: &str) -> PreflightInput {
    PreflightInput {
        hermes_home: home.to_path_buf(),
        profile: "fixture".to_owned(),
        login_name: login.to_owned(),
        host_name: host.to_owned(),
    }
}

/// A login name and host name that are long enough.
fn long_names() -> (String, String) {
    (
        format!("login{}", &random_hex()[..6]),
        format!("host{}", &random_hex()[..6]),
    )
}

fn contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(object) => {
            object.contains_key(key) || object.values().any(|inner| contains_key(inner, key))
        }
        Value::Array(items) => items.iter().any(|inner| contains_key(inner, key)),
        _ => false,
    }
}

/// No request carries `config.set`, `unattended` or `profile`.
fn assert_clean_requests(gateway: &Gateway) {
    for seen in gateway.seen() {
        assert_ne!(seen.method(), Some("config.set"), "config.set was sent");
        assert!(
            !contains_key(&seen.frame, "unattended"),
            "unattended was sent in {:?}",
            seen.method()
        );
        assert!(
            !contains_key(&seen.frame, "profile"),
            "profile was sent in {:?}",
            seen.method()
        );
    }
}

fn index_of(methods: &[String], method: &str) -> Option<usize> {
    methods.iter().position(|candidate| candidate == method)
}

fn last_index_of(methods: &[String], method: &str) -> Option<usize> {
    methods.iter().rposition(|candidate| candidate == method)
}

fn count_of(methods: &[String], method: &str) -> usize {
    methods
        .iter()
        .filter(|candidate| *candidate == method)
        .count()
}

fn scratch_dir_for_driver() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap_or_else(|_| panic!("scratch failed"));
    std::fs::write(dir.path().join("notes.txt"), "alpha\nbeta\n")
        .unwrap_or_else(|_| panic!("notes failed"));
    dir
}

fn dir_names(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(path)
        .unwrap_or_else(|_| panic!("read_dir failed"))
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn terminal_command() -> &'static str {
    "rm -rf .pytest_cache"
}

fn approval_request_payload(prompt_id: &str) -> Value {
    json!({
        "kind": "approval",
        "prompt_id": prompt_id,
        "execution_generation": 1,
        "command": terminal_command(),
        "description": "recursive delete",
        "choices": ["once", "session", "always", "deny"],
        "timeout_s": 300
    })
}

fn prompt_marker(kind: &str, prompt_id: &str) -> Value {
    json!({"kind": kind, "prompt_id": prompt_id, "execution_generation": 1})
}

// ---------------------------------------------------------------------------------------------
// Scenario table (spec 8.4)
// ---------------------------------------------------------------------------------------------

#[test]
fn scenario_table_matches_the_spec() {
    assert_eq!(
        SCENARIOS,
        [
            "plain-answer",
            "tool-diff",
            "approval",
            "clarification",
            "interrupted",
            "subagent-batch",
            "queued-prompt",
            "slash-palette",
            "rewind-and-rollback",
            "context-compaction",
            "attachments-mention-image",
            "todo-checklist",
            "hermes-commands",
        ]
    );
    assert!(Scenario::named("not-a-scenario").is_none());
    // The scripted scenarios are the ones the recorder cannot drive with a plain prompt.
    assert_eq!(
        SCRIPTED_SCENARIOS,
        [
            "queue-edit-remove",
            "queue-steer",
            "queue-move-send-now",
            "model-switch",
            "permissions-cd",
            "shell-and-btw",
            "background-work",
            "bg-launch",
            "review-session-info",
            "review-oneshot",
        ]
    );
    for name in SCRIPTED_SCENARIOS {
        assert!(
            !SCENARIOS.contains(&name),
            "{name} is both a plain and a scripted scenario"
        );
    }
    let table: [(&str, &[&str], &[&str]); 13] = [
        (
            "plain-answer",
            &["Reply with exactly the single word: pong. Do not use any tools."],
            &["complete"],
        ),
        (
            "tool-diff",
            &[
                "In notes.txt change the word beta to gamma. Use your file editing tool, then reply with the single word: done.",
            ],
            &["complete"],
        ),
        (
            "approval",
            &["Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache"],
            &["complete"],
        ),
        (
            "clarification",
            &[
                "Before anything else, use the clarify tool to ask me which colour I prefer, with the choices red and blue. Then tell me the colour I chose.",
            ],
            &["complete"],
        ),
        (
            "interrupted",
            &["Count from 1 to 500, one number per line, and do not stop early."],
            &["interrupted"],
        ),
        (
            "subagent-batch",
            &[
                "Use the delegate_task tool to run two subagents in parallel: one lists the files in the current directory, the other reads notes.txt. Then summarise both results in one sentence.",
            ],
            &["complete"],
        ),
        (
            "queued-prompt",
            &[
                "Count from 1 to 30, one number per line.",
                "Reply with the single word: second.",
            ],
            &["complete", "complete"],
        ),
        ("slash-palette", &[], &["complete"]),
        (
            "rewind-and-rollback",
            &[
                "Create a file named notes.txt containing exactly the line alpha. Use your file writing tool, not the terminal. Reply with the word done.",
                "In notes.txt replace alpha with beta using your patch or file tool, not the terminal. Reply with the word done.",
            ],
            &["complete", "complete", "complete"],
        ),
        (
            "context-compaction",
            &[
                "Reply with only the word one.",
                "Reply with only the word two.",
                "Reply with only the word three.",
                "Reply with only the word four.",
                "Reply with only the word five.",
                "Reply with only the word six.",
                "Reply with only the word seven.",
                "Reply with only the word eight.",
                "Reply with only the word nine.",
                "Reply with only the word ten.",
                "Reply with only the word eleven.",
                "Reply with only the word twelve.",
                "Reply with only the word thirteen.",
                "Reply with only the word fourteen.",
                "Reply with only the word fifteen.",
                "Reply with only the word sixteen.",
                "Reply with only the word done.",
            ],
            &[
                "complete", "complete", "complete", "complete", "complete", "complete", "complete",
                "complete", "complete", "complete", "complete", "complete", "complete", "complete",
                "complete", "complete", "complete",
            ],
        ),
        ("attachments-mention-image", &[], &["complete"]),
        (
            "todo-checklist",
            &[
                "Use your todo_list tool to write a plan with exactly three items: create a.txt, run sleep 30, create b.txt. Before each step mark it in_progress with the todo_list tool, and after it mark it completed. Do the steps in this folder. Then reply with the single word: done.",
            ],
            &["complete"],
        ),
        // The command scenario submits no prompt, so no turn ends in the recording.
        ("hermes-commands", &[], &[]),
    ];
    for (name, prompts, statuses) in table {
        let scenario = scenario(name);
        assert_eq!(scenario.name, name);
        assert_eq!(scenario.prompts, prompts, "prompts of {name}");
        assert_eq!(scenario.expected_statuses, statuses, "statuses of {name}");
    }
}

// ---------------------------------------------------------------------------------------------
// check_evidence
// ---------------------------------------------------------------------------------------------

fn message(kind: &str, payload: Value) -> (&'static str, Value) {
    ("in", event_frame(1, kind, payload))
}

fn complete(status: &str) -> (&'static str, Value) {
    message("message.complete", json!({"text": "", "status": status}))
}

fn start() -> (&'static str, Value) {
    message("message.start", json!({}))
}

fn delta() -> (&'static str, Value) {
    message("message.delta", json!({"text": "x"}))
}

fn submit_out() -> (&'static str, Value) {
    (
        "out",
        json!({"jsonrpc": "2.0", "id": 3, "method": "prompt.submit",
               "params": {"session_id": SESSION_ID, "text": "t", "submission_id": "u-1"}}),
    )
}

/// Write a hand-made version 3 recording and open it.
fn recording(
    dir: &Path,
    file: &str,
    scenario: &str,
    statuses: &[&str],
    frames: &[(&'static str, Value)],
) -> ReplaySource {
    let path = dir.join(file);
    let mut text = String::new();
    text.push_str(
        &json!({
            "kind": "header",
            "version": 3,
            "startedAt": "2026-10-23T09:14:02.331Z",
            "endpoint": "ws://<gateway>/api/ws",
            "subprotocols": ["hermes-gateway-v1"],
            "scenario": scenario,
            "expectedStatuses": statuses
        })
        .to_string(),
    );
    text.push('\n');
    for (index, (dir_name, frame)) in frames.iter().enumerate() {
        text.push_str(
            &json!({
                "kind": "frame",
                "seq": index + 1,
                "tMs": index * 10,
                "generation": 1,
                "dir": dir_name,
                "frame": frame
            })
            .to_string(),
        );
        text.push('\n');
    }
    std::fs::write(&path, text).unwrap_or_else(|_| panic!("recording write failed"));
    ReplaySource::open(&path).unwrap_or_else(|err| panic!("recording did not open: {err}"))
}

fn evidence(name: &str, statuses: &[&str], frames: &[(&'static str, Value)]) -> Result<(), String> {
    let dir = tempfile::tempdir().unwrap_or_else(|_| panic!("temp dir failed"));
    let source = recording(dir.path(), "run.jsonl", name, statuses, frames);
    check_evidence(&scenario(name), &source)
}

/// The frames of a good `rewind-and-rollback` run, one call and its answer at a time.
fn rewind_frames(
    enabled: bool,
    rows: usize,
    first_tool: &str,
    approval: bool,
    skip: Option<&str>,
) -> Vec<(&'static str, Value)> {
    let mut frames: Vec<(&'static str, Value)> = Vec::new();
    let mut id = 0_u64;
    let mut call = |frames: &mut Vec<(&'static str, Value)>, method: &str, result: Value| {
        id += 1;
        if skip != Some(method) {
            frames.push((
                "out",
                json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {}}),
            ));
            frames.push(("in", json!({"jsonrpc": "2.0", "id": id, "result": result})));
        }
    };
    for tool in [first_tool, "patch"] {
        call(&mut frames, "prompt.submit", json!({}));
        frames.push(start());
        if approval {
            frames.push(message("approval.request", approval_request_payload("p-1")));
        }
        frames.push(message(
            "tool.start",
            json!({"tool_id": "t1", "name": tool}),
        ));
        frames.push(complete("complete"));
    }
    let checkpoints: Vec<Value> = (0..rows)
        .map(|row| json!({"hash": format!("{row:040}")}))
        .collect();
    call(
        &mut frames,
        "rollback.list",
        json!({"enabled": enabled, "checkpoints": checkpoints}),
    );
    call(&mut frames, "rollback.diff", json!({"diff": ""}));
    call(&mut frames, "rollback.session_diff", json!({"diff": ""}));
    call(&mut frames, "session.resume", json!({}));
    call(&mut frames, "session.mutate", json!({}));
    call(&mut frames, "prompt.submit", json!({}));
    frames.push(complete("complete"));
    call(&mut frames, "session.resume", json!({}));
    call(&mut frames, "rollback.restore", json!({}));
    frames
}

#[test]
fn rewind_and_rollback_evidence_needs_every_step_and_the_file_tools() {
    let statuses = ["complete", "complete", "complete"];
    let good = rewind_frames(true, 2, "write_file", false, None);
    assert_eq!(evidence("rewind-and-rollback", &statuses, &good), Ok(()));

    let cases = [
        (
            "checkpoints off",
            rewind_frames(false, 2, "write_file", false, None),
        ),
        ("one row", rewind_frames(true, 1, "write_file", false, None)),
        (
            "no file tool",
            rewind_frames(true, 2, "read_file", false, None),
        ),
        (
            "an approval",
            rewind_frames(true, 2, "write_file", true, None),
        ),
        (
            "no restore answer",
            rewind_frames(true, 2, "write_file", false, Some("rollback.restore")),
        ),
    ];
    for (label, frames) in cases {
        assert!(
            evidence("rewind-and-rollback", &statuses, &frames).is_err(),
            "{label} was accepted"
        );
    }
}

#[test]
fn evidence_check_accepts_good_run_and_rejects_missing_one() {
    let diff = |value: Value| {
        message(
            "tool.complete",
            json!({"tool_id": "t1", "name": "patch", "inline_diff": value}),
        )
    };
    let tool_start = || message("tool.start", json!({"tool_id": "t1", "name": "patch"}));
    let subagent = |kind: &str| {
        message(
            kind,
            json!({"goal": "list", "task_count": 2, "task_index": 0}),
        )
    };
    let approval_request = || message("approval.request", approval_request_payload("p-1"));
    let approval_settled = || message("approval.settled", prompt_marker("approval", "p-1"));
    let clarify_request = || {
        message(
            "clarify.request",
            json!({"kind": "clarify", "prompt_id": "c-1", "question": "colour?",
                   "choices": ["red", "blue"], "multi_select": false}),
        )
    };
    let clarify_settled = || message("clarify.settled", prompt_marker("clarify", "c-1"));

    // (label, scenario, header statuses, frames, accepted)
    #[allow(clippy::type_complexity)]
    let cases: Vec<(&str, &str, Vec<(&'static str, Value)>, bool)> = vec![
        (
            "plain answer",
            "plain-answer",
            vec![submit_out(), start(), delta(), complete("complete")],
            true,
        ),
        (
            "plain answer without a delta",
            "plain-answer",
            vec![submit_out(), start(), complete("complete")],
            false,
        ),
        (
            "plain answer that used a tool",
            "plain-answer",
            vec![
                submit_out(),
                start(),
                tool_start(),
                delta(),
                complete("complete"),
            ],
            false,
        ),
        (
            "plain answer that ended in error",
            "plain-answer",
            vec![submit_out(), start(), delta(), complete("error")],
            false,
        ),
        (
            "plain answer that never completed",
            "plain-answer",
            vec![submit_out(), start(), delta()],
            false,
        ),
        (
            "tool diff",
            "tool-diff",
            vec![
                start(),
                tool_start(),
                diff(json!("--- a\n+++ b\n-beta\n+gamma")),
                delta(),
                complete("complete"),
            ],
            true,
        ),
        (
            "tool diff with a null diff",
            "tool-diff",
            vec![
                start(),
                tool_start(),
                diff(Value::Null),
                delta(),
                complete("complete"),
            ],
            false,
        ),
        (
            "tool diff with an empty diff",
            "tool-diff",
            vec![
                start(),
                tool_start(),
                diff(json!("")),
                delta(),
                complete("complete"),
            ],
            false,
        ),
        (
            "approval",
            "approval",
            vec![
                start(),
                approval_request(),
                approval_settled(),
                delta(),
                complete("complete"),
            ],
            true,
        ),
        (
            "approval never settled",
            "approval",
            vec![start(), approval_request(), delta(), complete("complete")],
            false,
        ),
        (
            "approval cancelled is not evidence",
            "approval",
            vec![
                start(),
                approval_request(),
                message(
                    "approval.cancelled",
                    json!({"session_id": SESSION_ID, "stored_session_id": "stored-1",
                           "reason": "interrupt", "cancelled_count": 1, "request_ids": []}),
                ),
                delta(),
                complete("complete"),
            ],
            false,
        ),
        (
            "approval request missing",
            "approval",
            vec![start(), approval_settled(), delta(), complete("complete")],
            false,
        ),
        (
            "clarification",
            "clarification",
            vec![
                start(),
                clarify_request(),
                clarify_settled(),
                delta(),
                complete("complete"),
            ],
            true,
        ),
        (
            "clarification never settled",
            "clarification",
            vec![start(), clarify_request(), delta(), complete("complete")],
            false,
        ),
        (
            "interrupted",
            "interrupted",
            vec![start(), delta(), complete("interrupted")],
            true,
        ),
        (
            "interrupted scenario that completed",
            "interrupted",
            vec![start(), delta(), complete("complete")],
            false,
        ),
        (
            "subagent batch",
            "subagent-batch",
            vec![
                start(),
                subagent("subagent.start"),
                subagent("subagent.complete"),
                delta(),
                complete("complete"),
            ],
            true,
        ),
        (
            "subagent batch without a complete",
            "subagent-batch",
            vec![
                start(),
                subagent("subagent.start"),
                delta(),
                complete("complete"),
            ],
            false,
        ),
        (
            "queued prompt",
            "queued-prompt",
            vec![
                start(),
                delta(),
                complete("complete"),
                start(),
                delta(),
                complete("complete"),
            ],
            true,
        ),
        (
            "queued prompt with only one completion",
            "queued-prompt",
            vec![start(), delta(), complete("complete"), start(), delta()],
            false,
        ),
        (
            "queued prompt whose second start came first",
            "queued-prompt",
            vec![
                start(),
                start(),
                delta(),
                complete("complete"),
                complete("complete"),
            ],
            false,
        ),
    ];
    for (label, name, frames, accepted) in cases {
        let expected = scenario(name).expected_statuses;
        let statuses: Vec<&str> = expected.iter().map(String::as_str).collect();
        let outcome = evidence(name, &statuses, &frames);
        assert_eq!(
            outcome.is_ok(),
            accepted,
            "{label}: expected accepted={accepted}, got {outcome:?}"
        );
        if let Err(reason) = outcome {
            assert!(!reason.is_empty(), "{label}: the reason is empty");
        }
    }

    // The same good recording with one frame removed: first accepted, then rejected.
    let good = vec![submit_out(), start(), delta(), complete("complete")];
    assert_eq!(evidence("plain-answer", &["complete"], &good), Ok(()));
    for removed in [2, 3] {
        let mut frames = good.clone();
        frames.remove(removed);
        assert!(
            evidence("plain-answer", &["complete"], &frames).is_err(),
            "removing frame {removed} must be rejected"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// preflight
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn no_gateway_is_exit_2_and_hermes_never_run() {
    let (login, host) = long_names();
    let home = empty_home();
    let error = preflight(&input(home.path(), &login, &host))
        .await
        .expect_err("an empty home has no gateway");
    assert!(
        error
            .message
            .contains("no Hermes gateway is running for this home"),
        "the message names the missing gateway"
    );

    // The same through the binary: a stub `hermes` first on PATH must never run, the
    // exit code is 2, and nothing is written to the output directory.
    let stub = StubHermes::new();
    let out = tempfile::tempdir().unwrap_or_else(|_| panic!("out dir failed"));
    let user_home = tempfile::tempdir().unwrap_or_else(|_| panic!("user home failed"));
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_record-fixture"))
        .arg("record")
        .arg("--home")
        .arg(home.path())
        .arg("--out")
        .arg(out.path())
        .env("PATH", stub.path_with_stub())
        .env("HOME", user_home.path())
        .env("USER", &login)
        .env("LOGNAME", &login)
        .kill_on_drop(true)
        .output()
        .await
        .unwrap_or_else(|_| panic!("the binary did not start"));
    assert_eq!(output.status.code(), Some(2), "exit code");
    assert!(!stub.was_run(), "the stub hermes ran");
    assert!(dir_names(out.path()).is_empty(), "something was written");
    // The host name cannot be injected into the child; a short one fails the name check first.
    if gethostname::gethostname().len() >= 4 {
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            combined.contains("no Hermes gateway is running for this home"),
            "the message names the missing gateway"
        );
    }
}

#[tokio::test]
async fn unserved_profile_is_exit_2() {
    let (login, host) = long_names();
    // Served: default only.
    let spec = HomeSpec {
        served: Vec::new(),
        ..HomeSpec::good()
    };
    let home = MockHome::start(spec, plain_script()).await;
    let error = preflight(&input(home.path(), &login, &host))
        .await
        .expect_err("fixture is not served");
    assert!(
        error.message.contains("fixture"),
        "the message names the profile"
    );

    // Parked is not served either.
    let spec = HomeSpec {
        served: Vec::new(),
        parked: vec![("fixture".to_owned(), "broken config".to_owned())],
        ..HomeSpec::good()
    };
    let home = MockHome::start(spec, plain_script()).await;
    let error = preflight(&input(home.path(), &login, &host))
        .await
        .expect_err("fixture is parked");
    assert!(
        error.message.contains("fixture"),
        "the message names the profile"
    );
}

#[tokio::test]
async fn pinned_sha_mismatch_or_missing_is_exit_2() {
    let (login, host) = long_names();
    let commit = hermes_contract::pinned::COMMIT;
    let wrong = format!("{:0<40}", "f");
    let short_prefix = commit[..6].to_owned();
    for (label, sha) in [
        ("different commit", Some(wrong)),
        ("null code_sha", None),
        ("prefix under seven characters", Some(short_prefix)),
    ] {
        let spec = HomeSpec {
            code_sha: sha,
            ..HomeSpec::good()
        };
        let home = MockHome::start(spec, plain_script()).await;
        let outcome = preflight(&input(home.path(), &login, &host)).await;
        assert!(outcome.is_err(), "{label} must be refused");
    }
}

#[tokio::test]
async fn approvals_mode_not_manual_is_exit_2() {
    let (login, host) = long_names();
    for (label, mode) in [
        ("smart", json!("smart")),
        ("off", json!("off")),
        ("null", Value::Null),
    ] {
        let spec = HomeSpec {
            approvals_mode: mode,
            ..HomeSpec::good()
        };
        let home = MockHome::start(spec, plain_script()).await;
        let error = preflight(&input(home.path(), &login, &host))
            .await
            .expect_err("approvals.mode is not manual");
        assert!(
            error.message.contains("approvals.mode") && error.message.contains("manual"),
            "{label}: the message names the setting"
        );
        let reads = home.gateway.calls("config.get");
        assert_eq!(reads.len(), 1, "{label}: one config.get");
        assert_eq!(reads[0].params()["key"], json!("approvals.mode"));
        assert!(
            reads[0].params().get("profile").is_none_or(Value::is_null),
            "{label}: config.get carries no profile"
        );
        assert_clean_requests(&home.gateway);
    }
}

#[tokio::test]
async fn short_login_name_is_exit_2() {
    let (login, host) = long_names();
    let home = MockHome::start(HomeSpec::good(), plain_script()).await;

    let short_login = "zq9".to_owned();
    let error = preflight(&input(home.path(), &short_login, &host))
        .await
        .expect_err("a three character login name is refused");
    assert!(
        error.message.to_lowercase().contains("login"),
        "the message names the login name check"
    );
    assert!(
        !error.message.contains(&short_login),
        "the message must not print the name"
    );
    assert!(
        !error.message.contains("no Hermes gateway"),
        "the gateway is fine; the name is the defect"
    );

    let short_host = "zq9".to_owned();
    let error = preflight(&input(home.path(), &login, &short_host))
        .await
        .expect_err("a three character host name is refused");
    assert!(
        error.message.to_lowercase().contains("host"),
        "the message names the host name check"
    );
    assert!(
        !error.message.contains(&short_host),
        "the message must not print the name"
    );
}

#[tokio::test]
async fn preflight_accepts_a_ready_manual_gateway() {
    let commit = hermes_contract::pinned::COMMIT;
    let seven = commit[..7].to_owned();
    let other = format!("p{}", &random_hex()[..8]);
    let parked = format!("p{}", &random_hex()[..8]);
    let spec = HomeSpec {
        code_sha: Some(seven.clone()),
        served: vec!["fixture".to_owned(), other.clone()],
        parked: vec![(parked.clone(), "broken config".to_owned())],
        approvals_mode: json!("manual"),
    };
    let home = MockHome::start(spec, plain_script()).await;
    // Exactly four characters each is long enough.
    let result: Preflight = preflight(&input(home.path(), "abcd", "wxyz"))
        .await
        .unwrap_or_else(|err| panic!("preflight refused a good gateway: {}", err.message));
    assert_eq!(
        result.profile_id,
        format!("{}/profiles/fixture", home.path().display())
    );
    assert_eq!(result.api_origin, home.gateway.origin());
    assert_eq!(result.code_sha, seven);
    let mut others = result.other_profiles.clone();
    others.sort();
    let mut expected = vec![other, parked];
    expected.sort();
    assert_eq!(
        others, expected,
        "other profiles exclude fixture and default"
    );

    let verbs = home.verbs();
    assert!(
        verbs
            .iter()
            .all(|verb| verb == "identify" || verb == "session-ticket"),
        "only identify and session-ticket reach the control socket"
    );
    assert_clean_requests(&home.gateway);
}

// ---------------------------------------------------------------------------------------------
// build_substitutions and ScratchDir
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct SharedBuffer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        support::lock(&self.0).extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn substitution_pairs_cover_home_tmp_gateway_user_profile() {
    let base = tempfile::tempdir().unwrap_or_else(|_| panic!("temp dir failed"));
    let home = base.path().join("userhome");
    let home_text = home.to_string_lossy().into_owned();
    let profile_id = format!("{home_text}/.hermes/profiles/fixture");
    let login = format!("user{}", &random_hex()[..8]);
    let scratch = base.path().join("scratch");
    std::fs::create_dir_all(&scratch).unwrap_or_else(|_| panic!("scratch failed"));
    let scratch_text = scratch.to_string_lossy().into_owned();
    let resolved = std::fs::canonicalize(&scratch)
        .unwrap_or_else(|_| panic!("canonical failed"))
        .to_string_lossy()
        .into_owned();
    let authority = "127.0.0.1:9119";

    let facts = DiscoveryFacts {
        profile_id: profile_id.clone(),
        api_origin: format!("http://{authority}"),
    };
    let env = EnvFacts {
        home: home_text.clone(),
        login_name: login.clone(),
    };
    let pairs = build_substitutions(&facts, &env, &scratch);

    let has = |needle: &str, replacement: &str| {
        pairs.iter().any(|(n, r)| n == needle && r == replacement)
    };
    assert!(has(&profile_id, "<profile>"), "profile id to <profile>");
    assert!(has(&home_text, "<home>"), "home to <home>");
    assert!(has(&scratch_text, "<tmp>"), "scratch project to <tmp>");
    assert!(has(&resolved, "<tmp>"), "resolved scratch path to <tmp>");
    assert!(
        has(authority, "<gateway>"),
        "gateway host and port to <gateway>"
    );
    assert!(has(&login, "<user>"), "login name to <user>");
    for (needle, _) in &pairs {
        assert!(
            needle.chars().count() >= 4,
            "a needle is under four characters"
        );
        assert_ne!(needle, "fixture", "the public profile name stays");
    }

    // Through a recorder: the longer profile id wins over the home prefix it contains.
    let buffer = SharedBuffer::default();
    let options = hermes_client::RecorderOptions {
        endpoint: format!("ws://{authority}/api/ws"),
        substitutions: pairs.clone(),
        ..Default::default()
    };
    let recorder = hermes_client::Recorder::from_writer(buffer.clone(), options)
        .unwrap_or_else(|err| panic!("recorder refused the pairs: {err}"));
    let frame = json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {"type": "session.info", "payload": {
            "profile_id": profile_id,
            "path": format!("{home_text}/work/a"),
            "cwd": format!("{resolved}/sub"),
            "origin": format!("http://{authority}/api"),
            "who": login
        }}
    });
    recorder.record_inbound(1, &frame.to_string());
    recorder
        .finish()
        .unwrap_or_else(|err| panic!("finish failed: {err}"));
    let written = String::from_utf8_lossy(&support::lock(&buffer.0)).into_owned();
    for expected in [
        "<profile>",
        "<home>/work/a",
        "<tmp>/sub",
        "http://<gateway>/api",
        "<user>",
    ] {
        assert!(written.contains(expected), "the recording lacks {expected}");
    }
    let forbidden_values: [&str; 5] = [&home_text, &resolved, &scratch_text, &login, authority];
    for forbidden in forbidden_values {
        assert!(
            !written.contains(forbidden),
            "a machine specific value reached the recording"
        );
    }
    assert!(
        !written.contains("<home>/.hermes"),
        "the profile id was cut at the home prefix"
    );
}

#[test]
fn scratch_dir_removed_on_drop() {
    let first = ScratchDir::create().unwrap_or_else(|err| panic!("create failed: {err}"));
    let second = ScratchDir::create().unwrap_or_else(|err| panic!("create failed: {err}"));
    let path: PathBuf = first.path().to_path_buf();
    assert!(path.is_absolute());
    assert_ne!(first.path(), second.path(), "each scratch directory is new");

    let temp = std::fs::canonicalize(std::env::temp_dir())
        .unwrap_or_else(|_| panic!("temp dir canonical failed"));
    let resolved =
        std::fs::canonicalize(&path).unwrap_or_else(|_| panic!("scratch canonical failed"));
    assert!(
        resolved.starts_with(&temp),
        "the scratch project is under the temp dir"
    );

    let notes = std::fs::read_to_string(path.join("notes.txt"))
        .unwrap_or_else(|_| panic!("notes.txt missing"));
    assert_eq!(notes.lines().collect::<Vec<_>>(), ["alpha", "beta"]);
    assert!(
        path.join(".pytest_cache").is_dir(),
        ".pytest_cache is a directory"
    );

    drop(first);
    assert!(!path.exists(), "the scratch directory survived its drop");
    assert!(
        second.path().exists(),
        "dropping one must not remove the other"
    );
}

// ---------------------------------------------------------------------------------------------
// run_scenario over a scripted gateway
// ---------------------------------------------------------------------------------------------

/// A script for a scenario that answers the submission and then follows `after`.
fn submit_then(events: Vec<(&'static str, Value)>) -> Script {
    Box::new(move |seen, ctx| {
        if seen.method() == Some("prompt.submit") {
            ctx.reply(admission(1, "started"));
            for (kind, payload) in events.clone() {
                ctx.event(kind, payload);
            }
            return true;
        }
        false
    })
}

#[tokio::test]
async fn unexpected_approval_is_denied_and_fails_attempt() {
    let script: Script = Box::new(|seen, ctx| match seen.method() {
        Some("prompt.submit") => {
            ctx.reply(admission(1, "started"));
            ctx.event("message.start", json!({}));
            ctx.event("message.delta", json!({"text": "po"}));
            ctx.event("approval.request", approval_request_payload("prompt-1"));
            true
        }
        Some("approval.respond") => {
            ctx.reply(json!({"status": "resolved", "prompt_id": "prompt-1"}));
            ctx.event("approval.settled", prompt_marker("approval", "prompt-1"));
            ctx.event(
                "message.complete",
                json!({"text": "", "status": "complete"}),
            );
            true
        }
        _ => false,
    });
    let mut rig = Rig::new(script).await;
    let scratch = scratch_dir_for_driver();
    let result = drive(
        &mut rig,
        "plain-answer",
        scratch.path(),
        Duration::from_secs(60),
    )
    .await;
    assert!(
        matches!(result, AttemptResult::Failed(_)),
        "an approval nobody expected fails the attempt, got {result:?}"
    );

    let answers = rig.gateway.calls("approval.respond");
    assert!(!answers.is_empty(), "the approval was never answered");
    for answer in &answers {
        assert_eq!(answer.params()["choice"], json!("deny"));
        assert_eq!(answer.params()["prompt_id"], json!("prompt-1"));
        assert_eq!(answer.params()["session_id"], json!(SESSION_ID));
        assert_eq!(answer.params()["execution_generation"], json!(1));
    }
    let methods = rig.gateway.methods();
    assert_eq!(methods.last().map(String::as_str), Some("session.detach"));
    assert_clean_requests(&rig.gateway);
}

#[tokio::test]
async fn submit_params_only_allowed_keys_never_unattended() {
    let mut submits = 0;
    let script: Script = Box::new(move |seen, ctx| {
        if seen.method() != Some("prompt.submit") {
            return false;
        }
        submits += 1;
        if submits == 1 {
            ctx.reply(admission(1, "started"));
            ctx.event("message.start", json!({}));
            ctx.event("message.delta", json!({"text": "1\n"}));
        } else {
            ctx.reply(admission(2, "queued"));
            ctx.event(
                "message.complete",
                json!({"text": "", "status": "complete"}),
            );
            ctx.event("message.start", json!({}));
            ctx.event("message.delta", json!({"text": "second"}));
            ctx.event(
                "message.complete",
                json!({"text": "second", "status": "complete"}),
            );
        }
        true
    });
    let mut rig = Rig::new(script).await;
    let scratch = scratch_dir_for_driver();
    let result = drive(
        &mut rig,
        "queued-prompt",
        scratch.path(),
        Duration::from_secs(60),
    )
    .await;
    assert_eq!(result, AttemptResult::Ok);

    let expected = scenario("queued-prompt");
    let submits = rig.gateway.calls("prompt.submit");
    assert_eq!(submits.len(), 2, "two prompts, two submissions");
    let mut ids = Vec::new();
    for (submit, prompt) in submits.iter().zip(&expected.prompts) {
        assert_eq!(
            submit.param_keys(),
            ["session_id", "submission_id", "text"],
            "a prompt.submit carries exactly session_id, text and submission_id"
        );
        assert_eq!(submit.params()["session_id"], json!(SESSION_ID));
        assert_eq!(submit.params()["text"], json!(prompt));
        let id = submit.params()["submission_id"]
            .as_str()
            .unwrap_or_else(|| panic!("submission_id is not a string"))
            .to_owned();
        assert!(!id.is_empty());
        ids.push(id);
    }
    assert_ne!(ids[0], ids[1], "submission ids are unique");

    let created = rig.gateway.calls("session.create");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].params()["source"], json!("cli"));
    assert_eq!(created[0].params()["model"], json!(MODEL));
    assert_eq!(created[0].params()["provider"], json!(PROVIDER));
    let sent_cwd = created[0].params()["cwd"]
        .as_str()
        .unwrap_or_else(|| panic!("cwd missing"));
    assert_eq!(
        std::fs::canonicalize(sent_cwd).ok(),
        std::fs::canonicalize(scratch.path()).ok(),
        "session.create uses the scratch project"
    );
    assert_clean_requests(&rig.gateway);
}

#[tokio::test]
async fn expected_approval_is_answered_once_with_event_ids() {
    let script: Script = Box::new(|seen, ctx| match seen.method() {
        Some("prompt.submit") => {
            ctx.reply(admission(1, "started"));
            ctx.event("message.start", json!({}));
            ctx.event(
                "tool.start",
                json!({"tool_id": "t1", "name": "terminal", "args": {"command": terminal_command()}}),
            );
            ctx.event("approval.request", approval_request_payload("p-7"));
            true
        }
        Some("approval.respond") => {
            ctx.reply(json!({"status": "resolved", "prompt_id": "p-7"}));
            ctx.event("approval.settled", prompt_marker("approval", "p-7"));
            ctx.event(
                "tool.complete",
                json!({"tool_id": "t1", "name": "terminal", "args": {"command": terminal_command()},
                       "result_text": "", "summary": terminal_command()}),
            );
            ctx.event("message.delta", json!({"text": "exit code 0"}));
            ctx.event(
                "message.complete",
                json!({"text": "exit code 0", "status": "complete"}),
            );
            true
        }
        _ => false,
    });
    let mut rig = Rig::new(script).await;
    let scratch = scratch_dir_for_driver();
    let result = drive(
        &mut rig,
        "approval",
        scratch.path(),
        Duration::from_secs(60),
    )
    .await;
    assert_eq!(result, AttemptResult::Ok);

    let answers = rig.gateway.calls("approval.respond");
    assert_eq!(answers.len(), 1, "one approval, one answer");
    assert_eq!(
        answers[0].param_keys(),
        ["choice", "execution_generation", "prompt_id", "session_id"]
    );
    assert_eq!(answers[0].params()["choice"], json!("once"));
    assert_eq!(answers[0].params()["prompt_id"], json!("p-7"));
    assert_eq!(answers[0].params()["session_id"], json!(SESSION_ID));
    assert_eq!(answers[0].params()["execution_generation"], json!(1));
    let methods = rig.gateway.methods();
    assert_eq!(methods.last().map(String::as_str), Some("session.detach"));
    assert_eq!(
        count_of(&methods, "session.interrupt"),
        0,
        "no turn was running"
    );
    assert_clean_requests(&rig.gateway);
}

#[tokio::test]
async fn expected_clarification_is_answered_blue() {
    let script: Script = Box::new(|seen, ctx| match seen.method() {
        Some("prompt.submit") => {
            ctx.reply(admission(1, "started"));
            ctx.event("message.start", json!({}));
            ctx.event(
                "clarify.request",
                json!({"kind": "clarify", "prompt_id": "c-3", "execution_generation": 1,
                       "question": "Which colour?", "choices": ["red", "blue"],
                       "multi_select": false, "timeout_s": 300}),
            );
            true
        }
        Some("clarify.respond") => {
            ctx.reply(json!({"status": "resolved", "prompt_id": "c-3"}));
            ctx.event("clarify.settled", prompt_marker("clarify", "c-3"));
            ctx.event("message.delta", json!({"text": "blue"}));
            ctx.event(
                "message.complete",
                json!({"text": "blue", "status": "complete"}),
            );
            true
        }
        _ => false,
    });
    let mut rig = Rig::new(script).await;
    let scratch = scratch_dir_for_driver();
    let result = drive(
        &mut rig,
        "clarification",
        scratch.path(),
        Duration::from_secs(60),
    )
    .await;
    assert_eq!(result, AttemptResult::Ok);

    let answers = rig.gateway.calls("clarify.respond");
    assert_eq!(answers.len(), 1);
    assert_eq!(
        answers[0].param_keys(),
        ["answer", "execution_generation", "prompt_id", "session_id"]
    );
    assert_eq!(answers[0].params()["answer"], json!("blue"));
    assert_eq!(answers[0].params()["prompt_id"], json!("c-3"));
    assert_clean_requests(&rig.gateway);
}

#[tokio::test]
async fn interrupted_scenario_interrupts_after_first_delta() {
    let script: Script = Box::new(|seen, ctx| match seen.method() {
        Some("prompt.submit") => {
            ctx.reply(admission(1, "started"));
            ctx.event("message.start", json!({}));
            ctx.event("message.delta", json!({"text": "1\n"}));
            true
        }
        Some("session.interrupt") => {
            ctx.reply(json!({
                "ref": {"profile_id": "profile-1", "session_id": SESSION_ID},
                "instance_id": support::INSTANCE_ID,
                "authority_epoch": 3,
                "revision": 2,
                "execution_generation": 1,
                "execution_state": "idle"
            }));
            ctx.event(
                "message.complete",
                json!({"text": "1\n", "status": "interrupted"}),
            );
            true
        }
        _ => false,
    });
    let mut rig = Rig::new(script).await;
    let scratch = scratch_dir_for_driver();
    let result = drive(
        &mut rig,
        "interrupted",
        scratch.path(),
        Duration::from_secs(60),
    )
    .await;
    assert_eq!(result, AttemptResult::Ok);

    let interrupts = rig.gateway.calls("session.interrupt");
    assert_eq!(
        interrupts.len(),
        1,
        "one interrupt, not a second one in the cleanup"
    );
    assert_eq!(
        interrupts[0].param_keys(),
        ["execution_generation", "session_id"]
    );
    assert_eq!(interrupts[0].params()["execution_generation"], json!(1));
    let methods = rig.gateway.methods();
    assert!(
        index_of(&methods, "prompt.submit") < index_of(&methods, "session.interrupt"),
        "the interrupt follows the submission"
    );
    assert_eq!(methods.last().map(String::as_str), Some("session.detach"));
    assert_clean_requests(&rig.gateway);
}

#[tokio::test]
async fn failed_attempt_leaves_no_file() {
    // In process: a failed attempt (the snapshot shows another provider) writes nothing itself.
    let out = tempfile::tempdir().unwrap_or_else(|_| panic!("out dir failed"));
    let scratch = scratch_dir_for_driver();
    let before = dir_names(scratch.path());
    let mut rig = Rig::new(wrong_provider_script()).await;
    let result = drive(
        &mut rig,
        "plain-answer",
        scratch.path(),
        Duration::from_secs(60),
    )
    .await;
    assert!(
        matches!(result, AttemptResult::Failed(_)),
        "a snapshot that shows another provider fails the attempt, got {result:?}"
    );
    assert!(dir_names(out.path()).is_empty(), "the driver wrote a file");
    assert_eq!(
        dir_names(scratch.path()),
        before,
        "the driver changed the scratch project"
    );
    assert_eq!(
        rig.gateway.calls("prompt.submit").len(),
        0,
        "no prompt after a bad snapshot"
    );
    assert_eq!(
        rig.gateway.methods().last().map(String::as_str),
        Some("session.detach"),
        "the attempt still ends with a detach"
    );

    // Through the binary: a scenario that never succeeds is exit 1 and leaves nothing behind,
    // not even a hidden temporary file.
    if gethostname::gethostname().len() < 4 {
        println!("note: host name under four characters; binary part not run");
        return;
    }
    let login = format!("login{}", &random_hex()[..6]);
    let home = MockHome::start(HomeSpec::good(), wrong_provider_script()).await;
    let stub = StubHermes::new();
    let out = tempfile::tempdir().unwrap_or_else(|_| panic!("out dir failed"));
    let user_home = tempfile::tempdir().unwrap_or_else(|_| panic!("user home failed"));
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_record-fixture"))
        .args(["record", "--scenario", "plain-answer", "--attempts", "1"])
        .args(["--attempt-timeout-s", "5", "--profile", "fixture"])
        .args(["--model", MODEL, "--provider", PROVIDER])
        .arg("--home")
        .arg(home.path())
        .arg("--out")
        .arg(out.path())
        .env("PATH", stub.path_with_stub())
        .env("HOME", user_home.path())
        .env("USER", &login)
        .env("LOGNAME", &login)
        .kill_on_drop(true)
        .output()
        .await
        .unwrap_or_else(|_| panic!("the binary did not start"));
    assert_eq!(
        output.status.code(),
        Some(1),
        "a scenario that never succeeds is exit 1"
    );
    assert!(
        dir_names(out.path()).is_empty(),
        "a failed scenario left a file behind"
    );
    assert!(!stub.was_run(), "the stub hermes ran");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for forbidden in [
        home.path().to_string_lossy().into_owned(),
        user_home.path().to_string_lossy().into_owned(),
        login,
        "hermes-gateway-ticket".to_owned(),
    ] {
        assert!(
            !combined.contains(&forbidden),
            "the output holds a private value"
        );
    }
}

#[tokio::test]
async fn approval_guard_without_card_is_exit_2() {
    // The turn ends with a completed shell call for the delete and no approval card.
    let script = submit_then(vec![
        ("message.start", json!({})),
        (
            "tool.start",
            json!({"tool_id": "t1", "name": "terminal", "context": terminal_command(),
                   "args": {"command": terminal_command()}}),
        ),
        (
            "tool.complete",
            json!({"tool_id": "t1", "name": "terminal", "args": {"command": terminal_command()},
                   "result_text": "", "summary": terminal_command(), "duration_s": 0.1}),
        ),
        ("message.delta", json!({"text": "The exit code was 0."})),
        (
            "message.complete",
            json!({"text": "The exit code was 0.", "status": "complete"}),
        ),
    ]);
    let mut rig = Rig::new(script).await;
    let scratch = scratch_dir_for_driver();
    let result = drive(
        &mut rig,
        "approval",
        scratch.path(),
        Duration::from_secs(60),
    )
    .await;
    match result {
        AttemptResult::PreconditionAbsent(message) => assert!(
            message.contains(
                "no approval card appeared; the profile `fixture` must set `approvals.mode: manual`"
            ),
            "the message tells the operator what to set"
        ),
        other => panic!("expected PreconditionAbsent, got {other:?}"),
    }
    let methods = rig.gateway.methods();
    assert_eq!(
        count_of(&methods, "approval.respond"),
        0,
        "there was no card to answer"
    );
    assert_eq!(methods.last().map(String::as_str), Some("session.detach"));
    assert_clean_requests(&rig.gateway);
}

#[tokio::test]
async fn attempt_ends_with_deny_then_detach() {
    // Case 1: an unexpected approval is pending and the turn is still running when the attempt
    // fails. The order is: deny, then interrupt (a turn runs), then detach. Never interrupt first.
    let script: Script = Box::new(|seen, ctx| match seen.method() {
        Some("prompt.submit") => {
            ctx.reply(admission(1, "started"));
            ctx.event("message.start", json!({}));
            ctx.event("message.delta", json!({"text": "po"}));
            ctx.event("approval.request", approval_request_payload("p-9"));
            true
        }
        Some("approval.respond") => {
            // Resolved, but the gateway never settles it and the turn keeps running.
            ctx.reply(json!({"status": "resolved", "prompt_id": "p-9"}));
            true
        }
        _ => false,
    });
    let mut rig = Rig::new(script).await;
    let scratch = scratch_dir_for_driver();
    let result = drive(
        &mut rig,
        "plain-answer",
        scratch.path(),
        Duration::from_secs(60),
    )
    .await;
    assert!(matches!(result, AttemptResult::Failed(_)), "got {result:?}");
    for answer in rig.gateway.calls("approval.respond") {
        assert_eq!(answer.params()["choice"], json!("deny"));
    }
    let methods = rig.gateway.methods();
    let last_deny = last_index_of(&methods, "approval.respond").expect("a deny was sent");
    let interrupt = index_of(&methods, "session.interrupt").expect("a running turn is interrupted");
    let detach = index_of(&methods, "session.detach").expect("the session is detached");
    assert!(last_deny < interrupt, "deny before interrupt: {methods:?}");
    assert!(interrupt < detach, "interrupt before detach: {methods:?}");
    assert_eq!(count_of(&methods, "session.interrupt"), 1);
    assert_eq!(count_of(&methods, "session.detach"), 1);
    assert_eq!(detach, methods.len() - 1, "detach is the last request");
    assert_clean_requests(&rig.gateway);

    // Case 2: nothing pending and no turn running: no interrupt, only the detach.
    let script = submit_then(vec![
        ("message.start", json!({})),
        ("message.delta", json!({"text": "pong"})),
        (
            "message.complete",
            json!({"text": "pong", "status": "complete"}),
        ),
    ]);
    let mut rig = Rig::new(script).await;
    let result = drive(
        &mut rig,
        "plain-answer",
        scratch.path(),
        Duration::from_secs(60),
    )
    .await;
    assert_eq!(result, AttemptResult::Ok);
    let methods = rig.gateway.methods();
    assert_eq!(
        count_of(&methods, "session.interrupt"),
        0,
        "no turn was running: {methods:?}"
    );
    assert_eq!(count_of(&methods, "approval.respond"), 0);
    assert_eq!(methods.last().map(String::as_str), Some("session.detach"));
}

#[tokio::test]
async fn attempt_timeout_fails_attempt() {
    // The turn starts and never completes. A short real timeout is used: with paused
    // time, waiting on loopback socket I/O auto-advances to the timer before the mock replies.
    let script = submit_then(vec![
        ("message.start", json!({})),
        ("message.delta", json!({"text": "p"})),
    ]);
    let mut rig = Rig::new(script).await;
    let scratch = scratch_dir_for_driver();
    let result = drive(
        &mut rig,
        "plain-answer",
        scratch.path(),
        Duration::from_millis(50),
    )
    .await;
    assert!(
        matches!(result, AttemptResult::Failed(_)),
        "an attempt that outlives its timeout fails, got {result:?}"
    );
    let methods = rig.gateway.methods();
    let interrupt =
        index_of(&methods, "session.interrupt").expect("the running turn is interrupted");
    let detach = index_of(&methods, "session.detach").expect("the session is detached");
    assert!(interrupt < detach, "interrupt before detach: {methods:?}");
    assert_eq!(detach, methods.len() - 1, "detach is the last request");
    assert_clean_requests(&rig.gateway);
}
