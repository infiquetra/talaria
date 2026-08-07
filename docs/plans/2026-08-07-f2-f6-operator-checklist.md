---
title: Operator checklist — the five flows the gate has never measured (F2–F6)
type: checklist
status: ready-for-operator
date: 2026-08-07
covers: F2, F3, F4, F5, F6; R4, R7, R8, R9, R11, R13, R14, R15, R23, R24, R30, R31, R40
targets: v0-1-daily-driver#row-6
---

# Operator checklist — F2 through F6, live

## Why this document exists

The v0.1 gate has nineteen evidence rows and **five of the seven flows have no
row at all**. F2 (blocking prompts), F3 (delegation), F4 (large paste), F5
(replay) and F6 (transport loss) are implemented and unit-tested — `ui/prompts.py`,
`ui/agents.py`, `tests/transport/test_paste_collapse.py`, `tests/replay/`,
`tests/transport/test_reconnect.py` — and **nothing in the gate can fail because
one of them is unusable.** The gate measures whether the machine does the right
thing on the wire. It does not measure whether a person can use the result.

That gap is not theoretical. The model picker passed every automated check in
the repository and was unusable the first time an operator opened it; the
row-19 acceptance run — one person driving the interface for an hour — produced
that finding *and* two genuine defects nothing else had caught.

## What this closes, exactly

**Row 6 — and it is reachable in one run.** Row 6 is `inferred` because nine of
the thirteen evidence-only gateway methods have never been called against a real
Hermes. Enumerated from `talaria/domain/compat.py` rather than from memory, the
nine are below, and **every one of them is reachable through F2–F6**:

| method | requirement | step below |
|---|---|---|
| `approval.respond` | R7 | 1 |
| `clarify.respond` | R7, R8 | 2 |
| `secret.respond` | R7, R9 | 3 |
| `sudo.respond` | R7, R9 | 4 |
| `terminal.read.respond` | R7, KTD1 | 5 |
| `session.interrupt` | R4 | 6 |
| `subagent.interrupt` | R15 | 7 |
| `paste.collapse` | R13 | 8 |
| `command.dispatch` | R23, R24 | 9 |

The four already carrying live evidence — `session.create`, `session.resume`,
`prompt.submit`, `slash.exec` — are not re-tested here; they were settled by the
row-19 run.

**If every step below lands, row 6 clears** and the v0.1 gate blocks on row 13
alone, which is work in Hermes and out of this repository's scope by operator
decision. That would be the whole gate down to one blocker that cannot be closed
here.

**A falsifiability note, because a run where everything improves is the failure
mode.** Steps 3, 4, 5 and 9 have **no established way to provoke them** (said
plainly below). It is entirely possible this run closes five or six of the nine
and not all nine. Recording which ones could not be reached, and why, is a
result — not a gap in the run.

## Preconditions

- A live Hermes gateway reachable over the transport, not a stub.
- Talaria built from the repository virtualenv. `talaria` on `PATH` resolves to
  a stale frozen scaffold and must not be used.
- `talaria refresh-credential` available: a Hermes restart invalidates the token
  in `~/.talaria/credentials`, and several steps below restart things.
- A throwaway session. Steps 1–7 submit real prompts to a real agent.
- Every step run under `talaria --record`.

## The rule this checklist is written under

**"Unreachable" and "does not exist" are claims, and they need the same standard
of evidence as a positive result.** This is written at the top because it is the
rule the last run broke twice: a plain HTTP `GET` against a WebSocket path was
read as "no route here", and a blank screen was read as "the feature is
missing" when the code said it renders only on a gap. Reaching for a stub to
make a branch go green is the visible failure; quietly concluding a branch
cannot be reached is the same error with no artifact to review.

So for any step below that does not land: record what was tried, what was
observed, and what would settle it — not "not reachable".

---

## F2 — blocking prompts (steps 1–5)

The flow: a human-facing prompt appears in place, stays tied to its request
until answered or expired, and a late answer cannot resolve a newer request
(R8). Five bridges, five methods, and they are the largest single block of
row 6's remainder.

### Step 1 — approval (`approval.respond`)

**Provoke:** ask the agent to run something the gateway classifies as dangerous
— a destructive shell command against a throwaway path is the usual trigger.
**Record:** the approval card verbatim; that the transcript above it kept
streaming; what was sent on answering. **Watch for:** `approval.request` carries
no `request_id` at all (catalogue rules RR-27/RR-28), so Talaria synthesises a
counting key per session — confirm the answer lands on the right request when
two approvals are outstanding, which is the case the synthesised key is weakest
for.

### Step 2 — clarification (`clarify.respond`)

**Provoke:** give the agent a genuinely ambiguous instruction.
**Record:** the prompt, the answer sent, and that the transcript shows the
exchange afterwards. **Also check:** the multi-select hint (queued as P2, likely
unhonoured) — record what the gateway sent and what Talaria drew.

### Step 3 — secret (`secret.respond`) — provocation not established

**What to try:** ask the agent for a task needing a credential it does not hold.
**If it cannot be provoked, say so and stop** — do not stub it.
**R9 is the sharp edge here:** the answer must never reach the frame log,
transcript export, diagnostic record or status payload. After answering,
`grep` the recording for the value you typed. Finding it is a P0.

### Step 4 — sudo (`sudo.respond`) — provocation not established

**What to try:** a task requiring a privileged operation.
**Same R9 check as step 3, and the same instruction if it cannot be provoked.**
**Note:** Talaria cannot *decline* a bridge — it has to wait out the 120-second
timeout (queued as P1). If you provoke this and do not want to answer, that wait
is the current behaviour, and confirming it is worth recording.

### Step 5 — terminal read (`terminal.read.respond`) — provocation not established

**What to try:** this one is gateway-initiated and asks Talaria for a serialized
terminal view; the agent requesting to see the terminal is the trigger.
**Record:** that **no human overlay appeared** — the flow specifies this returns
through the correlation boundary without prompting anybody (F2). A human-facing
card here is a defect, not a pass.
**Also:** the bridge serves un-defanged bytes while claiming to describe a
defanged screen (queued as P3). If you reach this, that queued item becomes
checkable.

---

## Step 6 — cancelling a turn in flight (`session.interrupt`, R4)

**Provoke:** submit something long-running, then press `F4`.
**Record:** that the transcript says the turn was **cancelled** and not that it
ended, and that a late completion event arriving afterwards does not overwrite
that terminal state. The second half is the whole requirement and the easier
half to skip.

## F3 — delegation (step 7)

### Step 7 — interrupting one sub-agent (`subagent.interrupt`, R15)

**Provoke:** ask for work that fans out to sub-agents.
**Record:** rows for delegated work appearing *while the parent is still
streaming* (R14); interrupting one without leaving the conversation (R15); the
count remaining visible when detail is collapsed (R16); and that a terminal row
stays terminal when a late live event arrives.
**Note:** `spawn_tree.list` and `delegation.status` are already probed at startup
and answer `present` — this step is about what Talaria *draws* from them, which
no probe covers.

## F4 — large paste (step 8)

### Step 8 — collapsing a paste (`paste.collapse`, R13)

**Provoke:** paste several hundred lines into the composer in one go.
**Record:** that the framework delivered it as a single paste event, that a
one-line placeholder replaced the text, and that the pasted content is **not**
re-rendered into the transcript.
**Then force the failure:** with collapse unavailable, the original text must
stay editable in the composer and nothing partial may be submitted. A pass on
the happy path alone does not settle R13 — the failure branch is half the
requirement.

## Step 9 — the fallback slash route (`command.dispatch`, R23/R24) — provocation not established

Talaria sends ordinary commands over `slash.exec` and reaches `command.dispatch`
only for what that handler refuses. **Which commands those are is not derivable
from source**: `command.dispatch` serves quick commands, plugin commands, skill
bundles, skill commands and twelve hardcoded name groups, and three of those
five sets are assembled at runtime from the operator's own config and installed
skills (`talaria/domain/commands.py`).
**What to try:** work down the catalogue looking for a command that errors on
`slash.exec` and succeeds on the fallback. Record which command did it.
**If none can be found, that is the result** — record the commands tried.

## F5 — replay (step 10)

### Step 10 — replaying with no gateway

Not a row-6 step: replay calls no gateway method by construction. It is here
because F5 has no gate row and one requirement in it (R40) is a *safety*
property nothing else checks.

**Run:** replay one of this run's own recordings with **no gateway running at
all**.
**Record:** the full interface and the status projection rendering from the
file; pause and speed control working (F8/F9/F10); and — the R40 half — that a
control which would mutate Hermes is **visibly inert or simulated**. Try to fire
one and record what happened.

## F6 — transport loss (step 11)

### Step 11 — killing the socket mid-turn

**Provoke:** restart the Hermes dashboard while a turn is streaming, and again
while a request is awaiting a response.
**Record:** that Talaria marks the disconnect; that it does **not** claim an
unknown RPC outcome as either success or failure; that it reconnects; and that
the focused session, transcript, sub-agents and outstanding prompts reconcile
**without duplication**.
**Note:** the restart invalidates the credential — `talaria refresh-credential`
before continuing. This step also exercises the connection-epoch guard (KTD4):
a picker listing held from before the restart must be refused on selection.

---

## What to hand back

- The corpus for the run: sha256 digest and frame count only, never a local path
  (R11/R29). One digest per recording in the `talaria-live-v1-<n>f-<hash>`
  namespace; the aggregate in `talaria-live-corpus-v1-<n>f-<hash>`. **The two
  namespaces are not interchangeable.**
- **Which of the nine methods each recording proves were called**, by name.
  Row 6 is graded by enumerating methods, never by counting runs — a run where
  every row improves is the failure mode this grading exists to catch.
- Literal on-screen text for every prompt card in steps 1–5.
- For steps 3, 4, 5 and 9: whether it was provoked at all, and if not, what was
  tried.
- For steps 3 and 4: the result of grepping the recording for the answered
  value. R9 says it must not be there.
- No operator profile names, paths, or private operational context in anything
  handed back — this is a public repository (R12).

## After the evidence is in hand

Re-grade row 6 by comparing the method list above against what the recordings
prove, and restate the evidence table and the gate block together — the gating
test (`tests/docs/test_gating_documents.py`) fails if they disagree. If row 6
clears, record the backlink in the notation that test reads: the word `Clears`,
a colon, then the gate identifier, `#`, and the row.

**Do not write that backlink into this document, or into any document, until
the row has actually cleared.** Drafting this checklist did exactly that by
accident — the sentence above originally spelled the backlink out as an example
and `test_no_backlink_claims_to_clear_a_condition_its_gate_still_blocks_on`
failed on it, correctly: a document claimed row 6 was cleared while the gate
still blocked on it. That is DRIFT-04's shape, caught in the seconds it took to
run the suite rather than a day later, which is the entire reason that check was
written.

Then propose the rows F2–F6 should have had all along, so the next interface
regression has something it can fail.
