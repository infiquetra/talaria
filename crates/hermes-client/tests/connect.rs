//! Mocked discovery, tickets, upgrade, and handshake.

mod support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hermes_client::{
    Closed, ConnectError, ConnectOptions, DiscoverOptions, DiscoveryError, Endpoint, EnsureError,
    EnsurePolicy, ExitKind, GatewayState, HandshakeError, HandshakeOptions, NodeKind, NodeProblem,
    PointerProblem, ProfileId, ProfileLookupError, Purpose, ReportedState, Scope, Ticket,
    TicketError, TicketOptions, connect, connect_with, discover_with, format_failure, handshake,
    handshake_with, home_hash, mint_ticket, mint_ticket_with, ws_url,
};
use serde_json::{Value, json};
use support::{
    AttachMock, CODE_SHA, ControlReply, DeleteOnDrop, Echo, Home, INSTANCE_ID, WsPlan, WsStep,
    assert_plain_text, assert_step, bind_stale, canonical_temp, ensure_fixture, frame_capabilities,
    frame_describe, frame_ready, pad_json, ready_identify, serve, set_mode, success_steps,
    write_program,
};

fn text(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| panic!("json"))
}

fn never() -> DiscoverOptions {
    DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    }
}

fn run_script(program: PathBuf, timeout: Duration) -> DiscoverOptions {
    DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Run { program, timeout },
    }
}

fn same(left: &str, right: &str, label: &str) {
    if left != right {
        panic!("{label} mismatch");
    }
}

fn hides_secret(blobs: &[String], secret: &str) {
    for blob in blobs {
        if blob.contains(secret) || blob.contains("hermes-gateway-ticket.") {
            panic!("ticket leaked into an error");
        }
    }
}

fn no_home(blob: &str, home: &str) {
    if blob.contains(home) {
        panic!("text held a home path");
    }
}

const READY_BODY: &str =
    r#"{"state":"ready","reason_code":null,"detail":null,"endpoint":null,"client_code_sha":null}"#;

fn ready_script() -> String {
    format!(
        "#!/bin/sh\nprintf '%s\\n' RUN >> \"${{0%/*}}/run.log\"\nprintf '%s\\n' \"$1\" \"$2\" \"$3\" >> \"${{0%/*}}/run.log\"\nprintf '%s\\n' \"$HERMES_HOME\" >> \"${{0%/*}}/run.log\"\n: > \"${{0%/*}}/marker\"\ni=0\nwhile [ \"$i\" -lt 50 ]; do\n  if [ -e \"$HERMES_HOME/gateway.sock\" ]; then\n    break\n  fi\n  i=$((i + 1))\n  sleep 0.1\ndone\nprintf '%s\\n' banner\nprintf '%s\\n' '{READY_BODY}'\nexit 0\n"
    )
}

fn exit_script(body: &str, code: i32) -> String {
    format!(
        "#!/bin/sh\nprintf '%s\\n' RUN >> \"${{0%/*}}/run.log\"\nprintf '%s\\n' '{body}'\nexit {code}\n"
    )
}

fn run_count(home: &Path) -> usize {
    match std::fs::read_to_string(home.join("run.log")) {
        Ok(text) => text.lines().filter(|line| *line == "RUN").count(),
        Err(_) => 0,
    }
}

fn assert_ready(endpoint: &Endpoint, home: &str, origin: &str) {
    same(endpoint.api_origin(), origin, "api_origin");
    same(endpoint.instance_id(), INSTANCE_ID, "instance");
    assert_eq!(endpoint.authority_epoch(), 3, "epoch");
    assert_eq!(endpoint.runtime_protocol(), 1, "protocol");
    assert_eq!(endpoint.state(), GatewayState::Ready, "state");
    assert!(
        endpoint
            .capabilities()
            .iter()
            .any(|item| item == "session-authority-v1"),
        "capability"
    );
    same(endpoint.code_sha().unwrap_or(""), CODE_SHA, "sha");
    same(endpoint.code_version().unwrap_or(""), "0.22.4", "version");
    assert_eq!(endpoint.profiles().len(), 2, "profiles");
    same(&endpoint.profiles()[0].name, "default", "default");
    same(endpoint.profiles()[0].id.as_ref(), home, "default id");
    same(&endpoint.profiles()[1].name, "fixture", "fixture");
    let fixture = format!("{home}/profiles/fixture");
    same(endpoint.profiles()[1].id.as_ref(), &fixture, "fixture id");
    assert!(endpoint.parked().is_empty(), "parked");
    no_home(&format!("{endpoint:?}"), home);
    no_home(&format!("{endpoint:#?}"), home);
    no_home(&format!("{:?}", endpoint.profiles()[0].id), home);
    no_home(&format!("{:#?}", endpoint.profiles()[0].id), home);
}

fn assert_identify(line: &str) {
    let value: Value =
        serde_json::from_str(line.trim()).unwrap_or_else(|_| panic!("identify request"));
    if value != json!({"protocol": 1, "verb": "identify", "id": 1}) {
        panic!("identify request mismatch");
    }
}

fn assert_ticket_line(line: &str, profile: &str, instance: &str, purpose: &str, scope: &str) {
    let value: Value =
        serde_json::from_str(line.trim()).unwrap_or_else(|_| panic!("ticket request"));
    let mut keys: Vec<&str> = value
        .as_object()
        .map(|obj| obj.keys().map(String::as_str).collect())
        .unwrap_or_default();
    keys.sort_unstable();
    if keys != ["id", "params", "protocol", "verb"] {
        panic!("ticket request keys");
    }
    if value["protocol"] != 1 || value["id"] != 2 || value["verb"] != "session-ticket" {
        panic!("ticket envelope");
    }
    let params = &value["params"];
    let mut param_keys: Vec<&str> = params
        .as_object()
        .map(|obj| obj.keys().map(String::as_str).collect())
        .unwrap_or_default();
    param_keys.sort_unstable();
    if param_keys != ["instance_id", "profile_id", "purpose", "scope"] {
        panic!("ticket param keys");
    }
    if params["profile_id"] != profile
        || params["instance_id"] != instance
        || params["purpose"] != purpose
        || params["scope"] != scope
    {
        panic!("ticket params");
    }
}

async fn wait_marker(path: PathBuf) {
    loop {
        if path.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

mod identify_parsing {
    use super::*;

    #[tokio::test]
    async fn from_identify_accepts_a_ready_reply() {
        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let origin = "http://127.0.0.1:43521";
        let value = ready_identify(&canonical, origin);
        let endpoint = Endpoint::from_identify(&value, Path::new(&canonical)).expect("ready");
        assert_ready(&endpoint, &canonical, origin);

        let mut draining = value.clone();
        draining["state"] = json!("draining");
        draining["drain_reason"] = json!("restart_wait");
        let endpoint = Endpoint::from_identify(&draining, Path::new(&canonical)).expect("draining");
        assert_eq!(
            endpoint.state(),
            GatewayState::DrainingForRestart,
            "restart wait"
        );

        for origin in [
            "http://127.0.0.2:5",
            "http://[::1]:5",
            "http://127.0.0.1:80",
            "https://127.0.0.1:9",
        ] {
            let mut row = ready_identify(&canonical, origin);
            row["extra"] = json!(true);
            let endpoint = Endpoint::from_identify(&row, Path::new(&canonical)).expect("origin");
            same(endpoint.api_origin(), origin, "accepted origin");
        }

        let mut bare = ready_identify(&canonical, origin);
        bare["code_sha"] = Value::Null;
        bare.as_object_mut().map(|obj| obj.remove("code_version"));
        let endpoint = Endpoint::from_identify(&bare, Path::new(&canonical)).expect("missing code");
        assert!(endpoint.code_sha().is_none(), "null sha");
        assert!(endpoint.code_version().is_none(), "missing version");
    }

    #[derive(Clone, Copy)]
    enum Expect {
        Incompatible,
        InvalidState,
        Starting,
        Conflict,
        Draining(bool),
        InvalidIdentity,
        InvalidApiOrigin,
        InvalidSupervisor,
        HomeNotServed,
    }

    fn matches_expect(err: &DiscoveryError, expect: Expect) -> bool {
        match (err, expect) {
            (DiscoveryError::Incompatible, Expect::Incompatible) => true,
            (DiscoveryError::InvalidState, Expect::InvalidState) => true,
            (
                DiscoveryError::NotReady {
                    state: ReportedState::Starting,
                    idle_exit: false,
                },
                Expect::Starting,
            ) => true,
            (
                DiscoveryError::NotReady {
                    state: ReportedState::Conflict,
                    idle_exit: false,
                },
                Expect::Conflict,
            ) => true,
            (
                DiscoveryError::NotReady {
                    state: ReportedState::Draining,
                    idle_exit,
                },
                Expect::Draining(want),
            ) => *idle_exit == want,
            (DiscoveryError::InvalidIdentity, Expect::InvalidIdentity) => true,
            (DiscoveryError::InvalidApiOrigin, Expect::InvalidApiOrigin) => true,
            (DiscoveryError::InvalidSupervisor, Expect::InvalidSupervisor) => true,
            (DiscoveryError::HomeNotServed, Expect::HomeNotServed) => true,
            _ => false,
        }
    }

    #[tokio::test]
    async fn from_identify_rejects_bad_replies() {
        let home = Home::new();
        let canonical = home.canonical();
        let rows: Vec<(&str, fn(&mut Value, &str), Expect)> = vec![
            (
                "protocol 2",
                |value, _| value["runtime_protocol"] = json!(2),
                Expect::Incompatible,
            ),
            (
                "protocol text",
                |value, _| value["runtime_protocol"] = json!("1"),
                Expect::Incompatible,
            ),
            (
                "protocol float",
                |value, _| {
                    value["runtime_protocol"] =
                        Value::Number(serde_json::Number::from_f64(1.0).expect("one"));
                },
                Expect::Incompatible,
            ),
            (
                "protocol missing",
                |value, _| {
                    value
                        .as_object_mut()
                        .map(|obj| obj.remove("runtime_protocol"));
                },
                Expect::Incompatible,
            ),
            (
                "state booting",
                |value, _| value["state"] = json!("booting"),
                Expect::InvalidState,
            ),
            (
                "state number",
                |value, _| value["state"] = json!(5),
                Expect::InvalidState,
            ),
            (
                "state missing",
                |value, _| {
                    value.as_object_mut().map(|obj| obj.remove("state"));
                },
                Expect::InvalidState,
            ),
            (
                "state starting",
                |value, _| value["state"] = json!("starting"),
                Expect::Starting,
            ),
            (
                "state conflict",
                |value, _| value["state"] = json!("conflict"),
                Expect::Conflict,
            ),
            (
                "draining without reason",
                |value, _| value["state"] = json!("draining"),
                Expect::Draining(false),
            ),
            (
                "draining reason number",
                |value, _| {
                    value["state"] = json!("draining");
                    value["drain_reason"] = json!(5);
                },
                Expect::Draining(false),
            ),
            (
                "draining update",
                |value, _| {
                    value["state"] = json!("draining");
                    value["drain_reason"] = json!("update");
                },
                Expect::Draining(false),
            ),
            (
                "draining idle_exit",
                |value, _| {
                    value["state"] = json!("draining");
                    value["drain_reason"] = json!("idle_exit");
                },
                Expect::Draining(true),
            ),
            (
                "capabilities missing authority",
                |value, _| value["capabilities"] = json!(["durable-admission-v1"]),
                Expect::Incompatible,
            ),
            (
                "capabilities not strings",
                |value, _| value["capabilities"] = json!([1]),
                Expect::Incompatible,
            ),
            (
                "epoch zero",
                |value, _| value["authority_epoch"] = json!(0),
                Expect::InvalidIdentity,
            ),
            (
                "epoch negative",
                |value, _| value["authority_epoch"] = json!(-1),
                Expect::InvalidIdentity,
            ),
            (
                "epoch float",
                |value, _| {
                    value["authority_epoch"] =
                        Value::Number(serde_json::Number::from_f64(1.5).expect("epoch"));
                },
                Expect::InvalidIdentity,
            ),
            (
                "epoch text",
                |value, _| value["authority_epoch"] = json!("3"),
                Expect::InvalidIdentity,
            ),
            (
                "epoch missing",
                |value, _| {
                    value
                        .as_object_mut()
                        .map(|obj| obj.remove("authority_epoch"));
                },
                Expect::InvalidIdentity,
            ),
            (
                "instance empty",
                |value, _| value["instance_id"] = json!(""),
                Expect::InvalidIdentity,
            ),
            (
                "instance number",
                |value, _| value["instance_id"] = json!(3),
                Expect::InvalidIdentity,
            ),
            (
                "supervisor manual",
                |value, _| value["supervisor"] = json!("manual"),
                Expect::InvalidSupervisor,
            ),
            (
                "supervisor desktop",
                |value, _| value["supervisor"] = json!("desktop"),
                Expect::InvalidSupervisor,
            ),
            (
                "supervisor missing",
                |value, _| {
                    value.as_object_mut().map(|obj| obj.remove("supervisor"));
                },
                Expect::InvalidSupervisor,
            ),
            (
                "parked list",
                |value, _| value["parked_profiles"] = json!([]),
                Expect::InvalidIdentity,
            ),
            (
                "parked value number",
                |value, _| value["parked_profiles"] = json!({"fixture": 1}),
                Expect::InvalidIdentity,
            ),
            (
                "duplicate names",
                |value, home| {
                    let left = format!("{home}/profiles/fixture");
                    let right = format!("{home}/profiles/Fixture");
                    value["served_profiles"] = json!([
                        {"profile_id": home, "home": home},
                        {"profile_id": left, "home": left},
                        {"profile_id": right, "home": right}
                    ]);
                },
                Expect::InvalidIdentity,
            ),
            (
                "served missing",
                |value, _| {
                    value
                        .as_object_mut()
                        .map(|obj| obj.remove("served_profiles"));
                },
                Expect::HomeNotServed,
            ),
            (
                "served not list",
                |value, _| value["served_profiles"] = json!({}),
                Expect::HomeNotServed,
            ),
            (
                "served empty",
                |value, _| value["served_profiles"] = json!([]),
                Expect::HomeNotServed,
            ),
            (
                "served other home",
                |value, home| {
                    let other = format!("{home}-other");
                    value["served_profiles"] = json!([{"profile_id": other, "home": other}]);
                },
                Expect::HomeNotServed,
            ),
            (
                "served twice",
                |value, home| {
                    value["served_profiles"] = json!([
                        {"profile_id": home, "home": home},
                        {"profile_id": home, "home": home}
                    ]);
                },
                Expect::HomeNotServed,
            ),
            (
                "empty profile id",
                |value, home| {
                    value["served_profiles"] = json!([{"profile_id": "", "home": home}]);
                },
                Expect::HomeNotServed,
            ),
            (
                "order protocol before state",
                |value, _| {
                    value["runtime_protocol"] = json!(2);
                    value["state"] = json!("starting");
                },
                Expect::Incompatible,
            ),
            (
                "order state before origin",
                |value, _| {
                    value["state"] = json!("starting");
                    value["api_origin"] = json!("http://127.0.0.1");
                },
                Expect::Starting,
            ),
            (
                "real starting shape",
                |value, _| {
                    value["state"] = json!("starting");
                    value["capabilities"] = json!([]);
                    value["served_profiles"] = json!([]);
                    value
                        .as_object_mut()
                        .map(|obj| obj.remove("authority_epoch"));
                },
                Expect::Starting,
            ),
        ];
        let origins = [
            "http://localhost:1234",
            "http://example.test:80",
            "http://127.0.0.1",
            "http://127.0.0.1:0",
            "http://127.0.0.1:99999",
            "http://u:p@127.0.0.1:9",
            "http://127.0.0.1:9/",
            "http://127.0.0.1:9/x",
            "http://127.0.0.1:9?q",
            "http://127.0.0.1:9#f",
            "ftp://127.0.0.1:9",
            "http://[::1:9",
            "http://[0:0:0:0:0:0:0:1]:9",
            "http://[::ffff:127.0.0.1]:9",
            "http://127.1:9",
            "http://127.0.0.01:9",
            "http://0127.0.0.1:9",
            "http://127.0.0.1:09",
            "http://127.0.0.1:0080",
        ];
        for (name, apply, expect) in rows {
            let mut value = ready_identify(&canonical, "http://127.0.0.1:43521");
            apply(&mut value, &canonical);
            let err = Endpoint::from_identify(&value, Path::new(&canonical)).expect_err(name);
            assert!(matches_expect(&err, expect), "{name}");
        }
        for origin in origins {
            let value = ready_identify(&canonical, origin);
            let err = Endpoint::from_identify(&value, Path::new(&canonical)).expect_err(origin);
            assert!(matches_expect(&err, Expect::InvalidApiOrigin), "{origin}");
        }
    }

    #[tokio::test]
    async fn profile_lookup_normalises_names_and_separates_parked_from_unknown() {
        let home = Home::new();
        let canonical = home.canonical();
        let served = ready_identify(&canonical, "http://127.0.0.1:43521");
        let endpoint = Endpoint::from_identify(&served, Path::new(&canonical)).expect("served");
        for name in ["fixture", "Fixture", " fixture ", "default", "Default"] {
            let id = endpoint
                .resolve_profile(name)
                .unwrap_or_else(|_| panic!("lookup {name}"));
            let want = if name.trim().eq_ignore_ascii_case("default") {
                canonical.clone()
            } else {
                format!("{canonical}/profiles/fixture")
            };
            same(id.as_ref(), &want, name);
        }
        let mut parked = ready_identify(&canonical, "http://127.0.0.1:43521");
        parked["served_profiles"] = json!([{"profile_id": canonical, "home": canonical}]);
        parked["parked_profiles"] = json!({"fixture": format!("see {canonical}/note")});
        let endpoint = Endpoint::from_identify(&parked, Path::new(&canonical)).expect("parked");
        match endpoint.resolve_profile("fixture") {
            Err(ProfileLookupError::Parked { name, reason }) => {
                same(&name, "fixture", "parked name");
                no_home(&reason, &canonical);
                assert!(
                    reason.contains("<home>") || reason.contains("<path>"),
                    "scrubbed reason"
                );
                assert_eq!(
                    ProfileLookupError::Parked {
                        name,
                        reason: reason.clone()
                    }
                    .next_step(),
                    "Fix the cause the gateway reports, then check `hermes gateway status`."
                );
            }
            _ => panic!("parked profile"),
        }
        match endpoint.resolve_profile("") {
            Err(err @ ProfileLookupError::NeverDiscovered { .. }) => {
                assert!(err.to_string().contains("named"), "empty name");
                assert!(err.next_step().ends_with('.'), "empty step");
            }
            _ => panic!("empty name"),
        }
        match endpoint.resolve_profile("nosuch") {
            Err(ProfileLookupError::NeverDiscovered { name }) => same(&name, "nosuch", "unknown"),
            _ => panic!("unknown profile"),
        }
    }
}

mod control_socket {
    use super::*;

    #[tokio::test]
    async fn discover_sends_exact_identify_request() {
        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let socket = home.path().join("gateway.sock");
        let mock = serve(
            &socket,
            {
                let canonical = canonical.clone();
                move |origin| {
                    let line = text(&ready_identify(&canonical, origin));
                    vec![ControlReply::Line(line.clone()), ControlReply::Line(line)]
                }
            },
            WsPlan::Idle,
        )
        .await;
        let endpoint = discover_with(home.path(), &never())
            .await
            .expect("discover");
        assert_eq!(mock.connections(), 1, "one connection");
        assert_eq!(mock.requests().len(), 1, "one request");
        assert_identify(&mock.requests()[0]);
        assert_ready(&endpoint, &canonical, mock.origin());

        let link_home = Home::new();
        let link = link_home.path().join("link");
        if std::os::unix::fs::symlink(home.path(), &link).is_err() {
            panic!("symlink failed");
        }
        let endpoint = discover_with(&link, &never())
            .await
            .expect("symlink discover");
        assert_ready(&endpoint, &canonical, mock.origin());
        assert_eq!(mock.connections(), 2, "symlink connection");
    }

    #[tokio::test]
    async fn discover_follows_a_valid_pointer_file() {
        assert_eq!(home_hash("/example-home"), "29e84367be239308");
        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let hash = home_hash(&canonical);
        let root = canonical_temp().join(format!("hermes-gw-{hash}"));
        let socket = root.join("control.sock");
        if socket.as_os_str().len() > 100 {
            panic!("TMPDIR is too long for a control socket");
        }
        if std::fs::create_dir_all(&root).is_err() {
            panic!("pointer directory failed");
        }
        let _guard = DeleteOnDrop(root.clone());
        set_mode(&root, 0o700);
        let canonical_for_reply = canonical.clone();
        let mock = serve(
            &socket,
            move |origin| {
                vec![ControlReply::Line(text(&ready_identify(
                    &canonical_for_reply,
                    origin,
                )))]
            },
            WsPlan::Idle,
        )
        .await;
        let pointer = home.path().join("gateway.sock.path");
        let socket_text = socket.to_string_lossy().into_owned();
        if std::fs::write(&pointer, format!("{socket_text}\n")).is_err() {
            panic!("pointer write failed");
        }
        set_mode(&pointer, 0o600);
        let endpoint = discover_with(home.path(), &never()).await.expect("pointer");
        assert_ready(&endpoint, &canonical, mock.origin());

        if std::fs::write(&pointer, format!("  {socket_text}  \n")).is_err() {
            panic!("pointer rewrite failed");
        }
        set_mode(&pointer, 0o600);
        discover_with(home.path(), &never())
            .await
            .expect("trimmed pointer");

        let bad = [
            (
                "relative",
                "relative.sock".to_string(),
                PointerProblem::NotAbsolute,
            ),
            ("too large", "x".repeat(4097), PointerProblem::TooLarge),
            (
                "wrong file",
                root.join("other.sock").to_string_lossy().into_owned(),
                PointerProblem::WrongLeaf,
            ),
            (
                "wrong directory",
                canonical_temp()
                    .join("other-dir")
                    .join("control.sock")
                    .to_string_lossy()
                    .into_owned(),
                PointerProblem::WrongLeaf,
            ),
            (
                "other hash",
                canonical_temp()
                    .join(format!("hermes-gw-{}", home_hash("other-home")))
                    .join("control.sock")
                    .to_string_lossy()
                    .into_owned(),
                PointerProblem::WrongLeaf,
            ),
        ];
        for (name, body, problem) in bad {
            if std::fs::write(&pointer, body).is_err() {
                panic!("pointer case failed");
            }
            set_mode(&pointer, 0o600);
            let err = discover_with(home.path(), &never()).await.expect_err(name);
            assert!(
                matches!(err, DiscoveryError::BadPointer(got) if got == problem),
                "{name}"
            );
        }

        set_mode(&pointer, 0o644);
        let err = discover_with(home.path(), &never())
            .await
            .expect_err("open pointer");
        assert!(
            matches!(
                err,
                DiscoveryError::UnsafeNode {
                    node: NodeKind::PointerFile,
                    ..
                }
            ),
            "mode"
        );
        let real = home.path().join("pointer-target");
        if std::fs::write(&real, &socket_text).is_err() {
            panic!("pointer target failed");
        }
        let _ = std::fs::remove_file(&pointer);
        if std::os::unix::fs::symlink(&real, &pointer).is_err() {
            panic!("pointer symlink failed");
        }
        let err = discover_with(home.path(), &never())
            .await
            .expect_err("symlink pointer");
        assert!(
            matches!(
                err,
                DiscoveryError::UnsafeNode {
                    node: NodeKind::PointerFile,
                    problem: NodeProblem::WrongType
                }
            ),
            "symlink"
        );

        let _ = std::fs::remove_file(&pointer);
        if std::fs::write(&pointer, format!("{socket_text}\n")).is_err() {
            panic!("pointer restore failed");
        }
        set_mode(&pointer, 0o600);
        set_mode(&root, 0o755);
        let err = discover_with(home.path(), &never())
            .await
            .expect_err("pointer dir");
        assert!(
            matches!(
                err,
                DiscoveryError::UnsafeNode {
                    node: NodeKind::PointerDir,
                    ..
                }
            ),
            "pointer directory"
        );

        set_mode(&root, 0o700);
        let missing = root.join("control.sock");
        drop(mock);
        let _ = std::fs::remove_file(&missing);
        if std::fs::write(&pointer, format!("{}\n", missing.to_string_lossy())).is_err() {
            panic!("missing pointer failed");
        }
        set_mode(&pointer, 0o600);
        let err = discover_with(home.path(), &never())
            .await
            .expect_err("missing target");
        assert!(matches!(err, DiscoveryError::NoGateway), "missing target");
    }

    fn assert_socket_step(err: &DiscoveryError, problem: NodeProblem, message: &str, step: &str) {
        assert!(
            matches!(err, DiscoveryError::UnsafeNode { node: NodeKind::Socket, problem: got } if *got == problem),
            "socket problem"
        );
        same(&err.to_string(), message, "socket message");
        same(err.next_step(), step, "socket step");
    }

    #[tokio::test]
    async fn discover_classifies_socket_nodes() {
        let empty = Home::new();
        let err = discover_with(empty.path(), &never())
            .await
            .expect_err("empty");
        assert!(matches!(err, DiscoveryError::NoGateway), "empty home");

        let missing = empty.path().join("absent");
        let err = discover_with(&missing, &never())
            .await
            .expect_err("missing");
        assert!(matches!(err, DiscoveryError::NoGateway), "missing home");

        let stale = Home::new();
        bind_stale(&stale.path().join("gateway.sock"), 0o600);
        let err = discover_with(stale.path(), &never())
            .await
            .expect_err("stale");
        assert!(matches!(err, DiscoveryError::NoGateway), "stale socket");

        let open = Home::new();
        let socket = open.path().join("gateway.sock");
        let mock = serve(&socket, |_| vec![ControlReply::Silence], WsPlan::Idle).await;
        set_mode(&socket, 0o666);
        let err = discover_with(open.path(), &never())
            .await
            .expect_err("mode 0666");
        assert_eq!(mock.connections(), 0, "open socket was contacted");
        assert_socket_step(
            &err,
            NodeProblem::OpenToOthers,
            "the control socket has unsafe permissions",
            "Fix the permissions (`chmod go-rwx` on the socket), or run Talaria as the user who runs the gateway.",
        );
        no_home(&err.to_string(), &open.canonical());

        let wide = Home::new();
        bind_stale(&wide.path().join("gateway.sock"), 0o755);
        let err = discover_with(wide.path(), &never())
            .await
            .expect_err("mode 0755");
        assert_socket_step(
            &err,
            NodeProblem::OpenToOthers,
            "the control socket has unsafe permissions",
            "Fix the permissions (`chmod go-rwx` on the socket), or run Talaria as the user who runs the gateway.",
        );

        let writable = Home::new();
        set_mode(writable.path(), 0o777);
        let err = discover_with(writable.path(), &never())
            .await
            .expect_err("home mode");
        assert!(
            matches!(
                err,
                DiscoveryError::UnsafeNode {
                    node: NodeKind::Home,
                    problem: NodeProblem::WritableByOthers
                }
            ),
            "writable home"
        );
        same(
            err.next_step(),
            "Remove write access for other users (`chmod o-w` on the Hermes home), or run Talaria as the user who runs the gateway.",
            "home step",
        );
        no_home(&err.to_string(), &writable.canonical());

        let file = Home::new();
        let sock = file.path().join("gateway.sock");
        if std::fs::write(&sock, "nope").is_err() {
            panic!("regular file failed");
        }
        set_mode(&sock, 0o600);
        let err = discover_with(file.path(), &never())
            .await
            .expect_err("regular file");
        assert!(
            matches!(
                err,
                DiscoveryError::UnsafeNode {
                    node: NodeKind::Socket,
                    problem: NodeProblem::WrongType
                }
            ),
            "regular file"
        );
        same(
            err.next_step(),
            "Remove the leftover file named gateway.sock from the Hermes home, then run `hermes gateway ensure --json`.",
            "file step",
        );

        let linked = Home::new();
        let target = linked.path().join("other");
        if std::fs::write(&target, "nope").is_err() {
            panic!("link target failed");
        }
        if std::os::unix::fs::symlink(&target, linked.path().join("gateway.sock")).is_err() {
            panic!("socket symlink failed");
        }
        let err = discover_with(linked.path(), &never())
            .await
            .expect_err("socket symlink");
        assert!(
            matches!(
                err,
                DiscoveryError::UnsafeNode {
                    node: NodeKind::Socket,
                    problem: NodeProblem::WrongType
                }
            ),
            "socket symlink"
        );

        let group = Home::new();
        ensure_fixture(group.path());
        let canonical = group.canonical();
        let socket = group.path().join("gateway.sock");
        let mock = serve(
            &socket,
            move |origin| {
                vec![ControlReply::Line(text(&ready_identify(
                    &canonical, origin,
                )))]
            },
            WsPlan::Idle,
        )
        .await;
        set_mode(group.path(), 0o775);
        let endpoint = discover_with(group.path(), &never())
            .await
            .expect("group home");
        assert_eq!(endpoint.state(), GatewayState::Ready, "group-writable home");
        let _ = mock;

        let err = discover_with(Path::new("relative-home"), &never())
            .await
            .expect_err("relative");
        assert!(matches!(err, DiscoveryError::NoGateway), "relative home");
    }

    #[tokio::test]
    async fn discover_refuses_a_named_profile_home_without_a_socket() {
        let root = Home::new();
        let profile = root.path().join("profiles").join("fixture");
        if std::fs::create_dir_all(&profile).is_err() {
            panic!("named home failed");
        }
        set_mode(&root.path().join("profiles"), 0o700);
        set_mode(&profile, 0o700);
        let program = write_program(root.path(), "stand-in", &ready_script());
        let options = run_script(program, Duration::from_secs(2));
        let err = discover_with(&profile, &options).await.expect_err("named");
        assert!(
            matches!(err, DiscoveryError::NamedProfileHome),
            "named home"
        );
        assert!(!root.path().join("marker").exists(), "ensure ran");

        bind_stale(&profile.join("gateway.sock"), 0o600);
        let err = discover_with(&profile, &options)
            .await
            .expect_err("stale named");
        assert!(
            matches!(err, DiscoveryError::NamedProfileHome),
            "stale named home"
        );
        assert!(!root.path().join("marker").exists(), "ensure ran for stale");

        let canonical = profile
            .canonicalize()
            .unwrap_or_else(|_| panic!("named canonical"));
        let canonical = canonical.to_string_lossy().into_owned();
        let socket = profile.join("gateway.sock");
        let _ = std::fs::remove_file(&socket);
        let mock = serve(
            &socket,
            move |origin| {
                let mut value = ready_identify(&canonical, origin);
                value["served_profiles"] = json!([{"profile_id": canonical, "home": canonical}]);
                vec![ControlReply::Line(text(&value))]
            },
            WsPlan::Idle,
        )
        .await;
        let endpoint = discover_with(&profile, &never())
            .await
            .expect("named socket");
        assert_eq!(endpoint.profiles().len(), 1, "named profile served");
        let _ = mock;
    }

    #[tokio::test]
    async fn discover_times_out_on_a_silent_gateway() {
        let home = Home::new();
        let socket = home.path().join("gateway.sock");
        let _mock = serve(&socket, |_| vec![ControlReply::Silence], WsPlan::Idle).await;
        let options = DiscoverOptions {
            control_timeout: Duration::from_millis(200),
            ensure: EnsurePolicy::Never,
        };
        let started = Instant::now();
        let err = discover_with(home.path(), &options)
            .await
            .expect_err("silent");
        assert!(matches!(err, DiscoveryError::ControlTimeout), "timeout");
        assert!(started.elapsed() < Duration::from_secs(2), "deadline");
    }

    #[tokio::test]
    async fn discover_rejects_malformed_replies() {
        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let socket = home.path().join("gateway.sock");
        let padded = pad_json(ready_identify(&canonical, "http://127.0.0.1:9"), 524_288);
        let mock = serve(
            &socket,
            move |_| vec![ControlReply::Line(padded.clone())],
            WsPlan::Idle,
        )
        .await;
        discover_with(home.path(), &never())
            .await
            .expect("exact limit");
        drop(mock);

        let cases = [
            ("over no newline", ControlReply::Bytes { len: 524_289, newline: false }),
            ("over with newline", ControlReply::Bytes { len: 524_289, newline: true }),
            ("close", ControlReply::Close),
            ("invalid json", ControlReply::Line("{".to_string())),
            ("wrong protocol", ControlReply::Line(r#"{"ok":true,"protocol":2,"id":1,"result":{}}"#.to_string())),
            ("wrong id", ControlReply::Line(r#"{"ok":true,"protocol":1,"id":9,"result":{}}"#.to_string())),
            ("result list", ControlReply::Line(r#"{"ok":true,"protocol":1,"id":1,"result":[]}"#.to_string())),
            (
                "two lines",
                ControlReply::Lines(
                    text(&ready_identify(&canonical, "http://127.0.0.1:9")),
                    "nope".to_string(),
                ),
            ),
            (
                "real oversize",
                ControlReply::Line(
                    r#"{"ok":false,"error":"response_too_large","code":"response_too_large","limit":524288,"protocol":1,"id":1}"#
                        .to_string(),
                ),
            ),
            ("bare refusal", ControlReply::Line(r#"{"ok":false,"error":"x"}"#.to_string())),
        ];
        for (name, reply) in cases {
            let mock = serve(&socket, move |_| vec![reply], WsPlan::Idle).await;
            if name == "two lines" {
                discover_with(home.path(), &never())
                    .await
                    .expect("two lines");
                drop(mock);
                continue;
            }
            let err = discover_with(home.path(), &never()).await.expect_err(name);
            match name {
                "over no newline" | "over with newline" => {
                    assert!(matches!(err, DiscoveryError::ReplyTooLarge), "{name}")
                }
                "real oversize" | "bare refusal" => {
                    assert!(
                        matches!(err, DiscoveryError::ControlRefused { .. }),
                        "{name}"
                    )
                }
                _ => assert!(matches!(err, DiscoveryError::InvalidReply), "{name}"),
            }
            drop(mock);
        }
    }
}

mod ensure {
    use super::*;

    fn step_for(code: i32, reason: Option<&str>) -> &'static str {
        match code {
            2 => "Talaria called Hermes incorrectly. Report a bug.",
            3 => "Update Hermes to v0.22.3 or later and restart its gateway.",
            4 => match reason {
                Some("profile_mismatch") => {
                    "The gateway does not serve this Hermes home; check HERMES_HOME and run `hermes gateway status`."
                }
                Some("authorization") | None => "Run Talaria as the user who runs the gateway.",
                Some(_) => {
                    "Run Talaria as the user who runs the gateway, then restart the gateway."
                }
            },
            5 => "Run `hermes gateway ensure --json` yourself and read the gateway log.",
            6 => "Wait a minute and try again.",
            7 => "Check `hermes gateway status`.",
            _ => "Run `hermes gateway ensure --json` yourself and read its output.",
        }
    }

    #[tokio::test]
    async fn ensure_runs_once_with_exact_arguments_and_environment() {
        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let program = write_program(home.path(), "stand-in", &ready_script());
        let marker = home.path().join("marker");
        let home_path = home.path().to_path_buf();
        let options = run_script(program, Duration::from_secs(5));
        let task = tokio::spawn(async move { discover_with(&home_path, &options).await });
        tokio::pin!(task);
        let mut marker_wait = Box::pin(wait_marker(marker.clone()));
        let mut mock: Option<AttachMock> = None;
        loop {
            tokio::select! {
                biased;
                joined = &mut task => {
                    match joined {
                        Ok(Ok(endpoint)) => {
                            assert_eq!(run_count(home.path()), 1, "one ensure");
                            let log = std::fs::read_to_string(home.path().join("run.log")).unwrap_or_default();
                            let mut lines = log.lines();
                            assert_eq!(lines.next(), Some("RUN"), "run marker");
                            assert_eq!(lines.next(), Some("gateway"), "arg");
                            assert_eq!(lines.next(), Some("ensure"), "arg");
                            assert_eq!(lines.next(), Some("--json"), "arg");
                            let recorded = lines.next().unwrap_or("");
                            same(recorded, &canonical, "home");
                            assert_ready(&endpoint, &canonical, mock.as_ref().map(|item| item.origin()).unwrap_or(""));
                            break;
                        }
                        Ok(Err(err)) => panic!("ensure discover failed: {err}"),
                        Err(join) if join.is_panic() => std::panic::resume_unwind(join.into_panic()),
                        Err(_) => panic!("discover task ended"),
                    }
                }
                _ = &mut marker_wait, if mock.is_none() => {
                    let socket = home.path().join("gateway.sock");
                    let canonical = canonical.clone();
                    mock = Some(serve(&socket, move |origin| {
                        vec![ControlReply::Line(text(&ready_identify(&canonical, origin)))]
                    }, WsPlan::Idle).await);
                }
            }
        }
    }

    #[tokio::test]
    async fn ensure_exit_codes_map_to_typed_errors() {
        let rows = [
            (2, ExitKind::InvalidInvocation, None, READY_BODY),
            (3, ExitKind::Incompatible, None, READY_BODY),
            (
                4,
                ExitKind::Authorization,
                Some("authorization"),
                r#"{"state":"absent","reason_code":"authorization","detail":null,"endpoint":null,"client_code_sha":null}"#,
            ),
            (
                4,
                ExitKind::Authorization,
                Some("profile_mismatch"),
                r#"{"state":"absent","reason_code":"profile_mismatch","detail":null,"endpoint":null,"client_code_sha":null}"#,
            ),
            (
                4,
                ExitKind::Authorization,
                Some("gateway_elevated"),
                r#"{"state":"absent","reason_code":"gateway_elevated","detail":null,"endpoint":null,"client_code_sha":null}"#,
            ),
            (
                4,
                ExitKind::Authorization,
                Some("gateway_other_session"),
                r#"{"state":"absent","reason_code":"gateway_other_session","detail":null,"endpoint":null,"client_code_sha":null}"#,
            ),
            (5, ExitKind::Deadline, None, READY_BODY),
            (6, ExitKind::Draining, None, READY_BODY),
            (7, ExitKind::Inaccessible, None, READY_BODY),
            (9, ExitKind::Other, None, READY_BODY),
        ];
        for (code, kind, reason, body) in rows {
            let home = Home::new();
            let program = write_program(home.path(), "stand-in", &exit_script(body, code));
            let err = discover_with(home.path(), &run_script(program, Duration::from_secs(2)))
                .await
                .expect_err("exit");
            let text = err.to_string();
            assert!(text.contains(&format!("exit {code}")), "exit {code}");
            if kind == ExitKind::Incompatible {
                assert!(text.contains("incompatible"), "incompatible");
            }
            assert_eq!(err.next_step(), step_for(code, reason), "step {code}");
            match &err {
                DiscoveryError::Ensure(EnsureError::Exit {
                    code: got,
                    kind: got_kind,
                    reason_code,
                    ..
                }) => {
                    assert_eq!(*got, code, "code");
                    assert_eq!(*got_kind, kind, "kind");
                    assert_eq!(reason_code.as_deref(), reason, "reason");
                }
                _ => panic!("expected ensure exit"),
            }
            assert_eq!(run_count(home.path()), 1, "one run");
        }

        let home = Home::new();
        let program = write_program(
            home.path(),
            "stand-in",
            "#!/bin/sh\nprintf '%s\\n' RUN >> \"${0%/*}/run.log\"\nkill -9 $$\n",
        );
        let err = discover_with(home.path(), &run_script(program, Duration::from_secs(2)))
            .await
            .expect_err("signal");
        match &err {
            DiscoveryError::Ensure(EnsureError::Exit {
                code: -1,
                kind: ExitKind::Other,
                ..
            }) => {}
            _ => panic!("signal death"),
        }
        assert!(err.to_string().contains("exit -1"), "signal text");
        assert_eq!(err.next_step(), step_for(-1, None), "signal step");

        let home = Home::new();
        let program = write_program(
            home.path(),
            "stand-in",
            "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"${0%/*}/pid\"\nsleep 30\n",
        );
        let started = Instant::now();
        let err = discover_with(
            home.path(),
            &run_script(program, Duration::from_millis(300)),
        )
        .await
        .expect_err("timed out");
        assert!(
            matches!(err, DiscoveryError::Ensure(EnsureError::TimedOut)),
            "timed out"
        );
        assert!(started.elapsed() < Duration::from_secs(3), "timeout budget");
        let pid = std::fs::read_to_string(home.path().join("pid")).unwrap_or_default();
        let pid = pid.trim();
        if !pid.is_empty() {
            let alive = std::process::Command::new("kill")
                .args(["-0", pid])
                .status();
            assert!(
                alive.map(|status| !status.success()).unwrap_or(true),
                "child still running"
            );
        }

        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let program = write_program(
            home.path(),
            "stand-in",
            "#!/bin/sh\nprintf '%s\\n' RUN >> \"${0%/*}/run.log\"\n: > \"${0%/*}/marker\"\nsleep 30 &\nwhile [ ! -e \"$HERMES_HOME/gateway.sock\" ]; do sleep 0.05; done\nprintf '%s\\n' '{\"state\":\"ready\",\"reason_code\":null,\"detail\":null,\"endpoint\":null,\"client_code_sha\":null}'\nexit 0\n",
        );
        let marker = home.path().join("marker");
        let home_path = home.path().to_path_buf();
        let options = run_script(program, Duration::from_secs(5));
        let task = tokio::spawn(async move { discover_with(&home_path, &options).await });
        tokio::pin!(task);
        let mut marker_wait = Box::pin(wait_marker(marker));
        let mut bound = false;
        let started = Instant::now();
        loop {
            tokio::select! {
                biased;
                joined = &mut task => {
                    match joined {
                        Ok(Ok(_)) => {
                            assert!(started.elapsed() < Duration::from_secs(5), "pipe held the call");
                            break;
                        }
                        Ok(Err(err)) => panic!("pipe ensure failed: {err}"),
                        Err(join) if join.is_panic() => std::panic::resume_unwind(join.into_panic()),
                        Err(_) => panic!("pipe task ended"),
                    }
                }
                _ = &mut marker_wait, if !bound => {
                    bound = true;
                    let socket = home.path().join("gateway.sock");
                    let canonical = canonical.clone();
                    let _mock = serve(&socket, move |origin| {
                        vec![ControlReply::Line(text(&ready_identify(&canonical, origin)))]
                    }, WsPlan::Idle).await;
                    // The listener has to outlive the retry. Leak it on purpose for this row.
                    std::mem::forget(_mock);
                }
            }
        }

        let home = Home::new();
        let program = write_program(
            home.path(),
            "stand-in",
            "#!/bin/sh\nprintf '%s\\n' \"$HERMES_HOME/secret-leaf\" >&2\nexit 7\n",
        );
        let canonical = home.canonical();
        let err = discover_with(home.path(), &run_script(program, Duration::from_secs(2)))
            .await
            .expect_err("stderr");
        no_home(&err.to_string(), &canonical);
        no_home(&format!("{err:?}"), &canonical);
        no_home(err.next_step(), &canonical);

        for (name, body) in [
            ("not json", "not-json"),
            ("empty", ""),
            ("starting", r#"{"state":"starting"}"#),
        ] {
            let home = Home::new();
            let program = write_program(home.path(), "stand-in", &exit_script(body, 0));
            let err = discover_with(home.path(), &run_script(program, Duration::from_secs(2)))
                .await
                .expect_err(name);
            assert!(
                matches!(err, DiscoveryError::Ensure(EnsureError::UnreadableOutput)),
                "{name}"
            );
            assert_eq!(run_count(home.path()), 1, "{name}");
        }

        let home = Home::new();
        let program = write_program(home.path(), "stand-in", &exit_script("nope", 3));
        let err = discover_with(home.path(), &run_script(program, Duration::from_secs(2)))
            .await
            .expect_err("exit 3 text");
        match err {
            DiscoveryError::Ensure(EnsureError::Exit {
                code: 3,
                kind: ExitKind::Incompatible,
                state: None,
                ..
            }) => {}
            _ => panic!("exit 3 non-json"),
        }

        let home = Home::new();
        let err = discover_with(
            home.path(),
            &run_script(PathBuf::from("hermes-absent"), Duration::from_secs(2)),
        )
        .await
        .expect_err("missing program");
        assert!(
            matches!(err, DiscoveryError::Ensure(EnsureError::NotInstalled)),
            "not installed"
        );
        assert_eq!(
            err.next_step(),
            "Install Hermes, or give the path to the `hermes` program.",
            "install step"
        );

        let home = Home::new();
        let canonical = home.canonical();
        let program = write_program(
            home.path(),
            "stand-in",
            "#!/bin/sh\nprintf '%s\\n' \"{\\\"state\\\":\\\"inaccessible\\\",\\\"reason_code\\\":null,\\\"detail\\\":\\\"$HERMES_HOME/secret-leaf\\\",\\\"endpoint\\\":null,\\\"client_code_sha\\\":null}\"\nexit 7\n",
        );
        let err = discover_with(home.path(), &run_script(program, Duration::from_secs(2)))
            .await
            .expect_err("detail");
        match err {
            DiscoveryError::Ensure(EnsureError::Exit {
                detail: Some(detail),
                ..
            }) => {
                no_home(&detail, &canonical);
                assert!(
                    detail.contains("<home>") || detail.contains("<path>"),
                    "detail scrub"
                );
            }
            _ => panic!("detail was not an ensure exit"),
        }
    }

    #[tokio::test]
    async fn ensure_runs_only_when_needed() {
        match DiscoverOptions::default().ensure {
            EnsurePolicy::Run { program, timeout } => {
                assert_eq!(program, Path::new("hermes"), "default program");
                assert_eq!(timeout, Duration::from_secs(90), "default ensure timeout");
            }
            EnsurePolicy::Never => panic!("default policy"),
        }

        let home = Home::new();
        let _program = write_program(home.path(), "stand-in", &ready_script());
        let err = discover_with(
            home.path(),
            &DiscoverOptions {
                control_timeout: Duration::from_secs(1),
                ensure: EnsurePolicy::Never,
            },
        )
        .await
        .expect_err("never");
        assert!(matches!(err, DiscoveryError::NoGateway), "never policy");
        assert!(!home.path().join("marker").exists(), "never ran");

        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let program = write_program(home.path(), "stand-in", &ready_script());
        let socket = home.path().join("gateway.sock");
        let mock = serve(
            &socket,
            {
                let canonical = canonical.clone();
                move |origin| {
                    vec![ControlReply::Line(text(&ready_identify(
                        &canonical, origin,
                    )))]
                }
            },
            WsPlan::Idle,
        )
        .await;
        discover_with(home.path(), &run_script(program, Duration::from_secs(2)))
            .await
            .expect("ready");
        assert!(
            !home.path().join("marker").exists(),
            "ready gateway ran ensure"
        );
        drop(mock);

        let home = Home::new();
        let program = write_program(
            home.path(),
            "stand-in",
            "#!/bin/sh\nprintf '%s\\n' RUN >> \"${0%/*}/run.log\"\nprintf '%s\\n' '{\"state\":\"ready\",\"reason_code\":null,\"detail\":null,\"endpoint\":null,\"client_code_sha\":null}'\nexit 0\n",
        );
        let err = discover_with(home.path(), &run_script(program, Duration::from_secs(2)))
            .await
            .expect_err("still unreachable");
        assert!(
            matches!(err, DiscoveryError::StillUnreachableAfterEnsure),
            "still unreachable"
        );
        assert_eq!(run_count(home.path()), 1, "one run when still unreachable");

        let home = Home::new();
        let program = write_program(home.path(), "stand-in", &ready_script());
        let missing = home.path().join("absent");
        let err = discover_with(&missing, &run_script(program, Duration::from_secs(2)))
            .await
            .expect_err("missing");
        assert!(matches!(err, DiscoveryError::NoGateway), "missing home");
        assert_eq!(run_count(home.path()), 0, "missing home ran ensure");

        let home = Home::new();
        let program = write_program(home.path(), "stand-in", &ready_script());
        let err = discover_with(
            Path::new("relative-home"),
            &run_script(program.clone(), Duration::from_secs(2)),
        )
        .await
        .expect_err("relative");
        assert!(matches!(err, DiscoveryError::NoGateway), "relative home");
        assert!(
            !home.path().join("marker").exists(),
            "relative home ran ensure"
        );

        same(
            &format_failure("ensure", "m", "n"),
            "ATTACH FAILED at ensure: m\nNext step: n",
            "failure lines",
        );
        let _ = program;
    }
}

mod ticket {
    use super::*;

    #[tokio::test]
    async fn mint_ticket_sends_the_exact_request() {
        assert!(Scope::host(Purpose::NativeHttp).is_err(), "host scope");
        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let fixture = format!("{canonical}/profiles/fixture");
        let socket = home.path().join("gateway.sock");
        let mock = serve(
            &socket,
            {
                let canonical = canonical.clone();
                let fixture = fixture.clone();
                move |origin| {
                    vec![
                        ControlReply::Line(text(&ready_identify(&canonical, origin))),
                        ControlReply::Issue {
                            profile_id: fixture.clone(),
                            instance_id: INSTANCE_ID.to_string(),
                            expired: false,
                        },
                        ControlReply::Issue {
                            profile_id: canonical.clone(),
                            instance_id: INSTANCE_ID.to_string(),
                            expired: false,
                        },
                        ControlReply::Issue {
                            profile_id: fixture,
                            instance_id: INSTANCE_ID.to_string(),
                            expired: false,
                        },
                        ControlReply::Silence,
                    ]
                }
            },
            WsPlan::Idle,
        )
        .await;
        let endpoint = discover_with(home.path(), &never())
            .await
            .expect("discover");
        let profile = endpoint.resolve_profile("fixture").expect("fixture");
        let before = mock.connections();
        set_mode(&socket, 0o666);
        let err = mint_ticket(
            home.path(),
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .expect_err("open socket");
        assert!(
            matches!(
                err,
                TicketError::UnsafeNode {
                    node: NodeKind::Socket,
                    problem: NodeProblem::OpenToOthers
                }
            ),
            "mint checks the socket"
        );
        assert_eq!(mock.connections(), before, "unsafe mint connected");
        set_mode(&socket, 0o600);

        let err = mint_ticket_with(
            home.path(),
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
            &TicketOptions {
                control_timeout: Duration::from_millis(200),
            },
        )
        .await
        .expect_err("silent mint");
        assert!(matches!(err, TicketError::Timeout), "mint timeout");

        let _ = mint_ticket(
            home.path(),
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .expect("interactive");
        let _ = mint_ticket(
            home.path(),
            &endpoint,
            &profile,
            Purpose::NativeHttp,
            Scope::profile(),
        )
        .await
        .expect("native");
        let host = Scope::host(Purpose::Interactive).expect("host");
        let host_profile: ProfileId = canonical.clone().into();
        let _ = mint_ticket(
            home.path(),
            &endpoint,
            &host_profile,
            Purpose::Interactive,
            host,
        )
        .await
        .expect("host ticket");
        let requests = mock.requests();
        assert_ticket_line(
            &requests[1],
            &fixture,
            INSTANCE_ID,
            "interactive",
            "profile",
        );
        assert_ticket_line(
            &requests[2],
            &fixture,
            INSTANCE_ID,
            "native-http",
            "profile",
        );
        assert_ticket_line(&requests[3], &canonical, INSTANCE_ID, "interactive", "host");
        let host = Scope::host(Purpose::Interactive).expect("host again");
        let seen = mock.connections();
        let err = mint_ticket(home.path(), &endpoint, &profile, Purpose::NativeHttp, host)
            .await
            .expect_err("scope");
        assert!(matches!(err, TicketError::ScopeMismatch), "scope mismatch");
        assert_eq!(
            err.to_string(),
            "a host ticket needs the interactive purpose",
            "scope text"
        );
        assert_eq!(err.next_step(), "Report a bug.", "scope step");
        assert_eq!(mock.connections(), seen, "scope mismatch connected");
    }

    #[tokio::test]
    async fn ticket_is_a_secret() {
        static_assertions::assert_not_impl_any!(Ticket: std::fmt::Display, Clone, serde::Serialize);
        let ticket = Ticket::parse("a".repeat(43)).expect("parse");
        assert_eq!(format!("{ticket:?}"), "Ticket(<redacted>)");
        assert_eq!(format!("{ticket:#?}"), "Ticket(<redacted>)");
    }

    #[tokio::test]
    async fn mint_ticket_classifies_replies() {
        let texts = [
            ("PermissionError: stale runtime instance", "stale"),
            ("PermissionError: bootstrap binding rejected", "binding"),
            (
                "PermissionError: authenticated runtime bootstrap unavailable",
                "auth",
            ),
            ("PermissionError: bootstrap capacity exhausted", "capacity"),
            ("ValueError: invalid bootstrap envelope", "envelope"),
            ("ValueError: invalid bootstrap parameters", "parameters"),
            ("ValueError: invalid bootstrap subject", "subject"),
            ("Oops: A: B: C", "split"),
        ];
        for (error, name) in texts {
            let home = Home::new();
            ensure_fixture(home.path());
            let canonical = home.canonical();
            let body = json!({"ok": false, "error": error, "protocol": 1, "id": 2});
            let socket = home.path().join("gateway.sock");
            let _mock = serve(
                &socket,
                {
                    let canonical = canonical.clone();
                    move |origin| {
                        vec![
                            ControlReply::Line(text(&ready_identify(&canonical, origin))),
                            ControlReply::Line(text(&body)),
                        ]
                    }
                },
                WsPlan::Idle,
            )
            .await;
            let endpoint = discover_with(home.path(), &never())
                .await
                .expect("discover");
            let profile = endpoint.resolve_profile("fixture").expect("fixture");
            let err = mint_ticket(
                home.path(),
                &endpoint,
                &profile,
                Purpose::Interactive,
                Scope::profile(),
            )
            .await
            .expect_err(name);
            match name {
                "stale" => assert!(matches!(err, TicketError::StaleInstance), "{name}"),
                "binding" => assert!(matches!(err, TicketError::BindingRejected), "{name}"),
                "auth" => {
                    assert!(matches!(err, TicketError::NotAuthenticated), "{name}");
                    assert_eq!(
                        err.next_step(),
                        "Run Talaria as the user who runs the gateway; if the Hermes home is writable by its group, run `chmod g-w` on it."
                    );
                }
                "capacity" => assert!(matches!(err, TicketError::CapacityExhausted), "{name}"),
                "envelope" | "parameters" | "subject" => {
                    assert!(matches!(err, TicketError::InvalidRequest), "{name}")
                }
                "split" => match err {
                    TicketError::Refused(text) => assert_eq!(text, "B: C", "split"),
                    _ => panic!("split"),
                },
                _ => panic!("row"),
            }
        }

        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let long = format!("Nope: {}", "p".repeat(400));
        let stuffed = format!("Nope: see {canonical}/note");
        for (name, error) in [
            ("missing", None),
            ("number", Some(json!(1))),
            ("empty", Some(json!(""))),
            ("unknown", Some(json!(long))),
            ("path", Some(json!(stuffed))),
        ] {
            let body = match error {
                None => json!({"ok": false, "protocol": 1, "id": 2}),
                Some(error) => json!({"ok": false, "error": error, "protocol": 1, "id": 2}),
            };
            let socket = home.path().join("gateway.sock");
            let mock = serve(
                &socket,
                {
                    let canonical = canonical.clone();
                    move |origin| {
                        vec![
                            ControlReply::Line(text(&ready_identify(&canonical, origin))),
                            ControlReply::Line(text(&body)),
                        ]
                    }
                },
                WsPlan::Idle,
            )
            .await;
            let endpoint = discover_with(home.path(), &never()).await.expect(name);
            let profile = endpoint.resolve_profile("fixture").expect("fixture");
            let err = mint_ticket(
                home.path(),
                &endpoint,
                &profile,
                Purpose::Interactive,
                Scope::profile(),
            )
            .await
            .expect_err(name);
            match name {
                "missing" | "number" | "empty" => {
                    assert!(matches!(err, TicketError::InvalidReply), "{name}")
                }
                "unknown" => match err {
                    TicketError::Refused(text) => {
                        assert!(text.chars().count() <= 200, "cut");
                        no_home(&text, &canonical);
                    }
                    _ => panic!("unknown refusal"),
                },
                "path" => match err {
                    TicketError::Refused(text) => no_home(&text, &canonical),
                    _ => panic!("path refusal"),
                },
                _ => panic!("row"),
            }
            drop(mock);
        }

        for name in ["instance", "profile", "protocol", "id"] {
            let home = Home::new();
            ensure_fixture(home.path());
            let canonical = home.canonical();
            let mut body = json!({
                "ok": true,
                "protocol": 1,
                "id": 2,
                "result": {
                    "ticket": "a".repeat(43),
                    "expires_in_seconds": 30,
                    "instance_id": INSTANCE_ID,
                    "profile_id": format!("{canonical}/profiles/fixture"),
                    "runtime_protocol": 1
                }
            });
            match name {
                "instance" => {
                    body["result"]["instance_id"] = json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
                }
                "profile" => body["result"]["profile_id"] = json!("other"),
                "protocol" => body["result"]["runtime_protocol"] = json!(2),
                "id" => body["id"] = json!(9),
                _ => {}
            }
            let socket = home.path().join("gateway.sock");
            let _mock = serve(
                &socket,
                {
                    let canonical = canonical.clone();
                    move |origin| {
                        vec![
                            ControlReply::Line(text(&ready_identify(&canonical, origin))),
                            ControlReply::Line(text(&body)),
                        ]
                    }
                },
                WsPlan::Idle,
            )
            .await;
            let endpoint = discover_with(home.path(), &never()).await.expect(name);
            let profile = endpoint.resolve_profile("fixture").expect("fixture");
            let err = mint_ticket(
                home.path(),
                &endpoint,
                &profile,
                Purpose::Interactive,
                Scope::profile(),
            )
            .await
            .expect_err(name);
            if name == "id" {
                assert!(matches!(err, TicketError::InvalidReply), "{name}");
            } else {
                assert!(matches!(err, TicketError::IdentityChanged), "{name}");
            }
        }

        assert!(
            matches!(Ticket::parse(String::new()), Err(TicketError::Malformed)),
            "empty"
        );
        assert!(
            matches!(Ticket::parse("a".repeat(257)), Err(TicketError::Malformed)),
            "long"
        );
        for bad in [" ", ",", ";", "\n", "\"", "+", "/", "=", ".", "~", "!", "é"] {
            let token = format!("aaaa{bad}aaaa");
            assert!(
                matches!(Ticket::parse(token), Err(TicketError::Malformed)),
                "bad character"
            );
        }
        assert!(Ticket::parse("a".repeat(256)).is_ok(), "max token");
    }
}

mod websocket {
    use super::*;

    #[tokio::test]
    async fn connect_offers_both_subprotocols_and_no_origin() {
        assert_eq!(
            ws_url("http://127.0.0.1:4567"),
            "ws://127.0.0.1:4567/api/ws"
        );
        assert_eq!(ws_url("http://[::1]:4567"), "ws://[::1]:4567/api/ws");
        assert_eq!(
            ws_url("https://127.0.0.1:4567"),
            "wss://127.0.0.1:4567/api/ws"
        );

        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let fixture = format!("{canonical}/profiles/fixture");
        let socket = home.path().join("gateway.sock");
        let mock = serve(
            &socket,
            {
                let canonical = canonical.clone();
                move |origin| {
                    vec![
                        ControlReply::Line(text(&ready_identify(&canonical, origin))),
                        ControlReply::Issue {
                            profile_id: fixture.clone(),
                            instance_id: INSTANCE_ID.into(),
                            expired: false,
                        },
                        ControlReply::Issue {
                            profile_id: fixture.clone(),
                            instance_id: INSTANCE_ID.into(),
                            expired: false,
                        },
                        ControlReply::Issue {
                            profile_id: fixture,
                            instance_id: INSTANCE_ID.into(),
                            expired: false,
                        },
                    ]
                }
            },
            WsPlan::Talk {
                echo: Echo::Version1,
                steps: Vec::new(),
                guard: false,
            },
        )
        .await;
        let endpoint = discover_with(home.path(), &never())
            .await
            .expect("discover");
        let profile = endpoint.resolve_profile("fixture").expect("fixture");
        let ticket = mint_ticket(
            home.path(),
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .expect("ticket");
        let _ = connect(&endpoint, ticket).await.expect("connect");
        let seen = mock.seen().expect("upgrade");
        same(&seen.path, "/api/ws", "path");
        same(
            &seen.host,
            mock.origin().trim_start_matches("http://"),
            "host",
        );
        assert!(seen.origin.is_none(), "origin header");
        assert_eq!(seen.protocols.len(), 2, "protocols");
        same(
            seen.protocols.first().map(String::as_str).unwrap_or(""),
            "hermes-gateway-v1",
            "version",
        );
        assert!(
            seen.protocols
                .get(1)
                .is_some_and(|item| item.starts_with("hermes-gateway-ticket.")),
            "ticket protocol"
        );

        let https_endpoint = Endpoint::from_identify(
            &ready_identify(&canonical, "https://127.0.0.1:9"),
            Path::new(&canonical),
        )
        .expect("https");
        let before = mock.attempts();
        let ticket = mint_ticket(
            home.path(),
            &https_endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .expect("https ticket");
        let err = connect(&https_endpoint, ticket).await.expect_err("tls");
        assert!(matches!(err, ConnectError::TlsNotSupported), "tls");
        assert_eq!(
            err.next_step(),
            "Talaria supports only a gateway on this machine over plain HTTP.",
            "tls step"
        );
        assert_eq!(mock.attempts(), before, "tls dialled");

        let native = mint_ticket(
            home.path(),
            &endpoint,
            &profile,
            Purpose::NativeHttp,
            Scope::profile(),
        )
        .await
        .expect("native");
        let before = mock.attempts();
        let err = connect(&endpoint, native).await.expect_err("purpose");
        assert!(matches!(err, ConnectError::WrongPurpose), "wrong purpose");
        assert_eq!(mock.attempts(), before, "native dialled");
    }

    #[tokio::test]
    async fn connect_rejects_a_wrong_or_missing_echoed_subprotocol() {
        for echo in [
            Echo::Text("hermes-gateway-v2".to_string()),
            Echo::Ticket,
            Echo::Absent,
        ] {
            let home = Home::new();
            ensure_fixture(home.path());
            let canonical = home.canonical();
            let fixture = format!("{canonical}/profiles/fixture");
            let socket = home.path().join("gateway.sock");
            let _mock = serve(
                &socket,
                {
                    let canonical = canonical.clone();
                    let fixture = fixture.clone();
                    move |origin| {
                        vec![
                            ControlReply::Line(text(&ready_identify(&canonical, origin))),
                            ControlReply::Issue {
                                profile_id: fixture,
                                instance_id: INSTANCE_ID.into(),
                                expired: false,
                            },
                        ]
                    }
                },
                WsPlan::Talk {
                    echo,
                    steps: Vec::new(),
                    guard: false,
                },
            )
            .await;
            let endpoint = discover_with(home.path(), &never())
                .await
                .expect("discover");
            let profile = endpoint.resolve_profile("fixture").expect("fixture");
            let ticket = mint_ticket(
                home.path(),
                &endpoint,
                &profile,
                Purpose::Interactive,
                Scope::profile(),
            )
            .await
            .expect("ticket");
            let err = connect(&endpoint, ticket).await.expect_err("echo");
            assert!(
                matches!(err, ConnectError::SubprotocolMismatch),
                "subprotocol"
            );
        }
    }

    #[tokio::test]
    async fn connect_maps_refusals_and_hides_the_ticket() {
        for (name, plan, timeout_ms, check) in [
            ("403", WsPlan::Status(403), 15_000u64, "403"),
            ("500", WsPlan::Status(500), 15_000, "500"),
            ("drop", WsPlan::Drop, 15_000, "io"),
            ("silent", WsPlan::Silence, 300, "timeout"),
        ] {
            let home = Home::new();
            ensure_fixture(home.path());
            let canonical = home.canonical();
            let fixture = format!("{canonical}/profiles/fixture");
            let socket = home.path().join("gateway.sock");
            let mock = serve(
                &socket,
                {
                    let canonical = canonical.clone();
                    let fixture = fixture.clone();
                    move |origin| {
                        vec![
                            ControlReply::Line(text(&ready_identify(&canonical, origin))),
                            ControlReply::Issue {
                                profile_id: fixture,
                                instance_id: INSTANCE_ID.into(),
                                expired: false,
                            },
                        ]
                    }
                },
                plan,
            )
            .await;
            let endpoint = discover_with(home.path(), &never()).await.expect(name);
            let profile = endpoint.resolve_profile("fixture").expect("fixture");
            let ticket = mint_ticket(
                home.path(),
                &endpoint,
                &profile,
                Purpose::Interactive,
                Scope::profile(),
            )
            .await
            .expect("ticket");
            let secret = mock.issued().first().cloned().unwrap_or_default();
            let err = connect_with(
                &endpoint,
                ticket,
                &ConnectOptions {
                    timeout: Duration::from_millis(timeout_ms),
                },
            )
            .await
            .expect_err(name);
            match check {
                "403" => assert!(
                    matches!(err, ConnectError::UpgradeRejected { status: Some(403) }),
                    "{name}"
                ),
                "500" => assert!(
                    matches!(err, ConnectError::UpgradeRejected { status: Some(500) }),
                    "{name}"
                ),
                "io" => assert!(matches!(err, ConnectError::Io(_)), "{name}"),
                "timeout" => assert!(matches!(err, ConnectError::Timeout), "{name}"),
                _ => panic!("row"),
            }
            hides_secret(
                &[err.to_string(), format!("{err:?}"), format!("{err:#?}")],
                &secret,
            );
            assert_step(err.next_step());
            assert_plain_text(&err.to_string());
        }
    }

    #[tokio::test]
    async fn unknown_used_or_expired_ticket_is_an_upgrade_refusal() {
        let unknown = Ticket::parse("b".repeat(43)).expect("unknown");
        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let fixture = format!("{canonical}/profiles/fixture");
        let socket = home.path().join("gateway.sock");
        let mock = serve(
            &socket,
            {
                let canonical = canonical.clone();
                let fixture = fixture.clone();
                move |origin| {
                    vec![
                        ControlReply::Line(text(&ready_identify(&canonical, origin))),
                        ControlReply::Issue {
                            profile_id: fixture.clone(),
                            instance_id: INSTANCE_ID.into(),
                            expired: false,
                        },
                        ControlReply::Issue {
                            profile_id: fixture,
                            instance_id: INSTANCE_ID.into(),
                            expired: true,
                        },
                    ]
                }
            },
            WsPlan::Talk {
                echo: Echo::Version1,
                steps: success_steps(&fixture),
                guard: true,
            },
        )
        .await;
        let endpoint = discover_with(home.path(), &never())
            .await
            .expect("discover");
        let err = connect(&endpoint, unknown).await.expect_err("unknown");
        assert!(
            matches!(err, ConnectError::UpgradeRejected { status: Some(403) }),
            "unknown"
        );

        let profile = endpoint.resolve_profile("fixture").expect("fixture");
        let first = mint_ticket(
            home.path(),
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .expect("mint");
        let secret = mock.issued().first().cloned().unwrap_or_default();
        connect(&endpoint, first).await.expect("first upgrade");
        let again = Ticket::parse(secret).expect("reparse");
        let err = connect(&endpoint, again).await.expect_err("used");
        assert!(
            matches!(err, ConnectError::UpgradeRejected { status: Some(403) }),
            "used"
        );

        let _expired = mint_ticket(
            home.path(),
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .expect("expired mint");
        let secret = mock.issued().get(1).cloned().unwrap_or_default();
        let expired = Ticket::parse(secret).expect("expired parse");
        let err = connect(&endpoint, expired).await.expect_err("expired");
        assert!(
            matches!(err, ConnectError::UpgradeRejected { status: Some(403) }),
            "expired"
        );
    }
}

mod handshake {
    use super::*;

    fn caps_frame() -> String {
        let name = hermes_contract::methods::legacy::client_capabilities::NAME;
        format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"{name}","params":{{"server_requests":true}}}}"#
        )
    }

    fn describe_frame() -> String {
        let name = hermes_contract::methods::canonical::runtime_describe::NAME;
        format!(r#"{{"jsonrpc":"2.0","id":2,"method":"{name}","params":{{}}}}"#)
    }

    async fn opened(plan: WsPlan) -> (Home, AttachMock, hermes_client::Connection) {
        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let fixture = format!("{canonical}/profiles/fixture");
        let socket = home.path().join("gateway.sock");
        let mock = serve(
            &socket,
            {
                let canonical = canonical.clone();
                let fixture = fixture.clone();
                move |origin| {
                    vec![
                        ControlReply::Line(text(&ready_identify(&canonical, origin))),
                        ControlReply::Issue {
                            profile_id: fixture,
                            instance_id: INSTANCE_ID.into(),
                            expired: false,
                        },
                    ]
                }
            },
            plan,
        )
        .await;
        let endpoint = discover_with(home.path(), &never())
            .await
            .expect("discover");
        let profile = endpoint.resolve_profile("fixture").expect("fixture");
        let ticket = mint_ticket(
            home.path(),
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .expect("ticket");
        let connection = connect(&endpoint, ticket).await.expect("connect");
        (home, mock, connection)
    }

    #[tokio::test]
    async fn handshake_sends_capabilities_then_describe_in_order() {
        let canonical_holder = Home::new();
        let canonical = canonical_holder.canonical();
        let fixture = format!("{canonical}/profiles/fixture");
        drop(canonical_holder);
        let steps = vec![
            WsStep::Wait(Duration::from_millis(100)),
            WsStep::Send(frame_ready()),
            WsStep::Take,
            WsStep::Send(frame_capabilities()),
            WsStep::Take,
            WsStep::Send(frame_describe(&fixture)),
        ];
        let (_home, mock, mut connection) = opened(WsPlan::Talk {
            echo: Echo::Version1,
            steps,
            guard: false,
        })
        .await;
        let done = handshake(&mut connection).await.expect("handshake");
        same(&done.ready.replay_epoch, "0123abcd", "epoch");
        assert_eq!(done.ready.session_authority, Some(true), "authority");
        same(&done.describe.instance_id, INSTANCE_ID, "describe instance");
        let _ = done.timings.ready;
        let _ = done.timings.capabilities;
        let _ = done.timings.describe;
        let frames = mock.frames();
        same(
            frames.first().map(String::as_str).unwrap_or(""),
            &caps_frame(),
            "capabilities frame",
        );
        same(
            frames.get(1).map(String::as_str).unwrap_or(""),
            &describe_frame(),
            "describe frame",
        );
        assert_eq!(
            frames
                .iter()
                .filter(|frame| frame
                    .contains(hermes_contract::methods::legacy::client_capabilities::NAME))
                .count(),
            1,
            "once"
        );
    }

    #[tokio::test]
    async fn handshake_keeps_frames_it_did_not_consume() {
        let early = r#"{"jsonrpc":"2.0","method":"event","params":{"type":"sessions.changed","payload":{}}}"#;
        let between = r#"{"jsonrpc":"2.0","method":"event","params":{"type":"platforms.changed","payload":{}}}"#;
        let server = r#"{"jsonrpc":"2.0","id":"srq-1","method":"approval.request","params":{}}"#;
        let method_id = r#"{"jsonrpc":"2.0","id":1,"method":"x.y"}"#;
        let string_id = r#"{"jsonrpc":"2.0","id":"1","result":{}}"#;
        let late = r#"{"jsonrpc":"2.0","id":1,"result":{"server_requests":[]}}"#;
        let home = Home::new();
        let fixture = format!("{}/profiles/fixture", home.canonical());
        let steps = vec![
            WsStep::Send(early.to_string()),
            WsStep::Send(frame_ready()),
            WsStep::Take,
            WsStep::Send(between.to_string()),
            WsStep::Send(frame_capabilities()),
            WsStep::Send(server.to_string()),
            WsStep::Send(method_id.to_string()),
            WsStep::Take,
            WsStep::Send(string_id.to_string()),
            WsStep::Send(late.to_string()),
            WsStep::Send(frame_describe(&fixture)),
        ];
        let (_home, _mock, mut connection) = opened(WsPlan::Talk {
            echo: Echo::Version1,
            steps,
            guard: false,
        })
        .await;
        let _ = handshake(&mut connection).await.expect("handshake");
        let parts = connection.into_parts();
        assert_eq!(parts.next_request_id, 3, "next id");
        if parts.buffered != [early, between, server, method_id] {
            panic!("buffered frames");
        }
        assert!(parts.profile_id.is_some(), "profile");
        same(&parts.instance_id, INSTANCE_ID, "instance");
    }

    #[tokio::test]
    async fn handshake_validates_ready_and_describe() {
        let home = Home::new();
        let fixture = format!("{}/profiles/fixture", home.canonical());
        drop(home);
        let base_ready = serde_json::from_str::<Value>(&frame_ready()).expect("ready json");
        let mut extra = base_ready.clone();
        extra["params"]["payload"]["extra"] = json!(true);
        let describe = frame_describe(&fixture);
        let ok_steps = vec![
            WsStep::Send(extra.to_string()),
            WsStep::Take,
            WsStep::Send(frame_capabilities()),
            WsStep::Take,
            WsStep::Send(describe.clone()),
        ];
        let (_home, _mock, mut connection) = opened(WsPlan::Talk {
            echo: Echo::Version1,
            steps: ok_steps,
            guard: false,
        })
        .await;
        handshake(&mut connection).await.expect("extra field");

        let cases = [
            "no epoch",
            "no skin",
            "authority false",
            "authority missing",
            "identity",
            "no session",
            "describe error",
            "reason space",
            "reason long",
        ];
        for name in cases {
            let mut ready = base_ready.clone();
            let mut describe_frame =
                serde_json::from_str::<Value>(&describe).expect("describe json");
            match name {
                "no epoch" => {
                    ready["params"]["payload"]
                        .as_object_mut()
                        .map(|obj| obj.remove("replay_epoch"));
                }
                "no skin" => {
                    ready["params"]["payload"]
                        .as_object_mut()
                        .map(|obj| obj.remove("skin"));
                }
                "authority false" => ready["params"]["payload"]["session_authority"] = json!(false),
                "authority missing" => {
                    ready["params"]["payload"]
                        .as_object_mut()
                        .map(|obj| obj.remove("session_authority"));
                }
                "identity" => {
                    describe_frame["result"]["instance_id"] =
                        json!("cccccccccccccccccccccccccccccccc")
                }
                "no session" => {
                    describe_frame["result"]
                        .as_object_mut()
                        .map(|obj| obj.remove("session_create"));
                }
                "describe error" => {
                    describe_frame = json!({"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"no","data":{"reason":"not_ready"}}});
                }
                "reason space" => {
                    describe_frame = json!({"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"no","data":{"reason":"not ready"}}});
                }
                "reason long" => {
                    describe_frame = json!({"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"no","data":{"reason":"a".repeat(81)}}});
                }
                _ => {}
            }
            let steps = vec![
                WsStep::Send(ready.to_string()),
                WsStep::Take,
                WsStep::Send(frame_capabilities()),
                WsStep::Take,
                WsStep::Send(describe_frame.to_string()),
            ];
            let (_home, _mock, mut connection) = opened(WsPlan::Talk {
                echo: Echo::Version1,
                steps,
                guard: false,
            })
            .await;
            let err = handshake(&mut connection).await.expect_err(name);
            match name {
                "no epoch" | "no skin" => {
                    assert!(matches!(err, HandshakeError::BadReady), "{name}")
                }
                "authority false" | "authority missing" => {
                    assert!(matches!(err, HandshakeError::NotSessionAuthority), "{name}")
                }
                "identity" => assert!(matches!(err, HandshakeError::IdentityChanged), "{name}"),
                "no session" => assert!(matches!(err, HandshakeError::BadDescribe), "{name}"),
                "describe error" => match err {
                    HandshakeError::DescribeFailed {
                        code: -32000,
                        reason: Some(reason),
                    } => same(&reason, "not_ready", "reason"),
                    _ => panic!("describe error"),
                },
                "reason space" | "reason long" => match err {
                    HandshakeError::DescribeFailed { reason: None, .. } => {}
                    _ => panic!("reason dropped"),
                },
                _ => panic!("row"),
            }
        }
    }

    #[tokio::test]
    async fn handshake_reports_closes_and_bad_frames() {
        let samples = [
            HandshakeError::ReadyTimeout,
            HandshakeError::CallTimeout {
                method: "runtime.describe",
            },
            HandshakeError::CallTimeout {
                method: "client.capabilities",
            },
            HandshakeError::BadReady,
            HandshakeError::BadFrame,
            HandshakeError::NotSessionAuthority,
            HandshakeError::IdentityChanged,
            HandshakeError::BadDescribe,
            HandshakeError::DescribeFailed {
                code: 1,
                reason: Some("reason".to_string()),
            },
            HandshakeError::DescribeFailed {
                code: -1,
                reason: None,
            },
            HandshakeError::Closed(Closed::Overloaded),
            HandshakeError::Closed(Closed::Dropped),
            HandshakeError::Io(std::io::ErrorKind::BrokenPipe),
        ];
        for err in samples {
            assert_plain_text(&err.to_string());
            assert_step(err.next_step());
        }

        for (name, step) in [
            (
                "overloaded",
                WsStep::Close {
                    code: 1011,
                    reason: String::new(),
                },
            ),
            (
                "dropped",
                WsStep::Close {
                    code: 1000,
                    reason: String::new(),
                },
            ),
            ("abrupt", WsStep::Drop),
            ("text", WsStep::Send("not-json".to_string())),
            (
                "no method",
                WsStep::Send(r#"{"jsonrpc":"2.0"}"#.to_string()),
            ),
            (
                "id only",
                WsStep::Send(r#"{"jsonrpc":"2.0","id":9}"#.to_string()),
            ),
        ] {
            let steps = vec![step];
            let (_home, _mock, mut connection) = opened(WsPlan::Talk {
                echo: Echo::Version1,
                steps,
                guard: false,
            })
            .await;
            let err = handshake(&mut connection).await.expect_err(name);
            match name {
                "overloaded" => assert!(
                    matches!(err, HandshakeError::Closed(Closed::Overloaded)),
                    "{name}"
                ),
                "dropped" | "abrupt" => assert!(
                    matches!(err, HandshakeError::Closed(Closed::Dropped)),
                    "{name}"
                ),
                _ => assert!(matches!(err, HandshakeError::BadFrame), "{name}"),
            }
        }

        let home = Home::new();
        let fixture = format!("{}/profiles/fixture", home.canonical());
        drop(home);
        let steps = vec![
            WsStep::Send(frame_ready()),
            WsStep::Binary,
            WsStep::Send(r#"{"jsonrpc":"2.0","id":9,"result":{}}"#.to_string()),
            WsStep::Send(r#"{"jsonrpc":"2.0","id":1,"method":"x.y"}"#.to_string()),
            WsStep::Take,
            WsStep::Send(frame_capabilities()),
            WsStep::Take,
            WsStep::Send(frame_describe(&fixture)),
        ];
        let (_home, _mock, mut connection) = opened(WsPlan::Talk {
            echo: Echo::Version1,
            steps,
            guard: false,
        })
        .await;
        handshake(&mut connection).await.expect("ignored frames");
        let parts = connection.into_parts();
        assert_eq!(parts.buffered.len(), 1, "method frame kept");
    }

    #[tokio::test]
    async fn handshake_deadlines_and_capabilities_failures() {
        let defaults = HandshakeOptions::default();
        assert_eq!(
            defaults.ready_timeout,
            Duration::from_secs(15),
            "ready default"
        );
        assert_eq!(
            defaults.call_timeout,
            Duration::from_secs(15),
            "call default"
        );

        let (_home, _mock, mut connection) = opened(WsPlan::Silence).await;
        let err = handshake_with(
            &mut connection,
            &HandshakeOptions {
                ready_timeout: Duration::from_millis(300),
                call_timeout: Duration::from_secs(15),
            },
        )
        .await
        .expect_err("silent");
        assert!(matches!(err, HandshakeError::ReadyTimeout), "ready timeout");

        let home = Home::new();
        let fixture = format!("{}/profiles/fixture", home.canonical());
        drop(home);
        let error_reply = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"missing"}}"#;
        let steps = vec![
            WsStep::Send(frame_ready()),
            WsStep::Take,
            WsStep::Send(error_reply.to_string()),
            WsStep::Take,
            WsStep::Send(frame_describe(&fixture)),
        ];
        let (_home, _mock, mut connection) = opened(WsPlan::Talk {
            echo: Echo::Version1,
            steps,
            guard: false,
        })
        .await;
        let done = handshake(&mut connection)
            .await
            .expect("capabilities error");
        assert!(
            done.server_requests.is_none(),
            "capabilities error continues"
        );

        let steps = vec![
            WsStep::Send(frame_ready()),
            WsStep::Take,
            WsStep::Take,
            WsStep::Send(frame_describe(&fixture)),
        ];
        let (_home, _mock, mut connection) = opened(WsPlan::Talk {
            echo: Echo::Version1,
            steps,
            guard: false,
        })
        .await;
        let done = handshake_with(
            &mut connection,
            &HandshakeOptions {
                ready_timeout: Duration::from_secs(15),
                call_timeout: Duration::from_millis(300),
            },
        )
        .await
        .expect("capabilities silence");
        assert!(
            done.server_requests.is_none(),
            "capabilities timeout continues"
        );

        let steps = vec![
            WsStep::Send(frame_ready()),
            WsStep::Take,
            WsStep::Send(frame_capabilities()),
            WsStep::Take,
        ];
        let (_home, _mock, mut connection) = opened(WsPlan::Talk {
            echo: Echo::Version1,
            steps,
            guard: false,
        })
        .await;
        let err = handshake_with(
            &mut connection,
            &HandshakeOptions {
                ready_timeout: Duration::from_secs(15),
                call_timeout: Duration::from_millis(300),
            },
        )
        .await
        .expect_err("describe silence");
        assert!(
            matches!(
                err,
                HandshakeError::CallTimeout {
                    method: "runtime.describe"
                }
            ),
            "describe timeout"
        );

        for (name, code) in [("overloaded", 1011u16), ("dropped", 1000u16)] {
            let steps = vec![
                WsStep::Send(frame_ready()),
                WsStep::Take,
                WsStep::Close {
                    code,
                    reason: String::new(),
                },
            ];
            let (_home, _mock, mut connection) = opened(WsPlan::Talk {
                echo: Echo::Version1,
                steps,
                guard: false,
            })
            .await;
            let err = handshake(&mut connection).await.expect_err(name);
            if code == 1011 {
                assert!(
                    matches!(err, HandshakeError::Closed(Closed::Overloaded)),
                    "{name}"
                );
            } else {
                assert!(
                    matches!(err, HandshakeError::Closed(Closed::Dropped)),
                    "{name}"
                );
            }
        }

        for name in ["neither", "bad code"] {
            let reply = if name == "neither" {
                r#"{"jsonrpc":"2.0","id":2}"#.to_string()
            } else {
                r#"{"jsonrpc":"2.0","id":2,"error":{"code":"bad"}}"#.to_string()
            };
            let steps = vec![
                WsStep::Send(frame_ready()),
                WsStep::Take,
                WsStep::Send(frame_capabilities()),
                WsStep::Take,
                WsStep::Send(reply),
            ];
            let (_home, _mock, mut connection) = opened(WsPlan::Talk {
                echo: Echo::Version1,
                steps,
                guard: false,
            })
            .await;
            let err = handshake(&mut connection).await.expect_err(name);
            assert!(matches!(err, HandshakeError::BadDescribe), "{name}");
        }
    }

    #[tokio::test]
    async fn handshake_methods_are_native_methods() {
        fn assert_native<M: hermes_contract::native::NativeMethod>() {}
        assert_native::<hermes_contract::methods::legacy::client_capabilities::Marker>();
        assert_native::<hermes_contract::methods::canonical::runtime_describe::Marker>();
        let home = Home::new();
        let fixture = format!("{}/profiles/fixture", home.canonical());
        drop(home);
        let steps = success_steps(&fixture);
        let (_home, mock, mut connection) = opened(WsPlan::Talk {
            echo: Echo::Version1,
            steps,
            guard: false,
        })
        .await;
        let _ = handshake(&mut connection).await.expect("handshake");
        let frames = mock.frames();
        same(
            frames.first().map(String::as_str).unwrap_or(""),
            &caps_frame(),
            "capabilities name",
        );
        same(
            frames.get(1).map(String::as_str).unwrap_or(""),
            &describe_frame(),
            "describe name",
        );
    }
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config {
            cases: 32,
            failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
            ..proptest::test_runner::Config::default()
        })]
        #[test]
        fn ticket_text_never_appears_in_any_error(secret in "[A-Za-z0-9_-]{16,256}") {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
            runtime.block_on(async {
                check_status(&secret).await;
                check_subprotocol(&secret).await;
                check_silent(&secret).await;
                check_close(&secret).await;
            });
        }
    }

    async fn endpoint_for(secret_plan: WsPlan) -> (Home, AttachMock, Endpoint) {
        let home = Home::new();
        ensure_fixture(home.path());
        let canonical = home.canonical();
        let socket = home.path().join("gateway.sock");
        let mock = serve(
            &socket,
            move |origin| {
                vec![ControlReply::Line(text(&ready_identify(
                    &canonical, origin,
                )))]
            },
            secret_plan,
        )
        .await;
        let endpoint = discover_with(home.path(), &never())
            .await
            .expect("discover");
        (home, mock, endpoint)
    }

    async fn check_status(secret: &str) {
        let (_home, _mock, endpoint) = endpoint_for(WsPlan::Status(403)).await;
        let ticket = Ticket::parse(secret.to_string()).expect("parse");
        let err = connect(&endpoint, ticket).await.expect_err("status");
        hides_secret(
            &[err.to_string(), format!("{err:?}"), format!("{err:#?}")],
            secret,
        );
    }

    async fn check_subprotocol(secret: &str) {
        let (_home, _mock, endpoint) = endpoint_for(WsPlan::Talk {
            echo: Echo::Text(secret.to_string()),
            steps: Vec::new(),
            guard: false,
        })
        .await;
        let ticket = Ticket::parse(secret.to_string()).expect("parse");
        let err = connect(&endpoint, ticket).await.expect_err("subprotocol");
        hides_secret(
            &[err.to_string(), format!("{err:?}"), format!("{err:#?}")],
            secret,
        );
    }

    async fn check_silent(secret: &str) {
        let (_home, _mock, endpoint) = endpoint_for(WsPlan::Silence).await;
        let ticket = Ticket::parse(secret.to_string()).expect("parse");
        let err = connect_with(
            &endpoint,
            ticket,
            &ConnectOptions {
                timeout: Duration::from_millis(200),
            },
        )
        .await
        .expect_err("silent");
        hides_secret(
            &[err.to_string(), format!("{err:?}"), format!("{err:#?}")],
            secret,
        );
    }

    async fn check_close(secret: &str) {
        let (_home, _mock, endpoint) = endpoint_for(WsPlan::Talk {
            echo: Echo::Version1,
            steps: vec![WsStep::Close {
                code: 1011,
                reason: secret.to_string(),
            }],
            guard: false,
        })
        .await;
        let ticket = Ticket::parse(secret.to_string()).expect("parse");
        let mut connection = connect(&endpoint, ticket).await.expect("connect");
        let err = handshake(&mut connection).await.expect_err("close");
        hides_secret(
            &[err.to_string(), format!("{err:?}"), format!("{err:#?}")],
            secret,
        );
    }
}
