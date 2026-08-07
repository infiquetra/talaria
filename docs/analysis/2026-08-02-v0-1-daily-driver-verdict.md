# Talaria v0.1 daily-driver verdict

**Date:** 2026-08-03 · **Unit:** U10 of the v0.1 prototype plan · **Branch:** `feat/talaria-milestone-2`
**Hermes baseline:** `7f4d15515` · **Requirements covered:** R1, R2, R3, R34, R36, R39; AE7, AE10; F1, F7

## How to read this document

The question is narrow: **can Talaria v0.1 be used as somebody's terminal client
for real work?** This document answers it from an evidence table rather than
from an opinion. Every row below names one thing the plan asked to be
verified, what was actually done, and where the proof lives. The verdict at the
end follows from the rows; it is not a summary of intent.

Three words are used precisely and never interchangeably:

- **Measured** — a program observed the behaviour and a test now pins it.
- **Inferred** — read from the pinned Hermes source, not run.
- **Unmet** — the verification the plan asked for has not happened.

## The finding that decided this document

**Superseded 2026-08-05, and kept because it is the record of what this document
originally rested on.** This section used to open: "**Talaria has never connected
to a Hermes gateway.** Not once, in any unit of this build." That was true when
it was written on 2026-08-02 and stopped being true on 2026-08-04, when Talaria
attached to a real Hermes dashboard and streamed turns to completion — see
evidence-table rows 17 and 18, which cite the recordings by digest. Read the rest
of this section as history: its argument about the stub still holds for
everything the recordings do not cover, which is most of the method table.

**Talaria had never connected to a Hermes gateway when this document was first
written.** Every automated test in the repository still dials a stub WebSocket
server on loopback that this repository wrote, answering with bodies transcribed
by hand from Hermes's handlers at `7f4d15515`.

The stub is a good stub. It is a real server on a real socket, it reads the
credential the way Hermes does — from the URL query string and nowhere else —
and it records the whole handshake so a test can prove a token appeared in no
header. What it cannot do is disagree with the transcription. Almost every
protocol belief in this client is a belief about a document, checked against a
server built from the same document.

**"Almost", because of one corpus.** In unit U2 the TypeScript reference
recorder — not this Python client — attached to a real Hermes dashboard on
loopback and captured a short session: two attaches, a session opened, and four
turns streamed to completion (4 `message.start`, 4 `message.complete`, 8
`thinking.delta`, 2 `gateway.ready`). That recording is on the machine that ran
U2 and is deliberately not in version control (R29); it is identified here by
digest, `04c556ac45e3cbf80f8ba22f3d2e4cec86c640d6d60d47ef9d38b9470cb86068`,
46 frames.
Two of its frames are JSON-RPC responses whose top-level shapes were checked
against U3's pinned baseline with the repository's own `compare_shape`, and both
match with no drift:

| Observed response | Matches the pin for | Result |
|---|---|---|
| `session_id`, `stored_session_id`, `message_count`, `messages`, `info` | `session.create` | no drift |
| `status` | `prompt.submit` | no drift |

Two qualifications, because this is exactly the kind of evidence that gets
overstated. The recording captured **server-to-client frames only**, so the
method *names* above are read off the response shapes and the sequence rather
than off a recorded request — the first shape fits `session.create`'s pin
exactly and mismatches `session.resume`'s on seven keys, and the second is
followed immediately by a streamed turn. And it says nothing about the other nine
evidence-only methods, nothing about request parameters, and nothing about
Talaria's own transport, which is what actually has to work.

Everything below followed from that when this document was written. It no longer
does: rows 17 and 18 rest on Talaria's own live recordings from 2026-08-04, and
the verdict is restated on the table rather than on this section.

## Evidence table

Status values: **measured**, **inferred**, **unmet**.

| # | What the plan asked for | Status | What was actually done | Where |
|---|---|---|---|---|
| 1 | **R34** — startup verification invokes only KTD9's read-only set | measured | Five read-only methods probed over a real socket; the *server's* received-call record contains those five names and none of the thirteen mutating ones | `tests/transport/test_compat_baseline.py::test_no_mutating_method_appears_in_the_startup_call_log` |
| 2 | **AE7** — a missing method is named and blocks ready | measured | Each of the five read-only methods removed in turn (`-32601`, the gateway's own answer at `tui_gateway/server.py:1762`); the report names that method, the other four still pass, `ready` is false | `test_a_missing_method_is_named_and_blocks_ready` |
| 3 | **AE7** — a drifted response shape is flagged | measured | A dropped key, a changed value kind, and an added key each flagged by name against the pinned signature | `test_a_dropped_response_key_is_flagged_by_name` and the two beside it |
| 4 | A probe that never answers must not read as a pass | measured | An unanswered probe grades `unproved` and blocks; a gateway that drops the socket mid-check leaves one named `unproved` row and two `present` rows | `test_a_probe_that_never_comes_back_blocks_rather_than_passing` |
| 5 | The startup probe cannot invoke a mutating method by mistake | measured | The guard is called directly with each of the thirteen evidence-only entries and raises before the dispatcher is touched (call count asserted at zero) | `test_probing_an_evidence_only_method_raises_before_any_call` |
| 6 | The **thirteen** other required methods are compatible | **measured** | Never probed at startup, so this row was `inferred` from 2026-08-02 to 2026-08-07: its evidence was the pinned source line plus the recorded request fixture and response shape in `talaria/domain/compat.py`. **Graded four times by enumerating methods rather than by counting runs** — 2026-08-06, after the row-19 acceptance run, after the F2–F6 live-evidence run, and after the secret-bridge probe of 2026-08-07 — see §The method enumeration. Those passes found **eight**, **nine**, **sixteen**, then **seventeen** distinct methods of the required eighteen. **Twelve of the thirteen evidence-only methods have a live call whose reply was matched to it and compared against the pinned shape**; the thirteenth, `terminal.read.respond`, was taken out of scope for runtime evidence on 2026-08-07 with its condition named below and stays required for compatibility. The comparison found **two pinned shapes wrong**, both corrected on source evidence — see §The reply side. Top-level keys and value kinds only (row 6a) | `talaria/domain/compat.py`; §The method enumeration; §The reply side |
| 6a | What "shape matches" covers | measured | Top-level only: the response's own key set and each value's kind. A gateway whose every *nested* payload had changed was graded `present` with `0 blocking` — deliberate v0.1 scope (`talaria/domain/compat.py:343`), stated here because `present` sounds broader than it is | `talaria/domain/compat.py::compare_shape` |
| 7 | **R36** — a normal exit restores the terminal | measured | The real client run on a pseudo-terminal; `termios` snapshotted before, during and after; attributes after are byte-identical to before. The falsifiability control (`SIGKILL` on the same run) leaves the terminal in raw mode, and that is asserted | `tests/ui/test_teardown.py::test_a_normal_exit_restores_the_terminal_modes`, `::test_the_terminal_restore_assertion_can_fail` |
| 8 | **R36** — an induced mid-stream failure still restores the terminal | measured | A frame source that streams two frames then raises; the app reports it, closes the source and exits 70; the terminal is restored | `::test_an_induced_mid_stream_failure_still_restores_the_terminal` |
| 9 | **R36** — no child process outlives Talaria | measured | The status command backgrounds a ten-minute worker; the worker is verified alive during the run and gone after exit, on both the normal and the failure path. A third case — a child spawned but **not yet recorded** by the runner — leaked, was found by CI after this document was first written, and is fixed; see §A leak this document originally missed | `::test_a_normal_exit_leaves_no_status_child_or_grandchild`, `::test_an_induced_mid_stream_failure_still_stops_the_status_child`, `tests/status/test_process_contract.py::test_aclose_sweeps_a_child_whose_spawn_has_not_been_recorded_yet` |
| 10 | **R36** — local waiters resolve at teardown | measured | A call in flight against a gateway that never answers resolves `unknown` with reason *the transport was closed*, rather than hanging | `::test_a_call_in_flight_at_teardown_resolves_instead_of_hanging` |
| 11 | **F7** — the gateway survives Talaria's exit | measured *against the stub* | Two tests. In process: after teardown a second client dials the same server object and receives the greeting. At process granularity: the stub runs as a **separate OS process, left in Talaria's own process group** so a mis-aimed group signal would kill it, and after teardown it is alive, still accepting connections, and still greeting. The server is the loopback stub, not Hermes | `::test_the_gateway_is_still_serving_after_talaria_exits`, `::test_the_gateway_process_survives_a_talaria_that_shares_its_process_group` |
| 12 | **R1** — argv carries no credential | measured **on macOS and Linux** | A running process built by the real launcher, holding a live credential in memory, inspected through the platform's own facility — `ps -ww` on macOS, `/proc/<pid>/cmdline` on Linux: no token, no `?token=` URL, no endpoint. The Linux half was measured when this branch first reached CI: all five process-surface tests ran and passed on `ubuntu-latest` under Python 3.12 and 3.13 (run `30865814553`). Earlier drafts of this document said the `/proc` branch had never executed, which was true when written | `tests/transport/test_process_surface.py::test_a_running_talarias_command_line_carries_no_credential` |
| 13 | **R1** — the environment carries no credential | **partially unmet — see below** | Talaria adds nothing credential-shaped to its own environment (set comparison against what it was launched with). An **inherited** `HERMES_DASHBOARD_SESSION_TOKEN` remains visible for the process's life and cannot be removed. **Re-graded exactly one step on 2026-08-06**, and no further, because that is the whole of what the decision supports. The open precedence question this row used to rest on **has been decided**: `HERMES_DASHBOARD_SESSION_TOKEN` was removed from the credential chain (option (b) of `QUEUED.md`'s entry), so the environment-free credential-file route is now documented in `README.md` as the supported one and R1's environment clause became **satisfiable by operator procedure**, which it was not before. It did **not** become met, on two residuals the deciding unit wrote down itself: an inherited variable is still readable from `/proc/<pid>/environ` and `ps -E` whether or not Talaria consults it, and route 1 — a `token` on `TALARIA_GATEWAY_URL` — is a *surviving supported route that is an environment variable*, and is the highest-precedence one. So this row moves from "a decision nobody has taken" to "a decision taken, with a residual the decision explicitly refuses to grade away" | `::test_talaria_adds_no_credential_of_its_own_to_its_environment`, `::test_the_inherited_credential_is_visible_in_the_process_environment`; `docs/engineering-journal/DECISIONS.md`, "`HERMES_DASHBOARD_SESSION_TOKEN` leaves the credential chain, and row 13 may be re-graded exactly one step — not to *met*" |
| 14 | **AE10** — a clean-environment install produces a working `talaria` | measured locally **and in CI** | `uv tool install .` into a fresh prefix, then the console script invoked by absolute path under `env -i`: `talaria --help` works. The CI `install` job ran for the first time on this branch's pull request and passed on Python 3.12 and 3.13 (run `30865814553`) | this document, §Install |
| 15 | **R39** — the platform matrix records exactly what was exercised | measured | See §Platform matrix. One operating system, two Python versions, two terminal hosts, one multiplexer | §Platform matrix |
| 16 | The launcher runs end to end — attach, probe, open, render, exit | measured *against the stub* | The real console script (`python -m talaria.cli`, no arguments) on a pseudo-terminal against the loopback stub: one connection accepted, the five read-only probes and no mutating method among them, exactly one `session.create`, tens of kilobytes of interface drawn, `ctrl+q` → exit 0, terminal restored | §Launcher run |
| 17 | **R2** — live startup acceptance against a running gateway | measured | The KTD7 precedence chain resolves into a real `session.create` / `session.resume` call and the launcher completes that sequence — now against a running Hermes gateway and not only against the stub. Measured over the live frame-log corpus, cited by digest and count rather than by path (R29): `talaria-live-corpus-v1-2659f-bd69e537f1d9`, 17 recordings, 2,659 frames. **How that digest is built**, so it can be re-derived: `sha256` over each recording's raw bytes concatenated in filename-sorted order, and the count is lines whose `kind` is `frame` summed across the corpus. It is an aggregate over many recordings, so it deliberately does *not* wear `live_corpus_identity`'s single-recording `talaria-live-v1-…` label. **What was counted:** gateway *replies*, not Talaria's calls — a call going out only proves Talaria tried — with each reply matched to the call that produced it by JSON-RPC `id`. 15 of the 17 recordings carry a reply whose result holds a `session_id`, from `session.create` and from `session.most_recent`; in all 15 the five read-only startup probes of row 1 were each answered as well. The remaining 2 are header-only recordings holding zero frames, and they answer nothing — the check does come back false where there is nothing to find. **This row previously read `unmet`**, on the reason "No Hermes gateway has answered one", which was true when it was written on 2026-08-02 and stopped being true on 2026-08-04 | corpus `talaria-live-corpus-v1-2659f-bd69e537f1d9`; `tests/transport/test_session_startup.py` for the stub half |
| 18 | **R3** — one live turn streamed to completion, compared against replay | measured | The row asks for two things, and each is cited separately because citing only the first would leave the row true-sounding and under-evidenced. **Streamed to completion:** over the same corpus `talaria-live-corpus-v1-2659f-bd69e537f1d9` (17 recordings, 2,659 frames, construction stated in row 17), the sequence an outbound `prompt.submit`, then an inbound `message.start` with no other `message.*` event in between, then one or more `message.delta`, then `message.complete`, occurs in 12 of the 17 recordings — 18 completed turns in total, one for every `prompt.submit` the corpus contains, carrying 1 to 616 `message.delta` frames per turn. **Compared against replay:** `docs/engineering-journal/LEARNINGS.md:111` records the comparison `talaria/cli.py` specifies — one live turn streamed to completion and its transcript compared against a replay of the same frames — passing on the 32-frame recording `talaria-live-v1-32f-5f477fa24fa5`: three live rows, three replayed lines, byte-identical, `interface_shows_everything` true. That label is `live_corpus_identity`'s single-recording form and re-derives from the corpus as `sha256` `5f477fa24fa50b391d73eee6f455190000281980a8db33c17f4130208d997549` over 32 frames, frame-log v1 recorded 2026-08-04T19:37:35.709Z. **This row previously read `unmet`**, on the reason "Nothing was submitted to a Hermes session. The replay-versus-live equivalence claim rests on the shared frame-source seam, not on a compared transcript." The first sentence stopped being true on 2026-08-04; the second is answered by the byte-identical comparison above | corpus `talaria-live-corpus-v1-2659f-bd69e537f1d9` for the streaming half, recording `talaria-live-v1-32f-5f477fa24fa5` and `docs/engineering-journal/LEARNINGS.md:111` for the replay half |
| 19 | **F1, F7** demonstrated live in an isolated session | **met** | **Met on 2026-08-07**, on the observation this row itself named as the one that would settle it. The operator executed `docs/plans/2026-08-06-u6-row19-operator-checklist.md` end to end against a live Hermes dashboard, present at the terminal, producing corpus `talaria-live-corpus-v1-107f-e40d9fd04ec5` (7 recordings, 107 frames). **F7** is settled by the sample this row asked for and no frame log could give: the gateway process's PID and start time taken before Talaria attached and again immediately after it exited, identical across the pair, with Talaria's exit confirmed by its absence from the process table rather than assumed — so the gateway is the same process, not a restart with an identical signature, which is the ambiguity the adjacent-recordings evidence could not resolve. **F1** ran end to end in a throwaway session nobody else was using: all five read-only startup probes answered and `session.create` landed a session. The four remaining branches also cleared. `--resume` and `--session <id>` ran live for the first time, and `session.resume` — never called in any prior corpus — appears in three recordings; a `--session` id the gateway did not know was refused (`code 4007`) with Talaria **failing rather than falling back** to the most-recent session it already held, which a passing run could not have demonstrated. The authentication-failure branch was forced by restarting the dashboard and observed verbatim on screen, then recovered with `talaria refresh-credential`. The absent-capability branch was reached live through a profile the gateway does not recognize, with no stub. The compatibility check reported five probes, five present, zero blocking. Two findings this run produced rather than assumed: `spawn_tree.list` is **present, not refused** against a live gateway (the refusal was fixture behavior), and `absent_capability` misreports a mistyped profile name as a too-old gateway — both queued, neither affecting this grade | corpus `talaria-live-corpus-v1-107f-e40d9fd04ec5`; `docs/work-sessions/2026-08-07-row19-live-acceptance-results.md`; `docs/plans/2026-08-06-u6-row19-operator-checklist.md` |

## The method table

Eighteen gateway methods are required. Five can be checked at startup without
side effects; thirteen cannot, and are not.

**Probed at startup — verified live against a socket (a stub socket):**

| Method | Purpose | Pinned evidence | Runtime status |
|---|---|---|---|
| `session.most_recent` | `--resume`'s target | `tui_gateway/methods_session.py:214-260` | probed, top-level shape compared |
| `spawn_tree.list` | finished sub-agent fan-outs | `methods_session.py:2860-2908` | probed, top-level shape compared |
| `agents.list` | registered agent processes | `methods_tools.py:1594-1616` | probed, top-level shape compared |
| `delegation.status` | live sub-agent roster and caps | `methods_session.py:2778-2795` | probed, top-level shape compared |
| `commands.catalog` | the slash inventory | `methods_tools.py:255-367` | probed, top-level shape compared |

**Never probed — mutating or request-scoped (R34). Evidence only:**

| Method | Purpose | Pinned evidence | Runtime status |
|---|---|---|---|
| `session.create` | default-new startup | `methods_session.py:14-158` | **never invoked by Talaria.** Real top-level response shape observed in U2's live capture; matches the pin |
| `session.resume` | `--resume` / `--session` startup | `methods_session.py:306-699` | **called by Talaria against a real Hermes** — 2026-08-07 |
| `prompt.submit` | submitting a turn | `methods_prompt.py:67-313` | **never invoked by Talaria.** Real top-level response shape observed in U2's live capture; matches the pin |
| `session.interrupt` | cancelling a turn | `methods_session.py:2706-2775` | **called by Talaria against a real Hermes** — 2026-08-07 |
| `subagent.interrupt` | interrupting one child | `methods_session.py:2806-2814` | **called by Talaria against a real Hermes** — 2026-08-07 |
| `slash.exec` | the route an ordinary slash command takes | `methods_tools.py:1073-1211` | **called by Talaria against a real Hermes** — ten calls across the live recordings, all answered |
| `command.dispatch` | the fallback slash route, for what `slash.exec` refuses | `methods_tools.py:432-1071` | **called by Talaria against a real Hermes** — 2026-08-07 |
| `paste.collapse` | collapsing a large paste | `methods_complete.py:14-39` | **called by Talaria against a real Hermes** — 2026-08-07 |
| `approval.respond` | answering an approval | `methods_prompt.py:886-920` | **called by Talaria against a real Hermes** — 2026-08-07 |
| `clarify.respond` | answering a clarification | `methods_prompt.py:858-864` | **called by Talaria against a real Hermes** — 2026-08-07 |
| `secret.respond` | answering a secret bridge | `methods_prompt.py:881-883` | **called by Talaria against a real Hermes** — 2026-08-07 |
| `sudo.respond` | answering a sudo bridge | `methods_prompt.py:876-878` | **called by Talaria against a real Hermes** — 2026-08-07 |
| `terminal.read.respond` | answering the terminal-read bridge | `methods_prompt.py:867-873` | **out of scope for runtime evidence — see below.** Still required for compatibility; the request is only emitted by a gateway started with `HERMES_DESKTOP` set |

**Thirteen of eighteen required methods are never probed at startup**, and as of
2026-08-07 **twelve of those thirteen have been called by Talaria itself**
against a real Hermes dashboard on loopback, with each call's reply matched back
to it and compared against the shape pinned here. One has not:
`terminal.read.respond`, which is out of scope for runtime evidence for the
reason set out immediately below. Earlier drafts of this section recorded
`session.create` and `prompt.submit` as response shapes observed through the
TypeScript reference recorder, which was true when written; Talaria's own frame
logs now carry the calls.

**Two of the shapes in this table were wrong, and the comparison is what found
them.** `approval.respond` was pinned as returning `resolved` as a `bool` when
the gateway returns a count, and `session.resume` returned a `messages_omitted`
key this baseline did not record at all. Both are corrected, and §The reply side
sets out how each was confirmed against the Hermes source rather than against
the reply alone.

### `terminal.read.respond` is out of scope for runtime evidence, and here is the condition

**The decision.** Row 6 does not require runtime evidence for
`terminal.read.respond`. It remains in `talaria/domain/compat.py` and remains
required for compatibility — nothing about what Talaria must support has
changed. What changed is that this row stops counting its absence as a gap.

**Why, stated as a condition so it can be falsified.** The bridge is answered by
Talaria without a human overlay (`UNATTENDED_KINDS` is exactly
`{"terminal_read"}`), so the client half exists and would run. The request
never arrives because the gateway never emits it. The gateway emits
`terminal.read.request` only when the agent calls the `read_terminal` tool, and
that tool is offered to the model only when its registration check passes:

```python
def check_read_terminal_requirements() -> bool:
    """Desktop GUI only — HERMES_DESKTOP is set on the gateway the app spawns."""
    return (os.getenv("HERMES_DESKTOP") or "").strip().lower() in ("1", "true", "yes")
```

`HERMES_DESKTOP` is set on the gateway **the Hermes desktop application
spawns**. ADR-0001 makes Talaria a client that dials a gateway it did not
launch, so whether that variable is set is not Talaria's to arrange.

**What was ruled out first**, because it is the obvious explanation and it is
wrong: the platform callback being absent. `tui_gateway/server.py` wires
`read_terminal_callback` for every session and it reaches the tool through
`run_agent.py` → `agent_init.py` → `tool_executor.py`. The session used on
2026-08-07 had one; the tool was still not offered.

**The falsifier.** Point Talaria at a gateway whose process has `HERMES_DESKTOP`
set, ask the agent to read the terminal, and the bridge should complete. If it
does, this exclusion is wrong and the row goes back to requiring it. Until
somebody runs that, "unreachable" is a claim about one environment variable and
is written here as one.

**What this does not do.** It does not reduce the eighteen, and — on the day it
was written — it did not clear row 6 either: `secret.respond` was still
outstanding and the row blocked on it. `secret.respond` was answered live later
the same day, so row 6 now clears with this method still excluded. That
sequencing is worth keeping visible: the exclusion did not clear the row, and a
reader should not read it as having done so.

The startup check states the gap on every run rather than reporting
"compatible": its first line reads `gateway compatibility: 0 blocking, 12
unverified at runtime (evidence-only, R34), baseline 7f4d15515`. The count in
that line is derived from the verdicts, not written into a string, because the
same arithmetic was wrong in three places in an earlier draft of this document
and of `talaria/transport/compat_check.py`.

## The method enumeration

**Added 2026-08-06, and this section is how row 6 is now graded.** Row 6 had been
re-graded twice before by reasoning about which runs had happened. That is the
failure mode this whole document exists to catch: *a row improves because runs
happened rather than because methods were observed*. The grade below is
mechanical instead. Every outbound frame Talaria sends is written to the frame
log before it goes on the wire (`talaria/transport/source.py`, where `call()`
records `request.to_frame()`, and the frame carries `method`), so the set of
methods a run exercised is recoverable from its recording rather than remembered.

**What was counted.** Every line of every frame log in the recording corpus whose
`kind` is `frame` and whose `dir` is `out` — Talaria's own calls, not the
gateway's replies — grouped by the `method` field of its `frame` object. The
corpus is the same one rows 17, 18 and 19 cite,
`talaria-live-corpus-v1-2659f-bd69e537f1d9`: 17 recordings, 2,659 frames.

**Eight distinct methods of the required eighteen appear. Ten do not.**

| Method called | Required set it belongs to | Calls | Recordings |
|---|---|---|---|
| `commands.catalog` | read-only startup probe | 30 | 15 of 17 |
| `session.most_recent` | read-only startup probe | 15 | 15 of 17 |
| `spawn_tree.list` | read-only startup probe | 15 | 15 of 17 |
| `agents.list` | read-only startup probe | 15 | 15 of 17 |
| `delegation.status` | read-only startup probe | 15 | 15 of 17 |
| `session.create` | **evidence-only (one of the thirteen)** | 15 | 15 of 17 |
| `prompt.submit` | **evidence-only (one of the thirteen)** | 18 | 12 of 17 |
| `slash.exec` | **evidence-only (one of the thirteen)** | 10 | 4 of 17 |

The only other `method` value anywhere in the corpus is `event`, which appears
2,393 times and only ever inbound — it is Hermes's notification envelope, not one
of the eighteen. No outbound frame carries a method outside the required set.

### Re-enumerated 2026-08-07, after the row-19 acceptance run

The same count, re-run over the corpus as it now stands —
`talaria-live-corpus-v1-2766f-5cd2ddb01be3`, **24 recordings, 2,766 frames**,
the 2026-08-04 corpus plus the seven recordings the acceptance run produced.

**Nine distinct methods of the required eighteen appear. Nine do not.**

| Method called | Required set it belongs to | Calls | Recordings |
|---|---|---|---|
| `commands.catalog` | read-only startup probe | 42 | 21 of 24 |
| `session.most_recent` | read-only startup probe | 22 | 21 of 24 |
| `spawn_tree.list` | read-only startup probe | 21 | 21 of 24 |
| `agents.list` | read-only startup probe | 21 | 21 of 24 |
| `delegation.status` | read-only startup probe | 21 | 21 of 24 |
| `session.create` | **evidence-only (one of the thirteen)** | 18 | 18 of 24 |
| `prompt.submit` | **evidence-only (one of the thirteen)** | 18 | 12 of 24 |
| `slash.exec` | **evidence-only (one of the thirteen)** | 12 | 5 of 24 |
| `session.resume` | **evidence-only (one of the thirteen)** | 3 | 3 of 24 |

**The change is exactly one method.** `session.resume` moved from never-called to
called, in three recordings, because the acceptance run exercised `--resume` and
`--session <id>` — the two startup paths that had never run against a live gateway.
Nothing else moved: seven live runs driven by hand through a model picker, a
profile picker and a credential-failure branch added one method to this table.

That number is the point of enumerating rather than reasoning. "The operator ran a
six-step acceptance checklist end to end" invites the reading that live coverage
widened substantially. It widened by one of thirteen, and **row 6 does not clear.**

**Never called, by count rather than by recollection — nine of the thirteen:**
`session.interrupt`, `subagent.interrupt`, `command.dispatch`,
`paste.collapse`, `approval.respond`, `clarify.respond`, `secret.respond`,
`sudo.respond`, `terminal.read.respond`.

### Re-enumerated 2026-08-07 again, after the F2–F6 live-evidence run

The same count over the corpus as it now stands —
`talaria-live-corpus-v1-4629f-1004da012f45`, **29 recordings, 4,629 frames**,
the previous corpus plus the recordings the F2–F6 run produced. The run is
written up in `docs/plans/2026-08-07-row6-live-evidence-results.md`.

**Sixteen distinct methods of the required eighteen appear. Two do not.**

| Evidence-only method | Calls | Recordings | First live evidence |
| --- | --- | --- | --- |
| `session.create` | 23 | 23 of 29 | 2026-08-04 |
| `prompt.submit` | 30 | 13 of 29 | 2026-08-04 |
| `slash.exec` | 18 | 10 of 29 | 2026-08-04 |
| `session.resume` | 3 | 3 of 29 | 2026-08-07, row-19 run |
| `session.interrupt` | 5 | 1 of 29 | 2026-08-07, F2–F6 run |
| `approval.respond` | 3 | 1 of 29 | 2026-08-07, F2–F6 run |
| `subagent.interrupt` | 1 | 1 of 29 | 2026-08-07, F2–F6 run |
| `command.dispatch` | 1 | 1 of 29 | 2026-08-07, F2–F6 run |
| `paste.collapse` | 1 | 1 of 29 | 2026-08-07, F2–F6 run |
| `clarify.respond` | 1 | 1 of 29 | 2026-08-07, F2–F6 run |
| `sudo.respond` | 1 | 1 of 29 | 2026-08-07, F2–F6 run |
| `secret.respond` | 0 | 0 of 29 | **none** |
| `terminal.read.respond` | 0 | 0 of 29 | **none — out of scope, see above** |

**The change is seven methods**, and unlike the previous two re-enumerations
this one moved the row a long way without clearing it. Eleven of the thirteen
evidence-only methods now have live evidence. Of the two that do not,
`terminal.read.respond` is out of scope for the `HERMES_DESKTOP` reason set out
earlier, which leaves **`secret.respond` as the single method row 6 blocks on**.
*(As this pass left things. The next subsection is where that last method got
its evidence; row 6 blocks on nothing as of the end of 2026-08-07.)*

**Why `secret.respond` was not attempted rather than attempted and failed —
written before it was attempted, and left standing because the reasoning was
sound and one of its facts was not.** Its bridge is wired to the skills tool's
secret-capture callback, so provoking it means installing or configuring a
credential-capturing skill on the operator's own machine. That is a change to
their environment rather than a throwaway session, and it is theirs to
authorise. The trigger is known; only the decision is outstanding. Recording it
as "not attempted, and why" rather than as "unreachable" is the distinction this
section exists to keep.

**Corrected 2026-08-07: "a credential-capturing skill" overstated what the
bridge needs, and the overstatement is what made the decision look expensive.**
No credential is involved. The callback fires on any skill declaring a
`required_environment_variables` entry that is not already persisted, whatever
the variable is for, and an empty answer is a supported response the gateway
records as skipped without writing anything to disk. The provocation that
actually ran was a throwaway skill declaring one variable nothing reads. See
§The reply side.

`session.resume` **left this list on 2026-08-07** — the row-19 acceptance run
called it live for the first time, in three of the seven recordings of corpus
`talaria-live-corpus-v1-107f-e40d9fd04ec5`, which is also what settled the
startup precedence chain of `KTD7 (2026-08-02 prototype plan)`.

**The result of the re-grade is that row 6 does not move, and the reason matters
more than the result.** The model-picker plan sequenced six units — the admin HTTP
surface, the model picker, the credential decision, the profile picker, the
default-model picker, and the scripted live acceptance run — and the natural
reading of "six units of work happened" is that live coverage widened. When this
section was written on 2026-08-06 it had not: units U1–U5 were built against the
loopback stub and the repository suite, and U6 — the unit whose product *is* live
evidence — produced a checklist for an operator and stopped there.

**Amended 2026-08-07.** The operator then ran that checklist, and coverage widened
by exactly one method. The corpus is no longer the 2026-08-04 one; the acceptance
run added `talaria-live-corpus-v1-107f-e40d9fd04ec5` (7 recordings, 107 frames),
and `session.resume` gained live evidence. **Row 6 stayed `inferred` at that
point**, now on nine methods rather than ten. One method in seven recordings is
what a picker built and driven by hand actually exercises, which is the honest
measure of that work rather than the count of units in it. Two further passes on
2026-08-07 moved it the rest of the way; they are below and in §The reply side.

### Re-enumerated 2026-08-07 a third time, after the secret-bridge probe

The last method with no live traffic of any kind was `secret.respond`. It was
provoked on 2026-08-07 and the corpus now stands at
`talaria-live-corpus-v1-4670f-fc5790017b70`, **30 recordings, 4,670 frames**.

**Seventeen distinct methods of the required eighteen appear. One does not**, and
it is `terminal.read.respond`, which is out of scope for runtime evidence.

| Evidence-only method | Calls | Recordings | First live evidence |
| --- | --- | --- | --- |
| `secret.respond` | 1 | 1 of 30 | 2026-08-07, secret-bridge probe |
| `terminal.read.respond` | 0 | 0 of 30 | **none — out of scope, see above** |

Every other evidence-only method is unchanged from the table above. **The change
is one method**, and it is the one the row had been blocking on.

One caveat on what an enumeration can prove, stated because it is easy to read
the tables above as broader than they are. An outbound frame proves Talaria
*sent* a call, not that the gateway answered it correctly. Until 2026-08-07 this
section deliberately did not do the reply-side work — rows 17 and 18 did it for
the methods they cover — because its question was narrower: *which* methods have
any live traffic at all. That is no longer the whole of it. §The reply side does
the matching for every evidence-only method, and it is where row 6's grade
actually comes from.

## The reply side

**Added 2026-08-07, and this is the half that changes row 6's grade from
`inferred` to `measured`.** Enumerating outbound methods answers "did Talaria
call it". This section answers the question the row actually asks: when Talaria
called it, did the gateway's reply match the shape pinned in
`talaria/domain/compat.py`?

**What was counted.** Every outbound frame in the corpus whose method is one of
the thirteen evidence-only methods, paired with the inbound frame carrying the
same JSON-RPC `id` in the same recording, and the reply's `result` compared
against the pinned entry using `talaria.domain.compat.compare_shape` — the same
function the startup check applies, rather than a re-implementation of it. The
corpus is `talaria-live-corpus-v1-4670f-fc5790017b70`, 30 recordings, 4,670
frames.

| Evidence-only method | Calls | Replies matched | Errors | Drift against the pin |
| --- | --- | --- | --- | --- |
| `session.create` | 24 | 24 | 0 | none |
| `session.resume` | 3 | 2 | 1 | **`messages_omitted` unrecorded** |
| `prompt.submit` | 31 | 31 | 0 | none |
| `session.interrupt` | 5 | 5 | 0 | none |
| `subagent.interrupt` | 1 | 1 | 0 | none |
| `slash.exec` | 18 | 17 | 1 | none |
| `command.dispatch` | 1 | 1 | 0 | none |
| `paste.collapse` | 1 | 1 | 0 | none |
| `approval.respond` | 3 | 3 | 0 | **`resolved` is an int, pinned as bool** |
| `clarify.respond` | 1 | 1 | 0 | none |
| `secret.respond` | 1 | 1 | 0 | none |
| `sudo.respond` | 1 | 1 | 0 | none |
| `terminal.read.respond` | 0 | 0 | 0 | no live call — out of scope |

The two errors are the point of running this at all, and both were in the
baseline rather than in Talaria's behaviour.

**`approval.respond` returns a count, and was pinned as a flag.** Three live
replies carried `{"resolved": 0}`, `{"resolved": 1}` and `{"resolved": 0}` —
JSON integers, never `true` or `false`. The gateway handler returns
`resolve_gateway_approval(...)` verbatim, and that function is typed `-> int`
with the docstring "Returns the number of approvals resolved (0 means nothing
was pending)" (`tools/approval.py:2490-2505`). So this is not a reading of one
reply; the source says the same thing. `talaria/ui/app.py:527-529` already read
the field as a count and rendered "*n* resolved", which is why nothing
misbehaved and why nothing caught it: the code was right and only the record of
what Hermes returns was wrong. Corrected to `int`.

**`session.resume` returns a key the baseline never recorded.** Every reply
carried `messages_omitted`, a `bool`, and all three of the gateway's
`session.resume` success paths set it (`methods_session.py:466`, `:551`,
`:712`). Corrected as a required key rather than an optional one.

**And chasing that key found a defect worth more than the correction.** Talaria
reads neither `messages_omitted` nor `messages`; both appear nowhere in the
package outside the baseline. Checked on screen rather than left as a code
reading: `talaria --resume` against a session holding a real exchange got a
reply carrying `message_count = 3`, three `messages`, and
`messages_omitted = False` — the gateway withheld nothing — and **Talaria
rendered an empty transcript**. Nothing on the wire after the reply could carry
the history either. Row 19 graded `--resume` *pass* on 2026-08-07 and that grade
stands, because what it measured was which session each startup path lands in;
nobody asked whether the conversation appeared. Queued as P1 in
`docs/engineering-journal/QUEUED.md`, not fixed here.

**Why correcting the baseline is not moving the goalposts.** The baseline is a
record of what Hermes returns, and the row asks whether Talaria's model of these
methods matches reality. A measurement that finds the model wrong and corrects
it on source evidence is that row working. What would be dishonest is editing
the pin and reporting "no drift found" — so both errors are named above, both
carry the source line that confirms them independently of the reply, and
`tests/domain/test_recorded_reply_shapes.py` pins every recorded shape so a
revert fails rather than passes quietly.

**How `secret.respond` was finally provoked, and the correction that made it
cheap.** The bridge fires from `tools/skills_tool.py`, which subtracts the
variables already persisted in `~/.hermes/.env` from a skill's
`required_environment_variables` frontmatter and calls the gateway's
secret-capture callback once per variable left over. The gateway registers that
callback for every session (`tui_gateway/server.py`) and `session.create` sets
the `HERMES_INTERACTIVE` flag the capture path checks — so unlike
`terminal.read.respond`, nothing here is gated on the desktop application. The
provocation was a throwaway skill declaring one variable that nothing reads,
loaded once and deleted afterwards. **No credential was involved**: the answer
was an empty field, which the gateway's callback records as skipped and returns
without writing to disk, and the frame log shows the value redacted structurally
in any case (R9). Describing this as needing "a credential-capturing skill", as
this document did earlier in the day, overstated the cost by a wide margin and
is corrected above.

**What this section still does not prove.** `compare_shape` is top-level only —
key names and value kinds, nested structure deliberately out of scope for v0.1
(row 6a). A reply whose top-level shape matches but whose nested contents have
changed passes here. The row is graded on the standard the row declares, and
that standard is stated rather than assumed.

## R1 in full: which half holds

R1 asks that a running Talaria's command line **and environment** carry no
credential. The two halves have different answers and it is worth being exact,
because the difference decides what an operator should do.

**Holds, measured.** Talaria never puts the credential on its own command line,
and never puts it into its own environment. The endpoint object every module
holds has had every credential-bearing query parameter stripped at construction
(`AttachTarget`), so the credentialed URL exists for the duration of one `attach`
call and is unreachable afterwards. A running process that has *acquired and is
holding* a live credential was inspected: argv is clean, and the set of
environment names carrying the credential is **exactly** the set the process was
launched with — asserted as equality rather than as a subset, because a subset
assertion is satisfied by an environment read that came back empty.

**Cannot hold.** The operator's highest-precedence credential source is the
`HERMES_DASHBOARD_SESSION_TOKEN` environment variable (KTD11), which Talaria
inherits. On macOS the inherited value is readable through `ps -E` **by the
owning user**, and that was measured on a running process. On Linux the kernel
snapshots the environment block at `exec` and serves that snapshot from
`/proc/<pid>/environ` for the life of the process, so `os.environ.pop` changes
nothing a reader can see — **measured**, when this branch first reached CI: all
five process-surface tests ran and passed on `ubuntu-latest` under Python 3.12
and 3.13, exercising the `/proc` branch of the reader against a real running
process. (Earlier drafts said that sentence was read from documentation rather
than measured, and filed the unexecuted branch as a P2. It was true when written;
pushing the branch is what changed it.) **R1's environment clause is therefore
not met when the credential is supplied through the environment, on either
platform, and no change to Talaria can meet it.**

**The mitigation exists and is measured.** KTD11's third precedence level is a
`0600` credential file at `<config_dir>/credentials`. With the credential
supplied that way, the running process's environment carries nothing — asserted
in `test_the_credential_file_route_keeps_the_environment_clean`. That is why
that level exists, and an operator who cares about the process surface should
use it.

The unmet half is filed in `docs/engineering-journal/QUEUED.md` rather than
redefined into a pass. The test that measures the failure asserts the failure,
so if a future Talaria ever does scrub its inherited environment, that test goes
red and somebody has to delete it deliberately.

## Platform matrix

R39: **exactly what was exercised, nothing broader.** The two GitHub-runner rows
were marked *declared, not observed* until this branch was pushed; they now carry
the result of the run that opened its pull request.

| Operating system | Arch | Python | Terminal host | Multiplexer | What ran | Result |
|---|---|---|---|---|---|---|
| macOS 26.5.2 (build 25F84) | arm64 | 3.12.11 | headless (no tty) | none | `ruff`, `mypy --strict`, `pytest` (×19), `bandit`, `git diff --check` | `ruff`/`mypy`/`bandit`/`git diff --check` clean; `pytest` `875 passed, 1 skipped`. One run of thirteen failed before the prompt-geometry synchronization was fixed; the six runs since, under six busy loops, are green — see §Test-suite honesty |
| macOS 26.5.2 (build 25F84) | arm64 | 3.13 | headless (no tty) | none | `pytest` | `874 passed, 1 skipped` — one run only, so it says nothing about the intermittent failure above |
| macOS 26.5.2 (build 25F84) | arm64 | 3.12.11 | pseudo-terminal, `TERM=xterm-256color`, 100×30 | none | the real client, replay mode, normal exit and induced failure | pass — terminal restored in both, no surviving children |
| macOS 26.5.2 (build 25F84) | arm64 | 3.12.11 | tmux 3.7b pane, 100×30 | tmux 3.7b | `talaria replay`, rendered transcript read back from the pane, `ctrl+q` | pass — 28 non-blank rendered lines, exit 0 |
| macOS 26.5.2 (build 25F84) | arm64 | 3.12.11 | pseudo-terminal, `TERM=xterm-256color`, 100×30 | none | the real console script, live mode, against the loopback stub | pass — see §Launcher run |
| macOS 26.5.2 (build 25F84) | arm64 | 3.12.11 | `env -i`, no `TERM` | none | `uv tool install .` into a fresh prefix, then `talaria --help` | pass |
| macOS 14 (GitHub runner) | — | 3.12, 3.13 | — | — | full check; `install` job (`uv tool install` + `talaria --help`) | **pass**, run `30865814553` |
| Ubuntu (GitHub runner) | — | 3.12, 3.13 | pseudo-terminal (in-test) | none | full check, informational only — including all 14 pseudo-terminal teardown tests and all 5 process-surface tests | **pass** — `868 passed, 7 skipped`, run `30865814553`. The 7 skips are all the TypeScript equivalence bridge, which needs `node_modules` |

Not exercised at all, and therefore claimed nowhere: **Linux as a daily driver**
— the suite passes there, including the pseudo-terminal teardown and
process-surface tests, but no person has driven the interface on a Linux machine,
no tmux pane and no real terminal emulator has been used there, and the
launcher-against-the-stub run in §Launcher run was performed on macOS only —
Windows, any terminal emulator other than a bare pseudo-terminal and a tmux pane,
screen, mosh, any remote session, any terminal narrower than 100 columns under a
real emulator, and Python 3.14.

Library versions in the measured rows: Textual 8.2.8, websockets 15.0.1.

## Install

**What was done.** `uv tool install .` into a purpose-made prefix
(`UV_TOOL_DIR`/`UV_TOOL_BIN_DIR` pointed at fresh directories), then the
installed console script invoked **by absolute path** under `env -i` with only
`PATH` and a throwaway `HOME`. `talaria --help` printed its usage and exited 0.
Nothing was written outside the tool prefix, and nothing at all was written into
a Hermes installation — Talaria is a standalone client (ADR-0001) and installing
it must not touch the thing it connects to.

**In CI.** The `install` job ran for the first time when this branch was pushed
and opened its pull request, and passed on Python 3.12 and 3.13 (run
`30865814553`, 13 seconds each). Earlier drafts of this document recorded it as
declared but never executed.

**Why it is a separate CI job and not a step in the existing one.** The existing
`python-check` job builds the development environment with `uv sync --all-groups`
and runs everything through `uv run`. A `talaria` that worked only from inside
the project virtualenv would pass it. The `install` job never runs `uv sync` and
never uses `uv run`.

**What the install job does not do.** It does not re-run the repository checks.
Those need the development dependency groups, which is exactly the environment
this job exists to avoid; `python-check` runs them on the same operating system
and the same two Python versions.

## Launcher run

Row 16 of the evidence table, in full, because it is the closest thing in this
build to the run R2 asks for and the difference matters.

The stub gateway was started on loopback in one process. The **real console
script** — `python -m talaria.cli`, no arguments, the same entry point
`uv tool install` produces — was launched in a second process with its standard
streams on a pseudo-terminal, with `TALARIA_GATEWAY_URL` pointing at the stub and
the credential supplied through the environment. What the *server* recorded, in
arrival order:

```
commands.catalog      ← the mount-time catalogue fetch
session.most_recent   ┐
spawn_tree.list       │
agents.list           ├ the five read-only compatibility probes (R34, KTD9)
delegation.status     │
commands.catalog      ┘
session.create        ← the one mutating call, the operator's own startup path
```

Then: an interface drawn to the terminal, `ctrl+q`, exit code 0, terminal
attributes byte-identical to before the launch. (An earlier draft of this row
gave an exact byte count for the interface. It was removed: repeating the run
gives a different figure every time, because how much a Textual app paints
before a keystroke arrives depends on timing. A number that reads like a
measurement and is not reproducible is worse than no number. What the tests
assert is a floor — that something substantial was drawn — not a figure.)

**What this is evidence of.** That the launcher assembles, dials, authenticates
against a server that reads the credential from the URL query, runs the
compatibility check before opening anything, opens exactly one session, renders,
and tears down cleanly.

**What it is not evidence of.** That any of those seven calls means to Hermes
what this repository believes it means. The server on the other end was written
here.

## A leak this document originally missed

Worth its own section, because it is the one R36 defect that survived the build,
two adversarial reviews and thirteen green local test runs, and because how it
was found says more than what it was.

**What happened.** The branch was pushed and the pull request opened. The second
push carried **documentation changes only** — not one line of Python — and
`python-check (3.13)` went red on
`tests/ui/test_teardown.py::test_teardown_stops_a_status_child_this_app_does_not_own`,
a test U10 itself had added. A status child was alive after teardown.

**What it was.** `asyncio.create_subprocess_exec` forks and execs the child and
then keeps awaiting while the subprocess transport is wired up. For the whole of
that tail the child is running — far enough here to write its own pid to a file —
while `StatusRunner._process` is still `None`, because the parent is suspended
inside the `await`. And because it is suspended, other coroutines run. A teardown
landing in that window read `self._process is None`, concluded there was no child
to kill, and returned. Cancellation could not cover it either: the `finally` that
sweeps the process group opens *after* the spawn, so a spawn interrupted before
recording has no sweep at all.

**How it was confirmed, since it would not reproduce.** Twelve runs of the test
alone and three full-suite runs on Python 3.13 all passed locally. Rather than
re-run until it broke, the suspected window was widened deliberately —
`await asyncio.sleep(0.5)` inserted between the spawn returning and the
assignment — and it failed every time, on the same assertion. With the fix in and
that half-second window still widened, it passes.

**The fix and its test.** `aclose()` now waits on a `_spawn_settled` event before
deciding there is nothing to kill. The intermittent app-level test can only catch
this by luck of scheduling, so it is not the pin: the pin is
`tests/status/test_process_contract.py::test_aclose_sweeps_a_child_whose_spawn_has_not_been_recorded_yet`,
which makes the transport tail explicit, asserts the window genuinely exists
before relying on it, and was watched to fail with only the wait removed.

**Why it belongs in this document.** R36 is one of the requirements this document
grades, row 9 said *measured*, and it was — for the two cases anyone had thought
of. The honest reading is that a green suite covered the shapes its authors
imagined, and the first slower machine that ever ran it found a third. That is an
argument for the verdict this document already reaches, not against it.

## Test-suite honesty

- **The suite was not reliably green, and the cause turned out to be arithmetic
  rather than mystery.** `tests/ui/test_prompts.py::test_a_card_mounting_into_a_full_region_is_still_recomputed`
  — a U8-era prompt-geometry test this unit did not touch — failed about one full
  run in eight locally and twice on GitHub runners. It was never reproducible on
  demand: 20 runs of it alone and 6 full-suite runs under six busy loops all
  passed. **Resolved by reading the chain instead of re-running until it broke.**
  The marking is two chained `call_after_refresh` calls deep
  (`Rewrapped` → `reveal_actions` → `mark_unreachable_controls`) while the test's
  `settle()` helper pumps exactly two refresh cycles — a margin with no slack, so
  a machine that loses one cycle samples the title too early. The test now waits
  for the marking with a bounded budget. That is weaker than the old assertion in
  exactly one respect — it no longer asserts *how many refresh cycles* the marking
  takes — and unchanged in the respect that matters: with the trigger deleted, the
  defect the test exists for, it still fails. Verified, not assumed.
- **This was the second intermittent failure in this unit and the first one was a
  real defect**, so neither was dismissed as a test race without evidence. See
  §A leak this document originally missed.
- A green run prints `874 passed, 1 skipped` on Python 3.12.11 and on 3.13 (the
  skip is the TypeScript equivalence bridge, when `node_modules` is absent).
  That is the number a *passing* run prints, not the number the command reliably
  prints. Earlier drafts of this document quoted a test count as a measured
  platform-matrix result with no such qualification.
- Neither `pytest-randomly` nor `pytest-xdist` is installed here — verified by
  import. **A green run is therefore not evidence of order-independence, and no
  such claim is made.** Given the ordering-sensitive failure above, that
  disclaimer is doing real work.
- Behaviours this unit depends on were each broken individually in a disposable
  clone and their pinned tests watched to fail — eighteen in the build pass, and
  a further set covering the closeout fixes (see §Closeout). Two of the original
  eighteen needed a second pass: one survived because a second code path was
  quietly doing the same work — that path was removed rather than left as a guard
  nothing could exercise — and one survived because the break was too weak to be
  a defect, so it was made a real one.

## Closeout

Two adversarial reviews read this build and this document. What their findings
changed, so a reader can tell which sentences here are first-draft and which
survived being attacked.

**Defects in shipped code, fixed.** Three background tasks — the frame pump, the
live startup sequence and the catalogue fetch — were each started with
`asyncio.create_task` and awaited by nobody. The pump's version was found and
fixed during the build; the other two shipped with it. Measured: a startup
sequence that raises leaves the app running, `compat` unset, no session opened
and nothing on screen. All three now go through one supervisor that names the
failure and exits 70. Separately, a `status.command` written as a TOML array —
the obvious operator guess for an argv — produced a raw traceback and exit 1
from a bare `talaria`, because U10 is what put that call on the launch path.

**A capability the document's own remediation needed.** Step 1 below said "record
the frames". `LiveSource` accepted a recorder, the launcher never passed one, and
`talaria record` draws no interface — so the step this document names as the
thing that would move its verdict could not be performed. `talaria --record` now
exists.

**Test-isolation hole.** The repository-wide fixture claimed to clear every
`TALARIA_*` variable and cleared four, not including `TALARIA_GATEWAY_URL` — the
one that decides where a live dial would attach. It now sweeps by prefix, and the
test that says so runs the fixture against a deliberately polluted environment
rather than against a clean developer machine, where it would pass either way.

**Four unpinned lines, each of which could be deleted with the suite green.**
`shutdown_sources`' cancellation of the startup task, `build_live_app`'s status
runner, `run_live`'s exit-code propagation, and `shutdown_sources`' call to the
status runner's teardown hook. Each now has a test, and each test was watched to
fail with its line removed.

**Two comments that overstated their code, corrected.** `shutdown_sources` said
cancelling the status task was not enough to satisfy R36; measured, it is, and
the comment now says what the call actually adds. `request_for`'s docstring
justified a session-id substitution by a failure that happens on every real
launch; the substitution is not reached from the only production caller and the
docstring now says so.

**A public-log hazard.** Two process-surface tests put the whole inherited
environment on the right-hand side of an assertion. pytest prints failing
operands, this repository is public and so are its CI logs. The raw block is now
private and the assertions read booleans and variable names.

**Eleven mutations, each applied alone in a disposable clone, restored between
runs, all eleven killed.** Two needed a second pass. One mutation hit the wrong
function (the same argument name appears in `run_replay` and `build_live_app`)
and was made unambiguous. One genuinely survived: removing the startup task from
the teardown cancel set changed nothing, because against a real `LiveSource`
closing the transport resolves the calls in flight and the task ends by itself —
so the test now uses a dispatcher that never answers, which is what separates
"teardown cancelled it" from "it happened to end".

**Not fixed, recorded instead.** The intermittent prompt-geometry failure
(§Test-suite honesty), `compare_shape`'s top-level-only comparison, and the
silence of a malformed `status.command` are in
`docs/engineering-journal/QUEUED.md` with reproductions. A fourth item — the
Linux `/proc` branch that had never executed — was closed by pushing the branch
rather than by any change to the code.

## Verdict

**Re-read 2026-08-06 against the model-picker plan's finished units; the verdict
does not move and neither does any of its three reasons.** Rows 6, 13 and 19 were
each re-graded on what that plan actually produced — row 6 by enumerating methods
out of the recordings (§The method enumeration), rows 13 and 19 against the
products of units U3 and U6. One of the three moved *within* its grade: row 13's
open precedence question has been decided, which narrows the reason without
clearing the row. The other two are unchanged. **No condition cleared, so no
`Clears:` backlink is written anywhere by this re-grade** — a backlink naming a
condition this gate still blocks on is the contradiction
`tests/docs/test_gating_documents.py` exists to fail, and writing one to mark
"work happened here" would be that contradiction.

**Restated 2026-08-05 on a corrected table.** Rows 17 and 18 read `unmet` until
that date on reasons that stopped being true on 2026-08-04; both rows now record
what they used to say and why. This section is rewritten on the table as it now
stands, and what it used to say is set out at the end of it.

Reading the table: rows 6, 17, 18 and 19 are **measured or met** and row 13
is **partially unmet**. Row 6 covers thirteen of the eighteen required gateway
methods, and as of 2026-08-07 twelve of those thirteen have been called by
Talaria against a real gateway, answered, and had the answer compared against
the pinned shape; the thirteenth, `terminal.read.respond`, is out of scope for
runtime evidence on a named condition. Rows 12 and 14 were the two weakest *measured* rows in
earlier drafts — macOS-only, and a CI job that had never run — and both were
closed by pushing this branch. The suite itself fails intermittently, twelve runs
green in thirteen.

**Row 19 cleared on 2026-08-07**, and it is worth saying what closed it, because
the row had spent five days blocked on something no amount of testing could
produce. It asked for one observation taken *outside* the frame log — the gateway
process's PID and start time, sampled before Talaria attached and again after it
exited — because a frame log ends at the exit being tested and can never settle
F7. The operator ran the checklist and took that sample. The two readings are
identical and Talaria's exit was confirmed by its absence from the process table,
which distinguishes "the gateway survived" from "the gateway died and was
restarted with an identical signature" — the ambiguity the adjacent-recordings
evidence in this row could not resolve.

Clears: v0-1-daily-driver#row-19

**Row 6 cleared later the same day**, and what closed it was the last of its
thirteen evidence-only methods getting live traffic — `secret.respond`, provoked
by a throwaway skill that declares one environment variable nothing reads. The
row is graded `measured` rather than `met` because the grade rests on a
measurement described in §The reply side: every one of the twelve in-scope
methods now has a live call, a reply matched back to it on JSON-RPC `id`, and
that reply compared against the pinned shape. The comparison found two pinned
shapes wrong and both were corrected on Hermes source evidence, which is the
most useful thing this row has produced.

Clears: v0-1-daily-driver#row-6

AE7 and R39 say the ready verdict is blocked on any gap. **One gap remains, so
the verdict does not move.**

### Talaria v0.1 is **NOT READY** as a daily driver.

**The verdict has not moved. Its reasons have moved entirely.** That distinction
is the point of this restatement: three words that used to rest on "this client
has never attached" now rest on one specific, much narrower gap, and a reader
who acts on the old reasons would be acting on a fact that expired on 2026-08-04.

The **one remaining** gap — rows 19 and 6, which were the other two, both cleared
on 2026-08-07:
- **Row 13 — R1's environment half is partially unmet.** Talaria adds no
  credential of its own to its environment, but an inherited
  `HERMES_DASHBOARD_SESSION_TOKEN` stays readable for the life of the process and
  Talaria cannot remove it. **The reason changed on 2026-08-06 and the grade did
  not.** This gap used to be "a decision nobody has taken": the
  credential-file-versus-environment-variable precedence question was recorded in
  `docs/engineering-journal/QUEUED.md` as deliberately open. It has now been
  decided — option (b), the dashboard variable leaves the chain — and the
  environment-free credential-file route is documented in `README.md` as the
  supported one. What still blocks the row is not the decision but its stated
  residual: an inherited variable stays readable from `/proc/<pid>/environ` and
  `ps -E` whether or not Talaria reads it, and the surviving highest-precedence
  route (a `token` on `TALARIA_GATEWAY_URL`) is itself an environment variable.
  The unit that took the decision wrote down in the same breath that this row may
  not be graded *met* on it, and this section obeys that rather than re-deriving a
  friendlier reading.

**What this section used to say.** Before this restatement the paragraph under
the verdict read:

> The blocking gap is not subtle and it is not a matter of polish. This is a
> client for one gateway, and it has never spoken to that gateway. The
> compatibility baseline, the six dispatch result shapes, the reconciliation
> rules, the frame contract, the credential handshake — with the two exceptions
> named at the top of this document, all of it is a careful reading of a source
> tree, checked against a stub built from the same reading. Two response shapes
> out of seventeen methods, observed by a different client in an earlier unit, is
> the whole of the live evidence. That is a good position from which to attach
> for the first time. It is not evidence that attaching works.

Every sentence of it was true when it was written on 2026-08-02. The load-bearing
one — "it has never spoken to that gateway" — stopped being true on 2026-08-04,
when this client attached to a real Hermes dashboard repeatedly and streamed
turns to completion; rows 17 and 18 cite the recordings. The paragraph also
carried a stale denominator, "seventeen methods", where `REQUIRED_METHODS` in
`talaria/domain/compat.py` holds eighteen.

What the build **is** ready for has narrowed rather than widened. It is no longer
"a first attach": the attach has happened and is recorded. It is a client whose
remaining gaps are one demonstration nobody has run, one credential question
nobody has decided, and ten method surfaces nobody has exercised — so a session
you would mind losing is still not what this table supports.

### What would change this verdict

**Re-ordered 2026-08-05 by what is actually left.** The list used to run (1) R2,
(2) R3, (3) R1's remaining half, (4) the matrix, (5) CI, ordered "because each
depends on the one before it", and closed: "Until at least (1) and (2) are done
and recorded here, this document's verdict does not move." Items (1), (2) and (5)
are now done and recorded, and the verdict did not move — because the words "at
least" made (1) and (2) necessary, not sufficient, and rows 13 and 19 are gaps
this list never named. Both facts are kept here on purpose: a list of unblocking
conditions that quietly drops the ones it satisfied is how a reader loses the
ability to tell an item that was met from an item that was never on the list.

**Still open, in the order they now block the verdict:**

1. **Row 19 — F1 and F7 in an isolated live session.** Run a session for the
   purpose, and observe F7 — the gateway still serving — *after* Talaria exits.
   The frame log alone cannot settle it: the log ends when Talaria exits, and the
   observation F7 needs happens after that. Row 11 has F7 against the loopback
   stub only. **Narrowed 2026-08-06 to "execute this document":**
   `docs/plans/2026-08-06-u6-row19-operator-checklist.md` names all six steps,
   what to record for each, and what to hand back. It cannot be run by an agent —
   step 6 needs somebody watching a process list before and after an exit — which
   is why writing it did not move the row.
2. **Row 13 — R1's remaining half.** **The decision this item asked for has been
   taken (2026-08-06), and the item does not close.** It used to read: accept the
   environment-inherited credential as an operator-side choice and document the
   credential-file route, or stop supporting the environment variable. The second
   was chosen. What is left is not another decision; it is the residual that
   choice explicitly did not remove — an inherited variable is readable from the
   process environment by anyone who can read that process, and one supported
   route (a `token` on `TALARIA_GATEWAY_URL`) is still an environment variable.
   Closing the row now means removing that last environment-borne route, which
   needs a way for Hermes to hand a client its session token that does not go
   through the endpoint URL. That is work in Hermes, not in Talaria, and it is out
   of this repository's scope by operator decision.
3. **Row 6 — the ten evidence-only methods with no runtime evidence.** Closing
   them needs live traffic that exercises them; it is work, not re-grading. This
   is the item the old list never had. **Re-confirmed 2026-08-06 by enumeration
   rather than by memory** (§The method enumeration), and the ten are named there
   individually so the next re-grade compares lists rather than counts.
4. **The matrix** — **partly done**, and unchanged since the list was written. A
   second operating system runs the full suite in CI, including the
   pseudo-terminal and process-surface tests. Still missing: a person driving the
   interface on Linux, and at least one real terminal emulator rather than a bare
   pseudo-terminal on either platform (§Platform matrix).

**Done and recorded:**

5. **R2 — one real attach.** **Done 2026-08-04, recorded in row 17.** A Hermes
   dashboard on loopback, `talaria --record`, landing in a session — the
   credential chain, the handshake, the greeting frame, the compatibility check
   against the real dispatcher, and `session.create`, with the frames on disk.

   **This item carried a wrong count and a prediction the measurement has since
   falsified, and marking it done is not a reason to carry either forward.** It
   used to close: "and `session.create` — **five of the twelve inferred surfaces**
   in one step, with the frames on disk." Two errors in one clause. The
   evidence-only set is **thirteen**, not twelve — commit `ec861fa` pinned
   `slash.exec` and took it from twelve to thirteen, sweeping this document's
   counts but missing this sentence. And the attach exercised **three** of those
   thirteen, not the five it predicted: `session.create`, `prompt.submit` and
   `slash.exec` (row 6).
6. **R3 — one real turn.** **Done 2026-08-04, recorded in row 18.** One prompt
   submitted, streamed to completion, and the transcript compared against a
   `talaria replay` of the same frames — byte-identical, which is what turned the
   shared-seam argument into a measured equivalence.

   Both steps were executable with the client as it stood, which they were not
   when this document was first drafted: `LiveSource` accepted a recorder, the
   launcher never passed one, and `talaria record` draws no interface — so
   recording a session and *using* it were mutually exclusive. `--record` on the
   bare launcher is what closed that.
7. **CI** — **done.** The branch was pushed, and all seven checks passed on the
   pull request that carries this document, `install` and Linux among them.

Rows 19, 13 and 6 are each on their own sufficient to hold the verdict at **not
ready** under AE7 and R39. Where the conditions above get recorded when they are
met is this document's evidence table — items (1) and (2) went stale for three
days precisely because nothing pointed the work that cleared them back here.

## Gate record

This block restates the verdict above in a form a test can read.
`tests/docs/test_gating_documents.py` checks it against the prose: the verdict
must appear in a heading of this document, each `blocks-on` line must name a real
evidence-table row whose grade it quotes correctly, and no condition may still be
listed once its row grades it settled. It exists because the three-day staleness
described in the paragraph above had nothing that could notice it — see
[`DECISIONS.md`](../engineering-journal/DECISIONS.md) for the convention and the
alternatives it was chosen over.

`review-by` is the part that fires on its own. Every other check needs somebody to
edit something; a document left alone while the evidence moves trips none of them,
which is precisely what happened here. Re-read the conditions against current
evidence, restate whatever moved, then set a new date.

**Restated 2026-08-06 together with the evidence table, in one edit, because that
is the rule.** Rows 6, 13 and 19 were re-graded above and this block is rewritten
in the same pass; a table restated without its block, or a block moved without its
table, is the drift the check reads for. All three conditions survived that
re-grade, so all three `blocks-on` lines stayed and the verdict stayed **NOT
READY**. `review-by` moves to 2026-09-06 because the re-read that date exists to
force has just happened — the date is moved *by* the re-read, never ahead of it.

**Restated again 2026-08-07, in the same one-edit rule.** Row 19 is now **met**:
the operator ran the acceptance checklist end to end, including the
outside-the-frame-log process sample the row itself named as the one observation
that would settle F7. Its `blocks-on` line is removed and a
`Clears: v0-1-daily-driver#row-19` backlink is written — a live claim, which the
gating check cross-examines against this block rather than taking on trust.

**Restated a third time on 2026-08-07, same one-edit rule, and row 6 is now
`measured`.** It went `inferred` → `measured` in two steps on the same day. The
F2–F6 live-evidence run gained seven methods and left one outstanding; the
secret-bridge probe then provoked that one, `secret.respond`, and a reply-side
pass matched every evidence-only call in the corpus to its reply and compared it
against the pinned shape (§The reply side). Its `blocks-on` line is removed and a
`Clears: v0-1-daily-driver#row-6` backlink is written.

**One condition remains and the verdict does not move.** Row 13 is untouched by
any of this work — R1's environment half is still partially unmet, an inherited
`HERMES_DASHBOARD_SESSION_TOKEN` is still readable for the life of the process,
and by AE7 and R39 one gap alone blocks ready. **NOT READY** stands.

**Read the shape of that honestly, because it is easy to misread in two
directions.** Row 6 was the largest movement this document has recorded — nine
outstanding methods on the morning of 2026-08-07, zero in scope by the evening —
and it is now a cleared row rather than a blocking one. That is real. What it
does not mean is that these methods are proven compatible in full: the grade
rests on top-level key names and value kinds (row 6a), on one or more live
replies per method rather than on exhaustive traffic, and the same measurement
found two of the pinned shapes wrong. A row that clears while its own
measurement is correcting the record is a row worth reading twice.

```gate
id: v0-1-daily-driver
verdict: NOT READY
review-by: 2026-09-06
blocks-on: row-13 partially unmet
```

## Related

- Plan: `docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md`, unit U10
- Compatibility baseline: `talaria/domain/compat.py` (U3 owns the data, U10 owns the check)
- The check: `talaria/transport/compat_check.py`
- Deferred work, including the live-verification item: `docs/engineering-journal/QUEUED.md`
