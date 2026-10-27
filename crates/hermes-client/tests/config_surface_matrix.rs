mod config_probe;
use config_probe::*;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn matrix() -> Value {
    serde_json::from_slice(
        &fs::read(root().join("fixtures/configuration-surface/matrix.json"))
            .expect("matrix missing"),
    )
    .expect("invalid matrix")
}
fn rows(v: &Value) -> &[Value] {
    v["rows"].as_array().expect("rows missing")
}
const EXPECTED: &[(&str, &str, &str, &str, &str, &str)] = &[
    ("C01", "config.get", "socket", "IP", "key=full", "R"),
    ("C02", "config.get", "socket", "IP", "key=mtime", "R"),
    ("C03", "config.get", "socket", "IP", "key=skin", "R"),
    (
        "C04",
        "config.get",
        "socket",
        "IP",
        "key=approvals.mode",
        "R",
    ),
    ("C05", "config.get", "socket", "IP", "key=profile", "R"),
    ("C06", "config.get", "socket", "IP", "key=display.skin", "R"),
    ("C07", "config.get", "socket", "IP", "key=model", "R"),
    (
        "C08",
        "config.set",
        "socket",
        "IP",
        "key=display.skin,value",
        "R",
    ),
    ("C09", "config.set", "socket", "IP", "key=model,value", "R"),
    ("C10", "config.set", "socket", "IP", "key=skin,value", "W"),
    ("C11", "GET /api/config/schema", "rest", "none", "", "R"),
    ("C12", "GET /api/config/schema", "rest", "N", "", "R"),
    ("C13", "GET /api/config/defaults", "rest", "none", "", "R"),
    ("C14", "GET /api/config/defaults", "rest", "N", "", "R"),
    (
        "C15",
        "GET /api/config",
        "rest",
        "N",
        "profile=fixture",
        "R",
    ),
    (
        "C16",
        "GET /api/config",
        "rest",
        "N",
        "profile=default",
        "R",
    ),
    (
        "C17",
        "GET /api/config",
        "rest",
        "N",
        "include_defaults=false,profile=fixture",
        "R",
    ),
    (
        "C18",
        "GET /api/config/raw",
        "rest",
        "N",
        "profile=fixture",
        "R",
    ),
    (
        "C19",
        "PUT /api/config",
        "rest",
        "N",
        "config.display.show_cost,profile=fixture",
        "W",
    ),
    (
        "C20",
        "PUT /api/config/raw",
        "rest",
        "N",
        "profile,yaml_text",
        "S",
    ),
    (
        "C21",
        "GET /api/config",
        "rest",
        "N",
        "profile=fixture",
        "R",
    ),
    (
        "C22",
        "GET /api/config",
        "rest",
        "N",
        "profile=fixture",
        "R",
    ),
    (
        "C23",
        "GET /api/config",
        "rest",
        "N",
        "profile=fixture",
        "R",
    ),
    (
        "C24",
        "GET /api/config",
        "rest",
        "none",
        "profile=fixture",
        "R",
    ),
    ("C25", "model.options", "socket", "IP", "", "R"),
    (
        "C26",
        "model.options",
        "socket",
        "IP",
        "profile=fixture",
        "R",
    ),
    ("C27", "model.save_key", "socket", "IP", "api_key,slug", "R"),
    ("C28", "model.disconnect", "socket", "IP", "slug", "R"),
    ("C29", "GET /api/env", "rest", "N", "profile=fixture", "R"),
    ("C30", "GET /api/providers/oauth", "rest", "N", "", "R"),
    (
        "C31",
        "GET /api/providers/custom-endpoints",
        "rest",
        "N",
        "",
        "R",
    ),
    (
        "C32",
        "POST /api/env/reveal",
        "rest",
        "N",
        "key,profile",
        "S",
    ),
    ("C33", "PUT /api/env", "rest", "N", "key,profile,value", "S"),
    ("C34", "GET /api/model/info", "rest", "none", "", "R"),
    ("C35", "GET /api/model/info", "rest", "N", "", "R"),
    ("C36", "GET /api/model/options", "rest", "N", "", "R"),
    ("C37", "GET /api/model/auxiliary", "rest", "N", "", "R"),
    ("C38", "GET /api/model/moa", "rest", "N", "", "R"),
    (
        "C39",
        "POST /api/model/set",
        "rest",
        "N",
        "model,provider,scope,task",
        "S",
    ),
    (
        "C40",
        "PUT /api/profiles/{name}/model",
        "rest",
        "N",
        "model,provider",
        "S",
    ),
    ("C41", "profiles.list", "socket", "IP", "", "R"),
    ("C42", "GET /api/profiles", "rest", "N", "", "R"),
    (
        "C43",
        "profiles.describe",
        "socket",
        "IP",
        "name=fixture",
        "R",
    ),
    (
        "C44",
        "profiles.configure",
        "socket",
        "IP",
        "name=fixture",
        "R",
    ),
    ("C45", "profiles.create", "socket", "IP", "name", "S"),
    ("C46", "POST /api/profiles", "rest", "N", "name", "S"),
    (
        "C47",
        "config.get",
        "socket",
        "IP",
        "key=mtime,profile=default",
        "R",
    ),
    (
        "C48",
        "config.get",
        "socket",
        "IH",
        "key=profile,profile=default",
        "R",
    ),
    (
        "C49",
        "config.get",
        "socket",
        "IH",
        "key=mtime,profile=<unserved>",
        "R",
    ),
    (
        "C50",
        "GET /api/config",
        "rest",
        "N",
        "profile=<unserved>",
        "R",
    ),
    ("C51", "runtime.describe", "socket", "IP", "", "R"),
    ("C52", "GET /api/status", "rest", "none", "", "R"),
    ("C53", "GET /api/health", "rest", "none", "", "R"),
    ("C54", "identify", "control", "null", "", "R"),
    ("C55", "status", "control", "null", "", "S"),
    (
        "C56",
        "POST /api/gateway/restart",
        "rest",
        "N",
        "profile",
        "S",
    ),
    (
        "C57",
        "POST /api/gateway/start",
        "rest",
        "N",
        "profile",
        "S",
    ),
    (
        "C58",
        "POST /api/gateway/stop",
        "rest",
        "N",
        "interrupt,profile",
        "S",
    ),
    ("C59", "talaria.probe.nonexistent", "socket", "IP", "", "R"),
    (
        "C60",
        "GET /api/talaria-probe-nonexistent",
        "rest",
        "N",
        "",
        "R",
    ),
    (
        "C61",
        "GET /api/talaria-probe-nonexistent",
        "rest",
        "none",
        "",
        "R",
    ),
];

#[test]
fn matrix_parses_and_every_row_is_complete() {
    let m = matrix();
    let pin = fs::read_to_string(root().join("crates/hermes-contract/PINNED")).unwrap();
    assert_eq!(
        m["hermes_release"].as_str(),
        pin.lines().find_map(|s| s.strip_prefix("tag="))
    );
    let keys: HashSet<_> = [
        "id",
        "question",
        "call",
        "transport",
        "ticket",
        "request_shape",
        "outcome",
        "reason",
        "result_keys",
        "latency_ms",
        "side_effect",
        "live_effect",
        "evidence",
        "source",
        "notes",
    ]
    .into_iter()
    .collect();
    for row in rows(&m) {
        let obj = row.as_object().unwrap();
        assert_eq!(obj.keys().map(String::as_str).collect::<HashSet<_>>(), keys);
        assert!((1..=17).contains(&row["question"].as_u64().unwrap()));
        assert!(["socket", "rest", "control"].contains(&row["transport"].as_str().unwrap()));
        let o = row["outcome"].as_str().unwrap();
        assert!(
            [
                "ok",
                "unknown_method",
                "timeout",
                "connection_lost",
                "not-probed"
            ]
            .contains(&o)
                || o.strip_prefix("rpc_error:")
                    .is_some_and(|v| v.parse::<u16>().is_ok())
                || o.strip_prefix("http:")
                    .is_some_and(|v| v.parse::<u16>().is_ok())
        );
        assert!(
            ["none", "write-restored", "write-residue", "not-probed"]
                .contains(&row["side_effect"].as_str().unwrap())
        );
        let live = row["evidence"] == "live";
        assert!(live || row["evidence"] == "static");
        assert_eq!(row["latency_ms"].as_u64().is_some(), live);
        if !live {
            assert!(row["latency_ms"].is_null());
            assert!(row["result_keys"].is_null());
            assert_eq!(row["side_effect"], "not-probed");
            assert!(row["notes"].as_str().unwrap().starts_with("not sent:"));
        }
        if o == "not-probed" {
            assert!(!row["reason"].as_str().unwrap().is_empty());
        }
        let write = ["C10", "C19"].contains(&row["id"].as_str().unwrap());
        assert_eq!(!row["live_effect"].is_null(), write && live);
        if !row["live_effect"].is_null() {
            assert!(
                ["applies-now", "next-session", "needs-restart", "unknown"]
                    .contains(&row["live_effect"].as_str().unwrap())
            );
        }
        assert!(row["request_shape"].is_string());
        assert!(!row["source"].as_str().unwrap().starts_with('/'));
    }
    let seed: Value = serde_json::from_slice(
        &fs::read(root().join("fixtures/configuration-surface/static-rows.json")).unwrap(),
    )
    .unwrap();
    for s in rows(&seed) {
        if !["C10", "C19"].contains(&s["id"].as_str().unwrap())
            || rows(&m).iter().find(|r| r["id"] == s["id"]).unwrap()["evidence"] == "static"
        {
            assert!(rows(&m).contains(s));
        }
    }
}

#[test]
fn matrix_covers_every_phase_4a_call() {
    let m = matrix();
    let all = rows(&m);
    assert_eq!(all.len(), 61);
    let mut ids = HashSet::new();
    let mut unique = HashSet::new();
    let mut counts = [0, 0, 0];
    for (id, call, transport, ticket, shape, class) in EXPECTED {
        assert!(ids.insert(*id));
        let found: Vec<_> = all.iter().filter(|r| r["id"] == *id).collect();
        assert_eq!(found.len(), 1);
        let r = found[0];
        assert_eq!(r["call"], *call);
        assert_eq!(r["transport"], *transport);
        assert_eq!(r["request_shape"], *shape);
        let want = match *ticket {
            "IP" => json!({"purpose":"interactive","scope":"profile","profile":"fixture"}),
            "IH" => json!({"purpose":"interactive","scope":"host","profile":"fixture"}),
            "N" => json!({"purpose":"native-http","scope":"profile","profile":"fixture"}),
            _ => Value::Null,
        };
        assert_eq!(r["ticket"], want);
        if !["C21", "C22", "C23"].contains(id) {
            assert!(unique.insert((call, transport, ticket, shape)));
        }
        match *class {
            "R" => {
                counts[0] += 1;
                assert_eq!(r["evidence"], "live");
            }
            "W" => {
                counts[1] += 1;
                if r["evidence"] == "static" {
                    assert_eq!(r["outcome"], "not-probed");
                    assert!(!r["reason"].as_str().unwrap().is_empty());
                } else {
                    assert_eq!(r["evidence"], "live");
                }
            }
            "S" => {
                counts[2] += 1;
                assert_eq!(r["evidence"], "static");
            }
            _ => panic!("invalid class"),
        }
    }
    assert_eq!(counts, [48, 2, 11]);
}

#[test]
fn matrix_socket_rows_name_methods_in_the_pinned_contract_or_are_unlisted() {
    let m = matrix();
    let contract: Value = serde_json::from_slice(
        &fs::read(root().join("crates/hermes-contract/vendor/gateway-contract.openrpc.json"))
            .unwrap(),
    )
    .unwrap();
    let names: HashSet<_> = ["methods", "x-canonical-methods"]
        .into_iter()
        .flat_map(|k| contract[k].as_array().unwrap().iter())
        .filter_map(|v| v["name"].as_str())
        .collect();
    for r in rows(&m).iter().filter(|r| r["transport"] == "socket") {
        assert_eq!(
            names.contains(r["call"].as_str().unwrap()),
            r["id"] != "C59"
        );
    }
}

#[test]
fn matrix_contains_no_values_paths_or_private_names() {
    let m = matrix();
    let base = root();
    let mut files = vec![
        base.join("fixtures/configuration-surface/matrix.json"),
        base.join("crates/hermes-client/tests/live_config_surface.rs"),
        base.join("crates/hermes-client/tests/config_probe/mod.rs"),
    ];
    let findings: Vec<_> = fs::read_dir(base.join("docs/analysis"))
        .unwrap()
        .map(Result::unwrap)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("-hermes-configuration-surface.md")
        })
        .collect();
    assert_eq!(findings.len(), 1);
    files.extend(findings);
    for file in files {
        let body = fs::read_to_string(&file).unwrap();
        for (n, line) in body.lines().enumerate() {
            // A route template such as `/api/profiles/{name}/model` stays one `/api/` token.
            let pieces = line
                .split(|c: char| c.is_whitespace() || "\"'`()<>[],;".contains(c))
                .filter(|s| !s.is_empty());
            let toks: Vec<String> = pieces
                .flat_map(|piece| {
                    if piece.starts_with("/api/") {
                        vec![piece.replace(['{', '}'], "")]
                    } else {
                        piece
                            .split(['{', '}'])
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect()
                    }
                })
                .collect();
            for tok in &toks {
                let tok = tok.as_str();
                let tail = tok.strip_prefix('/');
                assert!(
                    !tail
                        .is_some_and(|s| s
                            .starts_with(|c: char| c.is_ascii_alphanumeric() || "._~".contains(c)))
                        || tok.starts_with("/api/"),
                    "{}:{} path",
                    file.display(),
                    n + 1
                );
                assert!(
                    !tok.contains("~/")
                        && !tok
                            .as_bytes()
                            .windows(3)
                            .any(|w| w[0].is_ascii_alphabetic() && w[1] == b':' && w[2] == b'\\'),
                    "{}:{} path",
                    file.display(),
                    n + 1
                );
                for run in tok.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-') {
                    let hex = run.len() == 40
                        && run
                            .chars()
                            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
                    let snake = run.contains('_')
                        && run
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
                    assert!(
                        run.len() < 40 || hex || snake,
                        "{}:{} token",
                        file.display(),
                        n + 1
                    );
                }
                assert!(
                    !tok.contains("hermes-gateway-ticket."),
                    "{}:{} ticket",
                    file.display(),
                    n + 1
                );
            }
            for var in ["USER", "LOGNAME"] {
                if let Ok(name) = std::env::var(var)
                    && name.len() >= 3
                    && ![
                        "root", "runner", "user", "admin", "test", "dev", "build", "ubuntu",
                    ]
                    .contains(&name.as_str())
                {
                    for (pos, _) in line.match_indices(&name) {
                        let before = line[..pos].chars().next_back();
                        let after = line[pos + name.len()..].chars().next();
                        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
                        assert!(
                            before.is_some_and(word) || after.is_some_and(word),
                            "{}:{} private name",
                            file.display(),
                            n + 1
                        );
                    }
                }
            }
        }
    }
    for r in rows(&m) {
        if let Some(ticket) = r["ticket"].as_object() {
            assert_eq!(ticket["profile"], "fixture");
        }
        for part in r["request_shape"].as_str().unwrap().split(',') {
            if let Some(v) = part.strip_prefix("profile=") {
                assert!(["default", "fixture", "<unserved>"].contains(&v));
            }
        }
    }
}

#[test]
fn key_name_guard_rejects_paths_and_tokens() {
    for v in ["display", "category_order", "mcp_rev"] {
        assert!(key_name_is_safe(v));
    }
    for v in [
        "",
        &"a".repeat(65),
        "bad/name",
        "bad~name",
        "bad name",
        &"a".repeat(43),
        "bad\\name",
    ] {
        assert!(!key_name_is_safe(v));
    }
}
#[test]
fn write_guards_refuse_everything_but_the_opt_in_on_fixture() {
    for (p, e) in [
        ("default", Some("1")),
        ("fixture", None),
        ("fixture", Some("0")),
    ] {
        assert!(write_probe_allowed(p, e).is_err());
    }
    assert!(write_probe_allowed("fixture", Some("1")).is_ok());
    assert!(!config_set_allowed("skin", false));
    assert!(config_set_allowed("display.skin", false));
    assert!(config_set_allowed("model", false));
}
#[test]
fn secret_classifier_sorts_each_shape() {
    for (v, want) in [
        (Some(""), SecretClass::Empty),
        (None, SecretClass::Empty),
        (Some("${FOO_KEY}"), SecretClass::EnvReference),
        (Some("«redacted:abcd»"), SecretClass::Masked),
        (Some("***"), SecretClass::Masked),
        (Some("abcd...wxyz"), SecretClass::Masked),
        (Some("anything-else"), SecretClass::Plain),
    ] {
        assert_eq!(classify(v), want);
    }
    for k in [
        "api_key",
        "token",
        "X-API-Key",
        "github_token",
        "record_key",
        "FOO_API_KEY",
        "TERMINAL_SSH_PORT",
    ] {
        assert!(is_credential_key(k));
    }
    for k in [
        "base_url",
        "skin",
        "token_count",
        "TERMINAL_SSH_HOST",
        "FOO_API_URL",
    ] {
        assert!(!is_credential_key(k));
    }
}
#[test]
fn http_response_parser_handles_length_and_chunked() {
    let a = parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello").unwrap();
    assert_eq!(a.status, 200);
    assert_eq!(a.body, b"hello");
    assert!(
        a.headers
            .iter()
            .any(|(k, v)| k == "content-length" && v == "5")
    );
    let b = parse_response(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nhe\r\n3\r\nllo\r\n0\r\n\r\n",
    )
    .unwrap();
    assert_eq!(b.body, b"hello");
    assert!(parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nhello").is_err());
}
