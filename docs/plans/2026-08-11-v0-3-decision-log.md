---
title: v0.3 decision log — operator decisions, child sessions, and root decisions
type: decision-log
status: proposed
date: 2026-08-11
charter: docs/plans/2026-08-11-v0-3-orchestration-charter.md
---

# v0.3 decision log

The operational companion to [the v0.3 orchestration charter](2026-08-11-v0-3-orchestration-charter.md).
The charter says what the release is and who may do what; this file records what was actually decided,
by whom, and when.

**This document is proposed alongside the charter and approves nothing on its own.**

## How this file is used

- **Section 1 is the operator's.** Session-wide decisions, quoted rather than paraphrased. Root does
  not edit an operator decision; it appends a correction with its own date.
- **Section 2 is the child-session register.** One row per session created, whether or not it
  finished. Rows are never deleted — a session that was killed is recorded as killed.
- **Section 3 is append-only.** Every decision the root session made in response to a child session's
  question, with the alternative that was rejected. New entries go at the bottom.
- **Nothing operator-specific is recorded anywhere in this file.** No profile names, machine names,
  hosts, socket paths, or workspace identifiers — the repository is public. Sessions are named by the
  unit they work on.

## 1. Operator decisions, session-wide

### D1 — v0.3's scope is candidates A, B and C as one release

**Decided 2026-08-11.** From the four shapes put to the operator, the answer was "A+B+C as one
release": the keyboard path to an approval card, the confirm-what-it-just-did theme, and the composer
conventions, rather than holding the composer work as a stretch.

The diagnosis pass, handoff candidate E, stays as the precondition it was described as rather than
becoming a fifth candidate. Recorded in the charter under *Scope*, with spine C named as the release's
shock absorber if the `F1` diagnosis turns unit A1 into a redesign.

### D2 — the fleet axis is deferred by decision, not by neglect

**Decided 2026-08-11.** The un-built ideation — survivors 1 through 17 and 20 through 28 of
[the 2026-08-02 product-shape ideation](../ideation/2026-08-02-talaria-product-shape-ideation.md) —
stays out of v0.3, and the deferral is written into `DECISIONS.md` with a revisit condition rather
than left implicit.

Two entries, both scheduled as unit L4:

- The fleet axis is deliberately deferred, with the condition under which it reopens.
- The ideation's boundary question Q1 — does answering a blocked agent count as *driving* it or
  *authoring* it? — **has already been answered in practice by shipping the v0.2 answerability
  spine**, and that answer is *driving*. It has never been recorded. Boundary question Q2 (may Talaria
  ship Hermes-side plugin code?) remains genuinely open and is stated as open.

### D3 — the lifecycle runs from plan to merge, and stops there

**Decided 2026-08-11.** Commit, push, open a pull request and merge, on each completed unit. Tagging
and releasing v0.3 are held for a separate explicit go-ahead.

Not granted, and not requested: marketplace refresh, installation, deployment, runtime activation,
credential access, and live testing against a gateway. Repository governance stays with the operator —
unit L3 produces the required-checks decision and the operator applies it.

### D4 — four engines, with the root session remaining Claude

**Decided 2026-08-11**, in the operator's own words: "lets use: antigravity, qwen and claude-deepseek
with the main session remaining as claude".

Role assignment is in the charter under *Engine, model, and effort policy*. In summary: the root
Claude session orchestrates, authors plans, forms verdicts and performs every integration step;
Antigravity performs document and code review, where independence from the implementing model is the
point; the Claude command-line client routed to DeepSeek implements specified units; Qwen Code takes
read-only survey and mechanical work scoped narrowly enough to be verified whole.

**Two consequences recorded at the time of the decision rather than discovered later.** Qwen Code is
its own agent command-line client rather than a Claude provider, so it does not necessarily read this
repository's instruction files the way the others do. And routing a unit to an external provider sends
this repository's content to that provider — acceptable here because the repository is public and
carries no secrets by policy, which is a property to re-check rather than an assumption to inherit.

### D5 — concurrency stays at three active streams

**Standing operator rule, applied 2026-08-11.** At most three work streams in flight. The Anthropic
rate-limit half binds Claude-routed sessions specifically; the three-stream total binds everything,
because operator attention is the scarcer resource.

### D6 — elevated permission is not in use

**Default, unchanged 2026-08-11.** No child session runs with elevated permission. The local agent
launcher exposes a permission-enabled preset rather than the `--yolo` flag described in some older
instructions; neither is used unless the operator authorizes it for a named unit, and any such grant
is appended here.

### D7 — the four open questions, answered

**Decided 2026-08-11**, in the same sitting as D1 through D6. Each was put with a recommended default
identified as a proposal; all four came back on the recommendation.

| # | Question | Answer | Consequence |
| --- | --- | --- | --- |
| Q-a | Where do child sessions go? | The workspace this session already runs in | Child tabs join it; tab names stay durable and name the unit |
| Q-b | Is the diagnosis pass happening? | Yes — checklist requested | [The diagnosis checklist](2026-08-11-v0-3-diagnosis-checklist.md) is written and ready for the operator; unit A1 is sized on its captures |
| Q-c | Is a Linux hand-drive happening? | No | Handoff candidate D stays out of scope; the published limitation stands unchanged and stays honest |
| Q-d | Is the tooling-naming line right? | Yes, follow repository precedent | Public products named plainly; the operator's own tooling described by function |

**Nothing is left open.** The charter's *Operator answers* section carries the same four, and the two
documents are consistent as of 2026-08-11.

### D8 — the function-key row is the problem, not one binding, and the focus jump is not the priority

**Decided 2026-08-11**, correcting the charter within hours of its being written. The operator's own
words: much of the diagnosis checklist "has mostly been done during the company-account session that
generated the handoff document. many of these are because those keys are already mapped to other
things on the computer... we would want to maybe move away from those hot keys in general, moving to
something else... maybe using the mouse for some things."

**The correction was verifiable against this repository, and it is.** Note 19 of
[the hands-on notes](../analysis/2026-08-10-v0-2-hands-on-notes.md) carries the function-key map:
`F1` eaten before Talaria sees it, `F2` eaten by macOS Mission Control, `F5` ambiguous, `F4` and `F10`
untested, `F8` and `F9` working. Note 8 carries both the operator's reading of the `F1` loss and an
explicit priority call — the focus key is not important for v0.3, the approval dialogue is. Note 10
carries the counter-example that decides the shape: the session picker is fully keyboard-operable and
worked, because it is a dialog that owns focus and names its keys.

**The conflict this exposes, stated rather than smoothed over.** The handoff, written by the same
session that produced those notes, records that what claims `F1` "is not yet established" and files
the missing keyboard path as the release's only P0. Both readings are defensible — nobody measured
the keystroke outside Talaria, so *strictly* the cause is unestablished — but the handoff's framing
carried a priority the notes explicitly deny. **Where the two disagree, the operator's own recorded
priority wins.**

**What changed as a result**, in the charter's *Scope* section:

- Diagnostic item E1 is retired as a blocker. The remaining checklist is three items, not four.
- Unit A1 is reframed from "make `F1` arrive" to "the card is answerable where the operator's hands
  already are", with a click affordance alongside.
- Unit A3, the mis-aimed mouse, is promoted to load-bearing rather than secondary, because a click
  affordance in a pane whose clicks land several rows off is not an affordance.
- Unit A4 is added: the function-key row is re-decided as a whole rather than patched key by key.
- One design decision is recorded as **open** — which scheme replaces the row. The operator has
  leaned toward mouse-first affordances and a focus-owning card; the choice is theirs to make.

### D9 — the card owns focus, and click affordances layer on top of it

**Decided 2026-08-11**, closing the design decision D8 left open. Of four schemes put to the operator —
a focus-owning card, a focus-owning card with the click work deferred, mouse-first with keys
secondary, and relocating the whole row to chords or a leader key — the answer was the first: **the
card takes focus when it mounts and names its keys, and clickable controls layer on top where a
control is a genuine target.**

Two consequences follow directly and are already written into the charter's spine A:

- **Unit A3, the mis-aimed mouse, is a hard prerequisite** rather than a parallel fix, and it is
  diagnosed before either half of A1 is built. A clickable control in a pane whose clicks land several
  rows off is not a control.
- **The jump key is not replaced, it is removed.** It exists only because the card could not be
  reached; a card that owns focus leaves nothing to jump to. Unit A4 therefore decides homes for the
  keys that genuinely have no on-screen anchor — the sub-agent toggle, whose `F2` is eaten by the
  desktop, and the replay controls, of which `F8` and `F9` are confirmed working — rather than
  re-binding a jump.

Chords stay in reserve for exactly those cases and are not adopted as the scheme.

## 2. Child-session register

No child session has been created. The charter is preparation, and preparation is not permission.

| Session name | Unit | Engine | Effort | Permission | Created | Closed | Outcome |
| --- | --- | --- | --- | --- | --- | --- | --- |
| _(none yet)_ | | | | | | | |

**Column rules.** *Session name* is durable and names the unit, never the operator's machine or
workspace. *Permission* records the choice explicitly, including "standard" — a blank cell is a gap,
not a default. *Outcome* records what was accepted, not what was produced: a session whose findings
the root session rejected is recorded as rejected, with the reason.

## 3. Root decisions in response to child questions

Append-only. Each entry names the question, the decision, the alternative rejected, and the evidence
that settled it. Empty until a child session asks something.

_No entries._

## 4. Provenance

- **Scope and candidates.** [The v0.3 handoff](2026-08-11-v0-3-session-handoff.md), written
  2026-08-11 at `main` = `06dc858`. Its *Where the repository is* section is two merges stale — it
  names `main` at `1cd176a` — and both merges since are documentation-only, verified 2026-08-11 by
  `git diff 1cd176a..HEAD` showing no change under `talaria/`, `src/` or `tests/`. Every other claim
  in that section was re-checked and holds.
- **Every defect and design question in scope.**
  [The v0.2 hands-on notes](../analysis/2026-08-10-v0-2-hands-on-notes.md), 2026-08-10.
- **The deferred fleet axis.**
  [The product-shape ideation](../ideation/2026-08-02-talaria-product-shape-ideation.md), 2026-08-02 —
  twenty-eight survivors, of which five of the seven in its own MVP tier are unstarted or barely
  started, and the two later tiers are untouched apart from the record-and-replay survivor, which
  shipped in full.
- **Deferred work and durable decisions.**
  [QUEUED.md](../engineering-journal/QUEUED.md) and
  [DECISIONS.md](../engineering-journal/DECISIONS.md).
- **Constraints.** The six architecture decision records in
  [platform-specs/04-architecture/adrs/](../../platform-specs/04-architecture/adrs/), five `accepted`
  and ADR-0006 `proposed` pending unit L1.
- **Repository state at the time of writing.** `main` at `4048541`, clean, level with `origin/main`,
  zero open issues, zero open pull requests. One other branch, `outcome/talaria-v0-2`, local and
  remote both at `0ad5fc1`. Four tags, one of which — `evidence/block-markdown-gate` — is evidence
  rather than a release.
