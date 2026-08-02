# Frame log format

Status: `active`
Authority: `contract`
Version: 1

A frame log is the recording Talaria makes of its conversation with a Hermes gateway. It is written
by `talaria record` and is the corpus that replay, renderer comparison, and protocol-drift detection
all read.

This file is the contract. The format is specified here rather than being implied by whatever a
serializer emits, so a corpus recorded by today's TypeScript Talaria stays readable by a Talaria
written in another language later. That property is the reason the recorder was built first.

## Shape

One JSON object per line, UTF-8, newline-delimited (JSON Lines). Appended, never rewritten.

The first line is a header. Every subsequent line is a frame.

### Header

```json
{
  "kind": "header",
  "version": 1,
  "startedAt": "2026-08-02T12:21:35.016Z",
  "endpoint": "ws://127.0.0.1:8799/api/ws?token=%5Bredacted%5D"
}
```

| field       | meaning                                                                |
| ----------- | ---------------------------------------------------------------------- |
| `version`   | Format version. Bumped when a reader must notice a change.             |
| `startedAt` | When recording began, ISO-8601.                                        |
| `endpoint`  | The gateway dialled, with credential-shaped query parameters withheld. |

A log with only a header is a valid, complete recording of a session in which nothing arrived. That
is deliberately distinguishable from a missing file.

### Frame

```json
{
  "kind": "frame",
  "seq": 3,
  "at": "2026-08-02T12:21:35.066Z",
  "dir": "in",
  "frame": { "jsonrpc": "2.0", "method": "event", "params": {} }
}
```

| field        | meaning                                                      |
| ------------ | ------------------------------------------------------------ |
| `seq`        | Position in this file, starting at 1, monotonic and gapless. |
| `at`         | When the frame was _observed_, not when it was sent.         |
| `dir`        | `in` from the gateway, `out` from Talaria.                   |
| `frame`      | The decoded JSON-RPC frame, after redaction.                 |
| `redactions` | Present only when something was withheld.                    |
| `parseError` | Present only when the payload was not valid JSON.            |

## Redaction

**Credentials never reach the file.** The Hermes gateway carries them in plaintext on ordinary
client-to-server frames. Hermes calls these its four "blocking bridges", verified against
`tui_gateway/methods_prompt.py` at Hermes `7f4d15515`:

| method                  | plaintext field   |
| ----------------------- | ----------------- |
| `sudo.respond`          | `params.password` |
| `secret.respond`        | `params.value`    |
| `terminal.read.respond` | `params.text`     |
| `clarify.respond`       | `params.answer`   |

Every frame passes a redaction boundary before it is written. See `src/record/redact.ts`.

**Pin the Hermes revision whenever you re-derive this list.** It was first written against a
checkout six weeks behind the running Hermes, in which these methods lived in a different file, and
that reading missed `clarify.respond` entirely.

**The recorder is currently receive-only**, so it writes `dir: "in"` frames and nothing else. All
four fields above travel in the _other_ direction. Redaction is proven by unit tests, not by
observed traffic, and the direction that actually carries credentials is not yet recorded at all.
When Talaria gains a send path it must route outbound frames through the same boundary; that is the
moment this format starts holding secrets.

Withholding is recorded rather than silent, so a reader sees a marked hole instead of clean-looking
data:

```json
{
  "kind": "frame",
  "seq": 3,
  "dir": "in",
  "frame": {
    "method": "sudo.respond",
    "params": { "request_id": "r-1", "password": "[redacted]" }
  },
  "redactions": [
    { "path": "params.password", "reason": "deny-set:sudo.respond" }
  ]
}
```

`reason` is `deny-set:<method>` for an explicit rule, or `suspicious-key` for the key-name net that
catches credentials on methods the deny-set has never heard of.

**Consequence for readers:** a value of `"[redacted]"` is not data. Never treat it as a real string,
and never assume a frame with a `redactions` array is complete.

## Unparseable payloads

A payload that is not valid JSON is recorded with `frame: null` and a `parseError`. The payload
itself is withheld — it could not be walked by the redaction boundary, so it cannot be shown to be
safe.

```json
{
  "kind": "frame",
  "seq": 7,
  "dir": "in",
  "frame": null,
  "parseError": "Expected property name or '}' in JSON at position 2"
}
```

These are worth reading: an unparseable frame is exactly the protocol drift the corpus exists to
catch.

## Guarantees and non-guarantees

**Guaranteed.** Append-only. `seq` gapless within a file. Ordinary traffic byte-identical to what
arrived. Every withheld value marked.

**Not guaranteed.** No hash chain yet — the records are not tamper-evident, and nothing detects an
edited or truncated file. Adding one is a later decision, and it is the reason redaction had to be
correct from the first write: a chain cannot be redacted after the fact without destroying the
tamper-evidence that was the point of chaining it.

Timestamps are observation times from the recording host's clock, so they are not a reliable
ordering across two logs recorded on different machines.

## Reading one

```bash
# every event type seen, by frequency
jq -r 'select(.kind=="frame") | .frame.params.type // empty' log.jsonl | sort | uniq -c | sort -rn

# everything that was withheld
jq -c 'select(.redactions) | {seq, redactions}' log.jsonl

# protocol drift
jq -c 'select(.parseError) | {seq, parseError}' log.jsonl
```
