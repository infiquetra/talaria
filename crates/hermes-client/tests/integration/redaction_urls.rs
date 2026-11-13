//! URL cleaning by position.
//!
//! `redact_url` edits the original string and never re-serializes it, so the exact output
//! bytes below are part of the contract. The userinfo, query and fragment values are fake
//! placeholders, not credentials.

use hermes_client::{REDACTED, redact_url};

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
