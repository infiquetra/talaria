//! URL cleaning by position.
//!
//! `redact_url` edits the original string and never re-serializes it, so the exact output
//! bytes below are part of the contract. The userinfo, query and fragment values are fake
//! placeholders, not credentials.

use std::io::Write;
use std::sync::{Arc, Mutex};

use hermes_client::{REDACTED, Recorder, RecorderOptions, Redactor, redact_url, scan_for_secrets};
use hermes_contract::schemas::McpOauthStartResult;

/// The percent-encoded marker that stands where a URL part was withheld.
const MARK: &str = "%5Bredacted%5D";

fn check(cases: &[(&str, &str)]) {
    for (input, expected) in cases {
        let got = redact_url(input);
        assert_eq!(&got, expected, "redact_url({input:?})");
        // Cleaning is idempotent: the marker is itself a withheld-looking value.
        assert_eq!(
            &redact_url(&got),
            expected,
            "redact_url applied twice to {input:?}"
        );
    }
}

#[test]
fn withholds_userinfo_query_and_fragment() {
    check(&[
        // Spec 4.3 table.
        (
            "wss://user:pw@host:9119/api/ws?mode=1",
            "wss://%5Bredacted%5D@host:9119/api/ws?mode=1",
        ),
        ("https://tok@Host/Path", "https://%5Bredacted%5D@Host/Path"),
        (
            "ws://host/api/ws?token=abc&mode=1",
            "ws://host/api/ws?token=%5Bredacted%5D&mode=1",
        ),
        ("https://host/page#frag", "https://host/page#%5Bredacted%5D"),
        // Python table: the gateway attach token, the other pair kept.
        (
            "ws://127.0.0.1:8765/api/ws?token=abc123&session=s-1",
            "ws://127.0.0.1:8765/api/ws?token=%5Bredacted%5D&session=s-1",
        ),
        // A user name with no password is still withheld (a token is often sent as the name).
        (
            "wss://onlyname@host/api/ws",
            "wss://%5Bredacted%5D@host/api/ws",
        ),
        // Userinfo is the part before the LAST `@` of the authority.
        ("ws://a@b:c@host/p", "ws://%5Bredacted%5D@host/p"),
        // No path: the authority runs to the end of the string.
        ("wss://u:p@Host:9119", "wss://%5Bredacted%5D@Host:9119"),
        // All three places at once.
        (
            "wss://u:p@h/p?token=t&mode=1#f",
            "wss://%5Bredacted%5D@h/p?token=%5Bredacted%5D&mode=1#%5Bredacted%5D",
        ),
        // The value is split at the first `=` only.
        (
            "ws://host/api/ws?token=a=b&mode=1",
            "ws://host/api/ws?token=%5Bredacted%5D&mode=1",
        ),
        // A fragment is withheld whole, whatever it holds.
        (
            "https://host/page?a=1#section=2&x=y",
            "https://host/page?a=1#%5Bredacted%5D",
        ),
    ]);

    // The marker the tests expect is the percent-encoded form of the plain marker.
    assert_eq!(MARK, "%5Bredacted%5D");
    assert_eq!(REDACTED, "[redacted]");
}

#[test]
fn ticket_internal_and_mixed_case_keys() {
    check(&[
        (
            "ws://host/api/ws?Ticket=abc",
            "ws://host/api/ws?Ticket=%5Bredacted%5D",
        ),
        (
            "ws://host/api/ws?api_key=k&internal=i",
            "ws://host/api/ws?api_key=%5Bredacted%5D&internal=%5Bredacted%5D",
        ),
        // Keys are compared lower-cased and keep their original spelling.
        (
            "ws://host/api/ws?INTERNAL=x&TOKEN=y",
            "ws://host/api/ws?INTERNAL=%5Bredacted%5D&TOKEN=%5Bredacted%5D",
        ),
        (
            "ws://host/api/ws?mode=1&Token=x&y=2",
            "ws://host/api/ws?mode=1&Token=%5Bredacted%5D&y=2",
        ),
        // Keys are percent-decoded first: `to%6Ben` is `token`, `Api%5FKey` is `api_key`.
        (
            "ws://host/api/ws?to%6Ben=abc&mode=1",
            "ws://host/api/ws?to%6Ben=%5Bredacted%5D&mode=1",
        ),
        (
            "ws://host/api/ws?Api%5FKey=v&mode=1",
            "ws://host/api/ws?Api%5FKey=%5Bredacted%5D&mode=1",
        ),
        // Any name the key-name net withholds counts, in camel, squashed or plain spelling.
        (
            "ws://host/api/ws?apiKey=v&accessToken=v&password=v",
            "ws://host/api/ws?apiKey=%5Bredacted%5D&accessToken=%5Bredacted%5D&password=%5Bredacted%5D",
        ),
        (
            "ws://host/api/ws?client_secret=v&Authorization=v",
            "ws://host/api/ws?client_secret=%5Bredacted%5D&Authorization=%5Bredacted%5D",
        ),
    ]);
}

#[test]
fn clean_url_is_byte_identical() {
    // Nothing to withhold: the output is the input, byte for byte. In particular the host's
    // case, a missing trailing slash and percent-escapes elsewhere are not normalized.
    let clean = [
        "ws://127.0.0.1:9119/api/ws",
        "https://host/page?a=1&b",
        "https://Host.Example:9119",
        "wss://HOST/Api/Ws",
        "https://host/a%2Fb%20c?x=%7E",
        "http://[::1]:9119/api/ws",
        // An empty fragment is left alone.
        "https://host/page#",
        "https://host/p?a=1#",
        // A pair with no `=` is left alone, even when its name is a credential name.
        "https://host/page?token&mode=1",
        // Counters and ordinary names in a query are kept.
        "https://host/p?max_tokens=5&session=s-1&Mode=1",
        "https://host/p?%6Dode=1",
        // An `@` after the authority ends is not userinfo.
        "https://host/p?email=a@b.example",
        "https://host?x=a@b",
        // An empty query and an empty pair.
        "https://host/page?",
        "ws://host/api/ws?mode=1&&x=2",
        // Paths are not covered (spec 4.3): a capability in a path is recorded as is.
        "ws://127.0.0.1:9222/devtools/browser/0a1b2c3d",
        // `+` in an ordinary key is part of that key, not a reason to withhold the URL.
        "https://host/p?a+b=1&mode=1",
    ];
    for url in clean {
        assert_eq!(
            redact_url(url),
            url,
            "redact_url({url:?}) must be unchanged"
        );
    }
}

#[test]
fn oauth_code_and_state_are_redacted() {
    check(&[
        (
            "https://example.invalid/callback?code=abc&state=def",
            "https://example.invalid/callback?code=%5Bredacted%5D&state=%5Bredacted%5D",
        ),
        (
            "https://example.invalid/callback?State=Nonce&CODE=Abc&mode=1",
            "https://example.invalid/callback?State=%5Bredacted%5D&CODE=%5Bredacted%5D&mode=1",
        ),
        (
            "https://example.invalid/callback?st%61te=nonce&c%6Fde=abc",
            "https://example.invalid/callback?st%61te=%5Bredacted%5D&c%6Fde=%5Bredacted%5D",
        ),
        (
            "https://example.invalid/callback?state=secr%65t&code=a%62c&keep=1",
            "https://example.invalid/callback?state=%5Bredacted%5D&code=%5Bredacted%5D&keep=1",
        ),
        (
            "https://example.invalid/callback?state=&code=&mode=1",
            "https://example.invalid/callback?state=&code=&mode=1",
        ),
        (
            "https://example.invalid/callback#code=abc&state=def",
            "https://example.invalid/callback#%5Bredacted%5D",
        ),
        (
            "https://example.invalid/callback?code=abc&state=def#code=zzz&state=yyy",
            "https://example.invalid/callback?code=%5Bredacted%5D&state=%5Bredacted%5D#%5Bredacted%5D",
        ),
        (
            "https://example.invalid/callback?state=a+b&mode=1",
            "https://example.invalid/callback?state=%5Bredacted%5D&mode=1",
        ),
    ]);
}

#[test]
fn recorded_oauth_start_drops_auth_url_state_and_code() {
    let raw = "secret_nonce";
    let url = format!("https://example.invalid/authorize?state={raw}&code=abc");
    let frame = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "result": {
            "auth_url": url,
            "method": "mcp.oauth.start"
        }
    });
    let mut redactor = Redactor::new(Vec::new()).expect("redactor");
    let redacted = redactor.redact_inbound(&frame.to_string());
    let recorded = redacted
        .frame
        .as_ref()
        .map(serde_json::Value::to_string)
        .unwrap_or_default();
    assert!(
        !recorded.contains(raw),
        "the recorded auth_url still holds the oauth state"
    );
    assert!(
        !recorded.contains("code=abc"),
        "the recorded auth_url still holds the oauth code"
    );
    assert!(
        recorded.contains("state=%5Bredacted%5D"),
        "state was not replaced with the marker"
    );
    assert!(
        recorded.contains("code=%5Bredacted%5D"),
        "code was not replaced with the marker"
    );
}

#[test]
fn unredacted_oauth_query_is_flagged_until_redacted() {
    let live = [
        "https://example.invalid/callback?state=secret_nonce",
        "https://example.invalid/callback?code=abc&state=def",
        "https://example.invalid/callback?State=Nonce&CODE=Abc",
        "https://example.invalid/callback?st%61te=secret_nonce&c%6Fde=abc",
        "https://example.invalid/callback?mode=1&code=abc&state=def",
        "https://example.invalid/callback#code=abc&state=secret_nonce",
        "https://example.invalid/callback?keep=1#state=secret_nonce",
    ];
    for url in live {
        let hits = scan_for_secrets(url);
        assert!(
            hits.iter().any(|hit| hit.rule == "url-secret-query"),
            "a live oauth query was not flagged: {url}"
        );
        let cleaned = redact_url(url);
        assert!(
            scan_for_secrets(&cleaned).is_empty(),
            "the redacted oauth url was still flagged: {cleaned}"
        );
    }

    let clean = [
        "https://example.invalid/callback?state=&code=",
        "https://example.invalid/callback?state=%5Bredacted%5D&code=%5Bredacted%5D",
        "https://example.invalid/callback?state=[redacted]&code=[redacted]",
        "https://example.invalid/callback#",
        "https://example.invalid/callback#%5Bredacted%5D",
        "https://example.invalid/callback?mode=1&keep=yes",
    ];
    for url in clean {
        assert!(
            scan_for_secrets(url).is_empty(),
            "a clean oauth url was flagged: {url}"
        );
    }
}

#[test]
fn non_url_endpoint_is_withheld() {
    assert_eq!(REDACTED, "[redacted]");
    // Python table: anything that cannot be parsed is withheld rather than guessed at.
    assert_eq!(redact_url("not a url"), REDACTED);
    for input in [
        "",
        "   ",
        "/just/a/path?token=abc",
        "host.example/api/ws",
        "relative?token=abc",
    ] {
        let got = redact_url(input);
        assert_eq!(
            got, REDACTED,
            "redact_url({input:?}) must withhold a non-URL"
        );
    }
}

fn assert_oauth_value_redacted(url: &str, secret: &str, code: &str, keeps_mode: bool) {
    let cleaned = redact_url(url);
    assert!(
        cleaned.contains("example.invalid"),
        "a tab or newline in the name withheld the whole url: {cleaned}"
    );
    assert!(
        cleaned.contains(MARK),
        "the oauth value was not marked: {cleaned}"
    );
    assert!(!cleaned.contains(secret), "oauth state survived: {cleaned}");
    assert!(!cleaned.contains(code), "oauth code survived: {cleaned}");
    if keeps_mode {
        assert!(
            cleaned.contains("mode=1"),
            "an ordinary pair was dropped: {cleaned}"
        );
    }
    assert!(
        scan_for_secrets(&cleaned).is_empty(),
        "the redacted url was flagged: {cleaned}"
    );
}

#[test]
fn ascii_tab_or_newline_inside_oauth_names_is_redacted() {
    let secret = "secret_nonce";
    let code = "oauth-code-value";
    let tabbed = format!("https://example.invalid/callback?st\tate={secret}&co\tde={code}");
    assert_oauth_value_redacted(&tabbed, secret, code, false);
    assert!(
        scan_for_secrets(&tabbed)
            .iter()
            .any(|hit| hit.rule == "url-secret-query"),
        "a tab inside an oauth name was not flagged"
    );

    // The newline stays inside one JSON string. A raw newline would be a new log line.
    let decoded = format!("https://example.invalid/callback?st\nate={secret}&co\rde={code}&mode=1");
    assert_oauth_value_redacted(&decoded, secret, code, true);
    let line = serde_json::json!({ "auth_url": decoded }).to_string();
    assert!(
        !line.contains('\n'),
        "the json line must keep the newline inside the string"
    );
    assert!(
        scan_for_secrets(&line)
            .iter()
            .any(|hit| hit.line == 1 && hit.rule == "url-secret-query"),
        "a newline inside an oauth name was not flagged: {line}"
    );
}

#[test]
fn json_escaped_oauth_parameters_are_flagged_on_their_line() {
    let secret = "secret_nonce";
    let code = "oauth-code-value";
    let query = format!(
        "{{\"auth_url\":\"https://example.invalid/authorize?state\\u003d{secret}\\u0026code\\u003d{code}\"}}"
    );
    let routed = format!(
        "{{\"auth_url\":\"https://example.invalid/app#/callback?code\\u003d{code}\\u0026state\\u003d{secret}\"}}"
    );
    let apostrophe =
        format!("https://example.invalid/callback?name=o'brien&state={secret}&code={code}");
    assert!(
        !query.contains(&format!("state={secret}")),
        "the fixture must keep the equals sign escaped"
    );
    let text = format!("plain\n{query}\n{routed}\n{apostrophe}\n");
    let hits = scan_for_secrets(&text);
    for line in [2, 3, 4] {
        assert!(
            hits.iter()
                .any(|hit| hit.line == line && hit.rule == "url-secret-query"),
            "line {line} was not flagged: {hits:?}"
        );
    }
    let debug = format!("{hits:?}");
    assert!(!debug.contains(secret));
    assert!(!debug.contains(code));
}

#[test]
fn oauth_scan_ignores_prose_paths_and_encoded_markers() {
    let clean = [
        "Set state=secret_nonce and code=oauth-code-value in the form.",
        "The docs mention ?state=secret_nonce and #code=oauth-code-value as names.",
        "https://example.invalid/files/state=secret_nonce/code=oauth-code-value",
        "https://example.invalid/callback/state=secret_nonce",
        "https://example.invalid/callback?state=%5bredacted%5d&code=%5Bredacted%5D",
        "https://example.invalid/callback?state=%255Bredacted%255D&code=%255bredacted%255d",
        "https://example.invalid/app#/callback?code=%5Bredacted%5D&state=%5bredacted%5d",
    ];
    for text in clean {
        assert!(
            scan_for_secrets(text).is_empty(),
            "benign text was flagged: {text} {:?}",
            scan_for_secrets(text)
        );
    }
    let routed = "https://example.invalid/app#/callback?code=oauth-code-value";
    assert!(
        scan_for_secrets(routed)
            .iter()
            .any(|hit| hit.rule == "url-secret-query"),
        "a code parameter in a routed fragment was not flagged"
    );
}

#[test]
fn recorder_oauth_start_result_drops_live_state_and_code() {
    let secret = "secret_nonce";
    let code = "oauth-code-value";
    let result = McpOauthStartResult {
        auth_url: format!("https://example.invalid/authorize?state={secret}&code={code}"),
        flow: "authorization_code".to_owned(),
        ok: true,
        session_id: "oauth-session".to_owned(),
    };
    let frame = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 11,
        "result": result,
    });
    let log = record_frame(&frame);
    assert!(
        !log.contains(secret),
        "the recording still holds the oauth state"
    );
    assert!(
        !log.contains(&format!("code={code}")),
        "the recording still holds the oauth code"
    );
    assert!(log.contains("state=%5Bredacted%5D"));
    assert!(log.contains("code=%5Bredacted%5D"));
    assert!(
        scan_for_secrets(&log).is_empty(),
        "the recording was flagged: {:?}",
        scan_for_secrets(&log)
    );
}

#[test]
fn recorder_drops_oauth_values_nested_in_a_json_string() {
    let secret = "secret_nonce";
    let code = "oauth-code-value";
    let inner = serde_json::json!({
        "auth_url": format!("https://example.invalid/authorize?state={secret}&code={code}"),
    });
    let frame = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 12,
        "result": { "payload": inner.to_string() }
    });
    let log = record_frame(&frame);
    assert!(
        !log.contains(secret),
        "nested json still holds the oauth state: {log}"
    );
    assert!(
        !log.contains(&format!("code={code}")),
        "nested json still holds the oauth code: {log}"
    );
    assert!(
        scan_for_secrets(&log).is_empty(),
        "the nested recording was flagged: {:?}",
        scan_for_secrets(&log)
    );
}

struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuf {
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

fn record_frame(frame: &serde_json::Value) -> String {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder::from_writer(
        SharedBuf(Arc::clone(&sink)),
        RecorderOptions {
            endpoint: "ws://127.0.0.1:9119/api/ws".to_owned(),
            ..RecorderOptions::default()
        },
    )
    .expect("recorder");
    recorder.record_inbound(1, &frame.to_string());
    recorder.finish().expect("finish");
    let bytes = sink
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    String::from_utf8(bytes).expect("the log is utf-8")
}
