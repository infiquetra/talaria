---
title: Restate the v0.1 daily-driver verdict on current evidence
type: docs
status: active
date: 2026-08-05
origin: docs/analysis/2026-08-05-conformance-audit-drift-findings.md
---

# Restate the v0.1 daily-driver verdict on current evidence

## Summary

`docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md` is the artifact that gates the v0.1 release,
and two of its evidence rows grade requirements `unmet` on reasons that stopped being true on
2026-08-04. This plan re-grades those rows against the live recordings, re-reads the three other rows
the verdict's own closing rule depends on (6, 13 and 19), restates the verdict on whatever the
corrected table supports, and closes DRIFT-04 — the last open finding of the R1–R40 conformance
audit.

The expected outcome is **not** a move to READY. It is a verdict that says something true.

## Problem Frame

The verdict at `:428` reads "Talaria v0.1 is **NOT READY** as a daily driver", and its R2 and R3 rows
(`:85`, `:86`, evidence-table rows 17 and 18) give as reasons "No Hermes gateway has answered one"
and "Nothing was submitted to a Hermes session." Both became false on 2026-08-04, when Talaria
attached to a real Hermes dashboard repeatedly and streamed turns to completion.

Nothing links the live-attach work back to the verdict's rows, so no mechanism re-opens a verdict
when its blockers clear. The cost is not that anyone is misled into trusting something unproven —
this is an **under**-claim, and its direction is the safe one. The cost is that the project cannot
currently say either "ready" or "not ready" on evidence, and the release gate is held shut by two
sentences that outlived their facts.

**A correction to the finding, established before planning and load-bearing for the whole plan.**
DRIFT-04 states that the verdict "names exactly two things that would change" it. That is wrong.
Verified by reading the document on `main`:

- `:443-472` names **five** items, not two: (1) R2, (2) R3, (3) R1's remaining half — the
  environment-inherited credential decision, (4) the platform matrix, marked "partly done", (5) CI,
  marked done.
- It closes: "Until **at least** (1) and (2) are done and recorded here, this document's verdict does
  not move." The words "at least" make (1) and (2) necessary, not sufficient.
- `:415-441` states "AE7 and R39 say the ready verdict is blocked on any gap", and reads the table as
  rows 17, 18 and 19 unmet, row 13 partially unmet, row 6 inferred rather than measured.

So rows 13 and 19 are gaps that DRIFT-04 never mentions, and by the document's own rule either one
alone blocks READY. A plan that re-graded only R2 and R3 and then moved the verdict would commit the
same error as the defect it fixes, in the opposite direction.

## Requirements

**R1.** The R2 row (`:85`, table row 17) and the R3 row (`:86`, table row 18) are re-graded against
the live recordings and updated in place, each naming the specific frames that settle it.

**R2.** Every re-graded row cites a number produced by a stated, reproducible measurement — never a
number carried over from the audit's summary of the corpus. A digest is only a citation if its
construction is written down (KTD4); a hash with no stated rule cannot be checked and is therefore
not evidence.

**R3.** Rows 6, 13 and 19, and the five "What would change this verdict" items, are each re-read
against current evidence and their present state stated — whether it moved or not. A row that has
not moved is reported as not moved, not left silent.

**R4.** The verdict itself is restated on whatever the corrected table supports, applying the
document's own rule that any gap blocks READY. Neither outcome is presupposed.

**R5.** Every sentence outside the verdict that the 2026-08-04 attach falsified is corrected, and
each records what it used to say. There are **four**, all confirmed present on `main`:

| Location | The sentence | Why it is false |
| --- | --- | --- |
| `README.md:50-55` | The "Read the verdict before you rely on it" paragraph, opening "Talaria has never been connected to a Hermes gateway: every transport test in this repository dials a loopback stub" | The repository's front page, and the most prominent claim of the four. Flatly false since 2026-08-04. The rest of the paragraph reasons *from* that premise, so the whole paragraph is rewritten, not the one sentence |
| `talaria/cli.py:465` | "This path has never been run against a real Hermes gateway" — and the R2/R3 unmet claim it draws from that | First half already false; second half becomes false the moment U1 lands |
| `talaria/ui/app.py:1839` | "It has never run against a Hermes gateway — see R2 in [the verdict], which records it as unmet" | Both halves false |
| `tests/transport/test_session_startup.py:9-13` | "No Hermes gateway was attached at any point in this run … R2 … is recorded as unmet in [the verdict]" | The first sentence is still true — it scopes itself to *this test module's* run. Only the R2-is-unmet clause needs correcting |

Correcting `app.py` alone would leave a public README asserting the opposite of the verdict this plan
just restated, which is the same defect one level out.

A fifth sentence, `talaria/ui/app.py:1798`, is also wrong but is **not** in this list, because the
attach is not what falsified it — commit `ec861fa` did, by taking the required-method set from
seventeen to eighteen. U2 finds it and U3 fixes it in the same edit as the docstring fifty lines
below. Two defects in one file, two different causes; keeping them apart is what stops the fix
citing the wrong reason.

**R6.** DRIFT-04 is closed out: its register entry moves from "Open findings" to "Resolved findings"
with the closing commit, its P1 entry leaves `QUEUED.md`, and the register's counts, status header
and the `docs/analysis/README.md:18-19` cross-reference are updated to match.

**R7.** No corpus is committed and none is cited by local path. Citation is by digest and count
(R29). No credential value appears in any file, fixture, commit message or plan text (R6, R10).

**R8.** Every sentence this plan replaces because it became false records what it used to say. A true
sentence with a false history reads exactly like a true sentence, and that is the failure mode the
whole audit exists to catch.

**R9.** The journal is updated in the commit that ships the work: a `LEARNINGS.md` entry for the
stale-verdict mechanism, and a `DECISIONS.md` entry for KTD4's corpus-citation convention. The
deferred general problem — a gating document with no inbound link from the work that unblocks it —
is filed in `QUEUED.md` rather than left in this plan's Scope Boundaries, where nothing would find
it again.

## Key Technical Decisions

**KTD1 — A reproducible measurement stands in for a test gate; the measurement script is not
committed.** This work ships almost no code, so "the suite is green" proves nothing about it. The
analogue of a test here is a measurement anyone can re-run: the document states the method precisely
enough that a reader with a corpus reproduces the number, and the number in the document is the one
the measurement printed. The script itself stays out of the repository, because it can only run on a
machine holding recordings — committing it would add a check that cannot execute in CI and would sit
permanently skipped, which is worse than no check. Rejected alternative: commit the grader with a
skip-if-no-corpus guard. That buys a green tick that measures nothing on every machine that matters.

**KTD2 — The falsifiability control is a claim that must come back false.** A re-grading pass that
confirms everything it looks at has not been tested, it has been agreed with. Row 6 is the control:
grounding for this plan measured it and found it has **not** moved (see U2), which is the shape a
real re-grade produces. Any unit reporting that every row improved must be re-run.

**KTD3 — Rows are re-graded independently before the verdict is restated.** U3 reads the table U1 and
U2 produced; it does not re-derive rows. This keeps the restatement honest — a verdict written first
and evidence assembled to fit it is the defect being remediated.

**KTD4 — The two corpus digests are different scopes, are computed differently, and take different
label namespaces.** "The corpus" naming two different things is how a citation stops being checkable,
so both constructions are written down here.

- **One recording.** `live_corpus_identity` (`talaria/replay/gate.py:317-328`) is the repository's
  committed way to cite a single frame log: `sha256` over that one file's bytes, labelled
  `talaria-live-v1-<frames>f-<sha256[:12]>`. Both existing citations in the repository are this form
  — `talaria-live-v1-32f-5f477fa24fa5`, the 32-frame recording behind R3's replay comparison, and
  `talaria-live-v1-5773f-88a3604c34b7` in the Textual gate results.
- **The whole corpus.** An aggregate over every recording, which `live_corpus_identity` cannot
  produce and which must not borrow its label: `talaria-live-corpus-v1-<total frames>f-<sha256[:12]>`,
  where the hash is `sha256` over each recording's raw bytes concatenated in filename-sorted order,
  and the count is frames summed across the corpus.

The distinct `-corpus-` segment is introduced by this plan. Without it an aggregate figure wears the
single-recording label, and a reader who reaches for the repository's own helper to check it gets a
different hash — a number nobody can re-derive, which is precisely what KTD1 says DRIFT-04 *is*.

R2's row rests on the aggregate; R3's replay half rests on the single 32-frame recording. Each row
says which.

## Implementation Units

**Dependency order is a strict chain: U1 → U2 → U3 → U4, with no parallel wave.** U1, U2 and U3 all
edit `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`, and concurrent agents share one working
tree with no cross-agent file lock, so any two of them in the same wave would risk losing a write.

Sequencing rather than merging them into one unit is deliberate and costs prompt-cache reuse on
purpose. KTD3 requires the rows to be re-graded before and independently of the verdict that reads
them; a single agent holding both jobs is exactly the agent that can write the conclusion first and
assemble the evidence to fit it. That is the defect being remediated, so the boundary is a
correctness control, not a packaging choice.

### U1. Re-grade the R2 and R3 rows against the corpus

Replaces two stale rows with cited ones. Smallest unit, no dependencies, lands alone.

**Scope.** `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md` rows at `:85` and `:86` (evidence
table rows 17 and 18) plus their status column. Nothing else in the document.

**Where the corpus is**, without writing a path into a committed file (R29): the recordings live in
the directory `talaria.config.recordings_dir()` returns, which resolves against the operator's
configuration directory. Read them with the frame-log fields documented in
`docs/formats/frame-log.md` — `seq`, `dir` (`out` from Talaria, `in` from the gateway) and `frame`.
A reply is matched to the call that produced it by JSON-RPC `id`.

**What the grounding pass already measured**, to be reproduced rather than trusted: 17 recordings,
2,659 frames, corpus digest `talaria-live-corpus-v1-2659f-bd69e537f1d9` — KTD4's aggregate form, not
`live_corpus_identity`'s single-recording one. A gateway **reply** carrying a `session_id` appears in
15 of 17. A `prompt.submit` followed immediately by `message.start`, then `message.delta` frames,
then `message.complete` appears in 12 of 17, delta counts from 2 to 944. Two recordings hold zero
frames and account for the 17-vs-15 gap.

**The distinction that makes R2's row honest.** Check for gateway *replies*, not for Talaria's
*calls*. A call going out proves Talaria tried; only a reply carrying a `session_id` proves the
gateway answered. The row's original reason was "No Hermes gateway has **answered** one."

**R3 needs its second half named separately.** The row demands "one live turn streamed to completion,
**compared against replay**". Streaming is settled by the 12 recordings above; the replay comparison
is settled by `docs/engineering-journal/LEARNINGS.md:111`, which records it passing byte-identical on
the 32-frame recording. Both halves get cited, because citing only the first would leave the row
true-sounding and under-evidenced.

**Test expectation.** Re-run the measurement from a clean shell and confirm every number written into
the two rows matches what it prints. Confirm `LEARNINGS.md:111` says what the row claims it says, by
reading it rather than by trusting this plan.

### U2. Re-read rows 6, 13 and 19 and the five "what would change" items

The unit that keeps the restatement honest, and the one most likely to return "unchanged".

**Scope.** Evidence-table rows 6, 13 and 19; the five items at `:443-472`. State the present position
of each. Do not restate the verdict here.

**Row 6 has already been measured and has NOT moved — this is KTD2's control.** The verdict says
three of the thirteen inferred methods have since been called live and ten have no runtime evidence.
Grounding confirms that is still exactly right: the corpus shows eight distinct methods called, of
which five are the read-only startup probes (already row 1's business) and exactly three —
`session.create`, `prompt.submit`, `slash.exec` — are from the thirteen-method evidence-only set. The
remaining ten still have nothing. Reproduce this against `talaria/domain/compat.py`'s
`EVIDENCE_ONLY_METHODS` rather than by re-reading the prose.

**Two places still say twelve, and one of them is inside item (1).** Commit `ec861fa` pinned
`slash.exec` and took the evidence-only set from twelve to thirteen — verified by counting
`classification="evidence-only"` in `talaria/domain/compat.py` at `ec861fa~1` (twelve) and at
`ec861fa` (thirteen). That commit swept the verdict's counts but missed two sentences, and neither is
a copy of the other:

- The verdict at `:450`, inside item (1): "five of the twelve inferred surfaces". Two errors in one
  clause — the set is thirteen, and the attach it predicted would exercise **five** of them in fact
  exercised **three** (`session.create`, `prompt.submit`, `slash.exec`, per row 6). A prediction the
  measurement has since falsified, sitting in the list U3 re-orders.
- `talaria/ui/app.py:1798`: "five were verified and twelve were not probed at all", with the
  surrounding sentence reasoning about a hypothetical line reading "17 methods verified". Five plus
  thirteen is eighteen; `REQUIRED_METHODS` has eighteen entries. `ec861fa` never touched this file.

Report both. U3 corrects the verdict's; U3 also owns the `app.py` one, which sits fifty lines above
the docstring R5 already sends it to.

**Row 13 (R1's environment half) is open by design, not by neglect.** It depends on the
credential-file-versus-environment-variable precedence decision that `QUEUED.md` records as
deliberately unresolved. Say so, and say that this plan does not resolve it — an under-claim that is
recorded as a decision is not drift.

**Row 19 (F1 and F7 demonstrated live in an isolated session) is the genuinely open question.** F1
("First run") is plainly exercised by the 2026-08-04 sessions. F7 ("Exit" — the gateway survives
Talaria's exit) is not settled by any frame log, because the log ends when Talaria exits and the
observation F7 needs happens after that. Determine what record exists. If none does, the row stays
unmet and the reason changes from "no isolated live session has been run" to something narrower and
true.

**Item 4 (the matrix)** is still missing a person driving the interface on Linux and a real terminal
emulator on either platform; confirm against `§Platform matrix` before repeating it.

**Test expectation.** Every one of the eight items — rows 6, 13 and 19, plus the five at `:443-472` —
gets an explicit present-state sentence. At least one must come back "not moved" or the pass is
agreeing rather than measuring (KTD2).

### U3. Restate the verdict, and correct the four sentences that cite it

Reads U1 and U2's table; writes the verdict. Depends on U1 and U2.

**Scope.** In the verdict document: the `## Verdict` section at `:415-441` and the
`### What would change this verdict` list at `:443-472`, including item (1)'s stale count at `:450`.
Outside it, R5's four locations — `README.md:50-55`, `talaria/cli.py:465`, `talaria/ui/app.py:1839`
and `tests/transport/test_session_startup.py:9-13` — plus the fifth sentence R5 deliberately excludes,
`talaria/ui/app.py:1798`.

**Apply the document's own rule.** "AE7 and R39 say the ready verdict is blocked on any gap." If rows
13 and 19 remain short, the verdict stays NOT READY and its blocking reasons must be rewritten to be
the current ones — the present text blames a client that "has never spoken to that gateway", which is
now false and is the single most misleading sentence in the document.

**The `What would change this verdict` list must be re-ordered by what is actually left.** Items (1)
and (2) become done-and-recorded; the list's remaining items become the live ones. Item (1) also
carries the stale count U2 reports — "five of the twelve inferred surfaces" — and marking an item
done is not a reason to carry its wrong numbers forward: correct it to thirteen, say that the attach
exercised three rather than the five it predicted, and record what the sentence used to claim (R8).

**The four sentences (R5).** Correct each and record what it said. `README.md:50` is the one to get
right first — it is the public front page, and a README asserting "Talaria has never been connected
to a Hermes gateway" beside a verdict that cites live recordings is the same defect this plan is
closing, one level out. `talaria/ui/app.py:1798`'s "five were verified and twelve were not probed at
all" is the second half of U2's count finding and belongs in the same edit as the docstring fifty
lines below it. In `tests/transport/test_session_startup.py` only the R2-is-unmet clause is false —
"No Hermes gateway was attached at any point in this run" is a true statement about that module and
stays.

**Test expectation.** The project check runs green (`ruff`, `mypy`, `pytest`, `bandit`,
`git diff --check`). Only docstrings and comments are touched, so this is a regression gate, not
evidence. The evidence gate is that no sentence in the restated verdict cites a row U1 or U2 did not
produce; check each citation by reading the row it points at.

### U4. Close out DRIFT-04 across the register, the worklist and the index

The bookkeeping that makes the finding findable as resolved. Depends on U3.

**Scope.** `docs/analysis/2026-08-05-conformance-audit-drift-findings.md` (move DRIFT-04 from "Open
findings" to "Resolved findings", update the register table row, the status header and the
resolved/open counts in the prose), `docs/engineering-journal/QUEUED.md` (remove the P1 entry —
verified convention, commit `e1aac95` removes resolved entries rather than annotating them),
`docs/analysis/README.md:18-19` (the daily-driver line's cross-reference to DRIFT-04, and the
register line's "four resolved, DRIFT-04 open" count).

**Also correct the register's own "exactly two things" claim.** DRIFT-04's entry contains the wrong
statement this plan's Problem Frame corrects. Fix it in place and record that it was wrong, per R8 —
a findings register that misstates a finding is the same defect class it exists to track. Name the
cause, because it is instructive: the entry cites `:447-452`, which is items (1) and (2) and nothing
else. It did not misread the list; it read part of the list and reported the part as the whole.

**Dated plan artifacts are historical record and are not rewritten.** The sweep below will find
`DRIFT-04` in `docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md:287` — "DRIFT-02
and DRIFT-04, the other two open audit findings" — which is already wrong about DRIFT-02 (the
register marks it resolved) and becomes wrong about DRIFT-04 here. Leave it. A plan describes what
was true when it was written, and back-dating one destroys the record of what its author knew. The
same applies to this plan's own three artifacts. The sweep's obligation is the living documents: the
register, `QUEUED.md`, and the two `README.md` indexes.

**Journal.** Two entries, both in the commit that ships the work:

- `LEARNINGS.md` for the mechanism — a verdict with no back-link from the work that clears its
  blockers will always go stale, and the generalizable fix is that a document naming its own
  unblocking conditions should also name where those conditions get recorded when they are met.
- `DECISIONS.md` for KTD4's corpus-citation convention: a single recording is cited with
  `live_corpus_identity`'s `talaria-live-v1-…` label, an aggregate over many recordings is cited as
  `talaria-live-corpus-v1-…` with the construction stated, and the two namespaces never mix. This is
  a repo-scoped naming convention introduced by this work, so it belongs in `DECISIONS.md` rather
  than in this plan alone.

**File the deferred mechanism in `QUEUED.md` while you are in that file.** Removing DRIFT-04's entry
and filing this one are the same edit. The item: a gating document that names its own unblocking
conditions has no inbound link from the work that satisfies them, so it goes stale silently and
nothing notices. This plan fixes one instance by hand. Priority P3, and the "worth it when" is the
next time a document like the verdict is written — not now.

KTD1 is **already filed** in `DECISIONS.md` under 2026-08-05 ("When the deliverable is evidence
rather than code, a reproducible measurement stands in for a test gate"), committed with this plan
rather than with the work, because it is a decision about how the work is gated and had to be settled
before the work could start. Do not file it a second time.

**Test expectation.** No *living* document still describes DRIFT-04 as open. Check by searching for
`DRIFT-04` across the tree and reading every hit, not by searching for the word "open"; dated plan
artifacts are hits that are correctly left alone, and the sweep reports them as such rather than
silently skipping them.

## Scope Boundaries

**Out of scope — true non-goals.**

- Resolving the credential-file-versus-environment-variable precedence decision (row 13). It is
  recorded as deliberately open; this plan reports its state and does not decide it.
- Running new live sessions against a Hermes gateway to clear row 19. If the record does not settle
  F7, the row stays unmet — manufacturing evidence to close a row is the failure this plan exists to
  prevent.
- Any change to `talaria/domain/compat.py` or the compatibility baseline. Row 6 is re-read, not
  re-derived.
- The superseded TypeScript tree under `src/`.

**Deferred to follow-up work.**

- The remaining ten evidence-only methods with no runtime evidence (row 6). Closing them needs live
  traffic that does not exist yet, and is a work item rather than a re-grading one.
- A mechanism that re-opens a verdict when its stated blockers clear. This plan fixes one instance by
  hand; the general problem — a gating document with no inbound link from the work that unblocks it —
  gets a `QUEUED.md` entry filed by U4, not a solution invented here.

## Risk Analysis

**The plan's own claims go stale the same way.** Every number in this document was measured on
2026-08-05 against a corpus that grows. U1 must re-measure rather than copy from here; the numbers
above are a baseline to reproduce, not a source to cite.

**Confirmation pressure runs toward READY.** Four rows were graded against a client that had never
attached, and it has now attached; the pull is to read every row as improved. KTD2's control exists
against exactly this, and row 6 is the evidence that a careful pass returns "unchanged" for some of
what it examines.

**The corpus contains real session content.** It is redacted by construction and stays out of the
repository (R29), but a measurement script that prints frame bodies rather than counts would put
session text in a terminal and possibly a log. Every measurement reports aggregates only.
