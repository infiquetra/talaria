---
title: U8 live acceptance run — results
type: results
status: blocked
date: 2026-08-09
covers: R1, R2, R3, R4, R5, R6, R7
executes: docs/plans/2026-08-09-u8-acceptance-checklist.md
targets: docs/plans/2026-08-08-talaria-v0-2-answerability-and-session-story-plan.md#u8-the-live-acceptance-run
---

# U8 live acceptance run — results

## The verdict

**No leg ran.** R1 through R7 are **not claimed met** by this pass. This is not a finding about
Talaria's behavior — it is a finding about the run's own preconditions, checked before any key was
sent to any terminal pane, per the checklist at
`docs/plans/2026-08-09-u8-acceptance-checklist.md`.

## What blocked the run

Repository: `/Users/jefcox/workspace/infiquetra/talaria`, GitHub remote `infiquetra/talaria`.

1. **The plan's own precondition for U8 — "with every prior unit merged" — is not satisfied.**
   `git fetch origin` followed by `git log --oneline origin/main` shows `main` at commit `a0b0677`
   ("Merge pull request #44 from infiquetra/docs/v0-2-block-markdown-plan"), the block-markdown
   planning-document merge. No commit implementing units U1 through U7 (the F1 jump key, decline,
   the interrupt sweep, the status caret, `--resume` history, or the `/sessions` picker) exists on
   `main` or on any remote branch — `git branch -a` after the fetch shows no U1–U7 branch. What
   exists is an **uncommitted working-tree diff**: `git diff origin/main --stat` reports 24 modified
   files and 5 untracked files, roughly 3,440 insertions, touching exactly the files R1–R7 name
   (`talaria/ui/app.py`, `talaria/ui/prompts.py`, `talaria/domain/state.py`,
   `talaria/ui/status_region.py`, plus a new `talaria/domain/history.py` and
   `talaria/domain/session_list.py`). Nothing has been committed, so nothing has gone through a PR
   round, and requirement R11's KTD10 cross-engine review — which the plan says must happen before
   a unit's PR round closes — has no diff and no PR to attach to. Driving a live acceptance run
   against uncommitted, unreviewed code would produce evidence about a working tree that could
   change under the operator before the release ships it, not evidence about "the release" R12
   describes validating.

   Partial, weaker evidence that the code at least behaves headlessly: `uv run pytest
   tests/ui/test_focus_returns.py tests/ui/test_sessions.py -q` returned `16 passed` against the
   current working tree. That confirms the R1/R7-adjacent behavior is exercised by a headless
   suite; it is explicitly **not** R12's live evidence ("A requirement with only headless evidence
   is not claimed met") and does not substitute for it.

2. **No throwaway testing pane identity was supplied to this run.** The plan states: "The testing
   pane's machine-local identity is injected by the driver from the saga state." This invocation's
   task message named no pane id. `herdr pane list` (Herdr terminal-multiplexer control CLI,
   `HERDR_ENV=1` confirmed set) enumerates over twenty live panes across `talaria`,
   `team-mimir`, `campps-web-app`, `home-lab`, and other repositories, several of them agent
   sessions actively `working`. None is labeled as the v0.2 testing pane. Sending keys into an
   unidentified pane on the operator's guess, rather than a pane the driver explicitly names, is
   the exact mistake the safety envelope's first clause ("throwaway sessions only, never the
   operator's working sessions") exists to prevent.

## Safety envelope: held

No pane received `send-keys` or `send-text`. No session was opened or closed. No credential, real
or canary, was typed. No recording was made, so no recording exists that could ever need
redaction-checking or digest citation — this document cites no recording, by digest or otherwise,
because none was produced. The run stopped before leg 1 rather than improvising a target pane or
improvising past the unmerged-code gap, applying the checklist's "stop on the first failing leg"
rule one step earlier than a leg: to the run's own start condition.

## What would unblock this

1. U1 through U7 land on `main` through their PR rounds, each carrying a captured KTD10 Codex
   review per R11.
2. The operator (or the driving workflow) supplies a specific pane id for the throwaway testing
   pane, sourced from saga state as the plan specifies.

With both present, re-run `docs/plans/2026-08-09-u8-acceptance-checklist.md` leg by leg, recording
with `talaria --record`, redaction-checking each recording before citing it by digest, and stopping
at the first leg that fails rather than continuing past it.

## Out of scope for this pass

Re-grading any v0.1 verdict row is out of scope regardless (per the plan's U8 "Out" clause) and was
never reached.
