# Doc review — daily-driver verdict restatement plan

**Target.** `docs/plans/2026-08-05-daily-driver-verdict-restatement-plan.md`

**Reviewed revision.** `4bc8a77` on `docs/daily-driver-verdict-restatement-plan`, one commit ahead of
`origin/main`, working tree clean at review time. Open as pull request #26.

**Blocked.** No, as of 2026-08-05. The review found two `P1`s; all seven findings were fixed after the
review ran, at the operator's instruction to fix everything rather than only the blockers.

**Related artifacts.** Spec `docs/plans/2026-08-05-daily-driver-verdict-restatement-spec.json`;
workflow `docs/plans/2026-08-05-daily-driver-verdict-restatement.workflow.js`; saga
`task-talaria-drift-remediation`. Origin finding: DRIFT-04 in
`docs/analysis/2026-08-05-conformance-audit-drift-findings.md`.

## Readiness summary

The plan can drive implementation. Its grounding is unusually solid: every line citation in it was
checked against the file it points at and every one was correct, and the corpus numbers reproduced
exactly when re-measured during this review — 17 recordings, 2,659 frames, a gateway reply carrying a
`session_id` in 15 of 17, `submit`→`start`→`delta`→`complete` in 12 of 17, deltas from 2 to 944.

Both `P1`s are the same species as the defect the plan exists to close, which is what made them worth
finding. One is a citation nobody could re-derive; the other is three more sentences that the
2026-08-04 attach falsified and that the plan's scope did not reach — including the claim on the
public repository's front page that Talaria has never been connected to a Hermes gateway.

No invented decisions, no unverified assumption that would change what gets built, and no scope the
plan claims that its units do not cover — after the fixes below. The plan's own falsifiability control
(row 6 has not moved) was independently reproduced and holds.

## Applied fixes

| # | Fix | Evidence |
| --- | --- | --- |
| 1 | KTD4 rewritten to give the whole-corpus digest its own label namespace, `talaria-live-corpus-v1-…`, with the hash construction stated: `sha256` over each recording's bytes concatenated in filename-sorted order. U1's baseline label updated to match | `talaria/replay/gate.py:317-328` — `live_corpus_identity` hashes **one** file and owns the `talaria-live-v1-…` label; both existing citations in the repository are single recordings |
| 2 | R5 replaced with a four-row table naming every attach-falsified sentence, not just the docstring. U3's scope, label, prompt and declared files extended to reach them | `README.md:50`, `talaria/cli.py:465`, `talaria/ui/app.py:1839`, `tests/transport/test_session_startup.py:9-13`, all read on `main` |
| 3 | U2 now reports the two surviving "twelve" counts and the five-versus-three prediction; U3 now corrects them | `git show ec861fa~1:talaria/domain/compat.py` has twelve `evidence-only` entries, `ec861fa` has thirteen; verdict `:450` and `talaria/ui/app.py:1798` still say twelve |
| 4 | The deferred stale-gating-document item is now filed by U4 in `QUEUED.md` instead of being asserted as worth filing by nobody | Plan's Scope Boundaries said "is worth a `QUEUED.md` entry"; no unit wrote one |
| 5 | U4 given the rule that dated plan artifacts are historical record and are not back-dated, naming the hit its sweep will find | `docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md:287` calls DRIFT-02 and DRIFT-04 "the other two open audit findings"; the register already marks DRIFT-02 resolved |
| 6 | Two wrong counts in the plan itself: Summary said "four other rows" (rows 6, 13, 19 are three); U2's test expectation said "seven items" (three rows plus five items is eight) | Verdict `:417-420` names rows 6, 13 and 19 beyond 17 and 18 |
| 7 | New R9 maps the journal work; U4 now files the `DECISIONS.md` entry its spec already declared the file for | Repo rule: a convention decision gets a `DECISIONS.md` entry in the commit that ships it. KTD4's namespace split is that convention |

Fix 1 and fix 2 are the ones that mattered. Everything else is bookkeeping.

Spec revalidated with `--require-receipts` (exit 0), spend unchanged at 102 ordinal, workflow
re-emitted from the edited spec — 402 lines, three barriers, chain U1 → U2 → U3 → U4 intact.

## Remaining findings

All seven were resolved on 2026-08-05, after the review.

| Key | Priority | Finding | Status |
| --- | --- | --- | --- |
| D1 | P1 | The whole-corpus digest wears the repository's single-recording label and its construction is written down nowhere | **Resolved** — KTD4 splits the namespaces and states the construction; R2 now says a digest without a stated rule is not evidence |
| D2 | P1 | Three more sentences falsified by the 2026-08-04 attach sit outside the plan's scope, one of them the public `README.md` | **Resolved** — R5 is now a four-location table; U3's scope, spec prompt and file list extended |
| D3 | P2 | `ec861fa`'s twelve-to-thirteen sweep missed two sentences, one inside the list U3 re-orders | **Resolved** — U2 reports both, U3 corrects both; spec gained `stale_twelve_count_instances_reported` and `twelve_to_thirteen_count_corrections` |
| D4 | P2 | A deferred item the plan calls worth filing has no unit that files it | **Resolved** — assigned to U4; spec gained `queued_deferred_entry_filed` |
| D5 | P2 | U4's "read every hit" sweep lands on a file outside its declared list, with no rule for what to do | **Resolved** — U4 told to leave dated plan artifacts alone and to report them rather than skip them silently |
| D6 | P3 | Two wrong counts in a plan about wrong counts | **Resolved** — "four other rows" → three; "seven items" → eight |
| D7 | P3 | U4 declared `DECISIONS.md` in its file list but was told not to write it | **Resolved** — the declaration is now true; U4 files KTD4's convention there |

### D1 (P1) — the corpus digest cannot be re-derived, and borrows a label that means something else

`live_corpus_identity` (`talaria/replay/gate.py:317-328`) is the repository's committed way to cite a
recorded corpus. It takes **one** path, hashes that file's bytes, and labels the result
`talaria-live-v1-<frames>f-<sha256[:12]>`. Both citations already in the repository are that shape —
`talaria-live-v1-32f-5f477fa24fa5` in the findings register and `talaria-live-v1-5773f-88a3604c34b7`
in the Textual gate results.

The plan's `talaria-live-v1-2659f-bd69e537f1d9` is an aggregate over seventeen files wearing the same
label, computed by a grounding script that is deliberately not committed (KTD1), by a rule stated
nowhere. `live_corpus_identity` cannot produce it.

The failure is specific, not theoretical. The plan's Risk Analysis tells U1 to re-measure rather than
copy. U1 reaches for the repository's own helper, gets a different hash, and then either invents an
undocumented rule of its own or pastes the plan's number — which is a number nobody can re-derive.
KTD1 says that is exactly what DRIFT-04 *is*: "a number with no method behind it". KTD4 even named
the risk — "'the corpus' naming two different things is how a citation stops being checkable" — and
the plan then did it.

**Resolution.** KTD4 now defines two namespaces and both constructions. The aggregate is
`talaria-live-corpus-v1-<total frames>f-<sha256[:12]>` over each recording's raw bytes concatenated in
filename-sorted order. The hash value is unchanged, so the re-measurement in this review still
reproduces it; only the label moved.

### D2 (P1) — three more attach-falsified sentences, including the public front page

R5 named one stale sentence, `open_session`'s docstring. A sweep for every reference to the verdict
document found three more that the 2026-08-04 attach falsified:

- **`README.md:50`** — "Talaria has never been connected to a Hermes gateway: every transport test in
  this repository dials a loopback stub." This is the first thing any reader of a public repository
  sees, and the rest of the paragraph reasons from it. It is a stronger claim, more prominently
  placed, than the docstring the plan did cover.
- **`talaria/cli.py:465`** — "This path has never been run against a real Hermes gateway", plus the
  R2-and-R3-are-unmet conclusion drawn from it. The first half is already false; the second becomes
  false the moment U1 lands.
- **`tests/transport/test_session_startup.py:9-13`** — only the R2-is-unmet clause. The sentence "No
  Hermes gateway was attached at any point in this run" scopes itself to that test module and stays
  true; the distinction is worth keeping because over-correcting it would make the module lie in the
  other direction.

Leaving these would ship a restated verdict citing live recordings while the repository's own front
page says the client has never connected — the same defect one level out, and the reason R8 exists.

**Resolution.** R5 is now a table of four locations with the reason each is false. U3's scope, label,
spec prompt and declared files were extended; the spec's `docstring_correction_…` return key became
`stale_sentences_corrected_with_prior_wording`.

### D3 (P2) — a count sweep that missed two places, one of them inside the list U3 rewrites

Commit `ec861fa` pinned `slash.exec` and took the evidence-only method set from twelve to thirteen —
verified by counting `classification="evidence-only"` in `talaria/domain/compat.py` at `ec861fa~1`
(twelve) and at `ec861fa` (thirteen). It swept the verdict document's counts but left two sentences:

- The verdict at `:450`, inside item (1) of the "What would change this verdict" list: "five of the
  twelve inferred surfaces". Two errors in one clause. The set is thirteen, and the attach that item
  predicted would exercise five of them in fact exercised three — `session.create`, `prompt.submit`,
  `slash.exec`, which is what row 6 records. This sits inside the list U3 is told to re-order, so an
  agent marking the item done would carry the falsified prediction forward untouched.
- `talaria/ui/app.py:1798` — "five were verified and twelve were not probed at all", reasoning about a
  hypothetical line reading "17 methods verified". `REQUIRED_METHODS` has eighteen entries. `ec861fa`
  never touched this file.

**Resolution.** U2 reports both and the five-versus-three discrepancy; U3 corrects both. R5 explicitly
excludes `app.py:1798` from its four, because the attach is not what falsified it — `ec861fa` is — and
a fix that cites the wrong cause is its own small defect.

### D4 (P2) — deferred work with no owner

The plan's Scope Boundaries said the general problem — a gating document with no inbound link from the
work that unblocks it — "is worth a `QUEUED.md` entry, not a solution invented here." U4 edits
`QUEUED.md`, but only to remove DRIFT-04's entry. Nobody filed the new one, so the deferred item would
have evaporated at merge, which is the failure mode `QUEUED.md` exists to prevent.

**Resolution.** Assigned to U4, in the same edit that removes the DRIFT-04 entry. R9 records it.

### D5 (P2) — a sweep that lands outside its own file list

U4's test expectation is to search for `DRIFT-04` across the tree and read every hit. One hit,
`docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md:287`, calls DRIFT-02 and
DRIFT-04 "the other two open audit findings" — already wrong about DRIFT-02, which the register marks
resolved, and about to be wrong about DRIFT-04. That file is not in U4's declared list, so U4 would
either edit an undeclared file or silently drop a hit its own gate told it to read.

**Resolution.** U4 is told that dated plan artifacts are historical record and are not back-dated,
that this plan's own three artifacts fall under the same rule, and that the sweep reports such hits
rather than skipping them silently. The obligation is the living documents: the register, `QUEUED.md`,
and the two index files.

### D6 (P3) — two wrong counts, in a plan about wrong counts

The Summary said the plan "re-reads the four other rows the verdict's own closing rule depends on";
the verdict at `:417-420` names rows 6, 13 and 19 beyond 17 and 18, which is three, and R3 and U2 both
say three. U2's test expectation said "every one of the seven items" where its own scope is three rows
plus five items, which is eight.

**Resolution.** Both corrected, with the constituents spelled out so the count can be checked against
the list rather than trusted.

### D7 (P3) — a declared file the unit was told not to write

The spec listed `docs/engineering-journal/DECISIONS.md` among U4's files while the plan said KTD1 was
already filed there and "Do not file it a second time." Harmless under a strict sequential chain,
where the file list only serves the concurrent-writer collision check, but a false declaration.

**Resolution.** Resolved by making the declaration true rather than by deleting it. KTD4's namespace
split is a repo-scoped naming convention introduced by this work, and the repository's standing rule
is that a convention decision gets a `DECISIONS.md` entry in the commit that ships it. U4 files it.

## Residual risk

**The measurement is reproducible in principle and unattested in practice.** KTD1 keeps the grading
script out of the repository for a good reason — it cannot run in CI — but that means U1's numbers are
checked by a method described in prose rather than by an artifact anyone can execute. This review
re-ran the grounding script and got identical numbers, which raises confidence without changing the
structural fact: nothing in the repository will catch it if these numbers drift again.

**Row 19 is genuinely undetermined.** F7 asks whether the gateway survives Talaria's exit, and no frame
log can answer it, because the log ends at the exit being observed. U2 is told to find what record
exists and to leave the row unmet if none does. That is the right instruction, and it means the plan
cannot promise what the restated verdict will say about row 19 — which is the intended shape, not a
gap.
