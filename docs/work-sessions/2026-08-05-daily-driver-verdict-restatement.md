---
title: Daily-driver verdict restatement — execution
date: 2026-08-05
plan: docs/plans/2026-08-05-daily-driver-verdict-restatement-plan.md
branch: docs/restate-daily-driver-verdict
status: pr-ready
---

# Daily-driver verdict restatement — execution

Closes DRIFT-04, the last open finding of the R1–R40 conformance audit. The v0.1
daily-driver verdict graded R2 and R3 `unmet` on reasons that stopped being true
on 2026-08-04, so the project could say neither "ready" nor "not ready" on
evidence. Backend was `cc-workflows-ultracode` — the operator's recorded pick
over a `team-execution` recommendation — run as workflow `wf_76c230f1-120`, four
agents chained `U1 → U2 → U3 → U4`, no parallel fan-out.

**The verdict did not move. Its reasons moved entirely.** That was the expected
outcome and the plan said so before the work started, so the result is a
measurement rather than a confirmation.

## What was built

**U1 — rows 17 and 18 re-graded against the live corpus.** Both moved from
`unmet` to `measured`, each citing the frames that settle it rather than
asserting the grade. R2 rests on gateway *replies* carrying a `session_id`,
matched to the call that produced them by JSON-RPC `id` — 15 of 17 recordings —
because a call going out only proves Talaria tried. R3 is cited in two halves on
purpose: the streaming half (12 of 17 recordings, 18 completed turns) and the
replay-comparison half (`LEARNINGS.md:111`, byte-identical on the 32-frame
recording), because citing only the first would leave the row true-sounding and
under-evidenced.

Both rows record what they used to say, per R8.

**U2 — the falsifiability control, and it held.** Rows 6, 13 and 19 plus the five
"what would change this verdict" items were re-read independently of the
restatement. Four of the eight came back **not moved**. Row 6 is the designated
control and it did not move: exactly three of the thirteen evidence-only methods
have live evidence (`session.create`, `prompt.submit`, `slash.exec`); the other
ten have none. Re-measured by importing `talaria/domain/compat.py` rather than by
re-reading the document's own prose.

Row 19 stays `unmet` but its stated reason was wrong and narrowed. F1's startup
path ran live 15 times, so "no isolated live session has been run" was no longer
a description of what is missing. F7 cannot be settled by any frame log — the log
ends at the exit F7 needs somebody to observe. The corpus does hold adjacent
evidence that stops short of settling it: in ten pairs of consecutive recordings
the later run's `session.most_recent` returned the exact session identifier the
earlier run's `session.create` produced, eight forming one unbroken chain across
roughly two hours twenty minutes. That proves the endpoint answered again after
each exit; it does not prove Talaria did not stop it, because Hermes persists
sessions and an operator restart leaves an identical signature.

**U3 — the verdict restated on the corrected table.** Still **NOT READY**, now on
three narrower gaps (rows 19, 13 and 6), each independently sufficient under AE7
and R39. The retired paragraph is quoted verbatim under "What this section used
to say" rather than deleted. The "what would change this verdict" list is split
into still-open and done-and-recorded rather than having its satisfied items
dropped — a list that quietly drops what it satisfied is how a reader loses the
ability to tell a met item from an item that was never on the list.

Four sentences outside the verdict that the attach had falsified were corrected,
each recording its prior wording: `README.md`'s front-page paragraph,
`run_live`'s docstring in `talaria/cli.py`, `open_session`'s docstring in
`talaria/ui/app.py`, and the R2-is-unmet clause in
`tests/transport/test_session_startup.py`. In the test module only that clause
changed — "No Hermes gateway was attached at any point in this run" is a true
statement about that module and stays.

**U4 — DRIFT-04 closed out.** Register entry moved to Resolved with counts and
status header updated, P1 entry removed from `QUEUED.md`, `docs/analysis/README.md`
cross-reference corrected, and the register's own wrong "exactly two things"
claim corrected in place with the cause named: it cited `:447-452`, which is
items (1) and (2) and nothing else, so it read part of the list and reported the
part as the whole.

## What the plan got wrong, and what that cost

**The `ec861fa` count drift had four sites, not the two the plan predicted.**
That commit pinned `slash.exec` and took the evidence-only method set from twelve
to thirteen — verified by counting `classification="evidence-only"` in
`talaria/domain/compat.py` at `ec861fa~1` (twelve) and at `ec861fa` (thirteen).
It swept the verdict's counts and the compat tests but missed four sentences. Two
were in the plan; two were not:

- `tests/transport/test_compat_baseline.py:242` — a docstring reading "all twelve
  cases above", thirty-seven lines below an assertion that the set is thirteen.
  The file contradicted itself.
- `docs/engineering-journal/DECISIONS.md:241-243` — three stale counts including a
  quoted example summary line reading `12 unverified at runtime`, which is
  provably wrong rather than merely dated: `test_compat_baseline.py:553` asserts
  the live output is `13 unverified at runtime`.

Both sat outside U3's approved file list. Rather than widen a running unit's write
surface, they were ruled out of scope and handled in the driving session;
`DECISIONS.md` took a dated `**Correction**` note recording all three prior
wordings, following the convention already used twice in that file.

**Row 19's reason cell had no owner.** U2 measures rows 6, 13 and 19 but has no
write mandate; U3 writes only the Verdict section and the what-would-change list.
So the one cell U2 proved stale was in nobody's scope, and U3 flagged it rather
than reaching for it. Written in the driving session, matching rows 17 and 18's
convention of recording prior wording in the cell. **The plan should have given
row 19's cell to a writer** — a measuring unit with no write mandate needs its
findings routed somewhere explicit.

**A per-recording number was quietly standing in for a per-turn one.** The plan's
grounding pass recorded delta counts "2 to 944". U1 reported, rather than
adopted, that this is a per-*recording* total including deltas belonging to a
`message.start` no `prompt.submit` preceded — a session-resume replay. Scoped to
the turn row 18 actually claims, the range is 1 to 616. Neither number is wrong;
they count different things. A note in the register now records them as agreeing
rather than leaving two documents looking like they contradict each other. The
"twelve of seventeen" figure is identical under both rules.

## Process finding — the driving session forked a second writer

U3 stalled because a workflow unit's return value goes to the workflow runtime,
not into the next agent's context, and U3 correctly refused to re-derive rows
another unit was built to measure. Recovering U2's report from the workflow
journal and sending it over resumed U3 *as a second agent from the same
transcript*, so two writers were briefly editing the same five files.

It failed safe: one writer's edits landed, the other's `Edit` calls bounced on
exact-match mismatch and it stopped rather than forcing. The tree was verified
coherent by reading the full diff, and both instances' reports agree on every
substantive point. **The lesson is about the recovery, not the workflow:**
answering a blocked workflow agent by message is not a neutral act — it can
duplicate the writer. Where a unit's input is missing, the safer recovery is to
let the unit return, then supply the input to a re-run.

## Checks

Project check green on the full tree: `ruff` clean, `mypy` no issues,
`pytest` 1086 passed / 0 failed / 1 skipped, `bandit` exit 0,
`git diff --check` clean.

This is a regression gate, not evidence — only prose, docstrings and comments
were touched, so green proves nothing about the restatement's correctness. The
evidence gate is that no sentence in the restated verdict cites a row U1 or U2
did not produce, checked by reading each cited row.

Swept clean for the standing constraints: no local path or home directory, no
credential-shaped literal, no session identifier, and no attribution line
anywhere in the diff. The corpus is cited by digest and count throughout (R29),
and the aggregate label `talaria-live-corpus-v1-…` is kept distinct from
`live_corpus_identity`'s single-recording `talaria-live-v1-…` form, with both
constructions written down so either can be re-derived.

## Settlement

Four units, four delivered, zero casualties, `halt_required=false`, dead-letter
queue empty. Evidence receipts were built from the workflow journal and mapped to
units by exact return-key-set match against the settlement descriptor rather than
by position. Lease released.

## Code-review gate

One P1, fixed before the pull request rather than filed: the finding register's
DRIFT-04 entry cited the verdict by line number in five places, and this change
moved the verdict's content by about two hundred lines. `:85` had pointed at the
R2 row and landed on row 6a; `:86` at R3 and landed on row 7; `:428` at the NOT
READY heading and landed on the restatement preamble. "`:443-472` names **five**
items, not two" is present tense, so it did not merely dangle — it asserted
something false about the current file. `talaria/ui/app.py:1839` had the same
fault, the corrected docstring having moved to `:1845`.

Worth stating plainly because of where it was found: the change exists to correct
a document whose claims outlived the facts, and it would have shipped that exact
defect one level out, inside the entry certifying the fix. Every citation in the
DRIFT-04 entry now names a section or an evidence-table row instead. The restated
verdict itself was already clean — it used row numbers throughout — so the habit
existed inside the file being edited and had simply not been generalized across
files. Filed as a `LEARNINGS.md` entry in the same commit.

The gate also verified, from current sources rather than from unit reports: the
verdict did not move to READY; row 6's control held (`compat.py` confirms 5 / 13
/ 18, and the row still reads "Three of the thirteen"); the delta ranges
re-derived independently under `env -i` to exactly 12 of 17, 2,659 frames, 18
turns, 1–616 per turn, 2–716 per recording; and the four code files are
AST-identical after stripping docstrings, which proves zero logic change rather
than asserting it.

## Still open

Nothing. The one item that outlived the pull request was DRIFT-04's closing
reference — the "Fixed and merged: pull request #N, merged as `<sha>`" line the
other resolved entries carry — which could not be written while neither the pull
request number nor the merge commit existed. DRIFT-01 and DRIFT-03 got theirs the
same way, after the fact. This work merged as pull request #27, commit `ae76a39`,
and the reference was added in the follow-up commit that also replaced this
paragraph.
