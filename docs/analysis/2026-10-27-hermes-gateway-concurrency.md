# Gateway concurrency measurement — implementation handoff

The probe implementation is ready for offline review and fixture-gateway execution.
No live gateway measurements were run by the implementation seat.
The results file contains the complete 17-record matrix, with every record explicitly unmeasured.
There is **no healthy width**, so the default concurrency, topology, memory estimate and cited runs are all null.
The final evidence gate remains unsatisfied until the measurement owner supplies live results, including the shared width-one control and both recovery series.

## Scope and provenance

Hermes release: **v0.22.4**, commit **4d04303818b61bf3aa09278af48356315515965b**.
Machine class: not measured; the measurement owner must record it without a host name.
Execution mode: `unknown`; the operator must report `in-process` or `managed-worker` before interpreting results.
The draft record identity `00000000`, dated 2026-10-27 at Talaria `a498e28dc052`, denotes unmeasured placeholders only.
Each live invocation replaces those identities with its own random run id, date and Talaria commit.
No width cap or configuration change was made by the implementation seat.
No sessions were created by this seat; QA must report the sessions remaining in `fixture` after execution.

## Static readings

The table distinguishes a source reading from a measured outcome. The source revision above was inspected for titled creates, hidden-session acceptance, subscription authorization, receipts, event replay, settlement status, and the loopback streaming shape. None of the following is claimed as a live finding yet.

| Reading | Live verdict | Evidence still needed |
| --- | --- | --- |
| S1 — turn executor | Not tested | Canonical-turn overlap and provider peaks in A |
| S2 — admission cap | Not tested | Distinct sessions and refusal counts across A widths |
| S3 — canonical retention | Not tested | E receipts and resume; positive eviction evidence if any |
| S4 — cache pressure | Not tested | Process-tree samples before, during and after cleanup |
| S5 — execution policy | Not tested | Operator execution-mode answer and B support |
| S6 — create launch fields | Not tested | First A create and its snapshot |
| S6a — descriptor omits hidden | Not tested | Runtime descriptor and successful hidden create |
| S6b — titled creates reuse sessions | Not tested | Static source confirms title lookup; probes deliberately omit title |
| S7 — durable admission reply | Not tested | Submit statuses and admission identifiers |
| S8 — receipts without attachment | Not tested | D receipts on a fresh connection |
| S8a — detach authorization | Not tested | E refusal, resume and extra turn |
| S9 — event ordering | Not tested | Per-session epoch, sequence and completion counts |
| S9a — replay ranges | Not tested | Gap repair and D replay truncation |
| S10 — completion and tool events | Not tested | A completions, C tool counts and B rate-limit key names |
| S11 — gateway process identity | Not tested | Identify pid and sampler availability |
| S12 — custom loopback provider | Not tested | Width-one classification check and live completion |
| S13 — todo tool arguments | Not tested | Exactly one todo_list execution per C session |

## Series and current results

| Series | Topology | Widths | Repetitions | Current evidence |
| --- | --- | --- | --- | --- |
| A | shared | 1, 5, 10, 16, 20 | 3 | Not run: live evidence pending |
| A | per-session | 1, 5, 10, 16, 20 | 3 | Not run: live evidence pending |
| B | shared | 1, 5, 10, 16 | 1 | Not run: operator execution pending |
| C | shared | 10 | 1 | Not run: live evidence pending |
| D | shared | 10 | 1 | Not run: live recovery evidence pending |
| E | shared | 20 | 1 plus retention check | Not run: live recovery evidence pending |

Highest healthy shared width: none. Highest healthy per-session width: none.
Recommendations: `default_max_in_flight = null`, `topology = null`, `memory_mb_per_session = null`, `cited_runs = null`.
This is an absence of measurements, not a refutation of the assumed width or durable recovery.
The draft is not a completed phase gate and must not be merged as a finished measurement finding.

## Child issue edits after measurement

These are the concrete edits presently needed; measured numbers must replace the pending statements before dependent planning begins.

### #264 — Script engine

No change to the engine interface or its absence of a concurrency bound. Keep the start gate closed until the finding is complete.

### #265 — Journal and resume

State that prompt.receipt retrieves saved results without attaching, and session.resume is required before submitting another prompt. Preserve unknown outcomes without resubmission. Confirm these rules from D and E before finalizing the issue.

### #266 — Hermes binding

Require a unique request_id, source tool, hidden true and an explicit model/provider; omit title and all automatic-approval fields. Gate on the descriptor's launch fields, while checking hidden against the contract. Leave connection topology pending the comparison of A shared and per-session results.

### #267 — Staffing and scheduling

Replace the assumed default with the measured recommendation and its cited records. Until measurements exist, record no healthy width and leave the default and memory estimate undecided; do not invent a fallback of one.

### #268 — Progress view

Leave the coalescing interval pending measured peak_events_per_s. Require replay repair for gaps and treat an epoch change alone as normal.

### #269 — Runner lifecycle

Require receipt collection after reconnect, followed by session.resume before further submission. Leave any extra recovery steps pending D and E.

### #271 — Built-in workflows

Leave fan-out caps pending the scheduler's measured default. Do not raise them from the draft fixture.

### #273 — Phase acceptance

Keep the concurrency acceptance width pending the recommendation. No healthy width keeps this gate closed; do not substitute a successful compile for live evidence.

## Re-running

Run from the repository root using the pinned Rust toolchain. Use the operator-owned gateway serving `fixture`; never start, restart or reconfigure it from the probe. The fixture gateway port is owned by QA. Set `TALARIA_PROBE_EXECUTION_MODE` to the operator-reported mode. Only the operator may lower `TALARIA_PROBE_MAX_WIDTH` from 20.

```bash
cargo test -p hermes-client --test gateway_concurrency_results
TALARIA_LIVE_REQUIRE=1 cargo test -p hermes-client --test live_gateway_concurrency -- --ignored --nocapture probe_series_a_stub_provider
TALARIA_LIVE_REQUIRE=1 cargo test -p hermes-client --test live_gateway_concurrency -- --ignored --nocapture probe_series_c_tool_turns
TALARIA_LIVE_REQUIRE=1 cargo test -p hermes-client --test live_gateway_concurrency -- --ignored --nocapture probe_series_d_reattach_after_drop
TALARIA_LIVE_REQUIRE=1 cargo test -p hermes-client --test live_gateway_concurrency -- --ignored --nocapture probe_series_e_detached_retention
```

The operator runs `probe_series_b_free_model` separately or supplies its RECORD lines through `TALARIA_PROBE_IMPORT` and the ignored `import_operator_records` test. Results merge by id under an exclusive lock in the Cargo test temporary directory's gateway-concurrency/results.json. Copy the completed result file to fixtures/gateway-concurrency/results.json and rerun the offline evidence gate. Keep the run identities and dates in this finding, with each measured table, resource estimates and recovery totals. CPU time is an estimate over processes present in both endpoint samples and misses short-lived children.

## Operator questions and completion work

Record the machine class and execution mode, whether B is supported or waived, and any operator width cap. QA must supply A, C, D and E measurements, record cleanup, and replace the pending table and verdicts with supported findings. If recovery fails without a restart or no topology is healthy at ten, record a proposed decision awaiting the operator. Post the child issue finding links only after the completed pull request merges.

## Offline implementation verification

The independent suite passes 14 of its 15 required checks. The sole failure is `results_meet_the_evidence_gate`, because the committed records remain unmeasured; this check was preserved unchanged. All 21 supplemental engine checks pass. The copied live test target passes its offline checks with all six gateway/import measurements ignored. These outcomes verify the helper API and record structure without claiming a live concurrency or recovery result.
