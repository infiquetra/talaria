//! Answers to server requests.
//!
//! Hermes asks the client questions (`sudo`, `secret`, `clarify`, `vault.*`, `approval` and
//! others) as JSON-RPC requests with ids like `srq-<12 hex>`. The `Redactor` remembers the
//! method of each open request and withholds the answer, except for the two keys of an
//! `approval` answer. The method list comes from `x-server-requests` in the vendored contract
//! at test time; it is never written out here.
//!
//! Sentinels are random per run. Assertions that touch a frame holding a sentinel use plain
//! `assert!` with a label, so a failure prints the label and never the sentinel.

use std::path::Path;

use hermes_client::{REDACTED, Redacted, Redaction, Redactor};
use rand::random;
use serde_json::{Value, json};

const BOUND_PLUS_MARGIN: usize = 1_100;

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

/// The methods of `x-server-requests`, read from the vendored contract.
fn server_request_methods() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../hermes-contract/vendor/gateway-contract.openrpc.json");
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("the vendored contract cannot be read"));
    let contract: Value =
        serde_json::from_str(&text).unwrap_or_else(|_| panic!("the vendored contract is not JSON"));
    let methods: Vec<String> = contract["x-server-requests"]
        .as_array()
        .unwrap_or_else(|| panic!("the contract has no x-server-requests"))
        .iter()
        .filter_map(|m| m["name"].as_str().map(str::to_string))
        .collect();
    assert!(methods.len() > 1, "x-server-requests lists too few methods");
    assert!(
        methods.iter().any(|m| m == "approval"),
        "x-server-requests no longer lists approval; the approval tests need it"
    );
    methods
}

fn non_approval_methods() -> Vec<String> {
    server_request_methods()
        .into_iter()
        .filter(|m| m != "approval")
        .collect()
}

fn request_frame(id: &Value, method: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {"session_id": "s1"}})
}

fn answer_frame(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// Teaches the redactor an inbound server request and checks the request itself is recorded
/// whole.
fn teach(red: &mut Redactor, id: &Value, method: &str) {
    let frame = request_frame(id, method);
    let r = red.redact_inbound(&frame.to_string());
    assert!(r.parse_error.is_none(), "{method}: request did not parse");
    assert!(
        r.frame.as_ref() == Some(&frame),
        "{method}: request was altered"
    );
    assert!(r.redactions.is_empty(), "{method}: request was redacted");
}

fn recorded(r: &Redacted) -> String {
    r.frame.as_ref().map(Value::to_string).unwrap_or_default()
}

fn at<'a>(frame: &'a Option<Value>, path: &[&str]) -> Option<&'a Value> {
    let mut cur = frame.as_ref()?;
    for seg in path {
        cur = cur.as_object()?.get(*seg)?;
    }
    Some(cur)
}

fn is_marker(value: Option<&Value>) -> bool {
    value.and_then(Value::as_str) == Some(REDACTED)
}

fn redaction(path: &str, reason: &str) -> Redaction {
    Redaction {
        path: path.to_string(),
        reason: reason.to_string(),
    }
}

/// The answer was withheld whole: the result is the marker and the only redaction says so.
fn assert_answer_withheld(r: &Redacted, reason: &str, sentinels: &[&str], label: &str) {
    let text = recorded(r);
    for s in sentinels {
        assert!(!text.contains(s), "{label}: a sentinel was recorded");
    }
    assert!(
        is_marker(at(&r.frame, &["result"])),
        "{label}: result is not the marker"
    );
    assert!(
        r.redactions == [redaction("result", reason)],
        "{label}: want one redaction `{reason}` at result, got {:?}",
        r.redactions
    );
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[test]
fn answers_to_every_non_approval_request_are_withheld() {
    for method in non_approval_methods() {
        let mut red = redactor();
        let id = json!(srq_id());
        teach(&mut red, &id, &method);

        let (a, b, c) = (sentinel(), sentinel(), sentinel());
        let answer = answer_frame(
            &id,
            json!({"value": a, "answers": [b], "nested": {"x": c}, "outcome": "answered"}),
        );
        let r = red.redact_outbound(&answer.to_string());
        assert!(r.parse_error.is_none(), "{method}: answer did not parse");
        assert_answer_withheld(
            &r,
            &format!("server-response:{method}"),
            &[&a, &b, &c],
            &method,
        );
        // The frame around the result is kept.
        assert_eq!(at(&r.frame, &["id"]), Some(&id), "{method}");
        assert_eq!(at(&r.frame, &["jsonrpc"]), Some(&json!("2.0")), "{method}");
    }
}

#[test]
fn approval_answer_keeps_only_choice_and_all() {
    let mut red = redactor();
    let id = json!(srq_id());
    teach(&mut red, &id, "approval");

    let (note, comment) = (sentinel(), sentinel());
    let answer = answer_frame(
        &id,
        json!({"choice": "once", "all": true, "note": note, "comment": comment}),
    );
    let r = red.redact_outbound(&answer.to_string());
    let text = recorded(&r);
    assert!(
        !text.contains(&note) && !text.contains(&comment),
        "approval: an extra key was recorded"
    );
    assert_eq!(at(&r.frame, &["result", "choice"]), Some(&json!("once")));
    assert_eq!(at(&r.frame, &["result", "all"]), Some(&json!(true)));
    // Any other key is gone: either dropped or replaced by the marker, and reported.
    let result = at(&r.frame, &["result"])
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("approval: result is not an object"));
    for (key, value) in result {
        if key != "choice" && key != "all" {
            assert!(
                value.as_str() == Some(REDACTED),
                "approval: key {key} was kept"
            );
        }
    }
    for key in ["note", "comment"] {
        assert!(
            r.redactions.contains(&redaction(
                &format!("result.{key}"),
                "server-response:approval"
            )),
            "approval: no redaction for result.{key}, got {:?}",
            r.redactions
        );
    }
    assert!(
        !r.redactions
            .iter()
            .any(|x| x.path == "result.choice" || x.path == "result.all"),
        "approval: choice or all was reported as withheld"
    );

    // A bare choice passes through unchanged and claims nothing.
    let mut red = redactor();
    let id = json!(srq_id());
    teach(&mut red, &id, "approval");
    let answer = answer_frame(&id, json!({"choice": "deny"}));
    let r = red.redact_outbound(&answer.to_string());
    assert!(
        r.frame.as_ref() == Some(&answer),
        "a bare choice was altered"
    );
    assert!(
        r.redactions.is_empty(),
        "a bare choice was reported as withheld"
    );
}

#[test]
fn unknown_id_is_withheld() {
    let mut red = redactor();
    let s = sentinel();
    let answer = answer_frame(&json!(srq_id()), json!({"value": s}));
    let r = red.redact_outbound(&answer.to_string());
    assert_answer_withheld(&r, "server-response:unknown", &[&s], "never seen");

    // Not every frame with an id and a method is a server request the redactor can learn:
    // no params, params that are not an object, and Talaria's own outbound request.
    let id = json!(srq_id());
    let s = sentinel();
    let no_params = json!({"jsonrpc": "2.0", "id": id, "method": "sudo"});
    let list_params = json!({"jsonrpc": "2.0", "id": id, "method": "sudo", "params": []});
    for (label, frame) in [("no params", no_params), ("array params", list_params)] {
        let mut red = redactor();
        red.redact_inbound(&frame.to_string());
        let answer = answer_frame(&id, json!({"value": s}));
        let r = red.redact_outbound(&answer.to_string());
        assert_answer_withheld(&r, "server-response:unknown", &[&s], label);
    }
    let mut red = redactor();
    let outbound_request = request_frame(&id, "sudo");
    red.redact_outbound(&outbound_request.to_string());
    let r = red.redact_outbound(&answer_frame(&id, json!({"value": s})).to_string());
    assert_answer_withheld(&r, "server-response:unknown", &[&s], "outbound request");

    // An inbound event or notification (no id) teaches nothing.
    let mut red = redactor();
    let event = json!({"jsonrpc": "2.0", "method": "event", "params": {
        "type": "tool.start", "session_id": "s1", "payload": {"id": id}}});
    red.redact_inbound(&event.to_string());
    let r = red.redact_outbound(&answer_frame(&id, json!({"value": s})).to_string());
    assert_answer_withheld(&r, "server-response:unknown", &[&s], "event");

    // A different redactor knows nothing of this one's requests.
    let mut first = redactor();
    teach(&mut first, &id, "sudo");
    let r = redactor().redact_outbound(&answer_frame(&id, json!({"value": s})).to_string());
    assert_answer_withheld(&r, "server-response:unknown", &[&s], "other redactor");
}

#[test]
fn open_requests_teach_the_map() {
    // A resume result re-delivers the requests still open. Each entry is `{id, method,
    // params}` and teaches the map like a request frame does.
    let mut red = redactor();
    let (sudo_id, approval_id, clarify_id) = (srq_id(), srq_id(), srq_id());
    let resume = json!({"jsonrpc": "2.0", "id": 7, "result": {
        "session_id": "s1",
        "open_requests": [
            {"id": sudo_id, "method": "sudo", "params": {"session_id": "s1"}},
            {"id": approval_id, "method": "approval", "params": {"session_id": "s1"}},
            {"id": clarify_id, "method": "clarify", "params": {"session_id": "s1"}}]}});
    let r = red.redact_inbound(&resume.to_string());
    assert!(r.parse_error.is_none());

    let s = sentinel();
    let r = red.redact_outbound(&answer_frame(&json!(sudo_id), json!({"value": s})).to_string());
    assert_answer_withheld(&r, "server-response:sudo", &[&s], "open sudo");

    let s = sentinel();
    let r =
        red.redact_outbound(&answer_frame(&json!(clarify_id), json!({"answers": [s]})).to_string());
    assert_answer_withheld(&r, "server-response:clarify", &[&s], "open clarify");

    let s = sentinel();
    let answer = answer_frame(&json!(approval_id), json!({"choice": "once", "note": s}));
    let r = red.redact_outbound(&answer.to_string());
    assert!(!recorded(&r).contains(&s), "open approval: note recorded");
    assert_eq!(at(&r.frame, &["result", "choice"]), Some(&json!("once")));

    // The same id re-delivered after the answer (a reconnect) is remembered again, with the
    // method it had.
    let again = json!({"jsonrpc": "2.0", "id": 8, "result": {"open_requests": [
        {"id": sudo_id, "method": "sudo", "params": {"session_id": "s1"}}]}});
    red.redact_inbound(&again.to_string());
    let s = sentinel();
    let r = red.redact_outbound(&answer_frame(&json!(sudo_id), json!({"value": s})).to_string());
    assert_answer_withheld(&r, "server-response:sudo", &[&s], "re-delivered sudo");

    // Delivered twice before any answer: still one entry with the same method.
    let twice = json!({"jsonrpc": "2.0", "id": 9, "result": {"open_requests": [
        {"id": "srq-aaaaaaaaaaaa", "method": "secret", "params": {"session_id": "s1"}}]}});
    red.redact_inbound(&twice.to_string());
    red.redact_inbound(&twice.to_string());
    let s = sentinel();
    let r = red.redact_outbound(
        &answer_frame(&json!("srq-aaaaaaaaaaaa"), json!({"value": s})).to_string(),
    );
    assert_answer_withheld(&r, "server-response:secret", &[&s], "delivered twice");
}

#[test]
fn cancel_event_and_answer_remove_the_entry() {
    let unknown = "server-response:unknown";

    // The answer removes the entry: a second answer for the same id is unknown.
    {
        let mut red = redactor();
        let id = json!(srq_id());
        teach(&mut red, &id, "sudo");
        let s = sentinel();
        let r = red.redact_outbound(&answer_frame(&id, json!({"value": s})).to_string());
        assert_answer_withheld(&r, "server-response:sudo", &[&s], "first answer");
        let s = sentinel();
        let r = red.redact_outbound(&answer_frame(&id, json!({"value": s})).to_string());
        assert_answer_withheld(&r, unknown, &[&s], "second answer");
    }

    // `request.cancel` is an event frame: `params.type`, id in `params.payload.id`.
    {
        let mut red = redactor();
        let (cancelled, other) = (json!(srq_id()), json!(srq_id()));
        teach(&mut red, &cancelled, "sudo");
        teach(&mut red, &other, "secret");
        let cancel = json!({"jsonrpc": "2.0", "method": "event", "params": {
            "type": "request.cancel", "session_id": "s1",
            "payload": {"id": cancelled, "method": "sudo", "reason": "timeout"}}});
        let r = red.redact_inbound(&cancel.to_string());
        assert!(
            r.frame.as_ref() == Some(&cancel),
            "the cancel event was altered"
        );
        let s = sentinel();
        let r = red.redact_outbound(&answer_frame(&cancelled, json!({"value": s})).to_string());
        assert_answer_withheld(&r, unknown, &[&s], "after cancel");
        // The other request is untouched by the cancel.
        let s = sentinel();
        let r = red.redact_outbound(&answer_frame(&other, json!({"value": s})).to_string());
        assert_answer_withheld(&r, "server-response:secret", &[&s], "other id after cancel");
    }

    // Negative: a frame whose own top-level method is `request.cancel` is not what Hermes
    // sends and clears nothing, whether the id sits in `params.id` or `params.payload.id`.
    {
        let mut red = redactor();
        let id = json!(srq_id());
        teach(&mut red, &id, "sudo");
        let by_method = json!({"jsonrpc": "2.0", "method": "request.cancel", "params": {
            "id": id, "session_id": "s1", "payload": {"id": id}}});
        red.redact_inbound(&by_method.to_string());
        let s = sentinel();
        let r = red.redact_outbound(&answer_frame(&id, json!({"value": s})).to_string());
        assert_answer_withheld(
            &r,
            "server-response:sudo",
            &[&s],
            "top-level request.cancel",
        );
    }

    // An event of another type that mentions the id clears nothing either.
    {
        let mut red = redactor();
        let id = json!(srq_id());
        teach(&mut red, &id, "sudo");
        let other = json!({"jsonrpc": "2.0", "method": "event", "params": {
            "type": "tool.progress", "session_id": "s1", "payload": {"id": id}}});
        red.redact_inbound(&other.to_string());
        let s = sentinel();
        let r = red.redact_outbound(&answer_frame(&id, json!({"value": s})).to_string());
        assert_answer_withheld(&r, "server-response:sudo", &[&s], "other event type");
    }

    // `request.answer` removes the entry like a response frame does.
    {
        let mut red = redactor();
        let id = json!(srq_id());
        teach(&mut red, &id, "approval");
        let by_call = json!({"jsonrpc": "2.0", "id": "c-1", "method": "request.answer",
            "params": {"id": id, "result": {"choice": "once"}}});
        let r = red.redact_outbound(&by_call.to_string());
        assert_eq!(
            at(&r.frame, &["params", "result", "choice"]),
            Some(&json!("once"))
        );
        let s = sentinel();
        let r = red
            .redact_outbound(&answer_frame(&id, json!({"choice": "once", "note": s})).to_string());
        assert_answer_withheld(&r, unknown, &[&s], "response after request.answer");
    }
}

#[test]
fn map_is_bounded_and_fails_closed() {
    let mut red = redactor();
    let ids: Vec<Value> = (0..BOUND_PLUS_MARGIN).map(|_| json!(srq_id())).collect();
    for id in &ids {
        let r = red.redact_inbound(&request_frame(id, "sudo").to_string());
        assert!(r.parse_error.is_none());
    }

    let mut results = Vec::new();
    let mut sentinels = Vec::new();
    for id in &ids {
        let s = sentinel();
        let r = red.redact_outbound(&answer_frame(id, json!({"value": s})).to_string());
        results.push(r);
        sentinels.push(s);
    }
    // Whatever was evicted, no answer reaches the output.
    for (i, (r, s)) in results.iter().zip(&sentinels).enumerate() {
        assert!(
            !recorded(r).contains(s),
            "answer {i}: a sentinel was recorded"
        );
        assert!(
            is_marker(at(&r.frame, &["result"])),
            "answer {i}: result was kept"
        );
    }
    // The oldest was dropped to stay within 1,024 entries and fails closed.
    assert_answer_withheld(
        &results[0],
        "server-response:unknown",
        &[&sentinels[0]],
        "oldest id",
    );
    // The newest is still remembered with its method.
    let last = results.len() - 1;
    assert_answer_withheld(
        &results[last],
        "server-response:sudo",
        &[&sentinels[last]],
        "newest id",
    );
}

#[test]
fn request_answer_result_withheld_unless_approval() {
    // For a remembered request of any method but `approval`, `params.result` is withheld whole.
    for method in non_approval_methods() {
        let mut red = redactor();
        let id = json!(srq_id());
        teach(&mut red, &id, &method);
        let (a, b) = (sentinel(), sentinel());
        let call = json!({"jsonrpc": "2.0", "id": "c-1", "method": "request.answer",
            "params": {"id": id, "result": {"value": a, "answers": [b]}}});
        let r = red.redact_outbound(&call.to_string());
        let text = recorded(&r);
        assert!(
            !text.contains(&a) && !text.contains(&b),
            "{method}: result recorded"
        );
        assert!(
            is_marker(at(&r.frame, &["params", "result"])),
            "{method}: not the marker"
        );
        assert!(
            r.redactions == [redaction("params.result", "deny-set:request.answer")],
            "{method}: got {:?}",
            r.redactions
        );
        // The id and the call's own fields stay.
        assert_eq!(at(&r.frame, &["params", "id"]), Some(&id), "{method}");
    }

    // For a remembered `approval`, only `choice` and `all` are kept.
    let mut red = redactor();
    let id = json!(srq_id());
    teach(&mut red, &id, "approval");
    let note = sentinel();
    let call = json!({"jsonrpc": "2.0", "id": "c-2", "method": "request.answer",
        "params": {"id": id, "result": {"choice": "session", "all": false, "note": note}}});
    let r = red.redact_outbound(&call.to_string());
    assert!(!recorded(&r).contains(&note), "approval: note recorded");
    assert_eq!(
        at(&r.frame, &["params", "result", "choice"]),
        Some(&json!("session"))
    );
    assert_eq!(
        at(&r.frame, &["params", "result", "all"]),
        Some(&json!(false))
    );
    let kept_note = at(&r.frame, &["params", "result", "note"]);
    assert!(
        kept_note.is_none() || is_marker(kept_note),
        "approval: note was kept"
    );
    assert!(
        r.redactions.iter().any(|x| x.path == "params.result.note"),
        "approval: the withheld note is not reported, got {:?}",
        r.redactions
    );

    // An id the redactor never saw: the whole result is withheld.
    let s = sentinel();
    let call = json!({"jsonrpc": "2.0", "id": "c-3", "method": "request.answer",
        "params": {"id": srq_id(), "result": {"choice": "once", "note": s}}});
    let r = redactor().redact_outbound(&call.to_string());
    assert!(!recorded(&r).contains(&s), "unknown id: note recorded");
    assert!(is_marker(at(&r.frame, &["params", "result"])));
    assert!(r.redactions == [redaction("params.result", "deny-set:request.answer")]);
}

#[test]
fn error_response_is_kept() {
    // An error written by Talaria carries a code and a message, no secret; it is kept for a
    // remembered id and for an unknown one.
    let mut red = redactor();
    let id = json!(srq_id());
    teach(&mut red, &id, "sudo");
    let error = json!({"jsonrpc": "2.0", "id": id,
        "error": {"code": -32601, "message": "no handler for sudo"}});
    let r = red.redact_outbound(&error.to_string());
    assert!(
        r.frame.as_ref() == Some(&error),
        "error response was altered"
    );
    assert!(
        r.redactions.is_empty(),
        "error response was reported as withheld"
    );

    let unknown = json!({"jsonrpc": "2.0", "id": srq_id(),
        "error": {"code": -32603, "message": "handler failed"}});
    let r = redactor().redact_outbound(&unknown.to_string());
    assert!(
        r.frame.as_ref() == Some(&unknown),
        "unknown-id error was altered"
    );
    assert!(r.redactions.is_empty());

    // An approval id too.
    let id = json!(srq_id());
    teach(&mut red, &id, "approval");
    let error = json!({"jsonrpc": "2.0", "id": id,
        "error": {"code": -32603, "message": "handler failed"}});
    let r = red.redact_outbound(&error.to_string());
    assert!(
        r.frame.as_ref() == Some(&error),
        "approval error was altered"
    );
    assert!(r.redactions.is_empty());
}

#[test]
fn numeric_id_is_handled() {
    // The map is keyed by the id's JSON text, so a number is remembered when it was seen as a
    // number, and a number is never mistaken for the string with the same digits.
    let mut red = redactor();
    teach(&mut red, &json!(42), "sudo");

    let s = sentinel();
    let r = red.redact_outbound(&answer_frame(&json!("42"), json!({"value": s})).to_string());
    assert_answer_withheld(
        &r,
        "server-response:unknown",
        &[&s],
        "string id for a numeric one",
    );

    let s = sentinel();
    let r = red.redact_outbound(&answer_frame(&json!(43), json!({"value": s})).to_string());
    assert_answer_withheld(&r, "server-response:unknown", &[&s], "number never seen");

    let s = sentinel();
    let r = red.redact_outbound(&answer_frame(&json!(42), json!({"value": s})).to_string());
    assert_answer_withheld(&r, "server-response:sudo", &[&s], "remembered number");
    assert_eq!(at(&r.frame, &["id"]), Some(&json!(42)));

    // A numeric approval id keeps its two keys.
    teach(&mut red, &json!(7), "approval");
    let s = sentinel();
    let answer = answer_frame(
        &json!(7),
        json!({"choice": "once", "all": false, "note": s}),
    );
    let r = red.redact_outbound(&answer.to_string());
    assert!(
        !recorded(&r).contains(&s),
        "numeric approval: note recorded"
    );
    assert_eq!(at(&r.frame, &["result", "choice"]), Some(&json!("once")));
}
