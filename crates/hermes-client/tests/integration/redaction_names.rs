//! Key-name net of the frame redactor.
//!
//! The pure predicate decides which names are credential shaped; the same names are then
//! pushed through a real `Redactor` so the value, reason and path are proved too. The case
//! tables come from spec 4.2 and from `tests/recorder/test_redact.py` at `python-final`.

use hermes_client::{REDACTED, Redacted, Redactor, is_suspicious_key};
use serde_json::{Map, Value, json};

/// Names the net must withhold (spec 4.2 worked examples, then the Python tables).
const WITHHELD: &[&str] = &[
    "password",
    "db_password",
    "DBPASSWORD",
    "adminpassword",
    "passphrase",
    "client_secret",
    "credentials",
    "mycredentials",
    "Authorization",
    "proxy-authorization",
    "api_key",
    "openai_api_key",
    "OPENAI_API_KEY",
    "openaiapikey",
    "anthropicApiKey",
    "X-Api-Key",
    "privateKey",
    "access_token",
    "accesstoken",
    "sessiontoken",
    "authtoken",
    "ticket",
    "sessionticket",
    // Python table.
    "token",
    "authorization",
    "apiKey",
    "authToken",
    "secret",
];

/// Names the net must leave alone (spec 4.2 "Kept" and over-redaction controls, Python table).
const KEPT: &[&str] = &[
    "max_tokens",
    "maxtokens",
    "input_tokens",
    "output_tokens",
    "total_tokens",
    "reasoning_tokens",
    "session_total_tokens",
    "tokens_per_delta",
    "token_estimate",
    "token_line",
    "tokenizer",
    "prompt_id",
    "request_id",
    "submission_id",
    "execution_generation",
    "replay_epoch",
    "monkey",
    "turkey",
    "keyboard",
    "passage",
    "session_key",
    "idempotency_key",
    "session_id",
    "method",
    "params",
    "text",
    "type",
];

fn redactor() -> Redactor {
    Redactor::new(Vec::new()).expect("a redactor with no substitutions builds")
}

/// An inbound event frame carrying `payload`.
fn event(payload: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": { "type": "probe", "payload": payload },
    })
    .to_string()
}

fn payload_of(redacted: &Redacted) -> &Value {
    let frame = redacted.frame.as_ref().expect("the text was valid JSON");
    &frame["params"]["payload"]
}

fn has_redaction(redacted: &Redacted, path: &str, reason: &str) -> bool {
    redacted
        .redactions
        .iter()
        .any(|r| r.path == path && r.reason == reason)
}

#[test]
fn withholds_credential_shaped_keys() {
    for key in WITHHELD {
        assert!(is_suspicious_key(key), "key {key:?} must be suspicious");
    }

    // The same names through a real frame: every value becomes the marker, whatever it was,
    // and the reason is the net's.
    let sentinel = format!("canary-{}", "q".repeat(10));
    let mut payload = Map::new();
    for key in WITHHELD {
        payload.insert((*key).to_string(), json!(sentinel));
    }
    let redacted = redactor().redact_inbound(&event(Value::Object(payload)));

    let out = payload_of(&redacted);
    for key in WITHHELD {
        assert_eq!(
            out[*key],
            json!(REDACTED),
            "value under {key:?} must be withheld"
        );
        let path = format!("params.payload.{key}");
        assert!(
            has_redaction(&redacted, &path, "suspicious-key"),
            "missing suspicious-key redaction for {key:?}"
        );
    }
    let serialized = serde_json::to_string(redacted.frame.as_ref().unwrap()).unwrap();
    assert!(
        !serialized.contains(&sentinel),
        "a sentinel survived in the frame"
    );
}

#[test]
fn matches_camel_and_squashed_forms() {
    // Matches on the camel-normalized form (a separator is inserted at each lower-to-upper
    // boundary), the squashed form (separators dropped), or both.
    let camel = [
        "accessToken",
        "sessionToken",
        "sessionTicket",
        "privateKey",
        "publicKey",
        "signingKey",
        "clientSecret",
        "dbPassword",
        "anthropicApiKey",
        "proxyAuthorization",
    ];
    for key in camel {
        assert!(
            is_suspicious_key(key),
            "camel form {key:?} must be suspicious"
        );
    }

    // Squashed names have no separator left, so the patterns apply as suffixes.
    let squashed = [
        "DBPASSWORD",
        "adminpassword",
        "openaiapikey",
        "xapikey",
        "accesstoken",
        "authtoken",
        "sessiontoken",
        "mycredentials",
        "sessionticket",
        "proxyauthorization",
        "privatekey",
        "signingkey",
        "dbsecret",
    ];
    for key in squashed {
        assert!(
            is_suspicious_key(key),
            "squashed form {key:?} must be suspicious"
        );
    }

    // A separator in the middle of a word still matches through the squashed form.
    for key in ["pass_word", "passPhrase", "pass-phrase", "api.key"] {
        assert!(
            is_suspicious_key(key),
            "{key:?} must be suspicious via its squashed form"
        );
    }

    // `secret` matches anywhere in the name; credentials may be plural or singular.
    for key in [
        "secret",
        "secrets",
        "my_secret_value",
        "SecretValue",
        "credential",
        "credentials",
    ] {
        assert!(is_suspicious_key(key), "{key:?} must be suspicious");
    }

    // Camel-case spellings of counters must not be caught by either form.
    for key in [
        "maxTokens",
        "inputTokens",
        "outputTokens",
        "sessionTotalTokens",
        "promptId",
    ] {
        assert!(
            !is_suspicious_key(key),
            "camel counter {key:?} must be kept"
        );
    }
}

#[test]
fn prefixed_api_keys_and_ticket_withheld() {
    // The Python pattern was anchored at the start, so each of these passed it.
    for key in [
        "openai_api_key",
        "OPENAI_API_KEY",
        "openaiapikey",
        "anthropicApiKey",
        "provider_api_key",
        "X-Api-Key",
        "x_api_key",
        "my.private_key",
        "stripe_secret_key",
        "api_keys",
    ] {
        assert!(
            is_suspicious_key(key),
            "prefixed key {key:?} must be suspicious"
        );
    }

    // `ticket` in every spelling the contract and Hermes use.
    for key in [
        "ticket",
        "Ticket",
        "TICKET",
        "session_ticket",
        "sessionTicket",
        "sessionticket",
    ] {
        assert!(
            is_suspicious_key(key),
            "ticket name {key:?} must be suspicious"
        );
    }

    // The value is withheld whatever its type.
    let cases = [
        ("openai_api_key", json!("a string")),
        ("X-Api-Key", json!(12345)),
        ("ticket", json!(true)),
        ("anthropicApiKey", json!({"nested": "object"})),
        ("OPENAI_API_KEY", json!(["an", "array"])),
    ];
    for (name, value) in cases {
        let mut payload = Map::new();
        payload.insert(name.to_string(), value);
        let redacted = redactor().redact_inbound(&event(Value::Object(payload)));
        assert_eq!(
            payload_of(&redacted)[name],
            json!(REDACTED),
            "value under {name:?}"
        );
        let path = format!("params.payload.{name}");
        assert!(
            has_redaction(&redacted, &path, "suspicious-key"),
            "redaction entry for {name:?}"
        );
        // A withheld object is one redaction, not one per child.
        let child_prefix = format!("{path}.");
        assert!(
            !redacted
                .redactions
                .iter()
                .any(|r| r.path.starts_with(&child_prefix)),
            "a withheld value must not also list its children"
        );
    }
}

#[test]
fn keeps_counters_and_ids() {
    for key in KEPT {
        assert!(
            !is_suspicious_key(key),
            "key {key:?} must not be suspicious"
        );
    }
    assert!(!is_suspicious_key(""), "the empty name is not a credential");

    // A usage frame must round-trip numerically intact, with no redaction at all.
    let usage = json!({
        "input_tokens": 1200,
        "output_tokens": 340,
        "max_tokens": 4096,
        "session_total_tokens": 98765,
        "tokens_per_delta": 3,
        "prompt_id": "p-1",
        "request_id": "r-1",
        "submission_id": "s-1",
        "execution_generation": 2,
        "replay_epoch": 7,
        "tokenizer": "bpe",
    });
    let text = event(usage.clone());
    let redacted = redactor().redact_inbound(&text);
    assert!(
        redacted.redactions.is_empty(),
        "counters and ids must never be redacted"
    );
    assert_eq!(payload_of(&redacted), &usage);
    assert!(redacted.parse_error.is_none());
}

#[test]
fn free_text_names_are_withheld_in_any_position() {
    // `is_suspicious_key` covers the net only, so these names are proved through a real frame.
    // They fill a snapshot's `info` block; `model`, `provider` and `cwd` must stay.
    let marker = format!("free-text-{}", "k".repeat(12));
    let info = json!({
        "model": "fixture-model",
        "provider": "fixture-provider",
        "cwd": "<tmp>",
        "system_prompt": marker,
        "credential_hint": marker,
        "launch_request": { "argv": [marker], "cwd": marker },
    });
    let redacted = redactor().redact_inbound(&event(json!({ "info": info })));

    let out = &payload_of(&redacted)["info"];
    assert_eq!(out["model"], json!("fixture-model"));
    assert_eq!(out["provider"], json!("fixture-provider"));
    assert_eq!(out["cwd"], json!("<tmp>"));
    for name in ["system_prompt", "credential_hint", "launch_request"] {
        assert_eq!(out[name], json!(REDACTED), "{name} must be withheld");
        let path = format!("params.payload.info.{name}");
        assert!(
            has_redaction(&redacted, &path, "free-text"),
            "free-text entry for {name}"
        );
    }
    let serialized = serde_json::to_string(redacted.frame.as_ref().unwrap()).unwrap();
    assert!(
        !serialized.contains(&marker),
        "free text survived in the frame"
    );

    // Any depth, inside arrays, at the top of params, and on the outbound side.
    let deep =
        json!({ "a": [ { "b": { "system_prompt": marker } }, { "credential_hint": marker } ] });
    let redacted = redactor().redact_inbound(&event(deep));
    assert!(has_redaction(
        &redacted,
        "params.payload.a.0.b.system_prompt",
        "free-text"
    ));
    assert!(has_redaction(
        &redacted,
        "params.payload.a.1.credential_hint",
        "free-text"
    ));

    let outbound = json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "probe.method",
        "params": { "launch_request": { "x": marker } },
    })
    .to_string();
    let redacted = redactor().redact_outbound(&outbound);
    let frame = redacted.frame.as_ref().expect("valid JSON");
    assert_eq!(frame["params"]["launch_request"], json!(REDACTED));
    assert!(has_redaction(
        &redacted,
        "params.launch_request",
        "free-text"
    ));
}

#[test]
fn net_runs_on_unknown_methods_arrays_and_depth() {
    // Ported from test_redact.py: protocol drift (an unknown method), a credential nested at
    // arbitrary depth, and one inside an array.
    let canary = format!("canary-{}", "d".repeat(10));
    let frame = json!({
        "method": "future.unknown.respond",
        "params": { "api_key": canary },
    })
    .to_string();
    let redacted = redactor().redact_outbound(&frame);
    assert!(
        !serde_json::to_string(redacted.frame.as_ref().unwrap())
            .unwrap()
            .contains(&canary)
    );
    assert!(has_redaction(&redacted, "params.api_key", "suspicious-key"));

    let deep = event(json!({ "config": { "provider": { "token": canary } } }));
    let redacted = redactor().redact_inbound(&deep);
    assert!(
        !serde_json::to_string(redacted.frame.as_ref().unwrap())
            .unwrap()
            .contains(&canary)
    );
    assert!(has_redaction(
        &redacted,
        "params.payload.config.provider.token",
        "suspicious-key"
    ));

    let in_array = json!({
        "method": "batch",
        "params": { "items": [ { "ok": 1 }, { "password": canary } ] },
    })
    .to_string();
    let redacted = redactor().redact_outbound(&in_array);
    assert!(
        !serde_json::to_string(redacted.frame.as_ref().unwrap())
            .unwrap()
            .contains(&canary)
    );
    assert!(has_redaction(
        &redacted,
        "params.items.1.password",
        "suspicious-key"
    ));
}
