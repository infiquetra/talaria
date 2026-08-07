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

### `terminal.read.respond` — the tool reports itself unavailable

Asked directly to call `read_terminal`, the agent ran a tool search and reported
the tool unavailable. The tool's own module says why: it returns "read_terminal
is only available in the Hermes desktop app" whenever the platform callback is
absent, and its docstring explains that the terminal buffer "lives in the
desktop renderer (xterm.js)". A terminal-UI client is not that renderer.

**This may be unreachable by construction from Talaria**, which would make it
the wrong thing for row 6 to require of a TUI at all. That is a claim about the
gate's own list and it is not settled here — what is settled is that the agent
in this session could not call the tool.

### `secret.respond` — not attempted, and deliberately

The bridge is wired to the skills tool's secret-capture callback, so provoking
it means installing or configuring a skill that captures a credential on the
operator's own machine. That is a change to their environment rather than a
throwaway session, so it was left for them to decide. The trigger is known; only
the decision to fire it is outstanding.

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
2. **`terminal.read.respond`** — whether a terminal client can reach it at all.
   If it cannot, the honest fix is to re-scope row 6's required list rather than
   to leave the row permanently short by one method that only a desktop
   renderer can answer.
