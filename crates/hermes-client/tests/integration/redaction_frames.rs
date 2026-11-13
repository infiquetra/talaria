//! Frame-level redaction rules of the recorder.
//!
//! Tests 3 drive `Redactor::redact_outbound` and `redact_inbound` on frames shaped like the
//! gateway's, and read `Redacted.frame` and `Redacted.redactions`. Tests 4 read the vendored
//! contract at test time and walk it to full depth; they push the frames they build through a
//! real `Recorder` and search the raw bytes. The case tables come from spec 4.1 and from
//! `tests/recorder/test_redact.py` and `test_attachment_redact.py` at `python-final`.
//!
//! Sentinels are random per run. Assertions that touch a frame holding a sentinel use plain
//! `assert!` with a label, so a failure prints the label and never the sentinel.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use hermes_client::{REDACTED, Recorder, RecorderOptions, Redacted, Redactor, is_suspicious_key};
use rand::random;
use serde_json::{Map, Value, json};

/// Spec 4.1, copied by hand. Each row is a method and the keys withheld at any depth below its
/// `params`. `request.answer` keeps `result` only for a remembered `approval` id; every test
/// here that uses it sends an id the redactor has never seen.
const DENY_SET: &[(&str, &[&str])] = &[
    ("clarify.respond", &["answer"]),
    ("clarify.lock", &["answer"]),
    ("clarify", &["answers"]),
    ("sudo.respond", &["password"]),
    ("secret.respond", &["value"]),
    ("terminal.read.respond", &["text"]),
    ("model.save_key", &["api_key"]),
    ("mcp.servers.set_api_key", &["value"]),
    ("mcp.servers.oauth.callback", &["code", "state"]),
    ("connection.respond", &["env"]),
    ("config.set", &["value"]),
    ("vault.add", &["secret"]),
    ("mcp.servers.add", &["config"]),
    ("plugins.manage", &["values"]),
    ("diagnostics.share_nous", &["extra_files"]),
    ("vault.unlock", &["password"]),
    ("session.create", &["api_key", "client_env"]),
    ("session.resume", &["api_key"]),
    ("file.attach", &["path", "data_url", "name"]),
    ("image.attach", &["path"]),
    (
        "image.attach_bytes",
        &["content_base64", "data", "filename", "ext"],
    ),
    ("image.detach", &["path"]),
    (
        "pdf.attach",
        &["content_base64", "data", "filename", "path"],
    ),
    ("prompt.submit", &["path"]),
    ("prompt.replace_queued", &["path"]),
    ("request.answer", &["result"]),
];

/// Methods of older gateways that the vendored v0.22.4 contract no longer lists (spec 4.1).
const EXEMPT_FROM_CONTRACT: &[&str] = &["sudo.respond", "secret.respond", "terminal.read.respond"];

/// Spec 4.8, copied by hand: free-form object parameters that no rule covers. Each entry is a
/// method and the parameter's path under `params`, with `[]` for an array level.
const OPEN_POSITIONS: &[(&str, &str)] = &[
    ("groups.create", "members[].target"),
    ("groups.replicate", "members[].target"),
    ("groups.replicate", "page"),
    ("groups.peer.register", "catalog"),
    ("groups.send", "payload"),
    ("llm.oneshot", "variables"),
    ("profiles.configure", "ui_meta"),
    ("profiles.configure", "ui_meta_expected_revisions"),
    ("spawn_tree.save", "subagents[]"),
    ("bot_relay.deliver", "author"),
    ("bot_relay.settle", "author"),
    ("session.automation", "kwargs"),
    ("session.mutate", "payload"),
    ("session.prune", "filters"),
    ("session.create", "editor"),
    ("session.resume", "editor"),
    ("worker.persist", "payload"),
];

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn sentinel() -> String {
    format!("snt{:016x}{:016x}", random::<u64>(), random::<u64>())
}

fn srq_id() -> String {
    format!("srq-{:012x}", random::<u64>() & 0xffff_ffff_ffff)
}

fn redactor() -> Redactor {
    Redactor::new(Vec::new()).unwrap_or_else(|_| panic!("a redactor without substitutions"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir {
    In,
    Out,
}

fn redact(red: &mut Redactor, dir: Dir, frame: &Value) -> Redacted {
    let text = frame.to_string();
    match dir {
        Dir::In => red.redact_inbound(&text),
        Dir::Out => red.redact_outbound(&text),
    }
}

/// The recorded frame as text, for substring search.
fn recorded(r: &Redacted) -> String {
    r.frame.as_ref().map(Value::to_string).unwrap_or_default()
}

fn reasons_at(r: &Redacted, path: &str) -> Vec<String> {
    r.redactions
        .iter()
        .filter(|x| x.path == path)
        .map(|x| x.reason.clone())
        .collect()
}

/// Exactly one redaction at `path`, with `reason`.
fn withheld_once(r: &Redacted, path: &str, reason: &str) -> bool {
    reasons_at(r, path) == [reason.to_string()]
}

fn sorted_paths(r: &Redacted) -> Vec<String> {
    let mut paths: Vec<String> = r.redactions.iter().map(|x| x.path.clone()).collect();
    paths.sort();
    paths
}

fn at<'a>(frame: &'a Option<Value>, path: &[&str]) -> Option<&'a Value> {
    let mut cur = frame.as_ref()?;
    for seg in path {
        cur = match cur {
            Value::Object(map) => map.get(*seg)?,
            Value::Array(items) => items.get(seg.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

fn is_marker(value: Option<&Value>) -> bool {
    value.and_then(Value::as_str) == Some(REDACTED)
}

fn params_with(key: &str, value: Value) -> Value {
    let mut params = Map::new();
    params.insert("session_id".to_string(), json!("s1"));
    params.insert(key.to_string(), value);
    params.insert("keep".to_string(), json!("ok"));
    Value::Object(params)
}

fn contract() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../hermes-contract/vendor/gateway-contract.openrpc.json");
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("the vendored contract cannot be read"));
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("the vendored contract is not JSON"))
}

fn method_names(contract: &Value, list: &str) -> BTreeSet<String> {
    contract[list]
        .as_array()
        .unwrap_or_else(|| panic!("contract list {list} is missing"))
        .iter()
        .filter_map(|m| m["name"].as_str().map(str::to_string))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Test 3: the deny-set and the other frame rules
// ---------------------------------------------------------------------------------------------

#[test]
fn deny_set_withholds_each_listed_key() {
    for (method, keys) in DENY_SET {
        for &key in *keys {
            for dir in [Dir::In, Dir::Out] {
                for shape in ["string", "object"] {
                    let s = sentinel();
                    let value = if shape == "string" {
                        json!(s)
                    } else {
                        json!({"inner": [s]})
                    };
                    let frame = json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": method,
                        "params": params_with(key, value),
                    });
                    let label = format!("{method} / {key} / {dir:?} / {shape}");
                    let r = redact(&mut redactor(), dir, &frame);
                    assert!(r.parse_error.is_none(), "{label}: parse error");
                    assert!(
                        !recorded(&r).contains(&s),
                        "{label}: the sentinel was recorded"
                    );
                    assert!(
                        is_marker(at(&r.frame, &["params", key])),
                        "{label}: value is not the marker"
                    );
                    assert!(
                        withheld_once(&r, &format!("params.{key}"), &format!("deny-set:{method}")),
                        "{label}: want exactly one redaction `deny-set:{method}` at params.{key}, \
                         got {:?}",
                        r.redactions
                    );
                    // Fields the rule does not name stay, and carry no redaction.
                    assert_eq!(at(&r.frame, &["params", "keep"]), Some(&json!("ok")));
                    assert_eq!(at(&r.frame, &["params", "session_id"]), Some(&json!("s1")));
                    assert!(
                        reasons_at(&r, "params.keep").is_empty(),
                        "{label}: an unrelated field was withheld"
                    );
                    assert_eq!(at(&r.frame, &["method"]), Some(&json!(method)));
                }
            }
        }
    }
}

#[test]
fn deny_set_reaches_nested_params_and_batches() {
    // A batch: each element is judged by its own method, and the path starts with the index.
    let (a, b, c) = (sentinel(), sentinel(), sentinel());
    let batch = json!([
        {"jsonrpc": "2.0", "id": 1, "method": "clarify.respond",
         "params": {"session_id": "s1", "answer": a}},
        {"jsonrpc": "2.0", "id": 2, "method": "sudo.respond",
         "params": {"request_id": "r-1", "password": b}},
        {"jsonrpc": "2.0", "id": 3, "method": "config.set",
         "params": {"key": "theme", "value": c}},
    ]);
    for dir in [Dir::In, Dir::Out] {
        let r = redact(&mut redactor(), dir, &batch);
        let text = recorded(&r);
        assert!(
            !text.contains(&a) && !text.contains(&b) && !text.contains(&c),
            "batch {dir:?}: a sentinel was recorded"
        );
        assert!(withheld_once(
            &r,
            "0.params.answer",
            "deny-set:clarify.respond"
        ));
        assert!(withheld_once(
            &r,
            "1.params.password",
            "deny-set:sudo.respond"
        ));
        assert!(withheld_once(&r, "2.params.value", "deny-set:config.set"));
        // The recorded line keeps `key` so a reader sees what was set.
        assert_eq!(at(&r.frame, &["2", "params", "key"]), Some(&json!("theme")));
        assert_eq!(
            at(&r.frame, &["1", "params", "request_id"]),
            Some(&json!("r-1"))
        );
    }

    // Deep below `params`, through objects and arrays.
    let s = sentinel();
    let deep = json!({"method": "clarify.respond",
        "params": {"inner": {"deep": [{"ok": 1}, {"answer": s}]}}});
    for dir in [Dir::In, Dir::Out] {
        let r = redact(&mut redactor(), dir, &deep);
        assert!(
            !recorded(&r).contains(&s),
            "deep {dir:?}: sentinel recorded"
        );
        assert!(withheld_once(
            &r,
            "params.inner.deep.1.answer",
            "deny-set:clarify.respond"
        ));
        assert_eq!(
            at(&r.frame, &["params", "inner", "deep", "0", "ok"]),
            Some(&json!(1))
        );
    }

    // A wrapper object around a method-bearing object does not disarm the rule.
    let s = sentinel();
    let envelope = json!({"envelope": {"method": "clarify.respond", "params": {"answer": s}}});
    let r = redact(&mut redactor(), Dir::Out, &envelope);
    assert!(!recorded(&r).contains(&s), "envelope: sentinel recorded");
    assert!(withheld_once(
        &r,
        "envelope.params.answer",
        "deny-set:clarify.respond"
    ));

    // The path grammar: `.` and `\` inside a key are escaped with `\`.
    let s = sentinel();
    let odd = json!({"method": "clarify.respond",
        "params": {"we.ird\\key": {"answer": s}}});
    let r = redact(&mut redactor(), Dir::Out, &odd);
    assert!(
        !recorded(&r).contains(&s),
        "escaped path: sentinel recorded"
    );
    assert!(
        withheld_once(
            &r,
            r"params.we\.ird\\key.answer",
            "deny-set:clarify.respond"
        ),
        "escaped path: got {:?}",
        r.redactions
    );

    // Attachments: the operator-local path goes, the mime type and the prompt text stay.
    let s = sentinel();
    let submit = json!({"jsonrpc": "2.0", "id": 4, "method": "prompt.submit", "params": {
        "session_id": "s1", "text": "hello there", "submission_id": "sub-1",
        "attachments": [{"mime": "text/plain", "path": s}]}});
    for method in ["prompt.submit", "prompt.replace_queued"] {
        let mut frame = submit.clone();
        frame["method"] = json!(method);
        let r = redact(&mut redactor(), Dir::Out, &frame);
        assert!(!recorded(&r).contains(&s), "{method}: path recorded");
        assert!(withheld_once(
            &r,
            "params.attachments.0.path",
            &format!("deny-set:{method}")
        ));
        assert_eq!(
            at(&r.frame, &["params", "attachments", "0", "mime"]),
            Some(&json!("text/plain"))
        );
        assert_eq!(
            at(&r.frame, &["params", "text"]),
            Some(&json!("hello there"))
        );
        assert!(
            reasons_at(&r, "params.text").is_empty(),
            "prompt text is kept"
        );
    }

    // `connection.respond` carries its secrets at `params.result.targets[].env`.
    let s = sentinel();
    let respond = json!({"jsonrpc": "2.0", "id": 5, "method": "connection.respond", "params": {
        "session_id": "s1",
        "result": {"targets": [{"name": "srv", "env": {"SERVICE_NAME": s}}]}}});
    let r = redact(&mut redactor(), Dir::Out, &respond);
    assert!(
        !recorded(&r).contains(&s),
        "connection.respond: env recorded"
    );
    assert!(withheld_once(
        &r,
        "params.result.targets.0.env",
        "deny-set:connection.respond"
    ));
    assert_eq!(
        at(&r.frame, &["params", "result", "targets", "0", "name"]),
        Some(&json!("srv"))
    );

    // An `open_requests` entry in an inbound result is judged by its own method.
    let s = sentinel();
    let resume = json!({"jsonrpc": "2.0", "id": 6, "result": {
        "session_id": "s1",
        "open_requests": [{"id": srq_id(), "method": "clarify",
            "params": {"session_id": "s1", "answers": [s]}}]}});
    let r = redact(&mut redactor(), Dir::In, &resume);
    assert!(
        !recorded(&r).contains(&s),
        "open_requests: answers recorded"
    );
    assert!(withheld_once(
        &r,
        "result.open_requests.0.params.answers",
        "deny-set:clarify"
    ));
}

#[test]
fn nested_method_unions_outer_rules() {
    // Outer `clarify.respond`, inner `sudo.respond`: the inner rule adds `password`, and the
    // outer rule still withholds `answer` inside the inner object.
    let (a_outer, a_inner, pw) = (sentinel(), sentinel(), sentinel());
    let frame = json!({"method": "clarify.respond", "params": {
        "answer": a_outer,
        "inner": {"method": "sudo.respond", "params": {"answer": a_inner, "password": pw}}}});
    let r = redact(&mut redactor(), Dir::Out, &frame);
    let text = recorded(&r);
    assert!(
        !text.contains(&a_outer) && !text.contains(&a_inner) && !text.contains(&pw),
        "outer clarify, inner sudo: a sentinel was recorded"
    );
    assert!(withheld_once(
        &r,
        "params.answer",
        "deny-set:clarify.respond"
    ));
    assert!(withheld_once(
        &r,
        "params.inner.params.answer",
        "deny-set:clarify.respond"
    ));
    assert!(withheld_once(
        &r,
        "params.inner.params.password",
        "deny-set:sudo.respond"
    ));

    // The other way round: the outer `sudo.respond` still withholds its `password` inside a
    // nested `clarify.respond`, and the inner rule adds `answer`.
    let (pw_outer, pw_inner, ans) = (sentinel(), sentinel(), sentinel());
    let frame = json!({"method": "sudo.respond", "params": {
        "wrapper": {"method": "clarify.respond", "params": {"answer": ans, "password": pw_inner}},
        "password": pw_outer}});
    let r = redact(&mut redactor(), Dir::In, &frame);
    let text = recorded(&r);
    assert!(
        !text.contains(&pw_outer) && !text.contains(&pw_inner) && !text.contains(&ans),
        "outer sudo, inner clarify: a sentinel was recorded"
    );
    assert!(withheld_once(
        &r,
        "params.password",
        "deny-set:sudo.respond"
    ));
    assert!(withheld_once(
        &r,
        "params.wrapper.params.password",
        "deny-set:sudo.respond"
    ));
    assert!(withheld_once(
        &r,
        "params.wrapper.params.answer",
        "deny-set:clarify.respond"
    ));

    // Three levels: every enclosing rule applies at the bottom.
    let (v, a, p) = (sentinel(), sentinel(), sentinel());
    let frame = json!({"method": "secret.respond", "params": {"l1": {"method": "clarify.respond",
        "params": {"l2": {"method": "sudo.respond",
            "params": {"value": v, "answer": a, "password": p}}}}}});
    let r = redact(&mut redactor(), Dir::Out, &frame);
    let text = recorded(&r);
    assert!(
        !text.contains(&v) && !text.contains(&a) && !text.contains(&p),
        "three levels: a sentinel was recorded"
    );
}

#[test]
fn inner_http_verb_does_not_clear_deny_context() {
    // From the Python tests: an unrelated `"method": "GET"` under a nested key must not clear
    // the context established above it. Non-string and unlisted methods are the same case.
    let inner_methods = [
        json!("GET"),
        json!("event"),
        json!(5),
        json!(null),
        json!([1]),
    ];
    for inner in inner_methods {
        let s = sentinel();
        let label = format!("inner method {inner}");
        let frame = json!({"method": "clarify.respond",
            "params": {"req": {"method": inner, "answer": s}}});
        for dir in [Dir::In, Dir::Out] {
            let r = redact(&mut redactor(), dir, &frame);
            assert!(
                !recorded(&r).contains(&s),
                "{label} {dir:?}: sentinel recorded"
            );
            assert!(
                withheld_once(&r, "params.req.answer", "deny-set:clarify.respond"),
                "{label} {dir:?}: got {:?}",
                r.redactions
            );
        }
    }
}

#[test]
fn clarify_lock_env_oauth_and_client_env_withheld() {
    let mut red = redactor();

    // clarify.lock locks one answer of a batch clarify; the ids around it stay.
    let s = sentinel();
    let lock = json!({"jsonrpc": "2.0", "id": 1, "method": "clarify.lock", "params": {
        "request_id": "r-9", "question_id": "q-2", "answer": s}});
    let r = redact(&mut red, Dir::Out, &lock);
    assert!(!recorded(&r).contains(&s), "clarify.lock: answer recorded");
    assert!(withheld_once(&r, "params.answer", "deny-set:clarify.lock"));
    assert_eq!(at(&r.frame, &["params", "request_id"]), Some(&json!("r-9")));
    assert_eq!(
        at(&r.frame, &["params", "question_id"]),
        Some(&json!("q-2"))
    );

    // An OAuth authorisation code and its state.
    let (code, state) = (sentinel(), sentinel());
    let oauth = json!({"jsonrpc": "2.0", "id": 2, "method": "mcp.servers.oauth.callback",
        "params": {"name": "srv", "code": code, "state": state}});
    let r = redact(&mut red, Dir::Out, &oauth);
    let text = recorded(&r);
    assert!(
        !text.contains(&code) && !text.contains(&state),
        "oauth callback: a sentinel was recorded"
    );
    assert!(withheld_once(
        &r,
        "params.code",
        "deny-set:mcp.servers.oauth.callback"
    ));
    assert!(withheld_once(
        &r,
        "params.state",
        "deny-set:mcp.servers.oauth.callback"
    ));
    assert_eq!(at(&r.frame, &["params", "name"]), Some(&json!("srv")));

    // The client's environment on `session.create`: the whole map goes, the model stays.
    let (env_value, api_key_value) = (sentinel(), sentinel());
    let create = json!({"jsonrpc": "2.0", "id": 3, "method": "session.create", "params": {
        "source": "cli", "model": "m1", "client_env": {"WORKDIR_NAME": env_value},
        "api_key": api_key_value}});
    let r = redact(&mut red, Dir::Out, &create);
    let text = recorded(&r);
    assert!(
        !text.contains(&env_value) && !text.contains(&api_key_value),
        "session.create: a sentinel was recorded"
    );
    assert!(is_marker(at(&r.frame, &["params", "client_env"])));
    assert!(is_marker(at(&r.frame, &["params", "api_key"])));
    assert_eq!(
        sorted_paths(&r),
        vec![
            "params.api_key".to_string(),
            "params.client_env".to_string()
        ]
    );
    assert!(withheld_once(
        &r,
        "params.client_env",
        "deny-set:session.create"
    ));
    assert_eq!(at(&r.frame, &["params", "model"]), Some(&json!("m1")));

    // `session.resume` with a key, and `model.save_key`.
    let s = sentinel();
    let resume = json!({"jsonrpc": "2.0", "id": 4, "method": "session.resume",
        "params": {"session_id": "s1", "api_key": s}});
    let r = redact(&mut red, Dir::Out, &resume);
    assert!(!recorded(&r).contains(&s), "session.resume: key recorded");
    assert!(withheld_once(
        &r,
        "params.api_key",
        "deny-set:session.resume"
    ));

    let s = sentinel();
    let save = json!({"jsonrpc": "2.0", "id": 5, "method": "model.save_key",
        "params": {"slug": "deepseek", "api_key": s}});
    let r = redact(&mut red, Dir::Out, &save);
    assert!(!recorded(&r).contains(&s), "model.save_key: key recorded");
    assert_eq!(at(&r.frame, &["params", "slug"]), Some(&json!("deepseek")));
    assert!(withheld_once(
        &r,
        "params.api_key",
        "deny-set:model.save_key"
    ));

    // Free values that only the method rule can recognise: the name `value` is innocent.
    for (method, key) in [
        ("config.set", "value"),
        ("mcp.servers.set_api_key", "value"),
    ] {
        let s = sentinel();
        let frame = json!({"jsonrpc": "2.0", "id": 6, "method": method,
            "params": {"key": "model.default", key: s}});
        let r = redact(&mut red, Dir::Out, &frame);
        assert!(!recorded(&r).contains(&s), "{method}: value recorded");
        assert!(withheld_once(
            &r,
            &format!("params.{key}"),
            &format!("deny-set:{method}")
        ));
        assert_eq!(
            at(&r.frame, &["params", "key"]),
            Some(&json!("model.default"))
        );
    }

    // Attachments: file content and operator-local paths are gone, not labelled
    // (`test_attachment_redact.py`).
    let (path, data_url) = (sentinel(), sentinel());
    let attach = json!({"jsonrpc": "2.0", "id": 7, "method": "file.attach",
        "params": {"path": path, "data_url": data_url, "name": "notes.txt"}});
    let r = redact(&mut red, Dir::Out, &attach);
    let text = recorded(&r);
    assert!(
        !text.contains(&path) && !text.contains(&data_url) && !text.contains("notes.txt"),
        "file.attach: content, path or name recorded"
    );
    for key in ["path", "data_url", "name"] {
        assert!(withheld_once(
            &r,
            &format!("params.{key}"),
            "deny-set:file.attach"
        ));
    }
}

#[test]
fn inbound_clarify_answers_withheld() {
    // Hermes echoes locked batch answers back inside the `clarify` server request.
    let mut red = redactor();
    let (a, b) = (sentinel(), sentinel());
    let id = srq_id();
    let request = json!({"jsonrpc": "2.0", "id": id, "method": "clarify", "params": {
        "session_id": "s1",
        "questions": [{"question": "Which file?", "choices": ["a.txt", "b.txt"]}],
        "answers": [a, b]}});
    let r = redact(&mut red, Dir::In, &request);
    let text = recorded(&r);
    assert!(
        !text.contains(&a) && !text.contains(&b),
        "clarify: answers recorded"
    );
    assert!(is_marker(at(&r.frame, &["params", "answers"])));
    assert!(withheld_once(&r, "params.answers", "deny-set:clarify"));
    // The questions are what the reducer needs; they stay.
    assert_eq!(
        at(&r.frame, &["params", "questions", "0", "question"]),
        Some(&json!("Which file?"))
    );
    assert_eq!(at(&r.frame, &["id"]), Some(&json!(id)));

    // Answers shaped as an object are withheld the same way.
    let s = sentinel();
    let request = json!({"jsonrpc": "2.0", "id": srq_id(), "method": "clarify",
        "params": {"session_id": "s1", "answers": {"q1": s}}});
    let r = redact(&mut red, Dir::In, &request);
    assert!(
        !recorded(&r).contains(&s),
        "clarify object answers recorded"
    );
    assert!(withheld_once(&r, "params.answers", "deny-set:clarify"));

    // The same echo inside the `open_requests` list of a resume result.
    let s = sentinel();
    let resume = json!({"jsonrpc": "2.0", "id": 11, "result": {"session_id": "s1",
        "open_requests": [
            {"id": srq_id(), "method": "clarify",
             "params": {"session_id": "s1", "questions": [], "answers": [s]}},
            {"id": srq_id(), "method": "sudo", "params": {"session_id": "s1"}}]}});
    let r = redact(&mut red, Dir::In, &resume);
    assert!(
        !recorded(&r).contains(&s),
        "open_requests: answers recorded"
    );
    assert!(withheld_once(
        &r,
        "result.open_requests.0.params.answers",
        "deny-set:clarify"
    ));
    assert_eq!(r.redactions.len(), 1, "only the answers are withheld");
}

#[test]
fn url_credential_under_innocent_key() {
    // The key-name net reads names; a credential in a value is caught by the URL rule.
    let query_keys = [
        "token", "ticket", "internal", "Ticket", "TICKET", "tIcKeT", "Internal", "INTERNAL",
        "Token", "api_key",
    ];
    for dir in [Dir::In, Dir::Out] {
        for key in query_keys {
            let s = sentinel();
            let frame = json!({"method": "x", "params": {
                "url": format!("ws://h/api/ws?{key}={s}&mode=1")}});
            let r = redact(&mut redactor(), dir, &frame);
            let want = format!("ws://h/api/ws?{key}=%5Bredacted%5D&mode=1");
            let got = at(&r.frame, &["params", "url"]).and_then(Value::as_str);
            assert!(
                got == Some(want.as_str()),
                "query key {key} {dir:?}: wrong bytes"
            );
            assert!(
                withheld_once(&r, "params.url", "url-credential"),
                "query key {key} {dir:?}: got {:?}",
                r.redactions
            );
            assert_eq!(r.redactions.len(), 1);
        }
    }

    let cases = [
        (
            "userinfo",
            "https://op:{S}@host/p",
            "https://%5Bredacted%5D@host/p",
        ),
        (
            "user name only",
            "https://{S}@host/p",
            "https://%5Bredacted%5D@host/p",
        ),
        (
            "fragment",
            "ws://h/api/ws#token={S}",
            "ws://h/api/ws#%5Bredacted%5D",
        ),
        (
            "all three positions",
            "wss://u:{S}@h:9119/api/ws?token={S}&mode=1#{S}",
            "wss://%5Bredacted%5D@h:9119/api/ws?token=%5Bredacted%5D&mode=1#%5Bredacted%5D",
        ),
    ];
    for (label, input, want) in cases {
        let s = sentinel();
        let frame = json!({"method": "x", "params": {"url": input.replace("{S}", &s)}});
        let r = redact(&mut redactor(), Dir::Out, &frame);
        let got = at(&r.frame, &["params", "url"]).and_then(Value::as_str);
        assert!(got == Some(want), "{label}: wrong bytes");
        assert!(!recorded(&r).contains(&s), "{label}: sentinel recorded");
        assert!(withheld_once(&r, "params.url", "url-credential"), "{label}");
        assert_eq!(r.redactions.len(), 1, "{label}: one redaction per string");
    }

    // A basic-auth URL in a response body, with no query string at all.
    let s = sentinel();
    let response = json!({"jsonrpc": "2.0", "id": 9, "result": {
        "connected": true, "url": format!("http://operator:{s}@cdp.example/")}});
    let r = redact(&mut redactor(), Dir::In, &response);
    assert!(!recorded(&r).contains(&s), "cdp url: sentinel recorded");
    assert!(withheld_once(&r, "result.url", "url-credential"));

    // A URL inside an array is found by position like any other string.
    let s = sentinel();
    let frame = json!({"method": "x", "params": {"urls": ["https://ok.example/a",
        format!("ws://h/api/ws?ticket={s}")]}});
    let r = redact(&mut redactor(), Dir::Out, &frame);
    assert!(!recorded(&r).contains(&s), "array url: sentinel recorded");
    assert!(withheld_once(&r, "params.urls.1", "url-credential"));
    assert_eq!(
        at(&r.frame, &["params", "urls", "0"]),
        Some(&json!("https://ok.example/a"))
    );

    // A URL whose credential is already withheld changes nothing and claims nothing.
    let already = json!({"method": "x", "params": {
        "url": "ws://h/api/ws?ticket=%5Bredacted%5D"}});
    let r = redact(&mut redactor(), Dir::Out, &already);
    assert_eq!(r.frame.as_ref(), Some(&already));
    assert!(r.redactions.is_empty(), "no second withholding is reported");

    // A harmless URL is left byte for byte.
    for url in [
        "https://example.com/docs?page=2&sort=asc",
        "https://Example.COM:443/Path?A=1&b=%2Fx",
        "ws://127.0.0.1:8765/api/ws",
        "https://host/page?a=1&b",
    ] {
        let frame = json!({"method": "x", "params": {"url": url}});
        let r = redact(&mut redactor(), Dir::Out, &frame);
        assert_eq!(r.frame.as_ref(), Some(&frame), "{url}");
        assert!(r.redactions.is_empty(), "{url}");
    }
}

#[test]
fn ordinary_traffic_is_unchanged() {
    // Event, usage, gateway reply with workspace references, a prompt with an attachment, an
    // unlisted `approval.respond`, and non-object frames: all come back equal, with no
    // redaction. Over-redaction is a failure too (spec 4.2).
    let ordinary = [
        json!({"jsonrpc": "2.0", "method": "event", "params": {
            "type": "assistant_delta", "session_id": "s-1", "seq": 7,
            "payload": {"text": "hello"}, "execution_generation": 2, "replay_epoch": 1}}),
        json!({"jsonrpc": "2.0", "method": "event", "params": {
            "type": "usage", "payload": {"input_tokens": 1200, "output_tokens": 340,
                "total_tokens": 1540, "max_tokens": 4096, "session_total_tokens": 99,
                "tokens_per_delta": 3}}}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "prompt.submit", "params": {
            "session_id": "s1", "text": "Please list the files", "submission_id": "sub-1",
            "attachments": [{"mime": "text/plain"}]}}),
        json!({"method": "approval.respond",
            "params": {"request_id": "r-9", "approved": true, "reason": "looks fine"}}),
        json!({"jsonrpc": "2.0", "method": "event", "params": {"type": "approval.request",
            "session_id": "s1", "payload": {"kind": "command", "prompt_id": "p-1",
                "execution_generation": 2, "command": "rm -rf cache", "choices": ["once", "deny"],
                "timeout_s": 300}}}),
        json!({"jsonrpc": "2.0", "id": 6, "error": {"code": -32601, "message": "no handler"}}),
        json!([1, 2, {"a": "b"}]),
        json!("plain string"),
        json!(42),
        json!(null),
        json!(true),
    ];
    // Responses with a `result` are inbound only: written by Talaria, a frame with an id and a
    // result and no method is an answer to a server request and is withheld by design.
    let inbound_only = [
        json!({"jsonrpc": "2.0", "id": "7", "result": {
            "attached": true, "name": "notes.txt",
            "path": "/gateway/ws/.hermes/desktop-attachments/notes.txt",
            "ref_text": "@file:.hermes/desktop-attachments/notes.txt", "uploaded": true}}),
        json!({"jsonrpc": "2.0", "id": 5, "result": {"keys": 3, "monkey": 1, "tokenizer": "x",
            "request_id": "q", "submission_id": "u", "keyboard": "us"}}),
    ];
    for dir in [Dir::In, Dir::Out] {
        let extra: &[Value] = if dir == Dir::In { &inbound_only } else { &[] };
        for frame in ordinary.iter().chain(extra) {
            let r = redact(&mut redactor(), dir, frame);
            assert!(r.parse_error.is_none());
            assert_eq!(r.frame.as_ref(), Some(frame), "{dir:?}");
            assert!(
                r.redactions.is_empty(),
                "{dir:?} {frame}: {:?}",
                r.redactions
            );
        }
    }
}

#[test]
fn input_is_never_mutated() {
    // The redactor reads text and returns a new value. The text stays as it was, the sentinel
    // is only in the original, and redacting the same text twice gives the same result.
    let s = sentinel();
    let text = json!({"jsonrpc": "2.0", "id": 1, "method": "sudo.respond", "params": {
        "request_id": "r-1", "password": s}})
    .to_string();
    let before = text.clone();

    let mut first = redactor();
    let a = first.redact_outbound(&text);
    let b = first.redact_outbound(&text);
    let c = redactor().redact_outbound(&text);

    assert_eq!(text, before, "the input text changed");
    assert!(text.contains(&s));
    for r in [&a, &b, &c] {
        assert!(
            !recorded(r).contains(&s),
            "a result still holds the sentinel"
        );
        assert!(is_marker(at(&r.frame, &["params", "password"])));
        assert_eq!(at(&r.frame, &["params", "request_id"]), Some(&json!("r-1")));
    }
    assert!(a == b, "redacting twice gave different results");
    assert!(a == c, "a fresh redactor gave a different result");
}

#[test]
fn suspicious_key_value_is_withheld_whatever_its_type() {
    // The net runs on any frame, including a method nothing has heard of (protocol drift).
    let s = sentinel();
    let frame = json!({"method": "future.unknown.respond", "params": {
        "api_key": s, "password": 12345, "credentials": {"x": "y"}, "token": [1, 2],
        "Authorization": true, "max_tokens": 4096, "slug": "deepseek"}});
    for dir in [Dir::In, Dir::Out] {
        let r = redact(&mut redactor(), dir, &frame);
        assert!(!recorded(&r).contains(&s), "{dir:?}: sentinel recorded");
        for key in [
            "api_key",
            "password",
            "credentials",
            "token",
            "Authorization",
        ] {
            assert!(is_marker(at(&r.frame, &["params", key])), "{dir:?}: {key}");
            assert!(
                withheld_once(&r, &format!("params.{key}"), "suspicious-key"),
                "{dir:?}: {key}"
            );
        }
        assert_eq!(at(&r.frame, &["params", "max_tokens"]), Some(&json!(4096)));
        assert_eq!(at(&r.frame, &["params", "slug"]), Some(&json!("deepseek")));
        assert_eq!(r.redactions.len(), 5);
    }

    // Deep inside the frame, under arrays too.
    let s = sentinel();
    let frame = json!({"method": "event", "params": {"payload": {"config": [{"provider": {
        "token": s}}]}}});
    let r = redact(&mut redactor(), Dir::In, &frame);
    assert!(!recorded(&r).contains(&s), "deep key: sentinel recorded");
    assert!(withheld_once(
        &r,
        "params.payload.config.0.provider.token",
        "suspicious-key"
    ));
}

#[test]
fn free_text_names_are_withheld_in_any_position() {
    // `system_prompt`, `credential_hint` and `launch_request` fill the snapshot's `info`
    // block with free text (spec 4.2). The reducer needs model, provider and cwd.
    let (sp, hint, launch) = (sentinel(), sentinel(), sentinel());
    let snapshot = json!({"jsonrpc": "2.0", "id": 3, "result": {
        "session_id": "s1",
        "info": {"model": "m1", "provider": "p1", "cwd": "<tmp>", "tool_count": 4,
                 "system_prompt": sp, "credential_hint": hint, "launch_request": {"a": launch}}}});
    let r = redact(&mut redactor(), Dir::In, &snapshot);
    let text = recorded(&r);
    assert!(
        !text.contains(&sp) && !text.contains(&hint) && !text.contains(&launch),
        "info: a free-text value was recorded"
    );
    for name in ["system_prompt", "credential_hint", "launch_request"] {
        assert!(
            withheld_once(&r, &format!("result.info.{name}"), "free-text"),
            "{name}: got {:?}",
            r.redactions
        );
    }
    assert_eq!(
        at(&r.frame, &["result", "info", "model"]),
        Some(&json!("m1"))
    );
    assert_eq!(
        at(&r.frame, &["result", "info", "provider"]),
        Some(&json!("p1"))
    );
    assert_eq!(
        at(&r.frame, &["result", "info", "cwd"]),
        Some(&json!("<tmp>"))
    );
    assert_eq!(
        at(&r.frame, &["result", "info", "tool_count"]),
        Some(&json!(4))
    );

    // Any depth, any frame.
    let s = sentinel();
    let frame = json!({"method": "event", "params": {"a": [{"b": {"system_prompt": s}}]}});
    let r = redact(&mut redactor(), Dir::Out, &frame);
    assert!(!recorded(&r).contains(&s), "nested system_prompt recorded");
    assert!(withheld_once(&r, "params.a.0.b.system_prompt", "free-text"));
}

#[test]
fn connect_link_is_free_text() {
    let link = "https://example.invalid/connect/ls-sentinel-3b9d";
    let frame = json!({"method": "event", "params": {"type": "connection.update", "payload": {
        "targets": [{"name": "github", "connect_url": link}]
    }}});
    let redacted = redact(&mut redactor(), Dir::In, &frame);
    let text = recorded(&redacted);
    assert!(
        !text.contains("ls-sentinel-3b9d"),
        "link sentinel was recorded"
    );
    assert!(withheld_once(
        &redacted,
        "params.payload.targets.0.connect_url",
        "free-text"
    ));
    assert_eq!(
        at(
            &redacted.frame,
            &["params", "payload", "targets", "0", "connect_url"]
        ),
        Some(&json!("[redacted]"))
    );
}

#[test]
fn ticket_subprotocol_string_is_withheld_wherever_it_appears() {
    // The ticket travels as a subprotocol, never in a frame; any string that contains the
    // subprotocol name is withheld as a safety net.
    let prefix = concat!("hermes-gateway-ticket", ".");
    for dir in [Dir::In, Dir::Out] {
        let s = sentinel();
        let frame = json!({"jsonrpc": "2.0", "method": "event", "params": {
            "payload": {"message": format!("upgrade failed for {prefix}{s} today")},
            "list": [format!("{prefix}{s}")]}});
        let r = redact(&mut redactor(), dir, &frame);
        assert!(!recorded(&r).contains(&s), "{dir:?}: ticket recorded");
        assert!(
            withheld_once(&r, "params.payload.message", "ticket"),
            "{dir:?}"
        );
        assert!(withheld_once(&r, "params.list.0", "ticket"), "{dir:?}");
    }
}

#[test]
fn text_that_is_not_json_has_no_frame() {
    let s = sentinel();
    let text = format!("{{\"params\": {{\"password\": \"{s}\" oops");
    for dir in [Dir::In, Dir::Out] {
        let mut red = redactor();
        let r = match dir {
            Dir::In => red.redact_inbound(&text),
            Dir::Out => red.redact_outbound(&text),
        };
        assert!(r.frame.is_none(), "{dir:?}: a frame was produced");
        let error = r.parse_error.unwrap_or_default();
        assert!(!error.is_empty(), "{dir:?}: no parse error");
        assert!(
            !error.contains(&s),
            "{dir:?}: the parse error holds payload text"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Test 4: walks over the vendored contract
// ---------------------------------------------------------------------------------------------

/// One step down a schema: an object key, or one array level.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Seg {
    Key(String),
    Item,
}

/// What a walk found at a path.
#[derive(Debug)]
enum Found {
    /// A declared property; the path ends with its name.
    Property(Vec<Seg>),
    /// An object schema with no declared properties and additional properties allowed.
    FreeForm(Vec<Seg>),
}

fn is_object_type(schema: &Map<String, Value>) -> bool {
    match schema.get("type") {
        Some(Value::String(t)) => t == "object",
        Some(Value::Array(ts)) => ts.iter().any(|t| t == "object"),
        _ => false,
    }
}

/// Walks a schema through `$ref`, `anyOf`, `oneOf`, `allOf`, `items` and `properties`, with no
/// depth limit. A `$ref` already on the current path is not followed again (recursive schemas
/// would otherwise never end); every other branch is followed to its end.
fn walk(
    schemas: &Map<String, Value>,
    schema: &Value,
    path: &mut Vec<Seg>,
    on_path: &mut Vec<String>,
    out: &mut Vec<Found>,
) {
    let Some(obj) = schema.as_object() else {
        return;
    };
    if let Some(reference) = obj.get("$ref").and_then(Value::as_str) {
        let name = reference
            .rsplit('/')
            .next()
            .unwrap_or(reference)
            .to_string();
        if on_path.contains(&name) {
            return;
        }
        let Some(target) = schemas.get(&name) else {
            panic!("the contract has a dangling reference to {name}");
        };
        on_path.push(name);
        walk(schemas, target, path, on_path, out);
        on_path.pop();
        return;
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = obj.get(keyword).and_then(Value::as_array) {
            for branch in branches {
                walk(schemas, branch, path, on_path, out);
            }
        }
    }
    if let Some(items) = obj.get("items").filter(|i| i.is_object()) {
        path.push(Seg::Item);
        walk(schemas, items, path, on_path, out);
        path.pop();
    }
    match obj.get("properties").and_then(Value::as_object) {
        Some(props) if !props.is_empty() => {
            for (name, sub) in props {
                path.push(Seg::Key(name.clone()));
                out.push(Found::Property(path.clone()));
                walk(schemas, sub, path, on_path, out);
                path.pop();
            }
        }
        _ => {
            if is_object_type(obj) && obj.get("additionalProperties") != Some(&Value::Bool(false)) {
                out.push(Found::FreeForm(path.clone()));
            }
        }
    }
}

fn walk_schema(schemas: &Map<String, Value>, schema: &Value) -> Vec<Found> {
    let mut out = Vec::new();
    walk(schemas, schema, &mut Vec::new(), &mut Vec::new(), &mut out);
    out
}

/// `members[].target`: keys joined by `.`, one `[]` for each array level.
fn render(path: &[Seg]) -> String {
    let mut out = String::new();
    for seg in path {
        match seg {
            Seg::Key(k) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(k);
            }
            Seg::Item => out.push_str("[]"),
        }
    }
    out
}

/// The value tree that puts `leaf` at `path`: an object for a key, a one element array for an
/// array level.
fn build(path: &[Seg], leaf: Value) -> Value {
    match path.split_first() {
        None => leaf,
        Some((Seg::Key(k), rest)) => {
            let mut map = Map::new();
            map.insert(k.clone(), build(rest, leaf));
            Value::Object(map)
        }
        Some((Seg::Item, rest)) => Value::Array(vec![build(rest, leaf)]),
    }
}

fn schemas_of(contract: &Value) -> &Map<String, Value> {
    contract["components"]["schemas"]
        .as_object()
        .unwrap_or_else(|| panic!("the contract has no schemas"))
}

#[test]
fn deny_set_methods_exist_in_the_vendored_contract() {
    let contract = contract();
    let mut known = method_names(&contract, "methods");
    known.extend(method_names(&contract, "x-canonical-methods"));
    known.extend(method_names(&contract, "x-server-requests"));
    assert!(known.len() > 100, "the contract lists too few methods");

    let missing: Vec<&str> = DENY_SET
        .iter()
        .map(|(method, _)| *method)
        .filter(|method| !EXEMPT_FROM_CONTRACT.contains(method))
        .filter(|method| !known.contains(*method))
        .collect();
    assert!(
        missing.is_empty(),
        "deny-set methods missing from the vendored contract (renamed or removed?): {missing:?}"
    );
    // The exemption is for exactly the three old-gateway methods.
    for old in EXEMPT_FROM_CONTRACT {
        assert!(
            DENY_SET.iter().any(|(m, _)| m == old),
            "{old} must stay in the deny-set"
        );
    }
}

/// Collects every in-memory write so a test can search the raw bytes.
#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedBuffer {
    fn bytes(&self) -> Vec<u8> {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

struct Planted {
    dir: Dir,
    /// `params` or `result`, and the rendered path; for failure messages only.
    place: String,
    sentinel: String,
    frame: String,
}

#[test]
fn contract_params_matching_the_net_are_withheld() {
    // Spec 11, "Test 4 walk": every parameter schema and result schema, to full depth. For
    // each property name `is_suspicious_key` matches, plant a sentinel at that exact path in a
    // frame, record the frame through a real `Recorder`, and look for the sentinel in the
    // bytes that were written.
    let contract = contract();
    let schemas = schemas_of(&contract);
    let mut total = 0usize;

    let lists = [
        ("methods", Dir::Out),
        ("x-canonical-methods", Dir::Out),
        ("x-server-requests", Dir::In),
    ];
    for (list, params_dir) in lists {
        for entry in contract[list].as_array().cloned().unwrap_or_default() {
            let method = entry["name"].as_str().unwrap_or_default().to_string();
            let mut planted: Vec<Planted> = Vec::new();
            let mut seen: BTreeSet<(bool, Vec<Seg>)> = BTreeSet::new();
            let mut counter = 0u64;

            let mut plant = |is_result: bool, schema: &Value, planted: &mut Vec<Planted>| {
                for found in walk_schema(schemas, schema) {
                    let Found::Property(path) = found else {
                        continue;
                    };
                    let Some(Seg::Key(name)) = path.last() else {
                        continue;
                    };
                    if !is_suspicious_key(name) || !seen.insert((is_result, path.clone())) {
                        continue;
                    }
                    counter += 1;
                    let s = sentinel();
                    let nested = build(&path, json!(s));
                    let (dir, frame, place) = if is_result {
                        (
                            Dir::In,
                            json!({"jsonrpc": "2.0", "id": counter, "result": nested}),
                            format!("result {}", render(&path)),
                        )
                    } else {
                        let id = if params_dir == Dir::In {
                            json!(srq_id())
                        } else {
                            json!(counter)
                        };
                        (
                            params_dir,
                            json!({"jsonrpc": "2.0", "id": id, "method": method,
                                "params": nested}),
                            format!("params {}", render(&path)),
                        )
                    };
                    planted.push(Planted {
                        dir,
                        place,
                        sentinel: s,
                        frame: frame.to_string(),
                    });
                }
            };

            for param in entry["params"].as_array().cloned().unwrap_or_default() {
                plant(false, &param["schema"], &mut planted);
            }
            if list != "x-server-requests"
                && let Some(schema) = entry.get("result").and_then(|r| r.get("schema"))
            {
                plant(true, schema, &mut planted);
            }
            if planted.is_empty() {
                continue;
            }

            let buffer = SharedBuffer::default();
            let options = RecorderOptions {
                endpoint: "ws://127.0.0.1:9119/api/ws".to_string(),
                ..RecorderOptions::default()
            };
            let recorder = Recorder::from_writer(buffer.clone(), options)
                .unwrap_or_else(|_| panic!("the recorder did not start"));
            for p in &planted {
                match p.dir {
                    Dir::In => recorder.record_inbound(1, &p.frame),
                    Dir::Out => recorder.record_outbound(1, &p.frame),
                }
            }
            let summary = recorder
                .finish()
                .unwrap_or_else(|_| panic!("{method}: the recording failed"));
            assert_eq!(summary.dropped, 0, "{method}: frames were dropped");
            assert_eq!(summary.frames, planted.len(), "{method}: frames were lost");
            assert!(
                summary.redactions >= planted.len(),
                "{method}: {} frames carried a net-matching name but only {} redactions \
                 were written",
                planted.len(),
                summary.redactions
            );

            let bytes = buffer.bytes();
            let leaked: Vec<&str> = planted
                .iter()
                .filter(|p| contains_bytes(&bytes, p.sentinel.as_bytes()))
                .map(|p| p.place.as_str())
                .collect();
            assert!(
                leaked.is_empty(),
                "{list} {method}: value reached the bytes at {leaked:?}"
            );
            total += planted.len();
        }
    }
    assert!(
        total > 0,
        "the walk found no net-matching name; the walk is broken"
    );
}

#[test]
fn free_form_params_are_classified() {
    // Walk the parameters of `methods` and `x-canonical-methods` for object parameters with no
    // declared properties and additional properties allowed. Each must be a deny-set key of
    // its method (withheld) or a checked-in open position (spec 4.8). A Hermes release that
    // adds one fails here until someone decides which.
    let contract = contract();
    let schemas = schemas_of(&contract);
    let mut unclassified: BTreeSet<String> = BTreeSet::new();
    let mut found_any = false;

    for list in ["methods", "x-canonical-methods"] {
        for entry in contract[list].as_array().cloned().unwrap_or_default() {
            let method = entry["name"].as_str().unwrap_or_default();
            for param in entry["params"].as_array().cloned().unwrap_or_default() {
                for found in walk_schema(schemas, &param["schema"]) {
                    let Found::FreeForm(path) = found else {
                        continue;
                    };
                    found_any = true;
                    let rendered = render(&path);
                    let denied_key = match path.iter().rev().find(|s| matches!(s, Seg::Key(_))) {
                        Some(Seg::Key(k)) => DENY_SET
                            .iter()
                            .any(|(m, keys)| *m == method && keys.contains(&k.as_str())),
                        _ => false,
                    };
                    let open = OPEN_POSITIONS
                        .iter()
                        .any(|(m, p)| *m == method && *p == rendered);
                    if !denied_key && !open {
                        unclassified.insert(format!("{method}.{rendered}"));
                    }
                }
            }
        }
    }
    assert!(
        found_any,
        "the walk found no free-form parameter; the walk is broken"
    );
    assert!(
        unclassified.is_empty(),
        "free-form parameters in neither the deny-set nor the open positions: {unclassified:?}"
    );
}
