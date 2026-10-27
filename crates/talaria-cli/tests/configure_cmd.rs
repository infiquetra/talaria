#[path = "../../talaria-app/tests/support/mock_gateway.rs"]
mod mock_gateway;

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use mock_gateway::{Behavior, Fixture, StandaloneGateway, fixture_event};
use serde_json::{Value, json};

fn run(home: &Path, args: &[&str]) -> Output {
    let child = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(args)
        .env("HERMES_HOME", home)
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start command");
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    rx.recv_timeout(Duration::from_secs(15))
        .unwrap_or_else(|_| {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            panic!("command did not exit")
        })
        .expect("read command output")
}

fn gateway(home: &Path, reply: &str) -> StandaloneGateway {
    let gateway = StandaloneGateway::start(home);
    gateway.enable_configure_rest();
    gateway.configure_session();
    let mut complete = fixture_event(Fixture::PlainAnswer, "message.complete");
    complete["params"]["payload"]["status"] = json!("complete");
    complete["params"]["payload"]["text"] = json!(reply);
    gateway.replay_after("prompt.submit", vec![complete]);
    gateway
}

fn object(output: &Output, expected: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).expect("JSON result")
}

#[test]
fn dry_run_args_exit_codes_and_json() {
    use talaria_cli::configure_cmd::is_command;
    let args = |values: &[&str]| values.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert!(is_command(&args(&["--profile", "fixture", "configure"])));
    assert!(is_command(&args(&[
        "--provider=example",
        "--model=sample",
        "configure"
    ])));
    assert!(!is_command(&args(&["-p", "configure"])));
    assert!(!is_command(&args(&["--profile", "configure"])));
    let home = tempfile::tempdir().expect("temporary folder");
    let out = run(home.path(), &["configure", "make it compact"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim(),
        "Applying needs the terminal UI: run talaria and type /configure."
    );
    for args in [
        vec!["configure", "--dry-run"],
        vec!["--provider", "one", "configure", "--dry-run", "test"],
        vec!["--model", "one", "configure", "--dry-run", "test"],
    ] {
        let out = run(home.path(), &args);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }

    let failure = object(
        &run(home.path(), &["configure", "--dry-run", "--json", "test"]),
        1,
    );
    assert_eq!(failure["version"], 1);
    assert_eq!(failure["exit"], 1);
    assert!(failure["error"].as_str().is_some_and(|s| !s.is_empty()));

    let cases = [
        (
            "```talaria-configure\n{\"kind\":\"proposal\",\"summary\":\"Bell\",\"changes\":[{\"op\":\"set\",\"key\":\"display.bell_on_complete\",\"value\":true}]}\n```",
            0,
            "proposal",
            "ready",
        ),
        (
            "```talaria-configure\n{\"kind\":\"proposal\",\"summary\":\"Unknown\",\"changes\":[{\"op\":\"set\",\"key\":\"unknown.setting\",\"value\":true}]}\n```",
            3,
            "proposal",
            "invalid",
        ),
        (
            "```talaria-configure\n{\"kind\":\"answer\",\"text\":\"Nothing to change\"}\n```",
            3,
            "answer",
            "",
        ),
        (
            "```talaria-configure\n{\"kind\":\"cannot\",\"reason\":\"Not supported\"}\n```",
            3,
            "cannot",
            "",
        ),
        ("No proposal", 4, "no_block", ""),
        ("```talaria-configure\n{\n```", 4, "unreadable", ""),
    ];
    for (reply, exit, kind, status) in cases {
        let temp = tempfile::tempdir().expect("temporary folder");
        let server = gateway(temp.path(), reply);
        let out = run(
            temp.path(),
            &[
                "--profile",
                "fixture",
                "--provider",
                "example",
                "--model",
                "sample",
                "configure",
                "--dry-run",
                "--json",
                "enable",
                "bell",
            ],
        );
        let result = object(&out, exit);
        assert_eq!(result["version"], 1);
        assert_eq!(result["profile"], "fixture");
        assert_eq!(result["model"], "example/sample");
        assert_eq!(result["kind"], kind);
        assert!(result["summary"].is_string());
        assert_eq!(result["context_rounds"], 0);
        let rows = result["rows"].as_array().expect("rows");
        if status.is_empty() {
            assert!(rows.is_empty());
        } else {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0]["op"], "set");
            assert_eq!(rows[0]["status"], status);
            if status == "ready" {
                assert_eq!(rows[0]["reason"], Value::Null);
            } else {
                assert!(rows[0]["reason"].is_string());
            }
            assert_eq!(rows[0]["destructive"], false);
            assert!(rows[0]["text"].is_string());
        }
        let creates = server.requests("session.create");
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].params["provider"], "example");
        assert_eq!(creates[0].params["model"], "sample");
    }

    let temp = tempfile::tempdir().expect("temporary folder");
    let server = gateway(temp.path(), "ignored");
    server.set_behavior("session.create", Behavior::Refuse("failed".into()));
    let error = object(
        &run(
            temp.path(),
            &[
                "--profile",
                "fixture",
                "configure",
                "--dry-run",
                "--json",
                "test",
            ],
        ),
        1,
    );
    assert_eq!(error["exit"], 1);
    assert!(error["error"].is_string());

    let temp = tempfile::tempdir().expect("temporary folder");
    let config = temp.path().join(".talaria");
    std::fs::create_dir(&config).expect("configuration folder");
    std::fs::write(
        config.join("config.toml"),
        "[configure]\nprovider = \"saved\"\nmodel = \"saved-model\"\n",
    )
    .expect("configuration");
    let server = gateway(
        temp.path(),
        "```talaria-configure\n{\"kind\":\"answer\",\"text\":\"Done\"}\n```",
    );
    let output = run(
        temp.path(),
        &[
            "--profile",
            "fixture",
            "configure",
            "--dry-run",
            "--json",
            "test",
        ],
    );
    assert_eq!(object(&output, 3)["model"], "saved/saved-model");
    let create = server.requests("session.create");
    assert_eq!(create[0].params["provider"], "saved");
    assert_eq!(create[0].params["model"], "saved-model");
    let output = run(
        temp.path(),
        &[
            "--profile",
            "fixture",
            "--provider",
            "chosen",
            "--model",
            "chosen-model",
            "configure",
            "--dry-run",
            "--json",
            "test",
        ],
    );
    assert_eq!(object(&output, 3)["model"], "chosen/chosen-model");
    let create = server.requests("session.create");
    assert_eq!(create[1].params["provider"], "chosen");
    assert_eq!(create[1].params["model"], "chosen-model");

    let temp = tempfile::tempdir().expect("temporary folder");
    let server = gateway(temp.path(), "ignored");
    server.replay_after("prompt.submit", Vec::new());
    let child = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args([
            "--profile",
            "fixture",
            "configure",
            "--dry-run",
            "--json",
            "test",
        ])
        .env("HERMES_HOME", temp.path())
        .env("HOME", temp.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start command");
    server
        .wait_request("prompt.submit", 0, Duration::from_secs(10))
        .expect("submit");
    assert!(
        Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .expect("signal")
            .success()
    );
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let output = rx
        .recv_timeout(Duration::from_secs(15))
        .expect("interrupted command exited")
        .expect("output");
    assert_eq!(
        object(&output, 130),
        json!({"version": 1, "error": "interrupted", "exit": 130})
    );
    assert_eq!(server.requests("session.interrupt").len(), 1);
    assert_eq!(server.requests("session.detach").len(), 1);
}

#[test]
fn dry_run_never_writes() {
    let temp = tempfile::tempdir().expect("temporary folder");
    let server = gateway(
        temp.path(),
        "```talaria-configure\n{\"kind\":\"proposal\",\"summary\":\"Bell\",\"changes\":[{\"op\":\"set\",\"key\":\"display.bell_on_complete\",\"value\":true}]}\n```",
    );
    server.replay_after("prompt.submit", Vec::new());
    let child = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args([
            "--profile",
            "fixture",
            "configure",
            "--dry-run",
            "--json",
            "turn on bell",
        ])
        .env("HERMES_HOME", temp.path())
        .env("HOME", temp.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start command");
    server
        .wait_request("prompt.submit", 0, Duration::from_secs(10))
        .expect("submit");
    server.block_on(server.clarify_request("question-1", 1, "Which one?", &["first"], false));
    let response = server
        .wait_request("clarify.respond", 0, Duration::from_secs(10))
        .expect("clarify response");
    assert_eq!(response.params["answer"], "");
    server.block_on(server.send_event("message.complete", json!({"status": "complete", "text": "```talaria-configure\n{\"kind\":\"proposal\",\"summary\":\"Bell\",\"changes\":[{\"op\":\"set\",\"key\":\"display.bell_on_complete\",\"value\":true}]}\n```"})));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let out = rx
        .recv_timeout(Duration::from_secs(15))
        .expect("command exited")
        .expect("output");
    let result = object(&out, 0);
    assert_eq!(result["rows"][0]["status"], "ready");
    let requests = server.configure_rest_requests();
    for target in [
        "/api/config/schema",
        "/api/config/defaults",
        "/api/config",
        "/api/config?include_defaults=false",
    ] {
        assert!(
            requests
                .iter()
                .any(|(method, path, _)| method == "GET" && path == target),
            "missing configuration read {target}"
        );
    }
    assert!(
        requests.iter().all(|(method, _, _)| method == "GET"),
        "dry run sent an HTTP write: {requests:?}"
    );
    for method in ["mcp.servers.add", "mcp.servers.remove"] {
        assert!(server.requests(method).is_empty(), "write method {method}");
    }
    for method in ["skills.manage", "plugins.manage", "cron.manage"] {
        for request in server.requests(method) {
            assert!(
                matches!(request.params["action"].as_str(), Some("list" | "browse")),
                "write method {method}"
            );
        }
    }
    assert_eq!(server.requests("session.detach").len(), 1);
}
