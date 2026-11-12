# TRU-4: Debug output of the recorder options and the redactor can print a single-use ticket

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the types as they are today, for why the query key stays, and for the edge cases around a substitution pair. The requirements, the tests, and the acceptance checks are only in the issue. This page points at them by id.

## What exists today

`RecorderOptions` (`crates/hermes-client/src/record.rs:38-48`) derives `Debug`, `Clone`, and `Default`. The fields are `endpoint`, `subprotocols`, `gateway`, `scenario`, `expected_statuses`, `substitutions`, and `queue_capacity`. `substitutions` is `Vec<(String, String)>`. REQ-1 keeps `Clone` and `Default` and replaces only the derived Debug.

The ticket prefix is private (`record.rs:28`):

```rust
const TICKET_PROTOCOL_PREFIX: &str = "hermes-gateway-ticket.";
```

`Recorder::from_writer` (`record.rs:224-229`) builds `Redactor::new(options.substitutions.clone())` and then calls `start`. A needle shorter than four characters makes `Redactor::new` return `RecordError::NeedleTooShort` (`redact.rs:141-147`). `short_needle_refused` (`crates/hermes-client/tests/record_file.rs:707`) locks that refusal. Debug of options that have not been started must not take that error path. REQ-2 does not apply those needles to the endpoint and does not pass them to `Redactor::new`. The options' Debug still prints each short pair as `<redacted> -> <placeholder>`, and Debug returns normally. The text to find is not printed. `record_file.rs::debug_format_prints_a_short_pair` locks that print. It does not call `Redactor::new`.

`Recorder::start` (`record.rs:232`) writes the header before it returns. The endpoint and the subprotocols are (`record.rs:242-248`):

```rust
endpoint: redactor.substitute(&redact_url(&options.endpoint)),
subprotocols: options
    .subprotocols
    .iter()
    .map(String::as_str)
    .filter(|protocol| !protocol.starts_with(TICKET_PROTOCOL_PREFIX))
    .collect(),
```

REQ-2 and REQ-4 copy those two steps into Debug. They do not edit `start`.

`Redactor` (`redact.rs:132-136`) derives `Debug` and holds private fields:

```rust
pub struct Redactor {
    open: OpenRequests,
    substitutions: Vec<(String, String)>,
}
```

`OpenRequests` (`redact.rs:100-103`, with its derive at line 99) is private and itself derives `Debug`. It stores `methods: HashMap<String, String>` and `order: VecDeque<String>`. The field `open` on `Redactor` is at `redact.rs:134`. Derived Debug of a redactor that has seen frames would print request ids and method names. REQ-6 prints each stored pair as `<redacted> -> <placeholder>` and does not print `open`.

`Redactor::substitute` (`redact.rs:167-175`) is `pub(crate)`. It walks the pairs in their stored order and replaces each needle that occurs. It does not sort. `Redactor::new` sorts that store longest needle first (`redact.rs:149`, `Reverse` of `chars().count()`) only when every needle has at least four characters. Options can still hold an unsorted list, because Debug can run before `new`. REQ-2 applies that same order on a copy and does not reorder the struct. `Redactor::new` on only the needles it would keep, then `substitute`, is that order. Sorting the copy with the same key and walking it is the same order. REQ-2 adds no function.

`redact_url` (`redact.rs:614-616`) is public:

```rust
pub fn redact_url(url: &str) -> String {
    clean_url(url).unwrap_or_else(|| REDACTED.to_owned())
}
```

`REDACTED` is `[redacted]` (`redact.rs:17`). The percent-encoded marker inside a URL is private `URL_MARK`, `%5Bredacted%5D` (`redact.rs:20`). `clean_pair` (`redact.rs:576-588`) keeps the original key text and replaces a non-empty credential value:

```rust
if credential && !value.is_empty() {
    format!("{key}={URL_MARK}")
}
```

The credential test is the percent-decoded, lower-cased key matching `token`, `ticket`, or `internal`, or `is_suspicious_key` (`redact.rs:582-583`). `header_endpoint_goes_through_redact_url` (`record_file.rs:453`) expects the file endpoint `wss://%5Bredacted%5D@host:9119/api/ws?Ticket=%5Bredacted%5D&mode=1#%5Bredacted%5D` (`record_file.rs:473`). The key `Ticket` remains. Test 1 therefore asserts that `ticket=%5Bredacted%5D` is present. An assertion that `ticket=` is absent would fail after the header's own redaction.

`header_drops_ticket_subprotocols` (`record_file.rs:423`) asserts the file's `subprotocols` array and asserts that the bytes do not contain `hermes-gateway-ticket`. It does not format with `{:?}`. Both file tests stay as they are. AC-2 and AC-3 run them.

`Shared` (`record.rs:110-114`) already has a manual Debug that writes `Recorder`. `TokenSet` and `WsTicket` (`crates/hermes-client/src/remote/mod.rs:344-352`) write:

```rust
f.write_str("<redacted>")
```

That is the only Debug mark this crate already uses for a secret. The pair rule uses that same mark for the text to find, then `->`, then the placeholder as stored. REQ-7 keeps the mark those two impls write. `[redacted]` is the file marker. `%5Bredacted%5D` appears in an endpoint only because `redact_url` put it there. REQ-5 uses `<redacted> -> <placeholder>` on both types. The options struct stores the same pairs the redactor stores. Printing those pairs with derived Debug would show the text to find, which test 1 forbids.

`hermes_client` exports both types (`crates/hermes-client/src/lib.rs:33-34` and `:71-77`). The integration test can name `RecorderOptions` and `Redactor`. It cannot name `TICKET_PROTOCOL_PREFIX`. The impl in `record.rs` can.

`RemoteDial::dial` (`crates/hermes-client/src/remote/dial.rs:85-104`) already builds options whose endpoint is `dial_url(...).to_string()` and whose only subprotocol is `hermes-gateway-v1`. The pair is the real host as the text to find and `gateway.example.com` as the placeholder (`dial.rs:99-103`). `dial_url` (`connect.rs:6`) builds that URL, and line 14 appends the query pair `ticket`. The placeholder is the public stand-in, not the ticket. The text to find can still be a secret (`Redactor` stores whatever `new` was given). REQ-2 writes that placeholder into the endpoint wherever the host occurred, the same string the header writes. REQ-5 prints `<redacted> -> gateway.example.com`, so the host is not printed and the stand-in is. This issue does not edit the dial.

`sentinel` (`record_file.rs:19-21`) is `format!("zq{:032x}", rand::random::<u128>())`. Test 1 uses it so the test file does not contain a fixed ticket. `fixture_scan.rs::no_secret_shaped_strings` (`crates/hermes-client/tests/fixture_scan.rs:94`) scans fixture files. Test 1 adds no fixture.

The existing helper `options()` in the same test file builds an endpoint for the file tests. Test 1 does not call it. The endpoint host in test 1 is `gateway-host.test`, the text to find. The placeholder is `<gateway>`, the kind of stand-in the frame-log page names. `gateway.example.com` remains the stand-in `RemoteDial::dial` writes (`dial.rs:103`).

## Why the secret is the text to find

A pair is the text to find and the placeholder written in its place. `Redactor::substitute` (`redact.rs:167-171`) replaces the first string with the second. The frame-log page (`docs/formats/frame-log.md:145-148`) says needles are replaced by placeholders such as `<gateway>`. The text to find is the secret: a host name, a token, a path. The placeholder is safe and is what the recording header already shows. `RemoteDial::dial` (`dial.rs:99-103`) stores the real host as the first string and `gateway.example.com` as the second. Debug of that pair prints `<redacted> -> gateway.example.com`.

Test 1 puts the sentinel in the query value and in the ticket-subprotocol suffix. The pair is `(gateway-host.test, <gateway>)`. The endpoint host is `gateway-host.test`, so Debug writes `<gateway>` there and must not print `gateway-host.test`. `redact_url` runs first, so the query value is already `%5Bredacted%5D` before any needle is applied.

## Edge cases

| Input | What Debug does | Which requirement |
| --- | --- | --- |
| Query `ticket=<sentinel>` on an absolute URL | Key stays, value becomes `%5Bredacted%5D` | REQ-3 |
| Subprotocol `hermes-gateway-ticket.<sentinel>` beside `hermes-gateway-v1` | Ticket entry omitted, `hermes-gateway-v1` printed | REQ-4 |
| Pair `(gateway-host.test, <gateway>)` and the host is the endpoint host | Endpoint shows `<gateway>`, as the header would. The pair list shows `<redacted> -> <gateway>` and does not show `gateway-host.test` | REQ-2, REQ-5 |
| Needle shorter than four characters on options that were never started | Not applied to the endpoint and not passed to `Redactor::new`. The options list prints `<redacted> -> <placeholder>`. The text to find is absent. Debug does not panic. `debug_format_prints_a_short_pair` | REQ-2, REQ-5 |
| Endpoint that is not an absolute URL with a host | `[redacted]` | REQ-3 |
| `Redactor.open` holding request ids | Not printed | REQ-6 |
| Placeholder `gateway.example.com` and the needle occurs in the endpoint | Endpoint shows `gateway.example.com`, as the header would. The pair list shows `<redacted> -> gateway.example.com` | REQ-2, REQ-5 |
| `Shared`, `TokenSet`, `WsTicket` | Unchanged | REQ-7 |

`docs/formats/frame-log.md` lines 12-15 name the writer `crates/hermes-client/src/record.rs`, the reader `crates/hermes-client/src/replay_source.rs`, and the redaction rules `crates/hermes-client/src/redact.rs`. A change to any of those files changes the format file in the same pull request. The header table there already describes a cleaned endpoint and the removal of `hermes-gateway-ticket.` entries. This issue does not change those bytes. It adds one short paragraph after that sentence: the recorder options and the redactor have hand-written `Debug` output that never shows an endpoint credential, a ticket or the text being redacted, and the recording format itself does not change. No other edit to that page.
