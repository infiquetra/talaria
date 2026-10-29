# Print result, version 1

`talaria -p "prompt" --output-format json` writes one JSON object followed by a
newline. Every exit except a usage error (exit 2) produces this record. Standard
error carries failure lines and warnings separately.

All keys are present in the order below. Unknown optional values are `null`.
The answer is `""` when empty; warnings are `[]` when absent.

| Key | Value |
| --- | --- |
| `type` | Always `"result"`. |
| `version` | Always `1`. |
| `status` | `completed`, `failed`, `unknown`, `input_required`, `not_started`, `timed_out`, or `interrupted`. |
| `exit_code` | The process exit code. |
| `text` | The live answer, unchanged. After lost events, this may be the redacted stored answer, accompanied by a warning. |
| `session_id` | The opened session's stored identifier, or `null`. |
| `profile` | The session profile, or `null`. |
| `model` | The result's model, falling back to session information, or `null`. |
| `provider` | The result's provider, falling back to session information, or `null`. |
| `admission_id` | The prompt's admission identifier, or `null`. |
| `duration_ms` | Elapsed time in whole milliseconds. |
| `outcome` | Hermes's effective turn outcome, or `null`. |
| `turn_exit_reason` | The stored result's exit reason, or `null`. |
| `usage` | The nine fields described below. |
| `warnings` | An array of warning strings. |
| `error` | `null` on success; otherwise an object with `what` and `next_step` strings. |

`usage` contains these keys in this order: `input_tokens`, `output_tokens`,
`cache_read_tokens`, `cache_write_tokens`, `reasoning_tokens`, `total_tokens`,
`api_calls`, `estimated_cost_usd`, `cost_status`. The first seven are nonnegative
integers; cost is a nonnegative number and cost status is a string. Each value
comes only from the receipt's stored `result`. Missing or mistyped values become
`null`, never a substituted zero. Numeric strings are not converted.

Exit 0 means completed; 1 means failed or unknown; 3 means input required; 4 means
nothing started and retry is safe; 124 means timed out. Signal exits are 129 for
hangup, 130 for interrupt and 143 for termination. Another viewer interrupting
the turn or withdrawing a queued prompt also produces exit 130. An unknown
outcome must be checked before resubmitting: Talaria never automatically resends.

Without `--timeout`, `talaria -p` waits as long as the turn runs; set `--timeout`
in automated jobs. Pass `< /dev/null` when standard input is an open pipe you
do not mean to send. A deadline includes reading standard input, then allows at
most 25 seconds to stop the owned turn and detach. A second signal exits at once.

JSON escapes string content; it preserves the answer without terminal filtering.
Hermes-supplied pieces of errors and warnings have control and bidirectional
control characters replaced by `?`, including newlines and tabs, and are limited
to 200 Unicode characters when the message is built. The same built strings
appear on standard error and in this record.

Version changes when a key is removed or its meaning changes. Adding a key keeps
version 1; readers should tolerate additional keys. The [streaming JSON format](stream-v1.md) uses this object as its `result` record for each turn.

```json
{"type":"result","version":1,"status":"completed","exit_code":0,"text":"5555","session_id":"local-session-1","profile":"fixture","model":null,"provider":null,"admission_id":"adm-1","duration_ms":4210,"outcome":"completed","turn_exit_reason":null,"usage":{"input_tokens":null,"output_tokens":null,"cache_read_tokens":null,"cache_write_tokens":null,"reasoning_tokens":null,"total_tokens":null,"api_calls":null,"estimated_cost_usd":null,"cost_status":null},"warnings":[],"error":null}
```
