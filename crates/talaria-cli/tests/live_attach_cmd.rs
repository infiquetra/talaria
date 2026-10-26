//! Live attach by prefix and title against a running gateway. Issue #246.
//!
//! Ignored unless a person asks for it. A skip is not a pass: with
//! `TALARIA_LIVE_REQUIRE=1` a skip fails the test.
//!
//! Run by hand, one test at a time, with the gateway of the home named by
//! `HERMES_HOME`:
//! `TALARIA_LIVE_REQUIRE=1 cargo test --release -p talaria-cli --test live_attach_cmd -- --ignored
//! --nocapture --test-threads=1`

mod support;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hermes_client::{
    CallOptions, ConnectOptions, DiscoverOptions, EnsurePolicy, Purpose, Scope, connect_with,
    default_home, discover_with, handshake, mint_ticket,
};
use serde_json::json;
use support::{Launch, Terminal};

const RESULT: &str = "LIVE_TALARIA_ATTACH_RESULT";
const ROWS: u16 = 60;
const COLS: u16 = 160;
const RESUMED_LIMIT: Duration = Duration::from_secs(5);

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn never() -> DiscoverOptions {
    DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(future)
}

/// Eight random hex characters from the clock and the process id: unique per run.
fn random_hex() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|span| span.as_nanos())
        .unwrap_or(0);
    let mixed = nanos ^ (std::process::id() as u128).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("{:08x}", (mixed & 0xFFFF_FFFF) as u32)
}

fn skip_reason() -> Option<String> {
    let home = hermes_home();
    let endpoint = match block_on(discover_with(&home, &never())) {
        Ok(endpoint) => endpoint,
        Err(error) => return Some(error.to_string()),
    };
    if endpoint.resolve_profile("fixture").is_err() {
        return Some("the gateway does not serve fixture".to_owned());
    }
    hermes_contract::pinned::live_skip_reason(endpoint.code_sha())
}

fn begin() -> bool {
    if let Some(reason) = skip_reason() {
        println!("{RESULT}: SKIPPED ({reason})");
        if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
            panic!("live test skipped: {reason}");
        }
        return false;
    }
    true
}

/// The binary in a pseudo-terminal, started in `project`, against the live gateway.
fn start(user_home: &Path, project: &Path, args: &[&str]) -> Terminal {
    Launch::new(user_home)
        .hermes_home(&hermes_home())
        .cwd(project)
        .size(ROWS, COLS)
        .answer_queries()
        .args(args)
        .spawn()
}

fn type_line(terminal: &mut Terminal, text: &str) {
    terminal.type_keys(text);
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
}

/// Make the probe session on a profile-scoped `fixture` link: list live,
/// create a titled session, detach without an end reason. Returns its id.
fn make_probe(tag: &str) -> String {
    let home = hermes_home();
    block_on(async {
        let endpoint = discover_with(&home, &never()).await.expect("discovery");
        let profile = endpoint.resolve_profile("fixture").expect("fixture");
        let ticket = mint_ticket(
            &home,
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .expect("a profile ticket");
        let mut connection = connect_with(&endpoint, ticket, &ConnectOptions::default())
            .await
            .expect("connect");
        handshake(&mut connection).await.expect("handshake");
        let (client, _inbound) = hermes_client::Client::start(connection.into_parts());
        let options = CallOptions {
            timeout: Some(Duration::from_secs(30)),
        };
        let listed = client
            .call_raw(
                "session.list",
                json!({"scope": "live", "limit": 200}),
                options,
            )
            .await
            .expect("list live");
        let title = format!("attach-probe-{tag}");
        assert!(
            listed["sessions"].as_array().is_none_or(|rows| rows
                .iter()
                .all(|row| row["title"].as_str() != Some(title.as_str()))),
            "a probe with this tag already exists"
        );
        let folder = tempfile::tempdir().expect("a probe folder");
        let created = client
            .call_raw(
                "session.create",
                json!({
                    "request_id": format!("talaria-attach-probe-{tag}"),
                    "source": "tui",
                    "cwd": folder.path(),
                    "title": title,
                }),
                options,
            )
            .await
            .expect("create the probe");
        let session_id = created["session_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(!session_id.is_empty(), "the probe has no id");
        let subscription = created["subscription_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        client
            .call_raw(
                "session.detach",
                json!({"session_id": session_id, "subscription_id": subscription}),
                options,
            )
            .await
            .expect("detach the probe");
        client.close().await;
        session_id
    })
}

/// Attach by one reference: the resumed line within 5 s, `/quit`, exit 0.
/// Returns the milliseconds from spawn to the resumed line.
fn attach_round(user_home: &Path, project: &Path, reference: &str, extra: &[&str]) -> u128 {
    let mut args = vec!["attach", reference, "--no-ensure"];
    args.extend(extra.iter().copied());
    let mut terminal = start(user_home, project, &args);
    let seen = terminal.wait_for_text("── Resumed: attach-probe-", RESUMED_LIMIT);
    let elapsed = terminal.spawned_at().elapsed().as_millis();
    assert!(seen, "no resumed line for the probe within 5 s");
    type_line(&mut terminal, "/quit");
    let screen = terminal.finish(Duration::from_secs(20));
    assert_eq!(screen.exit_code, Some(0), "attach by {reference} failed");
    elapsed
}

#[test]
#[ignore = "needs a running gateway"]
fn live_attach_by_prefix_and_title() {
    if !begin() {
        return;
    }
    let tag = random_hex();
    let session_id = make_probe(&tag);
    let prefix: String = session_id
        .strip_prefix("local-")
        .unwrap_or(&session_id)
        .chars()
        .take(8)
        .collect();
    assert_eq!(prefix.len(), 8, "the probe id has no 8-hex prefix");

    let user_home = tempfile::tempdir().expect("a home");
    let project = tempfile::tempdir().expect("a project");
    let prefix_ms = attach_round(user_home.path(), project.path(), &prefix, &[]);
    let title_ms = attach_round(
        user_home.path(),
        project.path(),
        &format!("attach-probe-{tag}"),
        &[],
    );
    let fast_ms = attach_round(
        user_home.path(),
        project.path(),
        &session_id,
        &["--profile", "fixture"],
    );

    // No match: exit 1 with the no-match text. The text names every served
    // profile, so the test never prints it, not even on failure.
    let missing = format!("zz-no-such-session-{}", random_hex());
    let mut terminal = start(
        user_home.path(),
        project.path(),
        &["attach", &missing, "--no-ensure"],
    );
    let found = terminal.wait_for_text("talaria: no session matches", RESUMED_LIMIT);
    let screen = terminal.finish(Duration::from_secs(20));
    assert!(found, "no no-match text within 5 s");
    assert_eq!(screen.exit_code, Some(1), "a missing session exited 0");

    // An unknown short id passes through and Hermes reports it missing.
    let mut terminal = start(
        user_home.path(),
        project.path(),
        &[
            "attach",
            "local-0000000000000000",
            "--profile",
            "fixture",
            "--no-ensure",
        ],
    );
    let found = terminal.wait_for_text(
        "no session with the id local-0000000000000000 in the profile fixture.",
        Duration::from_secs(10),
    );
    let screen = terminal.finish(Duration::from_secs(20));
    assert!(found, "no not-found text within 10 s");
    assert_eq!(screen.exit_code, Some(1), "an unknown id exited 0");

    println!("{RESULT}: PASSED prefix_ms={prefix_ms} title_ms={title_ms} fast_ms={fast_ms}");
}
