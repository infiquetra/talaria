//! No secret reaches the disk (P1-04, test row 8).
//!
//! Every test here records through a real `Recorder::create` to a temporary file and then looks at
//! the raw bytes of that file with a plain substring search. A test that only called the pure
//! redaction function would not prove the property (spec section 11). Sentinels are random per
//! run. No failure message prints a sentinel; it names the method and the position instead.

use std::path::Path;

use hermes_client::{REDACTED, Recorder, RecorderOptions};
use proptest::prelude::*;
use serde_json::{Map, Value, json};

/// A random value that stands in for a secret. Hex, so it never looks like a real key.
fn sentinel() -> String {
    format!("zq{:032x}", rand::random::<u128>())
}

fn options() -> RecorderOptions {
    RecorderOptions {
        endpoint: "ws://127.0.0.1:9119/api/ws".to_string(),
        subprotocols: vec!["hermes-gateway-v1".to_string()],
        ..RecorderOptions::default()
    }
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// The lines after the header, parsed.
fn read_records(path: &Path) -> (Vec<u8>, Vec<Value>) {
    let bytes = std::fs::read(path).unwrap_or_else(|_| panic!("the log file cannot be read"));
    let text = std::str::from_utf8(&bytes).unwrap_or_else(|_| panic!("the log is not UTF-8"));
    let mut lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("bad log line")))
        .collect();
    assert_eq!(lines[0]["kind"], "header", "the first line is the header");
    lines.remove(0);
    (bytes, lines)
}

fn withheld(record: &Value) -> bool {
    record
        .get("redactions")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
}

fn has_reason(record: &Value, path: &str, reason: &str) -> bool {
    record
        .get("redactions")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item["path"] == path && item["reason"] == reason)
        })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    In,
    Out,
}

fn record(recorder: &Recorder, dir: Dir, frame: &Value) {
    let text = frame.to_string();
    match dir {
        Dir::In => recorder.record_inbound(1, &text),
        Dir::Out => recorder.record_outbound(1, &text),
    }
}

fn server_request_id() -> String {
    format!("srq-{:012x}", rand::random::<u64>() & 0xffff_ffff_ffff)
}

// ---- the sudo exchange -------------------------------------------------------------------------

struct Exchange {
    method: &'static str,
    request_params: Value,
    answer: Box<dyn Fn(&str) -> Value>,
}

fn value_answer(secret: &str) -> Value {
    json!({"value": secret})
}

#[test]
fn sudo_exchange_answer_never_reaches_disk() {
    let exchanges = vec![
        Exchange {
            method: "sudo",
            request_params: json!({"session_id": "s1", "command": "ls"}),
            answer: Box::new(value_answer),
        },
        Exchange {
            method: "secret",
            request_params: json!({"session_id": "s1", "env_var": "SERVICE_VAR"}),
            answer: Box::new(value_answer),
        },
        Exchange {
            method: "vault.code",
            request_params: json!({"session_id": "s1", "site": "example.test"}),
            answer: Box::new(value_answer),
        },
        Exchange {
            method: "vault.save_login",
            request_params: json!({"session_id": "s1", "site": "example.test"}),
            answer: Box::new(value_answer),
        },
        Exchange {
            method: "vault.unlock_prompt",
            request_params: json!({"session_id": "s1"}),
            answer: Box::new(value_answer),
        },
        Exchange {
            method: "display.install.sudo",
            request_params: json!({"session_id": "s1", "command": "ls"}),
            answer: Box::new(value_answer),
        },
        Exchange {
            method: "clarify",
            request_params: json!({
                "session_id": "s1",
                "questions": [{"id": "q1", "question": "Which colour?", "choices": ["red", "blue"]}],
            }),
            answer: Box::new(|s| json!({"answers": {"q1": s}, "outcome": "answered"})),
        },
    ];

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let recorder = Recorder::create(&path, options()).expect("create");

    let mut secrets: Vec<(String, String)> = Vec::new();
    for exchange in &exchanges {
        let id = server_request_id();
        let secret = sentinel();
        let mut params = exchange.request_params.clone();
        let echoed = sentinel();
        if exchange.method == "clarify" {
            // Hermes echoes locked answers back inside the request (spec 4.1, `clarify` row).
            params["answers"] = json!({"q1": echoed});
            secrets.push((format!("{} (echoed answers)", exchange.method), echoed));
        }
        record(
            &recorder,
            Dir::In,
            &json!({"jsonrpc": "2.0", "id": id, "method": exchange.method, "params": params}),
        );
        record(
            &recorder,
            Dir::Out,
            &json!({"jsonrpc": "2.0", "id": id, "result": (exchange.answer)(&secret)}),
        );
        secrets.push((exchange.method.to_string(), secret));
    }

    // An answer whose request this recorder never saw.
    let unknown_secret = sentinel();
    record(
        &recorder,
        Dir::Out,
        &json!({
            "jsonrpc": "2.0",
            "id": server_request_id(),
            "result": value_answer(&unknown_secret),
        }),
    );
    secrets.push(("unknown id".to_string(), unknown_secret));

    recorder.finish().expect("finish");
    let (bytes, records) = read_records(&path);
    for (label, secret) in &secrets {
        assert!(
            !contains(&bytes, secret),
            "a secret from the {label} exchange reached the file"
        );
    }

    // Two lines per exchange: the request, then the answer.
    for (index, exchange) in exchanges.iter().enumerate() {
        let request = &records[index * 2];
        let answer = &records[index * 2 + 1];
        assert_eq!(request["dir"], "in");
        assert_eq!(answer["dir"], "out");
        assert_eq!(
            answer["frame"]["result"], REDACTED,
            "the {} answer must be withheld whole",
            exchange.method
        );
        assert!(
            has_reason(
                answer,
                "result",
                &format!("server-response:{}", exchange.method)
            ),
            "the {} answer needs the reason server-response:{}",
            exchange.method,
            exchange.method
        );
        assert_eq!(answer["frame"]["id"], request["frame"]["id"]);
        if exchange.method == "clarify" {
            assert_eq!(request["frame"]["params"]["answers"], REDACTED);
            assert!(has_reason(request, "params.answers", "deny-set:clarify"));
        } else {
            assert!(
                !withheld(request),
                "the {} request is ordinary",
                exchange.method
            );
        }
    }
    let last = &records[exchanges.len() * 2];
    assert_eq!(last["frame"]["result"], REDACTED);
    assert!(has_reason(last, "result", "server-response:unknown"));

    // UAT-4 reads these with --nocapture. The assertions above ran first, so nothing printed here
    // can hold a sentinel.
    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines() {
        println!("{line}");
    }
}

// ---- every listed position ---------------------------------------------------------------------

/// How the sentinel is wrapped at a position: a bare string, a map, or a list.
#[derive(Clone, Copy, Debug)]
enum Leaf {
    Str,
    Map,
    List,
}

const LEAVES: [Leaf; 3] = [Leaf::Str, Leaf::Map, Leaf::List];

fn leaf(shape: Leaf, secret: &str) -> Value {
    match shape {
        Leaf::Str => json!(secret),
        Leaf::Map => json!({"inner": [secret]}),
        Leaf::List => json!([{"inner": secret}]),
    }
}

/// Copied by hand from spec section 4.1: method, then the path below `params` to a withheld key.
/// A numeric segment is an array index. The contract cannot name the absent methods, so this
/// table is not read from it; `deny_set_methods_exist_in_the_vendored_contract` keeps it honest.
const POSITIONS: &[(&str, &[&str])] = &[
    ("clarify.respond", &["answer"]),
    ("clarify.lock", &["answer"]),
    ("sudo.respond", &["password"]),
    ("secret.respond", &["value"]),
    ("terminal.read.respond", &["text"]),
    ("model.save_key", &["api_key"]),
    ("mcp.servers.set_api_key", &["value"]),
    ("mcp.servers.oauth.callback", &["code"]),
    ("mcp.servers.oauth.callback", &["state"]),
    ("connection.respond", &["result", "targets", "0", "env"]),
    ("config.set", &["value"]),
    ("vault.add", &["secret"]),
    ("vault.unlock", &["password"]),
    ("mcp.servers.add", &["config"]),
    ("plugins.manage", &["values"]),
    ("diagnostics.share_nous", &["extra_files"]),
    ("session.create", &["api_key"]),
    ("session.create", &["client_env"]),
    ("session.resume", &["api_key"]),
    ("file.attach", &["path"]),
    ("file.attach", &["data_url"]),
    ("file.attach", &["name"]),
    ("image.attach", &["path"]),
    ("image.attach_bytes", &["content_base64"]),
    ("image.attach_bytes", &["data"]),
    ("image.attach_bytes", &["filename"]),
    ("image.attach_bytes", &["ext"]),
    ("image.detach", &["path"]),
    ("pdf.attach", &["content_base64"]),
    ("pdf.attach", &["data"]),
    ("pdf.attach", &["filename"]),
    ("pdf.attach", &["path"]),
    ("prompt.submit", &["attachments", "0", "path"]),
    ("prompt.replace_queued", &["attachments", "0", "path"]),
    // Withheld unless the remembered method for `params.id` is `approval`; nothing is remembered here.
    ("request.answer", &["result"]),
];

fn place(path: &[&str], value: Value) -> Value {
    match path.split_first() {
        None => value,
        Some((head, rest)) => {
            let inner = place(rest, value);
            if head.parse::<usize>().is_ok() {
                Value::Array(vec![inner])
            } else {
                let mut map = Map::new();
                map.insert((*head).to_string(), inner);
                Value::Object(map)
            }
        }
    }
}

fn merge(into: &mut Value, other: Value) {
    if let (Value::Object(target), Value::Object(source)) = (into, other) {
        for (key, value) in source {
            target.insert(key, value);
        }
    }
}

/// `params` for a method with `value` at `path`, plus the siblings real traffic has.
fn params_for(method: &str, path: &[&str], value: Value) -> Value {
    let mut params = json!({"session_id": "s1"});
    if method == "request.answer" {
        params["id"] = json!(server_request_id());
    }
    merge(&mut params, place(path, value));
    params
}

struct Planted {
    label: String,
    dir: Dir,
    frame: Value,
    secret: String,
}

#[test]
fn every_listed_secret_position_never_reaches_disk() {
    let mut planted: Vec<Planted> = Vec::new();
    let mut next_id = 100u64;
    let mut plant = |label: String, dir: Dir, build: &dyn Fn(&str, u64) -> Value| {
        next_id += 1;
        let secret = sentinel();
        let frame = build(&secret, next_id);
        planted.push(Planted {
            label,
            dir,
            frame,
            secret,
        });
    };

    for (method, position) in POSITIONS {
        let at = position.join(".");
        for shape in LEAVES {
            // Directly below params.
            plant(
                format!("{method} at params.{at} ({shape:?})"),
                Dir::Out,
                &|secret, id| {
                    let params = params_for(method, position, leaf(shape, secret));
                    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
                },
            );
            // The rule holds at any depth below params, except for the one key that is bound to
            // `params.id` (`request.answer`).
            if *method != "request.answer" {
                plant(
                    format!("{method} nested below params, key {at} ({shape:?})"),
                    Dir::Out,
                    &|secret, id| {
                        let inner = params_for(method, position, leaf(shape, secret));
                        let nested = json!({"outer": {"list": [inner]}});
                        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": nested})
                    },
                );
            }
        }
    }

    // Inbound: the echoed `answers` of a clarify request, and the same request inside an
    // `open_requests` entry of a response.
    for shape in LEAVES {
        plant(
            format!("clarify request answers ({shape:?})"),
            Dir::In,
            &|secret, _| {
                json!({
                    "jsonrpc": "2.0",
                    "id": server_request_id(),
                    "method": "clarify",
                    "params": {"session_id": "s1", "questions": [], "answers": leaf(shape, secret)},
                })
            },
        );
        plant(
            format!("open_requests clarify answers ({shape:?})"),
            Dir::In,
            &|secret, id| {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "session_id": "s1",
                        "open_requests": [{
                            "id": server_request_id(),
                            "method": "clarify",
                            "params": {
                                "session_id": "s1",
                                "questions": [],
                                "answers": leaf(shape, secret),
                            },
                        }],
                    },
                })
            },
        );
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("log.jsonl");
    let recorder = Recorder::create(&path, options()).expect("create");
    for item in &planted {
        record(&recorder, item.dir, &item.frame);
    }
    recorder.finish().expect("finish");

    let (bytes, records) = read_records(&path);
    assert_eq!(records.len(), planted.len(), "one record per frame");
    for (index, item) in planted.iter().enumerate() {
        assert!(
            !contains(&bytes, &item.secret),
            "a secret reached the file: {}",
            item.label
        );
        assert!(
            withheld(&records[index]),
            "no redactions entry for {}",
            item.label
        );
    }
}

// ---- properties --------------------------------------------------------------------------------

/// Arbitrary JSON that holds nothing the recorder withholds by design (spec section 11).
///
/// Keys use only the letters g to z, so none can be a net-matching name (every pattern needs an
/// `a` or an `e`), a deny-set key, `id`, `method` or `result`. Strings never hold `://`, `#`,
/// `hermes-gateway-ticket.` or `[redacted]`. Numbers are integers in -(2^53)..2^53.
fn arb_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        (-(1i64 << 53)..(1i64 << 53)).prop_map(|n| json!(n)),
        "[ g-z0-9\u{e9}]{0,12}".prop_map(Value::String),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::btree_map("[g-z]{1,6}", inner, 0..4)
                .prop_map(|map| Value::Object(map.into_iter().collect())),
        ]
    })
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 64,
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
        ..ProptestConfig::default()
    }
}

/// Wraps `inner` in `depth` levels of objects and arrays.
fn wrap(depth: usize, inner: Value) -> Value {
    let mut value = inner;
    for level in 0..depth {
        value = if level % 2 == 0 {
            json!({"w": value})
        } else {
            json!([value])
        };
    }
    value
}

const SUSPICIOUS_KEYS: &[&str] = &[
    "password",
    "db_password",
    "DBPASSWORD",
    "client_secret",
    "api_key",
    "openai_api_key",
    "OPENAI_API_KEY",
    "anthropicApiKey",
    "X-Api-Key",
    "privateKey",
    "access_token",
    "authtoken",
    "mycredentials",
    "proxy-authorization",
    "sessionticket",
];

type UrlBuilder = fn(&str) -> String;

const URL_POSITIONS: &[UrlBuilder] = &[
    |s| format!("wss://{s}:pw@host:9119/api/ws?mode=1"),
    |s| format!("wss://user:{s}@host:9119/api/ws"),
    |s| format!("https://{s}@Host/Path"),
    |s| format!("ws://host/api/ws?mode=1&Token={s}"),
    |s| format!("ws://host/api/ws?ticket={s}"),
    |s| format!("ws://host/api/ws?internal={s}"),
    |s| format!("ws://host/api/ws?api_key={s}&mode=1"),
    |s| format!("https://host/page#{s}"),
];

#[derive(Debug, Clone)]
struct SecretCase {
    frame: Value,
    outbound: bool,
    sentinel: String,
}

fn secret_case(
    filler: Value,
    secret: String,
    kind: usize,
    pick: prop::sample::Index,
    depth: usize,
    outbound: bool,
    shape: usize,
) -> SecretCase {
    let shape = LEAVES[shape % LEAVES.len()];
    let value = leaf(shape, &secret);
    let (key, method): (String, Option<&str>) = match kind {
        0 => {
            let usable: Vec<&(&str, &[&str])> = POSITIONS
                .iter()
                .filter(|(method, _)| *method != "request.answer")
                .collect();
            let (method, position) = usable[pick.index(usable.len())];
            let key = position.last().copied().unwrap_or("value");
            (key.to_string(), Some(*method))
        }
        1 => (
            SUSPICIOUS_KEYS[pick.index(SUSPICIOUS_KEYS.len())].to_string(),
            None,
        ),
        _ => ("link".to_string(), None),
    };
    let secret_value = if kind == 2 {
        json!(URL_POSITIONS[pick.index(URL_POSITIONS.len())](&secret))
    } else {
        value
    };
    let mut body = Map::new();
    body.insert("pad".to_string(), filler);
    body.insert(key, secret_value);
    let body = wrap(depth, Value::Object(body));

    // An outbound frame always has a method, so it is never an answer; an inbound one is an event.
    let frame = match (method, outbound) {
        (Some(method), _) => json!({"jsonrpc": "2.0", "id": 9, "method": method, "params": body}),
        (None, true) => json!({"jsonrpc": "2.0", "id": 9, "method": "ping", "params": body}),
        (None, false) => json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {"type": "message.delta", "payload": body},
        }),
    };
    SecretCase {
        frame,
        outbound,
        sentinel: secret,
    }
}

fn arb_secret_case() -> impl Strategy<Value = SecretCase> {
    (
        arb_json(),
        "[a-f0-9]{24}",
        0usize..3,
        any::<prop::sample::Index>(),
        0usize..4,
        any::<bool>(),
        0usize..3,
    )
        .prop_map(|(filler, secret, kind, pick, depth, outbound, shape)| {
            secret_case(filler, secret, kind, pick, depth, outbound, shape)
        })
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn property_sentinel_never_reaches_disk(case in arb_secret_case()) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("log.jsonl");
        let recorder = Recorder::create(&path, options()).expect("create");
        record(&recorder, if case.outbound { Dir::Out } else { Dir::In }, &case.frame);
        recorder.finish().expect("finish");
        let (bytes, records) = read_records(&path);
        prop_assert!(!contains(&bytes, &case.sentinel), "a sentinel reached the file");
        prop_assert_eq!(records.len(), 1);
        prop_assert!(withheld(&records[0]), "nothing was recorded as withheld");
    }
}

/// A frame with no secret position, in a shape real traffic has.
fn arb_clean_frame() -> impl Strategy<Value = (Value, bool)> {
    (arb_json(), 0usize..4, any::<bool>()).prop_map(|(value, kind, outbound)| match kind {
        0 => (
            json!({
                "jsonrpc": "2.0",
                "method": "event",
                "params": {"type": "message.delta", "payload": value},
            }),
            outbound,
        ),
        1 => (
            json!({"jsonrpc": "2.0", "id": 5, "method": "ping", "params": value}),
            outbound,
        ),
        // Any value at all as the frame: an array, a scalar, or an object.
        2 => (value, outbound),
        // A response. Outbound, it would be an answer and withheld by design, so it is inbound.
        _ => (json!({"jsonrpc": "2.0", "id": 6, "result": value}), false),
    })
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn property_frames_without_secret_positions_round_trip((frame, outbound) in arb_clean_frame()) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("log.jsonl");
        let recorder = Recorder::create(&path, options()).expect("create");
        record(&recorder, if outbound { Dir::Out } else { Dir::In }, &frame);
        recorder.finish().expect("finish");
        let (_, records) = read_records(&path);
        prop_assert_eq!(records.len(), 1);
        prop_assert_eq!(&records[0]["frame"], &frame);
        prop_assert!(
            records[0].get("redactions").is_none(),
            "a frame with no secret position must carry no redactions"
        );
    }
}
