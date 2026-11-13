//! `HcCli` against the fake `hc` executable: the exact argument lists, recorded replies,
//! error mapping, key table, and inherited environment scrubbing.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use smoke::{HcCli, NewTab, PaneId, ReadSource, TabId, TermError, Terminal};

struct FakeHc {
    _root: tempfile::TempDir,
    state_dir: PathBuf,
    wrapper: PathBuf,
}

struct FakeHcBuilder {
    replies: BTreeMap<String, Vec<(&'static str, i32)>>,
}

impl FakeHcBuilder {
    fn new() -> Self {
        Self {
            replies: BTreeMap::new(),
        }
    }

    fn reply(mut self, command: &str, file: &'static str, status: i32) -> Self {
        self.replies
            .entry(command.to_owned())
            .or_default()
            .push((file, status));
        self
    }

    fn build(self) -> FakeHc {
        let root = tempfile::tempdir().expect("temporary folder for fake hc");
        let state_dir = root.path().join("state");
        fs::create_dir(&state_dir).expect("create fake hc state folder");

        let mut map = serde_json::Map::new();
        for (command, items) in self.replies {
            let entries: Vec<Value> = items
                .into_iter()
                .map(|(file, status)| json!({ "file": file, "status": status }))
                .collect();
            map.insert(command, Value::Array(entries));
        }
        fs::write(
            state_dir.join("replies.json"),
            Value::Object(map).to_string(),
        )
        .expect("write replies.json");

        let wrapper = root.path().join("hc");
        let fake_exe = PathBuf::from(env!("CARGO_BIN_EXE_fake-hc"));
        let body = format!(
            "#!/bin/sh\nFAKE_HC_DIR={} exec {} \"$@\"\n",
            shell_quote(&state_dir.to_string_lossy()),
            shell_quote(&fake_exe.to_string_lossy()),
        );
        write_executable(&wrapper, &body);

        FakeHc {
            _root: root,
            state_dir,
            wrapper,
        }
    }
}

impl FakeHc {
    fn builder() -> FakeHcBuilder {
        FakeHcBuilder::new()
    }

    fn executable(&self) -> &Path {
        &self.wrapper
    }

    fn log(&self) -> Vec<Vec<String>> {
        read_json_lines(&self.state_dir.join("log"))
    }

    fn env_log(&self) -> Vec<Vec<String>> {
        read_json_lines(&self.state_dir.join("env.log"))
    }
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write executable");
    let mut permissions = fs::metadata(path).expect("stat executable").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod executable");
}

fn read_json_lines(path: &Path) -> Vec<Vec<String>> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Vec<String>>(line).ok())
        .collect()
}

fn read_recording(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/support/hc_recordings")
        .join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read recording {name}: {error}"))
}

fn read_recording_json(name: &str) -> Value {
    let text = read_recording(name);
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("cannot parse recording {name} as JSON: {error}"))
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

fn text(path: &Path) -> &str {
    path.to_str().expect("the temporary folder is valid text")
}

fn expected_foreground(recording: &Value) -> Vec<String> {
    let argv: Vec<&str> = recording["foreground"]["argv"]
        .as_array()
        .expect("foreground.argv array")
        .iter()
        .map(|item| item.as_str().expect("argv string"))
        .collect();
    if argv.is_empty() {
        Vec::new()
    } else {
        vec![argv.join(" ")]
    }
}

#[test]
fn check_runs_hc_ls() {
    let fake = FakeHc::builder().reply("ls", "ls.json", 0).build();
    let client = HcCli::new(fake.executable());

    assert_eq!(client.check(), Ok(()));
    assert_eq!(fake.log(), vec![args(&["ls"])]);
}

#[test]
fn version_trims_and_is_empty_on_failure() {
    let fake = FakeHc::builder()
        .reply("--version", "version.txt", 0)
        .reply("--version", "keys_unknown.json", 6)
        .build();
    let client = HcCli::new(fake.executable());

    let expected = read_recording("version.txt");
    assert_eq!(client.version(), expected.trim());
    assert_eq!(client.version(), "");
    assert_eq!(fake.log(), vec![args(&["--version"]), args(&["--version"])]);
}

#[test]
fn send_keys_passes_each_key() {
    let fake = FakeHc::builder().reply("keys", "keys_ok.json", 0).build();
    let client = HcCli::new(fake.executable());
    let pane = PaneId("term_x".to_owned());

    client
        .send_keys(&pane, &["esc", "enter"])
        .expect("send_keys");

    assert_eq!(fake.log(), vec![args(&["keys", "term_x", "esc", "enter"])]);
}

#[test]
fn single_character_uses_keys_not_send() {
    let fake = FakeHc::builder()
        .reply("keys", "keys_blocked.json", 0)
        .build();
    let client = HcCli::new(fake.executable());
    let pane = PaneId("term_x".to_owned());

    client.send_text(&pane, "1").expect("send_text");

    let log = fake.log();
    assert_eq!(log, vec![args(&["keys", "term_x", "1"])]);
    assert!(
        !log.iter()
            .any(|entry| entry.first().is_some_and(|cmd| cmd == "send"))
    );
}

#[test]
fn read_uses_hc_read_without_a_source() {
    let fake = FakeHc::builder()
        .reply("read", "read_screen.txt", 0)
        .reply("read", "read_lines.txt", 0)
        .build();
    let client = HcCli::new(fake.executable());
    let pane = PaneId("term_x".to_owned());

    let screen = client
        .read(&pane, ReadSource::Visible, None)
        .expect("read visible");
    assert_eq!(screen, read_recording("read_screen.txt"));

    let recent = client
        .read(&pane, ReadSource::RecentUnwrapped, Some(5))
        .expect("read lines");
    assert_eq!(recent, read_recording("read_lines.txt"));

    let log = fake.log();
    assert_eq!(
        log,
        vec![
            args(&["read", "term_x"]),
            args(&["read", "term_x", "--lines", "5"]),
        ]
    );
    for entry in &log {
        assert!(
            !entry.iter().any(|arg| matches!(
                arg.as_str(),
                "--source" | "visible" | "recent" | "recent-unwrapped"
            )),
            "hc read must not pass --source or a source name: {entry:?}"
        );
    }
}

#[test]
fn close_runs_kill_then_rm() {
    let fake = FakeHc::builder()
        .reply("kill", "kill_live.json", 0)
        .reply("rm", "rm_finished.json", 0)
        .build();
    let client = HcCli::new(fake.executable());

    client
        .close_tab(&TabId("term_x".to_owned()))
        .expect("close_tab");

    assert_eq!(
        fake.log(),
        vec![args(&["kill", "term_x"]), args(&["rm", "term_x"])]
    );
}

#[test]
fn close_continues_after_a_failed_kill() {
    let fake = FakeHc::builder()
        .reply("run", "run_split.json", 0)
        .reply("kill", "get_removed.json", 3)
        .reply("kill", "get_removed.json", 3)
        .reply("rm", "rm_finished.json", 0)
        .reply("rm", "rm_finished.json", 0)
        .reply("rm", "rm_finished.json", 0)
        .build();
    let client = HcCli::new(fake.executable());
    let cwd = tempfile::tempdir().expect("temporary working folder");

    let split_addr = read_recording_json("run_split.json")["address"]
        .as_str()
        .expect("run_split.json address")
        .to_owned();
    let split = client
        .split_pane(&PaneId("term_x".to_owned()), cwd.path())
        .expect("split_pane");
    assert_eq!(split, PaneId(split_addr.clone()));

    let error = client
        .close_tab(&TabId("term_x".to_owned()))
        .expect_err("failed kill returns error");
    let expected_stderr = read_recording("get_removed.json").trim().to_owned();
    assert_eq!(error, TermError::Io(expected_stderr));

    assert_eq!(
        fake.log(),
        vec![
            args(&["run", "--bg", "--no-parent", "--cwd", text(cwd.path())]),
            args(&["kill", &split_addr]),
            args(&["rm", &split_addr]),
            args(&["kill", "term_x"]),
            args(&["rm", "term_x"]),
        ]
    );

    client
        .close_tab(&TabId("term_x".to_owned()))
        .expect("second close_tab");
    let log = fake.log();
    assert_eq!(
        &log[5..],
        &[args(&["kill", "term_x"]), args(&["rm", "term_x"])]
    );
    assert!(
        !log[5..].iter().any(|entry| entry.contains(&split_addr)),
        "second close_tab must not log split address {split_addr}: {log:?}"
    );
}

#[test]
fn create_tab_parses_the_json_address() {
    let fake = FakeHc::builder()
        .reply("run", "run_bg.json", 0)
        .reply("run", "version.txt", 0)
        .build();
    let client = HcCli::new(fake.executable());
    let cwd = tempfile::tempdir().expect("temporary working folder");

    let expected_addr = read_recording_json("run_bg.json")["address"]
        .as_str()
        .expect("run_bg.json address")
        .to_owned();
    let created = client
        .create_tab("talaria-smoke-abc", cwd.path())
        .expect("create_tab");
    assert_eq!(
        created,
        NewTab {
            tab: TabId(expected_addr.clone()),
            pane: PaneId(expected_addr),
        }
    );

    let second = client.create_tab("talaria-smoke-abc", cwd.path());
    assert!(
        matches!(second, Err(TermError::Parse(_))),
        "expected Parse error, got {second:?}"
    );

    let expected_call = args(&[
        "run",
        "--bg",
        "--no-parent",
        "--name",
        "talaria-smoke-abc",
        "--cwd",
        text(cwd.path()),
    ]);
    assert_eq!(fake.log(), vec![expected_call.clone(), expected_call]);
}

#[test]
fn split_pane_has_no_name() {
    let fake = FakeHc::builder().reply("run", "run_split.json", 0).build();
    let client = HcCli::new(fake.executable());
    let cwd = tempfile::tempdir().expect("temporary working folder");

    let expected_addr = read_recording_json("run_split.json")["address"]
        .as_str()
        .expect("run_split.json address")
        .to_owned();
    let pane = client
        .split_pane(&PaneId("term_x".to_owned()), cwd.path())
        .expect("split_pane");
    assert_eq!(pane, PaneId(expected_addr));

    let log = fake.log();
    assert_eq!(
        log,
        vec![args(&[
            "run",
            "--bg",
            "--no-parent",
            "--cwd",
            text(cwd.path()),
        ])]
    );
    assert!(!log[0].iter().any(|arg| arg == "--name"));
}

#[test]
fn foreground_and_pane_get_read_hc_get() {
    let fake = FakeHc::builder()
        .reply("get", "get_shell.json", 0)
        .reply("get", "get_shell.json", 0)
        .reply("get", "get_idle.json", 0)
        .reply("get", "get_idle.json", 0)
        .build();
    let client = HcCli::new(fake.executable());
    let pane = PaneId("term_x".to_owned());

    let shell = read_recording_json("get_shell.json");
    let idle = read_recording_json("get_idle.json");

    let fg_shell = client.foreground(&pane);
    assert_ne!(
        fg_shell,
        Err(TermError::Parse("pane get is not supported".to_owned()))
    );
    assert_eq!(
        fg_shell.expect("first foreground"),
        expected_foreground(&shell)
    );

    let pg_shell = client.pane_get(&pane);
    assert_ne!(
        pg_shell,
        Err(TermError::Parse("pane get is not supported".to_owned()))
    );
    assert_eq!(pg_shell.expect("first pane_get"), "\n");

    let fg_idle = client.foreground(&pane);
    assert_ne!(
        fg_idle,
        Err(TermError::Parse("pane get is not supported".to_owned()))
    );
    assert_eq!(
        fg_idle.expect("second foreground"),
        expected_foreground(&idle)
    );

    let pg_idle = client.pane_get(&pane);
    assert_ne!(
        pg_idle,
        Err(TermError::Parse("pane get is not supported".to_owned()))
    );
    let app = idle["status"]["app"].as_str().expect("status.app");
    let state = idle["status"]["state"].as_str().expect("status.state");
    assert_eq!(pg_idle.expect("second pane_get"), format!("{app}\n{state}"));

    let expected_call = args(&["get", "term_x", "--json"]);
    assert_eq!(
        fake.log(),
        vec![
            expected_call.clone(),
            expected_call.clone(),
            expected_call.clone(),
            expected_call,
        ]
    );
}

#[test]
fn missing_executable_is_not_found() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let missing = dir.path().join("no-such-hc");
    let missing_text = text(&missing);
    let client = HcCli::new(&missing);
    let pane = PaneId("term_x".to_owned());
    let tab = TabId("term_x".to_owned());

    let answers = [
        ("check", client.check()),
        ("send_keys", client.send_keys(&pane, &["esc"])),
        (
            "read",
            client.read(&pane, ReadSource::Visible, None).map(|_| ()),
        ),
        ("close_tab", client.close_tab(&tab)),
        (
            "create_tab",
            client.create_tab("label", dir.path()).map(|_| ()),
        ),
        (
            "split_pane",
            client.split_pane(&pane, dir.path()).map(|_| ()),
        ),
        ("run", client.run(&pane, "echo hi")),
        ("foreground", client.foreground(&pane).map(|_| ())),
        ("pane_get", client.pane_get(&pane).map(|_| ())),
    ];
    for (name, answer) in answers {
        match answer {
            Err(TermError::NotFound(message)) => assert!(
                message.contains(missing_text),
                "{name}: NotFound message must contain {missing_text:?}, got {message:?}"
            ),
            other => panic!("{name}: expected NotFound, got {other:?}"),
        }
    }
}

#[test]
fn tab_exists_reads_piped_ls() {
    let fake = FakeHc::builder()
        .reply("ls", "ls.json", 0)
        .reply("ls", "ls.json", 0)
        .reply("ls", "ls.json", 0)
        .reply("ls", "version.txt", 0)
        .build();
    let client = HcCli::new(fake.executable());

    let ls_doc = read_recording_json("ls.json");
    let live_addresses: Vec<String> = ls_doc["terminals"]
        .as_array()
        .expect("ls.json terminals array")
        .iter()
        .filter(|entry| entry["state"].as_str() == Some("live"))
        .map(|entry| {
            entry["address"]
                .as_str()
                .expect("live entry address")
                .to_owned()
        })
        .collect();
    assert!(!live_addresses.is_empty());

    for address in &live_addresses {
        assert_eq!(client.tab_exists(&TabId(address.clone())), Ok(true));
    }
    assert_eq!(client.tab_exists(&TabId("other".to_owned())), Ok(false));

    let invalid = client.tab_exists(&TabId("other".to_owned()));
    assert!(
        matches!(invalid, Err(TermError::Parse(_))),
        "expected Parse error, got {invalid:?}"
    );

    let log = fake.log();
    assert_eq!(log.len(), live_addresses.len() + 2);
    for entry in &log {
        assert_eq!(entry, &args(&["ls"]));
        assert!(!entry.iter().any(|arg| arg == "--json"));
    }
}

#[test]
fn run_sends_into_the_existing_terminal() {
    let fake = FakeHc::builder().reply("send", "send_idle.json", 0).build();
    let client = HcCli::new(fake.executable());
    let pane = PaneId("term_x".to_owned());

    client.run(&pane, "echo hi").expect("run");

    let log = fake.log();
    assert_eq!(log, vec![args(&["send", "term_x", "echo hi"])]);
    assert_eq!(log[0].len(), 3);
    assert!(!log[0].iter().any(|arg| arg == "run"));
}

#[test]
fn send_text_uses_the_key_table() {
    let fake = FakeHc::builder()
        .reply("keys", "keys_ok.json", 0)
        .reply("keys", "keys_ok.json", 0)
        .reply("keys", "keys_ok.json", 0)
        .build();
    let client = HcCli::new(fake.executable());
    let pane = PaneId("term_x".to_owned());

    client.send_text(&pane, "ab").expect("send ab");
    client.send_text(&pane, "a b").expect("send a b");
    client.send_text(&pane, "A").expect("send A");
    assert_eq!(client.send_text(&pane, ""), Ok(()));
    assert_eq!(
        client.send_text(&pane, "!"),
        Err(TermError::Parse("hc keys cannot type '!'".to_owned()))
    );

    assert_eq!(
        fake.log(),
        vec![
            args(&["keys", "term_x", "a", "b"]),
            args(&["keys", "term_x", "a", "space", "b"]),
            args(&["keys", "term_x", "shift+a"]),
        ]
    );
}

#[test]
fn child_drops_hc_identity() {
    let fake = FakeHc::builder().reply("ls", "ls.json", 0).build();

    let output = Command::new(env!("CARGO_BIN_EXE_hc-env-probe"))
        .arg(fake.executable())
        .env("HC_TERMINAL_ID", "term_x")
        .env("HC_SCOPE", "scope")
        .output()
        .expect("run hc-env-probe");
    assert!(
        output.status.success(),
        "hc-env-probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(fake.log(), vec![args(&["ls"])]);
    assert_eq!(fake.env_log(), vec![Vec::<String>::new()]);
}
