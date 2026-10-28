# Hermes workflow agents

An `agent()` call uses one ordinary Hermes session with `source: "tool"`. Hermes leaves these sessions out of human session pickers. The binding does not send a title or the separate bot-chat hidden flag.

Every launch names a model, provider, tool list and existing working directory. The request can override the run's profile and tools. A different profile uses its own connection; profile names never go in RPC params. Worktree isolation requires a `WorktreeProvider` supplied by the caller. The journal owns request identifiers, replay, retry and unknown-outcome decisions.

Prompts are finite admissions. They do not carry the automatic-approval flag. Dangerous commands follow the operator's finite-turn approval policy; with its default, they are denied immediately. An unattended approval queue is not part of this binding. Clarification tools receive Hermes's headless answer.

Results come from terminal receipts. Streaming events wake pollers and feed in-process observers, but their text is never substituted for the stored result. Unknown outcomes remain unknown until the journal's operator path resolves them. A dropped future sends no stop command. Explicit stop cancels queued work or interrupts the exact recorded execution generation, and never chases a newer generation.

A schema appends the pinned Hermes output contract to the task. The stored answer is extracted and validated using the pinned `jsonschema` crate, with draft 2020-12 set explicitly when the schema declares no draft. Local schema references are supported; the validator is built without filesystem or network reference retrieval. Invalid output gets one correction turn. A second failure produces a failed agent outcome, which the script engine returns as `null`.

Hermes redacts stored responses. Avoid credential-shaped property names such as `token`, `password`, `secret` and `api_key` in result schemas: their values may be masked even when they are ordinary business data. Validate the receipt exactly as received.

Turn input, output and total token counts are summed independently when present. Session cost is cumulative; the last reported session cost is the call's cost. Missing usage remains absent.

## Example

```bash
cargo run -p talaria-workflow --example hermes_agent -- \
  --profile fixture --model minimax-m3.1-flash --provider ollama-cloud \
  --toolsets file,web --cwd "$PWD" --prompt 'Report one finding.'
```

Use `--schema` for a schema file, `--script` for a whole workflow, `--runs` to select a journal parent and `--resume` to reopen a run folder. `--record` installs the redacting recorder on each new link. Ctrl+C detaches and leaves the admission with Hermes. Records and outcomes are emitted as JSON lines. The mock feature adds `--scripted` for offline wire scripts; without that feature the flag exits with status 64.

A port's event feed is unbounded so a slow observer cannot delay receipts. Drop the receiver when no observer needs it. After a replay gap, the port resumes and publishes a fresh snapshot before the observer rebuilds its view.


## Offline verification and fixture provenance

The wire fixtures are synthetic examples decoded with the pinned contract types;
they are not live captures. The recording owner supplies redacted live captures
separately. `scripts/stop.json` includes thirteen executable stop scenarios, and
`ten_parallel.json` groups concurrent calls with barriers. Script replies may use
`$param:<name>` to copy an outbound parameter, so parallel receipts retain their
session identity. `close_before_reply` simulates a lost reply after idempotency
state was stored; `drop_after` advances the connection on the next call.

The offline tests use the production journal. Direct backend drivers first record
`AgentStarted`; engine drivers forward the complete event channel into the
journal. The resume check detaches after durable admission, releases the previous
journal, and verifies the new transport sends no create or submit. The example
also restores saved profile, working directory and tool defaults on resume and
does not reprint admission lines from the earlier execution.
