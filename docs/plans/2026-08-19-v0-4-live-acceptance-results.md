# v0.4 live acceptance — checklist, evidence skeleton, and the agent-driven run

**Status: staged for the operator.** This document is the U9 deliverable in two halves. The checklist
and evidence skeleton below are complete and ready to drive. The agent-driven run's results are
recorded in their own section and are **labelled agent-driven throughout** — they do not substitute
for the operator's own drive, which AE10 names explicitly and which stays reserved.

## What U9 is for

Ten of v0.4's eleven acceptance expectations are verifiable without a gateway, and all ten have
verifying tests. **AE10 is the one no test can close** — it is the operator-driven live leg, and v0.3
shipped with no unit gated on a live drive. This release does not repeat that.

### Acceptance traceability

Coverage is by property rather than by name — the expectations are not greppable by number — so the
mapping is recorded here, where it is read beside the leg that completes it.

| | Expectation, in short | Verified by |
| --- | --- | --- |
| AE1 | a background event updates its row, the focused transcript does not change, an identity-less event is counted and creates no row | `tests/domain/test_registry.py`, `tests/ui/test_prompts.py` |
| AE2 | head-of-queue aiming; an ambiguous outcome settles and latches rather than restoring | `tests/domain/test_needs_you_queue.py`, `tests/ui/test_poll_cadence.py` |
| AE3 | an absent seam is named; a parameterized failure is re-asked bare before absence is claimed | `tests/transport/test_seam_probes.py`, `tests/domain/test_probe_replay.py` |
| AE4 | every derived value renders stale-since with source and age; never-observed is not zero | `tests/ui/test_prompts.py`, `tests/ui/test_needs_you.py` |
| AE5 | requested-with-age indefinitely; a late confirmation resolves exactly once | `tests/domain/test_projection.py` |
| AE6 | rapid focus switching leaks no withdrawn-approval hedge and disarms no bookkeeping | `tests/ui/test_focus_churn.py`, `tests/ui/test_prompts.py` |
| AE7 | no sensitive value reaches any row, log or payload; hostile text renders literally | `tests/ui/test_needs_you.py`, `tests/recorder/`, `tests/transport/test_bridges.py` |
| AE8 | two replays of one multi-session recording agree at every checkpoint, ages included | `tests/replay/test_fleet_replay.py` |
| AE9 | the summary occupies space reserved since first mount and moves no widget | `tests/ui/test_needs_you.py` |
| **AE10** | **a real background prompt, discovered, opened, answered, and the clear observed** | **this run** |
| AE11 | every queue kind reaches a keyboard-reachable end; no path ends silently | `tests/domain/test_needs_you_queue.py`, `tests/transport/test_bridges.py` |

## Safety envelope — binding on any drive, agent or operator

1. **Throwaway sessions only.** Never the operator's working sessions. Every session this run creates
   is created by it and closed by it.
2. **Canary approvals.** The approval leg uses a command that grants nothing and changes nothing —
   a marker file under a temporary directory, never a real operation.
3. **Sudo and secret legs are navigated and declined.** No live secret is ever typed. The point of
   those legs is that the caret reaches the control, not that a value is supplied.
4. **Recordings are redaction-checked before they are cited**, and no frame log is committed (R29).
5. **A failing leg stops the run** rather than being improvised around.
6. **The run closes what it opened.**
7. **Hermes itself is never patched, configured, or restarted.** If the gateway is unavailable, the
   run halts and says so — that is a prerequisite, not an obstacle to route around.

## Prerequisites

| | Requirement | How to check |
| --- | --- | --- |
| P1 | Talaria built from **this repository's** virtual environment, not a globally installed copy | `uv run talaria --version` from the repo root |
| P2 | A reachable gateway, with the endpoint the credential names | the credential file's `url`, and a listener on that port |
| P3 | A credential valid for the **currently running** gateway process | a restart invalidates it; `uv run talaria refresh-credential` re-mints |
| P4 | For the cross-profile leg: a **second** configured and reachable profile endpoint with its own paired credential | `/profiles` in-app; `refresh-credential --profile <name>` |

**P4 is the leg most likely to be unavailable**, and U1 anticipated that: with only one paired profile,
the cross-profile leg is deferred and the run says so rather than simulating it. Under PC1's staged
scope, two concurrent sessions of one gateway satisfy AE10's substance.

## The checklist

Each step names what must be **observed**, not merely what must be done — a step whose evidence is
"it did not error" has not been witnessed.

- [ ] **1. Provenance.** Record the Talaria version and commit, and that the binary is the repo's.
- [ ] **2. Connect.** Start Talaria against the gateway. Observe: the connection reports up, and the
      needs-you row is present and reserved from first mount.
- [ ] **3. Create two throwaway sessions.** Observe: both appear in the session picker with distinct
      durable ids.
- [ ] **4. Raise a background prompt.** In the session that is *not* focused, issue a canary approval
      request. Observe: the needs-you summary count increases without the focused transcript changing.
- [ ] **5. Discover it from the summary.** Observe: the bar names the count, the oldest item's source,
      its age, and its session — in that order, with the age and source before the variable-width
      title.
- [ ] **6. Open the drill-down.** `/needs`. Observe: the item is listed with its kind, wait age,
      source and session.
- [ ] **7. Answer it.** Inline for an approval — explicitly, never an empty choice. Observe: the
      answer is sent, the row renders requested-with-age, and it does **not** optimistically clear.
- [ ] **8. Observe the confirming clear.** Observe: after the gateway confirms, the item leaves the
      queue in the same render boundary, and the summary returns to its prior state.
- [ ] **9. Navigate rather than answer.** For a second item of another kind, press enter to travel to
      its session. Observe: the caret lands on the card's control (`focus_first_unanswered`).
- [ ] **10. The steal leg.** Attempt to activate a session another client is driving. Observe: the
      dialog appears; cancel; then confirm, and watch the other client lose the session.
- [ ] **11. Cross-profile (P4 permitting).** Repeat steps 4–8 with the waiting session on a *second*
      connection. Observe: the item is attributed to that connection, and answering it does not move
      home unless the navigation path says it did.
- [ ] **12. Close what was opened.** Observe: every session this run created is gone from the picker.

## Evidence — agent-driven run

**Everything in this section was produced by an agent, not by the operator.** It is recorded to
de-risk the operator's own drive — to prove the harness works, to surface prerequisites that fail, and
to leave a known-good path — and it is **not** an AE10 pass. AE10 says "driven by hand"; this was not.

### Verdict: **AE10 NOT satisfied.** The run was halted at step 4 by two prerequisite failures.

Both are properties of the gateway this run had available, not defects in Talaria — and the envelope
says a failing leg stops the run rather than being improvised around. They are recorded first because
**the operator's own drive will hit both**, and knowing that before starting is the main value this
run produced.

**Blocker 1 — `approval.pending` is absent on the running gateway process.** Talaria's own seam board
said so on the first screen, unprompted:

> `approval-detail: absent — approval detail unavailable: approval.pending absent — waiting rows are
> shown without their prompts: unknown method: approval.pending`

This is exactly the state U1 measured and the compatibility catalogue documents: the *checkout*
registers the method, the *serving process* does not. The consequence for AE10 is total rather than
partial — a background session's approval can only become **answerable** through feed B, and feed B is
`approval.pending`. Without it a foreign wait is visible as an unanswerable roster row and nothing
more. **AE10's "discovered from the summary, opened in the drill-down, and answered" cannot happen on
a gateway lacking this method, by anyone, agent or operator.**

**Blocker 2 — the session raised no approval to answer.** The canary prompt asked the agent to request
approval for a single harmless command. The transcript shows it *executed* the command instead:

> `⏺ terminal touch <canary path>` followed by `⏺ terminal ✓`

Confirmed on disk: the canary file existed afterwards and was removed by this run. So the session's
approval mode does not gate terminal commands, and no approval was ever raised. Changing that is a
gateway-side session setting; this run did not attempt it, because the standing rule is that Hermes is
never configured by an agent.

### What the run did witness, and it is not nothing

Each of these is a live observation against a real gateway, and each corresponds to an expectation
that previously had only test evidence.

| Observed live | Bearing |
| --- | --- |
| The needs-you row was present and one row high from first mount, before anything had been asked | AE9, live rather than in a headless harness |
| The empty state read `needs-you: none seen · 1 notice: part of the fleet could not be asked` — hedged, never a bare "none" | R14: an empty queue never means "we could not ask" |
| The absent seam was named with its disabled feature *and* its consequence for the queue | AE3, live |
| Sessions never polled rendered `never observed · no lifecycle poll yet` — not idle, not zero, not stale | AE4, live, and the exact distinction that expectation turns on |
| Rendered ages froze when no frames arrived, rather than drifting with the wall clock | R20/KTD12, live — ages come from the frame clock |
| A throwaway session was created on launch and marked current in the picker, with every other row untouched | the safety envelope, honoured and observable |
| The composer, submit and transcript path worked end to end against the real gateway | the seam this release is built on |

### What this run leaves for the operator

1. **A stray throwaway session** created by this run remains on the gateway — untitled, created
   2026-08-19, no useful content. Its identifier is in the driving session's transcript; removing it
   needs a gateway-side action this run did not take.
2. **A gateway with `approval.pending` registered** is a hard prerequisite for AE10. The checkout has
   it and the serving process does not, so satisfying AE10 needs the serving process to be running the
   revision that registers it — an operator action, and the one this run is forbidden to take.
3. **A session whose approval mode gates terminal commands**, or another way to raise a real approval.

### Provenance

Driven from this repository's virtual environment against the endpoint the credential names, through a
terminal pane in the project workspace. The version string reads `0.3.0` because the version has not
been bumped for this release; the code under it is this branch. **Every line in this section was
produced by an agent.** AE10 says "driven by hand". This was not, and does not satisfy it.

## Evidence — operator-driven run

_Reserved. AE10 is satisfied here and nowhere else._
