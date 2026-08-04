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

## The finding that decides this document

**Talaria has never connected to a Hermes gateway.** Not once, in any unit of
this build. Every transport test in the repository dials a stub WebSocket
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

Everything below follows from that.

## Evidence table

Status values: **measured**, **inferred**, **unmet**.

| # | What the plan asked for | Status | What was actually done | Where |
|---|---|---|---|---|
| 1 | **R34** — startup verification invokes only KTD9's read-only set | measured | Five read-only methods probed over a real socket; the *server's* received-call record contains those five names and none of the twelve mutating ones | `tests/transport/test_compat_baseline.py::test_no_mutating_method_appears_in_the_startup_call_log` |
| 2 | **AE7** — a missing method is named and blocks ready | measured | Each of the five read-only methods removed in turn (`-32601`, the gateway's own answer at `tui_gateway/server.py:1762`); the report names that method, the other four still pass, `ready` is false | `test_a_missing_method_is_named_and_blocks_ready` |
| 3 | **AE7** — a drifted response shape is flagged | measured | A dropped key, a changed value kind, and an added key each flagged by name against the pinned signature | `test_a_dropped_response_key_is_flagged_by_name` and the two beside it |
| 4 | A probe that never answers must not read as a pass | measured | An unanswered probe grades `unproved` and blocks; a gateway that drops the socket mid-check leaves one named `unproved` row and two `present` rows | `test_a_probe_that_never_comes_back_blocks_rather_than_passing` |
| 5 | The startup probe cannot invoke a mutating method by mistake | measured | The guard is called directly with each of the twelve evidence-only entries and raises before the dispatcher is touched (call count asserted at zero) | `test_probing_an_evidence_only_method_raises_before_any_call` |
| 6 | The **twelve** other required methods are compatible | **inferred** | Never invoked by Talaria. Their evidence is the pinned source line plus the recorded request fixture and response shape in `talaria/domain/compat.py`. Two of the twelve — `session.create` and `prompt.submit` — additionally had their real top-level response shapes observed in U2's live capture and both matched the pin (see above); the other ten have no runtime evidence of any kind | `talaria/domain/compat.py` |
| 6a | What "shape matches" covers | measured | Top-level only: the response's own key set and each value's kind. A gateway whose every *nested* payload had changed was graded `present` with `0 blocking` — deliberate v0.1 scope (`talaria/domain/compat.py:343`), stated here because `present` sounds broader than it is | `talaria/domain/compat.py::compare_shape` |
| 7 | **R36** — a normal exit restores the terminal | measured | The real client run on a pseudo-terminal; `termios` snapshotted before, during and after; attributes after are byte-identical to before. The falsifiability control (`SIGKILL` on the same run) leaves the terminal in raw mode, and that is asserted | `tests/ui/test_teardown.py::test_a_normal_exit_restores_the_terminal_modes`, `::test_the_terminal_restore_assertion_can_fail` |
| 8 | **R36** — an induced mid-stream failure still restores the terminal | measured | A frame source that streams two frames then raises; the app reports it, closes the source and exits 70; the terminal is restored | `::test_an_induced_mid_stream_failure_still_restores_the_terminal` |
| 9 | **R36** — no child process outlives Talaria | measured | The status command backgrounds a ten-minute worker; the worker is verified alive during the run and gone after exit, on both the normal and the failure path | `::test_a_normal_exit_leaves_no_status_child_or_grandchild`, `::test_an_induced_mid_stream_failure_still_stops_the_status_child` |
| 10 | **R36** — local waiters resolve at teardown | measured | A call in flight against a gateway that never answers resolves `unknown` with reason *the transport was closed*, rather than hanging | `::test_a_call_in_flight_at_teardown_resolves_instead_of_hanging` |
| 11 | **F7** — the gateway survives Talaria's exit | measured *against the stub* | Two tests. In process: after teardown a second client dials the same server object and receives the greeting. At process granularity: the stub runs as a **separate OS process, left in Talaria's own process group** so a mis-aimed group signal would kill it, and after teardown it is alive, still accepting connections, and still greeting. The server is the loopback stub, not Hermes | `::test_the_gateway_is_still_serving_after_talaria_exits`, `::test_the_gateway_process_survives_a_talaria_that_shares_its_process_group` |
| 12 | **R1** — argv carries no credential | measured **on macOS and Linux** | A running process built by the real launcher, holding a live credential in memory, inspected through the platform's own facility — `ps -ww` on macOS, `/proc/<pid>/cmdline` on Linux: no token, no `?token=` URL, no endpoint. The Linux half was measured when this branch first reached CI: all five process-surface tests ran and passed on `ubuntu-latest` under Python 3.12 and 3.13 (run `30865814553`). Earlier drafts of this document said the `/proc` branch had never executed, which was true when written | `tests/transport/test_process_surface.py::test_a_running_talarias_command_line_carries_no_credential` |
| 13 | **R1** — the environment carries no credential | **partially unmet — see below** | Talaria adds nothing credential-shaped to its own environment (set comparison against what it was launched with). An **inherited** `HERMES_DASHBOARD_SESSION_TOKEN` remains visible for the process's life and cannot be removed | `::test_talaria_adds_no_credential_of_its_own_to_its_environment`, `::test_the_inherited_credential_is_visible_in_the_process_environment` |
| 14 | **AE10** — a clean-environment install produces a working `talaria` | measured locally **and in CI** | `uv tool install .` into a fresh prefix, then the console script invoked by absolute path under `env -i`: `talaria --help` works. The CI `install` job ran for the first time on this branch's pull request and passed on Python 3.12 and 3.13 (run `30865814553`) | this document, §Install |
| 15 | **R39** — the platform matrix records exactly what was exercised | measured | See §Platform matrix. One operating system, two Python versions, two terminal hosts, one multiplexer | §Platform matrix |
| 16 | The launcher runs end to end — attach, probe, open, render, exit | measured *against the stub* | The real console script (`python -m talaria.cli`, no arguments) on a pseudo-terminal against the loopback stub: one connection accepted, the five read-only probes and no mutating method among them, exactly one `session.create`, tens of kilobytes of interface drawn, `ctrl+q` → exit 0, terminal restored | §Launcher run |
| 17 | **R2** — live startup acceptance against a running gateway | **unmet** | The KTD7 precedence chain resolves into a real `session.create` / `session.resume` call and the launcher completes that sequence — against the stub. No Hermes gateway has answered one | `tests/transport/test_session_startup.py` |
| 18 | **R3** — one live turn streamed to completion, compared against replay | **unmet** | Nothing was submitted to a Hermes session. The replay-versus-live equivalence claim rests on the shared frame-source seam, not on a compared transcript | — |
| 19 | **F1, F7** demonstrated live in an isolated session | **unmet** | No isolated live session has been run | — |

## The method table

Seventeen gateway methods are required. Five can be checked at startup without
side effects; twelve cannot, and are not.

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
| `command.dispatch` | generic slash dispatch | `methods_tools.py:432-1071` | **not verified at runtime** |
| `paste.collapse` | collapsing a large paste | `methods_complete.py:14-39` | **not verified at runtime** |
| `approval.respond` | answering an approval | `methods_prompt.py:886-920` | **not verified at runtime** |
| `clarify.respond` | answering a clarification | `methods_prompt.py:858-864` | **not verified at runtime** |
| `secret.respond` | answering a secret bridge | `methods_prompt.py:881-883` | **not verified at runtime** |
| `sudo.respond` | answering a sudo bridge | `methods_prompt.py:876-878` | **not verified at runtime** |
| `terminal.read.respond` | answering the terminal-read bridge | `methods_prompt.py:867-873` | **not verified at runtime** |

**Twelve of seventeen required methods have never been called by Talaria**, and
ten of those twelve have no runtime evidence from anything at all. The two
exceptions are named above: `session.create` and `prompt.submit` were answered by
a real Hermes dashboard during U2's capture, and their top-level response shapes
match the pin. That is evidence about *Hermes*, not about Talaria's transport —
the client that made those calls was the TypeScript reference recorder.

The startup check states the gap on every run rather than reporting
"compatible": its first line reads `gateway compatibility: 0 blocking, 12
unverified at runtime (evidence-only, R34), baseline 7f4d15515`. The count in
that line is derived from the verdicts, not written into a string, because the
same arithmetic was wrong in three places in an earlier draft of this document
and of `talaria/transport/compat_check.py`.

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
| macOS 26.5.2 (build 25F84) | arm64 | 3.12.11 | headless (no tty) | none | `ruff`, `mypy --strict`, `pytest` (×13), `bandit`, `git diff --check` | `ruff`/`mypy`/`bandit`/`git diff --check` clean; `pytest` **12 of 13 runs** `874 passed, 1 skipped`, 1 run one test failed — see §Test-suite honesty |
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

## Test-suite honesty

- **The suite is not reliably green, and that is a finding rather than a
  footnote.** Eight consecutive full runs on Python 3.12 during closeout: seven
  green, one red. The failure is always the same test —
  `tests/ui/test_prompts.py::test_a_card_mounting_into_a_full_region_is_still_recomputed`,
  a U8-era prompt-geometry test that this unit did not touch — and it is not
  reproducible in isolation: 20 consecutive runs of that test alone, on a
  deliberately saturated machine, all passed. The adversarial review measured two
  failures in five on the pre-closeout tree, so this is not a one-off. Five
  further full runs after closeout were all green, which neither confirms nor
  refutes a failure at that rate — at roughly one in eight, five clean runs are
  the more likely outcome. **Thirteen post-closeout runs are on record: twelve
  green, one red.** Filed as a P1 in `docs/engineering-journal/QUEUED.md` with the
  reproduction and with the mechanism labelled as the hypothesis it is. It was
  **not** adjusted until it passed; a red test whose cause is unknown is worth
  more than a green one that was tuned.
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

Reading the table: rows 17, 18 and 19 are **unmet**, row 13 is **partially
unmet**, and row 6 covers twelve of seventeen required gateway methods as
**inferred rather than measured** (two of those twelve with a real response shape
observed and matching, ten with nothing). Rows 12 and 14 were the two weakest
*measured* rows in earlier drafts — macOS-only, and a CI job that had never run —
and both were closed by pushing this branch; that is worth noting because it is
the only kind of gap on this list that closes without a Hermes gateway. The suite
itself fails intermittently, twelve runs green in thirteen.

AE7 and R39 say the ready verdict is blocked on any gap.

### Talaria v0.1 is **NOT READY** as a daily driver.

The blocking gap is not subtle and it is not a matter of polish. This is a
client for one gateway, and it has never spoken to that gateway. The compatibility
baseline, the six dispatch result shapes, the reconciliation rules, the frame
contract, the credential handshake — with the two exceptions named at the top of
this document, all of it is a careful reading of a source tree, checked against a
stub built from the same reading. Two response shapes out of seventeen methods,
observed by a different client in an earlier unit, is the whole of the live
evidence. That is a good position from which to attach for the first time. It is
not evidence that attaching works.

What the build **is** ready for: an isolated first attach against a throwaway
Hermes session, run by somebody watching it, with `--record` on.

### What would change this verdict

In order, because each depends on the one before it:

1. **R2 — one real attach.** Start a Hermes dashboard on loopback, run
   `talaria --record`, and land in a session. This exercises the credential
   chain, the handshake, the greeting frame, the compatibility check against the
   real dispatcher, and `session.create` — five of the twelve inferred surfaces
   in one step, with the frames on disk.
2. **R3 — one real turn.** Submit one prompt, let it stream to completion, then
   `talaria replay` the recording and compare the two transcripts. This is what
   turns the shared-seam argument into a measured equivalence.

   Both of those steps are executable with the client as it stands, which they
   were not when this document was first drafted: `LiveSource` accepted a
   recorder, the launcher never passed one, and `talaria record` draws no
   interface — so recording a session and *using* it were mutually exclusive.
   `--record` on the bare launcher is what closed that.
3. **R1's remaining half** — either accept the environment-inherited credential
   as an operator-side choice and document the credential-file route as the
   supported one, or stop supporting the environment variable.
4. **The matrix** — **partly done.** A second operating system now runs the full
   suite in CI, including the pseudo-terminal and process-surface tests. What is
   still missing is a person driving the interface on Linux, and at least one real
   terminal emulator rather than a bare pseudo-terminal on either platform.
5. **CI** — **done.** The branch was pushed, and all seven checks passed on the
   pull request that carries this document, `install` and Linux among them.

Until at least (1) and (2) are done and recorded here, this document's verdict
does not move.

## Related

- Plan: `docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md`, unit U10
- Compatibility baseline: `talaria/domain/compat.py` (U3 owns the data, U10 owns the check)
- The check: `talaria/transport/compat_check.py`
- Deferred work, including the live-verification item: `docs/engineering-journal/QUEUED.md`
