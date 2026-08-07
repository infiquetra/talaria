---
title: Row 6 live-evidence run — results (F2 through F6)
type: results
status: complete
date: 2026-08-07
covers: F2, F3, F4, F6; R4, R7, R9, R13, R14, R15, R16, R23, R24
executes: docs/plans/2026-08-07-f2-f6-operator-checklist.md
targets: v0-1-daily-driver#row-6
---

# Row 6 live-evidence run — what it proved and what it did not

## The measurement

Row 6 is graded by enumerating which required gateway methods the recordings
prove Talaria called, never by counting runs. Re-deriving that enumeration over
the whole corpus:

| | before this run | after this run |
|---|---|---|
| distinct outbound methods | 9 | **16** |
| of the nine row 6 still needed | 0 | **7** |

Corpus after the run: `talaria-live-corpus-v1-4467f-4d780a89ee7d` (29
recordings, 4,467 frames). Before: `talaria-live-corpus-v1-2850f-fb227b020866`
(28 recordings, 2,850 frames). Digest and count only, never a path (R11/R29).

**Row 6 does not clear.** Seven of nine is not nine, and the two that did not
land are recorded below with the evidence for *why* rather than as a shrug.

## What landed

| method | provoked by | observed |
|---|---|---|
| `paste.collapse` | a 300-line bracketed paste | composer replaced by a one-line placeholder; the pasted body was **not** re-rendered into the transcript (R13) |
| `session.interrupt` | `F4` during a streaming turn | output cut mid-sentence, transcript reads `[interrupted]` — cancelled, not ended (R4) |
| `approval.respond` | asking the agent to run a destructive shell command | card rendered verbatim, transcript kept streaming behind it, answer `once` accepted and the command genuinely ran |
| `clarify.respond` | asking the agent to use its clarification tool | four-choice card; answer accepted and echoed back into the turn |
| `subagent.interrupt` | three long fan-out sub-agents, then `enter` on one row | that row went `interrupted`, the other two stayed `running`, the count re-read `2 active · 1 finished`, conversation intact (R14/R15/R16) |
| `command.dispatch` | a skill command | fell back after `slash.exec` refused, and the dispatch result rendered |
| `sudo.respond` | a `sudo` command the host requires a password for | bridge fired, Talaria answered, the gateway reported the answer did not authenticate |

### `command.dispatch` was derivable, and the checklist said it was not

The checklist called this one "provocation not established" and advised working
down the catalogue looking for a command that errors. That advice is superseded:
the route is a **two-line derivation from source**, not a search.

`talaria/ui/app.py` calls `command.dispatch` whenever the `slash.exec` RPC comes
back not-`ok` — that is the whole condition. Hermes's own test names the case
that produces it: `test_slash_exec_rejects_skill_commands`, whose docstring
reads "slash.exec must reject skill commands so the TUI falls through to
command.dispatch", asserting error code 4018. So **any skill command** takes the
fallback. First attempt on that route worked.

An *unlisted* command does not: Talaria refuses it before any call with "the
catalogue did not list this command", so `slash.exec` is never reached.

## What did not land

### `terminal.read.respond` — gated on a gateway-side environment flag

Asked directly to call `read_terminal`, the agent ran a tool search and reported
the tool unavailable. **The first explanation for that was wrong and is worth
stating, because it is the more obvious one.** The tool returns "read_terminal
is only available in the Hermes desktop app" when its platform callback is
absent, so an absent callback looks like the answer — but the callback is *not*
absent. `tui_gateway/server.py` wires `read_terminal_callback` into the agent
callbacks for every session, and it reaches the tool through `run_agent.py` →
`agent_init.py` → `tool_executor.py`. Talaria's session had one.

The actual gate is the tool's **registration** check:

```python
def check_read_terminal_requirements() -> bool:
    """Desktop GUI only — HERMES_DESKTOP is set on the gateway the app spawns."""
    return (os.getenv("HERMES_DESKTOP") or "").strip().lower() in ("1", "true", "yes")
```

So the tool is only offered to the model when the **gateway process** has
`HERMES_DESKTOP` set, which happens only on a gateway the Hermes desktop app
spawned. No tool call means no `terminal.read.request`, and no request means
`terminal.read.respond` can never be sent — regardless of what the client is.

**This is a condition, not an impossibility, and the distinction decides how
the gate should treat it.** It is not "a terminal client cannot answer this
bridge": Talaria implements the bridge, answers it without a human overlay
(`UNATTENDED_KINDS` is exactly `{"terminal_read"}`), and would answer it if one
arrived. It is "the request is only emitted by a gateway launched a particular
way, and ADR-0001 makes the gateway something Talaria dials rather than
launches." Setting `HERMES_DESKTOP` on a gateway makes it reachable, and that
is the falsifier for everything in this section.

### `secret.respond` — not attempted, and deliberately

The bridge is wired to the skills tool's secret-capture callback, so provoking
it means installing or configuring a skill that captures a credential on the
operator's own machine. That is a change to their environment rather than a
throwaway session, so it was left for them to decide. The trigger is known; only
the decision to fire it is outstanding.

> **Amended 2026-08-07, later the same day — the paragraph above overstates the
> cost, and the overstatement is what deferred the run.** No credential is
> involved and no skill has to be adopted. `tools/skills_tool.py` fires the
> callback for *any* skill declaring a `required_environment_variables` entry
> not already persisted in `~/.hermes/.env`; what the variable is for is never
> inspected. The gateway's callback branches on the answer before storing
> anything — `val = _block("secret.request", …)` then `if not val:` returns
> `skipped` — so an empty answer never reaches `save_env_value_secure`, and
> `save_env_value_secure` returns `validated: False` in any case, because
> nothing is checked against any service.
>
> **What actually ran.** A throwaway skill declaring one variable that nothing
> reads, loaded once with `skill_view`, answered with an empty field, deleted
> afterwards. The bridge fired, Talaria answered, the gateway replied
> `{"status": "ok"}`, and the frame log carries
> `secret.respond {"request_id": "…", "value": "[redacted]"}` with an explicit
> redaction record — structural, since even an empty value was withheld (R9).
> Nothing was written to `~/.hermes/.env`, confirmed by grep afterwards.
>
> **Why this section is corrected rather than replaced.** "Provoking it changes
> the operator's machine" was written from what such a skill is *for* rather
> than from what the code branches on, and that is a repeatable mistake worth
> leaving legible. It is recorded as a lesson in
> `docs/engineering-journal/LEARNINGS.md`.

## R9: verified, and stronger than the requirement

R9 says a credential-bearing answer must never reach a frame log. Reading every
`*.respond` frame in the corpus:

```
approval.respond {"session_id": "…", "choice": "once"}
clarify.respond  {"request_id": "…", "answer": "[redacted]"}
approval.respond {"session_id": "…", "choice": "deny", "all": true}
sudo.respond     {"request_id": "…", "password": "[redacted]"}
```

The redaction is **structural, not case-by-case**: even the clarification
answer, which was a database name and not sensitive at all, is redacted. A
canary string was typed into the sudo field and grepped for across the whole
corpus — zero files.

## The finding this run exists to produce

**Answering any blocking prompt requires an unknown number of `tab` presses and
gives no visible sign of where focus is.** Every prompt card renders its
controls, but nothing on screen says focus can leave the composer, how far away
the control is, or which control currently has it. Observed first-hand, all in
one session:

1. **An approval expired while its answer was being aimed.** The reply came back
   "the gateway had no approval waiting — nothing was resolved". This happened
   **twice**, once for a single approval and once for a `deny all` across three.
2. **A value meant for a hidden credential field landed in the visible
   composer.** One `tab` from the sudo card put focus on the message box, and
   the typed answer appeared in plain text where a chat message goes. It was a
   canary, so nothing was lost — a real sudo password would have been one
   `enter` away from the transcript.
3. **Two typed answers went nowhere at all**, absorbed by whatever held focus.
4. The tab distance to a control **varied between 3 and 7** in the same session,
   because it depends on what else is on screen.

The focus styling exists — `AgentRow.-interruptible:focus` sets a 20% accent
background, buttons take reverse video — but it is only legible once you already
know which row to look at. Finding a control took an ANSI-level dump of the
screen.

This is an F2 defect of the same shape as the picker's: everything works when
driven correctly, and there is no way for the operator to learn what correct is.
It is queued as P0 in `docs/engineering-journal/QUEUED.md`.

### Two smaller observations from the same run

- **The multi-approval refusal fired for real, and correctly.** Three sub-agents
  each requested approval at once; Talaria refused to answer any of them, saying
  "more than one approval is waiting and this gateway sends no request id with
  an approval, so an answer cannot be aimed at one of them", and offered only
  `deny all`. The frame log confirms the cause: `approval.respond` carries a
  `session_id` and no `request_id`, while `clarify.respond` carries a
  `request_id`. This is RR-27/RR-28 in the field, refusing rather than guessing.
- **An outstanding blocking prompt survives `F4`.** Interrupting the turn left
  the sudo card up and the next submission queued behind it. Whether that is
  right is arguable — the bridge has its own timeout — but it is not documented
  anywhere and it is not obvious from the screen.

## What row 6 still needs

One decision and one question:

1. **`secret.respond`** — the operator's call on whether to configure a
   credential-capturing skill.
2. **`terminal.read.respond`** — whether row 6 should require runtime evidence
   for a bridge whose request is only emitted by a gateway started with
   `HERMES_DESKTOP` set. Talaria implements it and would answer it; what is
   missing is a deployment Talaria does not control. Leaving the row
   permanently short on that is not a measurement, it is a stalemate — but
   re-scoping it out has to name the condition, so the exclusion stays
   falsifiable.

## Both were settled on 2026-08-07, and row 6 cleared

Item 2 was re-scoped out with its condition named — the decision is in
`docs/engineering-journal/DECISIONS.md`. Item 1 turned out to cost a throwaway
skill file and an empty keypress, per the amendment above, and was run the same
day. The corpus stands at `talaria-live-corpus-v1-4670f-fc5790017b70`, 30
recordings, 4,670 frames, and seventeen of the eighteen required methods have
live traffic.

**What closed the row was not the enumeration, though.** Counting outbound
methods says Talaria *called* each one; row 6 asks whether they are compatible.
A reply-side pass matched every evidence-only call in the corpus back to its
reply on JSON-RPC `id` and compared it against the pinned shape using the
production `compare_shape`. Twelve of twelve in-scope methods matched — **after
two pinned shapes were found wrong and corrected**:

- `approval.respond` returns `resolved` as an **int**, a count of approvals
  resolved, and was pinned as a `bool`. Three live replies carried `0`, `1` and
  `0`. The gateway handler returns `resolve_gateway_approval(...)` verbatim,
  typed `-> int` (`tools/approval.py:2490-2505`). `talaria/ui/app.py:527-529`
  already read it as a count, so nothing misbehaved and nothing caught it.
- `session.resume` returns a `messages_omitted` key the baseline never recorded.

Chasing the second one found a defect worth more than either correction:
**Talaria discards the entire conversation history a resume returns.** Verified
on screen — a reply carrying three messages and `messages_omitted: False`
rendered an empty transcript. Queued as P1.

Row 6 is now graded `measured` and the gate blocks on row 13 alone. The full
account is in §The reply side of
`docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`.
