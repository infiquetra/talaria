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
followed immediately by a streamed turn. And it says nothing about the other ten
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
| 6 | The **thirteen** other required methods are compatible | **inferred** | Never probed at startup. Their evidence is the pinned source line plus the recorded request fixture and response shape in `talaria/domain/compat.py`. Three of the thirteen — `session.create`, `prompt.submit` and `slash.exec` — have been called by Talaria itself against a real Hermes dashboard and answered without error; the other ten have no runtime evidence of any kind. **Re-graded 2026-08-06 by enumerating methods rather than by counting runs** — see §The method enumeration, which reads every outbound frame in the corpus and finds **eight distinct methods of the required eighteen**, unchanged from the 2026-08-05 grade because the model-picker plan's units U1–U6 added no recording to the corpus | `talaria/domain/compat.py`; §The method enumeration |
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
| 19 | **F1, F7** demonstrated live in an isolated session | **unmet** | The row stays unmet, on a narrower reason than it used to carry. **F1 (first run)** has been exercised live, though not in the form this row asks for: over the corpus `talaria-live-corpus-v1-2659f-bd69e537f1d9` (17 recordings, 2,659 frames, construction stated in row 17), 15 of the 17 recordings authenticate against a real Hermes dashboard, have all five read-only startup probes of row 1 answered, and land in a session — the F1 sequence end to end. What is still missing for F1 is an isolated throwaway session run by somebody watching; the authentication-failure and absent-capability branches, which no recording exercises; and the compatibility check's real on-screen output, which was never captured. Only the bare startup path ran — neither `--resume` nor `--session` appears anywhere in the corpus and `session.resume` was never called, so KTD7's precedence chain is unverified live. **F7 (the gateway survives Talaria's exit)** cannot be settled by any frame log, because the log ends at the exit F7 needs somebody to observe; the only F7 evidence in this document is row 11's, against the loopback stub. The corpus does hold something adjacent that stops short of settling it: in ten pairs of adjacent recordings against the same loopback endpoint, the later run's `session.most_recent` returned the exact session identifier the earlier run's `session.create` had produced, eight of them forming one unbroken chain spanning roughly two hours twenty minutes. That proves the endpoint answered again after each Talaria process exited and still held the session — it does not prove Talaria did not stop it, because Hermes persists sessions to disk, so a gateway that died at exit and was restarted by the operator would leave an identical signature. What would settle F7 is one observation taken outside the frame log: the gateway process's PID or start time sampled before Talaria attaches and again after it exits. **This row previously read** "No isolated live session has been run", which was true when written on 2026-08-02 and stopped being an accurate description of what is missing on 2026-08-04. **Re-read 2026-08-06 and unchanged.** The model-picker plan's unit U6 produced the operator checklist that would close this row — `docs/plans/2026-08-06-u6-row19-operator-checklist.md`, six steps, still carrying `status: ready-for-operator` — and no step of it has been run: the recording corpus is byte-for-byte the 2026-08-04 one, its newest frame log started at `2026-08-04T19:43:17.075Z`, and nothing recorded on 2026-08-05 or 2026-08-06 exists. **A checklist written is not a checklist executed**, and grading this row on the existence of the document would be the exact substitution this table's method section exists to refuse | corpus `talaria-live-corpus-v1-2659f-bd69e537f1d9`; row 11 for the stub-only F7 evidence; `docs/plans/2026-08-06-u6-row19-operator-checklist.md` |

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
| `session.resume` | `--resume` / `--session` startup | `methods_session.py:306-699` | **not verified at runtime** |
| `prompt.submit` | submitting a turn | `methods_prompt.py:67-313` | **never invoked by Talaria.** Real top-level response shape observed in U2's live capture; matches the pin |
| `session.interrupt` | cancelling a turn | `methods_session.py:2706-2775` | **not verified at runtime** |
| `subagent.interrupt` | interrupting one child | `methods_session.py:2806-2814` | **not verified at runtime** |
| `slash.exec` | the route an ordinary slash command takes | `methods_tools.py:1073-1211` | **called by Talaria against a real Hermes** — ten calls across the live recordings, all answered |
| `command.dispatch` | the fallback slash route, for what `slash.exec` refuses | `methods_tools.py:432-1071` | **not verified at runtime** |
| `paste.collapse` | collapsing a large paste | `methods_complete.py:14-39` | **not verified at runtime** |
| `approval.respond` | answering an approval | `methods_prompt.py:886-920` | **not verified at runtime** |
| `clarify.respond` | answering a clarification | `methods_prompt.py:858-864` | **not verified at runtime** |
| `secret.respond` | answering a secret bridge | `methods_prompt.py:881-883` | **not verified at runtime** |
| `sudo.respond` | answering a sudo bridge | `methods_prompt.py:876-878` | **not verified at runtime** |
| `terminal.read.respond` | answering the terminal-read bridge | `methods_prompt.py:867-873` | **not verified at runtime** |

**Thirteen of eighteen required methods are never probed at startup**, and ten of
those thirteen have no runtime evidence from anything at all. Three are
exceptions, and what makes them exceptions changed on 2026-08-04: `session.create`,
`prompt.submit` and `slash.exec` were called by **Talaria itself** against a real
Hermes dashboard on loopback, and every one of those calls was answered without an
error. Earlier drafts of this section recorded the first two as response shapes
observed through the TypeScript reference recorder, which was true when written;
Talaria's own frame logs now carry the calls. The other ten have still never left
Talaria.

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

**Never called, by count rather than by recollection — ten of the thirteen:**
`session.resume`, `session.interrupt`, `subagent.interrupt`, `command.dispatch`,
`paste.collapse`, `approval.respond`, `clarify.respond`, `secret.respond`,
`sudo.respond`, `terminal.read.respond`.

**The result of the re-grade is that row 6 does not move, and the reason matters
more than the result.** The model-picker plan sequenced six units — the admin HTTP
surface, the model picker, the credential decision, the profile picker, the
default-model picker, and the scripted live acceptance run — and the natural
reading of "six units of work happened" is that live coverage widened. It did not.
The corpus has not gained a recording since 2026-08-04: its newest frame log's
header reads `2026-08-04T19:43:17.075Z`, and re-deriving the aggregate digest over
the current directory still yields `bd69e537f1d9…`, the same value rows 17 and 18
were written against. Units U1–U5 were built and tested against the loopback stub
and the repository suite; U6, the unit whose product *is* live evidence, produced
a checklist for an operator and stopped there (row 19). **Row 6 therefore stays
`inferred` on the same ten methods it named on 2026-08-05.**

One caveat on what this section can prove, stated because it is easy to read the
table above as broader than it is. An outbound frame proves Talaria *sent* a call,
not that the gateway answered it correctly. Rows 17 and 18 do the reply-side work
for the methods they cover by matching each reply to its call on JSON-RPC `id`;
this section deliberately does not repeat that, because its question is narrower —
*which* methods have any live traffic at all.

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

Reading the table: rows 17 and 18 are **measured**, row 19 is **unmet**, row 13
is **partially unmet**, and row 6 covers thirteen of the eighteen required
gateway methods as **inferred rather than measured** (three of those thirteen —
`session.create`, `prompt.submit` and `slash.exec` — have since been called by
Talaria against a real gateway and answered; the other ten have no runtime
evidence of any kind). Rows 12 and 14 were the two weakest *measured* rows in
earlier drafts — macOS-only, and a CI job that had never run — and both were
closed by pushing this branch. The suite itself fails intermittently, twelve runs
green in thirteen.

AE7 and R39 say the ready verdict is blocked on any gap.

### Talaria v0.1 is **NOT READY** as a daily driver.

**The verdict has not moved. Its reasons have moved entirely.** That distinction
is the point of this restatement: three words that used to rest on "this client
has never attached" now rest on three specific, much narrower gaps, and a reader
who acts on the old reasons would be acting on a fact that expired on 2026-08-04.

The three gaps, in the order the table grades them:

- **Row 19 — F1 and F7 have not been demonstrated live in an isolated session.**
  The table grades this **unmet**, and this section reads that grade rather than
  re-deriving it. What the row asks for is a session run for the purpose, with F7
  — the gateway still serving after Talaria exits — observed *after* that exit.
  Row 11 has F7 only against the loopback stub. **Unchanged on 2026-08-06.** The
  checklist that would close it now exists and is specific
  (`docs/plans/2026-08-06-u6-row19-operator-checklist.md`, six steps); not one of
  its steps has been executed, and the recording corpus is unchanged since
  2026-08-04. This row is now blocked on an operator session nobody has sat down
  and run, rather than on nobody knowing what to run.
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
- **Row 6 — thirteen of the eighteen required gateway methods are inferred
  rather than measured.** Three of the thirteen have now been called live and
  answered. Ten have no runtime evidence of any kind, and what `present` covers
  is narrower than it sounds: `compare_shape` compares top-level keys and value
  kinds only (row 6a). **Re-graded 2026-08-06 by enumeration and unchanged.**
  §The method enumeration counts outbound frames rather than reasoning about
  which runs happened: eight distinct methods of the eighteen appear anywhere in
  the corpus, the same eight as on 2026-08-05, because six units of picker work
  landed without adding one recording to it.

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
table, is the drift the check reads for. All three conditions survive the
re-grade, so all three `blocks-on` lines stay and the verdict stays **NOT READY**.
`review-by` moves to 2026-09-06 because the re-read that date exists to force has
just happened — the date is moved *by* the re-read, never ahead of it.

```gate
id: v0-1-daily-driver
verdict: NOT READY
review-by: 2026-09-06
blocks-on: row-19 unmet
blocks-on: row-13 partially unmet
blocks-on: row-6 inferred
```

## Related

- Plan: `docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md`, unit U10
- Compatibility baseline: `talaria/domain/compat.py` (U3 owns the data, U10 owns the check)
- The check: `talaria/transport/compat_check.py`
- Deferred work, including the live-verification item: `docs/engineering-journal/QUEUED.md`
