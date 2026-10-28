# Workflow journal and resume

A run directory is private local data. It contains exact prompts, final text and script values,
including anything a script author puts in them. Never upload it as public evidence. Tickets and
provider credentials belong to the gateway connection; manifests refuse credential-shaped fields
and ticket strings. Directories use mode 0700 and files 0600 on Unix.

`RunDir` creates or opens the directory without taking a lock, so another process can answer a
parked call. `RunJournal::open` exclusively locks `run.lock` before reading. The operating system
releases the lock on process death; there is no stale-lock deletion. Scripts are kept by SHA-256,
including edited versions. The immutable manifest fixes run ID, start time, seed and arguments.

The directory contains `manifest.json`, `journal.jsonl`, `run.lock`, `scripts/<sha256>.js`,
`blobs/<sha256>` and `resolutions/`. Run IDs are 26-character time-ordered Crockford base32 IDs.
A writer assigns `v: 1`, contiguous `seq` starting at 1 and display-only `at_ms`. Every successful
append has passed `sync_data`. Concurrent requests share syncs. Any write failure aborts execution.

| Record | Fields after the envelope | Meaning |
| --- | --- | --- |
| `run_started` | run_id, script_sha256 | First line only. |
| `run_resumed` | script_sha256, resume_count | Start another execution. |
| `engine` | event or event_ref | The complete engine event. |
| `call_intent` | key, attempt, call_id, label, phase, workflow_path, launch, request_id | Synced before session creation. |
| `session_bound` | key, attempt, session_id, stored_session_id | The created session. |
| `turn_intent` | key, attempt, turn, submission_id, text or text_ref | Synced before submission. |
| `turn_admitted` | key, attempt, turn, admission_id, status, outcome, execution_generation, authority_epoch | Admission identity; no inference result. |
| `turn_settled` | key, attempt, turn, admission_id, outcome, final_text or final_text_ref, result_meta, usage | Saved receipt, with usage copied verbatim. |
| `call_attention` | key, attempt, episode, cause, turn, admission_id, execution_generation, detail | One unresolved operator question. |
| `call_resolved` | key, attempt, episode, resolution, source | Durable answer before any resolution RPC. |
| `call_finished` | key, attempt, outcome, turns | Durable result before returning to the engine. |
| `run_finished` | outcome, summary | Final counts for encountered keys; closes this execution. |

Call outcomes are tagged objects: `{"kind":"completed","value":...}` (or `value_ref`),
`{"kind":"failed","reason":...}` or `{"kind":"skipped"}`. Null completed values are preserved.
Absent final text means the terminal admission saved no response. Result metadata copies only
completed, failed, interrupted, turn_exit_reason, model and provider. Metadata strings are cut to
1,024 Unicode scalar values; reasons and attention details to 4,096, with no added marker.

Payloads whose JSON encoding exceeds 256 KiB move to immutable blobs. Text blobs contain exact
UTF-8; event and value blobs contain JSON. A reference is `{"sha256":"...","bytes":123}`. The
blob is synced and linked into place before its referencing record. Existing blobs are never
overwritten. Lines remaining at or above 1 MiB are refused. The backend fails such a call before
a guarded gateway operation. Missing blobs or size changes are corruption at open; loading a blob
also verifies its hash. `engine_config` propagates a corrupt replay payload before the engine starts.

The strict grammar for each key is:

| Last record | Permitted successor in the same attempt |
| --- | --- |
| none | intent, or short failed/skipped finish with zero turns |
| intent | bound, attention, failed finish |
| bound | turn intent 0, attention, failed finish |
| turn intent t | admission t, attention, failed finish |
| admission t | settlement t with the same admission, attention, failed finish |
| settlement 0 | turn intent 1, any finish, attention |
| settlement 1 | any finish, attention |
| attention e | resolution e |
| resolution e | recovered settlement, attention e+1, failed finish |
| failed finish | next attempt only |
| completed/skipped finish | no successor |

A rerun or abort resolution also permits the next attempt. Attempts start at 0 and advance by one;
turns start at 0 and stop at 1. New attempts have new sessions. Unknown admissions never start a
new attempt automatically. A failed attempt runs again on the next execution. Finished completed
or skipped attempts are cached and do not acquire a gate permit or contact the gateway.

All newline-terminated lines, including the last, must parse and obey this grammar, the version,
sequence and delivery numbering. An incomplete tail is warned about and truncated only after the
complete prefix validates. Corruption refuses resume and leaves the file unchanged. Unlocked
`JournalReader::read_from` returns only complete lines and their next byte offset.

Replay re-executes the script with its manifest time, seed and args. The journal reconstructs the
latest consistent sequence of engine deliveries, retaining completed/skipped results whose launch
still resolves to the stored key. A live backend call waits for its own current execution's
`AgentStarted` to be synced before doing anything. Forward events in order with `forward_events`,
await that task after `engine::run`, then call `finish` once. Missing forwarding aborts after 30 s.
This barrier makes all deliveries on which a new call depends durable first.

The key hashes canonical JSON containing the engine CallId and resolved profile, model, provider,
effort, toolsets, skills, max turns and isolation. Labels, phases and cwd do not affect the key.
The launch stored in an intent retains cwd. Exact retries use that stored launch and text.
`request_id` is `wf:<run>:<key>` and `submission_id` adds `:<turn>`. Attempts after 0 add `:a<n>`
before the turn. Their maximum lengths are 74 and 76 characters. No submit request includes the
unattended flag; Hermes would treat it as automatic approval of dangerous commands.

| Crash | Resume action |
| --- | --- |
| C1: after gate, before intent | Gate Fresh, write intent. |
| C2: intent, before create | Retry stored create. |
| C3: create answered, before binding | Retry create; deduplication returns the same session. |
| C4: binding, before first turn | Compute and journal the first text. |
| C5: turn intent, before submit | Attach, retry stored submit. |
| C6: submit answered, before admission record | Retry submit; always read a receipt for a terminal result. |
| C7: admission, still running | Attach, receipt, wait. |
| C8: turn ended, before settlement | Read saved receipt and settle. |
| C9: settlement 0, before policy | Reapply the pure turn policy. |
| C10: correction intent | Retry the stored correction. |
| C11: settlement 1 | Reapply policy, enforcing the two-turn cap. |
| C12: finish, before engine delivery | Return stored outcome without gateway calls. |
| C13: gateway restarted mid-turn | Park unknown work. |
| C14: answer file, before resolution record | Consume the answer. |
| C15: resolution synced, file remains | Remove consumed file; repeat resolution procedure safely. |
| C16: resolve committed, reply lost | Exact resolve retry, then receipt. |
| C17: receipt answered, next record absent | Repeat resolution and receipt; keep any saved result. |
| C18: rerun decided, next intent absent | Start exactly attempt n+1. |
| C19: durable skipped finish | Return skipped without a gate or gateway call. |

Unknown, lost and conflicting identities park with a new episode and drop their gate permit;
other branches continue. An operator writes an answer with unlocked `RunDir::request_resolution`:

```json
{"v":1,"key":"0123456789abcdef0123456789abcdef","attempt":0,"episode":0,"resolution":"rerun"}
```

The filename is `resolutions/<key>.<attempt>.<episode>.json`. Allowed answers are `fail`, `rerun`
and `abort`. A create-new link makes the first writer win; another gets `ResolutionPending`.
The journal polls at open and every second while parked. Invalid content, extra fields or a name
mismatch become `.rejected`; stale answers are deleted with a warning. Temporary and rejected files
are left alone. A consumed answer is journaled before the file is removed. Each answer applies to
one episode. The default policy waits; `on_unknown: fail` supplies a policy answer immediately.

For unknown work, every answer first resolves the exact generation, then reads a receipt with
results. A saved result is settled for every answer, including abort. Abort ends this execution;
a later resume continues the saved settlement. A stale generation with a still-unknown receipt
parks a new episode. Without a saved result, fail returns a failed outcome (null to JavaScript),
rerun starts one fresh attempt, and abort ends execution; a later resume retries the aborted call.

Transport errors, timeouts and runtime draining retry after 0.5, 1, 2, 4 and 8 seconds, then every
15 seconds, reattaching first. The default 120-second continuous outage budget belongs to each
call; only its own successful port operations reset it. Healthy inference can take arbitrarily
long. Queue full and other refusals fail the call; an exact-identity conflict parks it. Permission
denial gets one reattach and retry.

Summaries include every settled turn's usage once, across all attempts, and all admitted unsettled
turns, including parked ones. Usage is opaque and contains no inferred price. `TurnSettled` notices
identify admission, attempt and turn for incremental accounting. Orphans are provisional until
finish; running orphans reflect the last journaled queued/started status. The journal never
interrupts sessions.

The `journal_resume` example supports `run`, `resume`, `stats`, `restart-gateway` and `resolve`.
It uses only a persistent fake gateway. Build it with `--features mock,fake-gateway`. Give `run`
`--runs DIR --gateway-state FILE --script PATH`; optionally `--hold LABEL --on-unknown wait|fail`.
`resume` adds `--run ID`. `resolve` takes `--runs DIR --run ID --key KEY --attempt N`, optional
`--episode E`, and `--as fail|rerun|abort`. JSON events go to stdout and errors to stderr. Exit
codes are 0 completed, 1 failed/corrupt, 3 aborted, 4 interrupted while parked, 5 locked, 64 usage.

A runner that has verified interrupted or ended work can explicitly queue restarts before starting
the resumed engine with
`pub async fn request_restart_keys(&self, keys: &[String]) -> Result<(), JournalError>`.
The method is async: await it before deleting the corresponding runner control files. It syncs a
failed marker for an unfinished attempt; that key's next call starts exactly attempt n+1, including
when the old last record was an admission or a settlement. Repeated requests in one execution are
idempotent. Completed/skipped keys and keys absent from the journal are untouched. A parked key instead receives a durable
operator rerun resolution, which still recovers any saved result before considering another session.
