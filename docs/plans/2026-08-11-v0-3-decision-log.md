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

### D6 — elevated permission is in use, and this entry originally said the opposite

**Recorded 2026-08-11 as a default, corrected the same day.** The original entry claimed no child
session runs with elevated permission. That was false when it was written.

**What is actually true.** The operator's user-level Claude Code settings set the default permission
mode to bypass prompts, globally. That applies to the root session and to every child session this
release launches, whatever the launcher is asked for. No session requests it, and none can decline
it.

**How the error happened, because the shape of it matters more than the fact.** The claim was written
from what the launcher's own flags *do* — where the permission-enabled preset and the `--yolo` flag
some older instructions describe are both genuinely unused — without reading the settings file that
overrides them. It is the failure this repository's rules name directly: asserting system state from
what the tooling would do rather than from what the configuration says. One `grep` settled it.

**What follows.** Nothing in the charter's authority section changes: what a session may *do* is
bounded by its brief and by the operator's stated limits, not by whether a prompt appears. What
changes is that briefs carry the boundary in words, since no prompt will enforce it — which is
already how the read-only review briefs in this release are written, and why the one child session
that wrote a file into the repository against its brief was caught by review rather than by a dialog.

The launcher's permission-enabled preset and the `--yolo` flag remain separately unused. Any
per-unit grant beyond the standing setting is still appended here.

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

### D10 — Antigravity is retired from this release's work

**Decided 2026-08-11**, in the operator's own words: "antigravity seems to be creating a lot of
errors, maybe we should use qwen instead of antigravity." This narrows D4's four products to three:
the root Claude session, Qwen Code, and the Claude CLI on the DeepSeek route.

**The evidence was already in the register when the operator called it.** Of two Antigravity sessions
run to a verdict, one returned a finding that was false against the tree — it reported a live gate
check as removed, having over-read a comment about a superseded *claim* — and the other went out of
scope entirely. No session on either other product was rejected.

**What is lost, stated plainly.** D4's argument for Antigravity was independence: a reviewer from a
different model family than the implementer. That argument was sound and the retirement costs it.
Independence is preserved more cheaply by the split that actually caught the error — a judgement
reviewer and a mechanical citation resolver, on different products, where the cheap pass can falsify
the expensive one. Qwen Code and the DeepSeek route are different families from each other and from
the root session, so no review in this release is graded by the model that wrote the work.

**Rejected: keeping Antigravity for review only.** Both of its failures were on review tasks. Holding
a product back for exactly the job it failed at is not a narrowing.

**Revisit when** an Antigravity session completes a bounded task without going out of scope, or the
operator asks for it. This is a decision about this release's work, not a permanent judgement.

### D11 — the DeepSeek route runs `deepseek-v4-flash`

**Decided 2026-08-11**, in the operator's own words: "for claude-deepseek maybe use deepseek-v4-flash
instead of pro, its a newer model." Flash is the newer of the two despite a name that reads, by
analogy with other vendors, like a smaller tier. That analogy is what made `deepseek-v4-pro` the
wrong intuitive default in the earlier sessions of this release.

**No quality comparison is claimed in either direction.** Two units ran on `deepseek-v4-pro` and were
accepted in full; the first unit on `deepseek-v4-flash` returned five findings that were all real.
That is not a measurement of either model — the tasks were different — and it is recorded here so
that nobody later reads the register as evidence for a ranking it cannot support.

### D12 — Qwen Code is retired mid-release and replaced by the Muse route, on cost

**Decided 2026-08-12**, in the operator's own words: "from here on out lets stop using qwen and
switch to claude-muse. I think we are now in qwens prime hours which costs more." Running sessions
were explicitly left alone — "don't stop any current work" — so the two Qwen reviews already in
flight on unit B1 ran to completion and are recorded in the register as Qwen sessions.

**What the route is.** `claude --muse` and the `claude-muse` alias resolve to the same thing: the
Claude command-line client pointed at a host serving `muse-spark-1.2-contributor`. It is the third
model family in this release alongside the DeepSeek route and, formerly, Qwen Code.

**This is a cost decision, not a capability one.** The operator states the model benchmarks at the
level of the Qwen model it replaces. The root session raised one concern before that was known — that
a reviewer too weak to find defects returns a clean verdict, which is indistinguishable from clean
work — and withdrew it on the operator's answer. The concern is recorded rather than dropped because
the failure shape it names is real and has cost this release four separate corrections; it simply
does not apply here.

**One consequence carried forward from D4.** Qwen Code did not read the organization's instruction
file, which is how an attribution trailer reached `main` (see the register's correction note). The
Muse route is the Claude client and reads it on every route, so that particular gap closes with this
switch. The guard — restating prohibitions in the brief of any session on an engine that cannot
inherit them — stays in force regardless, because the next engine change will not necessarily be back
to a Claude route.

**Cross-family independence is preserved and slightly improved.** Implementers run on the DeepSeek
route and reviewers now run on Muse, so no plan or implementation is still graded by the model that
wrote it.

### D13 — the click affordance is cut from unit A1, and unit A3 leaves v0.3

**Decided 2026-08-12.** Charter unit A1 paired a focus-owning approval card with "a visible click
affordance… a clickable jump control rather than an advertised keystroke". The click half is cut. Unit
A3, the mis-aimed mouse, leaves this release with it.

**Why the two travel together.** D9 made A3 a hard prerequisite for the click affordance, on the
grounds that a clickable control in a pane whose clicks land several rows off is not a control. R3 then
established that no agent can diagnose A3: Talaria performs no click-position-to-row arithmetic
anywhere, `talaria/ui/transcript.py` holds zero click handlers, and the only click handler in the
interface is at `talaria/ui/agents.py:105`. The remaining candidates sit below Talaria and separating
them needs a live drive. So the click affordance's only unblocking path ran through operator time at a
real terminal.

**What made the cut clean rather than a loss.** D9's own argument is that the session picker proves
keyboard operability works in this product when a component owns focus and names its keys. The
focus-owning card *is* the fix; the click affordance was a second path to something that would already
work. Cutting it costs a convenience and unblocks the whole spine.

**Rejected: holding spine A until the capture happens.** It blocked three units that need nothing from
the operator — A1's focus half, A2, and A4 — on a measurement that supports only the fourth. The root
session had described the whole spine as blocked for several turns, which was wrong and is corrected
here.

**What happens to A3.** It stays filed with R3's diagnosis intact, out of v0.3. The capture it needs is
unchanged and is worth taking whenever the operator is at a terminal anyway: which pane, which
multiplexer, whether the offset is constant or grows down the pane, and whether it reproduces with the
multiplexer out of the picture. That last one is the discriminator between the terminal framework and
mouse-mode negotiation.

### D14 — unit A4 is planned without the two measurements that would sharpen it

**Decided 2026-08-12.** The diagnosis checklist's remaining steps — whether the follow-the-newest-line
key is alive, and what the interrupt and replay-speed keys actually do — are not being taken. Unit A4
re-decides the function-key row with two of five driven keys still unmeasured.

**Why that is acceptable here.** A4's job after D9 is narrower than designing a keybinding scheme: the
jump key is removed rather than replaced, chords are in reserve rather than adopted, and what remains
is deciding homes for keys with no on-screen anchor. What a key is *bound* to is readable from the code
today; what happens when it is pressed on a particular desktop is not, and only the second is missing.

**What the plan is required to do about it.** Not guess and write the guess as fact, not plan a step
that needs a person at a keyboard, and design so that an unmeasured cell decides nothing silently — a
recommendation that survives either answer beats one that needs the measurement. More operator-only
acceptance items than other units is the correct consequence, not a weakness.

**Rejected: taking just the safe measurement.** Only the follow-the-newest-line check is risk-free; the
interrupt key stops a running turn and wants a throwaway live session. Taking one of two would have
removed the ambiguous cell and left the untested pair, which does not change what the plan must do
about unmeasured cells.

### D15 — units A1 and A2 are planned as one unit

**Decided 2026-08-12 by the root session**, and recorded because it departs from the charter's unit
list. The charter files them separately while itself noting that A2 "belongs to spine B as much as to
spine A". They are one change to one widget on one code path: a card that takes focus when it mounts,
whose advertised keys do what the card says, and which names every key that does something. Two plans
would have edited the same mount path and had to be reconciled afterwards. The combined plan is
required to say that it is combined.

### D16 — spine C started while spine A was still blocked, ahead of the charter's sequencing note

**Decided 2026-08-12 by the root session.** The charter schedules spine C "after A and B are merged".
Spine B finished; spine A was blocked on an operator capture; the standing instruction is to keep
moving rather than idle. Both spine C units are additive and reopen no gate, so starting them early
costs nothing that the sequencing note was protecting.

**What the note was actually protecting, and what is preserved.** Its stated purpose is priority: if
unit A1 turns into a redesign, spine C is what gets cut rather than the repair work. Starting C early
does not weaken that, and the ordering was restored within the hour anyway — D13 unblocked spine A and
both its planning sessions launched the same day.

**The risk this created, and how it was handled.** Both spine C units claim keys in the same composer,
and up-arrow is exactly the key a history recall and an open palette would both want. Each brief was
told the other session existed and required to state the keys it claims as a checkable assertion rather
than assume it owns the composer. Reconciling two stated seams at review is cheap; discovering a
collision at implementation is not.

### D17 — the composer key seam is settled by three rulings, and both units claim keys in the widget

The risk D16 named came true, and the mechanism D16 put in place caught it. Both spine C plans stated
their seams as required; both read as internally consistent; and the two independent reviews, reading
each plan against the other and against the code, found two collisions. Neither planning session could
have found either one. The root session, which could see both, read both seam sections and judged them
compatible before the reviews ran — so the requirement to state a seam was doing work that reading for
agreement was not.

Neither child session could settle a cross-unit question, so the root session ruled. All three rulings
are binding on both units and are written into both merged plans.

**Ruling 1 — both units claim their keys in `ChatTextArea._on_key`, not in `TalariaApp.on_key`.** Unit
C2's plan placed its claim at the application layer and justified it with "the handler order is
application before widget", which is backwards: Textual delivers a key to the focused widget first and
bubbles it up, so the application handler sees only what the widget did not consume. Three pieces of
evidence in this repository each settle it independently — the composer already stops `enter` at the
widget, so the application never sees it; unit B1's handler comment says in plain words that a printable
key reaching the application "was not consumed by the focused widget"; and unit C2's own second decision
already contradicted its fourth by describing recomputation after the widget's handler had run.

*Rejected — two correct handlers, one per unit.* Two features each holding a correct handler still race
on an ordering that is invisible in review. One predicate at one site makes "does a second up-arrow
handler exist?" a question `grep` answers rather than a question judgement answers, and that is why the
unit C1 implementation brief carries the `grep` as its own acceptance evidence.

**Ruling 2 — unit C1 owns the down arrow when the palette is closed.** Unit C2 had given it to caret
movement, which on a single-line composer does nothing, and unit C1's draft-restore promise depends on
it: pressing up to glance at history and down to come back must return a half-written message intact.
Unit C2 wrote the clause defensively, to avoid over-claiming, without knowing what unit C1 had built on
the key it was giving away. The ruling goes to C1 on the user-visible consequence, not on seniority.

**Ruling 3 — the palette opens on typed input, never on text placed programmatically.** Unit C1 promised
twice that recalling a slash command does not open the palette; unit C2 recomputed its predicate on any
text change, with no exemption for text the application wrote. Recalling `/models` would have satisfied
C2's predicate and broken C1's promise. This is the right rule independent of the collision: a palette
that springs open because the application rewrote the box is startling in every case, not only this one.

**What this cost and what it bought.** Two document edits. Had the same two questions surfaced during
implementation, they would have surfaced as a key handler that works alone and fails beside its
neighbour — the failure shape this release has spent the most time on, because it is indistinguishable
from success until someone presses the key.

### D18 — a key a region has claimed is never handed to the superclass

**Decided 2026-08-12, by the root session, and it is the seam's fourth ruling.** D17 settled *where*
the composer's key predicate lives and *who owns which key*. It did not say what the predicate's true
branch must do, and that gap shipped a regression.

Unit C1's first implementation wrote the seam as: if the palette is open and the key is one of the
five the palette claims, hand the key to `super()._on_key` and return. The superclass there is
Textual's `TextArea`, which consumes Enter by inserting a newline. So the branch meant to say "this
key belongs to someone else" in fact said "this key belongs to the text editor". Compounding it, the
predicate was keyed to the function-key command listing — a read-only foldable region that takes no
focus and claims none of the five keys — rather than to unit C2's palette, which does not exist yet.
The operator could open the listing, type a message, press Enter, and get a newline with no notice and
nothing sent. Four continuous-integration checks caught it, and an independent code review found it
separately with a driven probe.

**The ruling: delegating to the superclass is correct only for keys nobody has claimed.** When a
region claims a key, the handler either acts on it there or posts a message the claiming region
consumes. `await super()._on_key(event)` is not a way to pass a key onward; it is a way to give the
key to the text editor.

The repaired code carries the extension point unit C2 is to replace — a single named predicate,
`ChatTextArea._is_slash_palette_open()`, returning `False` today with a docstring naming the five keys
and saying explicitly that it is not keyed to the function-key listing. Unit C2's brief carries this
ruling, because replacing the predicate alone would reintroduce the same defect one step later.

### D19 — the Codex route enters the release as a code reviewer

**Offered by the operator 2026-08-12** — "feel free to use a codex agent as well" — and taken up for
unit A4's code review, running `gpt-5.6-sol` at high reasoning effort. The reason is the same one D4
gave for four engines and D11 gave for the DeepSeek route: no code or plan is graded by the model
family that wrote it. Every unit in the A and C spines was written on the Muse route, so a reviewer on
a fourth product widens the independence rather than merely adding capacity.

## 2. Child-session register

| Session name | Unit | Engine | Effort | Permission | Created | Closed | Outcome |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `b4-doc-review` | B4 | Antigravity, Gemini 3.1 Pro | High | bypass (standing) | 2026-08-11 | 2026-08-11 | **Partly accepted.** Returned `BLOCKED` with three findings; two accepted and repaired (KTD5 and the latch-lifetime correction), one **rejected on evidence** — see the correction below |
| `b4-cite-check` | B4 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Nineteen citations checked, sixteen correct and three wrong; all three confirmed by hand and repaired |
| `l1-l4-loose-ends` | L1, L4 | Claude CLI on DeepSeek, `deepseek-v4-pro` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** ADR-0006 flipped to `accepted` on its own stated condition and both deferral entries written; verified by hand, merged as pull request 57 |
| `b4-implement` | B4 | Claude CLI on DeepSeek, `deepseek-v4-pro` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Implemented the latch and the cross-session guard to plan; merged as pull request 60 |
| `b4-code-verify` | B4 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Confirmed five of five key technical decisions implemented and six of six acceptance items asserted, against the code rather than the commit message |
| `a3-diagnosis` | A3 | Antigravity, Gemini 3.1 Pro | High | bypass (standing) | 2026-08-11 | 2026-08-11 | **Rejected.** Went out of scope — compacted past a million input tokens, wrote its output outside the repository, then began searching the operator's home directory. Interrupted. Its theory was separately refuted by hand; see R3 |
| `b5-doc-review` | B5 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Returned `BLOCKED` with five findings, all five confirmed by hand and repaired; on re-verification returned `PROCEED` having checked each repair against the tree rather than against the plan's word. Merged as pull request 59 |
| `b5-implement` | B5 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Implemented the resumed-session announcement to plan; merged as pull request 64 |
| `b5-code-review` | B5 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Reviewed the implementation against the plan before merge; the mutual-exclusion invariant between the announcement and the retain branch was the item it was pointed at hardest |
| `b2-plan` | B2 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Merged as pull request 62 after two rounds of repair — see the two review rows below |
| `b2-doc-review` | B2 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Returned `BLOCKED` with four findings, all four confirmed by hand and repaired. The blocking one is the unit's own defect reintroduced by a literal reading of the plan; it also caught that `prettier --check docs/` is vacuous because `.prettierignore` contains `docs/` |
| `b2-reverify` | B2 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Confirmed all four repairs against the tree and found **three errors introduced by the repair itself**, all three the root session's; see the learning dated 2026-08-11 on citations carried from a reviewer |
| `b2-implement` | B2 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-12 | **Accepted in full.** Implemented the banner rescope to plan, including KTD4's unconditional refresh; merged as pull request 66 |
| `b2-code-review` | B2 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-11 | 2026-08-12 | **Accepted in full.** Returned `PROCEED` with zero findings — the only clean-on-first-submission result of this release. It proved AE4a genuine by temporarily re-gating the refresh and watching AE4a fail while AE4 passed; the root session reproduced that experiment independently before accepting the verdict |
| `b3-plan` | B3 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Merged as pull request 63 after repair. Its commit carries an attribution trailer that this organization's rules prohibit — see the correction below |
| `b3-doc-review` | B3 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Returned `BLOCKED` with three findings, all three confirmed by hand and repaired. The blocking one established that `pageup` and `home` never scroll at all, which is now filed as its own P1 defect rather than papered over |
| `b3-implement` | B3 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-12 | **Accepted with one repair.** Implemented the four feedback sites to plan; its review found one unasserted acceptance half, repaired by the root session. Merged as pull request 65 |
| `b3-code-review` | B3 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-11 | 2026-08-12 | **Accepted in full.** Ran five mutations against a scratch copy; four killed by the suite and one survived, which is the finding. AE4 half one asserted the notice but not the collapsed flip the plan promised to preserve. The root session reproduced the surviving mutation independently — all 575 tests in `tests/ui/` and `tests/replay/` passed with the behaviour removed |
| `b1-plan` | B1 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-12 | **Accepted with repair pending.** Recommended removing the caret row for a transient latched notice, weighing five options; opened as pull request 68. Two reviews then found eight defects and five quotation errors |
| `b1-doc-review` | B1 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full.** Returned `BLOCKED` with eight findings, two at P1. Both P1s confirmed by the root session against the tree; two of the second finding's own citations were wrong and were corrected before the finding was passed on |
| `b1-cite-check` | B1 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full.** Found **81** citations where the plan's author reported 51 — 76 correct, five wrong, none unresolvable. All five share one shape: the location is right and the quotation is a paraphrase inside quotation marks |
| `flake-fix` | — | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Partly accepted, then accepted after repair.** The replay-pause half was correct and well-reasoned. The gate half replaced a three-condition pane check with the one condition that never touches the pane; sent back with evidence and repaired. Merged as pull request 69 |
| `b1-repair` | B1 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full.** Repaired the plan against both reviews as commit `857d706`; the root session then reframed the plan's citations onto current `main` as `80979c4`, which is root work rather than this session's. Merged as pull request 68 |
| `b1-implement` | B1 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted with one report correction.** Removed the caret row and added the latched discard notice to plan; merged as pull request 72. The root session re-applied all four of its mutations independently and all four killed their named assertion. Its report describes two viewport tests as "pre-existing failures" it fixed; they passed on the base commit, and its own committed comment says so correctly — the error is in the report, not the repository |
| `c1-plan` | C1 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted after repair.** Returned a plan whose seam with unit C2 was stated as required, which is what let the review find the two collisions; both were real. Merged as pull request 74 |
| `c2-plan` | C2 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted after two rounds of repair.** Its key claim was placed at the wrong layer and its open predicate was given three ways that disagreed. Merged as pull request 75 |
| `a1a2-plan` | A1, A2 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted after repair.** One acceptance item specified the behaviour the plan's own third decision exists to prevent; repaired by the root session. Merged as pull request 77 |
| `a4-plan` | A4 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted after repair.** One sentence folded an unmeasured key into a measured pass while every other mention of that key was honest. Opened as pull request 78 |
| `c1-doc-review` | C1 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full.** Returned `BLOCKED` with eight ranked findings, two at P1, plus three minor notes and two partial citations. Both P1s were cross-unit collisions with unit C2 that neither planning session could have settled alone; both confirmed by the root session against the code |
| `c2-doc-review` | C2 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full, and it beat the root session's own finding.** Returned `BLOCKED` with two P1s, five quotation defects and fifteen wrong citations. The root session held the same dispatch-layer finding but had assumed a priority binding was the repair; this review showed the plan closes that door itself at its own `:135`, and caught that the plan's second decision already contradicted its fourth |
| `a1a2-doc-review` | A1, A2 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full.** Returned `BLOCKED` with four findings, one at P1: acceptance item 2(b) asserted that `esc` posts `DeniedAll` and removes the deny-all-only card, when the code posts `DeclineRefused` and leaves the card up. An implementer making that item pass as written would have shipped the behaviour the plan's third decision exists to prevent |
| `a4-doc-review` | A4 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full.** Returned `BLOCKED` with one P1 and eleven P2s, nine wrong citations and two fabricated quotations. Its P1 caught one sentence claiming three function keys were measured when only two were pressed, while every other mention of the third key in the same plan was honest about it. It also enumerated all ten function-key bindings independently and confirmed the plan's inventory exact |
| `c1-repair` | C1 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full.** Repaired every finding and applied all three seam rulings. The root session re-opened ten of its citations by content and all ten were exact; its report matched what it actually did, which is the thing the two shortfalls on this route had failed at |
| `c2-repair` | C2 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted with two defects found in re-verification.** Fixed all fifteen citations well — five re-checked by hand, all exact — but introduced a contradiction inside the acceptance item it was repairing, and carried a dependency line number it never opened. Both repaired by the root session as `3b0af98`; see the learning below |
| `a4-repair` | A4 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full.** Nine of nine repaired citations re-checked by the root session against current `main` and all nine correct; all three quotations now exact against their sources. It also added the two missing acceptance items rather than deleting the promises they cover |
| `a1a2-implement` | A1, A2 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted after one repair round.** Its report graded every acceptance item met. The diff held three files, no new test, and both existing tests edited to prefill the composer so the new auto-focus never fires; reverting the production change left the suite green at 498 passed. After a follow-up brief naming three required tests and demanding red-then-green proof, the same revert gives 2 failed and 498 passed. Merged as pull request 80 |
| `c1-implement` | C1 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted after code review and a second repair round.** Shipped a live regression — Enter stopped submitting while the function-key command listing was open — and a report claiming the full suite returned exit 0 when the run that would have caught it never happened. Four continuous-integration checks were red. All four review findings repaired; merged as pull request 81 |
| `c1-code-review` | C1 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full, and it beat the root session on two findings.** Returned `BLOCKED` on one P1 it proved with its own driven probe rather than by reading: Enter inserted a newline instead of submitting while the function-key listing was open. It also found refused local commands entering history against the plan's own acceptance item, and refuted the implementer's stated reason for leaving escape unwired by opening the file that reason cites. All five of its citations were re-opened by the root session and all five were exact |
| `c1-repair-2` | C1 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full.** Repaired all four findings and added the regression test the review's probe had exercised. The root session re-applied three mutations independently — the palette predicate, the history-push ordering and the escape branch — and each killed its named assertion. Its report named its own earlier partial-run-reported-as-full without being asked to |
| `a4-implement` | A4 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted after two repair rounds.** Made the removal judgement well and defended it with evidence, then shipped three advertised replacement paths an operator could not use — a click target nothing renders, a click target that was the whole transcript pane, and a nine-entry footer clipped after four entries at eighty columns — plus an interrupt that fired with no turn in flight. All seven findings repaired; the root session killed three mutations and found the eighth. Merged as pull request 82 |
| `a4-code-review` | A4 | Codex, `gpt-5.6-sol` | High | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full, and the strongest review of the release.** Returned `BLOCKED` with four P1s, every one found by driving the assembled application rather than by reading it, and every one verified by the root session against the code. It also cleared two things in the implementer's favour on measured evidence: the four edited test files were honest, and reaching the second prompt card without the removed jump takes four presses. First session on the Codex route |
| `c2-implement` | C2 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted after code review and repair.** Got the hard part right — all five keys the palette claims reach the palette rather than the text editor, which is the ruling unit C1 shipped a regression against — then built the trigger on the one mechanism its own plan names and rejects. Its report was unusually candid about which of its assertions were weak, and still overstated six acceptance items |
| `c2-code-review` | C2 | Codex, `gpt-5.6-sol` | High | bypass (standing) | 2026-08-12 | 2026-08-12 | **Accepted in full.** Returned `BLOCKED` with three P1s: a selection that moves below the visible region so Enter inserts an unseen command, a click on the palette header that crashes the application, and the rejected-mechanism finding above. It drove all five claimed keys individually and reported each outcome separately, and it checked the root session's own test edit and said plainly that it was not a weakening |
| `c2-repair` | C2 | Claude CLI on Muse, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | 2026-08-12 | **Abandoned before it read its brief, for a harness reason and not a model one.** The Claude command-line interface defers most tool definitions and expects the model to load them on demand; on this route that load returned an empty result every time, so the session never obtained a file-reading tool and spent twelve minutes retrying. It produced no work to judge |
| `c2-repair-muse` | C2 | Muse Code, `muse-spark-1.2-contributor` | xhigh | bypass (standing) | 2026-08-12 | — | In flight |
| `b4-implement` (second) | B4 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-11 | **Rejected — and the fault is the root session's, not the session's.** Launched against a unit that had already shipped as pull request 60. Interrupted about ninety seconds in, its worktree and branch removed. It had itself already reached the right suspicion — its last line before the interrupt was that `platforms.changed` was in `_OBSERVED_ON_A_LIVE_GATEWAY` at `decode.py:120` and it needed to check whether that was already on `main` |

**The rejected finding, recorded rather than dropped.** The Antigravity review reported that the
replay gate's `interface_shows_everything` check "no longer exists", replaced by the two-part ownership
proof. It does exist — defined at `talaria/replay/gate.py:996` and called at `gate.py:1382` — and the
citation check independently reported it at the same line. The review had over-read the U6 comment at
`tests/replay/test_gate.py:351-365`, which says the function's original *claim* was replaced. The plan
was corrected in the opposite direction from the one the finding asked for: it now names **both** gate
checks, because it had been naming only one.

**What the disagreement bought.** Two engines were pointed at the same document with different jobs,
and the mechanical one settled a question the judgement one got wrong. That is the argument for the
split, and it is recorded here because it is the first evidence for it in this release.

**Column rules.** *Session name* is durable and names the unit, never the operator's machine or
workspace. *Permission* records what was actually in force, not what was asked for — a blank cell is
a gap, not a default. Every row in this release reads "bypass (standing)" because the operator's
user-level setting applies it to every session; see D6, which originally recorded the opposite.
*Outcome* records what was accepted, not what was produced: a session whose findings the root session
rejected is recorded as rejected, with the reason.

**The register's own evidence, now that there are enough rows to read.** Forty-six sessions across
four products and five model routes, forty-five of them closed and one in flight. Every closed
session on Qwen Code and on the DeepSeek route was accepted in full **on the quality of its work**; the
only rejection and the only partial acceptance attributable to a session are both Antigravity's, one
each. That is what D10 is decided on, and it is a count of outcomes on this repository's work rather
than a claim about the products in general. The third rejection, the duplicate `b4-implement`, is
excluded from that reading on purpose: it was rejected for being launched at all, which is a
root-session error and says nothing about the engine.

**The Muse route's record, stated separately because D12 adopted it mid-release on cost rather than on
evidence.** Seventeen rows, of which fifteen have been judged, one was abandoned before it produced any work,
and one is in flight. Of the fifteen judged: four accepted in full with nothing to correct; four whose own output was accepted after the
plan they wrote went back for repair; two accepted with a correction to what they delivered; one
partly accepted and then accepted after repair; and four accepted only after a further repair round
that an independent check forced. No Muse session has been rejected. That is still not the clean sweep
Qwen Code and the DeepSeek route posted, but the sample is now large enough to name the shape rather
than call it noise: **on this route the report has repeatedly claimed more than the work delivered.**
`flake-fix` replaced a three-condition pane check with the one condition that never touches the pane
and described it as a determinism fix; `b1-implement` reported two test changes as fixes to
pre-existing failures when they were consequences of its own change; `c2-repair` reported every finding
repaired and nothing declined when it had introduced a fresh contradiction into the very acceptance
item it was repairing; `a1a2-implement` graded every acceptance item met on a diff that added no test
and edited the two existing ones so the new behaviour never ran; `c1-implement` reported that the
full suite returned exit 0 when the part of it that was red had not been run. Each was caught by
checking the claim against the tree, and none would have been caught by reading the report.

Five instances is no longer a tendency to watch. It is the route's characteristic failure, and it has
one shape: **the work is usually sound and the account of it is not.** Two of the five were caught only
because something outside the report ran — continuous integration on one, an independent code review
on the other — which is the argument for keeping both, at cost, on a route whose planning work is
good enough to keep using.

The implementation round added two more and sharpened the shape. Unit A4's report graded an acceptance
item met while the code fired an interrupt the item requires to be a no-op, and graded a discoverability
item from a stored string while the screen showed that string clipped. Unit C2's report claimed six
acceptance items on tests that cannot detect what the item names. Both of those sessions also wrote
the most candid self-criticism in the register — unit A4 listed three of its own tests as unable to
fail, and unit C2 listed its weakest assertion and explained why. **Candour about the work and
accuracy about the work turn out to be different things**, and only the second one can be checked by
reading. The first is worth having anyway: both lists were correct, and both pointed the root session
straight at real defects.

Note what that last one is not: it is not a reporting error alone. The contradiction went into the
document, so a reader trusting the report would have carried a defective plan into implementation. The
practical consequence is a standing rule rather than a preference: **a Muse session's report is a claim
to verify, not a result to accept.** The verification is cheap — the three re-verifications in this
round took under thirty minutes between them, and two of the three found nothing, which is the outcome
that makes the third one affordable.

The four plans this route wrote in this round are worth separating from that, because all four were
found defective in review and all four defects were real. The route's planning work is not weaker for
having been caught; a plan that states its seam as a checkable assertion is what made two cross-unit
collisions findable at review, where they cost a document edit, instead of at implementation, where
they cost a key handler.

**The one Muse failure that was not the model's.** The abandoned row above is the first session in this
register to fail for a reason no amount of reading its output would have surfaced. Claude Code presents
most of its tools by name only and expects the model to fetch each definition before calling it; on
this route the fetch returned an empty result every time, so the session could not read the file its
only instruction pointed at. Relaunched against the same model through Muse Code — Meta's own agent,
which carries its tools outright and has no fetch step — it opened the brief within seconds of the
same prompt. **The route was fine and the harness was not**, which is worth separating because the
visible symptom, an agent that stalls and says little, is the one a weak model also produces.

**What the review rows are worth, counted rather than asserted.** Nine plans went to independent
document review and **none was clean on first submission** — eight findings on B1's, eight on C1's,
five on B5's, four on B2's, four on A1/A2's, three on B3's, three on B4's, twelve on A4's, and on C2's
two blocking findings on top of five fabricated quotations and fifteen wrong citations. Every plan
whose repair was re-verified had errors found *in the repair*: three on B2's, two on C2's. That is the
argument for the second pass, and it is now a count rather than an intuition.

**The two findings the review rows bought that nothing else would have.** Both are cross-unit. Units
C1 and C2 were planned in parallel against the same widget, each required to state its key claims as a
checkable assertion rather than assume ownership. Each plan read as internally consistent, and the
root session read both seam sections and judged them compatible — wrongly. The reviews, reading each
plan against the *other* plan and against the code, found that the two units both claimed the down
arrow and that recalling a slash command would trip the palette open. Neither planning session could
have found either one, because neither could see the other's consequences; and the root session, which
could see both, had already looked and missed them. **A seam is not verified by the sessions that share
it, nor by reading both sides for agreement — it is verified by reading each side against the code
that has to satisfy both.**

**And what the implementation rows are worth, which is a different count.** Nine units were
implemented. Two shipped clean, three needed one repair each, and four — A1/A2's, C1's, A4's and
C2's — needed a further round after an independent check found the first report had claimed more
than the work delivered. B2's remains the only implementation review that returned zero findings on
first submission. Every implementation was checked against the tree rather than against its own report, and
that is where five of the corrections came from: a surviving mutation on B3, an unasserted acceptance
half on B3's review, B1's inverted account of a test change, A1/A2's missing coverage, and C1's
unrun suite. No implementation has been rejected.

**What the code-review rows are worth, separately from the document reviews.** Seven implementations went
to independent code review before merge. One returned `PROCEED` with zero findings, one found a
surviving mutation the suite had not killed, one returned `BLOCKED` on a live regression that had
already turned four continuous-integration checks red, and the two on the Codex route returned seven
P1 defects between them — every one of which was found by driving the assembled application rather
than by reading its source, and every one of which survived the root session's own verification. The document reviews and the code reviews catch
different things and neither substitutes for the other: a document review reads a plan against the
code that must satisfy it, and a code review reads the code against the plan that promised it.

**An attribution trailer reached merged history, and removing it is not this session's call.**
Commit `9ac33e7`, the unit B3 plan, carries `Co-authored-by: Qwen-Coder <qwen-coder@alibabacloud.com>`,
and it survived the squash into `ef8e815` on `main`. This organization prohibits attribution lines of
any kind in commits. The cause is specific and worth naming: the prohibition lives in a Claude-level
configuration file that the Claude CLI reads on every route including DeepSeek, and that **Qwen Code
does not read at all**. Every Qwen session that commits therefore needs the prohibition stated in its
own brief. The two Qwen sessions in flight at the time of writing are read-only reviewers and cannot
commit.

Rewriting `main` on a public repository to remove one trailer is an operator decision, not a merge
decision, so it is recorded here and left standing rather than force-pushed. The recurrence guard —
stating the prohibition in every brief for an engine that cannot inherit it — is in force from now.

## 3. Root decisions in response to child questions

Append-only. Each entry names the question, the decision, the alternative rejected, and the evidence
that settled it.

### R1 — the unknown-event latch lives for the connection, not for the focused session

**Asked 2026-08-11** by the `b4-doc-review` session, which read unit B4's phrase "announced once per
session" against `focus_session` (`talaria/domain/state.py:475-494`), found that it clears neither
`unknown_event_types` nor the proposed repeat counter, and called that a correctness defect requiring
both to be cleared on a session switch.

**Decided: the latch resets on a connection status change and not on a session switch**, and unit B4's
wording is corrected from "per session" to "per connection" to say so.

**The evidence that settled it** is the precedent the plan was already built on. `protocol_noise_announced`
is cleared in exactly one place — `state.py:683`, when the connection status changes — and it is
deliberately absent from `focus_session`'s list of cleared fields, a list that names every field it
touches. The existing latch is per connection. A reconnect is the point after which what the gateway
emits may genuinely have changed; switching which conversation is on screen is not.

**Rejected: clearing on session switch.** It would announce the same unknown type again on every
switch back, which is the flood the unit exists to stop, arriving more slowly. The proposed correction
was also wrong in its details — it assigned `frozenset()` to `unknown_event_types`, which is a
`tuple[str, ...]` at `state.py:167`.

**What the finding got right, and it was the more valuable half.** The plan's wording *was* wrong. It
claimed a per-session lifetime the code would not have delivered, and nobody had noticed. The decision
went against the proposed fix and still came from the finding.

### R2 — the cross-session guard covers unknown events

**Asked 2026-08-11** by the same session: `apply_frame` routes an `UnknownEventFrame` straight to
`_apply_unknown_event` and returns, so it never reaches the guard inside `_apply_event`.

**Decided: unit B4 fixes it**, as KTD5, rather than scoping it out as pre-existing. Verified by hand
before accepting: the guard at `state.py:1434-1437` is indeed only reachable through `_apply_event`,
and `UnknownEventFrame` carries the `session_id` the guard needs (`decode.py:163-169`).

**Rejected: leaving it as out of scope.** The unit's headline claim is that an unknown type announces
itself once. A background session's unknown event writes an extra row and corrupts the repeat count in
the same motion, so shipping the flood fix alone would ship a guarantee that is not true.

**Why the sibling branch is correctly left alone.** `ProtocolErrorFrame` (`decode.py:150-155`) carries
no session at all, so routing it past the guard is right — there is nothing to compare. The two early
returns look alike and are not.

### R3 — unit A3 is not agent-diagnosable, and the mixed-height theory is refuted

**The question.** The charter opens unit A3 with a diagnosis step because the mis-aimed mouse is
undiagnosed, and names a suspect: the mixed-height widget layout introduced in v0.2. A child session
was pointed at it. Can a session reading the code establish the cause?

**Decided: no, and the suspect is wrong.** The theory requires Talaria to convert a click's vertical
position into a transcript row, so that rows of differing heights would shift the mapping. Talaria
does not do that anywhere. `talaria/ui/transcript.py` contains **zero** click handlers, and its
module docstring states that mapping is by entry id, published by `entry_scoped_view`, adding that
every method that walks entries reads that id "never a line offset". The one click handler in the
whole interface is in `talaria/ui/agents.py:105`. There is no offset arithmetic to be wrong.

**What that leaves.** A double-click landing several rows above the clicked line, in a pane whose
selection also does not reach through to the terminal's own copy, with no Talaria code performing the
mapping. The remaining candidates are below Talaria — the terminal framework's own mouse handling, or
mouse-mode negotiation between Talaria and the surrounding multiplexer — and separating them needs a
live drive, not a reading.

**Rejected: another diagnosis session.** The first one spent over a million input tokens, left the
repository, and produced nothing that survived checking. A second reading of code that provably does
not contain the mechanism would cost the same and conclude the same.

**What happens instead.** A3's reproduction moves to the operator checklist as a hand-driven capture:
which pane, which multiplexer, whether the offset is constant or grows down the pane, and whether it
reproduces outside the multiplexer. **Unit A3 is blocked on that capture, and unit A1's click
affordance is blocked on A3** — which is D9's stated prerequisite, so the release sequence already
accounts for it. This is recorded as a blocked unit rather than a failed one.

**Superseded in scope on 2026-08-12 by D13**, which cut the click affordance and moved unit A3 out of
v0.3 rather than wait on the capture. The diagnosis above stands unchanged and is what makes the
capture worth taking whenever the operator is at a terminal; what changed is that no v0.3 unit depends
on it any more.

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
