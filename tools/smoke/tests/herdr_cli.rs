//! `HerdrCli` against the fake `herdr` executable: the exact argument lists of the recorded
//! table, the ids read from the recorded answers and the mapping of errors.

mod support;

use std::path::Path;

use smoke::{HerdrCli, NewTab, PaneId, ReadSource, TabId, TermError, Terminal};
use support::{
    FailBuilder, FakeHerdr, ScriptBuilder, WORKSPACE_ID, first_pane, second_pane, tab_id,
};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

fn cli(fake: &FakeHerdr) -> HerdrCli {
    HerdrCli::new(fake.executable(), WORKSPACE_ID)
}

fn text(path: &Path) -> &str {
    path.to_str().expect("the temporary folder is valid text")
}

/// A fake with a tab already created through the cli; the log then holds that one call.
fn with_tab(script: &ScriptBuilder) -> (FakeHerdr, HerdrCli, tempfile::TempDir) {
    let fake = FakeHerdr::new(script);
    let client = cli(&fake);
    let cwd = tempfile::tempdir().expect("temporary working folder");
    client
        .create_tab("talaria-smoke-abc12345", cwd.path())
        .expect("create the tab");
    (fake, client, cwd)
}

#[test]
fn create_tab_arguments_and_ids() {
    let fake = FakeHerdr::plain();
    let client = cli(&fake);
    let cwd = tempfile::tempdir().expect("temporary working folder");

    let created = client
        .create_tab("talaria-smoke-abc12345", cwd.path())
        .expect("create the tab");

    assert_eq!(
        created,
        NewTab {
            tab: tab_id(),
            pane: first_pane()
        },
        "the ids come from .result.tab.tab_id and .result.root_pane.pane_id"
    );
    assert_eq!(
        fake.log(),
        vec![args(&[
            "tab",
            "create",
            "--workspace",
            "w1",
            "--cwd",
            text(cwd.path()),
            "--label",
            "talaria-smoke-abc12345",
            "--no-focus",
        ])]
    );
}

#[test]
fn run_and_send_text_one_argument() {
    let (fake, client, _cwd) = with_tab(&ScriptBuilder::new());
    let pane = first_pane();
    let line = "echo 'two words'  \"quoted\" ; echo done $HOME";

    client.run(&pane, line).expect("run");
    client.send_text(&pane, "1").expect("send text");
    client
        .send_text(&pane, "one two  three")
        .expect("send text with spaces");

    let log = fake.log();
    assert_eq!(
        &log[1..],
        &[
            args(&["pane", "run", "w1:p1G", line]),
            args(&["pane", "send-text", "w1:p1G", "1"]),
            args(&["pane", "send-text", "w1:p1G", "one two  three"]),
        ],
        "the text goes as exactly one argument, with its spaces and quotes untouched"
    );
}

#[test]
fn send_keys_passes_each_key_as_an_argument() {
    let (fake, client, _cwd) = with_tab(&ScriptBuilder::new());
    let pane = first_pane();

    client.send_keys(&pane, &["esc"]).expect("send esc");
    client
        .send_keys(&pane, &["ctrl+c", "enter"])
        .expect("send two keys");

    assert_eq!(
        &fake.log()[1..],
        &[
            args(&["pane", "send-keys", "w1:p1G", "esc"]),
            args(&["pane", "send-keys", "w1:p1G", "ctrl+c", "enter"]),
        ]
    );
}

#[test]
fn read_passes_source_and_lines() {
    let script = ScriptBuilder::new().first_initial("alpha line\nbeta line\ngamma line");
    let (fake, client, _cwd) = with_tab(&script);
    let pane = first_pane();

    let visible = client
        .read(&pane, ReadSource::Visible, None)
        .expect("read visible");
    let unwrapped = client
        .read(&pane, ReadSource::RecentUnwrapped, Some(3000))
        .expect("read recent-unwrapped");
    let last_two = client
        .read(&pane, ReadSource::Recent, Some(2))
        .expect("read recent with a line limit");

    assert_eq!(
        visible.lines().collect::<Vec<_>>(),
        ["alpha line", "beta line", "gamma line"],
        "the plain text of the pane comes back as is"
    );
    assert_eq!(unwrapped.lines().count(), 3);
    assert_eq!(
        last_two.lines().collect::<Vec<_>>(),
        ["beta line", "gamma line"],
        "the line limit reaches the pane read"
    );
    assert_eq!(
        &fake.log()[1..],
        &[
            args(&["pane", "read", "w1:p1G", "--source", "visible"]),
            args(&[
                "pane",
                "read",
                "w1:p1G",
                "--source",
                "recent-unwrapped",
                "--lines",
                "3000"
            ]),
            args(&[
                "pane", "read", "w1:p1G", "--source", "recent", "--lines", "2"
            ]),
        ]
    );
}

#[test]
fn foreground_returns_command_lines() {
    let script = ScriptBuilder::new().first_foreground(&["-bash", "talaria-bin --profile fixture"]);
    let (fake, client, _cwd) = with_tab(&script);

    let lines = client.foreground(&first_pane()).expect("process info");

    assert_eq!(lines, ["-bash", "talaria-bin --profile fixture"]);
    assert_eq!(
        &fake.log()[1..],
        &[args(&["pane", "process-info", "--pane", "w1:p1G"])]
    );

    let (_fake, client, _cwd) = with_tab(&ScriptBuilder::new());
    assert_eq!(
        client.foreground(&first_pane()).expect("process info"),
        ["-bash"],
        "a quiet prompt lists the shell alone"
    );
}

#[test]
fn split_pane_right_no_focus() {
    let (fake, client, cwd) = with_tab(&ScriptBuilder::new());

    let second = client
        .split_pane(&first_pane(), cwd.path())
        .expect("split the pane");

    assert_eq!(
        second,
        second_pane(),
        "the id is read from .result.pane.pane_id"
    );
    assert_eq!(
        &fake.log()[1..],
        &[args(&[
            "pane",
            "split",
            "w1:p1G",
            "--direction",
            "right",
            "--no-focus",
            "--cwd",
            text(cwd.path()),
        ])]
    );
}

#[test]
fn tab_exists_and_close_follow_tab_list() {
    let fake = FakeHerdr::plain();
    let client = cli(&fake);
    let cwd = tempfile::tempdir().expect("temporary working folder");
    let tab = tab_id();

    assert!(
        !client.tab_exists(&tab).expect("tab list before"),
        "the recorded list without the tab does not hold it"
    );
    client
        .create_tab("talaria-smoke-abc12345", cwd.path())
        .expect("create the tab");
    assert!(
        client.tab_exists(&tab).expect("tab list with the tab"),
        "the recorded list with the tab holds it"
    );
    assert!(
        client
            .tab_exists(&TabId("w1:t1".to_owned()))
            .expect("tab list for another tab"),
        "every tab of the list counts, not only the new one"
    );
    assert!(
        !client
            .tab_exists(&TabId("w1:t9".to_owned()))
            .expect("tab list for a tab that is not there"),
        "a tab that is not in the list does not exist"
    );

    client.close_tab(&tab).expect("close the tab");
    assert!(
        !client.tab_exists(&tab).expect("tab list after the close"),
        "the closed tab is gone from the list"
    );

    let list = args(&["tab", "list", "--workspace", "w1"]);
    let log = fake.log();
    assert_eq!(log.len(), 7, "one herdr call per client call, got {log:?}");
    assert_eq!(log[0], list);
    assert_eq!(log[1][..2], args(&["tab", "create"])[..]);
    assert_eq!(log[2], list);
    assert_eq!(log[3], list);
    assert_eq!(log[4], list);
    assert_eq!(log[5], args(&["tab", "close", "w1:t5"]));
    assert_eq!(log[6], list);
}

#[test]
fn version_runs_herdr_version_flag() {
    let fake = FakeHerdr::plain();
    let client = cli(&fake);

    let version = client.version();

    let recorded = include_str!("fixtures/herdr/version.txt");
    assert_eq!(version.trim(), recorded.trim());
    assert!(
        version.starts_with("herdr "),
        "the answer is the text of herdr --version, got {version:?}"
    );
    assert_eq!(fake.log(), vec![args(&["--version"])]);
}

#[test]
fn error_json_becomes_term_error() {
    let (_fake, client, _cwd) = with_tab(&ScriptBuilder::new());
    let pane = first_pane();

    assert_eq!(
        client.send_keys(&pane, &["bogus-key"]),
        Err(TermError::Herdr {
            code: "invalid_key".to_owned(),
            message: "unsupported key bogus-key".to_owned(),
        }),
        "exit 1 with JSON on stderr carries its code and message"
    );
    assert_eq!(
        client.read(&PaneId("w1:p9Z".to_owned()), ReadSource::Visible, None),
        Err(TermError::Herdr {
            code: "pane_not_found".to_owned(),
            message: "pane w1:p9Z not found".to_owned(),
        })
    );
    assert_eq!(
        client.close_tab(&TabId("w1:t9".to_owned())),
        Err(TermError::Herdr {
            code: "tab_not_found".to_owned(),
            message: "tab w1:t9 not found".to_owned(),
        })
    );

    let planned = ScriptBuilder::new()
        .fail(FailBuilder::call("pane read", "pane_not_found").message("pane went away"));
    let (_fake, client, _cwd) = with_tab(&planned);
    assert_eq!(
        client.read(&first_pane(), ReadSource::Visible, None),
        Err(TermError::Herdr {
            code: "pane_not_found".to_owned(),
            message: "pane went away".to_owned(),
        }),
        "a planned error answer arrives the same way"
    );
}

#[test]
fn missing_herdr_is_a_clear_error() {
    let dir = tempfile::tempdir().expect("temporary folder");
    let missing = dir.path().join("no-such-herdr");
    let client = HerdrCli::new(&missing, WORKSPACE_ID);
    let pane = first_pane();
    let tab = tab_id();

    let answers = [
        (
            "create_tab",
            client.create_tab("label", dir.path()).map(|_| ()),
        ),
        (
            "split_pane",
            client.split_pane(&pane, dir.path()).map(|_| ()),
        ),
        ("run", client.run(&pane, "echo hi")),
        ("send_text", client.send_text(&pane, "1")),
        ("send_keys", client.send_keys(&pane, &["esc"])),
        (
            "read",
            client.read(&pane, ReadSource::Visible, None).map(|_| ()),
        ),
        ("foreground", client.foreground(&pane).map(|_| ())),
        ("tab_exists", client.tab_exists(&tab).map(|_| ())),
        ("close_tab", client.close_tab(&tab)),
    ];
    for (name, answer) in answers {
        match answer {
            Err(TermError::NotFound(message)) => assert!(
                message.contains("no-such-herdr"),
                "{name}: the message must name the executable that is missing, got {message:?}"
            ),
            other => panic!("{name}: a missing executable must be NotFound, got {other:?}"),
        }
    }
}
