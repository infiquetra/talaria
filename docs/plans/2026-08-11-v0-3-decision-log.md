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
| `b2-implement` | B2 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | — | Open as pull request 66, under independent review |
| `b2-code-review` | B2 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-11 | — | In flight |
| `b3-plan` | B3 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Merged as pull request 63 after repair. Its commit carries an attribution trailer that this organization's rules prohibit — see the correction below |
| `b3-doc-review` | B3 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | 2026-08-11 | **Accepted in full.** Returned `BLOCKED` with three findings, all three confirmed by hand and repaired. The blocking one established that `pageup` and `home` never scroll at all, which is now filed as its own P1 defect rather than papered over |
| `b3-implement` | B3 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | — | Open as pull request 65, under independent review |
| `b3-code-review` | B3 | Qwen Code, `qwen3.8-max-preview` | n/a | bypass (standing) | 2026-08-11 | — | In flight |
| `b1-plan` | B1 | Claude CLI on DeepSeek, `deepseek-v4-flash` | xhigh | bypass (standing) | 2026-08-11 | — | In flight |
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

**The register's own evidence, now that there are enough rows to read.** Twenty sessions across three
products, fifteen of them closed and five in flight. Every closed session on Qwen Code and on the
DeepSeek route was accepted in full **on the quality of its work**; the only rejection and the only
partial acceptance attributable to a session are both Antigravity's, one each. That is what D10 is
decided on,
and it is a count of outcomes on this repository's work rather than a claim about the products in
general. The third rejection, the duplicate `b4-implement`, is excluded from that reading on purpose:
it was rejected for being launched at all, which is a root-session error and says nothing about the
engine.

**What the review rows are worth, counted rather than asserted.** Four plans went to independent
document review and **none was clean on first submission** — five findings on B5's, four on B2's, three
on B3's, three on B4's. Both plans that were re-verified after repair had errors found *in the repair*.
That is the argument for the second pass, and it is now a count rather than an intuition.

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
