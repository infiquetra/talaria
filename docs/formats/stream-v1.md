# Streaming JSON, version 1

`talaria -p --output-format stream-json "prompt"` writes UTF-8 JSON objects,
one per line. Each line ends in a newline and is flushed as soon as it is
available. Standard output contains only these records; diagnostics and
warnings also go to standard error. The first record is `system` after a
session opens. If the session cannot open, the only record is a `result`.

For multiple turns, use `--input-format stream-json --output-format stream-json`
and pipe JSON lines to standard input. Streaming input cannot be a terminal and
cannot be combined with a prompt argument. Each nonblank line must be one of:

```json
{"type":"user","text":"First prompt"}
{"type":"interrupt"}
{"type":"user","text":"Next prompt"}
```

The `user` text must be a nonblank string. No other keys are allowed. A line
may contain at most 1,048,576 bytes; CRLF and a final line without a newline
are accepted. Input is read in line order through a bounded queue. Turns run
one at a time in the same session; a `user` line waits for the previous result,
and an `interrupt` stops the most recent turn before it in the input order.
An idle interrupt produces a warning. An invalid line stops an active turn,
then produces an `input_error`; no later input is processed.

## Records

Keys appear in the order shown. Every record other than `result` has an
`elapsed_ms` measured from process start. Its `turn` is the 1-based turn number,
or `null` when no turn applies. The session identifier in `system` is the stored
session identifier. `model` and `provider` can be `null`. These examples
illustrate individual record shapes; they do not form a single run.

```json
{"type":"system","subtype":"init","stream_version":1,"turn":null,"session_id":"session-1","profile":"fixture","model":"model-name","provider":"provider-name","input_format":"text","elapsed_ms":180}
{"type":"submitted","turn":1,"admission_id":"adm-1","status":"started","elapsed_ms":210}
{"type":"text","turn":1,"text":"Listing","elapsed_ms":900}
{"type":"tool_use","turn":1,"tool_id":"call_1","name":"terminal","context":"ls","input":{"command":"ls"},"elapsed_ms":1400}
{"type":"tool_result","turn":1,"tool_id":"call_1","name":"terminal","is_error":false,"output":"a.txt\n","duration_ms":350,"elapsed_ms":1750}
{"type":"warning","turn":1,"text":"Hermes did not return the turn's usage figures.","elapsed_ms":2600}
{"type":"gap","turn":1,"elapsed_ms":2000}
{"type":"input_error","turn":null,"line":1,"what":"line 1 is not valid JSON.","elapsed_ms":200}
{"type":"result","version":1,"status":"completed","exit_code":0,"text":"a.txt","session_id":"session-1","profile":"fixture","model":null,"provider":null,"admission_id":"adm-1","duration_ms":2800,"outcome":"completed","turn_exit_reason":null,"usage":{"input_tokens":null,"output_tokens":null,"cache_read_tokens":null,"cache_write_tokens":null,"reasoning_tokens":null,"total_tokens":null,"api_calls":null,"estimated_cost_usd":null,"cost_status":null},"warnings":[],"error":null}
```

`submitted.status` is `queued`, `started`, or `terminal`. Records for events
that arrived before the submission reply follow `submitted`. Only events for
the submitted admission produce `text`, `tool_use`, and `tool_result`. Text
records are live deltas and may contain commentary between tool calls; the
answer is `result.text`. After a replay gap, one `gap` record is written and
no more text deltas are emitted for that turn. Tool records continue; a
recovered answer may be a redacted stored copy, with a warning.

Tool identifiers use the string `tool_id`, falling back to `tool_call_id`, then
`""`; names use `name`, then `tool_name`, then `"tool"`. A missing or mistyped
context becomes `""`; a non-object input becomes `{}`. `is_error` is a boolean
or `null`. A non-string result is compact JSON and a missing result is `""`.
Tool output longer than 65,536 UTF-8 bytes is cut at a character boundary and
followed by `...`. `duration_ms` is measured from the corresponding tool start,
or `null` when that start was not observed. At most 4,096 open starts are
tracked; later calls still produce records and a warning.

Each `result` has exactly the [print result version 1](print-result-v1.md)
shape. Its `duration_ms` is the time from process start to that turn's end,
not the duration of that turn alone. Turn warnings appear as `warning` records
just before the result, remain in `result.warnings`, and appear on standard
error. Stream-specific warnings appear immediately and on standard error, but
are not included in `result.warnings`. An `input_error` includes the line number,
or `null` when no user record arrived before end of input.

## Exit and compatibility

Usage errors exit 2 without writing standard output. A failure to start exits 4.
A signal or timeout ends the run with 129, 130, 143, or 124; a closed output
stops the owned turn and exits 141. An input error exits 2. A failed turn ends
the run with its exit code; an interrupt record can stop a turn with 130 while
allowing the next input turn to run. At end of input, the last turn's exit code
is returned. These cases take precedence in the order listed. The last output
line is normally a `result`, `input_error`, or idle-interrupt `warning`; usage
errors, closed output, and a signal or timeout between turns are exceptions.

The record names `system`/`init`, `text`, `tool_use`, `tool_result`, and
`result` are shared with Hermes's stream, but their fields differ. Talaria uses
`tool_id` where Hermes uses `tool_call_id`, and elapsed process time instead of
Hermes's wall-clock `timestamp`. Talaria may use `null` for `is_error` and
`duration_ms`, adds `submitted`, `warning`, `gap`, and `input_error`, and uses
its print-result object for `result` instead of Hermes's result fields such as
`tokens` and `replay_gap`. Consumers of Hermes's format need to adapt.

Removing a key or changing its meaning requires a new stream version; adding a
key keeps version 1. Readers should tolerate additional keys.
