# Protocol fixtures

`live_two_turns.frames.jsonl` was copied from the handoff recording
`reference-slices/rust/tests/fixtures/live_two_turns.jsonl`. The recording
came from Hermes revision `22063fc09`, which is older than the pinned
release. It was scrubbed before it was copied: profile names are `default`,
the working directory is the placeholder `/work/project`, and identifiers
look like `id-0004`.

Each line is one JSON-RPC message. It is the exact text of the recording's
`frame` string. The `t_ms` key is dropped. The line is not re-serialised.

`gateway_ready_completed.json` is not a recording. It is a whole `params`
object completed from the pinned contract: the recorded `gateway.ready`
payload plus `"skin": {}` and `"replay_epoch": "replay-epoch-placeholder"`.
The recorded frame in the JSON Lines file is unchanged and still lacks those
two fields. The completed file does not repeat the recording's envelope
fields; those fields are optional on an event.

The six responses are mapped to methods by their order, because the recording
stores no outbound frames:

| id | method |
| --- | --- |
| 101 | `runtime.describe` |
| 102 | `session.create` |
| 103 | `prompt.submit` |
| 104 | `prompt.submit` |
| 105 | `approval.respond` |
| 106 | `session.detach` |

Issue 201 (P1-04) will replace these fixtures with a fresh recording.
