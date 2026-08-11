---
title: v0.3 orchestration charter — objective, evidence rules, lifecycle, and authority
type: charter
status: proposed
date: 2026-08-11
origin: docs/plans/2026-08-11-v0-3-session-handoff.md
---

# v0.3 orchestration charter

Written 2026-08-11 at `main` = `4048541`, immediately after the operator chose v0.3's scope from the
candidate list in [the v0.3 handoff](2026-08-11-v0-3-session-handoff.md). It records what this
release is, what evidence counts, how each unit travels from need to merge, who may do what, and
where the root session must stop and ask.

**This document is proposed, not approved, and it is not permission.** Writing it authorizes nothing:
no commit, no push, no pull request, no merge, no installation, no credential read, no live run. The
companion [decision log](2026-08-11-v0-3-decision-log.md) records the operator's answers and every
root decision made in response to a child session's question.

## Objective

**v0.3 makes the answerability spine reachable, makes Talaria confirm what it just did, and gives the
composer the conventions every comparable interface has.** That is candidates A, B and C of the
handoff taken as one release, on the operator's decision of 2026-08-11, with candidate E — the
diagnosis pass — kept as its precondition rather than as a competing candidate.

The release theme is one sentence, and it is the finding the hands-on drive produced rather than a
slogan chosen afterwards: **four defects that look unrelated are all the same problem, which is that
Talaria does not confirm what it just did.**

## Scope

### Precondition — the diagnosis pass, reduced 2026-08-11

**Most of this was already run**, in the same hands-on session that produced the handoff, and the
evidence is in [the hands-on notes](../analysis/2026-08-10-v0-2-hands-on-notes.md) rather than
missing. Note 19 carries the function-key map as far as it has been driven:

| Key | Talaria's binding | Result in a real terminal on macOS |
| --- | --- | --- |
| `F1` | jump to the newest unanswered prompt | eaten before Talaria sees it |
| `F2` | toggle the sub-agent rows | eaten — macOS Mission Control |
| `F4` | interrupt, then sweep | untested |
| `F5` | follow the newest line | "does nothing" — ambiguous, see E2 |
| `F8` | pause | works |
| `F9` | slow down | works, repeats correctly |
| `F10` | speed up | untested |

What remains genuinely unknown is three items, not four:

- **E2. Whether `F5` is alive.** It was pressed at the bottom of a paused replay, where re-following
  the newest line is a legitimate no-op, so the observation is ambiguous rather than negative. Scroll
  up and press it again. Thirty seconds, operator-only.
- **E3. `F4` and `F10` have never been pressed.** `F4` interrupts the in-flight turn *before* it
  sweeps, so it is tested deliberately rather than casually. Operator-only.
- **E4. The duplicated-content sighting.** Sighted twice; one recorded reproduction attempt failed
  and that negative result narrows the next one — try a turn where the model speaks, calls a tool,
  then continues. Operator-only to produce, agent-analysable afterwards.

**E1 is retired as a blocker.** The handoff records that what claims `F1` "is not yet established",
and that is true in the strict sense that nobody measured the keystroke outside Talaria — but it no
longer gates anything, because the fix no longer depends on the answer. See the reframing under
*Spine A*. The measurement stays available as a ten-second confirmation and is worth taking if the
operator is at the keyboard anyway; nothing waits on it.

### Spine A — make the answerability spine reachable

**Reframed 2026-08-11 on the operator's correction, and the evidence for the reframing was already in
the notes when the handoff was written.** Three notes say the same thing from different directions.
Note 19's table above records that two of the five function keys driven by hand never reach Talaria.
Note 8 records the operator's own reading — "it actually appears to be a hotkey for macos usage. So
we might want to do something different here that use a key" — and, in the same breath, an explicit
priority call: the focus key is **not** important for v0.3, and seeing the approval dialogue **is**.
Note 10 supplies the decisive counter-example, because the session picker was driven successfully in
the same session and is fully keyboard-operable — it is a dialog that owns focus and names its keys
in a footer. **Dialogs are reachable by keyboard in this product; prompt cards are not.**

So spine A is not "make `F1` arrive". The jump is the wrong mechanism rather than a broken one.

- **A1. An approval card is answerable where the operator's hands already are.** The card takes focus
  when it mounts, or is otherwise operable without any jump key — following the shape the picker
  dialog already proves works here. A prompt card presently never takes focus on mount unless it is
  input-backed (`talaria/ui/prompts.py:1171`), and an approval card is button-backed. A visible click
  affordance ships alongside, in the shape the operator cited from another interface: a clickable
  jump control rather than an advertised keystroke.
- **A2. The card's advertised keys do what the card says**, and the card names every key that does
  something, the way the picker's footer does. It presently prints `enter select · esc decline` and
  neither worked from the composer. This unit belongs to spine B as much as to spine A and is filed
  once, here.
- **A3. Mouse selection lands on the row that was clicked — promoted to load-bearing.** A double-click
  landed several rows above the clicked line, which is separately why the terminal's own
  select-and-copy does not reach through the Talaria pane while working in every other pane of the
  same terminal. Suspect: the mixed-height widget layout v0.2 introduced. **Undiagnosed**, so the unit
  opens with a diagnosis step. It was a secondary defect while the mouse was merely the fallback; it
  is load-bearing now that a click affordance is part of A1, because a clickable control in a pane
  whose clicks land several rows off is not an affordance.
- **A4. The function-key row is re-decided as a whole, not patched key by key.** Two of five keys
  driven are eaten by the desktop, one is ambiguous, and two are untested. Patching one binding at a
  time re-runs this discovery every release on a different key. The unit decides what stays on a
  function key, what moves to a chord, and what becomes a click affordance, and it records the
  decision with its reasoning.

**The open design decision inside spine A, to be settled in planning rather than here.** Which
scheme replaces the function-key row: modifier chords, a leader key, mouse-first affordances with
keys as the secondary path, or a card that owns focus so that most of the row stops being needed at
all. The operator has leaned toward the last two; the choice is theirs and it is recorded as open
until they make it.

### Spine B — Talaria confirms what it just did

- **B1. The caret status row says something an operator can interpret**, or it goes away. The
  operator's own proposal — that focus should always return to the composer — is a *design question*
  settled in planning, not a defect, because `F1` and `F4` move focus deliberately and an
  unconditional snap-back breaks the answerability spine. A narrower rule may work.
- **B2. The fallback banner names what is hidden, and the two markers share one scope.** The banner
  reports the retained count under the word "clipped", proven live by the number *falling* from 499
  to 494 as more was hidden. The condensed marker counts pane-wide while the banner counts within the
  entry. **Fixed as one change**, not two, because neither answers the operator's actual question.
- **B3. A keypress that did something is distinguishable from one that did not.**
- **B4. `platforms.changed` stops flooding the transcript.** Twenty-six rows in a single turn. The
  one-line fix is `_OBSERVED_ON_A_LIVE_GATEWAY` in `talaria/domain/decode.py:110`, whose own comment
  names this job. The unit also decides whether repeated unknown events coalesce rather than each
  taking a row.
- **B5. A resumed session names itself on arrival.** `--resume` resumes the gateway's most recent
  session rather than the operator's — correct per `session.most_recent`, and silently surprising on
  a gateway shared with automation. This is the cheapest fix and an instance of the release theme.

### Spine C — composer conventions

- **C1. Up-arrow history in the composer.**
- **C2. A filterable slash-command palette on `/`.**

Both were named unprompted by the operator while driving. Both are additive and reopen no gate.
**Sequencing note:** spine C is scheduled after A and B are merged, and it is the release's shock
absorber — if E1 turns unit A1 into a redesign, spine C is what gets cut, not the repair work.

### Loose ends folded in

- **L1. ADR-0006's status.** Its own stated acceptance condition has been met — the gate ran green,
  24 of 24, across three runs ending at `2e96324`. Flip it to `accepted` or record why it is held
  open; a record whose condition is satisfied while its status disagrees is worse than either.
- **L2. The published `F4` half-description.** Both `docs/releases/v0.2.0.md:21` and `CHANGELOG.md:26`
  say `F4` "sweeps the answerable set" and omit that it first interrupts the in-flight turn. The
  omitted half is the destructive one. Decide between correcting a shipped release's notes in place
  and correcting forward in v0.3's changelog, then do it.
- **L3. Required status checks on `main`.** Branch protection requires exactly `python-check (3.12)`
  and `python-check (3.13)`; the Node `check` job and both `install` jobs cannot block a merge, and
  `check` merged red twice — including through the v0.2.0 release. **Repository governance is an
  operator action, not a root action**; this unit produces the decision and the operator applies it.
- **L4. The ideation deferral, recorded rather than drifted into.** Two entries in
  `docs/engineering-journal/DECISIONS.md`: the fleet axis is deliberately deferred with a revisit
  condition, and shipping the answerability spine has already answered the ideation's boundary
  question Q1 — answering a blocked agent is *driving* it, not *authoring* it. Boundary question Q2
  (may Talaria ship Hermes-side plugin code?) stays open and is stated as open.
- **L5. Repository care.** A registered git worktree for `outcome/talaria-v0-2` points into an
  earlier session's scratchpad directory under `/private/tmp`. It is clean and its branch matches the
  remote, so nothing is at risk, but the path may be cleared by the operating system. **Prune the
  worktree registration; never the branch.**

### Explicitly out of scope

- **The fleet axis.** Ideation survivors 1 through 17 and 20 through 28 — the typed queue across
  profiles, the telemetry exporter read, the append-only event ledger, the session registry as root
  object, spawn-as-sessions, the Kanban engine of record, the pane-manager integration, the headless
  core, the one-line status emitted elsewhere. Deferred by decision under L4, not by neglect.
- **The Linux hand-drive** (handoff candidate D), unless the operator states that a Linux desktop
  session is happening this cycle. Absent that, v0.3's published limitations keep saying that no
  person has driven the interface on Linux, because no person has.
- **The `talaria` name on the Python Package Index**, unchanged since 2026-08-08. It reopens only when
  the name is settled *and* there is intent to publish.
- **Tagging and releasing v0.3.** The lifecycle stops at merge; see *Authority*.

## Evidence rules

These are the repository's existing rules, restated because a release that forgets them produces
exactly the class of defect this one is repairing.

1. **Source, installed bytes, and live runtime behaviour are three different kinds of evidence.**
   None of them is inferred from another. A green suite is not a working interface; a merged branch
   is not an installed one; an installed one is not a driven one.
2. **A gate measures the claim it was built to measure.** v0.2 passed a 24-of-24 replay gate, six
   external review rounds and roughly 1,700 tests, and then two hours in a real terminal produced
   seven defects — because all of them live in the seam that apparatus explicitly does not cover.
   **Every unit in spines A and B names, before it is implemented, what evidence would show it works
   for a person rather than for a test.**
3. **A claim about a person cannot be measured without one.** Units gated on E1 through E4 do not
   proceed on a plausible reading of the code.
4. **Child output is evidence, not authority.** Every finding a child session returns is
   independently checked by the root session before it changes a plan, a document, or the tree.
5. **Nothing is written anywhere claiming R1's environment clause is met.** An inherited credential
   stays readable from the process environment for the life of the process, and no change to Talaria
   reaches it.
6. **Negative results are recorded.** The failed duplicated-content reproduction is worth more than
   silence, and it is what narrows E4.

## Work intake

The work source is this repository's own documents, in this order of authority:

1. [The v0.3 handoff](2026-08-11-v0-3-session-handoff.md) — candidates, defects with mechanism,
   undiagnosed items, and the release's loose ends.
2. [The v0.2 hands-on notes](../analysis/2026-08-10-v0-2-hands-on-notes.md) — nineteen operator notes
   and the sorted candidate list every finding above is drawn from.
3. [QUEUED.md](../engineering-journal/QUEUED.md) — the `## P0` section opens with the two entries the
   handoff expands on. Priorities there are the author's judgement, not a decision.
4. [DECISIONS.md](../engineering-journal/DECISIONS.md) and the six architecture decision records —
   constraints, not history.

**There is no external board, milestone, or issue tracker in play.** The repository has zero open
issues and zero open pull requests as of 2026-08-11. A candidate is not implemented merely because it
appears in a list: each unit's plan restates the need and says why the work is still desirable.

## Lifecycle per unit

```
validate need --> plan --> independent document review --> repair findings --> implement
    --> independent code review --> repair findings --> narrow tests --> broader tests --> PR --> merge
```

Every actionable review finding is either fixed or explicitly reclassified as non-actionable **with
evidence**. A finding is never closed by assertion. Narrow tests run first; the broader project check
runs when risk warrants and always before a pull request is opened.

The lifecycle **stops at merge**. Tagging, releasing, and publishing are not part of it.

## Review policy

- **Document review and code review are performed by a session that did not author the work**, and
  where possible by a different model family than the implementer, because independence is the point.
- **Review is proportionate.** Concrete correctness, security, privacy, reliability, test and
  maintainability problems are fixed. Speculative hardening, new abstractions, control planes and
  rare-edge-case machinery are not added on a review's suggestion unless the repository and the
  objective already warrant them.
- **The root session reads every review itself** and forms its own verdict before acting on it.

## Concurrency and session operation

- **At most three active work streams**, matching the operator's standing rule. The Anthropic
  rate-limit half of that rule binds Claude-routed sessions specifically — the root session plus any
  Claude subagents — while externally-routed engines are additionally bounded by the same three-stream
  total, because operator attention is the scarcer resource.
- **Child sessions are created as named tabs in the operator-approved workspace of the local terminal
  workspace manager**, one root pane per tab, no pane splits, created without stealing focus where
  the tooling supports it. Tab names are durable so a session can be resumed.
- **Launch behaviour comes from the local agent launcher's own documented surface**, read at launch
  time rather than remembered. Its command syntax is not invented.
- **Tabs are closed promptly** once their output is accepted and the branch or handoff is durable.
- **Elevated permission is not used.** The local launcher exposes a permission-enabled preset rather
  than the `--yolo` flag some older instructions describe; neither is used unless the operator
  authorizes it for a named unit, and the grant is recorded in the decision log.

## Engine, model, and effort policy

The operator authorized four products on 2026-08-11: the root Claude session, plus three external
agent products — Antigravity, Qwen Code, and the Claude command-line client routed to DeepSeek's
Anthropic-compatible endpoint. **Every session gets an explicitly chosen engine and effort; nothing
is inherited silently.**

| Role | Engine | Why this one | Effort |
| --- | --- | --- | --- |
| Root orchestration, plan authorship, final verdicts, all integration | The root Claude session | Owns the charter, the evidence bar, and every irreversible action | High |
| Document review and code review | Antigravity | Independence from the implementing model is its whole value; a different family sees different defects | High |
| Implementation of specified units | Claude routed to DeepSeek | The same command-line surface and therefore the same repository conventions and project check, at a strong reasoning tier | High |
| Read-only survey, mechanical sweeps, evidence collation | Qwen Code | A different agent surface, so it gets narrow and independently verifiable jobs | Default |

**Two risks this policy carries, stated rather than discovered later.** Qwen Code is its own agent
command-line client rather than a Claude provider, so it does not necessarily read this repository's
instruction files the way the others do — its units are scoped narrowly enough that the root session
can verify the whole output. And a unit routed to an external provider sends this repository's
content to that provider; the repository is public and carries no secrets by policy, which is what
makes this acceptable rather than a data-handling question.

**The P0 keyboard path (unit A1) is implemented under root supervision**, not handed off whole,
because it is the unit whose failure mode is a second release claiming a feature that does not work.

## Authority

Granted by the operator on 2026-08-11, for this session, for the scope above:

- **Commit and push** — yes, on a branch, with revert-quality messages.
- **Open a pull request** — yes.
- **Merge** — yes, on each completed unit, with the required checks observed green rather than
  assumed.
- **Tag, release, publish** — **no.** Held for a separate explicit go-ahead.
- **Marketplace refresh, installation, deployment, runtime activation** — **not requested and not
  granted.**
- **Credential access** — **not requested and not granted.** Nothing in this scope needs one.
- **Live testing against a running Hermes gateway** — **operator-only.** The root session neither
  attaches to a gateway nor drives the interface.
- **Repository governance** (branch protection, required checks, ruleset changes) — **operator-only.**
  Unit L3 produces the decision; the operator applies it.

A grant from an earlier session does not carry into this one, and none of the above extends to work
outside the scope section.

## Stop boundary

The root session makes bounded decisions inside this charter and **stops and asks** when a choice
would:

- change product behaviour beyond a unit's stated intent, or reopen a settled architecture decision;
- change security posture, privacy posture, or data handling;
- change repository governance, branch protection, or what a released artifact claims;
- change the scope, the release theme, or which candidates are in v0.3;
- delete, rewrite, or overwrite work it did not create — including any branch, tag, or worktree;
- require a credential, a live gateway, an installation, or a real-terminal keypress;
- cost materially more than the unit's estimate, or turn a repair into a redesign.

It also stops when the evidence disagrees with this charter. A charter that has gone stale is
corrected before it is followed.

## Repository-care rules

- **`main` is never committed to directly.** One branch per unit, merged by pull request.
- **`outcome/talaria-v0-2` is never deleted**, locally or on the remote. It carries the v0.2 outcome
  specification, which lives nowhere else.
- **The tag `evidence/block-markdown-gate` is never deleted.** It preserves the build history the
  published gate results cite by hash; none of those commits is reachable from `main`.
- **Dirty, untracked, divergent or unpublished work is identified and preserved before anything
  else.** Nothing is broad-staged, force-pushed, or silently absorbed.
- **This repository is public.** No operator profile name, profile path, machine name, host, socket
  path, workspace identifier, or other operator-specific inventory reaches any committed file or
  commit message. Machine-specific facts belong in session memory.
- **Naming convention for tooling, following this repository's own precedent.** Publicly available
  products are named plainly, exactly as the existing documents name Hermes, Textual, `prompt_toolkit`,
  k9s, stern and Claude Code. The operator's private tooling is described by function — "the local
  agent launcher", "the terminal workspace manager" — exactly as the 2026-08-02 ideation described the
  pane manager. This charter follows that line; if the operator draws it elsewhere, this section is
  the thing to edit.

## Validation

The project check, exactly as `CLAUDE.md` states it:

```bash
uv sync --all-groups
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

`npm run check` additionally, and only, when anything under `src/` is touched — that tree is the
TypeScript reference recorder `tests/recorder/test_equivalence.py` asserts the Python recorder is
equivalent to, not dead bootstrap.

**One green run is not proof.** The v0.1-era intermittent suite failure has not recurred and has not
been diagnosed either. A unit that touches the transcript pane, the prompt registry, or the coalescing
timer runs the affected tests repeatedly rather than once.

**Acceptance evidence for spines A and B is a hands-on drive**, not a green suite. Each unit names
the observable a person would see, and the release is not called done on test evidence alone.

## Integration

- One branch per unit, named for the unit.
- Conventional commit messages, `type(scope): description`, small and atomic, with no attribution
  lines of any kind.
- A pull request per unit; merge once the two required checks are observed green.
- **The engineering journal is updated in the same commit that ships the change** — a dated
  `LEARNINGS.md` entry for any non-obvious fix, with evidence, mechanism, and a generalizable rule; a
  `DECISIONS.md` entry for any convention or tooling decision, with rationale, rejected alternatives,
  and a revisit condition.

## Installation, deployment, activation

**None in scope.** Nothing is installed, deployed, refreshed, or activated. No claim about installed
behaviour is made anywhere, because no installed bytes are read and no fresh process is started.

## Live testing

**Operator-only, and it is the release's binding constraint.** Units E1 through E4 and the acceptance
evidence for spines A and B all require a real terminal on a real desktop against a running Hermes
gateway. The root session prepares written, step-by-step checklists in the shape of the existing
`docs/plans/2026-08-06-u6-row19-operator-checklist.md` — each step naming what to run, what to capture
verbatim, and what to hand back — and the operator runs them. The first of them,
[the v0.3 diagnosis checklist](2026-08-11-v0-3-diagnosis-checklist.md), is written and ready.

Nothing an agent produces is recorded as live evidence. A checklist that has not been run is not
progress against the item it belongs to.

## Operator answers — settled 2026-08-11

The four questions this charter opened with are answered. Nothing here is assumed.

1. **Child sessions join the workspace this session already runs in**, which is also the local
   launcher's own default when a run starts from inside one. Tab names stay durable and name the unit.
2. **The diagnosis pass is happening this cycle.** The checklist is written:
   [the v0.3 diagnosis checklist](2026-08-11-v0-3-diagnosis-checklist.md), `status: ready-for-operator`,
   four steps, roughly half an hour. Units in spines A, B and C are sized against its captures rather
   than against a reading of the code.
3. **No Linux hand-drive this cycle.** Handoff candidate D stays out of scope and v0.3 keeps
   publishing that no person has driven the interface on Linux, because no person has.
4. **The tooling naming line stands as written** — public products named plainly, the operator's own
   tooling described by function.
