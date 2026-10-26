//! Command-line background launch. Step 0 exits 0 and writes nothing.

mod support;

use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use support::talaria;

fn invoke(home: &Path, args: &[&str], stdin: Option<&[u8]>, limit: Duration) -> Output {
    let mut command = talaria(args, home);
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().expect("spawn talaria");
    if let Some(bytes) = stdin {
        let owned = bytes.to_vec();
        let mut pipe = child.stdin.take().expect("stdin");
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = pipe.write_all(&owned);
        });
    }
    let mut stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        let _ = stderr.read_to_end(&mut err);
        (out, err)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break status;
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            break child.wait().expect("wait");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (out, err) = reader.join().unwrap_or_default();
    Output {
        status,
        stdout: out,
        stderr: err,
    }
}

fn usage(what: &str, next: &str) -> String {
    format!("talaria: {what}\nNext step: {next}\n")
}

fn help_usage(what: &str) -> String {
    usage(what, "Run talaria --help.")
}

struct Expect {
    name: &'static str,
    args: Vec<String>,
    stdin: Option<Vec<u8>>,
    code: i32,
    stderr: String,
    stdout_has: Option<String>,
}

fn check(home: &Path, expect: &Expect) {
    let args: Vec<&str> = expect.args.iter().map(String::as_str).collect();
    let output = invoke(home, &args, expect.stdin.as_deref(), Duration::from_secs(3));
    let code = output.status.code();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let code_ok = code == Some(expect.code);
    let err_ok = expect.stderr.is_empty() || stderr.contains(&expect.stderr);
    let out_ok = expect
        .stdout_has
        .as_ref()
        .is_none_or(|needle| stdout.contains(needle));
    assert!(
        code_ok && err_ok && out_ok,
        "{}: expected exit {} got {:?} stderr {:?} stdout {:?}",
        expect.name,
        expect.code,
        code,
        stderr.as_ref(),
        stdout.as_ref()
    );
}

fn prompt_and_flag_cases(prompt: &str) -> Vec<Expect> {
    vec![
        Expect {
            name: "no prompt",
            args: vec!["--bg".to_owned()],
            stdin: None,
            code: 2,
            stderr: help_usage(
                "talaria --bg needs a prompt: pass it as an argument or on standard input.",
            ),
            stdout_has: None,
        },
        Expect {
            name: "whitespace prompt",
            args: vec!["--bg".to_owned(), "   ".to_owned()],
            stdin: None,
            code: 2,
            stderr: help_usage(
                "talaria --bg needs a prompt: pass it as an argument or on standard input.",
            ),
            stdout_has: None,
        },
        Expect {
            name: "bg with print",
            args: vec!["--bg".to_owned(), "-p".to_owned(), prompt.to_owned()],
            stdin: None,
            code: 2,
            stderr: help_usage("talaria --bg cannot be combined with -p."),
            stdout_has: None,
        },
        Expect {
            name: "bg with continue",
            args: vec!["--bg".to_owned(), "-c".to_owned(), prompt.to_owned()],
            stdin: None,
            code: 2,
            stderr: help_usage(
                "talaria --bg starts a new session; it cannot be combined with -c or --resume.",
            ),
            stdout_has: None,
        },
        Expect {
            name: "bg with resume",
            args: vec!["--bg".to_owned(), "--resume".to_owned(), prompt.to_owned()],
            stdin: None,
            code: 2,
            stderr: help_usage(
                "talaria --bg starts a new session; it cannot be combined with -c or --resume.",
            ),
            stdout_has: None,
        },
        Expect {
            name: "worktree without bg",
            args: vec!["--worktree".to_owned(), prompt.to_owned()],
            stdin: None,
            code: 2,
            stderr: help_usage("talaria --worktree needs --bg."),
            stdout_has: None,
        },
        Expect {
            name: "name without worktree",
            args: vec![
                "--bg".to_owned(),
                "--name".to_owned(),
                "fix".to_owned(),
                prompt.to_owned(),
            ],
            stdin: None,
            code: 2,
            stderr: help_usage("talaria --name needs --worktree."),
            stdout_has: None,
        },
    ]
}

fn name_and_effort_cases(prompt: &str) -> Vec<Expect> {
    vec![
        Expect {
            name: "name dashes",
            args: vec![
                "--bg".to_owned(),
                "--worktree".to_owned(),
                "--name=--".to_owned(),
                prompt.to_owned(),
            ],
            stdin: None,
            code: 2,
            stderr: help_usage("talaria --name needs at least one ASCII letter or digit."),
            stdout_has: None,
        },
        Expect {
            name: "name marks",
            args: vec![
                "--bg".to_owned(),
                "--worktree".to_owned(),
                "--name".to_owned(),
                "!!".to_owned(),
                prompt.to_owned(),
            ],
            stdin: None,
            code: 2,
            stderr: help_usage("talaria --name needs at least one ASCII letter or digit."),
            stdout_has: None,
        },
        Expect {
            name: "name accent",
            args: vec![
                "--bg".to_owned(),
                "--worktree".to_owned(),
                "--name".to_owned(),
                "é".to_owned(),
                prompt.to_owned(),
            ],
            stdin: None,
            code: 2,
            stderr: help_usage("talaria --name needs at least one ASCII letter or digit."),
            stdout_has: None,
        },
        Expect {
            name: "name spaces",
            args: vec![
                "--bg".to_owned(),
                "--worktree".to_owned(),
                "--name".to_owned(),
                "   ".to_owned(),
                prompt.to_owned(),
            ],
            stdin: None,
            code: 2,
            stderr: help_usage("talaria --name needs at least one ASCII letter or digit."),
            stdout_has: None,
        },
        Expect {
            name: "timeout with bg",
            args: vec![
                "--bg".to_owned(),
                "--timeout".to_owned(),
                "5".to_owned(),
                prompt.to_owned(),
            ],
            stdin: None,
            code: 2,
            stderr: help_usage("--timeout only works with -p."),
            stdout_has: None,
        },
        Expect {
            name: "unknown effort",
            args: vec![
                "--bg".to_owned(),
                "--effort".to_owned(),
                "hgih".to_owned(),
                prompt.to_owned(),
            ],
            stdin: None,
            code: 2,
            stderr: usage(
                "--effort must be one of none, minimal, low, medium, high, xhigh, max, ultra (got hgih).",
                "Pick one of those values.",
            ),
            stdout_has: None,
        },
    ]
}

#[test]
fn usage_errors() {
    let home = tempfile::tempdir().expect("home");
    let prompt = "hello";
    let flags = prompt_and_flag_cases(prompt);
    let names = name_and_effort_cases(prompt);
    for case in flags.iter().chain(names.iter()) {
        check(home.path(), case);
    }

    let recording = home.path().join("taken-recording");
    std::fs::write(&recording, "taken").expect("recording");
    check(
        home.path(),
        &Expect {
            name: "record exists",
            args: vec![
                "--bg".to_owned(),
                "--record".to_owned(),
                "taken-recording".to_owned(),
                prompt.to_owned(),
            ],
            stdin: None,
            code: 1,
            stderr: usage(
                "the recording file already exists.",
                "Choose a new path for --record.",
            ),
            stdout_has: None,
        },
    );
    check(
        home.path(),
        &Expect {
            name: "blank model",
            args: vec![
                "--bg".to_owned(),
                "--model".to_owned(),
                String::new(),
                prompt.to_owned(),
            ],
            stdin: None,
            code: 2,
            stderr: "the value is empty".to_owned(),
            stdout_has: None,
        },
    );

    let mut big = vec![b'a'; 1_048_577];
    big.push(b'\n');
    check(
        home.path(),
        &Expect {
            name: "prompt over 1 MiB",
            args: vec!["--bg".to_owned()],
            stdin: Some(big),
            code: 2,
            stderr: help_usage("talaria --bg: the prompt is larger than 1 MiB."),
            stdout_has: None,
        },
    );
    check(
        home.path(),
        &Expect {
            name: "stdin not utf-8",
            args: vec!["--bg".to_owned()],
            stdin: Some(vec![0xff, 0xfe]),
            code: 2,
            stderr: help_usage("talaria --bg: standard input is not valid UTF-8."),
            stdout_has: None,
        },
    );
}

#[test]
fn exit_codes_and_messages_against_mock() {
    let home = tempfile::tempdir().expect("home");
    let gateway = mock_gateway_start(home.path());
    let cases = [
        (
            4001_i64,
            "session.create",
            4,
            "Hermes refused to create the session",
        ),
        (
            -32601,
            "session.create",
            4,
            "this Hermes gateway does not know session.create",
        ),
        (
            5001,
            "session.create",
            4,
            "Hermes did not confirm that the session was created",
        ),
        (
            5001,
            "prompt.submit",
            1,
            "the prompt may have started, but the reply from Hermes was lost",
        ),
    ];
    for (code, method, exit, text) in cases {
        gateway.set_behavior("session.create", Behavior::Normal);
        gateway.set_behavior("prompt.submit", Behavior::Normal);
        gateway.set_behavior(
            method,
            Behavior::FailCode {
                code,
                message: "refused".to_owned(),
                reason: Some("invalid_params".to_owned()),
            },
        );
        let output = invoke(
            home.path(),
            &[
                "--bg",
                "--profile",
                "fixture",
                "--output-format",
                "json",
                "hello",
            ],
            None,
            Duration::from_secs(3),
        );
        assert_eq!(
            output.status.code(),
            Some(exit),
            "{method} {code}: expected exit {exit}, got {:?}",
            output.status.code()
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(text),
            "{method} {code}: missing refusal text"
        );
    }
}

#[test]
fn prompt_from_standard_input() {
    let home = tempfile::tempdir().expect("home");
    let _gateway = mock_gateway_start(home.path());
    let output = invoke(
        home.path(),
        &["--bg", "--output-format", "json"],
        Some(b"hello from stdin"),
        Duration::from_secs(3),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"status\":\"launched\""),
        "stdout missing launched status"
    );
    let both = invoke(
        home.path(),
        &["--bg", "--output-format", "json", "from the argument"],
        Some(b"from stdin"),
        Duration::from_secs(3),
    );
    let text = String::from_utf8_lossy(&both.stdout);
    assert!(text.contains("\"status\":\"launched\""));
}

#[test]
fn signal_before_submit_rolls_back() {
    let home = tempfile::tempdir().expect("home");
    let gateway = mock_gateway_start(home.path());
    gateway.hold("session.create");
    let mut command = talaria(&["--bg", "--profile", "fixture", "hello"], home.path());
    let mut child = command.spawn().expect("spawn");
    assert!(
        gateway.wait_for_method("session.create").is_some(),
        "the create never reached the gateway"
    );
    let _ = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status();
    // Give the signal time to reach the command before the held reply is let go.
    std::thread::sleep(Duration::from_millis(100));
    gateway.release("session.create");
    let status = child.wait().expect("wait");
    assert_eq!(
        status.code(),
        Some(130),
        "expected exit 130, got {status:?}"
    );
    let detaches = gateway.requests("session.detach");
    let abandoned = detaches
        .iter()
        .any(|frame| frame.params["end_reason"] == "launch_abandoned");
    assert!(abandoned, "missing launch_abandoned detach");
    assert!(gateway.requests("prompt.submit").is_empty());
}

#[test]
fn launch_time_budget() {
    let home = tempfile::tempdir().expect("home");
    let _gateway = mock_gateway_start(home.path());
    let output = invoke(
        home.path(),
        &[
            "--bg",
            "--profile",
            "fixture",
            "--output-format",
            "json",
            "hello",
        ],
        None,
        Duration::from_secs(3),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"status\":\"launched\""),
        "stdout missing launched status"
    );
    let mut samples = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let _ = invoke(
            home.path(),
            &[
                "--bg",
                "--profile",
                "fixture",
                "--output-format",
                "json",
                "hello",
            ],
            None,
            Duration::from_secs(3),
        );
        samples.push(started.elapsed());
    }
    samples.sort();
    let p95 = samples[18];
    assert!(p95 <= Duration::from_millis(300), "p95 {p95:?}");
}

// The mock lives in the application crate's test support. This binary includes it by path so the
// command is pointed at a gateway that records frames.
#[path = "../../talaria-app/tests/support/mock_gateway.rs"]
mod gateway;

use gateway::{Behavior, StandaloneGateway};

fn mock_gateway_start(home: &Path) -> StandaloneGateway {
    StandaloneGateway::start(home)
}
