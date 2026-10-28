# Hermes gateway concurrency measurement

On the measured fixture gateway, all 312 loopback turns in series A completed at widths up to 20 on both connection layouts. The contiguous healthy width was 1 on the shared layout and none on the per-session layout, because foreground latency exceeded the rule at shared width 5 and at the per-session control. The computed recommendation is a shared connection, `max_in_flight = 1`, and approximately 1.1 MB per session, citing A-shared-1. Ten dropped-client admissions and twenty detached admissions returned saved results, and a resumed turn completed. The tool probe recorded zero matching tool executions despite ten completed turns, so its tool criterion failed and needs investigation.

## Scope and provenance

Hermes v0.22.4, commit `4d04303818b61bf3aa09278af48356315515965b`; Talaria probe commit `9a935b425a98`. Machine class: Apple M2 Ultra, 128 GiB memory. Execution mode: `unknown` (operator could not confirm managed-worker setting); child-process observations do not establish the configuration. No width cap, gateway restart or configuration change was made. Runs were dated 2026-10-28 UTC: A `dfd8956a`, B `a01684ae`, C `ab3e8d55`, D `ebc51101`, E `ac680e0f`. The probes created 160 distinct session rows in `fixture`; they detached subscriptions but did not delete sessions. The real-model run was performed at the operator's direction and imported from its recorded lines.

## Static readings and live verdicts

| Reading | Verdict and evidence |
| --- | --- |
| S1 — executor | Canonical turns overlapped, but the provider peak did not reach widths 10–20 in A; this measurement does not establish the executor's internal implementation. |
| S2 — admission | A admitted all distinct sessions and turns through 20 on both layouts. This does not establish behavior under a configured cap. |
| S3 — retention | E returned 20 saved results after detach and a 60-second wait, with a successful resume and next turn; longer retention and eviction are unmeasured. |
| S4 — cache | RSS after cleanup stayed within 20% of each before sample; the cache pressure policy itself was not tested. |
| S5 — execution policy | Execution mode is unknown; B was accepted. Child process counts do not prove policy selection. |
| S6 and S6a — launch fields | The descriptor gate and hidden loopback creates succeeded; 104 distinct sessions were created in A. |
| S6b — title reuse | Source reading only; the probes omit title. |
| S7 — admission | All A submissions returned admissions, with queued status; completion was observed for every turn. |
| S8 — receipts | D retrieved ten saved results on a fresh connection without attaching. |
| S8a — detach | E refused submission after detach; resume and an additional turn succeeded. |
| S9 — event ordering | A had no repeats, decreases or unresolved gaps; some gaps were repaired. This is bounded by the probe's event tracking. |
| S9a — replay | D reported no replay truncation; A reported no unresolved gaps. |
| S10 — completion and tools | A completions were observed once per admission. C recorded zero matching tool executions; B recorded no rate-limit keys. |
| S11 — process identity | Process-tree sampling succeeded for each live record. |
| S12 — loopback provider | A control completed all three turns and classified three turn requests. Unexpected routes were counted; their contents were not retained. |
| S13 — todo arguments | Not verified: C's matching execution count was zero, despite completed turns and two provider turn requests per session. |

## Measurements

A values are three repetitions; latency columns are milliseconds. Peak is the minimum provider overlap across repetitions. RSS is before / peak / after in MB. Failure numbers refer to the unit's health criteria.

| Record | Completed | Start p95 | Foreground p95 / max | Provider peak | RSS | Health failures |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| A-shared-1 | 3/3 | 49 | 34 / 47 | 1 | 501.6 / 502.7 / 502.5 | none |
| A-shared-5 | 15/15 | 617 | 74 / 212 | 5 | 502.5 / 503.7 / 503.3 | 3 |
| A-shared-10 | 30/30 | 2709 | 428 / 964 | 3 | 503.3 / 506.9 / 504.5 | 2, 3 |
| A-shared-16 | 48/48 | 3261 | 435 / 827 | 4 | 504.5 / 508.1 / 507.7 | 2, 3 |
| A-shared-20 | 60/60 | 3315 | 356 / 678 | 5 | 507.7 / 511.3 / 510.0 | 2, 3 |
| A-per-session-1 | 3/3 | 51 | 27 / 305 | 1 | 510.0 / 510.2 / 510.1 | 3 |
| A-per-session-5 | 15/15 | 272 | 47 / 156 | 5 | 510.1 / 511.3 / 510.9 | none |
| A-per-session-10 | 30/30 | 1127 | 332 / 653 | 6 | 510.9 / 512.6 / 514.0 | 2, 3 |
| A-per-session-16 | 48/48 | 2516 | 681 / 1107 | 4 | 514.0 / 519.8 / 519.3 | 2, 3 |
| A-per-session-20 | 60/60 | 2235 | 485 / 1079 | 5 | 519.4 / 529.3 / 528.6 | 2, 3 |

Highest contiguous healthy widths: shared 1; per-session none. The latter's width 5 was healthy individually but its width-1 control was not. This refutes the assumed healthy width 10 under the specified rule on this machine; it does not establish a universal gateway limit. CPU estimates and the full event and resource counts are in the results. CPU estimates omit short-lived processes absent from endpoint samples.

| B record | Completed / failed | Health failures | Observation |
| --- | --- | --- | --- |
| B-shared-1 | 1 / 0 | none | real provider |
| B-shared-5 | 5 / 0 | none | real provider |
| B-shared-10 | 9 / 1 | 1, 2, 3 | one `error`, kind `unknown`; no rate-limit keys observed |
| B-shared-16 | not run | — | stopped after an unhealthy width |

The single B error's cause is unknown; it is not evidence of a gateway fault or a rate limit. Series B used the free model and the operator-authorized quota.

| Record | Outcome | Health failures |
| --- | --- | --- |
| C-shared-10 | 10 turns completed; 0 matching tool executions | 7 |
| D-shared-10 | 10 terminal receipts with results; 0 unknown, unresolved or replay truncations | none |
| E-shared-20 | 20 terminal receipts with results; submit after detach refused; resume and next turn succeeded | 2, 3 |

D's receipt p95 was 16,528 ms and resume p95 170 ms. E's receipt p95 was 48 ms, resume 243 ms and next turn 721 ms. E showed no positive eviction evidence within the observation period. The largest measured A event rate was 293 events/s at A-per-session-20; this is a peak sample, not a guaranteed upper bound. C's recorded tool count may reflect either execution or observation behavior and must be investigated before asserting the tool failed to run.

## Recommendations and dependent work

`recommendations` computes `default_max_in_flight = 1`, `topology = shared`, `memory_mb_per_session = 1.099999999999966` (about 1.1), and `cited_runs = [A-shared-1]`. The estimate uses the peak minus before sample at width 1; it is not a production capacity forecast. No topology met the contiguous health rule at 10; the decision to adopt or revisit this default awaits the operator.

### #264 — Script engine

Keep concurrency control in the lifecycle host rather than the script engine; pass the measured scheduler limit through its interface.

### #265 — Journal and resume

Persist admissions and recover results by receipt after reconnect, preserving unknown outcomes. D and E support recovery without gateway restart; attach with `session.resume` before another submit.

### #266 — Hermes binding

Use the measured shared connection and cite A-shared-1. Use distinct request identifiers, hidden tool sessions and explicit model/provider, omit title, and enforce the descriptor gate. D and E support receipt-before-resume recovery.

### #267 — Staffing and scheduling

Set the proposed default to 1 and budget approximately 1.1 MB per session from A-shared-1; only the operator may raise it. Re-measure on the intended deployment before extrapolating.

### #268 — Progress view

Handle replay repair and epoch changes; size coalescing with the observed peak of 293 events/s from A-per-session-20 while allowing higher bursts.

### #269 — Runner lifecycle

Collect receipts after reconnect, then resume before new submissions. D and E did not show a recovery failure without restart.

### #271 — Built-in workflows

Use the scheduler's proposed fan-out default of 1 until an operator approves a different measured value.

### #273 — Phase acceptance

Use the measured shared width 1 for the proposed acceptance baseline and obtain the operator's decision on the failed width-10 assumption.

## Re-running and open questions

Use the pinned toolchain and the operator-owned gateway serving `fixture`; do not start, restart or reconfigure it. Run the ignored live tests for A, C, D and E with `TALARIA_LIVE_REQUIRE=1`; the operator controls the quota-consuming B run and import. Records merge by id in the Cargo test temporary directory. Copy the results into the fixture and run the offline results suite and privacy checks. The operator should confirm execution mode, decide whether to adopt the width-1 recommendation, and investigate the C tool observation and B's unknown error. The 60-second retention window does not establish behavior after a gateway restart or longer detach.
