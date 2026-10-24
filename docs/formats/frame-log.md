# Frame log format

Status: `active`
Authority: `contract`
Version: 3

A frame log is the recording Talaria makes of its conversation with a Hermes gateway: every
JSON-RPC frame (one WebSocket text message) in both directions, with secrets withheld before
anything is written. The fixtures under `fixtures/frames/`, the session reducer's tests, the screen
snapshots and the performance harness all read it.

This file is the contract. The writer is `crates/hermes-client/src/record.rs`, the reader is
`crates/hermes-client/src/replay_source.rs`, and the redaction rules are in
`crates/hermes-client/src/redact.rs`. A change to any of them changes this file in the same pull
request.

## Shape

JSON Lines: UTF-8, one JSON object per line, every line ending in a newline, appended and never
rewritten. The first line is the header; every later line is a record.

Readers ignore fields they do not know and skip records whose `kind` they do not know. A reader
stops with a clear error on a header whose `version` is not 3.

### Header

```json
{
  "kind": "header",
  "version": 3,
  "startedAt": "2026-10-23T09:14:02.331Z",
  "endpoint": "ws://<gateway>/api/ws",
  "subprotocols": ["hermes-gateway-v1"],
  "gateway": { "codeVersion": "0.22.4", "codeSha": "<sha>" },
  "scenario": "approval",
  "expectedStatuses": ["complete"]
}
```

| Field | Meaning |
|---|---|
| `kind` | Always `header`. |
| `version` | Always 3 for this layout. |
| `startedAt` | When recording began, ISO 8601 in UTC with milliseconds. Written once; records carry `tMs`, not a clock time. |
| `endpoint` | The WebSocket URL dialled, cleaned by the URL rule below. An endpoint that is not an absolute URL is written as the marker `[redacted]` alone; the header has no `redactions` list. |
| `subprotocols` | The subprotocols offered, with every entry starting `hermes-gateway-ticket.` removed. Never holds a ticket. |
| `gateway` | Optional. `codeVersion` and `codeSha` as the gateway's `identify` reported them (public build identifiers). |
| `scenario` | Optional. The fixture scenario this file records. |
| `expectedStatuses` | Optional. The `message.complete` status each turn is expected to end with, in order. |

A file with only a header is a valid recording of a session in which nothing crossed the socket.

### Records

```json
{"kind":"frame","seq":3,"tMs":152,"generation":1,"dir":"in","frame":{"jsonrpc":"2.0","method":"event","params":{}}}
{"kind":"dropped","seq":4,"tMs":160,"count":12}
```

| Field | Meaning |
|---|---|
| `kind` | `frame`, `dropped`, or a kind a reader does not know (skipped). |
| `seq` | Position among the records after the header, from 1, gapless. A `dropped` record takes a number. |
| `tMs` | Milliseconds since the recorder was created, from a monotonic clock. Never decreases. Replay paces by it. |
| `generation` | The connection generation the frame crossed; the first connection is 1. The recorder copies it from the caller. |
| `dir` | `in` from the gateway, `out` from Talaria. |
| `frame` | The frame after redaction and substitution, or `null` when the text was not valid JSON. |
| `redactions` | Present only when something was withheld: a list of `{path, reason}`. |
| `parseError` | Present only when `frame` is `null`: the parser's position and expectation. The text itself is withheld. |
| `count` | On `dropped` records only: how many frames were not written because the recorder's queue was full. |

The recorder never blocks the session. Frames wait in a bounded queue (4,096 by default; the queue
counts frames, not bytes); when it is full a frame is dropped and counted, and a `dropped` record is
written before the next frame that is written, or when recording ends. A recording states what it
lost.

**Paths.** A redaction `path` is a dotted path from the frame root: object keys joined by `.`,
array indices as plain numbers (`params.attachments.0.path`). A `.` inside a key is written `\.` and
a `\` inside a key is written `\\`, so the key `a.b` under `params` is `params.a\.b`.

**The marker.** A value `"[redacted]"` is not data. Never treat it as real text, and never assume a
frame with `redactions` is complete. Inside a URL the marker is percent-encoded, `%5Bredacted%5D`.

### Reasons

| Reason | Meaning |
|---|---|
| `deny-set:<method>` | A method rule withheld this key (below). Inbound `answers` of a `clarify` request use `deny-set:clarify`. |
| `suspicious-key` | The key name is credential shaped. |
| `free-text` | `system_prompt`, `credential_hint` or `launch_request`: free text a profile's author wrote. |
| `url-credential` | A URL carried a credential in its userinfo, a credential-named query value or its fragment. |
| `ticket` | The string contained `hermes-gateway-ticket.`. |
| `server-response:<method>` | The answer to a server request of that method. |
| `server-response:unknown` | An answer whose request this recorder never saw. |

## Redaction

Redaction happens before anything is written; nothing is redacted at read time, because by then
the secret would already be on disk. The rules, in the order they apply to an object key:

1. **Method deny-set.** An object whose `method` is listed withholds the listed keys at any depth
   below its `params`, wherever the object sits (a frame, a batch element, a server request, an
   `open_requests` entry). A listed method inside another listed method adds its rule; the outer
   rule still applies. An inner `method` that is not listed clears nothing. The list:
   `clarify.respond` and `clarify.lock` (`answer`); `clarify` (`answers`); `sudo.respond`
   (`password`); `secret.respond` (`value`); `terminal.read.respond` (`text`); `model.save_key`
   (`api_key`); `mcp.servers.set_api_key` (`value`); `mcp.servers.oauth.callback` (`code`,
   `state`); `connection.respond` (`env`); `config.set` (`value`); `vault.add` (`secret`);
   `vault.unlock` (`password`); `mcp.servers.add` (`config`); `plugins.manage` (`values`);
   `diagnostics.share_nous` (`extra_files`); `session.create` (`api_key`, `client_env`);
   `session.resume` (`api_key`); `file.attach` (`path`, `data_url`, `name`); `image.attach`
   (`path`); `image.attach_bytes` (`content_base64`, `data`, `filename`, `ext`); `image.detach`
   (`path`); `pdf.attach` (`content_base64`, `data`, `filename`, `path`); `prompt.submit` and
   `prompt.replace_queued` (`path`); `request.answer` (`result`, kept as an `approval` answer only
   when the request it answers was an approval). The prompt text is kept: a user who records a
   real session records what they typed.
2. **Free text.** `system_prompt`, `credential_hint` and `launch_request` are withheld by name at
   any depth.
3. **Key-name net.** A key is credential shaped when one of these case-insensitive patterns matches
   its camel-normalized form (an `_` inserted at each lower-case-or-digit to upper-case boundary,
   then lower-cased) or its squashed form (lower-cased, non-alphanumerics removed; there the
   optional prefix group is dropped, so the patterns match as suffixes):
   `^(.*[-_])?pass(word|phrase)$`, `secret` anywhere, `^(.*[-_])?credentials?$`,
   `^(.*[-_])?authorization$`, `^(.*[-_])?(api|private|public|access|secret|signing)[-_]?keys?$`,
   `^(.*[-_])?token$` (singular only, so `max_tokens` is kept) and `^(.*[-_])?ticket$`. The value
   becomes the marker whatever its type.

Every string value then passes two more rules:

4. **Ticket.** A string that contains `hermes-gateway-ticket.` is withheld whole.
5. **URLs, by position.** A string that contains `://` and parses as an absolute URL with a host is
   cleaned by editing the original text, never by re-serializing it: any non-empty userinfo
   becomes `%5Bredacted%5D`; a query value whose percent-decoded, lower-cased key is `token`,
   `ticket`, `internal` or credential shaped becomes `%5Bredacted%5D`; a non-empty fragment becomes
   `#%5Bredacted%5D`. A URL with none of these is left byte for byte.

**Answers to server requests.** Hermes asks the client questions (`sudo`, `secret`, `clarify`,
`approval` and others) as JSON-RPC requests. The recorder remembers the method of each request it
sees, from inbound request frames and from `open_requests` entries of an inbound result, and
forgets it when the answer is written or a `request.cancel` event names it (at most 1,024 are
held; the oldest is forgotten first). An outbound answer keeps only `choice` and `all` of an
`approval` result; every other answer's `result` is withheld whole, so a server request added by a
later Hermes cannot leak an answer. An `error` answer is kept.

**Substitutions.** After redaction the recorder replaces caller-supplied needles (longest first) in
every string value, every object key and the header endpoint. The fixture script uses it to write
`<profile>`, `<home>`, `<tmp>`, `<gateway>` and `<user>` in place of machine-specific text. A
needle under four characters is refused.

**Limits.** The claim "no secret reaches the file" is bounded by these rules:

- The path component of a URL is not covered: a bearer capability carried in a path (a browser
  debugging socket URL is the known example) is recorded as is.
- Free-form object parameters that no rule names are open positions: a secret written there under
  an innocent key is recorded. The unit specification lists them, and a test fails when Hermes
  adds a new one.
- Free text a user or model writes is not redacted; the secret scan is the backstop for the
  committed fixtures.
- Dashboard HTTP exchanges are not recorded. The first unit that adds an HTTP client adds
  route-aware rules here.

## Unparseable payloads

Text that is not valid JSON is recorded with `frame: null` and a `parseError` holding the parser's
position and expectation. The text itself is withheld: it could not be walked by the rules, so it
cannot be shown to be safe. These records are worth reading: an unparseable frame is protocol
drift. Replay skips them.

## Guarantees and non-guarantees

**Guaranteed.** Append-only. `seq` gapless. `tMs` never decreases. A frame with no secret position
round-trips to an equal JSON value (not equal bytes: key order and whitespace are not kept). Every
withheld value is marked and listed.

**Not guaranteed.** No hash chain: records are not tamper-evident. A chain cannot be redacted after
the fact without destroying the tamper-evidence that was its point, which is why redaction must be
right at the first write; adding one is a later decision. A recording cut short leaves a last line
with no newline; a reader ignores it and reports the file as truncated.

## Earlier versions

Version 1 (single connection) and version 2 (several profiles in one file) were written by the
Python and TypeScript clients. They used `at` (a wall-clock time per frame) where version 3 uses
`tMs`, and version 2 tagged each frame with a `profile` naming the observing connection where
version 3 uses `generation`; version 2's header carried a `connections` list where version 3 has
`endpoint` and `subprotocols`. Those corpora were recorded against an older protocol, and the
version 3 reader does not read them. Recording several gateways into one file is left to a later
phase.

## Reading a recording

```bash
# event types by frequency
jq -r 'select(.kind=="frame" and .dir=="in") | .frame.params.type // empty' fixtures/frames/approval.jsonl | sort | uniq -c | sort -rn
# everything that was withheld
jq -c 'select(.redactions) | {seq, redactions}' fixtures/frames/clarification.jsonl
# turn outcomes
jq -r 'select(.frame.params.type=="message.complete") | .frame.params.payload.status' fixtures/frames/*.jsonl
```
