---
title: U8 live acceptance checklist — R1 through R7
type: checklist
status: executed-pass
date: 2026-08-09
covers: R1, R2, R3, R4, R5, R6, R7, R12
targets: docs/plans/2026-08-08-talaria-v0-2-answerability-and-session-story-plan.md#u8-the-live-acceptance-run
---

# U8 live acceptance checklist — R1 through R7

Authored per the plan's instruction that "the checklist authored first names the expected
observation per leg" (U8 scope). This document names the expected observation for each leg. Its
first pass on 2026-08-09 did **not** drive live — see "Why this run did not proceed" below, kept as
the record of that pass. Later the same day, with both blockers cleared (U1–U7 merged via pull
request #45, a throwaway pane created fresh by the driver), every leg was driven live and passed:
see `docs/plans/2026-08-09-u8-live-acceptance-results.md`.

## Preconditions (per the plan: "With every prior unit merged")

| precondition | required by | status observed 2026-08-09 |
|---|---|---|
| U1–U7 diffs merged to `main` via PR round, each carrying its KTD10 cross-engine review (R11) | U8 scope, R11 | **not met** — `main` at commit `a0b0677` carries no U1–U7 commits; the R1–R7 implementation exists only as an uncommitted working-tree diff (24 modified files, 5 untracked files, ~3,440 insertions) with no commit, no PR, and no recorded Codex review |
| A designated throwaway testing pane, its machine-local identity injected from saga state | U8 scope, safety envelope | **not received** — this invocation's task message names no pane id; `herdr pane list` enumerates 20+ panes across unrelated repos and no pane is marked as the v0.2 testing pane |
| `talaria --record` corpus / redaction tooling reachable | R12, R29 discipline | not checked — moot while the above two are unmet |

## Legs and expected observations

Each row names the requirement, the drive (keys sent, screen read), and the observation that would
count as a pass. `herdr pane send-keys` / `send-text` drives the pane; `herdr pane read --source
recent-unwrapped` reads it back — `visible` is known stale for this purpose (see `talaria`
project memory, "herdr pane read: `visible` is stale").

1. **R1 — F1 jump reaches the first unanswered prompt's control.** Provoke a prompt (approval or
   clarify) on a throwaway session with an agent row on screen, then send `F1`. Expected: caret
   moves to the first unanswered card's control in one keypress; the card shows a hint line naming
   its operating keys, matching `talaria/ui/prompts.py`'s hint convention.
2. **R2 — Focused control is legible.** With focus on a prompt control (from leg 1), read the
   screen. Expected: the focused card is visually distinguished against the default terminal theme
   by more than reverse video alone — a card-level tint, not just the widget's own focus ring.
3. **R3 — Decline via escape.** With a sudo prompt outstanding and focused, send `escape`. Expected:
   the card clears from the transcript; the wire message sent is the sudo kind's empty field value
   (never the approval `deny` shape) — confirmed from the recording, not the screen, since the wire
   payload is not visible on screen. **No real credential is typed on this leg; the leg exists to
   verify the decline path, not to complete the sudo prompt.**
4. **R4 — Confirmed interrupt sweeps outstanding prompts.** With a prompt outstanding on the focused
   session, send `F4` and confirm the interrupt. Expected: the outstanding card clears once the
   interrupt confirms (not before), and nothing new queues behind it for that turn.
5. **R5 — Status region names caret location; no height change.** Walk focus through transcript,
   prompt card, and composer in turn, reading the status region after each move. Expected: the
   status region names the current holder each time; no widget's height changes and no row is
   added to or removed from the `#body` stack across any of the three focus states.
6. **R6 — `--resume` renders history.** Exit the throwaway session, then relaunch with `--resume`
   (or `--session <id>`) against the same session. Expected: the prior turns render as committed
   transcript entries before any new live event; if `messages_omitted` is true, the transcript names
   the omission explicitly using `message_count`, not a silently short history.
7. **R7 — `/sessions` picker switches and renders.** With two throwaway sessions live, open
   `/sessions`, pick the session not currently focused. Expected: the switch lands through
   `session.resume` → `_land_session` (same path as startup), the switched-to session's history
   renders per leg 6's expectation, and the picker's focused-session highlight matches the durable
   session identity, not the runtime id.

## Safety envelope (mandatory, unchanged from the plan)

- Throwaway sessions only, created for this run — never the operator's working sessions.
- No real credential is ever typed: the sudo leg (3) exists to *decline*; any leg that does answer
  uses a canary command that grants nothing.
- Recordings are redaction-checked before they are cited by digest.
- The operator is present for the whole drive.
- A failing leg stops the run rather than improvising a workaround.
- The run closes whatever it opened (sessions, panes, recordings left in a known state).

## Why this run did not proceed

Two preconditions failed the check above before any key was sent into any pane:

1. **The code under test is not the thing the plan describes testing.** R12 and this unit's scope
   both gate the live drive on "every prior unit merged" — a PR-reviewed, `main`-committed state.
   What exists in the working tree on 2026-08-09 is an uncommitted diff implementing the R1–R7
   behavior (confirmed present: `F1` bound to `action_jump_to_prompt` in `talaria/ui/app.py:701`,
   `PromptCard.focus_answer` extended in `talaria/ui/prompts.py:851`, `seed_history` present in
   `talaria/domain/state.py:472`). The headless suite for two of the seven legs passes against this
   tree (`uv run pytest tests/ui/test_focus_returns.py tests/ui/test_sessions.py -q` → 16 passed),
   which is evidence the code exists and is exercised headlessly — it is not R12's live evidence,
   and it is not evidence the KTD10 cross-engine review ran, because no commit or PR exists for the
   review to attach to.
2. **No testing pane identity was supplied.** The plan states the testing pane's machine-local
   identity is injected by the driver from saga state; this invocation's task carried no pane id.
   Absent that, there is no way to distinguish "the operator's designated throwaway testing pane"
   from any other live pane in the workspace, and typing prompt answers, `F4`, or escape sequences
   into the wrong pane is exactly the kind of mistake the safety envelope exists to prevent.

Per the checklist's own rule ("stop on the first failing leg rather than improvising"), this applies
one step earlier: stop before leg 1 rather than improvise a target pane or improvise around the
unmerged-code gap. Re-run this checklist once U1–U7 are merged (each carrying its KTD10 review) and
a specific throwaway testing pane id is supplied.
