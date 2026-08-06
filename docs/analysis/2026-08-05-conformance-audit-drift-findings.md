# Talaria conformance audit: drift findings

Status: `living` — all five findings resolved
Authority: `evidence`
Date: 2026-08-05

## What this document is

Between 2026-08-04 and 2026-08-05 the Talaria implementation was audited against the forty
requirements it was built to satisfy, R1 through R40, written down in
[`docs/brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md`](../brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md).
This is the register of what that audit found.

**"Drift" has a narrow meaning here.** It means the implementation departs from a requirement
**and no record explains why**. A departure written down in `DECISIONS.md`, `QUEUED.md`, or an
architecture decision record is a deliberate divergence, not drift, and is excluded — the last
section lists the ones that were considered and excluded, so they are not re-raised.

Severity below is about consequence if the finding is left alone, not about effort to fix.

Grading is complete: all forty requirements were graded twice, across four batches, once by a pass
that ran the program and once by an independent pass that read the source. Five findings were
produced. All five are resolved.

Note what "resolved" means for DRIFT-02: that finding was a *missing record*, not a defect, so writing
the record closes it. The engineering choice it describes is still ahead of the project, and the
`QUEUED.md` entry now exists to surface it at the moment it has to be made.

Note what "resolved" means for DRIFT-04 as well: that finding was a stale document, not a code
defect, so restating the verdict on current evidence — which stays **NOT READY**, on different and
narrower grounds — closes it. See
[`docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`](2026-08-02-v0-1-daily-driver-verdict.md)
for the corrected table.

## The register

| ID | One-line statement | Requirement | Severity | Status |
| --- | --- | --- | --- | --- |
| DRIFT-00 | The compatibility baseline pinned the fallback route for typed commands, not the real one | R34 | Moderate | **Resolved** 2026-08-04 |
| DRIFT-01 | The fourth blocking bridge had no live-socket test proving its answer never reached disk | R26 | Low (evidence quality) | **Resolved** 2026-08-05 |
| DRIFT-02 | Removing the superseded TypeScript tree silently deletes the proof that R28 holds | R28 | Low now, moderate at removal | **Resolved** 2026-08-05 (as a recorded consequence) |
| DRIFT-03 | `talaria record` could only authenticate by putting the credential on the command line | R9, restated as R1 | Moderate | **Resolved** 2026-08-05 |
| DRIFT-04 | The daily-driver verdict was stale and held **NOT READY** on two blockers that had since cleared | R2, R3 | Moderate (decision quality) | **Resolved** 2026-08-05 |

DRIFT-03 was counted under two requirement numbers — R9 in batch 3, R1 in batch 4 — but it is one
defect, not two.

---

## Resolved findings

### DRIFT-04 — the daily-driver verdict was stale, and held NOT READY on two cleared blockers

**Plain statement.** [`docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`](2026-08-02-v0-1-daily-driver-verdict.md)
is the document that gates the v0.1 release. In its `## Verdict` section it read "Talaria v0.1 is
**NOT READY** as a daily driver", and in its "What would change this verdict" list it named two of
the five items (see the correction below): one real attach to a Hermes gateway (R2) and one real
conversational turn (R3). Both happened on 2026-08-04. The document had not been updated, so the
project's own release gate was blocked on grounds that were no longer true.

**A note on how that document is cited here.** This entry originally pointed at the verdict by line
number. Those numbers were correct when it was written and stopped resolving the moment the
restatement rewrote the document — the same failure mode, one level out, in the entry certifying
that the failure mode was fixed. Every citation here now names a section or an evidence-table row
instead, because those survive a rewrite and a line number does not. The restated verdict follows
the same rule internally, and it is worth generalizing: a document cited by line number by anything
it does not control is one edit away from lying about itself.

**Correction, 2026-08-05 — this entry's own "exactly two things" claim was wrong.** The original
text above read "it names exactly two things that would change that verdict", citing the two lines
that hold items (1) and (2). That understates the document it is describing, and it was wrong the
day this entry was written, not made stale by later work. The "What would change this verdict" list
named **five** items, not two: (1) R2, (2) R3, (3) R1's
remaining half — the environment-inherited credential decision, (4) the platform matrix, marked
"partly done" at the time, (5) CI, marked done. The verdict's own closing line read "Until **at
least** (1) and (2) are done and recorded here, this document's verdict does not move" — the words
"at least" make (1) and (2) necessary, not sufficient, and this entry read them as the whole list.
The cause is visible in the original citation: the line range it gave is exactly items (1) and (2)
and nothing else, so
this entry did not misread the five-item list — it read part of the list and reported the part as
the whole. Rows 13 and 19 of the evidence table (row 13 is item (3); row 19 is F1/F7) were gaps this
entry never named, and by the document's own rule either one alone blocks READY. This was caught
before the restatement below began, in
[`docs/plans/2026-08-05-daily-driver-verdict-restatement-plan.md`](../plans/2026-08-05-daily-driver-verdict-restatement-plan.md),
whose Problem Frame states the correction and requires rows 6, 13 and 19 to be re-read alongside R2
and R3, not just the two rows this entry originally pointed at.

**Requirements.** R2 — "At startup Talaria can create a new session, resume a stored human-facing
session, or honour an explicit session target." R3 — "The operator can submit a prompt and watch the
response stream into the transcript." Both were graded `met` by the observed-behaviour pass in
batch 4. The defect was not in the code; it was that the artifact deciding the release still graded
them `unmet`.

**The direction of the error matters.** Unlike DRIFT-03 this was an **under**-claim. It could not
mislead anyone into trusting something unproven; the risk was the opposite one, that the project
could not tell what it had actually achieved and kept paying for a blocker it had cleared.

**Evidence.**

- The verdict's evidence-table row 17 graded R2 **unmet**, reason given: "No Hermes gateway has answered one."
  Across the 17 live recordings then available, a real Hermes gateway answered `session.most_recent`
  in 15 and `session.create` in 15 — and both replies carry a `session_id`. Checking for *replies*
  rather than for *calls* is the point: a call going out is not evidence of an outcome.
- The verdict's evidence-table row 18 graded R3 **unmet**, reason given: "Nothing was submitted to a Hermes
  session." Twelve of those 17 recordings contain a `prompt.submit` followed immediately by
  `message.start`, then `message.delta` frames, then `message.complete`. Delta counts range from 2
  to 944, and in every one of the twelve the first event after the submit is `message.start`, so
  the ordering is the real streaming sequence rather than a coincidence of counts.
  **Note on that range, added 2026-08-05.** "2 to 944" is a per-*recording* total: every
  `message.delta` in the file, including deltas belonging to a `message.start` that no
  `prompt.submit` preceded, which is what a session-resume replay produces. The restatement measured
  the narrower thing the verdict's row 18 actually claims — deltas between a submit's own
  `message.start` and its `message.complete` — and that range is **1 to 616 per turn**, with
  per-recording sums over submit-initiated turns running 2 to 716. Neither number is wrong; they
  count different things, and the two documents are recorded here as agreeing rather than left to
  look like a contradiction. The "twelve of seventeen" figure is identical under both rules.
- [`docs/engineering-journal/LEARNINGS.md`](../engineering-journal/LEARNINGS.md) already recorded R3
  as done on 2026-08-04, verified by a byte-identical replay comparison against corpus
  `talaria-live-v1-32f-5f477fa24fa5`. The journal and the verdict contradicted each other, and
  nothing reconciled them.
- `talaria/ui/app.py` carried the same stale claim inside `open_session`'s docstring: "**This is not
  covered by any live evidence.** It has never run against a Hermes gateway." That sentence was true
  when written and had become false.

**Why the existing records did not cover it.** There was no mechanism that re-opens a verdict when
its blockers clear. The verdict is dated 2026-08-02 and reads as a snapshot; the live attach
happened on 2026-08-04 and was recorded in `LEARNINGS.md`, which is the correct place for it, but
nothing linked the two. A search for any entry tying the live-attach work back to the verdict's R2
and R3 rows returned nothing. The general shape of that gap — a gating document with no inbound link
from the work that clears it — is filed as a deferred item in `QUEUED.md` rather than left here,
where nothing would find it again.

**Severity.** Moderate. Nothing was unsafe and nothing was over-claimed. The cost was decision
quality: the project carried a NOT READY verdict whose two stated reasons were both obsolete, so
neither "we are ready" nor "we are not ready" could be said on evidence.

**Resolution.** Rows 17 and 18 (R2, R3) were re-graded `met` against the live recordings, each
citing the specific frames that settle it. Rows 6, 13 and 19, and the five "What would change this
verdict" items, were independently re-read and their present state recorded — row 6 had **not**
moved (still ten of eighteen required methods with no runtime evidence, the deliberate
falsifiability control), row 13 stays partially unmet by design (the environment-credential
precedence decision recorded as open in `QUEUED.md`), row 19 stays unmet (F7, the gateway surviving
Talaria's exit, still has no isolated recording proving it). The verdict was then restated on the
corrected table: it still reads **NOT READY**, but its reasons are now rows 6, 13 and 19 rather than
the two obsolete ones this finding was about. Four sentences elsewhere that the 2026-08-04 attach had
falsified were corrected in place, each recording what it used to say: `README.md`'s "Read the
verdict before you rely on it" paragraph, `run_live`'s docstring in `talaria/cli.py`,
`open_session`'s docstring in `talaria/ui/app.py`, and the R2-is-unmet clause in the module
docstring of `tests/transport/test_session_startup.py`. A fifth sentence, the method-count claim in
`verify_gateway`'s docstring in `talaria/ui/app.py`, was corrected in the same pass for an unrelated reason — commit `ec861fa`
took the required-method count from seventeen to eighteen, not the live attach — and is not counted
among the four. See
[`docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`](2026-08-02-v0-1-daily-driver-verdict.md)
for the restated verdict and
[`docs/plans/2026-08-05-daily-driver-verdict-restatement-plan.md`](../plans/2026-08-05-daily-driver-verdict-restatement-plan.md)
for the plan that carried it out.

**The reasons were confirmed, not assumed, before the new verdict was trusted.** The failure that
produced this finding was a verdict whose reasons outlived the facts, so a revision that simply
flipped two rows to `met` would have repeated the mistake in the other direction. Each re-graded row
names the recording and the frames that settle it, and rows 6, 13 and 19 were re-read independently
rather than folded into the same pass that restated the verdict (KTD3 in the plan above) — row 6
coming back "not moved" is the control that shows the pass measured rather than agreed.

**Clearing this did not make Talaria ready.** It removed two obsolete reasons; three others
(rows 6, 13 and 19) remained, which is what kept the restated verdict at NOT READY.

**Found by** batch 4 of the audit, by reading recordings rather than prose. Closed 2026-08-05 by the
restatement plan above.

---

### DRIFT-03 — `talaria record` could only authenticate by putting the credential on the command line

**Plain statement.** R9 says attach credentials stay out of command-line arguments, shell history,
and process listings. The `talaria record` subcommand took the gateway URL as a required positional
argument, and the credential rode that URL as a query parameter. It never consulted Talaria's
credential chain, so there was no other way to authenticate it. Anyone able to run `ps` on the
machine saw the live gateway credential for as long as the recording ran, and the operator's shell
history kept a copy afterwards.

**Requirement.** R9 — "Attach credentials also stay out of command-line arguments, shell history,
and process listings." Restated as R1 in batch 4.

**Observed, not inferred.** `talaria record` was run against a dead port with a canary value in the
URL, and the canary was read back out of `ps -ww -Ao pid,command` from a different process.

**Why the existing records did not cover it — they asserted the opposite.** `DECISIONS.md` stated
that attach credentials "never appear in argv". The `QUEUED.md` entry owning R1's process surface
stated "The argv half **holds and is measured**". Both were true of `talaria` and neither was true
of `talaria record`. The test cited as the measurement built its probe from the bare launcher with
no subcommand, so it could not see the argv the documented capture command created.

**Resolution.** Fixed and merged: pull request #23, merged as `65ec12a`. `talaria record` now
resolves its credential through the same provider chain the launcher uses — environment variable,
then URL query, then a `0600` file, then a hidden prompt — and attaches it at dial time. Its
positional argument is now optional and means the endpoint, never the credential. A URL arriving
with a credential in it is **refused** with exit code 2 rather than silently stripped, in all three
shapes one can ride in on: userinfo, a credential-named query parameter, or a fragment. The
process-surface sweep now covers every entry point that can hold a credential, with a guard that
fails on any subcommand nobody has classified.

Execution detail, including two defects introduced during the work and the vacuous-test finding the
review gate uncovered, is in
[`docs/work-sessions/2026-08-05-credential-and-bridge-drift-remediation.md`](../work-sessions/2026-08-05-credential-and-bridge-drift-remediation.md).

**The corrections matter as much as the fix.** The two false sentences were not merely updated to
be true; each records that its earlier wording was an overclaim. A true sentence with a false
history reads exactly like a true sentence, and this finding survived a full audit precisely because
one of those sentences was believed.

---

### DRIFT-01 — the fourth blocking bridge was never swept off a real socket

**Plain statement.** Talaria has four "blocking bridges" — the four request/response paths where
Hermes asks the operator something and Talaria answers. Three had a test that answered over a live
socket and then searched the raw bytes of the recording file to prove the answer never reached disk.
The fourth, `terminal.read.respond`, did not. It was protected by other tests, so this was never an
exposed secret; it was missing evidence on the bridge where the evidence matters most.

**Requirement.** R26 — "Every inbound and outbound raw frame passes through redaction before it can
reach disk rather than being scrubbed afterward."

**Why this bridge in particular.** The other three carry text the operator typed. This one carries
text **Talaria generates itself** by serializing whatever is in the transcript. It is the only
bridge that can put a secret on the wire that the operator never typed and does not know is being
sent — a credential pasted into the transcript by an agent, an environment dump in command output.
That makes its redaction evidence worth the most, and it was the one without live evidence.

**It was also a documentation defect.** `docs/formats/frame-log.md:86` stated that
`tests/transport/test_bridges.py` "answers each bridge over a real socket with a distinctive value
and then searches the raw bytes of the file the recorder wrote." That sentence was true for three
bridges and false for the fourth. The frame-log format document declares itself the authority on the
format under R25, so a false claim inside it is load bearing: a reader checking whether a field was
covered would conclude it was.

**Resolution.** Fixed and merged: pull request #23, merged as `65ec12a`.
`tests/transport/test_bridges.py` gains `test_a_terminal_read_value_is_withheld_from_the_recording`,
a sibling test rather than a fourth row of the existing parametrize list, because the canary has to
reach the bridge through pushed transcript content — `terminal.read` is answered automatically from
the projection and never crosses the operator-answer path the other three use. The test runs the
same raw-bytes sweep and pins the reason `deny-set:terminal.read.respond` specifically, so a test
passing via the general key-name net instead could not masquerade as this one. That distinction
matters because `text` is one of the two fields the key-name net does not catch.

The format document's "each bridge" sentence was made true rather than weakened, which was the
resolution this finding asked for. Verified 2026-08-05: all four bridges now have a live-socket
raw-bytes test in that file.

**Red demonstrated before the test was trusted.** With the deny-set entry temporarily removed, the
new test failed on the raw-bytes assertion. A redaction test that passes because the canary never
reached the frame at all would assert nothing forever.

**Found by the observed-behaviour pass, not by the external auditor.** The independent static pass
had both halves of the contradiction in hand — it cited the passage that overclaims, and four lines
later correctly described the live test as covering three bridges — and graded R26 `met` without
joining them. That is a data point about what a careful static read does not catch, not a criticism
of the auditor.

---

### DRIFT-02 — removing the superseded TypeScript tree silently deletes R28's proof

**Plain statement.** R28 requires that the TypeScript and Python recorders produce equivalent frame
logs. The test that proves this runs the **real** TypeScript recorder as a subprocess. The project
has decided to remove the TypeScript tree under `src/`. When that happens the proof stops being
executable — and nothing said so, so whoever removed the tree would not have been warned.

**Requirement.** R28 — "The TypeScript and Python recorders produce contract-equivalent frame logs
for equivalent receive-only input, which is their shared executable boundary."

**Never violated.** R28 is `met` today and the harness is genuinely enforced in continuous
integration. This was a scheduled consequence with no record attached, which is the same category as
drift: an action the project intends to take that will break a requirement, with nothing written
down to catch it.

**Evidence.** `tests/recorder/ts_bridge/run_ts_recorder.mjs` imports `FrameRecorder` directly from
`src/record/recorder.js`. The dependency is a direct file import, so deleting `src/` breaks it
immediately rather than degrading.

**Why the existing records did not cover it.** They anticipated the wrong casualty. Both `CLAUDE.md`
and the `QUEUED.md` and `DECISIONS.md` entries about `src/` removal reason about the Node `check`
job that runs `npm run check`. R28's harness does not live there — it is a pytest test in the
`python-check` job that spawns `tsx`. So the records correctly predict that Prettier and the `check`
job leave with `src/`, and are silent on the equivalence proof, which leaves with it too.

**Severity.** Low today, moderate at removal time. The failure is loud rather than silent — the
bridge import breaks and the test errors — so this cannot leak a credential or pass a false proof.
The cost is that the decision gets made under time pressure at deletion time, with the likely
outcome being deletion of the failing test, which retires R28's evidence without a decision record.

**Resolution.** The resolution this finding asked for was a record, not code, and that record now
exists: see "R28's equivalence proof leaves the repository with the TypeScript tree" in
[`docs/engineering-journal/QUEUED.md`](../engineering-journal/QUEUED.md). It states the choice that
has to be made when `src/` is removed — vendor a frozen copy of the TypeScript reference recorder as
a test fixture, or accept that R28 becomes historical and record that the equivalence relation was
proven at a named commit and is no longer re-verified. Either is defensible; making the choice
implicitly by deleting a red test is not.

**Found by the external auditor** in batch 2, not by the observed-behaviour pass. Its claim that
*nothing* records `src/` removal was slightly too broad — records do exist — so the finding is
narrowed above to what those records actually miss, which was verified.

---

### DRIFT-00 — the compatibility baseline pinned the fallback route, not the real one

**Plain statement.** Talaria checks a list of Hermes gateway methods against a pinned baseline before
it will call itself ready for daily use. That list omitted `slash.exec`, the method an ordinary typed
command actually travels over. It pinned only `command.dispatch`, the fallback used for the minority
of commands `slash.exec` refuses. A client verifying only the fallback would report ready and then
fail on the first command an operator typed.

**Requirement.** R34 — verify **every** gateway method required by R1 through R31 against a pinned
compatibility baseline before reporting daily-driver ready.

**How it was found.** Not by reading code against requirements, but by comparing the methods Talaria
*declares in source* against the methods the baseline *pins*. Two static grading passes over this
area had not surfaced it.

**Resolution.** Fixed and merged: commit `ec861fa`, pull request #21, merged as `7eae211`. Added the
`slash.exec` baseline entry as `evidence-only` — it executes a command, so R34 forbids probing it —
plus `tests/domain/test_compat_coverage.py`, a guard that parses `talaria/` for gateway-method
constants and fails if any is unpinned. The guard was confirmed to fail on the pre-fix tree naming
`SLASH_EXEC_METHOD`, and it carries a falsifiability control so a scan that silently matches nothing
cannot pass forever.

---

## Departures that are recorded, and so are not drift

Listed so the audit's reasoning is visible and these are not re-raised.

| Requirement | Departure | Where it is recorded |
| --- | --- | --- |
| R6 — "Markdown … presentation is out of scope" | Inline markdown *is* rendered on agent prose | `DECISIONS.md`, which deliberately amends R6; the "content is never dropped" clause still holds and is tested |
| R38 — "memory stays bounded as history grows" | The domain transcript accumulates without eviction; the bound holds only inside the measured window | `QUEUED.md`; the validation gate's explicit non-claim in [`2026-08-03-textual-validation-gate-results.md`](2026-08-03-textual-validation-gate-results.md); ADR-0005's measured slope of 0.23 MB per 1,000 frames |

**One error that is not drift, because it departs from no requirement.**
[`docs/analysis/2026-08-02-hermes-reconciliation-rules.md:43`](2026-08-02-hermes-reconciliation-rules.md)
says "thirty-six rules below" while the table beneath it carries 38 rows, numbered RR-01 through
RR-38. Verified 2026-08-05 by counting both the rows and the distinct rule identifiers. It is listed
here because it is the same species as DRIFT-03 and DRIFT-04 in miniature: a sentence of prose that
was true when written and is no longer true of the artifact beside it. Two rules were added and the
sentence introducing them was not updated. Nothing depends on the number, which is exactly why
nobody noticed. A one-word fix.

---

## How the audit was run, and what it proved about method

Every requirement was graded twice: once by a pass that ran the program and inspected its output,
and once by an independent pass reading the source, run on a different model in a separate terminal
against a brief that named the project's divergence records so recorded departures would not be
reported as defects. The reading pass wrote nothing to the repository, verified after each batch
with `git status --porcelain`.

| Batch | Requirements | met | diverged-by-decision | unmet / drift |
| --- | --- | --- | --- | --- |
| 1 | R6, R10, R11, R12, R13, R16, R17, R20, R21, R38 | 8 | 2 | 0 |
| 2 | R25–R33, R40 | 10 | 0 | 0 unmet; 2 evidence findings (DRIFT-01, DRIFT-02) |
| 3 | R5, R7, R8, R9, R14, R15, R18, R19, R22, R23 | 8 | 1 (R7) | **1 unmet — R9 (DRIFT-03)** |
| 4 | R1, R2, R3, R4, R24, R34, R35, R36, R37, R39 | 9 | 0 | **1 unmet — R1 (DRIFT-03 again)**; 1 evidence finding (DRIFT-04) |

**Neither lens found everything, in any batch.**

| Batch | The reading pass found | The running pass found |
| --- | --- | --- |
| 2 | DRIFT-02 — R28's proof is hosted by a tree slated for deletion | DRIFT-01 — the fourth bridge is never swept over a live socket |
| 3 | no defect; strong corroboration | **DRIFT-03** |
| 4 | no defect; mis-graded R1 as a recorded divergence | **DRIFT-04**; independently confirmed **DRIFT-03** |

**The strongest result the audit produced is the R1 disagreement in batch 4, and it argues for a
practice rather than for a finding.** The reading pass graded R1 `diverged-by-decision`, citing the
`QUEUED.md` entry that says R1's environment clause is unmet and no change to Talaria can meet it.
That entry is real and it is a genuine recorded divergence — but it records the **environment
variable** channel. DRIFT-03 is about a different channel entirely, the credential on the **command
line**. The cited entry did not merely omit the argv channel; it asserted the opposite, in the
sentence "The argv half holds and is measured."

DRIFT-03 had already predicted this in writing, saying an operator reading that entry "would
conclude argv is settled and only the environment is open." An independent auditor then read it and
concluded precisely that. The prediction was confirmed by experiment rather than argued.

The same clause was graded three times by the reading lens and came back a different way each time —
`met` as R9 in batch 3, `diverged-by-decision` as R1 in batch 4 — and never `unmet`. Reading alone
did not once reach the correct answer about a credential that a single `ps` invocation displays.

The generalizable point: **redundancy between readers does not catch a defect whose cover story is
written down.** A second reader reads the same false sentence and agrees with the first. Only
running the program settles it. That is why the second lens has to observe rather than read, and it
is the reason this project's audits are structured the way they are.
